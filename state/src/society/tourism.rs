#![allow(missing_docs)]

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::economy::weather::get_region_weather_modifier;
use crate::entities::Company;
use crate::infrastructure::cultural::{CulturalBuilding, CulturalBuildingType};
use crate::registries::enums::Sector;
use crate::society::geography::{LandCategory, LandUseInventory};
use crate::society::housing::{CommercialBuilding, CommercialBuildingType};
use crate::state::climate::ClimateConfig;
use crate::state::{Country, Season};

/// Natural wonder type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WonderType {
    /// Waterfall
    Waterfall,
    /// Geyser
    Geyser,
    /// Beach
    Beach,
    /// Mountain peak
    MountainPeak,
    /// Canyon
    Canyon,
    /// Cave system
    Cave,
    /// Volcanic crater
    VolcanicCrater,
    /// Hot spring
    HotSpring,
    /// Ancient forest
    AncientForest,
    /// Unique geological formation
    GeologicalFormation,
}

/// Natural wonder with environmental health tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NaturalWonder {
    /// Unique wonder ID
    pub id: String,
    /// Wonder name
    pub name: String,
    /// Type of wonder
    pub wonder_type: WonderType,
    /// Region where wonder is located
    pub region_id: String,
    /// Health 0-1 (degrades from pollution)
    pub health: f64,
    /// Recreation value 0-1 (tourism attractiveness)
    pub recreation_value: f64,
    /// Visitor capacity per turn
    pub visitor_capacity: f64,
    /// Current visitors
    pub current_visitors: f64,
    /// Pollution sensitivity 0-1 (how quickly health degrades)
    pub pollution_sensitivity: f64,
    /// Restoration cost per turn
    pub restoration_cost: f64,
}

impl NaturalWonder {
    /// Calculate recreation generation based on health.
    pub fn recreation_generation(&self) -> f64 {
        self.recreation_value * self.health * self.visitor_capacity
    }

    /// Apply pollution damage to the wonder.
    ///
    /// # Arguments
    /// * pollution_level - Local pollution level 0-1
    pub fn apply_pollution(&mut self, pollution_level: f64) {
        let damage = pollution_level * self.pollution_sensitivity * 0.1;
        self.health = (self.health - damage).max(0.2);
    }

    /// Apply restoration to improve health.
    pub fn apply_restoration(&mut self) {
        self.health = (self.health + 0.05).min(1.0);
    }

    /// Process wonder for one turn.
    ///
    /// # Returns
    /// Recreation value generated this turn
    pub fn process_turn(&mut self) -> f64 {
        // Health naturally recovers slightly if not overused
        if self.current_visitors < self.visitor_capacity * 0.8 {
            self.health = (self.health + 0.01).min(1.0);
        }

        // Overuse degrades health
        if self.current_visitors > self.visitor_capacity {
            self.health *= 0.98;
            self.health = self.health.max(0.3);
        }

        self.recreation_generation()
    }
}

/// Tourism destination (region or specific site).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TourismDestination {
    /// Unique destination ID
    pub id: String,
    /// Region ID
    pub region_id: String,
    /// Destination name
    pub name: String,
    /// Natural wonders in this destination
    #[serde(default)]
    pub natural_wonders: Vec<String>,
    /// Infrastructure quality 0-1
    pub infrastructure_quality: f64,
    /// Visitor satisfaction 0-1
    pub visitor_satisfaction: f64,
}

impl TourismDestination {
    /// Process destination for one turn.
    ///
    /// # Arguments
    /// * wonder_recreation - Total recreation from natural wonders
    /// * forest_recreation - Total recreation from forests
    ///
    /// # Returns
    /// Total recreation available
    pub fn process_turn(&mut self, wonder_recreation: f64, forest_recreation: f64) -> f64 {
        let total_recreation = wonder_recreation + forest_recreation;

        // Visitor satisfaction based on capacity utilization.
        // Since accommodation capacity is computed dynamically from buildings,
        // we use the total recreation as a proxy for utilization pressure.
        // High recreation relative to visitor satisfaction indicates overuse.
        if total_recreation > self.visitor_satisfaction * 10_000.0 {
            self.visitor_satisfaction *= 0.95;
        } else if total_recreation < self.visitor_satisfaction * 5_000.0 {
            self.visitor_satisfaction = (self.visitor_satisfaction + 0.02).min(1.0);
        }

        total_recreation * self.visitor_satisfaction
    }
}

/// Per-destination computed demand (Pass 1 output).
#[derive(Debug, Clone)]
pub struct DestinationSettlement {
    /// (company_id, share) — proportional capacity shares for revenue distribution.
    pub company_shares: Vec<(String, f64)>,
    /// Domestic spend for this destination (already debited from citizens).
    pub domestic_spend: f64,
    /// Foreign spend requested for this destination (pre-clamp).
    pub foreign_spend: f64,
}

/// Result of tourism demand computation (Pass 1 output).
#[derive(Debug, Clone, Default)]
pub struct TourismDemandResult {
    /// Per-destination demand entries for sequential settlement.
    pub destinations: Vec<DestinationSettlement>,
    /// Total foreign spend requested (will be clamped sequentially).
    pub total_foreign_requested: f64,
    /// Total domestic spend (already debited from citizen savings).
    pub total_domestic_spend: f64,
}

/// Create a new natural wonder.
///
/// # Arguments
/// * name - Wonder name
/// * wonder_type - Type of wonder
/// * region_id - Region where wonder is located
/// * rng - Random number generator for unique ID
///
/// # Returns
/// New NaturalWonder instance
pub fn create_natural_wonder(
    name: String,
    wonder_type: WonderType,
    region_id: String,
    rng: &mut impl Rng,
) -> NaturalWonder {
    let unique_id: u64 = rng.gen();
    let (recreation_value, visitor_capacity, pollution_sensitivity) = match wonder_type {
        WonderType::Waterfall => (0.9, 10_000.0, 0.8),
        WonderType::Geyser => (0.85, 5_000.0, 0.9),
        WonderType::Beach => (0.8, 50_000.0, 0.7),
        WonderType::MountainPeak => (0.85, 8_000.0, 0.6),
        WonderType::Canyon => (0.9, 15_000.0, 0.5),
        WonderType::Cave => (0.75, 3_000.0, 0.3),
        WonderType::VolcanicCrater => (0.95, 7_000.0, 0.4),
        WonderType::HotSpring => (0.8, 4_000.0, 0.5),
        WonderType::AncientForest => (0.85, 20_000.0, 0.4),
        WonderType::GeologicalFormation => (0.7, 6_000.0, 0.3),
    };

    NaturalWonder {
        id: format!("Wonder-{}-{}", unique_id, wonder_type as u8),
        name,
        wonder_type,
        region_id,
        health: 1.0,
        recreation_value,
        visitor_capacity,
        current_visitors: 0.0,
        pollution_sensitivity,
        restoration_cost: visitor_capacity * 0.1,
    }
}

/// Compute tourism demand for one country for one turn.
///
/// This is Pass 1 of the two-pass tourism mechanism. It:
/// 1. Computes attractiveness and capacity per destination (read-only).
/// 2. Debits domestic savings pro-rata by savings share (Rule 7).
/// 3. Returns `TourismDemandResult` for sequential clamping and settlement.
///
/// Does NOT credit companies. Does NOT touch `GlobalMarket`.
///
/// # Arguments
/// * `country` - Mutable country (owns regions, natural_wonders, tourism_destinations, parks).
/// * `commercial_buildings` - All commercial buildings (scanned for hotels/resorts).
/// * `companies` - All companies (read-only — scanned for hospitality sector).
/// * `season` - Current season for tourism modifiers.
/// * `climate_config` - Climate-season matrix for per-region tourism multipliers.
/// * `heritage_tourism_boost` - Per-region heritage multipliers from heritage processing.
/// * `market_foreign_sector_balance` - Current foreign sector balance (for PPP forex calc).
/// * `total_world_population` - Total world population (for PPP forex calc).
///
/// # Returns
/// `TourismDemandResult` with per-destination settlement data.
///
/// # Rules
/// * FIX #1: Foreign spending debited from GlobalMarket.foreign_sector_balance (not offshore_capital).
/// * FIX #2: Zero capacity → zero spend. No evaporating cash.
/// * FIX #4: domestic_spend capped at total_available_savings.
/// * Rule 7: Domestic spend debited by savings share, not population share.
/// * Rule 2: No magic numbers — forex, spend fraction, pilgrimage all scale dynamically.
/// * Rule 16: Climate and weather tourism multipliers are consumed.
pub fn compute_tourism_demand(
    country: &mut Country,
    commercial_buildings: &[CommercialBuilding],
    companies: &[Company],
    season: Season,
    climate_config: &ClimateConfig,
    heritage_tourism_boost: &std::collections::BTreeMap<String, f64>,
    market_foreign_sector_balance: f64,
    total_world_population: i64,
) -> TourismDemandResult {
    let average_wage = country.macro_indicators.average_wage.max(1.0);

    // ── Dynamic forex multiplier (B1): PPP ratio ──
    // Foreign tourists come from economies with different purchasing power.
    // The forex multiplier is the ratio of foreign_sector_balance per-capita
    // to domestic average_wage. The * 2.0 inverts the * 0.5 seeding ratio
    // to recover the implied rest-of-world GDP.
    let global_gdp_per_capita = if total_world_population > 0 {
        (market_foreign_sector_balance * 2.0) / total_world_population as f64
    } else {
        average_wage
    };
    let forex_multiplier = (global_gdp_per_capita / average_wage).clamp(0.5, 3.0);

    // ── Compute total available citizen savings (FIX #4: liquidity cap) ──
    let total_available_savings: f64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.savings)
        .sum();

    let total_population: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.population)
        .sum();

    // ── Dynamic tourism spend fraction (B2): disposable-income scaling ──
    let avg_savings_per_capita = total_available_savings / total_population.max(1) as f64;
    let tourism_spend_fraction = (0.03 + 0.07 * (avg_savings_per_capita / average_wage).min(1.0))
        .min(0.10);

    // =====================================================================
    // PASS 1A: Process natural wonders and group recreation by region.
    // =====================================================================
    let mut wonder_recreation_by_region: BTreeMap<String, f64> = BTreeMap::new();

    // Build a lookup of region data for pollution and forest area queries.
    // We clone the minimal data we need to avoid borrow conflicts with
    // country.natural_wonders (which we mutate in the same loop).
    let region_data: Vec<(String, f64, LandUseInventory)> = country
        .regions
        .iter()
        .map(|r| {
            (
                r.id.clone(),
                r.elevation_difference_m,
                r.land_use_inventory.clone(),
            )
        })
        .collect();

    for wonder in &mut country.natural_wonders {
        // E2: Apply regional pollution damage before processing.
        if let Some(rd) = region_data.iter().find(|(rid, _, _)| *rid == wonder.region_id) {
            let (_, _, ref land_use) = rd;
            let industrial_area = land_use
                .get_category(LandCategory::Industrial)
                .map(|d| d.area_hectares)
                .unwrap_or(0.0);
            let total_area = land_use.total_area.max(1.0);
            let pollution_level = (industrial_area / total_area).min(1.0);
            wonder.apply_pollution(pollution_level);
        }

        let recreation = wonder.process_turn();
        *wonder_recreation_by_region
            .entry(wonder.region_id.clone())
            .or_insert(0.0) += recreation;
    }

    // =====================================================================
    // PASS 1B: For each destination, compute attractiveness, capacity, demand.
    // =====================================================================
    let mut destination_demands: Vec<DestinationSettlement> = Vec::new();
    let mut total_domestic_spend = 0.0_f64;
    let mut total_foreign_spend = 0.0_f64;

    for dest in country.tourism_destinations.values() {
        // ── Phase B: Attractiveness calculation ──
        let wonder_rec = wonder_recreation_by_region
            .get(&dest.region_id)
            .copied()
            .unwrap_or(0.0);

        // G1: Dynamic forest area from land_use_inventory (not static field).
        let forest_rec = region_data
            .iter()
            .find(|(rid, _, _)| *rid == dest.region_id)
            .and_then(|(_, _, land_use)| land_use.get_category(LandCategory::Forests))
            .map(|f| f.area_hectares * 0.001 * dest.infrastructure_quality)
            .unwrap_or(0.0);

        // Conservation boost from national parks and landscape parks.
        let mut conservation_boost = 0.0_f64;
        for park in &country.national_parks {
            if park.region_id == dest.region_id {
                conservation_boost += park.tourism_revenue_boost();
            }
        }
        for park in &country.landscape_parks {
            if park.region_id == dest.region_id {
                conservation_boost += park.tourism_revenue_boost();
            }
        }

        // ── B3: Pilgrimage boost with physical temple scaling ──
        let mut pilgrimage_boost = 0.0_f64;
        if let Some(region) = country.regions.iter().find(|r| r.id == dest.region_id) {
            if let Some(holy_site) = &region.holy_site {
                let has_active_temple = country.cultural_institutions.iter().any(|b: &CulturalBuilding| {
                    b.region_id == region.id
                        && (b.building_type == CulturalBuildingType::Temple
                            || b.building_type == CulturalBuildingType::Monastery)
                        && b.condition > 0.3
                });
                if has_active_temple {
                    let authority = country
                        .religious_authority_state
                        .authority
                        .get(&holy_site.religion_key)
                        .copied()
                        .unwrap_or(0.0);
                    // Scale by physical temple capacity × condition × average wage.
                    let temple_capacity: f64 = country
                        .cultural_institutions
                        .iter()
                        .filter(|b| {
                            b.region_id == region.id
                                && (b.building_type == CulturalBuildingType::Temple
                                    || b.building_type == CulturalBuildingType::Monastery)
                                && b.condition > 0.3
                        })
                        .map(|b| b.capacity * b.condition)
                        .sum();
                    pilgrimage_boost =
                        holy_site.pilgrimage_attractiveness * authority * temple_capacity * average_wage;
                }
            }
        }

        // ── B5: Climate tourism multiplier from climate-season matrix ──
        let climate_mult = country
            .regions
            .iter()
            .find(|r| r.id == dest.region_id)
            .and_then(|r| climate_config.climate_season_matrix.get(&(r.climate_profile, season)))
            .map(|m| m.tourism_multiplier)
            .unwrap_or(1.0);

        // ── B6: Weather tourism multiplier ──
        let weather_mult = country
            .regions
            .iter()
            .find(|r| r.id == dest.region_id)
            .map(|r| get_region_weather_modifier(&country.weather_state, &r.id).tourism_multiplier)
            .unwrap_or(1.0);

        // ── E3: Heritage tourism boost ──
        let heritage_boost = heritage_tourism_boost
            .get(&dest.region_id)
            .copied()
            .unwrap_or(1.0);

        let total_attractiveness = (wonder_rec + forest_rec + conservation_boost + pilgrimage_boost)
            * climate_mult
            * weather_mult
            * heritage_boost
            * dest.visitor_satisfaction;

        // ── Phase C: Physical capacity check (No Phantom Resorts) ──
        let mut accommodation_capacity = 0.0_f64;

        for b in commercial_buildings {
            if b.micro_region_id.is_empty() {
                continue;
            }
            let in_region = country.regions.iter().any(|r| {
                r.id == dest.region_id
                    && (b.micro_region_id.starts_with(&r.id)
                        || r.micro_regions
                            .values()
                            .any(|mr| mr.id == b.micro_region_id))
            });

            if !in_region {
                continue;
            }

            match b.building_type {
                CommercialBuildingType::Hotel | CommercialBuildingType::Resort => {
                    accommodation_capacity += b.office_capacity;
                }
                CommercialBuildingType::Restaurant | CommercialBuildingType::Casino => {
                    // Service capacity tracked separately (not currently used in visitor cap).
                }
                _ => {}
            }
        }

        // FIX #2: Zero-cacity guard — no hotels means no tourism revenue.
        if accommodation_capacity <= 0.0 {
            continue;
        }

        let effective_capacity = total_attractiveness.min(accommodation_capacity);

        // ── E1: Wire current_visitors to actual tourist counts ──
        let total_wonder_capacity: f64 = country
            .natural_wonders
            .iter()
            .filter(|w| w.region_id == dest.region_id)
            .map(|w| w.visitor_capacity)
            .sum();
        if total_wonder_capacity > 0.0 {
            for wonder in &mut country.natural_wonders {
                if wonder.region_id == dest.region_id {
                    wonder.current_visitors = effective_capacity
                        * (wonder.visitor_capacity / total_wonder_capacity);
                }
            }
        }

        // ── Phase D: Tourist demand & revenue computation ──
        let domestic_demand = effective_capacity * 0.6;
        let theoretical_domestic_spend = domestic_demand * average_wage * tourism_spend_fraction;

        let foreign_demand = effective_capacity * 0.4;
        let foreign_spend = foreign_demand * average_wage * tourism_spend_fraction * forex_multiplier;

        // Identify hospitality companies that own buildings in this region.
        let hospitality_companies: Vec<&crate::entities::Company> = companies
            .iter()
            .filter(|c| c.sector == Sector::Hospitality)
            .filter(|c| {
                commercial_buildings.iter().any(|b| {
                    b.owner_id == c.id
                        && country.regions.iter().any(|r| {
                            r.id == dest.region_id
                                && (b.micro_region_id.starts_with(&r.id)
                                    || r.micro_regions
                                        .values()
                                        .any(|mr| mr.id == b.micro_region_id))
                        })
                })
            })
            .collect();

        // FIX #2: If no hospitality companies, no spending happens.
        if hospitality_companies.is_empty() {
            continue;
        }

        // Compute company capacity shares.
        let mut company_shares: Vec<(String, f64)> = Vec::new();
        let total_company_capacity: f64 = hospitality_companies
            .iter()
            .map(|c| {
                commercial_buildings
                    .iter()
                    .filter(|b| b.owner_id == c.id)
                    .map(|b| b.office_capacity + b.retail_capacity)
                    .sum::<f64>()
            })
            .sum();

        if total_company_capacity <= 0.0 {
            continue;
        }

        for c in &hospitality_companies {
            let cap: f64 = commercial_buildings
                .iter()
                .filter(|b| b.owner_id == c.id)
                .map(|b| b.office_capacity + b.retail_capacity)
                .sum();
            company_shares.push((c.id.clone(), cap / total_company_capacity));
        }

        total_domestic_spend += theoretical_domestic_spend;
        total_foreign_spend += foreign_spend;

        destination_demands.push(DestinationSettlement {
            company_shares,
            domestic_spend: theoretical_domestic_spend,
            foreign_spend,
        });
    }

    // ── FIX #4: Cap total domestic spend at available savings ──
    let capped_domestic_spend = total_domestic_spend.min(total_available_savings);
    let domestic_ratio = if total_domestic_spend > 0.0 {
        capped_domestic_spend / total_domestic_spend
    } else {
        0.0
    };

    // Scale each destination's domestic spend by the cap ratio.
    for dd in &mut destination_demands {
        dd.domestic_spend *= domestic_ratio;
    }

    // =====================================================================
    // PASS 1C: Debit domestic savings pro-rata by savings share (Rule 7).
    // =====================================================================
    if capped_domestic_spend > 0.0 && total_available_savings > 0.0 {
        for region in &mut country.regions {
            for class in region.class_demographics.rural_classes.values_mut() {
                let savings_share = class.savings / total_available_savings;
                let debit = (capped_domestic_spend * savings_share).min(class.savings);
                class.savings -= debit;
                if class.population > 0 {
                    class.savings_per_capita = class.savings / class.population as f64;
                }
            }
            for class in region.class_demographics.urban_classes.values_mut() {
                let savings_share = class.savings / total_available_savings;
                let debit = (capped_domestic_spend * savings_share).min(class.savings);
                class.savings -= debit;
                if class.population > 0 {
                    class.savings_per_capita = class.savings / class.population as f64;
                }
            }
        }
    }

    // Update destination satisfaction based on utilization.
    for dest in country.tourism_destinations.values_mut() {
        let wonder_rec = wonder_recreation_by_region
            .get(&dest.region_id)
            .copied()
            .unwrap_or(0.0);
        let forest_rec = region_data
            .iter()
            .find(|(rid, _, _)| *rid == dest.region_id)
            .and_then(|(_, _, land_use)| land_use.get_category(LandCategory::Forests))
            .map(|f| f.area_hectares * 0.001 * dest.infrastructure_quality)
            .unwrap_or(0.0);
        let total_rec = wonder_rec + forest_rec;
        // Satisfaction adjusts based on recreation pressure.
        if total_rec > dest.visitor_satisfaction * 10_000.0 {
            dest.visitor_satisfaction *= 0.95;
        } else if total_rec < dest.visitor_satisfaction * 5_000.0 {
            dest.visitor_satisfaction = (dest.visitor_satisfaction + 0.02).min(1.0);
        }
    }

    TourismDemandResult {
        destinations: destination_demands,
        total_foreign_requested: total_foreign_spend,
        total_domestic_spend: capped_domestic_spend,
    }
}

/// Settle tourism revenue — credit hospitality companies with clamped amounts.
///
/// This is Pass 2 of the two-pass tourism mechanism. It credits each
/// hospitality company its share of domestic and foreign spending.
/// The `foreign_scaling_ratio` is computed sequentially after clamping
/// the total foreign inflow against `GlobalMarket.foreign_sector_balance`.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (hospitality companies credited).
/// * `demand` - Tourism demand result from `compute_tourism_demand`.
/// * `foreign_scaling_ratio` - Ratio by which foreign spend is scaled (0.0–1.0).
///
/// # Rules
/// * Debit = Credit: The sum of all company credits equals
///   `demand.total_domestic_spend + total_foreign_requested * foreign_scaling_ratio`.
/// * No country mutation, no market mutation.
pub fn settle_tourism_revenue(
    companies: &mut [crate::entities::Company],
    demand: &TourismDemandResult,
    foreign_scaling_ratio: f64,
) {
    for dd in &demand.destinations {
        let dest_revenue = dd.domestic_spend + dd.foreign_spend * foreign_scaling_ratio;
        for (company_id, share) in &dd.company_shares {
            if let Some(company) = companies.iter_mut().find(|c| &c.id == company_id) {
                company.available_cash += dest_revenue * share;
            }
        }
    }
}
