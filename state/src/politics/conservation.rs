#![allow(missing_docs)]

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::society::geography::LandCategory;

/// Zoning rule type for conservation areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoningRule {
    /// No industrial expansion allowed
    NoIndustrialExpansion,
    /// Limited industrial expansion
    LimitedIndustrialExpansion,
    /// No construction allowed
    NoConstruction,
    /// Sustainable development only
    SustainableDevelopment,
}

/// Conservation policy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConservationPolicyType {
    /// National Park (strictest protection)
    NationalPark,
    /// Landscape Park (moderate protection)
    LandscapePark,
    /// Nature Reserve
    NatureReserve,
    /// Protected Landscape
    ProtectedLandscape,
}

/// Conservation policy for environmental protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConservationPolicy {
    /// Unique policy ID

    pub id: String,
    /// Policy name

    pub name: String,
    /// Country implementing the policy

    pub country: String,
    /// Policy type

    pub policy_type: ConservationPolicyType,
    /// Region where policy applies

    pub region_id: String,
    /// Zoning rules enforced
    #[serde(default)]
    pub zoning_rules: Vec<ZoningRule>,
    /// Tourism boost multiplier

    pub tourism_boost: f64,
    /// Capitalist discontent generated

    pub capitalist_discontent: f64,
    /// Enforcement level 0-1

    pub enforcement_level: f64,
    /// Maintenance cost per turn

    pub maintenance_cost: f64,
    /// Valid from turn

    pub valid_from: u32,
    /// Valid until turn

    pub valid_until: u32,
}

impl ConservationPolicy {
    /// Check if policy is valid for current turn.
    ///
    /// # Arguments
    /// * current_turn - Current game turn
    ///
    /// # Returns
    /// true if policy is valid
    pub fn is_valid(&self, current_turn: u32) -> bool {
        current_turn >= self.valid_from && current_turn <= self.valid_until
    }

    /// Check if a land use change is allowed under this policy.
    ///
    /// # Arguments
    /// * source_category - Current land category
    /// * target_category - Proposed new land category
    ///
    /// # Returns
    /// true if change is allowed
    pub fn is_land_change_allowed(&self, source_category: LandCategory, target_category: LandCategory) -> bool {
        for rule in &self.zoning_rules {
            match rule {
                ZoningRule::NoIndustrialExpansion => {
                    if target_category == LandCategory::Industrial {
                        return false;
                    }
                }
                ZoningRule::NoConstruction => {
                    if target_category == LandCategory::Urbanized || target_category == LandCategory::Industrial {
                        return false;
                    }
                }
                ZoningRule::SustainableDevelopment => {
                    if target_category == LandCategory::Industrial {
                        return false;
                    }
                }
                ZoningRule::LimitedIndustrialExpansion => {
                    // Allow limited expansion based on enforcement level
                    if target_category == LandCategory::Industrial && self.enforcement_level > 0.7 {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Calculate total capitalist discontent generated.
    pub fn total_capitalist_discontent(&self) -> f64 {
        self.capitalist_discontent * self.enforcement_level
    }
}

/// National Park with strict environmental protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NationalPark {
    /// Unique park ID

    pub id: String,
    /// Park name

    pub name: String,
    /// Country managing the park

    pub country: String,
    /// Region where park is located

    pub region_id: String,
    /// Total area in hectares

    pub total_area: f64,
    /// Protected area in hectares

    pub protected_area: f64,
    /// Zoning rules (strict: no industrial expansion)
    #[serde(default)]
    pub zoning_rules: Vec<ZoningRule>,
    /// Tourism revenue multiplier

    pub tourism_revenue_multiplier: f64,
    /// Capitalist discontent per turn

    pub capitalist_discontent_per_turn: f64,
    /// Ecological health 0-1

    pub ecological_health: f64,
    /// Visitor capacity

    pub visitor_capacity: f64,
    /// Management cost per turn

    pub management_cost: f64,
}

impl NationalPark {
    /// Calculate tourism revenue boost.
    pub fn tourism_revenue_boost(&self) -> f64 {
        self.ecological_health * self.tourism_revenue_multiplier * self.visitor_capacity * 10.0
    }

    /// Process park for one turn.
    ///
    /// # Returns
    /// Tourism revenue and capitalist discontent
    pub fn process_turn(&mut self) -> (f64, f64) {
        // Ecological health naturally improves
        self.ecological_health = (self.ecological_health + 0.01).min(1.0);

        let tourism_revenue = self.tourism_revenue_boost();
        let capitalist_discontent = self.capitalist_discontent_per_turn * self.ecological_health;

        (tourism_revenue, capitalist_discontent)
    }
}

/// Landscape Park with moderate environmental protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LandscapePark {
    /// Unique park ID

    pub id: String,
    /// Park name

    pub name: String,
    /// Country managing the park

    pub country: String,
    /// Region where park is located

    pub region_id: String,
    /// Total area in hectares

    pub total_area: f64,
    /// Protected area in hectares

    pub protected_area: f64,
    /// Zoning rules (moderate: limited industrial expansion)
    #[serde(default)]
    pub zoning_rules: Vec<ZoningRule>,
    /// Tourism revenue multiplier

    pub tourism_revenue_multiplier: f64,
    /// Capitalist discontent per turn

    pub capitalist_discontent_per_turn: f64,
    /// Ecological health 0-1

    pub ecological_health: f64,
    /// Visitor capacity

    pub visitor_capacity: f64,
    /// Management cost per turn

    pub management_cost: f64,
}

impl LandscapePark {
    /// Calculate tourism revenue boost.
    pub fn tourism_revenue_boost(&self) -> f64 {
        self.ecological_health * self.tourism_revenue_multiplier * self.visitor_capacity * 8.0
    }

    /// Process park for one turn.
    ///
    /// # Returns
    /// Tourism revenue and capitalist discontent
    pub fn process_turn(&mut self) -> (f64, f64) {
        // Ecological health naturally improves
        self.ecological_health = (self.ecological_health + 0.005).min(1.0);

        let tourism_revenue = self.tourism_revenue_boost();
        let capitalist_discontent = self.capitalist_discontent_per_turn * self.ecological_health * 0.7;

        (tourism_revenue, capitalist_discontent)
    }
}

/// Create a new national park.
///
/// # Arguments
/// * name - Park name
/// * country - Country managing the park
/// * region_id - Region where park is located
/// * total_area - Total area in hectares
/// * rng - Random number generator for unique ID
///
/// # Returns
/// New NationalPark instance
pub fn create_national_park(
    name: String,
    country: String,
    region_id: String,
    total_area: f64,
    rng: &mut impl Rng,
) -> NationalPark {
    let unique_id: u64 = rng.gen();
    NationalPark {
        id: format!("NationalPark-{}-{}", unique_id, name),
        name,
        country,
        region_id,
        total_area,
        protected_area: total_area * 0.9, // 90% protected
        zoning_rules: vec![ZoningRule::NoIndustrialExpansion, ZoningRule::NoConstruction],
        tourism_revenue_multiplier: 2.0,
        capitalist_discontent_per_turn: 0.05,
        ecological_health: 1.0,
        visitor_capacity: total_area * 0.01, // 1 visitor per hectare
        management_cost: total_area * 0.05,
    }
}

/// Create a new landscape park.
///
/// # Arguments
/// * name - Park name
/// * country - Country managing the park
/// * region_id - Region where park is located
/// * total_area - Total area in hectares
/// * rng - Random number generator for unique ID
///
/// # Returns
/// New LandscapePark instance
pub fn create_landscape_park(
    name: String,
    country: String,
    region_id: String,
    total_area: f64,
    rng: &mut impl Rng,
) -> LandscapePark {
    let unique_id: u64 = rng.gen();
    LandscapePark {
        id: format!("LandscapePark-{}-{}", unique_id, name),
        name,
        country,
        region_id,
        total_area,
        protected_area: total_area * 0.6, // 60% protected
        zoning_rules: vec![ZoningRule::LimitedIndustrialExpansion, ZoningRule::SustainableDevelopment],
        tourism_revenue_multiplier: 1.5,
        capitalist_discontent_per_turn: 0.02,
        ecological_health: 1.0,
        visitor_capacity: total_area * 0.02, // 2 visitors per hectare
        management_cost: total_area * 0.03,
    }
}

/// Process conservation for one turn — parks, policies, and tourism revenue.
///
/// # Arguments
/// * `country` - Mutable reference to the country (for treasury and parks)
/// * `regions` - Mutable slice of regions (for citizen savings debit)
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Park upkeep: Debit treasury.liquid_reserves (pure expenditure, no credit).
/// * Tourism revenue: Debit ClassDemographics.savings in park's region → Credit treasury.liquid_reserves.
/// * Conservation policies: Expire if past valid_until.
/// * Double-entry invariant: Sum of citizen savings debits == tourism revenue credited to treasury.
pub fn process_conservation_turn(
    country: &mut crate::state::Country,
    regions: &mut [crate::society::geography::Region],
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Process National Parks
    for park in &mut country.national_parks {
        let (tourism_revenue, _capitalist_discontent) = park.process_turn();

        // Upkeep: debit treasury (pure expenditure)
        country.budget.liquid_reserves -= park.management_cost;

        // Tourism revenue: debit citizen savings → credit treasury
        if tourism_revenue > 0.0 {
            if let Some(region) = regions.iter_mut().find(|r| r.id == park.region_id) {
                let total_pop: i64 = region.class_demographics.rural_classes.values()
                    .chain(region.class_demographics.urban_classes.values())
                    .map(|c| c.population)
                    .sum();

                if total_pop > 0 {
                    let per_capita_spend = tourism_revenue / total_pop as f64;
                    let mut total_debited = 0.0;

                    for class in region.class_demographics.rural_classes.values_mut() {
                        let debit = per_capita_spend * class.population as f64;
                        class.savings = (class.savings - debit).max(0.0);
                        if class.population > 0 {
                            class.savings_per_capita = class.savings / class.population as f64;
                        }
                        total_debited += debit;
                    }
                    for class in region.class_demographics.urban_classes.values_mut() {
                        let debit = per_capita_spend * class.population as f64;
                        class.savings = (class.savings - debit).max(0.0);
                        if class.population > 0 {
                            class.savings_per_capita = class.savings / class.population as f64;
                        }
                        total_debited += debit;
                    }

                    // Credit treasury with actual debited amount (handles max(0.0) clipping)
                    country.budget.liquid_reserves += total_debited;
                }
            }
        }

        messages.push(format!(
            "[PARK] {} upkeep: -{:.0}, tourism: +{:.0}",
            park.name, park.management_cost, tourism_revenue
        ));
    }

    // Process Landscape Parks
    for park in &mut country.landscape_parks {
        let (tourism_revenue, _capitalist_discontent) = park.process_turn();

        // Upkeep: debit treasury
        country.budget.liquid_reserves -= park.management_cost;

        // Tourism revenue: debit citizen savings → credit treasury
        if tourism_revenue > 0.0 {
            if let Some(region) = regions.iter_mut().find(|r| r.id == park.region_id) {
                let total_pop: i64 = region.class_demographics.rural_classes.values()
                    .chain(region.class_demographics.urban_classes.values())
                    .map(|c| c.population)
                    .sum();

                if total_pop > 0 {
                    let per_capita_spend = tourism_revenue / total_pop as f64;
                    let mut total_debited = 0.0;

                    for class in region.class_demographics.rural_classes.values_mut() {
                        let debit = per_capita_spend * class.population as f64;
                        class.savings = (class.savings - debit).max(0.0);
                        if class.population > 0 {
                            class.savings_per_capita = class.savings / class.population as f64;
                        }
                        total_debited += debit;
                    }
                    for class in region.class_demographics.urban_classes.values_mut() {
                        let debit = per_capita_spend * class.population as f64;
                        class.savings = (class.savings - debit).max(0.0);
                        if class.population > 0 {
                            class.savings_per_capita = class.savings / class.population as f64;
                        }
                        total_debited += debit;
                    }

                    country.budget.liquid_reserves += total_debited;
                }
            }
        }

        messages.push(format!(
            "[PARK] {} upkeep: -{:.0}, tourism: +{:.0}",
            park.name, park.management_cost, tourism_revenue
        ));
    }

    // Expire old conservation policies
    country.conservation_policies.retain(|p| p.is_valid(current_turn));

    messages
}
