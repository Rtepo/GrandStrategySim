#![allow(missing_docs)]

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(rename = "id_cudu")]
    pub id: String,
    /// Wonder name
    #[serde(rename = "nazwa")]
    pub name: String,
    /// Type of wonder
    #[serde(rename = "typ_cudu")]
    pub wonder_type: WonderType,
    /// Region where wonder is located
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Health 0-1 (degrades from pollution)
    #[serde(rename = "zdrowie")]
    pub health: f64,
    /// Recreation value 0-1 (tourism attractiveness)
    #[serde(rename = "wartość_rekreacyjna")]
    pub recreation_value: f64,
    /// Visitor capacity per turn
    #[serde(rename = "pojemność_odwiedzających")]
    pub visitor_capacity: f64,
    /// Current visitors
    #[serde(rename = "aktualni_odwiedzający")]
    pub current_visitors: f64,
    /// Pollution sensitivity 0-1 (how quickly health degrades)
    #[serde(rename = "wrażliwość_na_zanieczyszczenia")]
    pub pollution_sensitivity: f64,
    /// Restoration cost per turn
    #[serde(rename = "koszt_odnowy")]
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
    #[serde(rename = "id_destynacji")]
    pub id: String,
    /// Region ID
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Destination name
    #[serde(rename = "nazwa")]
    pub name: String,
    /// Natural wonders in this destination
    #[serde(rename = "cuda_naturalne", default)]
    pub natural_wonders: Vec<String>,
    /// Forest areas (hectares)
    #[serde(rename = "obszary_leśne")]
    pub forest_area: f64,
    /// Infrastructure quality 0-1
    #[serde(rename = "jakość_infrastruktury")]
    pub infrastructure_quality: f64,
    /// Accommodation capacity
    #[serde(rename = "pojemność_noclegowa")]
    pub accommodation_capacity: f64,
    /// Visitor satisfaction 0-1
    #[serde(rename = "zadowolenie_odwiedzających")]
    pub visitor_satisfaction: f64,
    /// Marketing budget
    #[serde(rename = "budżet_marketingowy")]
    pub marketing_budget: f64,
}

impl TourismDestination {
    /// Calculate total recreation capacity.
    pub fn total_recreation_capacity(&self) -> f64 {
        // Forest recreation: 0.001 recreation per hectare
        let forest_recreation = self.forest_area * 0.001;
        // Infrastructure multiplier
        forest_recreation * self.infrastructure_quality
    }

    /// Process destination for one turn.
    ///
    /// # Arguments
    /// * wonder_recreation - Total recreation from natural wonders
    ///
    /// # Returns
    /// Total recreation available
    pub fn process_turn(&mut self, wonder_recreation: f64) -> f64 {
        let forest_recreation = self.total_recreation_capacity();
        let total_recreation = wonder_recreation + forest_recreation;

        // Visitor satisfaction based on capacity utilization
        let utilization = total_recreation / (self.accommodation_capacity.max(1.0));
        if utilization > 0.9 {
            self.visitor_satisfaction *= 0.95;
        } else if utilization < 0.5 {
            self.visitor_satisfaction = (self.visitor_satisfaction + 0.02).min(1.0);
        }

        total_recreation * self.visitor_satisfaction
    }
}

/// Tourism industry for a country.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TourismIndustry {
    /// Country operating the tourism industry
    #[serde(rename = "kraj")]
    pub country: String,
    /// Tourism destinations
    #[serde(rename = "destynacje", default)]
    pub destinations: BTreeMap<String, TourismDestination>,
    /// Total recreation consumed
    #[serde(rename = "rekreacja_skonsumowana")]
    pub recreation_consumed: f64,
    /// Revenue generated
    #[serde(rename = "przychód")]
    pub revenue: f64,
    /// Citizen satisfaction boost
    #[serde(rename = "wzrost_zadowolenia_obywateli")]
    pub citizen_satisfaction_boost: f64,
    /// Employment in tourism sector
    #[serde(rename = "zatrudnienie_w_turystyce")]
    pub employment: u32,
}

impl TourismIndustry {
    /// Process tourism industry for one turn.
    ///
    /// # Arguments
    /// * available_recreation - Total recreation available from wonders/forests
    /// * population - Country population
    ///
    /// # Returns
    /// Revenue and satisfaction boost
    pub fn process_turn(&mut self, available_recreation: f64, population: i64) -> (f64, f64) {
        // Consume recreation (limited by demand)
        let recreation_demand = population as f64 * 0.1; // 10% of population seeks recreation
        let consumed = available_recreation.min(recreation_demand);
        self.recreation_consumed = consumed;

        // Generate revenue based on consumption
        let revenue_per_unit = 100.0; // Revenue per recreation unit
        self.revenue = consumed * revenue_per_unit;

        // Calculate satisfaction boost
        let satisfaction_per_capita = consumed / population as f64 * 10.0;
        self.citizen_satisfaction_boost = satisfaction_per_capita.min(0.2);

        // Employment based on revenue
        self.employment = (self.revenue / 50_000.0) as u32;

        (self.revenue, self.citizen_satisfaction_boost)
    }
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

/// Create a new tourism destination.
///
/// # Arguments
/// * region_id - Region ID
/// * name - Destination name
/// * forest_area - Forest area in hectares
/// * accommodation_capacity - Accommodation capacity
/// * rng - Random number generator for unique ID
///
/// # Returns
/// New TourismDestination instance
pub fn create_tourism_destination(
    region_id: String,
    name: String,
    forest_area: f64,
    accommodation_capacity: f64,
    rng: &mut impl Rng,
) -> TourismDestination {
    let unique_id: u64 = rng.gen();
    TourismDestination {
        id: format!("Destination-{}-{}", unique_id, name),
        region_id,
        name,
        natural_wonders: Vec::new(),
        forest_area,
        infrastructure_quality: 0.7,
        accommodation_capacity,
        visitor_satisfaction: 0.8,
        marketing_budget: 0.0,
    }
}

/// Result of one turn of tourism processing.
///
/// Returned by `process_tourism_turn` so the caller can debit
/// `GlobalMarket.offshore_capital` sequentially after parallel tasks.
#[derive(Debug, Clone, Default)]
pub struct TourismTurnResult {
    /// Total foreign tourism spending (to be debited from GlobalMarket.offshore_capital).
    pub foreign_tourism_inflow: f64,
    /// Total domestic tourism spending (debited from citizen savings).
    pub domestic_tourism_spend: f64,
    /// Total revenue credited to hospitality companies.
    pub tourism_revenue: f64,
}

/// Per-destination computed demand (Pass 1 output).
struct DestinationDemand {
    domestic_spend: f64,
    foreign_spend: f64,
    /// (company_id, share) — proportional capacity shares for revenue distribution.
    company_shares: Vec<(String, f64)>,
}

/// Process tourism for one country for one turn.
///
/// # Arguments
/// * `country` - Mutable country (owns regions, natural_wonders, tourism_destinations, parks).
/// * `commercial_buildings` - All commercial buildings (scanned for hotels/resorts).
/// * `companies` - All companies (hospitality companies receive revenue).
/// * `season` - Current season for tourism modifiers.
///
/// # Returns
/// `TourismTurnResult` with foreign inflow (for sequential GlobalMarket debit),
/// domestic spend, and total tourism revenue.
///
/// # Rules
/// * Three-pass design: Pass 1 computes read-only, Pass 2 mutates companies,
///   Pass 3 mutates country. No simultaneous mutable borrows.
/// * FIX #1: Foreign spending debited from GlobalMarket.offshore_capital (not treasury).
/// * FIX #2: Zero capacity → zero spend. No evaporating cash.
/// * FIX #3: Three-pass borrow safety.
/// * FIX #4: domestic_spend capped at total_available_savings.
pub fn process_tourism_turn(
    country: &mut crate::state::Country,
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
    companies: &mut [crate::entities::Company],
    season: crate::state::Season,
) -> TourismTurnResult {
    use crate::registries::enums::Sector;
    use crate::society::housing::CommercialBuildingType;

    let average_wage = country.macro_indicators.average_wage.max(1.0);
    let tourism_spend_fraction = 0.05;
    let forex_multiplier = 1.5;

    // Seasonal modifier for tourism attractiveness.
    let seasonal_modifier = match season {
        crate::state::Season::Winter => 0.4,
        crate::state::Season::Spring => 1.1,
        crate::state::Season::Summer => 1.3,
        crate::state::Season::Autumn => 0.9,
    };

    // =====================================================================
    // PASS 1: COMPUTE (Read-Only — no mutations)
    // =====================================================================

    // Phase A: Process natural wonders and group recreation by region.
    let mut wonder_recreation_by_region: BTreeMap<String, f64> = BTreeMap::new();
    for wonder in &mut country.natural_wonders {
        let recreation = wonder.process_turn();
        *wonder_recreation_by_region
            .entry(wonder.region_id.clone())
            .or_insert(0.0) += recreation;
    }

    // Compute total available citizen savings (FIX #4: liquidity cap).
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

    // Phase B+C+D: For each destination, compute attractiveness, capacity, demand.
    let mut destination_demands: Vec<DestinationDemand> = Vec::new();
    let mut total_domestic_spend = 0.0_f64;
    let mut total_foreign_spend = 0.0_f64;

    for dest in country.tourism_destinations.values() {
        // Phase B: Attractiveness calculation.
        let wonder_rec = wonder_recreation_by_region
            .get(&dest.region_id)
            .copied()
            .unwrap_or(0.0);
        let forest_rec = dest.forest_area * 0.001 * dest.infrastructure_quality;

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

        // Phase 17A: Pilgrimage boost from Holy Sites with active temples.
        let mut pilgrimage_boost = 0.0_f64;
        if let Some(region) = country.regions.iter().find(|r| r.id == dest.region_id) {
            if let Some(holy_site) = &region.holy_site {
                let has_active_temple = country.cultural_institutions.iter().any(|b| {
                    b.region_id == region.id
                        && (b.building_type == crate::infrastructure::cultural::CulturalBuildingType::Temple
                            || b.building_type == crate::infrastructure::cultural::CulturalBuildingType::Monastery)
                        && b.condition > 0.3
                });
                if has_active_temple {
                    let authority = country
                        .religious_authority_state
                        .authority
                        .get(&holy_site.religion_key)
                        .copied()
                        .unwrap_or(0.0);
                    pilgrimage_boost = holy_site.pilgrimage_attractiveness * authority * 5000.0;
                }
            }
        }

        let total_attractiveness =
            (wonder_rec + forest_rec + conservation_boost + pilgrimage_boost) * seasonal_modifier * dest.visitor_satisfaction;

        // Phase C: Physical capacity check (No Phantom Resorts).
        let mut accommodation_capacity = 0.0_f64;
        let mut service_capacity = 0.0_f64;

        // Map building types to tourism capacity.
        for b in commercial_buildings {
            if b.micro_region_id.is_empty() {
                continue;
            }
            // Check if building is in this destination's region.
            // We match by checking if the building's micro_region_id starts with the region_id,
            // or if region_id contains the micro_region_id.
            // For simplicity, we check if any region's micro-regions contain this building.
            let in_region = country.regions.iter().any(|r| {
                r.id == dest.region_id
                    && (b.micro_region_id.starts_with(&r.id)
                        || r.micro_regions.values().any(|mr| mr.id == b.micro_region_id))
            });

            if !in_region {
                continue;
            }

            match b.building_type {
                CommercialBuildingType::Hotel | CommercialBuildingType::Resort => {
                    accommodation_capacity += b.office_capacity;
                }
                CommercialBuildingType::Restaurant | CommercialBuildingType::Casino => {
                    service_capacity += b.retail_capacity;
                }
                _ => {}
            }
        }

        // FIX #2: Zero-capacity guard — no hotels means no tourism revenue.
        if accommodation_capacity <= 0.0 {
            continue;
        }

        let effective_capacity = total_attractiveness.min(accommodation_capacity);

        // Phase D: Tourist demand & revenue computation.
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
                                    || r.micro_regions.values().any(|mr| mr.id == b.micro_region_id))
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

        destination_demands.push(DestinationDemand {
            domestic_spend: theoretical_domestic_spend,
            foreign_spend,
            company_shares,
        });
    }

    // FIX #4: Cap total domestic spend at available savings.
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

    let total_revenue = capped_domestic_spend + total_foreign_spend;

    // =====================================================================
    // PASS 2: MUTATE COMPANIES (Credit Revenue)
    // =====================================================================

    for dd in &destination_demands {
        let dest_revenue = dd.domestic_spend + dd.foreign_spend;
        for (company_id, share) in &dd.company_shares {
            if let Some(company) = companies.iter_mut().find(|c| &c.id == company_id) {
                company.available_cash += dest_revenue * share;
            }
        }
    }

    // =====================================================================
    // PASS 3: MUTATE COUNTRY (Debit Savings + Update State)
    // =====================================================================

    // Debit domestic savings proportionally across all regions.
    if capped_domestic_spend > 0.0 && total_population > 0 {
        for region in &mut country.regions {
            for class in region.class_demographics.rural_classes.values_mut() {
                let class_share = class.population as f64 / total_population as f64;
                let debit = (capped_domestic_spend * class_share).min(class.savings);
                class.savings -= debit;
                if class.population > 0 {
                    class.savings_per_capita = class.savings / class.population as f64;
                }
            }
            for class in region.class_demographics.urban_classes.values_mut() {
                let class_share = class.population as f64 / total_population as f64;
                let debit = (capped_domestic_spend * class_share).min(class.savings);
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
        let forest_rec = dest.forest_area * 0.001 * dest.infrastructure_quality;
        let total_rec = wonder_rec + forest_rec;
        let utilization = total_rec / dest.accommodation_capacity.max(1.0);
        if utilization > 0.9 {
            dest.visitor_satisfaction *= 0.95;
        } else if utilization < 0.5 {
            dest.visitor_satisfaction = (dest.visitor_satisfaction + 0.02).min(1.0);
        }
    }

    TourismTurnResult {
        foreign_tourism_inflow: total_foreign_spend,
        domestic_tourism_spend: capped_domestic_spend,
        tourism_revenue: total_revenue,
    }
}
