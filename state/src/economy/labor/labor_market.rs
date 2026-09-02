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
    minimum_wage: Option<f64>,
    _calendar: &Calendar,
    config: &LaborConfig,
    pit_rate: f64,
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
    for company in region_companies {
        // Reset fulfilled_fte
        company.fulfilled_fte = 0;

        // Liquidity clamping
        // Phase 33: Fall back to available_cash if no brokerage_account.
        // This prevents companies loaded from old saves (without brokerage_account)
        // from being clamped to 0 FTE and mass-bankrupting.
        let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
            company
                .brokerage_account
                .as_ref()
                .map(|ba| ba.cash / company.offered_wage_per_fte)
                .unwrap_or(company.available_cash / company.offered_wage_per_fte)
        } else {
            0.0
        };

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
        let retention_base = if company
            .seasonal_profile
            .as_ref()
            .map(|p| p.current_state == crate::entities::SeasonalState::Furloughed)
            .unwrap_or(false)
        {
            company.target_fte_demand as f64 // standby level
        } else {
            company.prev_fulfilled_fte as f64 // normal: 90% of last turn's workforce
        };
        let retention_floor = retention_base * FTE_RETENTION_FLOOR;
        if max_affordable_fte < retention_floor && retention_floor > 0.0 {
            // Company can't afford full payroll — retain at retention_floor,
            // the unpaid wages will accrue as arrears after clearing.
            clamped_demand = clamped_demand.max(retention_floor);
        }

        // Phase 37: Hiring friction — cap growth to 15% per turn.
        // Small companies (<10 FTE) are exempt so they can scale up from zero.
        // This prevents the ±100% employment swings that destabilize GDP.
        const MAX_HIRING_GROWTH_RATE: f64 = 0.15;
        const SMALL_COMPANY_FTE_THRESHOLD: f64 = 10.0;
        if company.prev_fulfilled_fte as f64 >= SMALL_COMPANY_FTE_THRESHOLD {
            let max_hireable = company.prev_fulfilled_fte as f64 * (1.0 + MAX_HIRING_GROWTH_RATE);
            clamped_demand = clamped_demand.min(max_hireable);
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

    // 3. Sort bids by wage (descending)
    bids.sort_by(|a, b| {
        b.offered_wage_per_fte
            .partial_cmp(&a.offered_wage_per_fte)
            .unwrap()
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
    let mut remaining_pool = pool.total_available_fte;
    for bid in &bids {
        if remaining_pool <= 0.0 {
            break; // Pool exhausted
        }

        let mut fte_to_distribute = bid.target_fte_demand.min(remaining_pool);
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
        let wage_payment = company.fulfilled_fte as f64 * company.offered_wage_per_fte;
        total_wage_obligation += wage_payment;

        // Compute how much cash is actually available
        let available_cash = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(company.available_cash.max(0.0));

        let actual_paid = if wage_payment <= available_cash {
            // Company can afford full payroll — debit normally
            if let Some(ba) = &mut company.brokerage_account {
                ba.cash -= wage_payment;
            } else {
                company.available_cash -= wage_payment;
            }
            // Phase 92: Track transient wage flow for financial history.
            company.wages_paid_this_turn = wage_payment;
            company.arrears_accrued_this_turn = 0.0;
            wage_payment
        } else {
            // Company cannot afford full payroll — pay what's available,
            // accrue the rest as wage arrears (liability).
            let payable = available_cash;
            let arrears_this_turn = wage_payment - payable;
            if payable > 0.0 {
                if let Some(ba) = &mut company.brokerage_account {
                    ba.cash -= payable;
                } else {
                    company.available_cash -= payable;
                }
            }
            company.wage_arrears += arrears_this_turn;
            // Phase 92: Track transient wage flow for financial history.
            company.wages_paid_this_turn = payable;
            company.arrears_accrued_this_turn = arrears_this_turn;
            payable
        };

        // Phase 43: Accumulate wage debit for batch bank sync.
        if actual_paid > 0.0 {
            if let Some(ref bank_id) = company.primary_bank_id {
                *bank_debits.entry(bank_id.clone()).or_insert(0.0) += actual_paid;
            }
        }
        total_actual_paid += actual_paid;

        // Phase 40: Repay existing arrears from available cash (30% of cash).
        // This runs after the current turn's payroll, so it uses remaining cash.
        if company.wage_arrears > 0.0 {
            let remaining_cash = company
                .brokerage_account
                .as_ref()
                .map(|ba| ba.cash.max(0.0))
                .unwrap_or(company.available_cash.max(0.0));
            let repayment = (remaining_cash * 0.30).min(company.wage_arrears);
            if repayment > 0.0 {
                if let Some(ba) = &mut company.brokerage_account {
                    ba.cash -= repayment;
                } else {
                    company.available_cash -= repayment;
                }
                company.wage_arrears -= repayment;
                // Phase 43: Accumulate arrears repayment for batch bank sync.
                if let Some(ref bank_id) = company.primary_bank_id {
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
        let available = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(company.available_cash.max(0.0));
        let payable = gross_severance.min(available);
        let unpaid = gross_severance - payable;

        // Pay what's available
        if payable > 0.0 {
            if let Some(ba) = &mut company.brokerage_account {
                ba.cash -= payable;
            } else {
                company.available_cash -= payable;
            }
            total_severance_to_workers += payable;
            // Phase 43: Accumulate severance debit for batch bank sync.
            if let Some(ref bank_id) = company.primary_bank_id {
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
        let remaining_cash = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(company.available_cash.max(0.0));
        let repayment = (remaining_cash * 0.30).min(company.severance_arrears);
        if repayment > 0.0 {
            if let Some(ba) = &mut company.brokerage_account {
                ba.cash -= repayment;
            } else {
                company.available_cash -= repayment;
            }
            company.severance_arrears -= repayment;
            total_severance_to_workers += repayment;
            if let Some(ref bank_id) = company.primary_bank_id {
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
    for (bank_id, total_debit) in &bank_debits {
        if let Some(bank) = companies.iter_mut().find(|c| c.id == *bank_id) {
            if let Some(ref mut bs) = bank.balance_sheet {
                bs.deposits -= total_debit;
                bs.reserves_at_central_bank -= total_debit;
                // Phase 94: No reserve clamping — negative reserves represent
                // CB Lombard borrowing. Clamping breaks A=L+E (deposits
                // debited by full amount but reserves debited by less) and
                // causes M0 FiatCreation. Bank resolution (Step 10) handles
                // insolvent banks with negative reserves.
            }
        }
    }
    // Credit severance to regional class savings proportionally to their FTE share
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
            // E.6.3: Typed map keys — distribute to rural and urban classes
            for demo in region.class_demographics.rural_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                demo.savings += total_severance_to_workers * share;
            }
            for demo in region.class_demographics.urban_classes.values_mut() {
                let share = demo.allocated_fte / total_class_fte;
                demo.savings += total_severance_to_workers * share;
            }
        }
    }

    // 2. Credit classes with their net earnings (gross - garnishment - PIT - remittances), track withheld amounts
    let mut total_pit_withheld: f64 = 0.0;
    let mut total_remittances_withheld: f64 = 0.0;
    let mut total_garnishments_withheld: f64 = 0.0;

    // Phase 94: Scale earned_wages by the actual paid ratio.
    // If companies couldn't afford full payroll, actual_paid < wage_obligation.
    // Crediting citizens with full gross_wage while only debiting actual_paid
    // creates money. Scale all earned_wages proportionally (Rule 5).
    let paid_ratio = if total_wage_obligation > 0.0 {
        (total_actual_paid / total_wage_obligation).min(1.0)
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
            let taxable_base = (post_garnishment - per_capita_contribution).max(0.0);

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
            let taxable_base = (post_garnishment - per_capita_contribution).max(0.0);

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

    // Fix 1.22: Store total PIT withheld for caller to route to Treasury
    allocation_matrix.pit_withheld = total_pit_withheld;
    // Phase 18A: Store total remittances withheld for caller to route to ForeignEntity
    allocation_matrix.remittances_withheld = total_remittances_withheld;
    // Phase 18B: Store total garnishments withheld for caller to route to Treasury
    allocation_matrix.garnishments_withheld = total_garnishments_withheld;

    // Phase 23C: Extract commuter wages and FTE for caller-side remittance.
    // Commuters pay PIT in the host region (already included in
    // `total_pit_withheld` via the synthetic class ledger above). Their net
    // wages must be remitted to their home regions by the caller — they are
    // NOT credited to local `class_demographics` (the `Commuter` variant
    // is not present in `rural_classes`/`urban_classes`).
    if let Some(commuter_ledger) = pool.class_ledgers.get(&commuter_class) {
        let gross = commuter_ledger.earned_wages;
        if gross > 0.0 {
            // PIT on commuter wages (host region keeps it).
            let pit = gross * pit_rate;
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
            None,
            &calendar,
            &config,
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
            None,
            &calendar,
            &config,
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
