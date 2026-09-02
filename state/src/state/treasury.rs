//! Financial and structural country state — the `Treasury` (Python `data`,
//! i.e. `ctx.budgets[country]`).
//!
//! Every struct here mirrors the Python save schema key-for-key via
//! `#[serde(rename)]`. Because the live Python engine attaches many
//! runtime-computed fields beyond the world-gen baseline (e.g. `resources`,
//! `warehouses`, per-sector `pmi`/`wage`), each struct carries a
//! `#[serde(flatten)] extra` catch-all so **no data is dropped** on a
//! load/save round-trip.

use crate::registries::enums::Sector;
use crate::registries::tech_tree::TechId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};

/// Returns `true` if an `f64` is exactly zero.
///
/// Used by `#[serde(skip_serializing_if)]` for optional budget keys that the
/// Python save may omit when they are zero.
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// Returns `true` if the wage cap is the default value (1.2).
///
/// Used by `#[serde(skip_serializing_if)]` to avoid serializing the default
/// wage cap in legacy Python saves that don't have this field.
fn is_default_wage_cap(v: &f64) -> bool {
    (*v - 1.2).abs() < 1e-9
}

// ============================================================================
// STAGE C: TAX HISTORY STRUCTURES
// ============================================================================

/// Tax history entry for tracking tax collection over time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TaxHistoryEntry {
    /// Turn number.
    pub turn: u32,
    /// PIT collected.
    pub pit_collected: f64,
    /// CIT collected.
    pub cit_collected: f64,
    /// VAT collected.
    pub vat_collected: f64,
    /// Wealth tax collected.
    pub wealth_tax_collected: f64,
    /// Capital gains tax collected.
    pub capital_gains_collected: f64,
    /// Microregion share.
    pub microregion_share: f64,
    /// Region share.
    pub region_share: f64,
    /// Central share.
    pub central_share: f64,
    /// Evasion rate.
    pub evasion_rate: f64,
    /// Capital flight amount.
    pub capital_flight: f64,
    /// Any additional tax history fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Stock-market state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StockMarket {
    /// Headline index level.
    pub index: f64,
    /// Investor confidence, 0–100.
    pub confidence: f64,
    /// Change recorded on the previous turn.
    pub last_change: f64,
    /// Per-industry sub-indices; kept as a raw JSON
    /// value to losslessly preserve its evolving shape.
    #[serde(default)]
    pub sector_indices: Value,
    /// Any additional keys not explicitly modeled.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for StockMarket {
    fn default() -> Self {
        Self {
            index: 1000.0,
            confidence: 50.0,
            last_change: 0.0,
            sector_indices: Value::Object(serde_json::Map::new()),
            extra: Map::new(),
        }
    }
}

/// Government spending allocation as fractions of the budget that sum to `1.0`
///.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BudgetAllocations {
    /// Industry.
    pub industry: f64,
    /// Education & propaganda.
    pub education_propaganda: f64,
    /// Healthcare.
    pub healthcare: f64,
    /// Infrastructure & transport.
    pub infrastructure_transport: f64,
    /// Social programs.
    pub social_programs: f64,
    /// Agriculture & rural economy.
    pub agriculture_rural: f64,
    /// Armed forces.
    pub armed_forces: f64,
    /// Justice system (courts, police, prisons) — Phase 14.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub justice: f64,
    /// Public administration (tax offices, civil service) — Phase 14.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub public_administration: f64,
    /// Any additional allocation categories.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for BudgetAllocations {
    fn default() -> Self {
        Self {
            industry: 0.0,
            education_propaganda: 0.0,
            healthcare: 0.0,
            infrastructure_transport: 0.0,
            social_programs: 0.0,
            agriculture_rural: 0.0,
            armed_forces: 0.0,
            justice: 0.0,
            public_administration: 0.0,
            extra: Map::new(),
        }
    }
}

/// The multi-slot production-method selection for a sector.
/// Phase 81 Wave 2: Expanded with 7 new slots (4 implemented + 3 future-proofed).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ProductionMethodChoice {
    /// Automation method.
    pub automation: String,
    /// Production method.
    pub production: String,
    /// Organization method.
    pub organization: String,
    /// Phase 81 Wave 2: Active lighting method (e.g., "Kerosene Lamps", "LED Lighting").
    #[serde(default)]
    pub lighting: String,
    /// Phase 81 Wave 2: Active heating method (e.g., "Coal Stove", "Heat Pump").
    #[serde(default)]
    pub heating: String,
    /// Phase 81 Wave 2: Active ventilation method (e.g., "Steam-Driven", "Electric Pumps/Fans").
    #[serde(default)]
    pub ventilation: String,
    /// Phase 81 Wave 2: Active power generation method (e.g., "None", "Rooftop PV").
    #[serde(default)]
    pub power_generation: String,
    /// Phase 83 (future-proofed): Active water supply method. Defaults to "None".
    #[serde(default)]
    pub water_supply: String,
    /// Phase 83 (future-proofed): Active sanitation method. Defaults to "None".
    #[serde(default)]
    pub sanitation: String,
    /// Phase 84 (future-proofed): Active waste disposal method. Defaults to "None".
    #[serde(default)]
    pub waste_disposal: String,
    /// Phase 82B: Active emission control method (e.g., "None", "Wet Scrubber").
    /// Upgradable independently of production method.
    #[serde(default)]
    pub emission_control: String,
    /// Any additional method slots.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// One macroeconomic sector's share and runtime economics (an entry of
/// "sektory").
///
/// # Rules
/// * Only `gdp_share` is guaranteed present across all sectors; other fields
///   are optional because service/state sectors (e.g. `transport_i_logistyka`,
///   `public_services`) omit them. Runtime fields (`pmi`, `wage`,
///   `employment`, ...) are preserved through `extra`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SectorShare {
    /// Share of GDP in `[0.0, 1.0]`.
    #[serde(default)]
    pub gdp_share: f64,
    /// Crisis vulnerability coefficient, absent for
    /// some service sectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crisis_vulnerability: Option<f64>,
    /// Currently selected production methods, absent for
    /// some service sectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_method: Option<ProductionMethodChoice>,
    /// Runtime-computed fields (`pmi`, `wage`, `oferta`, `employment`,
    /// `srednia_placa`, `wykorzystanie_mocy`, ...).
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// National R&D / science state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScienceState {
    /// Accumulated innovation points.
    pub innovation_points: f64,
    /// Technology currently being researched, if any.
    #[serde(default)]
    pub researching: Option<TechId>,
    /// Technologies already discovered.
    #[serde(default)]
    pub discovered: Vec<TechId>,
    /// Baseline innovativeness.
    pub base_innovativeness: f64,
    /// Any additional science fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for ScienceState {
    fn default() -> Self {
        Self {
            innovation_points: 0.0,
            researching: None,
            discovered: Vec::new(),
            base_innovativeness: 0.0,
            extra: Map::new(),
        }
    }
}

/// Financial and structural state of a nation (Python `ctx.budgets[country]`).
///
/// # Rules
/// * Schema-guaranteed scalars are strictly typed; everything the live engine
///   adds at runtime (`resources`, `warehouses`, `exports`, `energy_stats`, ...) is
///   preserved verbatim in [`Treasury::extra`], guaranteeing a lossless
///   round-trip against Python saves.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Treasury {
    /// Gross Domestic Product in nominal currency units.
    pub gdp: f64,
    /// Total population head-count.
    pub population: u64,
    /// Nominal state budget.
    pub nominal_budget: f64,
    /// Liquid reserves.
    pub liquid_reserves: f64,
    /// Aggregate citizen savings.
    pub citizen_savings: f64,
    /// Private capital stock.
    pub private_capital: f64,
    /// Aggregate infrastructure level.
    pub infrastructure_level: f64,
    /// Installed energy infrastructure.
    pub energy_infrastructure: f64,
    /// Stock-market state.
    pub stock_market: StockMarket,
    /// Budget allocation fractions.
    pub allocations: BudgetAllocations,
    /// Hidden black-ops fund; never surfaced in
    /// public fiscal reports.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub black_ops_budget: f64,
    /// Per-sector shares and economics.
    /// Phase 43: Default for legacy Polish fixtures — Polish sector names
    /// (mining_sector, etc.) don't match the Sector enum, so the
    /// 'sektory' key stays in extra and sectors defaults to empty.
    #[serde(default)]
    pub sectors: HashMap<Sector, SectorShare>,
    /// National science / R&D state.
    #[serde(default)]
    pub science: ScienceState,
    // STAGE C: Tax Office Company IDs (NOT custom structs)
    /// Tax Office Company IDs for budget allocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tax_office_ids: Vec<String>,
    /// Tax history entries.
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    pub tax_history: VecDeque<TaxHistoryEntry>,
    /// Free-text log of the last fiscal balance.
    #[serde(default)]
    pub last_balance_log: String,
    /// Trade balance for the current turn; absent before
    /// the first trading session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trade_balance: Option<f64>,
    /// Maximum public wage multiplier to prevent crowding out effect (Phase 5).
    /// Public administration companies cannot offer wages higher than
    /// regional_average_wage * max_public_wage_multiplier.
    #[serde(default, skip_serializing_if = "is_default_wage_cap")]
    pub max_public_wage_multiplier: f64,
    /// PHASE 4: Outstanding corporate debts from SSE clawbacks (receivable assets)
    #[serde(default)]
    pub outstanding_corporate_debts: HashMap<String, f64>,
    /// Phase 6.3: Emergency liquidation expenses (wages funded by State for bankrupt companies)
    #[serde(default)]
    pub liquidation_expenses: f64,
    /// Phase 6.3.5: Placeholder logistics revenue from transport fees
    #[serde(default)]
    pub logistics_revenue: f64,
    /// Phase D.9: Dedicated earmarked equalization fund (Janosikowe).
    /// Rich regions are debited into this fund, poor regions are credited
    /// from it. The fund must zero out each turn — any unallocated remainder
    /// is swept to general `liquid_reserves` as an administrative fee.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub equalization_fund: f64,
    /// All other runtime-added keys, preserved losslessly.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for Treasury {
    fn default() -> Self {
        Self {
            gdp: 0.0,
            population: 0,
            nominal_budget: 0.0,
            liquid_reserves: 0.0,
            citizen_savings: 0.0,
            private_capital: 0.0,
            infrastructure_level: 0.0,
            energy_infrastructure: 0.0,
            stock_market: StockMarket::default(),
            allocations: BudgetAllocations::default(),
            black_ops_budget: 0.0,
            sectors: HashMap::new(),
            science: ScienceState::default(),
            tax_office_ids: Vec::new(),
            tax_history: VecDeque::new(),
            last_balance_log: String::new(),
            trade_balance: None,
            max_public_wage_multiplier: 1.2, // Phase 5: Default to prevent crowding out
            outstanding_corporate_debts: HashMap::new(),
            liquidation_expenses: 0.0, // Phase 6.3: Default liquidation expenses
            logistics_revenue: 0.0,    // Phase 6.3.5: Default logistics revenue
            equalization_fund: 0.0,    // Phase D.9: Default equalization fund
            extra: Map::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "gdp": 57771122285.24455,
        "population": 17490459,
        "nominal_budget": 15424488847.98,
        "liquid_reserves": 3393522742.23,
        "citizen_savings": 5126423288.11,
        "private_capital": 0,
        "infrastructure_level": 221.08,
        "energy_infrastructure": 2803.84,
        "stock_market": { "index": 1000.0, "confidence": 76.04, "last_change": 0.0, "sector_indices": {} },
        "allocations": {
            "industry": 0.18, "education_propaganda": 0.05, "healthcare": 0.17,
            "infrastructure_transport": 0.23, "social_programs": 0.26,
            "agriculture_rural": 0.04, "armed_forces": 0.04
        },
        "black_ops_budget": 0.0,
        "sectors": {
            "agriculture": {
                "gdp_share": 0.12, "crisis_vulnerability": 0.2,
                "active_method": {"automation": "Combustion Tractors", "production": "Three-Field System", "organization": "Peasant Farms"},
                "capacity_utilization": 0.0, "wage": 660.6, "employment": 2146400, "pmi": 33.9
            },
            "public_services": { "gdp_share": 0.03, "capacity_utilization": 0.0, "pmi": 50.0, "employment": 1000 }
        },
        "science": { "innovation_points": 0.0, "researching": null, "discovered": ["tech_001","tech_002"], "base_innovativeness": 0.0 },
        "last_balance_log": "",
        "resources": {"coal": 999},
        "warehouses": {"grain": 42.0}
    }"#;

    #[test]
    fn deserializes_known_and_extra_fields() {
        let t: Treasury = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(t.population, 17_490_459);
        assert_eq!(t.private_capital, 0.0);
        assert_eq!(t.stock_market.index, 1000.0);
        assert!((t.allocations.industry - 0.18).abs() < 1e-9);
        // Runtime-only top-level keys land in `extra`.
        assert!(t.extra.contains_key("resources"));
        assert!(t.extra.contains_key("warehouses"));
    }

    #[test]
    fn optional_sector_fields_handled() {
        let t: Treasury = serde_json::from_str(FIXTURE).unwrap();
        let public = &t.sectors[&Sector::PublicServices];
        assert!(public.crisis_vulnerability.is_none());
        assert!(public.active_method.is_none());
        assert!(public.extra.contains_key("pmi"));

        let agri = &t.sectors[&Sector::Agriculture];
        assert_eq!(agri.crisis_vulnerability, Some(0.2));
        assert_eq!(
            agri.active_method.as_ref().unwrap().production,
            "Three-Field System"
        );
    }

    #[test]
    fn struct_round_trip_is_lossless() {
        let t1: Treasury = serde_json::from_str(FIXTURE).unwrap();
        let json = serde_json::to_string(&t1).unwrap();
        let t2: Treasury = serde_json::from_str(&json).unwrap();
        assert_eq!(t1, t2);
    }
}
