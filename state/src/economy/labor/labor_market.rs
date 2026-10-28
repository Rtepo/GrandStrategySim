#![allow(missing_docs)]

use crate::economy::legal_status::LegalStatus;
use crate::entities::Company;
use crate::politics::citizenship::{CitizenshipLaw, DiscriminationConfig};
use crate::registries::enums::Sector;
use crate::society::geography::{DemographicClass, Region};
use crate::state::Calendar;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Default remittance rate for TemporaryWorkers (10% of net income).
fn default_remittance_rate() -> f64 {
    0.10
}

/// Default PIT personal allowance expressed as a share of the national
/// average wage. Every FTE's first `share × average_wage` of turn earnings
/// is exempt from withholding. A statutory zero-rated band is standard in
/// real PIT systems (~25–55% of average earnings across OECD members —
/// higher in redistributive regimes; e.g. Romania's personal deduction
/// reaches ~60% at the minimum wage); countries may override via
/// `tax_rates.income_tax.extra["personal_allowance_wage_share"]`.
pub const DEFAULT_PIT_PERSONAL_ALLOWANCE_WAGE_SHARE: f64 = 0.55;

/// Data-driven configuration for labor market mechanics
/// Loaded via JSON to avoid hardcoded simulation logic
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LaborConfig {
    /// Suitability matrix: class -> (sector -> multiplier)
    /// E.6.3: Typed key (DemographicClass) replaces raw string class_id.
    /// Multipliers represent class suitability for specific sectors
    /// Missing keys default to 1.0 (neutral suitability)
    pub suitability_matrix: HashMap<DemographicClass, HashMap<Sector, f64>>,
}

/// Labor market bid from a company
pub struct LaborBid {
    pub company_id: String,
    pub target_fte_demand: f64,
    pub offered_wage_per_fte: f64,
    pub sector: Sector, // Required for class-sector suitability matching
}

/// Per-class labor ledger for tracking FTE and wages during market clearing
pub struct ClassLaborLedger {
    /// FTE this class offered to the market
    pub available_fte: f64,
    /// FTE allocated to companies during clearing
    pub allocated_fte: f64,
    /// Wages earned from allocated FTE (bid-specific, NOT pooled)
    pub earned_wages: f64,
}

/// Phase 6.5: Company×class labor allocation matrix for payment-in-kind
///
/// Tracks which company received FTE from which specific demographic class,
/// enabling agricultural companies to deduct in-kind payments from the correct
/// worker classes.
/// E.6.3: Uses `DemographicClass` typed key instead of `(DemographyType, String)`.
#[derive(Debug, Default)]
pub struct LaborAllocationMatrix {
    /// (company_id, demographic_class) -> FTE allocated this turn
    pub fte: BTreeMap<(String, DemographicClass), f64>,
    /// Same key -> cash wages credited (needed to clamp in-kind wage offset)
    pub wages: BTreeMap<(String, DemographicClass), f64>,
    /// Fix 1.22: Total PIT withheld at source during wage payment.
    /// The caller must credit this amount to `country.budget.liquid_reserves`.
    pub pit_withheld: f64,
    /// Phase 18A: Total remittances withheld at source from TemporaryWorker wages.
    /// The caller must route this amount via `TransferRecipient::ForeignEntity`.
    pub remittances_withheld: f64,
    /// Phase 18B: Total community service garnishments withheld at source.
    /// The caller must credit this amount to `country.budget.liquid_reserves`.
    pub garnishments_withheld: f64,
    /// Phase 23C: Total gross wages earned by commuter FTE (workers from
    /// adjacent regions). The caller must remit these wages back to the
    /// home regions' class savings. PIT is already included in
    /// `pit_withheld` (commuters pay PIT in the host region).
    pub commuter_wages: f64,
    /// Phase 23C: FTE secured by commuters (for caller-side remittance
    /// proportional allocation across home regions).
    pub commuter_fte: f64,
    /// Total FTE demanded by bids this clearing (pre-allocation). The
    /// caller uses `sum_bid_fte − allocated` as the region's revealed
    /// unfilled labor demand for next turn's commuter routing — structural
    /// `target_fte_demand` overstates demand because M&A consolidates
    /// targets while the work never scales.
    pub sum_bid_fte: f64,
}

impl LaborAllocationMatrix {
    /// Merge another matrix into this one, summing all scalar fields and
    /// combining the BTreeMap entries. Used when clearing labor markets
    /// across multiple regions into a single aggregated result.
    pub fn merge(&mut self, other: LaborAllocationMatrix) {
        self.pit_withheld += other.pit_withheld;
        self.remittances_withheld += other.remittances_withheld;
        self.garnishments_withheld += other.garnishments_withheld;
        self.commuter_wages += other.commuter_wages;
        self.commuter_fte += other.commuter_fte;
        self.sum_bid_fte += other.sum_bid_fte;
        for (k, v) in other.fte {
            *self.fte.entry(k).or_insert(0.0) += v;
        }
        for (k, v) in other.wages {
            *self.wages.entry(k).or_insert(0.0) += v;
        }
    }
}

/// Aggregate labor pool for a region
pub struct RegionalLaborPool {
    /// Total available FTE across all classes
    pub total_available_fte: f64,

    /// Track per-class labor and wage allocations
    /// E.6.3: Typed `DemographicClass` key replaces `(DemographyType, String)`.
    /// The enum variants inherently prevent rural/urban key collisions.
    pub class_ledgers: BTreeMap<DemographicClass, ClassLaborLedger>,
}

/// Get suitability multiplier for a class-sector combination (data-driven)
///
/// # Arguments
/// * `class` - Demographic class (typed)
/// * `sector` - GDP sector
/// * `config` - LaborConfig containing suitability matrix
///
/// # Returns
/// Multiplier (0.0 to 2.0+) representing class suitability for sector
///
/// # Rules
/// * Looks up multiplier from config.suitability_matrix
/// * Missing keys default to 1.0 (neutral suitability)
/// * Multiplier affects labor share during bid distribution, NOT actual FTE count
fn get_suitability_multiplier(class: DemographicClass, sector: &Sector, config: &LaborConfig) -> f64 {
    config
        .suitability_matrix
        .get(&class)
        .and_then(|sector_map| sector_map.get(sector))
        .copied()
        .unwrap_or(1.0) // Default to neutral if not configured
}

/// Resolve regional labor market through competitive bidding
///
/// # Arguments
/// * `region` - Mutable reference to the region
/// * `companies` - Slice of all companies (filtered by region_id)
/// * `minimum_wage` - Optional statutory minimum wage per FTE (None = laissez-faire)
/// * `calendar` - Current calendar state (for seasonal demand adjustments)
/// * `config` - LaborConfig containing data-driven suitability matrix
///
/// # Returns
/// * `LaborAllocationMatrix` tracking company×class FTE and wage allocations
///
/// # Rules
/// * Companies are filtered by region_id
/// * Bids are sorted by offered_wage_per_fte (descending)
/// * Highest-paying companies consume FTE first
/// * Liquidity clamping applied before bidding
/// * Minimum wage rejection applied before bidding (if Some)
/// * Wages tracked per-class, NOT pooled (maintains inequality)
/// * Suitability multipliers loaded from config (data-driven, not hardcoded)
/// * Returns LaborAllocationMatrix for payment-in-kind (Phase 6.5)
pub fn resolve_regional_labor_market(
    region: &mut Region,
    companies: &mut [Company],
    buildings: &[crate::entities::Building],
    minimum_wage: Option<f64>,
    _calendar: &Calendar,
    config: &LaborConfig,
    pit_rate: f64,
    pit_allowance_per_fte: f64,
    garnishment_rates: &std::collections::BTreeMap<(String, DemographicClass), f64>,
    commuter_inflow_fte: f64,
    civil_rights_law: &str,
    discrimination_config: &DiscriminationConfig,
) -> LaborAllocationMatrix {
    // Phase 6.5: Initialize labor allocation matrix for payment-in-kind tracking
    let mut allocation_matrix = LaborAllocationMatrix {
        fte: BTreeMap::new(),
        wages: BTreeMap::new(),
        pit_withheld: 0.0,
        remittances_withheld: 0.0,
        garnishments_withheld: 0.0,
        commuter_wages: 0.0,
        commuter_fte: 0.0,
        sum_bid_fte: 0.0,
    };

    // Phase 92: Reset transient wage tracking fields for all companies in this
    // region. These fields are set during payroll processing and read by
    // process_company to compute the true wage expense (including arrears).
    for company in companies.iter_mut() {
        if company.region_id == region.id {
            company.wages_paid_this_turn = 0.0;
            company.arrears_accrued_this_turn = 0.0;
        }
    }

    // Phase 1: Extract and Validate Bids

    // 1. Filter companies by region_id
    let region_companies: Vec<&mut Company> = companies
        .iter_mut()
        .filter(|c| c.region_id == region.id)
        .collect();

    // 2. Extract bids with liquidity clamping and minimum wage check
    let mut bids: Vec<LaborBid> = Vec::new();

    // Macro-Remediation: per-company realized utilization — the mean
    // `last_fulfillment_ratio` across owned buildings (last turn's
    // production outcome). A plant that could not run cannot honestly
    // recruit at nameplate capacity: its bid vacuums released workers out
    // of the pool and straight back onto the furlough bench while deficit
    // companies with working supply lines go unstaffed. Scaling labor
    // demand by utilization breaks that churn cycle; a small restart
    // floor keeps a skeleton bid so a restocking plant ramps back up as
    // inputs land (fulfillment rises → next turn's bid follows).
    let mut utilization_by_owner: std::collections::BTreeMap<String, (f64, usize)> =
        std::collections::BTreeMap::new();
    for b in buildings {
        let e = utilization_by_owner
            .entry(b.owner_id.clone())
            .or_insert((0.0, 0));
        e.0 += b.last_fulfillment_ratio;
        e.1 += 1;
    }
    #[cfg(feature = "diagnostic")]
    let mut _diag_sum_target = 0.0_f64;
    let mut _diag_sum_affordable = 0.0_f64;
    let mut _diag_sum_cap = 0.0_f64;
    #[cfg(feature = "diagnostic")]
    let mut _diag_company_count = 0usize;
    for company in region_companies {
        #[cfg(feature = "diagnostic")]
        {
            _diag_sum_target += company.target_fte_demand as f64;
            _diag_company_count += 1;
        }
        // Reset fulfilled_fte
        company.fulfilled_fte = 0;

        // Liquidity clamping
        // Phase 33: Fall back to available_cash if no brokerage_account.
        // This prevents companies loaded from old saves (without brokerage_account)
        // from being clamped to 0 FTE and mass-bankrupting.
        // 12-turn mandate: use the same solvency basis as the distress/furlough
        // and reinstatement checks (`operational_cash + company_capital`), not
        // spendable cash alone. Under a pure-cash gate a company that burned
        // its genesis payroll float during the input-starvation window could
        // never bid above the 90% retention floor again — a one-way ratchet
        // that geometrically decays employment ~10%/turn. Unpaid wages already
        // accrue as arrears, and `company_capital` erodes with them, so the
        // bound self-limits as genuine insolvency approaches.
        // v4.17 FS-7: construction contractors also staff against contract
        // receivables — unreleased escrowed tranches already debited from
        // the investor, contractually theirs on completion (real-world
        // progress-payment/mobilization financing). Published each turn by
        // `update_construction_labor_demand` under
        // `extra["contract_receivables"]`; counted at the same 50% haircut
        // the material-bid capacity path applies to pending tranches, so
        // milestone slippage still self-limits via wage arrears.
        let contract_receivables = company
            .extra
            .get("contract_receivables")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .max(0.0);
        let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
            (company.operational_cash()
                + company.company_capital.max(0.0)
                + contract_receivables * 0.5)
                / company.offered_wage_per_fte
        } else {
            0.0
        };
        #[cfg(feature = "diagnostic")]
        {
            _diag_sum_affordable += max_affordable_fte;
        }

        let mut clamped_demand = (company.target_fte_demand as f64).min(max_affordable_fte);

        // Phase 40: FTE retention floor — companies can retain up to 90% of
        // prev_fulfilled_fte even with zero cash, by accruing wage arrears.
        // This prevents instant 100% layoffs from short-term cash shortages.
        // The law is equal for ALL companies, including banks — no exemptions.
        // Phase 47: For furloughed companies, the retention floor applies to
        // the standby crew (target_fte_demand), not the pre-furlough workforce.
        // Note: fulfilled_fte is already at standby level (set by apply_seasonal_furlough),
        // so the wage payment = fulfilled_fte * offered_wage is correct.
        const FTE_RETENTION_FLOOR: f64 = 0.90; // 10% max layoff per turn
        // Macro-Remediation: a plant that could not run cannot honestly
        // recruit — or retain — a crew it cannot feed. `sustainable` is the
        // labor demand last turn's input fulfillment could actually use;
        // the restart floor keeps a skeleton bid so restocking ramps back.
        // The retention floor decays toward `sustainable` at the usual
        // 10%/turn rather than holding the whole roster on arrears —
        // without this, a starved company's bid recaptures its own released
        // workers straight back onto the furlough bench (the churn cycle
        // that parked ~4.6M workers while deficit regions went unstaffed).
        const RESTART_UTILIZATION_FLOOR: f64 = 0.15;
        // Only a CHRONICALLY starved company sheds toward its sustainable
        // crew — a transient input hiccup must not vaporize employment.
        // The clock is the same `furlough_turns_accumulated` the hard
        // release uses, so a company holding a bench it cannot feed bleeds
        // workers to real demand instead of recapturing them each turn.
        const CHRONIC_STARVATION_TURNS: u32 = 8;
        let is_seasonal_standby = company
            .seasonal_profile
            .as_ref()
            .map(|p| p.current_state == crate::entities::SeasonalState::Furloughed)
            .unwrap_or(false);
        let chronically_starved = !is_seasonal_standby
            && company.furlough_turns_accumulated >= CHRONIC_STARVATION_TURNS;
        let (retention_floor, demand_ceiling) = if is_seasonal_standby {
            (
                company.target_fte_demand as f64 * FTE_RETENTION_FLOOR,
                f64::MAX,
            )
        // v4.17 FS-7: construction firms are exempt from the
        // input-utilization collapse — their work happens on INVESTOR-owned
        // sites, so `utilization_by_owner` sees only their own yards'
        // production inputs and systematically under-measures them (the
        // observed exactly-25% fills). Their demand is already paced by
        // contract value and pay-cycle capacity in
        // `update_construction_labor_demand`; the 90% retention floor then
        // governs decay like any cash-short-but-working firm.
        } else if chronically_starved
            && company.sector != crate::registries::enums::Sector::Construction
        {
            let utilization = utilization_by_owner
                .get(&company.id)
                .map(|(sum, n)| sum / *n as f64)
                .unwrap_or(1.0);
            let sustainable = company.target_fte_demand as f64
                * utilization.max(RESTART_UTILIZATION_FLOOR);
            // The bid must collapse to the crew the input line can feed.
            // `sustainable.max(prev)` let a bloated roster bid its full
            // previous headcount, so released workers were recaptured out
            // of the pool each turn, paid via arrears, and re-furloughed —
            // the churn bench that parked millions while deficit regions
            // posted unfilled demand. A chronically starved firm sheds to
            // `sustainable` outright; recovery still works because the
            // ceiling rises automatically when input fulfillment does.
            (sustainable, sustainable)
        } else {
            (
                company.prev_fulfilled_fte as f64 * FTE_RETENTION_FLOOR,
                f64::MAX,
            )
        };
        if max_affordable_fte < retention_floor && retention_floor > 0.0 {
            // Company can't afford full payroll — retain at retention_floor,
            // the unpaid wages will accrue as arrears after clearing.
            clamped_demand = clamped_demand.max(retention_floor);
        }
        clamped_demand = clamped_demand.min(demand_ceiling);

        // Phase 37: Hiring friction — cap growth to 15% per turn.
        // Small companies (<10 FTE) are exempt so they can scale up from zero.
        // This prevents the ±100% employment swings that destabilize GDP.
        // 12-turn mandate: a company restoring seats it already owns (bid <=
        // target) may close 40% of the gap to target per turn. The pure
        // prev-relative cap turns any collapse into a ~30-turn recovery —
        // after an input-starvation wave crushed prev_fulfilled, companies
        // re-bid ~0.75x of target for the whole window and aggregate
        // unemployment never clears 20%. Gap-closing still bounds the swing
        // (max +40% of missing crew, decelerating near target) while letting
        // a distressed economy re-staff its existing capital stock.
        const MAX_HIRING_GROWTH_RATE: f64 = 0.15;
        const RECOVERY_GAP_CLOSE_RATE: f64 = 0.40;
        const SMALL_COMPANY_FTE_THRESHOLD: f64 = 10.0;
        if company.prev_fulfilled_fte as f64 >= SMALL_COMPANY_FTE_THRESHOLD {
            let prev = company.prev_fulfilled_fte as f64;
            let growth_cap = prev * (1.0 + MAX_HIRING_GROWTH_RATE);
            let gap_close =
                prev + (company.target_fte_demand as f64 - prev).max(0.0) * RECOVERY_GAP_CLOSE_RATE;
            clamped_demand = clamped_demand.min(growth_cap.max(gap_close));
        }
        #[cfg(feature = "diagnostic")]
        {
            _diag_sum_cap += clamped_demand;
        }

        // Minimum wage check (only if Some)
        let passes_min_wage = match minimum_wage {
            Some(min_wage) => company.offered_wage_per_fte >= min_wage,
            None => true, // Laissez-faire: no minimum wage enforcement
        };

        if passes_min_wage && clamped_demand > 0.0 {
            bids.push(LaborBid {
                company_id: company.id.clone(),
                target_fte_demand: clamped_demand,
                offered_wage_per_fte: company.offered_wage_per_fte,
                sector: company.sector, // Include sector for suitability matching
            });
        }
    }

    #[cfg(feature = "diagnostic")]
    {
        let sum_target: f64 = _diag_sum_target;
        let sum_bid: f64 = bids.iter().map(|b| b.target_fte_demand).sum();
        let zero_bid = _diag_company_count - bids.len();
        let mut prefix_hist: std::collections::BTreeMap<String, (usize, f64)> =
            std::collections::BTreeMap::new();
        for b in &bids {
            let p = b
                .company_id
                .split('-')
                .next()
                .unwrap_or("")
                .chars()
                .take(12)
                .collect::<String>();
            let e = prefix_hist.entry(p).or_default();
            e.0 += 1;
            e.1 += b.target_fte_demand;
        }
        let hist: Vec<String> = prefix_hist
            .iter()
            .map(|(k, v)| format!("{}:{}x{:.0}", k, v.0, v.1))
            .collect();
        eprintln!(
            "BIDPROBE[{}]: bids={} sum_bid={:.0} sum_target={:.0} sum_afford={:.0} sum_postcap={:.0} excluded={} pool_pre={:.0} | {}",
            region.id,
            bids.len(),
            sum_bid,
            sum_target,
            _diag_sum_affordable,
            _diag_sum_cap,
            zero_bid,
            region
                .class_demographics
                .rural_classes
                .values()
                .chain(region.class_demographics.urban_classes.values())
                .map(|d| d.available_fte)
                .sum::<f64>(),
            hist.join(" ")
        );
    }

    allocation_matrix.sum_bid_fte = bids.iter().map(|b| b.target_fte_demand).sum();

    // 3. Sort bids by wage (descending), company_id as canonical tiebreak —
    // most companies offer the same `average_wage`, so equal keys are the
    // norm and input order resolves them. Input is canonical today, but
    // explicit tiebreak keeps it so under any future reordering.
    bids.sort_by(|a, b| {
        b.offered_wage_per_fte
            .partial_cmp(&a.offered_wage_per_fte)
            .unwrap()
            .then_with(|| a.company_id.cmp(&b.company_id))
    });

    // Phase 2: Clear Market and Update Structures

    // 4. Build regional labor pool with per-class ledgers
    // E.6.3: Typed `DemographicClass` keys replace `(DemographyType, String)`.

    // Phase 23C: Synthetic commuter class — transient FTE injected from
    // adjacent regions. Kept out of `rural_classes`/`urban_classes` so wages
    // are not credited to local demographics — the caller remits them to home
    // regions. E.6.3: Now a typed `DemographicClass::Commuter` variant.
    let commuter_class = DemographicClass::Commuter;

    let mut pool = RegionalLaborPool {
        total_available_fte: 0.0,
        class_ledgers: BTreeMap::new(),
    };

    // Phase 17A: Job access restrictions for non-citizen classes.
    // If block_expert_jobs or block_skilled_jobs is set, reduce available_fte
    // proportionally to represent exclusion from certain job tiers.
    let citizenship_law = CitizenshipLaw::parse_law(civil_rights_law);
    let job_blocking_factor = if citizenship_law == CitizenshipLaw::Segregation {
        // Segregation blocks all non-citizens from most jobs.
        if discrimination_config.block_expert_jobs && discrimination_config.block_skilled_jobs {
            0.50 // Only unskilled labor available
        } else if discrimination_config.block_expert_jobs {
            0.70 // Expert tier blocked
        } else {
            1.0 // No blocking
        }
    } else if citizenship_law == CitizenshipLaw::CulturalAssimilation {
        if discrimination_config.block_expert_jobs {
            0.85 // Partial blocking under assimilation
        } else {
            1.0
        }
    } else {
        1.0
    };

    for (rural_class, demographics) in &region.class_demographics.rural_classes {
        let effective_fte = if demographics.legal_status != LegalStatus::Citizen {
            demographics.available_fte * job_blocking_factor
        } else {
            demographics.available_fte
        };
        pool.total_available_fte += effective_fte;
        pool.class_ledgers.insert(
            DemographicClass::from(*rural_class),
            ClassLaborLedger {
                available_fte: effective_fte,
                allocated_fte: 0.0,
                earned_wages: 0.0,
            },
        );
    }

    for (urban_class, demographics) in &region.class_demographics.urban_classes {
        let effective_fte = if demographics.legal_status != LegalStatus::Citizen {
            demographics.available_fte * job_blocking_factor
        } else {
            demographics.available_fte
        };
        pool.total_available_fte += effective_fte;
        pool.class_ledgers.insert(
            DemographicClass::from(*urban_class),
            ClassLaborLedger {
                available_fte: effective_fte,
                allocated_fte: 0.0,
                earned_wages: 0.0,
            },
        );
    }

    // Phase 47: Workforce isolation — subtract furloughed workers from the
    // available labor pool. Furloughed workers are held by their companies
    // (in `furloughed_workers_count`) and do NOT participate in labor market
    // clearing. They are not counted as unemployed or available for hire.
    // The total furloughed FTE is subtracted proportionally from all classes
    // since we don't track which class furloughed workers belong to.
    let total_furloughed: f64 = companies
        .iter()
        .filter(|c| c.region_id == region.id)
        .map(|c| c.furloughed_workers_count)
        .sum();
    if total_furloughed > 0.0 && pool.total_available_fte > 0.0 {
        let fraction = (total_furloughed / pool.total_available_fte).min(1.0);
        for ledger in pool.class_ledgers.values_mut() {
            ledger.available_fte = (ledger.available_fte * (1.0 - fraction)).max(0.0);
        }
        pool.total_available_fte = (pool.total_available_fte - total_furloughed).max(0.0);
    }

    // Phase 23C: Inject commuter FTE (workers from adjacent regions who can
    // afford the PassengerTransport ticket). These are tracked under the
    // synthetic `DemographicClass::Commuter` so they participate in the same
    // weighted-distribution clearing loop. Their wages are NOT credited to
    // local class savings; they are accumulated in `allocation_matrix.commuter_wages`
    // for the caller to remit to home regions.
    if commuter_inflow_fte > 0.0 {
        pool.total_available_fte += commuter_inflow_fte;
        pool.class_ledgers.insert(
            commuter_class,
            ClassLaborLedger {
                available_fte: commuter_inflow_fte,
                allocated_fte: 0.0,
                earned_wages: 0.0,
            },
        );
    }

    // 5. Clear market (highest wage first) with per-class wage tracking
    //
    // Macro-Remediation: under labor scarcity (total demand > pool), allocate
    // the pool pro-rata weighted by demand×wage instead of winner-take-all.
    // The old loop filled the top bid 100% before moving on, so when demand
    // exceeded supply the bottom half of the market hired literally zero
    // workers — a zero-FTE company cannot produce, bid for inputs, or ever
    // recover, turning wage competition into a one-turn death lottery for
    // hundreds of companies. Weighted shares preserve the wage signal (a
    // 2× wage still draws 2× the workers per unit demand) while every
    // bidder with a positive offer staffs a nonzero workforce. When the
    // pool covers total demand the share formula naturally allocates each
    // bid's full demand — identical to the old behavior.
    let total_demand_fte: f64 = bids
        .iter()
        .map(|b| b.target_fte_demand)
        .sum();
    let scarcity = if total_demand_fte > 0.0 {
        pool.total_available_fte < total_demand_fte
    } else {
        false
    };
    let pool_at_clear = pool.total_available_fte;
    let total_weighted: f64 = if scarcity {
        bids.iter()
            .map(|b| {
                b.target_fte_demand * b.offered_wage_per_fte.max(0.0)
            })
            .sum()
    } else {
        0.0
    };

    let mut remaining_pool = pool.total_available_fte;
    for bid in &bids {
        if remaining_pool <= 0.0 {
            break; // Pool exhausted
        }

        let mut fte_to_distribute = if scarcity {
            let weight =
                bid.target_fte_demand * bid.offered_wage_per_fte.max(0.0);
            let share = if total_weighted > 0.0 {
                weight / total_weighted
            } else {
                bid.target_fte_demand / total_demand_fte
            };
            (pool_at_clear * share).min(bid.target_fte_demand)
        } else {
            bid.target_fte_demand.min(remaining_pool)
        }
        .min(remaining_pool);
        let mut total_secured_by_company = 0.0;
        let wage_per_fte = bid.offered_wage_per_fte;

        // CRITICAL FIX: Spillover Loop with physical clamping to prevent Negative Labor Leak
        // Highly suitable classes with tiny populations cannot be over-drafted
        while fte_to_distribute > 0.001 {
            // Recalculate weighted availability each pass (changes as classes are depleted)
            let weighted_total_available: f64 = pool
                .class_ledgers
                .iter()
                .map(|(class, ledger)| {
                    let suitability = get_suitability_multiplier(*class, &bid.sector, config);
                    ledger.available_fte * suitability
                })
                .sum();

            // CRITICAL FIX: Phantom Labor guard - if no one is willing to take the job
            if weighted_total_available <= 0.0 {
                break;
            }

            let mut secured_this_pass = 0.0;

            for (class, ledger) in pool.class_ledgers.iter_mut() {
                if ledger.available_fte > 0.0 {
                    let suitability = get_suitability_multiplier(*class, &bid.sector, config);
                    let weighted_share =
                        (ledger.available_fte * suitability) / weighted_total_available;
                    let theoretical_fte = fte_to_distribute * weighted_share;

                    // CRITICAL FIX: Physical clamp - cannot draft more than available
                    let actual_fte = theoretical_fte.min(ledger.available_fte);
                    let class_wage = actual_fte * wage_per_fte;

                    ledger.allocated_fte += actual_fte;
                    ledger.earned_wages += class_wage;
                    ledger.available_fte -= actual_fte;

                    // Phase 6.5: Track company×class allocation for payment-in-kind
                    // E.6.3: Typed `DemographicClass` key
                    let key = (bid.company_id.clone(), *class);
                    *allocation_matrix.fte.entry(key.clone()).or_insert(0.0) += actual_fte;
                    *allocation_matrix.wages.entry(key).or_insert(0.0) += class_wage;

                    secured_this_pass += actual_fte;
                }
            }

            // Break if no progress made (prevents infinite floating-point loops)
            if secured_this_pass < 0.001 {
                break;
            }

            fte_to_distribute -= secured_this_pass;
            total_secured_by_company += secured_this_pass;
        }

        // Update company with actual secured FTE (may be less than target due to clamping)
        if let Some(company) = companies.iter_mut().find(|c| c.id == bid.company_id) {
            company.fulfilled_fte = total_secured_by_company.round() as u32;
        }

        remaining_pool -= total_secured_by_company;
    }

    // Phase 3: Double-Entry Wage Distribution with PIT Withholding (Fix 1.22)
    //
    // Fix 1.22: PIT is withheld at the source during wage payment.
    // - Gross wage is debited from the company (unchanged)
    // - Net wage (gross * (1 - pit_rate)) is credited to citizen savings
    // - Withheld PIT (gross * pit_rate) is accumulated for Treasury routing

    // Phase 43: Batch bank sync — accumulate wage+severance debits per bank
    // in a HashMap during the hot loops, then apply them to bank balance
    // sheets AFTER the loops. This avoids O(N) bank lookups inside the
    // per-company wage loop (strict performance rule).
    let mut bank_debits: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    // Phase 94: Track total actual paid vs total wage obligation.
    // Citizens must be credited based on ACTUAL paid amount, not gross wage.
    // If a company can't afford full payroll, actual_paid < wage_payment.
    // Crediting citizens with gross_wage while debiting only actual_paid
    // creates money (M0 violation). We compute a region-wide paid ratio
    // and scale all class earned_wages by it (Rule 5: proportional).
    let mut total_actual_paid: f64 = 0.0;
    let mut total_wage_obligation: f64 = 0.0;
    // Phase 94: Track total arrears repayment to credit citizen savings.
    let mut total_arrears_to_workers: f64 = 0.0;
    let mut total_actual_paid_banked: f64 = 0.0;
    let mut total_actual_paid_unbanked: f64 = 0.0;
    let mut total_bank_self_debits: f64 = 0.0;

    // Phase 94: Build a map of company_id → exact earned_wages from the
    // allocation matrix. This avoids rounding discrepancies between
    // fulfilled_fte (rounded) and sum(earned_wages) (unrounded), which
    // would create FiatDestruction when rounding goes UP.
    let mut company_earned_wages: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for ((company_id, _class), wage) in &allocation_matrix.wages {
        *company_earned_wages.entry(company_id.clone()).or_insert(0.0) += wage;
    }

    // 1. Debit companies for their exact gross wage payments
    // Phase 40: Wage arrears — if fulfilled_fte exceeds what the company can
    // afford (FTE retention floor), the unpaid portion accrues as arrears
    // instead of driving cash negative. This maintains strict double-entry:
    // arrears are a liability, not magical money.
    // Phase 41: Skip payroll for striking companies — workers are on strike,
    // the company does not pay them. The union pays strike benefits instead.
    for company in companies.iter_mut().filter(|c| c.region_id == region.id) {
        if company.is_striking {
            // Striking workers are NOT paid by the company.
            // Strike benefits are paid by the union in process_unions.
            continue;
        }
        // Phase 94: Use exact earned_wages from the allocation matrix instead
        // of fulfilled_fte (rounded) * offered_wage_per_fte. When rounding
        // goes UP, the rounded wage_payment exceeds sum(earned_wages),
        // debiting more from companies than is credited to citizens.
        let wage_payment = company_earned_wages.get(&company.id).copied().unwrap_or(0.0);
        total_wage_obligation += wage_payment;

        // Phase 94: Banks pay wages from reserves_at_central_bank (M0), not
        // from brokerage_account.cash (off-balance-sheet, NOT in M0). Debiting
        // non-M0 cash while crediting citizen savings (M0) creates fiat.
        let is_bank = company.balance_sheet.is_some();
        // ARREARS-1: the wage trust (`debit_cash`) holds settlement revenue
        // garnished for workers — it funds payroll ahead of general cash.
        // For banked firms it is deposit-backed escrow; for unbanked firms
        // it is M0-counted fiat. Either way it is the company's own money
        // earmarked for workers.
        let trust_cash = company.debit_cash.max(0.0);
        let pocket_cash = if is_bank {
            company.balance_sheet.as_ref()
                .map(|bs| bs.reserves_at_central_bank.max(0.0))
                .unwrap_or(0.0)
        } else {
            company.brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(company.available_cash.max(0.0))
        };
        let available_cash = pocket_cash + trust_cash;

        // Draw the wage payment: trust (earmarked wages) first, then the
        // general pocket. Any unmet remainder accrues as wage arrears.
        // Wage overdraft remains permanently disabled: paying wages on
        // implicit credit cost ~10+ points of unemployment in
        // controlled A/B runs — for unbanked firms it poisons the
        // cash pocket (every later payment fails → production
        // dies), and for banked firms the growing `liabilities`
        // shrinks credit headroom into earlier cash-shortage
        // furloughs. Arrears are the designed payroll-credit
        // channel; the mandate's arrears gate only constrains
        // profitable companies, which pay full payroll anyway.
        let actual_paid = wage_payment.min(available_cash);
        if actual_paid > 0.0 {
            let from_trust = actual_paid.min(trust_cash);
            company.consume_bid_encumbrance(from_trust);
            let from_pocket = actual_paid - from_trust;
            if from_pocket > 0.0 {
                if is_bank {
                    if let Some(ref mut bs) = company.balance_sheet {
                        bs.reserves_at_central_bank -= from_pocket;
                        // Phase 94: Wages are an operating expense — debit equity
                        // (retained earnings) to maintain A=L+E. Without this,
                        // reserves decrease but liabilities/equity don't, breaking
                        // the balance sheet identity.
                        bs.tier_1_capital -= from_pocket;
                    }
                } else if let Some(ba) = &mut company.brokerage_account {
                    ba.cash -= from_pocket;
                } else {
                    company.available_cash -= from_pocket;
                }
            }
        }
        // Phase 92: Track transient wage flow for financial history.
        company.wages_paid_this_turn = actual_paid;
        company.arrears_accrued_this_turn = wage_payment - actual_paid;
        company.wage_arrears += wage_payment - actual_paid;

        // Phase 43: Accumulate wage debit for batch bank sync.
        if actual_paid > 0.0 {
            if is_bank {
                // Banks debit their own reserves directly — no bank_debits entry.
                total_actual_paid_banked += actual_paid;
                total_bank_self_debits += actual_paid;
            } else if let Some(ref bank_id) = company.primary_bank_id {
                *bank_debits.entry(bank_id.clone()).or_insert(0.0) += actual_paid;
                total_actual_paid_banked += actual_paid;
            } else {
                total_actual_paid_unbanked += actual_paid;
            }
        }
        total_actual_paid += actual_paid;

        // Phase 40: Repay existing arrears from available cash (30% of cash).
        // This runs after the current turn's payroll, so it uses remaining cash.
        // Phase 94: Arrears repayment credits citizen savings — the arrears
        // represent unpaid wages owed to workers. When repaid, the workers
        // must receive the money (M0 conservation: bank reserves ↓ = citizen
        // savings ↑). Without this credit, M0 is destroyed.
        if company.wage_arrears > 0.0 {
            // The wage trust (`debit_cash`) holds revenue garnished for
            // workers at settlement — it repays arrears ahead of the pocket.
            let trust = company.debit_cash.max(0.0);
            let pocket = if is_bank {
                company.balance_sheet.as_ref()
                    .map(|bs| bs.reserves_at_central_bank.max(0.0))
                    .unwrap_or(0.0)
            } else {
                company.brokerage_account
                    .as_ref()
                    .map(|ba| ba.cash.max(0.0))
                    .unwrap_or(company.available_cash.max(0.0))
            };
            let remaining_cash = pocket + trust;
            // 24-turn fix: a company whose recent books are in the black must
            // retire wage arrears in full from cash on hand — profit covers
            // payroll before any other drain (ARREARS-1). Unprofitable firms
            // keep the gradual 30%-of-crawl repayment.
            let repayment = if company.moving_avg_net_profit(3) > 0.0 {
                company.wage_arrears.min(remaining_cash)
            } else {
                (remaining_cash * 0.30).min(company.wage_arrears)
            };
            if repayment > 0.0 {
                let from_trust = repayment.min(trust);
                company.consume_bid_encumbrance(from_trust);
                let from_pocket = repayment - from_trust;
                if from_pocket > 0.0 {
                    if is_bank {
                        if let Some(ref mut bs) = company.balance_sheet {
                            bs.reserves_at_central_bank -= from_pocket;
                            // Phase 94: Wage arrears repayment is an expense — debit equity.
                            bs.tier_1_capital -= from_pocket;
                        }
                    } else if let Some(ba) = &mut company.brokerage_account {
                        ba.cash -= from_pocket;
                    } else {
                        company.available_cash -= from_pocket;
                    }
                }
                company.wage_arrears -= repayment;
                total_arrears_to_workers += repayment;
                // Phase 43: Accumulate arrears repayment for batch bank sync.
                if is_bank {
                    total_bank_self_debits += repayment;
                } else if let Some(ref bank_id) = company.primary_bank_id {
                    *bank_debits.entry(bank_id.clone()).or_insert(0.0) += repayment;
                }
            }
        }

        // Phase 40: Update productivity penalty from arrears.
        // 1% penalty per 10K arrears, capped at 50%.
        company.productivity_penalty = (company.wage_arrears / 10_000.0).min(0.50);
    }

    // Phase 37: Severance pay for laid-off workers.
    // Emergency Stabilization: Severance pay — 4x weekly wage per laid-off FTE
    // (increased from 2x). The old 30% cash cap is REMOVED. If the company
    // cannot afford full severance, the unpaid portion accrues as
    // `severance_arrears` (a liability repaid at 30%/turn from future cash,
    // same pattern as wage_arrears). This makes firing expensive even when
    // cash is low, forcing the corporate AI to prefer furlough.
    const SEVERANCE_MULTIPLIER: f64 = 4.0; // 4 weeks of wages per laid-off FTE
    let mut total_severance_to_workers: f64 = 0.0;
    for company in companies.iter_mut().filter(|c| c.region_id == region.id) {
        let laid_off = company.prev_fulfilled_fte as f64 - company.fulfilled_fte as f64;
        if laid_off <= 0.0 || company.offered_wage_per_fte <= 0.0 {
            continue;
        }
        let gross_severance = laid_off * company.offered_wage_per_fte * SEVERANCE_MULTIPLIER;
        let is_bank = company.balance_sheet.is_some();
        let available = if is_bank {
            company.balance_sheet.as_ref()
                .map(|bs| bs.reserves_at_central_bank.max(0.0))
                .unwrap_or(0.0)
        } else {
            company.brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(company.available_cash.max(0.0))
        };
        let payable = gross_severance.min(available);
        let unpaid = gross_severance - payable;

        // Pay what's available
        if payable > 0.0 {
            if is_bank {
                if let Some(ref mut bs) = company.balance_sheet {
                    bs.reserves_at_central_bank -= payable;
                    // Phase 94: Severance is an expense — debit equity.
                    bs.tier_1_capital -= payable;
                }
            } else if let Some(ba) = &mut company.brokerage_account {
                ba.cash -= payable;
            } else {
                company.available_cash -= payable;
            }
            total_severance_to_workers += payable;
            // Phase 43: Accumulate severance debit for batch bank sync.
            if is_bank {
                total_bank_self_debits += payable;
            } else if let Some(ref bank_id) = company.primary_bank_id {
                *bank_debits.entry(bank_id.clone()).or_insert(0.0) += payable;
            }
        }
        // Accrue unpaid severance as a liability (same pattern as wage_arrears)
        if unpaid > 0.0 {
            company.severance_arrears += unpaid;
        }
    }

    // Emergency Stabilization: Repay severance arrears from remaining cash
    // (30% of available cash per turn, same pattern as wage_arrears repayment).
    for company in companies.iter_mut().filter(|c| c.region_id == region.id) {
        if company.severance_arrears <= 0.0 {
            continue;
        }
        let is_bank = company.balance_sheet.is_some();
        let remaining_cash = if is_bank {
            company.balance_sheet.as_ref()
                .map(|bs| bs.reserves_at_central_bank.max(0.0))
                .unwrap_or(0.0)
        } else {
            company.brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(company.available_cash.max(0.0))
        };
        let repayment = (remaining_cash * 0.30).min(company.severance_arrears);
        if repayment > 0.0 {
            if is_bank {
                if let Some(ref mut bs) = company.balance_sheet {
                    bs.reserves_at_central_bank -= repayment;
                    // Phase 94: Severance arrears repayment is an expense — debit equity.
                    bs.tier_1_capital -= repayment;
                }
            } else if let Some(ba) = &mut company.brokerage_account {
                ba.cash -= repayment;
            } else {
                company.available_cash -= repayment;
            }
            company.severance_arrears -= repayment;
            total_severance_to_workers += repayment;
            if is_bank {
                total_bank_self_debits += repayment;
            } else if let Some(ref bank_id) = company.primary_bank_id {
                *bank_debits.entry(bank_id.clone()).or_insert(0.0) += repayment;
            }
        }
    }

    // Phase 43: Batch bank sync — apply aggregated wage+severance debits to
    // bank balance sheets. This runs ONCE per region, iterating only the
    // handful of banks that received debits (O(B), not O(N*B)).
    // When a company pays wages from its brokerage account (bank deposit),
    // the bank's deposits liability and reserves asset must decrease by the
    // same amount to maintain double-entry consistency.
    let mut _bank_debits_applied: f64 = 0.0;
    let mut _bank_debits_dropped: f64 = 0.0;
    for (bank_id, total_debit) in &bank_debits {
        if let Some(bank) = companies.iter_mut().find(|c| c.id == *bank_id) {
            if let Some(ref mut bs) = bank.balance_sheet {
                crate::state::banking::settle_deposit_debit(bs, *total_debit);
                bs.reserves_at_central_bank -= total_debit;
                _bank_debits_applied += total_debit;
            } else {
                _bank_debits_dropped += total_debit;
                #[cfg(feature = "diagnostic")]
                eprintln!("BANK_SYNC_DROP: bank_id={} debit={:.0} reason=no_balance_sheet", bank_id, total_debit);
            }
        } else {
            _bank_debits_dropped += total_debit;
            #[cfg(feature = "diagnostic")]
            eprintln!("BANK_SYNC_DROP: bank_id={} debit={:.0} reason=bank_not_found", bank_id, total_debit);
        }
    }
    #[cfg(feature = "diagnostic")]
    eprintln!("BANK_SYNC_SUM: region={} applied={:.0} dropped={:.0} bank_debits_entries={}",
        region.id, _bank_debits_applied, _bank_debits_dropped, bank_debits.len());


    // 2. Credit classes with their net earnings (gross - garnishment - PIT - remittances), track withheld amounts
    let mut total_pit_withheld: f64 = 0.0;
    let mut total_remittances_withheld: f64 = 0.0;
    let mut total_garnishments_withheld: f64 = 0.0;

    // Phase 94: Scale earned_wages by the actual paid ratio.
    // If companies couldn't afford full payroll, actual_paid < wage_obligation.
    // Crediting citizens with full gross_wage while only debiting actual_paid
    // creates money. Scale all earned_wages proportionally (Rule 5).
    //
    // Phase 94 fix: Use total_earned_wages (unrounded sum of class earned_wages)
    // as the denominator, NOT total_wage_obligation. The wage_obligation uses
    // fulfilled_fte which is ROUNDED (total_secured_by_company.round() as u32),
    // while earned_wages uses the unrounded total_secured_by_company. When
    // rounding goes down, earned_wages > wage_obligation, and the old formula
    // paid_ratio = actual_paid / wage_obligation = 1.0 (if companies afford
    // payroll), leaving earned_wages > actual_paid — creating M0 fiat.
    // Using total_earned_wages ensures earned_wages * paid_ratio = actual_paid
    // exactly, sealing the rounding-induced FiatCreation leak.
    let total_earned_wages: f64 = pool.class_ledgers.values().map(|l| l.earned_wages).sum();
    let paid_ratio = if total_earned_wages > 0.0 {
        (total_actual_paid / total_earned_wages).min(1.0)
    } else {
        1.0
    };
    if paid_ratio < 1.0 {
        for ledger in pool.class_ledgers.values_mut() {
            ledger.earned_wages *= paid_ratio;
        }
    }

    // Phase 17A: Apply wage discrimination against non-citizen classes.
    // Non-citizen workers (legal_status != Citizen under CulturalAssimilation
    // or Segregation) receive wage * non_citizen_wage_multiplier. The withheld
    // fraction remains with the employer as labor cost savings — the employer
    // debits less cash for non-citizen FTE. This is a market outcome, not a
    // state seizure.
    let citizenship_law = CitizenshipLaw::parse_law(civil_rights_law);
    if citizenship_law != CitizenshipLaw::OpenCitizenship {
        // Build a set of non-citizen DemographicClass keys.
        let mut non_citizen_classes: Vec<DemographicClass> = Vec::new();
        for (rural_class, demo) in &region.class_demographics.rural_classes {
            if demo.legal_status != LegalStatus::Citizen {
                non_citizen_classes.push(DemographicClass::from(*rural_class));
            }
        }
        for (urban_class, demo) in &region.class_demographics.urban_classes {
            if demo.legal_status != LegalStatus::Citizen {
                non_citizen_classes.push(DemographicClass::from(*urban_class));
            }
        }
        // Scale earned_wages for non-citizen classes.
        for key in &non_citizen_classes {
            if let Some(ledger) = pool.class_ledgers.get_mut(key) {
                ledger.earned_wages *= discrimination_config.non_citizen_wage_multiplier;
            }
        }
    }

    // v4.24.0 WS2: feudal rent on tied rural labor. Serfs and landless
    // laborers owe manorial dues to the estate-owning aristocracy —
    // modeled as a rent share of net wage income, withheld from the class
    // credit below and pooled into `total_feudal_rent` for transfer to
    // the region's Aristocracy class (M0-conserving). The rate is the
    // population-weighted `feudal_dues_rate` of the region's
    // AristocraticEstate domains where they exist; where domains aren't
    // modeled, the rate scales with manorial COVERAGE — the fraction of
    // the rural labor market tied to estates (Latifundium capacity plus
    // bound serf FTE over total rural FTE). Serfdom IS a feudal relation:
    // where serfs work, dues exist regardless of how few serfs remain.
    let feudal_rent_rate = {
        use crate::society::factional_domains::FactionalDomainConfig;
        use crate::society::geography::FactionDomainType;
        let rural_fte: f64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|d| d.available_fte)
            .sum();
        if rural_fte <= 0.0 {
            0.0
        } else {
            let default_dues =
                FactionalDomainConfig::default().aristocratic_feudal_dues_rate;
            let mut estate_pop = 0.0;
            let mut weighted_rate = 0.0;
            for domain in region.micro_regions.values() {
                if domain.faction_type == FactionDomainType::AristocraticEstate {
                    let w = domain.population as f64;
                    estate_pop += w;
                    weighted_rate += domain.local_laws.feudal_dues_rate * w;
                }
            }
            let domain_rate = if estate_pop > 0.0 {
                (weighted_rate / estate_pop).min(default_dues)
            } else {
                0.0
            };
            // Manorial presence: the estate economy is the class
            // structure itself — where an Aristocracy class or bound
            // serfs exist, landless rural labor works estate land and
            // owes dues. (Latifundium legal-form companies are only a
            // marginal estate representation — e.g. monastery farms —
            // so they can't gate the rule.)
            let latif_fte: f64 = companies
                .iter()
                .filter(|c| {
                    c.region_id == region.id
                        && matches!(
                            c.legal_form,
                            crate::entities::legal_form::LegalForm::Latifundium(_)
                        )
                })
                .map(|c| c.worker_capacity as f64)
                .sum();
            let has_serfs = region
                .class_demographics
                .rural_classes
                .get(&crate::society::geography::RuralClass::Serf)
                .map(|d| d.population > 0 || d.available_fte > 0.0)
                .unwrap_or(false);
            let has_aristocracy = region
                .class_demographics
                .rural_classes
                .get(&crate::society::geography::RuralClass::Aristocracy)
                .map(|d| d.population > 0)
                .unwrap_or(false);
            let has_manors = latif_fte > 0.0 || has_serfs || has_aristocracy;
            domain_rate.max(if has_manors { default_dues } else { 0.0 })
        }
    };
    let mut total_feudal_rent = 0.0_f64;

    // E.6.3: Use typed `DemographicClass` keys to access ledgers
    for (rural_class, demographics) in region.class_demographics.rural_classes.iter_mut() {
        let demo_class = DemographicClass::from(*rural_class);
        if let Some(ledger) = pool.class_ledgers.get(&demo_class) {
            let gross_wage = ledger.earned_wages;

            // Phase 18B: Source-level community service garnishment (deducted from gross wage)
            let garnish_key = (region.id.clone(), demo_class);
            let garnishment_rate = garnishment_rates.get(&garnish_key).copied().unwrap_or(0.0);
            let garnishment_amount = gross_wage * garnishment_rate;
            let post_garnishment = gross_wage - garnishment_amount;

            // R8.6: TaxDeferred PIT deduction — reduce taxable base by
            // per-capita tax-advantaged contributions. Clamp at zero so
            // PIT never goes negative (no cash refund for zero-tax citizens).
            let per_capita_contribution = if demographics.population > 0 {
                demographics.tax_advantaged_contributions_this_year / demographics.population as f64
            } else {
                0.0
            };
            // v4.21.0 WS2: statutory personal allowance — the first
            // `pit_allowance_per_fte` of each FTE's earnings is PIT-exempt.
            // Aggregate units: `allocated_fte * allowance_per_fte` matches
            // `post_garnishment`'s class-total units. Relief is progressive
            // in effect: a class paid below the wage anchor sees a larger
            // exempt share than one paid above it.
            let allowance_amount = ledger.allocated_fte * pit_allowance_per_fte;
            let taxable_base =
                (post_garnishment - per_capita_contribution - allowance_amount).max(0.0);

            let pit_amount = taxable_base * pit_rate;
            let net_wage = post_garnishment - pit_amount;

            // v4.24.0 WS2: feudal rent — tied rural classes owe manorial
            // dues on net wage income, withheld before the class credit
            // and pooled for the region's Aristocracy (credited after
            // this loop — the aristocracy class can't be borrowed while
            // rural_classes is iterated mutably).
            //
            // Serfs are BOUND labor: their entire labor product belongs
            // to the estate — they subsist on in-kind payment, never
            // cash (see payment_in_kind: "in-kind INSTEAD of wages").
            // The whole net wage is the manorial due. Landless laborers
            // are free wage workers — only the estate-covered share of
            // their wage owes dues.
            let feudal_due = match rural_class {
                crate::society::geography::RuralClass::Serf => net_wage.max(0.0),
                crate::society::geography::RuralClass::LandlessLaborer => {
                    (net_wage * feudal_rent_rate).max(0.0)
                }
                _ => 0.0,
            };
            total_feudal_rent += feudal_due;
            let take_home = net_wage - feudal_due;

            // Phase 18A: Source-level remittance deduction for TemporaryWorkers
            let remittance_amount = if demographics.legal_status == LegalStatus::TemporaryWorker {
                let rate = default_remittance_rate();
                let remit = take_home * rate;
                demographics.savings += take_home - remit;
                remit
            } else {
                demographics.savings += take_home;
                0.0
            };

            demographics.allocated_fte = ledger.allocated_fte; // CRITICAL: Update for Exploitation Penalty
            total_pit_withheld += pit_amount;
            total_remittances_withheld += remittance_amount;
            total_garnishments_withheld += garnishment_amount;
        }
    }

    #[cfg(feature = "diagnostic")]
    eprintln!(
        "FEUDALRENT: r={} rate={:.4} rent={:.0}",
        region.id, feudal_rent_rate, total_feudal_rent
    );

    // v4.24.0 WS2: transfer pooled feudal rent to the region's
    // Aristocracy class — the manorial income that funds aristocratic
    // savings. Rent extracted above was withheld from serf/laborer wage
    // credits, so this is a pure transfer (M0-conserving).
    if total_feudal_rent > 0.0 {
        let demo = region
            .class_demographics
            .rural_classes
            .entry(crate::society::geography::RuralClass::Aristocracy)
            .or_default();
        demo.savings += total_feudal_rent;
        if demo.population > 0 {
            demo.savings_per_capita = demo.savings / demo.population as f64;
        }
    }

    // v4.24.0 WS2: serfs hold no property — the feudal relation vests
    // everything in the manor ("in-kind INSTEAD of wages … savings forced
    // to 0", payment_in_kind.rs). Non-wage scraps still trickle in from
    // relief, remittances, helicopter backstops and migration, so sweep
    // the residual class balance into the estate pool each turn. Pure
    // class-to-class transfer (M0-conserving); gated on the manorial
    // economy operating this turn — serfs exist and dues were extracted
    // (or a resident Aristocracy claims the feudal right).
    {
        use crate::society::geography::RuralClass;
        let serf_residual = region
            .class_demographics
            .rural_classes
            .get(&RuralClass::Serf)
            .map(|d| d.savings.max(0.0))
            .unwrap_or(0.0);
        let manor_active = total_feudal_rent > 0.0
            || region
                .class_demographics
                .rural_classes
                .get(&RuralClass::Aristocracy)
                .map(|d| d.population > 0)
                .unwrap_or(false);
        if serf_residual > 0.0 && manor_active {
            if let Some(serf) = region
                .class_demographics
                .rural_classes
                .get_mut(&RuralClass::Serf)
            {
                serf.savings -= serf_residual;
                serf.savings_per_capita = if serf.population > 0 {
                    serf.savings / serf.population as f64
                } else {
                    0.0
                };
            }
            let demo = region
                .class_demographics
                .rural_classes
                .entry(RuralClass::Aristocracy)
                .or_default();
            demo.savings += serf_residual;
            if demo.population > 0 {
                demo.savings_per_capita = demo.savings / demo.population as f64;
            }
        }
    }

    for (urban_class, demographics) in region.class_demographics.urban_classes.iter_mut() {
        let demo_class = DemographicClass::from(*urban_class);
        if let Some(ledger) = pool.class_ledgers.get(&demo_class) {
            let gross_wage = ledger.earned_wages;

            // Phase 18B: Source-level community service garnishment (deducted from gross wage)
            let garnish_key = (region.id.clone(), demo_class);
            let garnishment_rate = garnishment_rates.get(&garnish_key).copied().unwrap_or(0.0);
            let garnishment_amount = gross_wage * garnishment_rate;
            let post_garnishment = gross_wage - garnishment_amount;

            // R8.6: TaxDeferred PIT deduction — reduce taxable base by
            // per-capita tax-advantaged contributions. Clamp at zero so
            // PIT never goes negative (no cash refund for zero-tax citizens).
            let per_capita_contribution = if demographics.population > 0 {
                demographics.tax_advantaged_contributions_this_year / demographics.population as f64
            } else {
                0.0
            };
            // v4.21.0 WS2: statutory personal allowance — the first
            // `pit_allowance_per_fte` of each FTE's earnings is PIT-exempt.
            // Aggregate units: `allocated_fte * allowance_per_fte` matches
            // `post_garnishment`'s class-total units. Relief is progressive
            // in effect: a class paid below the wage anchor sees a larger
            // exempt share than one paid above it.
            let allowance_amount = ledger.allocated_fte * pit_allowance_per_fte;
            let taxable_base =
                (post_garnishment - per_capita_contribution - allowance_amount).max(0.0);

            let pit_amount = taxable_base * pit_rate;
            let net_wage = post_garnishment - pit_amount;

            // Phase 18A: Source-level remittance deduction for TemporaryWorkers
            let remittance_amount = if demographics.legal_status == LegalStatus::TemporaryWorker {
                let rate = default_remittance_rate();
                let remit = net_wage * rate;
                demographics.savings += net_wage - remit;
                remit
            } else {
                demographics.savings += net_wage;
                0.0
            };

            demographics.allocated_fte = ledger.allocated_fte; // CRITICAL: Update for Exploitation Penalty
            total_pit_withheld += pit_amount;
            total_remittances_withheld += remittance_amount;
            total_garnishments_withheld += garnishment_amount;
        }
    }

    #[cfg(feature = "diagnostic")]
    {
        let total_gross_after: f64 = pool.class_ledgers.values().map(|l| l.earned_wages).sum();
        // Comprehensive reconciliation:
        // Credits = citizen_savings_increase + PIT + garnishments + remittances + commuter_wages
        // Debits = actual_paid + severance + arrears
        // citizen_savings_increase = gross_after - pit - garnishments - remittances (wage credits)
        //   + severance_credited + arrears_credited
        // Total credits = gross_after + commuter_wages + severance + arrears (if all credited)
        // Total debits = actual_paid + severance + arrears
        // Diff = gross_after + commuter - actual_paid
        let total_class_fte_check: f64 = region
            .class_demographics.rural_classes.values().map(|c| c.allocated_fte).sum::<f64>()
            + region.class_demographics.urban_classes.values().map(|c| c.allocated_fte).sum::<f64>();
        let severance_credited = if total_class_fte_check > 0.0 { total_severance_to_workers } else { 0.0 };
        let arrears_credited = if total_class_fte_check > 0.0 { total_arrears_to_workers } else { 0.0 };
        let total_credits = total_gross_after + allocation_matrix.commuter_wages + severance_credited + arrears_credited;
        let total_debits = total_actual_paid + total_severance_to_workers + total_arrears_to_workers;
        eprintln!("LABOR_RECON: region={} debits={:.0} credits={:.0} diff={:.0} | actual_paid={:.0} (banked={:.0} unbanked={:.0}) gross_after={:.0} commuter={:.0} sev_debit={:.0} sev_cred={:.0} arr_debit={:.0} arr_cred={:.0} pit={:.0} garn={:.0} rem={:.0} class_fte={:.0} bank_debits_applied={:.0} bank_self={:.0}",
            region.id, total_debits, total_credits, total_credits - total_debits,
            total_actual_paid, total_actual_paid_banked, total_actual_paid_unbanked, total_gross_after, allocation_matrix.commuter_wages,
            total_severance_to_workers, severance_credited,
            total_arrears_to_workers, arrears_credited,
            total_pit_withheld, total_garnishments_withheld, total_remittances_withheld,
            total_class_fte_check, _bank_debits_applied, total_bank_self_debits);
    }
    // Fix 1.22: Store total PIT withheld for caller to route to Treasury
    allocation_matrix.pit_withheld = total_pit_withheld;
    // Phase 18A: Store total remittances withheld for caller to route to ForeignEntity
    allocation_matrix.remittances_withheld = total_remittances_withheld;
    // Phase 18B: Store total garnishments withheld for caller to route to Treasury
    allocation_matrix.garnishments_withheld = total_garnishments_withheld;
    // Phase 94: Credit severance and arrears to regional class savings.
    // MOVED here from before the citizen wage credit phase, because
    // demo.allocated_fte is set DURING the wage credit phase (line ~720).
    // On Turn 0, allocated_fte was 0 at the old location, causing
    // total_class_fte=0 and severance/arrears NOT being distributed —
    // destroying M0 (bank debits happened but citizen credits didn't).
    // Now demo.allocated_fte reflects the current turn's allocation.
    if total_severance_to_workers > 0.0 {
        let total_class_fte: f64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|c| c.allocated_fte)
            .sum::<f64>()
            + region
                .class_demographics
                .urban_classes
                .values()
                .map(|c| c.allocated_fte)
                .sum::<f64>();
        if total_class_fte > 0.0 {
            // Phase 94: Track total credited to apply residual correction.
            // Proportional distribution (share * total) can lose cents due
            // to f64 rounding, accumulating to ~600K across 180 regions.
            let mut total_credited = 0.0;
            for demo in region.class_demographics.rural_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                let credit = total_severance_to_workers * share;
                demo.savings += credit;
                total_credited += credit;
            }
            for demo in region.class_demographics.urban_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                let credit = total_severance_to_workers * share;
                demo.savings += credit;
                total_credited += credit;
            }
            // Residual correction: credit any rounding remainder to the
            // first urban class (or first rural class if no urban classes).
            let residual = total_severance_to_workers - total_credited;
            if residual.abs() > 0.0 {
                if let Some(demo) = region.class_demographics.urban_classes.values_mut().next() {
                    demo.savings += residual;
                } else if let Some(demo) = region.class_demographics.rural_classes.values_mut().next() {
                    demo.savings += residual;
                }
            }
        }
    }
    if total_arrears_to_workers > 0.0 {
        let total_class_fte: f64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|c| c.allocated_fte)
            .sum::<f64>()
            + region
                .class_demographics
                .urban_classes
                .values()
                .map(|c| c.allocated_fte)
                .sum::<f64>();
        if total_class_fte > 0.0 {
            let mut total_credited = 0.0;
            for demo in region.class_demographics.rural_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                let credit = total_arrears_to_workers * share;
                demo.savings += credit;
                total_credited += credit;
            }
            for demo in region.class_demographics.urban_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                let credit = total_arrears_to_workers * share;
                demo.savings += credit;
                total_credited += credit;
            }
            let residual = total_arrears_to_workers - total_credited;
            if residual.abs() > 0.0 {
                if let Some(demo) = region.class_demographics.urban_classes.values_mut().next() {
                    demo.savings += residual;
                } else if let Some(demo) = region.class_demographics.rural_classes.values_mut().next() {
                    demo.savings += residual;
                }
            }
        }
    }

    // Phase 23C: Extract commuter wages and FTE for caller-side remittance.
    // Commuters pay PIT in the host region (already included in
    // `total_pit_withheld` via the synthetic class ledger above). Their net
    // wages must be remitted to their home regions by the caller — they are
    // NOT credited to local `class_demographics` (the `Commuter` variant
    // is not present in `rural_classes`/`urban_classes`).
    if let Some(commuter_ledger) = pool.class_ledgers.get(&commuter_class) {
        let gross = commuter_ledger.earned_wages;
        if gross > 0.0 {
            // PIT on commuter wages (host region keeps it); the personal
            // allowance applies identically to commuter earnings.
            let pit = (gross - pit_allowance_per_fte * commuter_ledger.allocated_fte).max(0.0)
                * pit_rate;
            allocation_matrix.pit_withheld += pit;
            allocation_matrix.commuter_wages = gross - pit;
            allocation_matrix.commuter_fte = commuter_ledger.allocated_fte;
        }
    }

    // Phase 6.5: Return labor allocation matrix for payment-in-kind
    allocation_matrix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::society::geography::{ClassDemographics, Region, RuralClass};
    use crate::state::Calendar;

    fn make_region_with_labor(id: &str, fte: f64) -> Region {
        let mut region = Region::default();
        region.id = id.to_string();
        let mut demo = ClassDemographics::default();
        demo.available_fte = fte;
        demo.population = (fte * 2.0) as i64;
        demo.labor_participation = 0.5;
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, demo);
        region
    }

    fn make_company(id: &str, region: &str, wage: f64, demand: f64, cash: f64) -> Company {
        let mut c = Company::default();
        c.id = id.to_string();
        c.region_id = region.to_string();
        c.offered_wage_per_fte = wage;
        c.target_fte_demand = demand as u32;
        c.sector = Sector::Agriculture;
        if let Some(ref mut ba) = c.brokerage_account {
            ba.cash = cash;
        } else {
            let mut ba = crate::securities::BrokerageAccount::default();
            ba.cash = cash;
            c.brokerage_account = Some(ba);
        }
        c
    }

    #[test]
    fn commuter_inflow_augments_labor_pool() {
        let mut region = make_region_with_labor("R1", 10.0);
        let mut companies = vec![make_company("C1", "R1", 100.0, 20.0, 100000.0)];
        let calendar = Calendar::default();
        let config = LaborConfig::default();
        let garnishments = BTreeMap::new();

        let alloc = resolve_regional_labor_market(
            &mut region,
            &mut companies,
            &[],
            None,
            &calendar,
            &config,
            0.0,
            0.0,
            &garnishments,
            5.0, // 5 commuter FTE
            "open_citizenship",
            &DiscriminationConfig::default(),
        );

        // Company demanded 20 FTE, local pool 10 + commuter 5 = 15 available.
        // Should fulfill 15 (or close to it).
        assert!(
            companies[0].fulfilled_fte >= 14,
            "fulfilled_fte = {}",
            companies[0].fulfilled_fte
        );
        // Commuter wages should be tracked.
        assert!(
            alloc.commuter_fte > 0.0,
            "commuter_fte = {}",
            alloc.commuter_fte
        );
        assert!(
            alloc.commuter_wages > 0.0,
            "commuter_wages = {}",
            alloc.commuter_wages
        );
    }

    #[test]
    fn no_commuter_inflow_when_zero() {
        let mut region = make_region_with_labor("R1", 10.0);
        let mut companies = vec![make_company("C1", "R1", 100.0, 5.0, 100000.0)];
        let calendar = Calendar::default();
        let config = LaborConfig::default();
        let garnishments = BTreeMap::new();

        let alloc = resolve_regional_labor_market(
            &mut region,
            &mut companies,
            &[],
            None,
            &calendar,
            &config,
            0.0,
            0.0,
            &garnishments,
            0.0, // no commuters
            "open_citizenship",
            &DiscriminationConfig::default(),
        );

        assert_eq!(alloc.commuter_fte, 0.0);
        assert_eq!(alloc.commuter_wages, 0.0);
    }
}
