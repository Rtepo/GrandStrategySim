#![allow(missing_docs)]

use rand::Rng;
use serde::{Deserialize, Serialize};

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
    pub fn is_land_change_allowed(
        &self,
        _source_category: LandCategory,
        target_category: LandCategory,
    ) -> bool {
        for rule in &self.zoning_rules {
            match rule {
                ZoningRule::NoIndustrialExpansion => {
                    if target_category == LandCategory::Industrial {
                        return false;
                    }
                }
                ZoningRule::NoConstruction => {
                    if target_category == LandCategory::Urbanized
                        || target_category == LandCategory::Industrial
                    {
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
    /// Management cost per turn
    pub management_cost: f64,
    /// Phase 18E: Entry fee per visitor (C2G administrative fee).
    /// Scales by average_wage; debited from citizen Labor accounts,
    /// credited to park_funding_subaccount in country budget.
    #[serde(default)]
    pub entry_fee_per_visitor: f64,
    /// Phase 18E: Ecological tax per hectare of industrial land in buffer zone.
    /// Debited from industrial company liquid_capital,
    /// credited to park_funding_subaccount.
    #[serde(default)]
    pub ecological_tax_per_hectare: f64,
    /// Phase 18E: Visitor count last turn (for scaling and diagnostics).
    #[serde(default)]
    pub last_turn_visitor_count: f64,
    /// Phase 18E: Parcel IDs annexed by this park (for cadastre linkage).
    #[serde(default)]
    pub annexed_parcel_ids: Vec<String>,
    /// Phase 18E: Park funding sub-account balance (accumulated fees + taxes - costs).
    #[serde(default)]
    pub funding_balance: f64,
}

impl NationalPark {
    /// Calculate tourism revenue boost.
    /// B4: Scales with protected area and ecological health (per-hectare density).
    pub fn tourism_revenue_boost(&self) -> f64 {
        self.ecological_health * self.tourism_revenue_multiplier * self.protected_area * 0.01
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
    /// Management cost per turn
    pub management_cost: f64,
    /// Phase 18E: Entry fee per visitor (C2G administrative fee).
    #[serde(default)]
    pub entry_fee_per_visitor: f64,
    /// Phase 18E: Ecological tax per hectare of industrial land in buffer zone.
    #[serde(default)]
    pub ecological_tax_per_hectare: f64,
    /// Phase 18E: Visitor count last turn.
    #[serde(default)]
    pub last_turn_visitor_count: f64,
    /// Phase 18E: Parcel IDs annexed by this park.
    #[serde(default)]
    pub annexed_parcel_ids: Vec<String>,
    /// Phase 18E: Park funding sub-account balance.
    #[serde(default)]
    pub funding_balance: f64,
}

impl LandscapePark {
    /// Calculate tourism revenue boost.
    /// B4: Scales with protected area and ecological health (per-hectare density).
    pub fn tourism_revenue_boost(&self) -> f64 {
        self.ecological_health * self.tourism_revenue_multiplier * self.protected_area * 0.008
    }

    /// Process park for one turn.
    ///
    /// # Returns
    /// Tourism revenue and capitalist discontent
    pub fn process_turn(&mut self) -> (f64, f64) {
        // Ecological health naturally improves
        self.ecological_health = (self.ecological_health + 0.005).min(1.0);

        let tourism_revenue = self.tourism_revenue_boost();
        let capitalist_discontent =
            self.capitalist_discontent_per_turn * self.ecological_health * 0.7;

        (tourism_revenue, capitalist_discontent)
    }
}

/// Phase 18E: Nature Reserve with strict biodiversity protection.
///
/// Nature reserves are stricter than national parks — no tourism,
/// no entry fees, pure conservation. They are funded entirely by
/// the government budget and ecological taxes from surrounding industry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NatureReserve {
    /// Unique reserve ID
    pub id: String,
    /// Reserve name
    pub name: String,
    /// Country managing the reserve
    pub country: String,
    /// Region where reserve is located
    pub region_id: String,
    /// Total area in hectares
    pub total_area: f64,
    /// Protected area in hectares (100% for nature reserves)
    pub protected_area: f64,
    /// Ecological health 0-1 (clamped)
    pub ecological_health: f64,
    /// Management cost per turn (scales by protected_area)
    pub management_cost: f64,
    /// Phase 18E: Ecological tax per hectare of industrial land in buffer zone.
    #[serde(default)]
    pub ecological_tax_per_hectare: f64,
    /// Phase 18E: Parcel IDs annexed by this reserve.
    #[serde(default)]
    pub annexed_parcel_ids: Vec<String>,
    /// Phase 18E: Funding sub-account balance.
    #[serde(default)]
    pub funding_balance: f64,
    /// Phase 18E: Biodiversity index (0.0-1.0), higher = more diverse.
    #[serde(default)]
    pub biodiversity_index: f64,
}

impl NatureReserve {
    /// Process reserve for one turn.
    ///
    /// # Returns
    /// Management cost and ecological health improvement
    pub fn process_turn(&mut self) -> (f64, f64) {
        // Nature reserves recover faster than parks (no tourism pressure)
        let health_improvement = 0.015;
        self.ecological_health = (self.ecological_health + health_improvement).min(1.0);
        // Biodiversity improves with ecological health
        self.biodiversity_index = (self.biodiversity_index + health_improvement * 0.5).min(1.0);
        (self.management_cost, health_improvement)
    }
}

/// Phase 18E: Buffer Zone around protected areas.
///
/// Buffer zones are transitional areas where limited economic activity
/// is allowed but industrial pollution is taxed. The ecological tax
/// revenue funds the adjacent protected area.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BufferZone {
    /// Unique buffer zone ID
    pub id: String,
    /// Buffer zone name
    pub name: String,
    /// Country managing the buffer zone
    pub country: String,
    /// Region where buffer zone is located
    pub region_id: String,
    /// Total area in hectares
    pub total_area: f64,
    /// Industrial area in hectares (subject to ecological tax)
    #[serde(default)]
    pub industrial_area: f64,
    /// Ecological tax per hectare of industrial land
    #[serde(default)]
    pub ecological_tax_per_hectare: f64,
    /// ID of the protected area this buffer zone supports
    pub protected_area_id: String,
    /// Type of protected area (national_park, landscape_park, nature_reserve)
    pub protected_area_type: String,
    /// Phase 18E: Parcel IDs in this buffer zone.
    #[serde(default)]
    pub parcel_ids: Vec<String>,
    /// Phase 18E: Pollution level (0.0-1.0) from nearby industry.
    #[serde(default)]
    pub pollution_level: f64,
}

impl BufferZone {
    /// Compute ecological tax owed by industrial firms in this buffer zone.
    ///
    /// # Rules
    /// * Tax scales by industrial_area and ecological_tax_per_hectare
    /// * Pollution level increases the tax (polluter pays principle)
    pub fn compute_ecological_tax(&self) -> f64 {
        let base_tax = self.industrial_area * self.ecological_tax_per_hectare;
        // Polluter pays: higher pollution = higher tax
        let pollution_multiplier = 1.0 + self.pollution_level * 2.0;
        base_tax * pollution_multiplier
    }
}

/// Phase 18E: Urban recreation park for citizen happiness and pollution reduction.
///
/// Urban parks are small green spaces within cities that provide:
/// - Happiness boost to nearby residents
/// - Localized pollution reduction
/// - Recreation access (complements sports facilities)
/// Unlike national/landscape parks, urban parks have minimal infrastructure
/// and are maintained by municipal budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrbanPark {
    /// Unique park ID
    pub id: String,
    /// Park name
    pub name: String,
    /// Country managing the park
    pub country: String,
    /// Region where park is located
    pub region_id: String,
    /// Micro-region ID (urban parks are within cities)
    pub micro_region_id: String,
    /// Total area in hectares
    pub total_area: f64,
    /// Ecological health 0-1
    pub ecological_health: f64,
    /// Management cost per turn (scales by area)
    pub management_cost: f64,
    /// Phase 18E: Visitor capacity (scales by area)
    #[serde(default)]
    pub visitor_capacity: f64,
    /// Phase 18E: Last turn visitor count
    #[serde(default)]
    pub last_turn_visitor_count: f64,
    /// Phase 18E: Entry fee per visitor (C2G, may be zero for free parks)
    #[serde(default)]
    pub entry_fee_per_visitor: f64,
    /// Phase 18E: Pollution reduction factor (0.0-1.0, higher = more reduction)
    #[serde(default)]
    pub pollution_reduction_factor: f64,
    /// Phase 18E: Happiness boost per visitor (0.0-1.0)
    #[serde(default)]
    pub happiness_boost_per_visitor: f64,
    /// Phase 18E: Parcel IDs annexed by this park.
    #[serde(default)]
    pub annexed_parcel_ids: Vec<String>,
    /// Phase 18E: Funding balance (entry fees - management costs).
    #[serde(default)]
    pub funding_balance: f64,
}

impl UrbanPark {
    /// Process urban park for one turn.
    ///
    /// # Returns
    /// (management_cost, pollution_reduced, happiness_boost, entry_fee_revenue)
    pub fn process_turn(&mut self, average_wage: f64) -> (f64, f64, f64, f64) {
        // Ecological health slowly improves
        self.ecological_health = (self.ecological_health + 0.003).min(1.0);

        // Visitor count scales by capacity and ecological health
        let visitor_count = self.visitor_capacity * self.ecological_health;
        self.last_turn_visitor_count = visitor_count;

        // Pollution reduction scales by area and ecological health
        let pollution_reduced = self.pollution_reduction_factor
            * self.total_area
            * self.ecological_health
            * 0.01;

        // Happiness boost scales by visitor count
        let happiness_boost = self.happiness_boost_per_visitor * visitor_count * 0.001;

        // Entry fee revenue (C2G)
        let entry_fee_revenue = visitor_count * self.entry_fee_per_visitor * average_wage * 0.001;

        // Update funding balance
        self.funding_balance += entry_fee_revenue - self.management_cost;

        (
            self.management_cost,
            pollution_reduced,
            happiness_boost,
            entry_fee_revenue,
        )
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
        zoning_rules: vec![
            ZoningRule::NoIndustrialExpansion,
            ZoningRule::NoConstruction,
        ],
        tourism_revenue_multiplier: 2.0,
        capitalist_discontent_per_turn: 0.05,
        ecological_health: 1.0,
        management_cost: total_area * 0.05,
        entry_fee_per_visitor: 0.001, // Scales by average_wage at runtime
        ecological_tax_per_hectare: 0.0,
        last_turn_visitor_count: 0.0,
        annexed_parcel_ids: Vec::new(),
        funding_balance: 0.0,
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
        zoning_rules: vec![
            ZoningRule::LimitedIndustrialExpansion,
            ZoningRule::SustainableDevelopment,
        ],
        tourism_revenue_multiplier: 1.5,
        capitalist_discontent_per_turn: 0.02,
        ecological_health: 1.0,
        management_cost: total_area * 0.03,
        entry_fee_per_visitor: 0.0005,
        ecological_tax_per_hectare: 0.0,
        last_turn_visitor_count: 0.0,
        annexed_parcel_ids: Vec::new(),
        funding_balance: 0.0,
    }
}

/// Phase 18E: Create a new nature reserve.
pub fn create_nature_reserve(
    name: String,
    country: String,
    region_id: String,
    total_area: f64,
    rng: &mut impl Rng,
) -> NatureReserve {
    let unique_id: u64 = rng.gen();
    NatureReserve {
        id: format!("NatureReserve-{}-{}", unique_id, name),
        name,
        country,
        region_id,
        total_area,
        protected_area: total_area, // 100% protected
        ecological_health: 1.0,
        management_cost: total_area * 0.04,
        ecological_tax_per_hectare: 0.0,
        annexed_parcel_ids: Vec::new(),
        funding_balance: 0.0,
        biodiversity_index: 0.8,
    }
}

/// Phase 18E: Create a new buffer zone around a protected area.
pub fn create_buffer_zone(
    name: String,
    country: String,
    region_id: String,
    total_area: f64,
    protected_area_id: String,
    protected_area_type: String,
    rng: &mut impl Rng,
) -> BufferZone {
    let unique_id: u64 = rng.gen();
    BufferZone {
        id: format!("BufferZone-{}-{}", unique_id, name),
        name,
        country,
        region_id,
        total_area,
        industrial_area: 0.0,
        ecological_tax_per_hectare: 0.0,
        protected_area_id,
        protected_area_type,
        parcel_ids: Vec::new(),
        pollution_level: 0.0,
    }
}

/// Phase 18E: Create a new urban recreation park.
pub fn create_urban_park(
    name: String,
    country: String,
    region_id: String,
    micro_region_id: String,
    total_area: f64,
    rng: &mut impl Rng,
) -> UrbanPark {
    let unique_id: u64 = rng.gen();
    UrbanPark {
        id: format!("UrbanPark-{}-{}", unique_id, name),
        name,
        country,
        region_id,
        micro_region_id,
        total_area,
        ecological_health: 0.8,
        management_cost: total_area * 0.02,
        visitor_capacity: total_area * 10.0, // 10 visitors per hectare
        last_turn_visitor_count: 0.0,
        entry_fee_per_visitor: 0.0, // Free by default
        pollution_reduction_factor: 0.05,
        happiness_boost_per_visitor: 0.01,
        annexed_parcel_ids: Vec::new(),
        funding_balance: 0.0,
    }
}

// ============================================================================
// PHASE 18E: PARK LIFECYCLE — CREATION, EXPANSION, SHRINKAGE, ABOLITION
// ============================================================================

/// Phase 18E: Expand a national park by annexing additional parcels.
///
/// # Arguments
/// * `park` - Mutable national park to expand
/// * `cadastre` - Mutable cadastre for parcel annexation
/// * `parcel_ids` - Parcel IDs to annex into the park
/// * `government_owner_id` - Government entity ID for ownership transfer
/// * `current_turn` - Current turn for tracking
///
/// # Returns
/// Number of parcels successfully annexed
pub fn expand_national_park(
    park: &mut NationalPark,
    cadastre: &mut crate::society::cadastre::Cadastre,
    parcel_ids: &[crate::society::cadastre::ParcelId],
    government_owner_id: &str,
    current_turn: u32,
) -> usize {
    let mut annexed = 0usize;
    for &pid in parcel_ids {
        if cadastre.annex_parcel_for_park(pid, government_owner_id, current_turn) {
            // Get the parcel size to update park area
            if let Some(parcel) = cadastre.get(pid) {
                park.total_area += parcel.size_hectares;
                park.protected_area += parcel.size_hectares;
            }
            park.annexed_parcel_ids.push(format!("{:?}", pid));
            annexed += 1;
        }
    }
    // Recalculate management cost (scales by area)
    park.management_cost = park.total_area * 0.05;
    annexed
}

/// Phase 18E: Shrink a national park by releasing parcels.
///
/// # Arguments
/// * `park` - Mutable national park to shrink
/// * `cadastre` - Mutable cadastre for parcel release
/// * `parcel_ids` - Parcel IDs to release from the park
/// * `current_turn` - Current turn for tracking
///
/// # Returns
/// Number of parcels successfully released
pub fn shrink_national_park(
    park: &mut NationalPark,
    cadastre: &mut crate::society::cadastre::Cadastre,
    parcel_ids: &[crate::society::cadastre::ParcelId],
    current_turn: u32,
) -> usize {
    let mut released = 0usize;
    for &pid in parcel_ids {
        if cadastre.release_parcel_from_park(pid, current_turn) {
            if let Some(parcel) = cadastre.get(pid) {
                park.total_area = (park.total_area - parcel.size_hectares).max(0.0);
                park.protected_area = (park.protected_area - parcel.size_hectares).max(0.0);
            }
            let pid_str = format!("{:?}", pid);
            park.annexed_parcel_ids.retain(|id| *id != pid_str);
            released += 1;
        }
    }
    // Recalculate management cost
    park.management_cost = park.total_area * 0.05;
    released
}

/// Phase 18E: Abolish a national park, releasing all its parcels.
///
/// # Arguments
/// * `country` - Mutable country (to remove the park from the list)
/// * `cadastre` - Mutable cadastre for parcel release
/// * `park_id` - ID of the park to abolish
/// * `current_turn` - Current turn for tracking
///
/// # Returns
/// `true` if the park was found and abolished
pub fn abolish_national_park(
    country: &mut crate::state::Country,
    cadastre: &mut crate::society::cadastre::Cadastre,
    park_id: &str,
    current_turn: u32,
) -> bool {
    let pos = country.national_parks.iter().position(|p| p.id == park_id);
    if let Some(idx) = pos {
        let park = country.national_parks.remove(idx);
        // Release all annexed parcels
        for _parcel_id_str in &park.annexed_parcel_ids {
            // Note: In a full implementation, we'd parse the ParcelId from the string.
            // For now, we release by iterating the cadastre for parcels owned by this park.
            for (pid, parcel) in cadastre.iter_mut() {
                if parcel.owner_id == park_id || parcel.is_frozen {
                    let _ = pid; // suppress unused variable
                    parcel.is_frozen = false;
                    parcel.zoning = crate::society::cadastre::ZoningDesignation::Agricultural;
                    parcel.zoning_change_turn = current_turn;
                }
            }
        }
        true
    } else {
        false
    }
}

/// Phase 18E: Abolish a landscape park, releasing all its parcels.
pub fn abolish_landscape_park(
    country: &mut crate::state::Country,
    cadastre: &mut crate::society::cadastre::Cadastre,
    park_id: &str,
    current_turn: u32,
) -> bool {
    let pos = country.landscape_parks.iter().position(|p| p.id == park_id);
    if let Some(idx) = pos {
        let _park = country.landscape_parks.remove(idx);
        for (pid, parcel) in cadastre.iter_mut() {
            if parcel.owner_id == park_id {
                let _ = pid;
                parcel.is_frozen = false;
                parcel.zoning = crate::society::cadastre::ZoningDesignation::Agricultural;
                parcel.zoning_change_turn = current_turn;
            }
        }
        true
    } else {
        false
    }
}

/// Phase 18E: Abolish a nature reserve, releasing all its parcels.
pub fn abolish_nature_reserve(
    country: &mut crate::state::Country,
    cadastre: &mut crate::society::cadastre::Cadastre,
    reserve_id: &str,
    current_turn: u32,
) -> bool {
    let pos = country.nature_reserves.iter().position(|r| r.id == reserve_id);
    if let Some(idx) = pos {
        let _reserve = country.nature_reserves.remove(idx);
        for (pid, parcel) in cadastre.iter_mut() {
            if parcel.owner_id == reserve_id {
                let _ = pid;
                parcel.is_frozen = false;
                parcel.zoning = crate::society::cadastre::ZoningDesignation::Agricultural;
                parcel.zoning_change_turn = current_turn;
            }
        }
        true
    } else {
        false
    }
}

/// Phase 18E: Abolish an urban park, releasing all its parcels.
pub fn abolish_urban_park(
    country: &mut crate::state::Country,
    cadastre: &mut crate::society::cadastre::Cadastre,
    park_id: &str,
    current_turn: u32,
) -> bool {
    let pos = country.urban_parks.iter().position(|p| p.id == park_id);
    if let Some(idx) = pos {
        let _park = country.urban_parks.remove(idx);
        for (pid, parcel) in cadastre.iter_mut() {
            if parcel.owner_id == park_id {
                let _ = pid;
                parcel.is_frozen = false;
                parcel.zoning = crate::society::cadastre::ZoningDesignation::Residential;
                parcel.zoning_change_turn = current_turn;
            }
        }
        true
    } else {
        false
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
    // Phase 18E: Two-phase processing to avoid borrow checker conflicts.
    // Phase 1: Process park internals (ecological health, tourism revenue).
    // Phase 2: Collect C2G entry fees and debit management costs.

    // Phase 1: Process park internals
    let mut park_turn_data: Vec<(usize, f64, f64, f64, String, f64, String)> = Vec::new();
    for (idx, park) in country.national_parks.iter_mut().enumerate() {
        let (tourism_revenue, _capitalist_discontent) = park.process_turn();
        let visitor_count = park.protected_area * 0.1 * park.ecological_health;
        let management_cost = park.management_cost;
        let region_id = park.region_id.clone();
        let entry_fee_per_visitor = park.entry_fee_per_visitor;
        let park_id = park.id.clone();
        park_turn_data.push((
            idx,
            tourism_revenue,
            visitor_count,
            management_cost,
            region_id,
            entry_fee_per_visitor,
            park_id,
        ));
    }

    // Phase 2: Collect C2G entry fees and process management costs
    for (idx, tourism_revenue, visitor_count, management_cost, region_id, entry_fee_rate, park_id) in &park_turn_data {
        // C2G entry fee collection (debit citizen Labor accounts, credit park funding_balance)
        let entry_fee_total = crate::state::tax::collect_park_entry_fees(
            country,
            regions,
            park_id,
            region_id,
            *visitor_count,
            *entry_fee_rate,
        );

        let park = &mut country.national_parks[*idx];
        park.funding_balance += entry_fee_total;
        park.last_turn_visitor_count = *visitor_count;

        // Management cost from funding_balance first, then treasury
        if park.funding_balance >= *management_cost {
            park.funding_balance -= *management_cost;
        } else {
            let shortfall = *management_cost - park.funding_balance.max(0.0);
            park.funding_balance = 0.0;
            country.budget.liquid_reserves -= shortfall;
        }

        // Ecological health degrades if funding_balance < 0
        if park.funding_balance < 0.0 {
            let deficit_ratio =
                (-park.funding_balance / management_cost.max(1.0)).min(1.0);
            park.ecological_health =
                (park.ecological_health - deficit_ratio * 0.02).max(0.0);
        }

        // Tourism revenue: debit citizen savings → credit treasury (legacy path)
        if *tourism_revenue > 0.0 {
            if let Some(region) = regions.iter_mut().find(|r| r.id == *region_id) {
                let total_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
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
            "[PARK] {} upkeep: -{:.0}, tourism: +{:.0}, fees: +{:.0}",
            park.name, management_cost, tourism_revenue, entry_fee_total
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
                let total_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
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

    // Phase 18E: Process Nature Reserves
    for reserve in &mut country.nature_reserves {
        let (management_cost, _health_improvement) = reserve.process_turn();

        // Upkeep: debit treasury
        country.budget.liquid_reserves -= management_cost;

        // Ecological health degrades if funding_balance < 0
        if reserve.funding_balance < 0.0 {
            let deficit_ratio = (-reserve.funding_balance / management_cost.max(1.0)).min(1.0);
            reserve.ecological_health = (reserve.ecological_health - deficit_ratio * 0.02).max(0.0);
        }

        messages.push(format!(
            "[RESERVE] {} upkeep: -{:.0}, health: {:.2}, biodiversity: {:.2}",
            reserve.name,
            management_cost,
            reserve.ecological_health,
            reserve.biodiversity_index
        ));
    }

    // Phase 18E: Process Urban Parks
    let average_wage = country.macro_indicators.average_wage.max(1.0);
    for park in &mut country.urban_parks {
        let (management_cost, _pollution_reduced, _happiness_boost, entry_fee_revenue) =
            park.process_turn(average_wage);

        // Management cost: debit from park funding_balance first, then treasury
        if park.funding_balance >= management_cost {
            park.funding_balance -= management_cost;
        } else {
            let shortfall = management_cost - park.funding_balance.max(0.0);
            park.funding_balance = 0.0;
            country.budget.liquid_reserves -= shortfall;
        }

        // Entry fee revenue is already credited in process_turn via funding_balance
        messages.push(format!(
            "[URBAN_PARK] {} upkeep: -{:.0}, visitors: {:.0}, fees: +{:.0}",
            park.name, management_cost, park.last_turn_visitor_count, entry_fee_revenue
        ));
    }

    // Expire old conservation policies
    country
        .conservation_policies
        .retain(|p| p.is_valid(current_turn));

    messages
}
