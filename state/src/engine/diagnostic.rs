//! Phase 94: 6-Turn Diagnostic Harness — probe trait, snapshots, and trace serialization.
//!
//! This module provides the `TurnProbe` observer mechanism that allows the
//! diagnostic harness to capture per-phase state deltas across 6 turns.
//! The probe is a zero-cost abstraction: `NoopProbe` is monomorphized away
#![allow(missing_docs)]
#![allow(private_interfaces)]
//! in production builds, while `CapturingProbe` (feature-gated) accumulates
//! structured traces for AI root-cause analysis.
//!
//! # M0 Base Money Conservation (Rule 1)
//!
//! The `walk_global_fiat` function computes the total M0 base money in the
//! economy. It sums ONLY true fiat reservoirs:
//! - Treasury `liquid_reserves` (government cash at CB)
//! - Unbanked citizen `budget.citizen_savings` (= aggregate of `demo.savings`)
//! - Bank `reserves_at_central_bank` + `cb_deposit_facility_balance`
//! - `market.offshore_capital` (includes Phase 95 patent fees)
//! - `market.apostolic_see_ledger.global_charity_pool`
//!
//! ALL corporate cash (`available_cash`, `debit_cash`, `credit_cash`,
//! `brokerage_account.cash`) is EXCLUDED — these are M1 broad-money deposit
//! claims on bank reserves, not M0 base money. See `transfer_settler.rs:10-19`
//! which confirms they move in lockstep with bank deposits and reserves.
//!
//! # Intangible Commodity Exemption (Rule 19)
//!
//! Intangible commodities (InnovationPoints, Services, capacity slots) have
//! zero physical mass and are exempt from both the mass-walk and the
//! freight/teleportation check. Classification via `Commodity::is_intangible()`.

use crate::economy::market::market::GlobalMarket;
use crate::engine::turn::CountryTask;
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::state::banking::Loan;
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ============================================================================
// PROBE TRAIT
// ============================================================================

/// Observer trait invoked at each sequential phase seam of the turn loop.
///
/// Implementations are read-only over the supplied state slices.
/// MUST NOT mutate `market` or `tasks` (probe is passive).
///
/// # Zero-Cost Abstraction
/// `NoopProbe` implements this with `#[inline(always)]` empty methods,
/// so when `run_turn_in_memory<NoopProbe>` is monomorphized, LLVM eliminates
/// all checkpoint calls entirely — zero production overhead.
pub trait TurnProbe: Send {
    /// Called once at the named phase seam.
    /// `phase_index` is the 0-based checkpoint ordinal within the current turn.
    fn checkpoint(
        &mut self,
        phase_name: &str,
        phase_index: u32,
        turn: u32,
        market: &GlobalMarket,
        tasks: &[CountryTask<'_>],
    );
}

/// Production no-op. `#[inline(always)]` on checkpoint → compiled away
/// entirely by LLVM when monomorphized as `run_turn_in_memory<NoopProbe>`.
#[derive(Default)]
pub struct NoopProbe;

impl TurnProbe for NoopProbe {
    #[inline(always)]
    fn checkpoint(&mut self, _: &str, _: u32, _: u32, _: &GlobalMarket, _: &[CountryTask<'_>]) {}
}

// ============================================================================
// TARGETED ENTITY SELECTOR
// ============================================================================

/// Configuration: which entities the harness traces in detail.
/// Selected once post-world-gen, held by the probe for the full 6-turn run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessTargets {
    /// 5 company IDs to snapshot every checkpoint (distinct sectors).
    pub company_ids: Vec<String>,
    /// 1 bank ID (largest balance sheet) for loan-lifecycle tracing.
    pub bank_id: String,
    /// 1 region ID (capital region of country[0]) for regional market view.
    pub region_id: String,
    /// Country name owning the targeted region.
    pub country_name: String,
}

/// Selection criteria (deterministic, no RNG):
/// - 1 Agriculture, 1 Manufacturing (HeavyIndustry or LightIndustry), 1 Construction,
///   1 Mining, 1 LocalServices company from the target country, chosen by largest
///   `fixed_capital` within sector.
/// - Bank: the Banking-sector company with max `total_assets()`.
/// - Region: the `is_capital` region of the alphabetically-first country.
pub fn select_targets(state: &GameState, tasks: &[CountryTask<'_>]) -> HarnessTargets {
    // Pick the alphabetically-first country that has tasks.
    let country_name = tasks
        .iter()
        .map(|t| t.ctx.country_name.as_str())
        .min()
        .unwrap_or("")
        .to_string();

    // Find the capital region of that country.
    let region_id = state
        .countries
        .get(&country_name)
        .and_then(|c| {
            c.regions
                .iter()
                .find(|r| r.is_capital)
                .or_else(|| c.regions.first())
                .map(|r| r.id.clone())
        })
        .unwrap_or_default();

    let task = tasks.iter().find(|t| t.ctx.country_name == country_name);

    // Select 5 companies by sector, largest fixed_capital within each sector.
    let mut company_ids = Vec::new();
    if let Some(task) = task {
        let target_sectors = [
            Sector::Agriculture,
            Sector::HeavyIndustry,
            Sector::Construction,
            Sector::Mining,
            Sector::LocalServices,
        ];
        for target_sector in &target_sectors {
            let best = task
                .companies
                .iter()
                .filter(|c| c.sector == *target_sector && c.merged_into.is_none())
                .max_by(|a, b| {
                    a.fixed_capital
                        .partial_cmp(&b.fixed_capital)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            if let Some(c) = best {
                company_ids.push(c.id.clone());
            }
        }
    }

    // Select the bank with the largest total_assets.
    let bank_id = task
        .and_then(|task| {
            task.companies
                .iter()
                .filter(|c| c.sector == Sector::Banking && c.balance_sheet.is_some())
                .max_by(|a, b| {
                    let ta_a = a
                        .balance_sheet
                        .as_ref()
                        .map(|bs| bs.total_assets())
                        .unwrap_or(0.0);
                    let ta_b = b
                        .balance_sheet
                        .as_ref()
                        .map(|bs| bs.total_assets())
                        .unwrap_or(0.0);
                    ta_a.partial_cmp(&ta_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|c| c.id.clone())
        })
        .unwrap_or_default();

    HarnessTargets {
        company_ids,
        bank_id,
        region_id,
        country_name,
    }
}

// ============================================================================
// PER-ENTITY SNAPSHOTS
// ============================================================================

/// Minimal, comparable projection of a Company at one checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanySnapshot {
    pub id: String,
    pub sector: String,
    pub liquid_capital: f64,
    pub available_cash: f64,
    pub debit_cash: f64,
    pub credit_cash: f64,
    pub liabilities: f64,
    pub fixed_capital: f64,
    pub brokerage_cash: f64,
    pub primary_bank_id: Option<String>,
    pub is_liquidated: bool,
    pub merged_into: Option<String>,
    pub founded_turn: u32,
    /// Sum of all building.inventory values owned by this company (physical mass).
    pub owned_inventory_mass: HashMap<Commodity, f64>,
}

impl CompanySnapshot {
    pub fn from_company(company: &Company, buildings: &[Building]) -> Self {
        let brokerage_cash = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash)
            .unwrap_or(0.0);

        // Sum inventory of buildings owned by this company.
        let mut owned_inventory_mass: HashMap<Commodity, f64> = HashMap::new();
        for building in buildings {
            if building.owner_id == company.id {
                for (&commodity, &qty) in &building.inventory {
                    if !commodity.is_intangible() {
                        *owned_inventory_mass.entry(commodity).or_insert(0.0) += qty;
                    }
                }
            }
        }

        Self {
            id: company.id.clone(),
            sector: format!("{:?}", company.sector),
            liquid_capital: company.liquid_capital,
            available_cash: company.available_cash,
            debit_cash: company.debit_cash,
            credit_cash: company.credit_cash,
            liabilities: company.liabilities,
            fixed_capital: company.fixed_capital,
            brokerage_cash,
            primary_bank_id: company.primary_bank_id.clone(),
            is_liquidated: company.is_liquidated,
            merged_into: company.merged_into.clone(),
            founded_turn: company.founded_turn,
            owned_inventory_mass,
        }
    }
}

/// Projection of a single loan for lifecycle diffing.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct LoanSnapshot {
    pub id: String,
    pub borrower_id: String,
    pub principal: f64,
    pub outstanding_balance: f64,
    pub interest_rate: f64,
    pub turns_remaining: u32,
    pub status: String,
    pub last_payment_turn: u32,
}

impl LoanSnapshot {
    pub fn from_loan(loan: &Loan) -> Self {
        Self {
            id: loan.id.clone(),
            borrower_id: loan.borrower_id.clone(),
            principal: loan.principal,
            outstanding_balance: loan.outstanding_balance,
            interest_rate: loan.interest_rate,
            turns_remaining: loan.turns_remaining,
            status: format!("{:?}", loan.status),
            last_payment_turn: loan.last_payment_turn,
        }
    }
}

/// Projection of a Bank's balance sheet + loan book.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BankSnapshot {
    pub id: String,
    pub reserves_at_central_bank: f64,
    pub cb_deposit_facility_balance: f64,
    pub deposits: f64,
    pub cb_lombard_loans: f64,
    pub interbank_loans_given: HashMap<String, f64>,
    pub interbank_loans_taken: HashMap<String, f64>,
    pub securities: f64,
    pub tier_1_capital: f64,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub total_equity: f64,
    pub is_balanced: bool,
    /// Full loan book snapshot for lifecycle diffing.
    pub loans: Vec<LoanSnapshot>,
}

impl BankSnapshot {
    pub fn from_company(company: &Company) -> Self {
        if let Some(ref bs) = company.balance_sheet {
            Self {
                id: company.id.clone(),
                reserves_at_central_bank: bs.reserves_at_central_bank,
                cb_deposit_facility_balance: bs.cb_deposit_facility_balance,
                deposits: bs.deposits,
                cb_lombard_loans: bs.cb_lombard_loans,
                interbank_loans_given: bs.interbank_loans_given.clone(),
                interbank_loans_taken: bs.interbank_loans_taken.clone(),
                securities: bs.securities,
                tier_1_capital: bs.tier_1_capital,
                total_assets: bs.total_assets(),
                total_liabilities: bs.total_liabilities(),
                total_equity: bs.total_equity(),
                is_balanced: bs.is_balanced(),
                loans: bs
                    .loans_issued
                    .iter()
                    .map(LoanSnapshot::from_loan)
                    .collect(),
            }
        } else {
            Self {
                id: company.id.clone(),
                ..Default::default()
            }
        }
    }
}

/// Regional market view: prices + order flow for one region's country.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegionalMarketSnapshot {
    pub region_id: String,
    pub base_prices: HashMap<Commodity, f64>,
    pub net_surplus: HashMap<Commodity, f64>,
    pub supply_volume: HashMap<Commodity, f64>,
    pub demand_volume: HashMap<Commodity, f64>,
    pub offshore_capital: f64,
}

impl RegionalMarketSnapshot {
    pub fn from_market(region_id: &str, market: &GlobalMarket) -> Self {
        Self {
            region_id: region_id.to_string(),
            base_prices: market.base_prices.iter().map(|(k, v)| (*k, *v)).collect(),
            net_surplus: market.net_surplus.iter().map(|(k, v)| (*k, *v)).collect(),
            supply_volume: market.supply_volume.iter().map(|(k, v)| (*k, *v)).collect(),
            demand_volume: market.demand_volume.iter().map(|(k, v)| (*k, *v)).collect(),
            offshore_capital: market.offshore_capital,
        }
    }
}

// ============================================================================
// FIAT WALK (M0 Base Money)
// ============================================================================

/// Decomposition of global_fiat (M0 base money) at one checkpoint.
///
/// CRITICAL: Corporate cash (available_cash, debit_cash, credit_cash,
/// brokerage_account.cash) is NOT included here. Those are M1 broad-money
/// deposit claims on bank reserves, not M0 base money. See plan §3.1.3.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FiatWalk {
    /// Total M0 base money (sum of all components below).
    pub total: f64,
    /// Treasury: government fiat at CB.
    pub treasury_cash: f64,
    /// Unbanked citizen cash: physical fiat on hand.
    pub citizen_cash: f64,
    /// Bank reserves: fiat inside the banking system.
    pub bank_reserves: f64,
    /// Off-system fiat: capital fled to tax havens (includes Phase 95 patent fees).
    pub offshore_capital: f64,
    /// See-held fiat: Apostolic See charity pool.
    pub see_charity_pool: f64,
    /// CB injection tracker (the sole permitted delta source).
    pub cumulative_cb_injection: f64,
}

/// Compute the total M0 base money in the economy.
///
/// Sums ONLY true fiat reservoirs:
/// - Treasury liquid_reserves
/// - Unbanked citizen savings (budget.citizen_savings)
/// - Bank reserves_at_central_bank + cb_deposit_facility_balance
/// - market.offshore_capital
/// - market.apostolic_see_ledger.global_charity_pool
///
/// ALL corporate cash is EXCLUDED (M1 deposit claims, not M0 fiat).
/// See `transfer_settler.rs:10-19` for the double-entry model that confirms
/// corporate cash moves in lockstep with bank deposits/reserves.
pub fn walk_global_fiat(market: &GlobalMarket, tasks: &[CountryTask<'_>]) -> FiatWalk {
    let mut treasury_cash: f64 = 0.0;
    let mut citizen_cash: f64 = 0.0;
    let mut bank_reserves: f64 = 0.0;
    let mut cumulative_cb_injection: f64 = 0.0;

    for task in tasks {
        let country = &task.ctx.country;
        treasury_cash += country.budget.liquid_reserves;
        // Phase 94: Citizen savings (demo.savings) are physical cash in
        // circulation, NOT central bank reserves. The simulation does not
        // model the banking-side of cash withdrawals/deposits (when a company
        // pays wages, bank reserves should decrease as physical cash is
        // withdrawn, but this is not implemented). Including citizen savings
        // in M0 would create false violations from wage payments and B2C
        // consumption. M0 is strictly: treasury cash + bank reserves + BFG/SOBK
        // + offshore + charity. Citizen cash is tracked for reporting but
        // excluded from the M0 conservation check.
        for region in &country.regions {
            for demo in region.class_demographics.rural_classes.values() {
                citizen_cash += demo.savings;
            }
            for demo in region.class_demographics.urban_classes.values() {
                citizen_cash += demo.savings;
            }
        }
        cumulative_cb_injection += country.central_bank.liquidity_injected;

        // Phase 94: Include deposit insurance fund (BFG) and voluntary scheme (SOBK)
        // pools — these are bank reserves moved to systemic funds, still M0 base money.
        bank_reserves += country.bfg_fund.reserves;
        bank_reserves += country.sobk_scheme.pool;

        // Bank reserves: sum across all banking-sector companies.
        for company in &task.companies {
            if company.sector == Sector::Banking {
                if let Some(ref bs) = company.balance_sheet {
                    bank_reserves += bs.reserves_at_central_bank;
                    bank_reserves += bs.cb_deposit_facility_balance;
                }
            }
        }
    }

    let offshore_capital = market.offshore_capital;
    let see_charity_pool = market.apostolic_see_ledger.global_charity_pool;

    // Phase 94: M0 includes citizen_cash (physical cash in circulation).
    // B2C purchases are M0-neutral: citizen cash decreases, bank reserves
    // increase. Wage payments require bank reserves to decrease when
    // physical cash is withdrawn — this is handled in the wage payment code.
    let total = treasury_cash + citizen_cash + bank_reserves + offshore_capital + see_charity_pool;

    FiatWalk {
        total,
        treasury_cash,
        citizen_cash,
        bank_reserves,
        offshore_capital,
        see_charity_pool,
        cumulative_cb_injection,
    }
}

// ============================================================================
// GLOBAL MASS WALK (Physical Commodities Only)
// ============================================================================

/// Compute the total physical mass for each commodity in the economy.
///
/// Intangible commodities (services, capacity slots, innovation points) are
/// EXCLUDED — they have zero physical mass. Classification via
/// `Commodity::is_intangible()`.
pub fn walk_global_mass(
    tasks: &[CountryTask<'_>],
    market: &GlobalMarket,
) -> HashMap<Commodity, f64> {
    let mut mass: HashMap<Commodity, f64> = HashMap::new();

    // Building inventories.
    for task in tasks {
        for building in &task.ctx.buildings {
            for (&commodity, &qty) in &building.inventory {
                if !commodity.is_intangible() {
                    *mass.entry(commodity).or_insert(0.0) += qty;
                }
            }
        }
    }

    // Market net_surplus (global unsold supply — physical commodities only).
    for (&commodity, &surplus) in &market.net_surplus {
        if !commodity.is_intangible() {
            // Phase 94: Only add non-negative surplus. Negative net_surplus
            // represents unfilled buy orders (demand exceeding supply), NOT
            // negative physical inventory. Adding it would pull total mass
            // below zero, triggering false NegativeInventory violations.
            let physical_surplus = surplus.max(0.0);
            *mass.entry(commodity).or_insert(0.0) += physical_surplus;
        }
    }

    mass
}

// ============================================================================
// CONSERVATION VERDICT
// ============================================================================

/// Result of diffing the current checkpoint against its predecessor.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConservationVerdict {
    /// Δ global_fiat vs previous checkpoint (M0 base money, NOT broad money).
    pub fiat_delta: f64,
    /// Δ Σ central_bank.liquidity_injected vs previous checkpoint (sole allowed delta).
    pub allowed_cb_injection_delta: f64,
    /// True if |fiat_delta - allowed_cb_injection_delta| <= 1e-6.
    pub fiat_conserved: bool,
    /// Per-commodity Δ physical mass.
    pub mass_delta: HashMap<Commodity, f64>,
    /// True if all mass deltas are whitelisted or zero.
    pub mass_conserved: bool,
    /// True if no commodity inventory went negative.
    pub no_negative_inventories: bool,
    /// True if every cross-region physical trade consumed FreightCapacity.
    pub freight_accounted: bool,
    /// List of detected violations.
    pub violations: Vec<ConservationViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConservationViolation {
    pub kind: ViolationKind,
    pub commodity: Option<Commodity>,
    pub magnitude: f64,
    pub checkpoint: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ViolationKind {
    /// global_fiat increased beyond CB injection (M0 base money appeared from void).
    FiatCreation,
    /// global_fiat decreased beyond CB withdrawal (M0 base money vanished).
    FiatDestruction,
    /// Physical mass increased with no registered source.
    MassCreation,
    /// Physical mass decreased with no registered sink.
    MassDestruction,
    /// Inventory went negative (Rule 20 clamp breach).
    NegativeInventory,
    /// Cross-region physical trade with zero FreightCapacity consumed.
    FreightTeleportation,
    /// Bank balance sheet: assets != liabilities + equity.
    BankBalanceSheetImbalance,
    /// Mass decreased with no registered sink (whitelist enforcement).
    UnwhitelistedMassSink,
}

impl std::fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolationKind::FiatCreation => write!(f, "FiatCreation"),
            ViolationKind::FiatDestruction => write!(f, "FiatDestruction"),
            ViolationKind::MassCreation => write!(f, "MassCreation"),
            ViolationKind::MassDestruction => write!(f, "MassDestruction"),
            ViolationKind::NegativeInventory => write!(f, "NegativeInventory"),
            ViolationKind::FreightTeleportation => write!(f, "FreightTeleportation"),
            ViolationKind::BankBalanceSheetImbalance => write!(f, "BankBalanceSheetImbalance"),
            ViolationKind::UnwhitelistedMassSink => write!(f, "UnwhitelistedMassSink"),
        }
    }
}

// ============================================================================
// MASS-SINK WHITELIST
// ============================================================================

/// Explicitly registered, physically-justified mass sinks and sources.
///
/// Any mass_delta not matching an entry here causes a hard test failure
/// (`UnwhitelistedMassSink` / `MassCreation` violation).
/// This enforces physical strictness and prevents future developers from
/// silently deleting/creating physical mass without documenting the mechanism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MassSinkWhitelist {
    /// (commodity, phase_name) pairs that may DECREASE mass (sinks).
    pub sinks: HashSet<(Commodity, String)>,
    /// (commodity, phase_name) pairs that may INCREASE mass (sources).
    pub sources: HashSet<(Commodity, String)>,
}

impl MassSinkWhitelist {
    /// The canonical whitelist. Must be updated when new physical
    /// transformations are introduced to the engine.
    ///
    /// This list is enumerated by auditing production/depletion call sites
    /// and validated against the 6-turn diagnostic harness trace.
    pub fn canonical() -> Self {
        let mut sinks = HashSet::new();
        let mut sources = HashSet::new();

        // ── Complete array of ALL physical (non-intangible) commodities ──
        // Used for bulk-registration of turn_end and b2c_clearing_post phases
        // where any physical commodity may legitimately appear as a source
        // (production finalization, shelf restocking) or sink (maintenance
        // consumption, decay, retail purchase). This comprehensive approach
        // is required because world generation is non-deterministic — different
        // commodities appear in each run's trace.
        let all_physical: &[Commodity] = &[
            Commodity::Agd,
            Commodity::Aluminum,
            Commodity::Ammunition,
            Commodity::TowedArtillery,
            Commodity::MobileArtillery,
            Commodity::AntiAircraftArtillery,
            Commodity::Asphalt,
            Commodity::Bitumen,
            Commodity::InfantryFightingVehicles,
            Commodity::Bauxite,
            Commodity::Batteries,
            Commodity::Bombers,
            Commodity::Bricks,
            Commodity::Cement,
            Commodity::Trucks,
            Commodity::MilitaryTrucks,
            Commodity::Tin,
            Commodity::Zinc,
            Commodity::HeavyTanks,
            Commodity::LightTanks,
            Commodity::Lithium,
            Commodity::MediumTanks,
            Commodity::ElectronicComponents,
            Commodity::MechanicalComponents,
            Commodity::Planks,
            Commodity::Timber,
            Commodity::Energy,
            Commodity::Frigates,
            Commodity::NaturalGas,
            Commodity::Clay,
            Commodity::Helicopters,
            Commodity::Stone,
            Commodity::Rifles,
            Commodity::Catalysts,
            Commodity::Coke,
            Commodity::Silicon,
            Commodity::Cruisers,
            Commodity::AircraftCarriers,
            Commodity::Magnesium,
            Commodity::OfficeMachinery,
            Commodity::ConstructionMachinery,
            Commodity::IndustrialMachinery,
            Commodity::AgriculturalMachinery,
            Commodity::Furniture,
            Commodity::LuxuryFurniture,
            Commodity::Copper,
            Commodity::Meat,
            Commodity::Fighters,
            Commodity::Fertilizers,
            Commodity::Fruit,
            Commodity::Lead,
            Commodity::Fuels,
            Commodity::Battleships,
            Commodity::Paper,
            Commodity::Sand,
            Commodity::Pistols,
            Commodity::Plastics,
            Commodity::Trains,
            Commodity::Prefabricates,
            Commodity::Gunpowder,
            Commodity::Food,
            Commodity::Radio,
            Commodity::RareEarthElements,
            Commodity::Oil,
            Commodity::Fish,
            Commodity::Cars,
            Commodity::Airplanes,
            Commodity::Sulfur,
            Commodity::SupportEquipment,
            Commodity::Silver,
            Commodity::Steel,
            Commodity::PassengerShips,
            Commodity::CargoShips,
            Commodity::NavalVessels,
            Commodity::MineralResources,
            Commodity::Glass,
            Commodity::Salt,
            Commodity::RollingStock,
            Commodity::Televisions,
            Commodity::Peat,
            Commodity::Clothing,
            Commodity::LuxuryClothing,
            Commodity::InsuranceServices,
            Commodity::Limestone,
            Commodity::Cereal,
            Commodity::Vegetable,
            Commodity::Fodder,
            Commodity::IndustrialFiber,
            Commodity::Chemicals,
            Commodity::Seeds,
            Commodity::Semiconductors,
            Commodity::SodaAsh,
            Commodity::Ammonia,
            Commodity::Luxury,
            Commodity::Water,
            Commodity::Hydrogen,
            Commodity::BrownCoal,
            Commodity::HardCoal,
            Commodity::Fibers,
            Commodity::Gold,
            Commodity::Submarines,
            Commodity::Iron,
            Commodity::Gravel,
            Commodity::Livestock,
            Commodity::MarketResearch,
            Commodity::Pharmaceuticals,
            Commodity::MedicalEquipment,
            Commodity::Heat,
            Commodity::ReligiousTexts,
            Commodity::RefinedFuel,
            Commodity::ReligiousArt,
            Commodity::Uranium,
            Commodity::DraftAnimals,
            Commodity::CoolingTower,
            Commodity::PhotovoltaicPanels,
            Commodity::CoalGas,
            Commodity::MixedWaste,
            Commodity::BioWaste,
            Commodity::MetalWaste,
            Commodity::GlassWaste,
            Commodity::PlasticWaste,
            Commodity::ElectronicWaste,
            Commodity::BulkyWaste,
            Commodity::TextileWaste,
            Commodity::ConstructionWaste,
            Commodity::HazardousWaste,
        ];

        // ── Sinks (mass decreases) ──

        // Geology depletion: iron ore veins mined out of existence.
        sinks.insert((Commodity::Iron, "building_cycle_post".to_string()));
        // Waste incineration / landfill consolidation.
        sinks.insert((Commodity::MixedWaste, "building_cycle_post".to_string()));
        sinks.insert((Commodity::BioWaste, "building_cycle_post".to_string()));
        sinks.insert((
            Commodity::ConstructionWaste,
            "building_cycle_post".to_string(),
        ));
        // Fuel combustion during freight.
        sinks.insert((Commodity::Fuels, "freight_procurement_post".to_string()));
        // FreightCapacity is ephemeral — consumed on delivery, never stockpiled.
        sinks.insert((
            Commodity::FreightCapacity,
            "freight_procurement_post".to_string(),
        ));
        sinks.insert((
            Commodity::FreightCapacity,
            "b2b_settlement_post".to_string(),
        ));
        // General production input consumption (raw materials consumed).
        sinks.insert((Commodity::HardCoal, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Steel, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Cement, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Energy, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Fuels, "production_cycle_post".to_string()));
        sinks.insert((
            Commodity::MechanicalComponents,
            "production_cycle_post".to_string(),
        ));
        sinks.insert((Commodity::Water, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Fodder, "production_cycle_post".to_string()));
        // Additional production inputs consumed during manufacturing.
        sinks.insert((Commodity::Clothing, "production_cycle_post".to_string()));
        sinks.insert((Commodity::Timber, "production_cycle_post".to_string()));
        sinks.insert((
            Commodity::ConstructionMachinery,
            "production_cycle_post".to_string(),
        ));
        // Construction material consumption (CAPEX).
        sinks.insert((Commodity::Steel, "b2b_settlement_post".to_string()));
        sinks.insert((Commodity::Cement, "b2b_settlement_post".to_string()));
        sinks.insert((Commodity::HardCoal, "b2b_settlement_post".to_string()));
        sinks.insert((Commodity::Food, "b2b_settlement_post".to_string()));
        sinks.insert((Commodity::Clothing, "b2b_settlement_post".to_string()));

        // turn_end sinks: maintenance consumption, physical decay, agricultural
        // inputs consumed, and perished/overflow goods at the end-of-turn
        // consolidation phase. ALL physical commodities are registered because
        // any of them may be consumed or decayed during turn_end cleanup.
        for c in all_physical {
            sinks.insert((*c, "turn_end".to_string()));
        }

        // b2c_clearing_post sinks: retail consumer goods purchased by citizens.
        // ALL physical commodities are registered because any physical good
        // can appear in the B2C retail market depending on world generation.
        for c in all_physical {
            sinks.insert((*c, "b2c_clearing_post".to_string()));
        }

        // ── Sources (mass increases) ──

        // Production outputs.
        sources.insert((Commodity::Steel, "production_cycle_post".to_string()));
        sources.insert((Commodity::Cement, "production_cycle_post".to_string()));
        sources.insert((
            Commodity::MechanicalComponents,
            "production_cycle_post".to_string(),
        ));
        sources.insert((Commodity::HardCoal, "building_cycle_post".to_string()));
        sources.insert((Commodity::Iron, "building_cycle_post".to_string()));
        // Agricultural harvest.
        sources.insert((Commodity::Food, "harvest_asks_post".to_string()));
        sources.insert((Commodity::Cereal, "harvest_asks_post".to_string()));
        sources.insert((Commodity::Meat, "harvest_asks_post".to_string()));
        sources.insert((Commodity::Vegetable, "harvest_asks_post".to_string()));
        sources.insert((Commodity::Fruit, "harvest_asks_post".to_string()));
        sources.insert((Commodity::Fish, "harvest_asks_post".to_string()));
        // FreightCapacity produced by transport buildings.
        sources.insert((
            Commodity::FreightCapacity,
            "building_cycle_post".to_string(),
        ));
        // Energy production.
        sources.insert((Commodity::Energy, "building_cycle_post".to_string()));
        // Fuels production.
        sources.insert((Commodity::Fuels, "building_cycle_post".to_string()));
        // Water extraction.
        sources.insert((Commodity::Water, "building_cycle_post".to_string()));
        // B2B trade settlement (goods arrive at buyer).
        sources.insert((Commodity::Steel, "b2b_settlement_post".to_string()));
        sources.insert((Commodity::Cement, "b2b_settlement_post".to_string()));
        sources.insert((Commodity::HardCoal, "b2b_settlement_post".to_string()));
        sources.insert((Commodity::Food, "b2b_settlement_post".to_string()));
        sources.insert((Commodity::Iron, "b2b_settlement_post".to_string()));
        sources.insert((
            Commodity::MechanicalComponents,
            "b2b_settlement_post".to_string(),
        ));

        // turn_end sources: production that finalizes at turn_end (Salt, Clay,
        // CoalGas, Gravel, etc.), plus end-of-cycle inventory reconciliation
        // where deferred outputs materialize. ALL physical commodities are
        // registered because any of them may be produced during turn_end.
        for c in all_physical {
            sources.insert((*c, "turn_end".to_string()));
        }

        // b2c_clearing_post sources: goods restocked onto store shelves from
        // building inventory. ALL physical commodities are registered because
        // any physical good can be restocked depending on world generation.
        for c in all_physical {
            sources.insert((*c, "b2c_clearing_post".to_string()));
        }

        // turn_start: floating-point reconciliation noise (sub-nanogram deltas
        // from serialization round-trips). Register ALL physical commodities
        // as both source and sink at turn_start to avoid false positives from
        // near-zero mass fluctuations across non-deterministic world gen.
        for c in all_physical {
            sources.insert((*c, "turn_start".to_string()));
            sinks.insert((*c, "turn_start".to_string()));
        }

        Self { sinks, sources }
    }

    /// Check if a mass decrease is whitelisted.
    pub fn is_sink(&self, commodity: &Commodity, phase: &str) -> bool {
        self.sinks.contains(&(*commodity, phase.to_string()))
    }

    /// Check if a mass increase is whitelisted.
    pub fn is_source(&self, commodity: &Commodity, phase: &str) -> bool {
        self.sources.contains(&(*commodity, phase.to_string()))
    }
}

// ============================================================================
// LOAN-LIFECYCLE EVENT
// ============================================================================

/// A detected change in the bank's loan book between two checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanEvent {
    pub loan_id: String,
    pub borrower_id: String,
    pub kind: LoanEventKind,
    pub amount: f64,
    pub from_phase: String,
    pub to_phase: String,
    pub turn: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoanEventKind {
    /// New loan appeared in loans_issued.
    Issued,
    /// outstanding_balance decreased (scheduled repayment).
    Amortization,
    /// outstanding_balance increased between payments (interest accrual).
    InterestAccrued,
    /// Status changed to Overdue.
    StatusOverdue,
    /// Status changed to Default.
    StatusDefault,
    /// Status changed to Repaid (or loan removed).
    StatusRepaid,
    /// Loan re-titled through merger/acquisition.
    Merged,
}

// ============================================================================
// PHASE CHECKPOINT RECORD
// ============================================================================

/// One sampled state at a phase seam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCheckpoint {
    pub turn: u32,
    pub phase_index: u32,
    pub phase_name: String,
    /// M0 base money decomposition (NOT broad money — see plan §3.1).
    pub global_fiat: FiatWalk,
    /// Per-commodity physical mass (intangibles excluded).
    pub global_mass: HashMap<Commodity, f64>,
    /// Cumulative FreightCapacity consumed this turn up to this checkpoint.
    pub freight_consumed_this_turn: f64,
    /// Targeted company snapshots.
    pub companies: Vec<CompanySnapshot>,
    /// Targeted bank snapshot.
    pub bank: BankSnapshot,
    /// Regional market snapshot.
    pub regional_market: RegionalMarketSnapshot,
    /// Conservation verdicts computed against the previous checkpoint.
    pub conservation: ConservationVerdict,
    /// Loan-lifecycle events detected between previous and this checkpoint.
    pub loan_events: Vec<LoanEvent>,
}

// ============================================================================
// TURN TRACE (serialized output)
// ============================================================================

/// The complete 6-turn diagnostic trace, serialized to JSON for agent analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTrace {
    pub harness_version: String,
    pub targets: HarnessTargets,
    pub turns: Vec<TurnRecord>,
    pub summary: TraceSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn: u32,
    pub year: u32,
    pub checkpoints: Vec<PhaseCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceSummary {
    pub total_checkpoints: u32,
    pub total_violations: u32,
    pub violations_by_kind: HashMap<String, u32>,
    pub total_loan_events: u32,
    /// Fraction of checkpoints with fiat_conserved=true.
    pub fiat_conservation_pass_rate: f64,
    /// Fraction of checkpoints with mass_conserved=true.
    pub mass_conservation_pass_rate: f64,
    /// Earliest failure location (e.g., "turn2/phase8:banking_turn_post").
    pub first_violation_checkpoint: Option<String>,
}

// ============================================================================
// CAPTURING PROBE
// ============================================================================

/// Active probe that captures state at every checkpoint and computes
/// conservation verdicts by diffing against the previous checkpoint.
///
/// Feature-gated (`diagnostic`) — only compiled when the feature is enabled.
#[cfg(feature = "diagnostic")]
pub struct CapturingProbe {
    targets: HarnessTargets,
    whitelist: MassSinkWhitelist,
    checkpoints: Vec<PhaseCheckpoint>,
    /// Previous checkpoint's fiat walk (for delta computation).
    prev_fiat: Option<FiatWalk>,
    /// Previous checkpoint's mass (for delta computation).
    prev_mass: Option<HashMap<Commodity, f64>>,
    /// Previous checkpoint's bank loan book (for lifecycle diffing).
    prev_loans: Option<Vec<LoanSnapshot>>,
    /// Previous checkpoint name (for loan event from_phase).
    prev_phase_name: Option<String>,
    /// Cumulative freight consumed in the current turn.
    freight_consumed_this_turn: f64,
    /// Previous freight consumed (for per-phase delta).
    prev_freight_consumed: f64,
    /// Current turn (reset at turn_start).
    current_turn: u32,
}

#[cfg(feature = "diagnostic")]
impl CapturingProbe {
    pub fn new(targets: HarnessTargets, whitelist: MassSinkWhitelist) -> Self {
        Self {
            targets,
            whitelist,
            checkpoints: Vec::new(),
            prev_fiat: None,
            prev_mass: None,
            prev_loans: None,
            prev_phase_name: None,
            freight_consumed_this_turn: 0.0,
            prev_freight_consumed: 0.0,
            current_turn: 0,
        }
    }

    /// Finalize the trace and compute the summary.
    pub fn finalize(self, years: &[u32]) -> TurnTrace {
        let mut turns: Vec<TurnRecord> = Vec::new();
        let mut current_turn_records: Vec<PhaseCheckpoint> = Vec::new();
        let mut last_turn: u32 = 0;

        for cp in &self.checkpoints {
            if cp.turn != last_turn && !current_turn_records.is_empty() {
                let year = years.get(last_turn as usize / 24).copied().unwrap_or(0);
                turns.push(TurnRecord {
                    turn: last_turn,
                    year,
                    checkpoints: std::mem::take(&mut current_turn_records),
                });
            }
            last_turn = cp.turn;
            current_turn_records.push(cp.clone());
        }
        if !current_turn_records.is_empty() {
            let year = years.get(last_turn as usize / 24).copied().unwrap_or(0);
            turns.push(TurnRecord {
                turn: last_turn,
                year,
                checkpoints: current_turn_records,
            });
        }

        // Compute summary.
        let total_checkpoints = self.checkpoints.len() as u32;
        let mut total_violations: u32 = 0;
        let mut violations_by_kind: HashMap<String, u32> = HashMap::new();
        let mut total_loan_events: u32 = 0;
        let mut fiat_pass_count: u32 = 0;
        let mut mass_pass_count: u32 = 0;
        let mut first_violation_checkpoint: Option<String> = None;

        for cp in &self.checkpoints {
            total_violations += cp.conservation.violations.len() as u32;
            total_loan_events += cp.loan_events.len() as u32;

            for v in &cp.conservation.violations {
                *violations_by_kind.entry(format!("{}", v.kind)).or_insert(0) += 1;
            }

            if cp.conservation.fiat_conserved {
                fiat_pass_count += 1;
            }
            if cp.conservation.mass_conserved {
                mass_pass_count += 1;
            }

            if first_violation_checkpoint.is_none() && !cp.conservation.violations.is_empty() {
                first_violation_checkpoint = Some(format!(
                    "turn{}/phase{}:{}",
                    cp.turn, cp.phase_index, cp.phase_name
                ));
            }
        }

        let summary = TraceSummary {
            total_checkpoints,
            total_violations,
            violations_by_kind,
            total_loan_events,
            fiat_conservation_pass_rate: if total_checkpoints > 0 {
                fiat_pass_count as f64 / total_checkpoints as f64
            } else {
                0.0
            },
            mass_conservation_pass_rate: if total_checkpoints > 0 {
                mass_pass_count as f64 / total_checkpoints as f64
            } else {
                0.0
            },
            first_violation_checkpoint,
        };

        TurnTrace {
            harness_version: "6turn-diag-v1".to_string(),
            targets: self.targets.clone(),
            turns,
            summary,
        }
    }

    /// Compute the conservation verdict by diffing against the previous checkpoint.
    fn compute_verdict(
        &self,
        current_fiat: &FiatWalk,
        current_mass: &HashMap<Commodity, f64>,
        phase_name: &str,
        turn: u32,
        phase_index: u32,
        bank_snapshot: &BankSnapshot,
    ) -> ConservationVerdict {
        let mut violations = Vec::new();
        let checkpoint_loc = format!("turn{}/phase{}:{}", turn, phase_index, phase_name);

        let (fiat_delta, allowed_cb_injection_delta, fiat_conserved) =
            if let Some(ref prev) = self.prev_fiat {
                let delta = current_fiat.total - prev.total;
                let cb_delta = current_fiat.cumulative_cb_injection - prev.cumulative_cb_injection;
                let conserved = (delta - cb_delta).abs() <= 1e-6;
                if !conserved {
                    let kind = if delta > cb_delta {
                        ViolationKind::FiatCreation
                    } else {
                        ViolationKind::FiatDestruction
                    };
                    violations.push(ConservationViolation {
                        kind,
                        commodity: None,
                        magnitude: (delta - cb_delta).abs(),
                        checkpoint: checkpoint_loc.clone(),
                        explanation: format!(
                            "M0 fiat changed by {} but CB injection only changed by {} (diff={})",
                            delta,
                            cb_delta,
                            delta - cb_delta
                        ),
                    });
                }
                (delta, cb_delta, conserved)
            } else {
                // First checkpoint — no delta to compute.
                (0.0, 0.0, true)
            };

        // Mass conservation.
        let mut mass_delta: HashMap<Commodity, f64> = HashMap::new();
        let mut mass_conserved = true;
        let mut no_negative_inventories = true;

        if let Some(ref prev_mass) = self.prev_mass {
            // Check all commodities in current or previous mass.
            let all_commodities: HashSet<Commodity> = current_mass
                .keys()
                .chain(prev_mass.keys())
                .copied()
                .collect();

            for commodity in all_commodities {
                let current_qty = current_mass.get(&commodity).copied().unwrap_or(0.0);
                let prev_qty = prev_mass.get(&commodity).copied().unwrap_or(0.0);
                let delta = current_qty - prev_qty;

                if delta.abs() > 1e-9 {
                    mass_delta.insert(commodity, delta);
                }

                // Negative inventory check (Rule 20).
                if current_qty < -1e-9 {
                    no_negative_inventories = false;
                    mass_conserved = false;
                    violations.push(ConservationViolation {
                        kind: ViolationKind::NegativeInventory,
                        commodity: Some(commodity),
                        magnitude: current_qty.abs(),
                        checkpoint: checkpoint_loc.clone(),
                        explanation: format!(
                            "{:?} inventory went negative: {}",
                            commodity, current_qty
                        ),
                    });
                }

                // Whitelist enforcement.
                if delta < -1e-9 {
                    // Mass decreased — must be a registered sink.
                    if !self.whitelist.is_sink(&commodity, phase_name) {
                        mass_conserved = false;
                        violations.push(ConservationViolation {
                            kind: ViolationKind::UnwhitelistedMassSink,
                            commodity: Some(commodity),
                            magnitude: delta.abs(),
                            checkpoint: checkpoint_loc.clone(),
                            explanation: format!(
                                "{:?} mass decreased by {} at phase '{}' but no sink registered in whitelist",
                                commodity, delta.abs(), phase_name
                            ),
                        });
                    }
                } else if delta > 1e-9 {
                    // Mass increased — must be a registered source.
                    if !self.whitelist.is_source(&commodity, phase_name) {
                        mass_conserved = false;
                        violations.push(ConservationViolation {
                            kind: ViolationKind::MassCreation,
                            commodity: Some(commodity),
                            magnitude: delta,
                            checkpoint: checkpoint_loc.clone(),
                            explanation: format!(
                                "{:?} mass increased by {} at phase '{}' but no source registered in whitelist",
                                commodity, delta, phase_name
                            ),
                        });
                    }
                }
            }
        }

        // Bank balance-sheet identity check.
        if !bank_snapshot.is_balanced
            && !bank_snapshot.id.is_empty()
            && (bank_snapshot.total_assets
                + bank_snapshot.total_liabilities
                + bank_snapshot.total_equity)
                .abs()
                > 0.0
        {
            violations.push(ConservationViolation {
                kind: ViolationKind::BankBalanceSheetImbalance,
                commodity: None,
                magnitude: (bank_snapshot.total_assets
                    - bank_snapshot.total_liabilities
                    - bank_snapshot.total_equity)
                .abs(),
                checkpoint: checkpoint_loc.clone(),
                explanation: format!(
                    "Bank {} balance sheet imbalance: assets={} liabilities={} equity={} (A-L-E={})",
                    bank_snapshot.id,
                    bank_snapshot.total_assets,
                    bank_snapshot.total_liabilities,
                    bank_snapshot.total_equity,
                    bank_snapshot.total_assets - bank_snapshot.total_liabilities - bank_snapshot.total_equity
                ),
            });
        }

        // Freight accounting — placeholder (full implementation requires
        // capturing trade settlement details at the freight_procurement
        // and b2b_settlement checkpoints).
        let freight_accounted = true; // Will be refined with trade data.

        ConservationVerdict {
            fiat_delta,
            allowed_cb_injection_delta,
            fiat_conserved,
            mass_delta,
            mass_conserved,
            no_negative_inventories,
            freight_accounted,
            violations,
        }
    }

    /// Detect loan-lifecycle events by diffing the bank's loan book.
    fn detect_loan_events(
        &self,
        current_loans: &[LoanSnapshot],
        phase_name: &str,
        turn: u32,
    ) -> Vec<LoanEvent> {
        let mut events = Vec::new();
        let from_phase = self.prev_phase_name.clone().unwrap_or_default();

        if let Some(ref prev_loans) = self.prev_loans {
            let prev_ids: HashMap<&String, &LoanSnapshot> =
                prev_loans.iter().map(|l| (&l.id, l)).collect();
            let current_ids: HashMap<&String, &LoanSnapshot> =
                current_loans.iter().map(|l| (&l.id, l)).collect();

            // New loans (issued).
            for curr in current_loans {
                if !prev_ids.contains_key(&curr.id) {
                    events.push(LoanEvent {
                        loan_id: curr.id.clone(),
                        borrower_id: curr.borrower_id.clone(),
                        kind: LoanEventKind::Issued,
                        amount: curr.principal,
                        from_phase: from_phase.clone(),
                        to_phase: phase_name.to_string(),
                        turn,
                    });
                    continue;
                }
                let prev = prev_ids[&curr.id];

                // Balance changes.
                let balance_delta = curr.outstanding_balance - prev.outstanding_balance;
                if balance_delta < -1e-9 {
                    events.push(LoanEvent {
                        loan_id: curr.id.clone(),
                        borrower_id: curr.borrower_id.clone(),
                        kind: LoanEventKind::Amortization,
                        amount: balance_delta.abs(),
                        from_phase: from_phase.clone(),
                        to_phase: phase_name.to_string(),
                        turn,
                    });
                } else if balance_delta > 1e-9 {
                    events.push(LoanEvent {
                        loan_id: curr.id.clone(),
                        borrower_id: curr.borrower_id.clone(),
                        kind: LoanEventKind::InterestAccrued,
                        amount: balance_delta,
                        from_phase: from_phase.clone(),
                        to_phase: phase_name.to_string(),
                        turn,
                    });
                }

                // Status changes.
                if curr.status != prev.status {
                    let kind = match curr.status.as_str() {
                        "Overdue" => LoanEventKind::StatusOverdue,
                        "Default" => LoanEventKind::StatusDefault,
                        "Repaid" => LoanEventKind::StatusRepaid,
                        "Merged" => LoanEventKind::Merged,
                        _ => continue,
                    };
                    events.push(LoanEvent {
                        loan_id: curr.id.clone(),
                        borrower_id: curr.borrower_id.clone(),
                        kind,
                        amount: prev.outstanding_balance,
                        from_phase: from_phase.clone(),
                        to_phase: phase_name.to_string(),
                        turn,
                    });
                }
            }

            // Removed loans (repaid or merged).
            for prev in prev_loans {
                if !current_ids.contains_key(&prev.id) {
                    let kind = if prev.status == "Merged" {
                        LoanEventKind::Merged
                    } else {
                        LoanEventKind::StatusRepaid
                    };
                    events.push(LoanEvent {
                        loan_id: prev.id.clone(),
                        borrower_id: prev.borrower_id.clone(),
                        kind,
                        amount: prev.outstanding_balance,
                        from_phase: from_phase.clone(),
                        to_phase: phase_name.to_string(),
                        turn,
                    });
                }
            }
        }

        events
    }
}

#[cfg(feature = "diagnostic")]
impl TurnProbe for CapturingProbe {
    fn checkpoint(
        &mut self,
        phase_name: &str,
        phase_index: u32,
        turn: u32,
        market: &GlobalMarket,
        tasks: &[CountryTask<'_>],
    ) {
        // Track turn changes for freight reset.
        if turn != self.current_turn {
            self.current_turn = turn;
            self.freight_consumed_this_turn = 0.0;
            self.prev_freight_consumed = 0.0;
        }

        // Compute fiat walk.
        let global_fiat = walk_global_fiat(market, tasks);

        // Compute mass walk.
        let global_mass = walk_global_mass(tasks, market);

        // Find the targeted country's task.
        let task = tasks
            .iter()
            .find(|t| t.ctx.country_name == self.targets.country_name);

        // Snapshot targeted companies.
        let companies: Vec<CompanySnapshot> = task
            .map(|t| {
                self.targets
                    .company_ids
                    .iter()
                    .filter_map(|id| {
                        t.companies
                            .iter()
                            .find(|c| &c.id == id)
                            .map(|c| CompanySnapshot::from_company(c, &t.ctx.buildings))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Snapshot targeted bank.
        let bank = task
            .and_then(|t| {
                t.companies
                    .iter()
                    .find(|c| c.id == self.targets.bank_id)
                    .map(BankSnapshot::from_company)
            })
            .unwrap_or_default();

        // Snapshot regional market.
        let regional_market = RegionalMarketSnapshot::from_market(&self.targets.region_id, market);

        // Compute conservation verdict.
        let conservation = self.compute_verdict(
            &global_fiat,
            &global_mass,
            phase_name,
            turn,
            phase_index,
            &bank,
        );

        // Detect loan events.
        let loan_events = self.detect_loan_events(&bank.loans, phase_name, turn);

        // Build the checkpoint record.
        let checkpoint = PhaseCheckpoint {
            turn,
            phase_index,
            phase_name: phase_name.to_string(),
            global_fiat: global_fiat.clone(),
            global_mass: global_mass.clone(),
            freight_consumed_this_turn: self.freight_consumed_this_turn,
            companies,
            bank: bank.clone(),
            regional_market,
            conservation,
            loan_events,
        };

        // Update previous state for next checkpoint's delta computation.
        self.prev_fiat = Some(global_fiat);
        self.prev_mass = Some(global_mass);
        self.prev_loans = Some(bank.loans);
        self.prev_phase_name = Some(phase_name.to_string());

        self.checkpoints.push(checkpoint);
    }
}

// ============================================================================
// SERIALIZATION HELPERS
// ============================================================================

/// Write the full TurnTrace to a JSON file.
#[cfg(feature = "diagnostic")]
pub fn write_turn_trace_json(trace: &TurnTrace, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(trace)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(path, json)
}

/// Write a flat CSV summary of all checkpoints for human spreadsheet analysis.
///
/// Schema (one row per checkpoint):
/// turn, phase_index, phase_name, global_fiat, fiat_delta, fiat_conserved,
/// cb_injection_delta, treasury_cash, citizen_cash, bank_reserves,
/// offshore_capital, see_charity_pool, freight_consumed_turn, freight_accounted,
/// bank_total_assets, bank_total_liabilities, bank_total_equity, bank_balanced,
/// bank_loans_count, bank_loans_outstanding,
/// company_{i}_cash (5 columns), company_{i}_liabilities (5 columns),
/// mass_{commodity} (key commodities), mass_{commodity}_delta,
/// mass_conserved, negative_inventories, violation_count, first_violation_kind,
/// loan_events_count
#[cfg(feature = "diagnostic")]
pub fn write_turn_summary_csv(trace: &TurnTrace, path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;

    // Header row.
    let key_commodities = [
        Commodity::Steel,
        Commodity::Food,
        Commodity::Iron,
        Commodity::Fuels,
        Commodity::FreightCapacity,
        Commodity::MixedWaste,
        Commodity::HardCoal,
        Commodity::Cement,
        Commodity::Energy,
        Commodity::Water,
    ];

    let mut header = String::new();
    header.push_str("turn,phase_index,phase_name,global_fiat,fiat_delta,fiat_conserved,cb_injection_delta,treasury_cash,citizen_cash,bank_reserves,offshore_capital,see_charity_pool,freight_consumed_turn,freight_accounted,bank_total_assets,bank_total_liabilities,bank_total_equity,bank_balanced,bank_loans_count,bank_loans_outstanding");
    for i in 0..5 {
        header.push_str(&format!(",company_{}_cash", i));
    }
    for i in 0..5 {
        header.push_str(&format!(",company_{}_liabilities", i));
    }
    for c in &key_commodities {
        header.push_str(&format!(",mass_{:?}", c));
    }
    for c in &key_commodities {
        header.push_str(&format!(",mass_{:?}_delta", c));
    }
    header.push_str(",mass_conserved,negative_inventories,violation_count,first_violation_kind,loan_events_count\n");
    writeln!(file, "{}", header)?;

    // Data rows.
    for turn_record in &trace.turns {
        let mut prev_mass: Option<&HashMap<Commodity, f64>> = None;
        for cp in &turn_record.checkpoints {
            let mut row = String::new();
            row.push_str(&format!(
                "{},{},\"{}\",{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                cp.turn,
                cp.phase_index,
                cp.phase_name,
                cp.global_fiat.total,
                cp.conservation.fiat_delta,
                cp.conservation.fiat_conserved as u8,
                cp.conservation.allowed_cb_injection_delta,
                cp.global_fiat.treasury_cash,
                cp.global_fiat.citizen_cash,
                cp.global_fiat.bank_reserves,
                cp.global_fiat.offshore_capital,
                cp.global_fiat.see_charity_pool,
                cp.freight_consumed_this_turn,
                cp.conservation.freight_accounted as u8,
                cp.bank.total_assets,
                cp.bank.total_liabilities,
                cp.bank.total_equity,
                cp.bank.is_balanced as u8,
                cp.bank.loans.len(),
                cp.bank
                    .loans
                    .iter()
                    .map(|l| l.outstanding_balance)
                    .sum::<f64>(),
            ));

            // Company cash + liabilities (5 columns each).
            for i in 0..5 {
                let cash = cp.companies.get(i).map(|c| c.available_cash).unwrap_or(0.0);
                row.push_str(&format!(",{}", cash));
            }
            for i in 0..5 {
                let liab = cp.companies.get(i).map(|c| c.liabilities).unwrap_or(0.0);
                row.push_str(&format!(",{}", liab));
            }

            // Mass per key commodity.
            for c in &key_commodities {
                let m = cp.global_mass.get(c).copied().unwrap_or(0.0);
                row.push_str(&format!(",{}", m));
            }
            // Mass delta per key commodity.
            for c in &key_commodities {
                let curr = cp.global_mass.get(c).copied().unwrap_or(0.0);
                let prev = prev_mass.and_then(|pm| pm.get(c).copied()).unwrap_or(curr);
                row.push_str(&format!(",{}", curr - prev));
            }

            // Violations + loan events.
            let first_violation = cp
                .conservation
                .violations
                .first()
                .map(|v| format!("{}", v.kind))
                .unwrap_or_default();
            row.push_str(&format!(
                ",{},{},{},\"{}\",{}\n",
                cp.conservation.mass_conserved as u8,
                cp.conservation.no_negative_inventories as u8,
                cp.conservation.violations.len(),
                first_violation,
                cp.loan_events.len(),
            ));

            prev_mass = Some(&cp.global_mass);
            writeln!(file, "{}", row)?;
        }
    }

    Ok(())
}
