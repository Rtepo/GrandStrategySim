//! Society and cultural systems used during world generation and later
//! social-simulation phases.

#![allow(missing_docs)]

pub mod cadastre;
pub mod charities;
pub mod culture_registry;
pub mod cultures;
pub mod disasters;
pub mod factional_domains;
pub mod geography;
pub mod geography_config;
pub mod housing;
pub mod planet;
pub mod real_estate_market;
pub mod religious_authority;
pub mod tourism;
pub mod urbanization;

pub use housing::{
    CommercialBuilding, CommercialBuildingType, CommercialInventory, HousingBuilding,
    HousingInventory, HousingSlots, HousingType, UtilityConnections,
};
pub use tourism::{
    compute_tourism_demand, create_natural_wonder, settle_tourism_revenue, DestinationSettlement,
    NaturalWonder, TourismDemandResult, TourismDestination, WonderType,
};

// ============================================================================
// v4.31.0 (TASK-032): pastoral nomadics + Society KPI history
// ============================================================================

use serde::{Deserialize, Serialize};

/// One turn of the twelve Society KPI card metrics (plus the nomadic
/// livestock herd figure powering the Nomadic card).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SocietyKpiSample {
    pub population: f64,
    pub average_age: f64,
    pub median_age: f64,
    pub birth_rate: f64,
    pub death_rate: f64,
    /// Estimated total fertility rate (~1.3 × CBR%).
    pub fertility: f64,
    pub life_expectancy: f64,
    pub literacy_rate: f64,
    pub higher_education_rate: f64,
    pub net_migration_rate: f64,
    pub urban_share: f64,
    pub rural_share: f64,
    pub nomad_share: f64,
    /// Livestock headcount herded by the nomadic population.
    pub nomad_herds: f64,
}

const SOCIETY_KPI_MAX_TURNS: usize = 50;

/// Rolling per-turn history of the Society KPI cards — sparklines and
/// the click-to-pinned trend chart read this series (engine is the
/// single source of truth; capped at 50 entries like BankingHistory).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SocietyKpiHistory {
    #[serde(default)]
    pub turns: Vec<u32>,
    #[serde(default)]
    pub samples: Vec<SocietyKpiSample>,
}

impl SocietyKpiHistory {
    /// Record a new turn's KPI sample, dropping the oldest when full.
    pub fn record(&mut self, turn: u32, s: SocietyKpiSample) {
        self.turns.push(turn);
        self.samples.push(s);
        while self.turns.len() > SOCIETY_KPI_MAX_TURNS {
            self.turns.remove(0);
            self.samples.remove(0);
        }
    }
}

/// v4.31.0 (TASK-032): per-region pastoral-terrain weight — steppe,
/// arid, highland and open continental pasture support nomadic
/// livestock herding; dense temperate lowland does not.
fn region_pastoral_weight(r: &geography::Region) -> f64 {
    use geography::{Climate, ClimateProfile};
    let mut w: f64 = match r.climate_profile {
        ClimateProfile::Desert => 1.0,
        ClimateProfile::Continental => 0.7,
        ClimateProfile::Mountainous => 0.8,
        ClimateProfile::SubTropical => 0.3,
        ClimateProfile::Temperate => 0.15,
        _ => 0.0,
    };
    if matches!(r.climate, Climate::Desert) {
        w += 0.4;
    }
    if r.elevation_difference_m > 800.0 {
        w += 0.3;
    } else if r.elevation_difference_m > 300.0 {
        w += 0.15;
    }
    w.min(1.0)
}

/// Population-weighted pastoral intensity across the country (0–1).
/// Steppe nations score high → nomadic share lands in the 3%–25% band.
pub fn pastoral_intensity(country: &crate::state::Country) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for r in &country.regions {
        if r.node_type != geography::NodeType::LandRegion {
            continue;
        }
        let pop = r.population.max(0) as f64;
        num += region_pastoral_weight(r) * pop;
        den += pop;
    }
    if den <= 0.0 {
        return 0.0;
    }
    (num / den).clamp(0.0, 1.0)
}

/// Per-turn pastoral dynamics (called once per country per turn):
/// 1. Seeds `demographics.nomads` where pastoral terrain exists —
///    3%–25% of population scaled by pastoral intensity (was +0.00%
///    everywhere because nothing ever initialized the share).
/// 2. Nomads herd livestock — `nomad_herds` tracks ~2.4 head per
///    pastoralist (sheep, goats, horses, camels).
/// 3. Gradual sedentarization — urbanization, development and land
///    enclosures draw pastoralists into the settled rural share.
pub fn update_pastoral_dynamics(country: &mut crate::state::Country) {
    let intensity = pastoral_intensity(country);
    let population = country.budget.population as f64;
    let mean_dev = if country.regions.is_empty() {
        0.0
    } else {
        country
            .regions
            .iter()
            .map(|r| r.development_level)
            .sum::<f64>()
            / country.regions.len() as f64
    };
    let demo = &mut country.macro_indicators.demographics;
    if demo.nomads <= 0.0 {
        if intensity > 0.05 {
            demo.nomads = (0.03 + intensity * 0.22).clamp(0.03, 0.25);
        } else {
            return;
        }
    }
    // Sedentarization — 0.2%–2.0% of the remaining nomad share per turn,
    // pulled by urbanization and development.
    let urban = demo.city_urban.clamp(0.0, 1.0);
    let settle_rate = (0.002 + urban * 0.004 + mean_dev * 0.004).min(0.02);
    let settled = demo.nomads * settle_rate;
    demo.nomads = (demo.nomads - settled).max(0.0);
    demo.rural = (demo.rural + settled).min((1.0 - demo.city_urban).max(0.0));
    demo.nomad_herds = demo.nomads * population * 2.4;
}
