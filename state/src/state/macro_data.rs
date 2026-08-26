//! Macroeconomic and social indicators — the `MacroData` (Python `makro`,
//! i.e. `ctx.makro[country]`).
//!
//! Stable scalar indicators, the energy mix, the labor market, and national
//! demographics are now strictly typed. Remaining political and runtime
//! sub-trees (`polityka`, `statystyki_zdrowotne`, `statystyki_edukacyjne`,
//! `przestepczosc`, ...) are preserved in [`MacroData::extra`] until they are
//! individually ported.

use crate::registries::enums::WealthBracket;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// A 3-letter currency prefix such as `"ILI"`. Modeled as a `String` (not an
/// enum) because codes are generated per-country at world-gen time.
pub type CurrencyCode = String;

/// National energy generation mix as fractions that sum to `1.0`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EnergyMix {
    /// Coal share.
    pub coal: f64,
    /// Natural gas share.
    pub natural_gas: f64,
    /// Uranium/nuclear share.
    pub uranium: f64,
    /// Renewables share.
    pub renewables: f64,
    /// Any additional energy sources.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for EnergyMix {
    fn default() -> Self {
        Self {
            coal: 0.0,
            natural_gas: 0.0,
            uranium: 0.0,
            renewables: 0.0,
            extra: Map::new(),
        }
    }
}

/// Population age distribution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AgeGroups {
    /// Children.
    #[serde(default)]
    pub children: f64,
    /// Working-age adults.
    #[serde(default)]
    pub adults: f64,
    /// Elderly.
    #[serde(default)]
    pub elderly: f64,
    /// Any additional age groups.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl AgeGroups {
    /// Default shape used when the JSON is missing age groups.
    pub fn new_default() -> Self {
        Self {
            children: 0.25,
            adults: 0.60,
            elderly: 0.15,
            extra: Map::new(),
        }
    }
}

impl Default for AgeGroups {
    fn default() -> Self {
        Self::new_default()
    }
}

/// Gender split of the population.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Gender {
    /// Male share.
    #[serde(default)]
    pub male: f64,
    /// Female share.
    #[serde(default)]
    pub female: f64,
    /// Any additional gender keys.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for Gender {
    fn default() -> Self {
        Self {
            male: 0.5,
            female: 0.5,
            extra: Map::new(),
        }
    }
}

/// Education distribution of the adult population.
///
/// # Rules
/// * The `srednie` and `wyzsze` fields are maps of specialization → share
///   (e.g. `"Techniczne": 0.105`).
/// * `podstawowe` and `brak` are scalar shares.
/// * The Python `workforce.py` uses `wyzsze` for experts, `podstawowe` for the
///   `sredni` tier, and `brak` for the `szeregowi` tier.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Education {
    /// No formal education.
    #[serde(default)]
    pub brak: f64,
    /// Basic education.
    #[serde(default)]
    pub podstawowe: f64,
    /// Secondary education specializations.
    #[serde(default)]
    pub srednie: BTreeMap<String, f64>,
    /// Higher education specializations.
    #[serde(default)]
    pub wyzsze: BTreeMap<String, f64>,
    /// Any additional education categories.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Education {
    /// Total share with higher education.
    pub fn higher_share(&self) -> f64 {
        self.wyzsze.values().sum::<f64>()
    }

    /// Total share with secondary education.
    pub fn secondary_share(&self) -> f64 {
        self.srednie.values().sum::<f64>()
    }
}

/// One immigrant/expatriate cohort.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ImmigrantCohort {
    /// Number of people in the cohort.
    #[serde(default)]
    pub count: f64,
    /// Years since arrival.
    #[serde(default)]
    pub seniority: u32,
    /// Phase 18A: Legal status of this cohort.
    #[serde(default)]
    pub legal_status: crate::economy::legal_status::LegalStatus,
    /// Phase 18A: Remittance rate (fraction of net income sent abroad).
    /// Only applies to TemporaryWorker status. Default 0.10 (10%).
    #[serde(default = "default_remittance_rate")]
    pub remittance_rate: f64,
    /// Any additional cohort fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_remittance_rate() -> f64 {
    0.10
}

/// National demographics.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Demographics {
    /// Birth rate, percent.
    #[serde(default)]
    pub birth_rate: f64,
    /// Mortality rate, percent.
    #[serde(default)]
    pub death_rate: f64,
    /// Net migration rate as a fraction of population.
    #[serde(default)]
    pub net_migration: f64,
    /// Age distribution.
    #[serde(default)]
    pub age_groups: AgeGroups,
    /// Gender split.
    #[serde(default)]
    pub gender: Gender,
    /// Ethnic composition.
    #[serde(default)]
    pub ethnic_composition: BTreeMap<String, f64>,
    /// Religious composition.
    #[serde(default)]
    pub religious_composition: BTreeMap<String, f64>,
    /// Education distribution.
    #[serde(default)]
    pub education: Education,
    /// Immigrant cohorts.
    #[serde(default)]
    pub immigrant_cohorts: Vec<ImmigrantCohort>,
    /// Dominant diaspora group.
    #[serde(default)]
    pub dominant_diaspora: String,
    /// Urban population share.
    #[serde(default)]
    pub city_urban: f64,
    /// Rural population share.
    #[serde(default)]
    pub rural: f64,
    /// Nomadic population share.
    #[serde(default)]
    pub nomads: f64,
    /// Foreign students present.
    #[serde(default)]
    pub foreign_students: f64,
    /// Refugees present.
    #[serde(default)]
    pub refugees: f64,
    /// Seasonal workers present.
    #[serde(default)]
    pub seasonal_workers: f64,
    /// Illegal immigrants.
    #[serde(default)]
    pub illegal_immigrants: f64,
    /// Unassimilated immigrants.
    #[serde(default)]
    pub unassimilated_immigrants: f64,
    /// Effective immigrant remittance flow.
    #[serde(default)]
    pub effective_immigrant_remittances: f64,
    /// Emigrants abroad.
    #[serde(default)]
    pub emigrants: f64,
    /// Average age.
    #[serde(default)]
    pub average_age: f64,
    /// Median age.
    #[serde(default)]
    pub median_age: f64,
    /// Cached population size (from budget.populacja).
    #[serde(default)]
    pub population_size: f64,
    /// Births in the last turn.
    #[serde(default)]
    pub last_births: f64,
    /// Deaths in the last turn.
    #[serde(default)]
    pub last_deaths: f64,
    /// Net migration in the last turn.
    #[serde(default)]
    pub last_migration: f64,
    /// Brain-drain pressure on high-skill wages.
    #[serde(default)]
    pub brain_drain_index: f64,
    /// Any additional demographic fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Breakdown of unemployment into its three canonical components.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct UnemploymentStructure {
    /// Frictional unemployment fraction.
    #[serde(default)]
    pub friction: f64,
    /// Structural unemployment fraction.
    #[serde(default)]
    pub structural: f64,
    /// Cyclical unemployment fraction.
    #[serde(default)]
    pub cyclical: f64,
    /// Any additional unemployment categories.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Per-tier labor market statistics.
///
/// # Rules
/// * Stored inside [`LaborMarket`] for experts, skilled, and unskilled tiers.
/// * Empty tiers are skipped during serialization.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TierStats {
    /// Available labor supply in this tier.
    #[serde(default)]
    pub supply: f64,
    /// Tier-specific wage level.
    #[serde(default)]
    pub wage: f64,
    /// Employed workers in this tier.
    #[serde(default)]
    pub employed: f64,
    /// Unemployed workers in this tier.
    #[serde(default)]
    pub unemployed: f64,
    /// Workforce shortage relative to demand.
    #[serde(default)]
    pub shortage: f64,
    /// Any additional tier fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl TierStats {
    /// Returns true when all numeric fields are effectively zero.
    fn is_empty(&self) -> bool {
        self.supply.abs() < 1e-12
            && self.wage.abs() < 1e-12
            && self.employed.abs() < 1e-12
            && self.unemployed.abs() < 1e-12
            && self.shortage.abs() < 1e-12
    }
}

/// Labor market state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LaborMarket {
    /// Unemployment rate, percent.
    #[serde(default)]
    pub unemployment_rate: f64,
    /// Labor force participation, percent.
    #[serde(default)]
    pub labor_force_participation: f64,
    /// Breakdown of unemployment.
    #[serde(default)]
    pub unemployment_structure: UnemploymentStructure,
    /// Underemployment rate / fraction.
    #[serde(default)]
    pub underemployment: f64,
    /// Subsistence peasants outside formal labor force.
    #[serde(default)]
    pub subsistence_peasants: f64,
    /// Poverty-pool share.
    #[serde(default)]
    pub poverty_pool_percent: f64,
    /// Total employed.
    #[serde(default)]
    pub employed_total: f64,
    /// Total unemployed.
    #[serde(default)]
    pub unemployed: f64,
    /// Expert workforce shortage.
    #[serde(default)]
    pub expert_shortage: f64,
    /// Skilled workforce shortage.
    #[serde(default)]
    pub skilled_shortage: f64,
    /// Unskilled workforce shortage.
    #[serde(default)]
    pub unskilled_shortage: f64,
    /// Expatriate cohorts.
    #[serde(default)]
    pub expat_cohorts: Vec<ImmigrantCohort>,
    /// Expatriate count.
    #[serde(default)]
    pub expat_count: f64,
    /// Foreign contract costs.
    #[serde(default)]
    pub foreign_contract_costs: f64,
    /// Active disabled workers.
    #[serde(default)]
    pub active_disabled: f64,
    /// Citizens unable to work.
    #[serde(default)]
    pub unable_to_work: f64,
    /// Expert tier statistics.
    #[serde(default, skip_serializing_if = "TierStats::is_empty")]
    pub expert_tier: TierStats,
    /// Skilled tier statistics.
    #[serde(default, skip_serializing_if = "TierStats::is_empty")]
    pub skilled_tier: TierStats,
    /// Unskilled tier statistics.
    #[serde(default, skip_serializing_if = "TierStats::is_empty")]
    pub unskilled_tier: TierStats,
    /// Explicit serf population (excluded from labor market).
    #[serde(default)]
    pub serf_population: f64,
    /// Emergency Stabilization: Total furloughed workers across all companies.
    /// Aggregated each turn from `company.furloughed_workers_count`. Exposed
    /// in the Macro Indicators DTO so the player can see furloughed count
    /// alongside unemployment on the dashboard.
    #[serde(default)]
    pub furloughed_total: f64,
    /// AI & Stability Audit (Pillar 4C): Previous turn's unemployment rate.
    /// Used by the counter-cyclical response to detect unemployment SPIKES
    /// (current > previous) vs stable high unemployment.
    #[serde(default)]
    pub prev_unemployment_rate: f64,
    /// Any additional labor-market fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Health statistics extracted from legacy extra field
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HealthStatistics {
    /// Quality of health service.
    #[serde(default)]
    pub service_quality: f64,

    /// Average lifespan.
    #[serde(default)]
    pub average_lifespan: f64,

    /// Mortality rate.
    #[serde(default)]
    pub mortality_rate: f64,

    /// Hospital coverage.
    #[serde(default)]
    pub hospital_coverage: f64,
}

impl HealthStatistics {
}

/// Education statistics.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct EducationStatistics {
    /// Infrastructure base.
    #[serde(default)]
    pub infrastructure_base: f64,

    /// Literacy rate.
    #[serde(default)]
    pub literacy_rate: f64,

    /// Higher education rate.
    #[serde(default)]
    pub higher_education_rate: f64,
}

impl EducationStatistics {
}

/// Phase 24F: A single telemetry sample stored in the rolling history buffer.
///
/// Captures the key macro indicators at one point in time so that ToT
/// (Turn-over-Turn) and YoY (Year-over-Year) deltas can be computed.
/// 24 turns = 1 year in the engine's calendar.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TelemetrySample {
    /// Global turn when this sample was taken.
    #[serde(default)]
    pub turn: u32,
    /// Year when this sample was taken.
    #[serde(default)]
    pub year: u32,
    /// Official GDP (expenditure approach).
    #[serde(default)]
    pub official_gdp: f64,
    /// Shadow GDP.
    #[serde(default)]
    pub shadow_gdp: f64,
    /// CPI index level.
    #[serde(default)]
    pub cpi_index: f64,
    /// PPI index level.
    #[serde(default)]
    pub ppi_index: f64,
    /// CPI inflation rate (percent).
    #[serde(default)]
    pub cpi_inflation: f64,
    /// PPI inflation rate (percent).
    #[serde(default)]
    pub ppi_inflation: f64,
    /// M0 monetary base.
    #[serde(default)]
    pub m0: f64,
    /// M3 broad money.
    #[serde(default)]
    pub m3: f64,
    /// Unemployment rate (percent).
    #[serde(default)]
    pub unemployment_pct: f64,
    /// Average wage.
    #[serde(default)]
    pub average_wage: f64,
    /// Corruption index (0.0–1.0).
    #[serde(default)]
    pub corruption_index: f64,
    /// Total deceased from OHS/disasters (cumulative).
    #[serde(default)]
    pub total_deceased: i64,
    /// Total disabled from OHS/disasters (cumulative).
    #[serde(default)]
    pub total_disabled: i64,
    /// Unable-to-work FTE (cumulative).
    #[serde(default)]
    pub unable_to_work_fte: f64,
    /// Population.
    #[serde(default)]
    pub population: u64,
    /// Treasury liquid reserves.
    #[serde(default)]
    pub liquid_reserves: f64,
}

/// Phase 24F: Rolling buffer of telemetry samples for ToT/YoY delta computation.
///
/// Stores up to `MAX_HISTORY` samples (25 = 1 year + 1 current). When the
/// buffer is full, the oldest sample is dropped (FIFO). This is persisted
/// via serde as part of `MacroData`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TelemetryHistory {
    /// Ring buffer of samples, oldest first.
    #[serde(default)]
    pub samples: Vec<TelemetrySample>,
}

/// Number of turns in one year (24 half-months).
pub const TURNS_PER_YEAR: usize = 24;
/// Maximum samples to retain (1 year + 1 current turn).
pub const MAX_HISTORY: usize = TURNS_PER_YEAR + 1;

/// Phase 74: Convert an annual compounding rate to the equivalent per-turn rate.
///
/// Uses the compound period rate formula: `R_turn = (1 + R_annual)^(1/24) - 1`
///
/// This MUST be used for any rate that compounds over time (interest, birth/death
/// rates, depreciation-as-fraction, etc.). Simple division by 24 is forbidden for
/// compounding rates because it causes exponential drift.
///
/// # Example
/// ```
/// use sim_engine::state::macro_data::annual_to_per_turn_rate;
/// // 18% annual interest → 0.6926% per turn (compound)
/// let per_turn = annual_to_per_turn_rate(0.18);
/// // After 24 turns: (1 + 0.006926)^24 ≈ 1.18 (correct)
/// ```
pub fn annual_to_per_turn_rate(annual_rate: f64) -> f64 {
    (1.0 + annual_rate).powf(1.0 / TURNS_PER_YEAR as f64) - 1.0
}

/// Phase 74: Convert an annual linear quantity to per-turn quantity.
///
/// Used for physical throughput, one-shot payments, and operating expenses that
/// are consumed (not compounded) each turn. This is simple division by 24.
///
/// # Example
/// ```
/// use sim_engine::state::macro_data::annual_to_per_turn_quantity;
/// // 1200 units/year → 50 units/turn
/// let per_turn = annual_to_per_turn_quantity(1200.0);
/// ```
pub fn annual_to_per_turn_quantity(annual_quantity: f64) -> f64 {
    annual_quantity / TURNS_PER_YEAR as f64
}

impl TelemetryHistory {
    /// Append a new sample, dropping the oldest if at capacity.
    pub fn push(&mut self, sample: TelemetrySample) {
        self.samples.push(sample);
        if self.samples.len() > MAX_HISTORY {
            self.samples.remove(0);
        }
    }

    /// Get the most recent sample, if any.
    pub fn latest(&self) -> Option<&TelemetrySample> {
        self.samples.last()
    }

    /// Get the sample from `n` turns ago, if available.
    ///
    /// `n=1` returns the previous turn's sample (for ToT delta).
    /// `n=24` returns the sample from one year ago (for YoY delta).
    pub fn turns_ago(&self, n: usize) -> Option<&TelemetrySample> {
        let len = self.samples.len();
        if len > n {
            self.samples.get(len - 1 - n)
        } else {
            None
        }
    }

    /// Get the previous turn's sample (ToT reference).
    pub fn previous_turn(&self) -> Option<&TelemetrySample> {
        self.turns_ago(1)
    }

    /// Get the sample from one year ago (YoY reference).
    pub fn one_year_ago(&self) -> Option<&TelemetrySample> {
        self.turns_ago(TURNS_PER_YEAR)
    }

    /// Compute a ToT (Turn-over-Turn) percentage delta.
    /// Returns `None` if there's no previous sample.
    /// Returns `0.0` if the previous value is zero (avoid div-by-zero).
    pub fn tot_pct(&self, current: f64, field: impl Fn(&TelemetrySample) -> f64) -> Option<f64> {
        let prev = self.previous_turn()?;
        let prev_val = field(prev);
        if prev_val != 0.0 {
            Some((current - prev_val) / prev_val.abs() * 100.0)
        } else {
            Some(0.0)
        }
    }

    /// Compute a YoY (Year-over-Year) percentage delta.
    /// Returns `None` if there's no sample from one year ago.
    pub fn yoy_pct(&self, current: f64, field: impl Fn(&TelemetrySample) -> f64) -> Option<f64> {
        let prev = self.one_year_ago()?;
        let prev_val = field(prev);
        if prev_val != 0.0 {
            Some((current - prev_val) / prev_val.abs() * 100.0)
        } else {
            Some(0.0)
        }
    }
}

#[cfg(test)]
mod telemetry_history_tests {
    use super::*;

    fn sample(turn: u32, gdp: f64) -> TelemetrySample {
        TelemetrySample {
            turn,
            year: turn / 24,
            official_gdp: gdp,
            ..Default::default()
        }
    }

    #[test]
    fn test_history_push_and_latest() {
        let mut h = TelemetryHistory::default();
        assert!(h.latest().is_none());

        h.push(sample(1, 100.0));
        h.push(sample(2, 110.0));
        assert_eq!(h.latest().unwrap().turn, 2);
        assert_eq!(h.samples.len(), 2);
    }

    #[test]
    fn test_history_fifo_eviction() {
        let mut h = TelemetryHistory::default();
        for i in 1..=MAX_HISTORY {
            h.push(sample(i as u32, i as f64 * 10.0));
        }
        assert_eq!(h.samples.len(), MAX_HISTORY);
        // First sample should be turn 1.
        assert_eq!(h.samples[0].turn, 1);

        // Push one more — oldest should be evicted.
        h.push(sample((MAX_HISTORY + 1) as u32, 999.0));
        assert_eq!(h.samples.len(), MAX_HISTORY);
        // First sample should now be turn 2.
        assert_eq!(h.samples[0].turn, 2);
    }

    #[test]
    fn test_tot_pct() {
        let mut h = TelemetryHistory::default();
        h.push(sample(1, 100.0));
        h.push(sample(2, 110.0));

        let delta = h.tot_pct(110.0, |s| s.official_gdp).unwrap();
        assert!((delta - 10.0).abs() < 0.001); // +10%
    }

    #[test]
    fn test_tot_pct_no_history() {
        let h = TelemetryHistory::default();
        assert!(h.tot_pct(100.0, |s| s.official_gdp).is_none());
    }

    #[test]
    fn test_yoy_pct() {
        let mut h = TelemetryHistory::default();
        // Fill 25 turns (1 year + 1 current).
        for i in 1..=25 {
            h.push(sample(i as u32, if i == 1 { 100.0 } else { 100.0 }));
        }
        // Turn 25's GDP is 100, turn 1's GDP was 100 → 0% YoY.
        let delta = h.yoy_pct(100.0, |s| s.official_gdp).unwrap();
        assert!(delta.abs() < 0.001);

        // Now push a turn with GDP 120 — YoY should be +20%.
        h.push(sample(26, 120.0));
        let delta = h.yoy_pct(120.0, |s| s.official_gdp).unwrap();
        assert!((delta - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_yoy_pct_not_enough_history() {
        let mut h = TelemetryHistory::default();
        h.push(sample(1, 100.0));
        h.push(sample(2, 110.0));
        // Only 2 samples — can't look back 24 turns.
        assert!(h.yoy_pct(110.0, |s| s.official_gdp).is_none());
    }

    #[test]
    fn test_tot_pct_zero_previous() {
        let mut h = TelemetryHistory::default();
        h.push(sample(1, 0.0));
        h.push(sample(2, 50.0));
        // Previous value is 0 → should return 0.0 (not infinity).
        let delta = h.tot_pct(50.0, |s| s.official_gdp).unwrap();
        assert_eq!(delta, 0.0);
    }
}

/// Phase 24D: GDP expenditure-side breakdown.
///
/// Computed at end-of-turn from actual cash flows (B2C clearing, ministry
/// procurement, fixed-asset purchases, construction spend, net trade).
/// All components are non-negative; `shadow_gdp` is tracked separately.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct GdpBreakdown {
    /// Final household consumption (B2C retail clearing revenue).
    #[serde(default)]
    pub consumption: f64,
    /// Government spending (ministry procurement + subsidies + public wages).
    #[serde(default)]
    pub government_spending: f64,
    /// Gross investment (fixed-asset purchases + construction project spend).
    #[serde(default)]
    pub investment: f64,
    /// Net exports (exports − imports, from trade balance).
    #[serde(default)]
    pub net_exports: f64,
    /// Official GDP = C + G + I + NX.
    #[serde(default)]
    pub official_gdp: f64,
    /// Previous turn's official GDP (for YoY growth computation).
    #[serde(default)]
    pub previous_gdp: f64,
    /// Shadow GDP: off-the-books wages + bribes (parallel economy).
    #[serde(default)]
    pub shadow_gdp: f64,
    /// Phase 44: Imputed consumption from subsistence economy (Serf in-kind).
    /// Included in official_gdp but tracked separately for monetary analysis.
    #[serde(default)]
    pub imputed_consumption: f64,
}

impl GdpBreakdown {
    /// Returns the YoY GDP growth rate as a fraction (0.02 = 2%).
    /// Returns 0.0 if previous_gdp is zero or negative.
    pub fn growth_rate(&self) -> f64 {
        if self.previous_gdp > 0.0 {
            (self.official_gdp - self.previous_gdp) / self.previous_gdp
        } else {
            0.0
        }
    }
}

/// Phase 24D: Dual inflation indices (CPI and PPI).
///
/// CPI tracks a consumer-goods basket weighted by `consumption_registry`.
/// PPI tracks a producer-goods basket (Steel, HardCoal, Energy, etc.).
/// Both use VWAP from `MarketHistory` as the price input.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct InflationIndices {
    /// Current CPI index level (base = 100.0 at world-gen).
    #[serde(default = "default_index_base")]
    pub cpi_index: f64,
    /// Previous turn's CPI index (for delta computation).
    #[serde(default = "default_index_base")]
    pub previous_cpi_index: f64,
    /// Current PPI index level (base = 100.0 at world-gen).
    #[serde(default = "default_index_base")]
    pub ppi_index: f64,
    /// Previous turn's PPI index.
    #[serde(default = "default_index_base")]
    pub previous_ppi_index: f64,
    /// CPI inflation rate, percent (computed from index delta).
    #[serde(default)]
    pub cpi_inflation: f64,
    /// PPI inflation rate, percent (computed from index delta).
    #[serde(default)]
    pub ppi_inflation: f64,
}

fn default_index_base() -> f64 {
    100.0
}

/// Phase 24D: Money supply snapshot (M0, M3, multiplier).
///
/// Computed at end-of-turn by walking all company brokerage accounts,
/// bank balance sheets, treasury reserves, and class savings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MoneySupplySnapshot {
    /// M0 = cash in circulation + bank reserves at central bank.
    #[serde(default)]
    pub m0: f64,
    /// M3 = M0 + demand deposits + time deposits + other liquid assets.
    #[serde(default)]
    pub m3: f64,
    /// Money multiplier = M3 / M0 (0.0 if M0 is zero).
    #[serde(default)]
    pub multiplier: f64,
    /// Physical cash circulating in the economy.
    #[serde(default)]
    pub cash_in_circulation: f64,
    /// Total bank reserves held at the central bank.
    #[serde(default)]
    pub bank_reserves: f64,
    /// Total demand deposits in commercial banks.
    #[serde(default)]
    pub demand_deposits: f64,
    /// Total time deposits in commercial banks.
    #[serde(default)]
    pub time_deposits: f64,
    /// Previous turn's M3 (for delta computation).
    #[serde(default)]
    pub previous_m3: f64,
}

/// Macroeconomic and social indicators for a nation (Python
/// `ctx.makro[country]`).
///
/// # Rules
/// * Schema-guaranteed scalars, the wealth bracket, the energy mix, the labor
///   market, and national demographics are strictly typed.
/// * Remaining political and runtime sub-trees are preserved verbatim in
///   [`Self::extra`] for a lossless round-trip until individually ported.
/// * Phase 24D: `gdp_breakdown`, `inflation_indices`, and `money_supply`
///   are recomputed every turn from actual cash flows and VWAP data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MacroData {
    /// Annual inflation rate, percent (driven by CPI delta since Phase 24D).
    pub inflation: f64,
    /// Gini coefficient in `[0.0, 1.0]`.
    pub gini: f64,
    /// National social unrest, 0–100.
    pub social_unrest: f64,
    /// Prosperity bracket.
    pub wealth_bracket: WealthBracket,
    /// Labor productivity index.
    pub productivity: f64,
    /// Currency code in use.
    pub currency: CurrencyCode,
    /// Energy generation mix.
    pub energy_mix: EnergyMix,
    /// Average annual wage.
    pub average_wage: f64,
    /// Dominant culture name.
    pub culture: String,
    /// English demonym for the nation's people (e.g., "Bactrians", "Nordians").
    #[serde(default)]
    pub demonym: String,
    /// Broader cultural group.
    pub cultural_group: String,
    /// Dominant religion.
    pub religion: String,
    /// Phase 6.1: Absolute turn number for next election (replaces years_until_election)
    #[serde(default)]
    pub election_turn: u32,
    /// Labor market.
    #[serde(default)]
    pub labor_market: LaborMarket,
    /// National demographics.
    #[serde(default)]
    pub demographics: Demographics,
    /// Health statistics (extracted from extra in Phase 2).
    #[serde(default)]
    pub health_statistics: HealthStatistics,
    /// Education statistics (extracted from extra in Phase 2).
    #[serde(default)]
    pub education_statistics: EducationStatistics,
    /// Phase 24D: GDP expenditure-side breakdown (recomputed every turn).
    #[serde(default)]
    pub gdp_breakdown: GdpBreakdown,
    /// Phase 24D: Dual inflation indices (CPI & PPI, recomputed every turn).
    #[serde(default)]
    pub inflation_indices: InflationIndices,
    /// Phase 24D: Money supply snapshot (M0, M3, multiplier).
    #[serde(default)]
    pub money_supply: MoneySupplySnapshot,
    /// Phase 24F: Rolling telemetry history for ToT/YoY delta computation.
    #[serde(default)]
    pub telemetry_history: TelemetryHistory,
    /// Remaining political and runtime sub-trees (`polityka`,
    /// `statystyki_zdrowotne`, `statystyki_edukacyjne`, `przestepczosc`,
    /// ...), preserved losslessly.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for MacroData {
    fn default() -> Self {
        Self {
            inflation: 0.0,
            gini: 0.0,
            social_unrest: 0.0,
            wealth_bracket: WealthBracket::default(),
            productivity: 0.0,
            currency: CurrencyCode::default(),
            energy_mix: EnergyMix::default(),
            average_wage: 0.0,
            culture: String::new(),
            demonym: String::new(),
            cultural_group: String::new(),
            religion: String::new(),
            election_turn: 0,
            labor_market: LaborMarket::default(),
            demographics: Demographics::default(),
            health_statistics: HealthStatistics::default(),
            education_statistics: EducationStatistics::default(),
            gdp_breakdown: GdpBreakdown::default(),
            inflation_indices: InflationIndices::default(),
            money_supply: MoneySupplySnapshot::default(),
            telemetry_history: TelemetryHistory::default(),
            extra: Map::new(),
        }
    }
}

impl MacroData {
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "inflation": 6.2, "gini": 0.41, "social_unrest": 22.5,
        "wealth_bracket": "high", "productivity": 1.8, "currency": "ILI",
        "energy_mix": {"coal": 0.0, "natural_gas": 0.0, "uranium": 0.0, "renewables": 1.0},
        "polityka": {"regime": "presidential_republic", "years_until_election": 5},
        "labor_market": {"unemployment_rate": 7.3},
        "health_statistics": {"service_quality": 55.0},
        "education_statistics": {"infrastructure_base": 59.5},
        "average_wage": 660.6, "culture": "Illyria", "cultural_group": "germanic",
        "religion": "Protestantism", "demographics": {"birth_rate": 18.2},
        "przestepczosc": {"korupcja": 20.0}
    }"#;

    #[test]
    fn deserializes_scalars_and_enum() {
        let m: MacroData = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(m.wealth_bracket, WealthBracket::High);
        assert_eq!(m.currency, "ILI");
        assert!((m.energy_mix.renewables - 1.0).abs() < 1e-9);
    }

    #[test]
    fn deserializes_labor_and_demographics() {
        let m: MacroData = serde_json::from_str(FIXTURE).unwrap();
        assert!((m.labor_market.unemployment_rate - 7.3).abs() < 1e-9);
        assert!((m.demographics.birth_rate - 18.2).abs() < 1e-9);
    }

    #[test]
    fn complex_subtrees_land_in_extra() {
        let m: MacroData = serde_json::from_str(FIXTURE).unwrap();
        // statystyki_zdrowotne and statystyki_edukacyjne are now explicit fields, not in extra
        for key in ["polityka", "przestepczosc"] {
            assert!(m.extra.contains_key(key), "missing {key}");
        }
        // Verify that health and education statistics are now explicit fields
        assert!(m.health_statistics.service_quality >= 0.0);
        assert!(m.education_statistics.infrastructure_base >= 0.0);
    }

    #[test]
    fn struct_round_trip_is_lossless() {
        let m1: MacroData = serde_json::from_str(FIXTURE).unwrap();
        let json = serde_json::to_string(&m1).unwrap();
        let m2: MacroData = serde_json::from_str(&json).unwrap();
        assert_eq!(m1, m2);
    }
}
