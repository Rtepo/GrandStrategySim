//! Construction project system for multi-turn building development.
//!
//! This module implements construction projects that physically consume
//! materials delivered via the B2B OrderBook. Progress is driven by
//! material delivery, not by time alone.

use crate::construction::tenders::{SubcontractorAssignment, Tranche};
use crate::economy::transport_networks::NetworkLevel;
use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Construction project type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConstructionProjectType {
    #[default]
    /// Residential building
    Residential,
    /// Commercial building
    Commercial,
    /// Utility network (sewage, water, heating)
    UtilityNetwork,
    /// Infrastructure (roads, bridges)
    Infrastructure,
    /// Social housing project
    SocialHousing,
    /// Factory or industrial building
    Factory,
    /// Phase 23B: Transport network (roads, rail, highways, canals).
    /// On completion, installs a NetworkLink into the country's
    /// TransportNetworkOverlay instead of adding building capacity.
    TransportNetwork,
    /// Phase 39: Court building (Justice ministry).
    Court,
    /// Phase 39: Customs office building (Treasury/customs).
    CustomsOffice,
    /// Phase 39: Embassy building (Foreign Affairs, host country).
    Embassy,
    /// Phase 39: Research institute (Science ministry).
    ResearchInstitute,
    /// Phase 39: Labor inspectorate building (Labor ministry).
    LaborInspectorate,
    /// Phase 39: Public works site (Labor ministry).
    PublicWorksSite,
    /// Phase 39: National theater (Culture ministry).
    NationalTheater,
    /// Phase 39: National library (Culture ministry).
    NationalLibrary,
    /// Phase 39: Transport depot (Transport ministry).
    TransportDepot,
    /// Phase 82: Thermal grid pipe network construction (district heating pipes).
    ThermalGridPipe,
    /// Phase 82: Thermal heating plant construction (dedicated heat generation).
    ThermalHeatingPlant,
    /// Phase 82: CHP (Combined Heat and Power) retrofit for existing power plants.
    CHPRetrofit,
    /// Phase 83: Water main pipe network construction (potable distribution).
    /// Distinct from `UtilityNetwork` — dedicated pressurized clean-water mains.
    WaterMainPipe,
    /// Phase 83: Sewer main pipe network construction (blackwater collection).
    /// Distinct from `UtilityNetwork` — dedicated gravity-fed sewer mains.
    SewerMainPipe,
    /// Phase 83: Water treatment plant construction (upgrades environmental
    /// water quality to potable, does NOT create water mass).
    WaterTreatmentPlant,
    /// Phase 83: Wastewater treatment plant construction (filters blackwater,
    /// produces `Commodity::Fertilizers`, returns healed water to surface reserves).
    WastewaterTreatmentPlant,
    /// Phase 84: Landfill construction (controlled or modern). Stores waste
    /// with liner/leachate/gas capture systems.
    Landfill,
    /// Phase 84: Waste separation plant construction (sorts MixedWaste into
    /// recyclable fractions + residual refuse).
    WasteSeparationPlant,
    /// Phase 84: Recycling facility construction (converts sorted waste
    /// fractions back into virgin commodities + residual).
    RecyclingFacility,
    /// Phase 84: Waste-to-Energy plant construction (incinerates residual waste
    /// → Energy + Heat + HazardousWaste ash).
    WasteToEnergyPlant,
    /// Phase 84: Civic Amenity Site (PSZOK) construction — drop-off for
    /// BulkyWaste, ConstructionWaste, HazardousWaste. Requires FreightCapacity.
    CivicAmenitySite,
}

/// Multi-turn construction project tied to a physical building site.
///
/// The project lives on `Building.active_project`. The building's `owner_id`
/// determines who funds the B2B buy bids. Progress is computed from
/// `delivered_materials` vs `required_materials`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConstructionProject {
    /// Unique project ID.
    #[serde(default)]
    pub id: String,

    /// Project type.
    pub project_type: ConstructionProjectType,

    /// Micro-region where construction occurs.
    #[serde(default)]
    pub micro_region_id: String,

    /// Target building type name (e.g. "Cement Plant").
    #[serde(default)]
    pub target_building_type: String,

    /// Required construction materials (Commodity → total quantity needed).
    #[serde(default)]
    pub required_materials: BTreeMap<Commodity, f64>,

    /// Materials physically delivered and consumed so far.
    #[serde(default)]
    pub delivered_materials: BTreeMap<Commodity, f64>,

    /// Worker capacity to add on completion.
    #[serde(default)]
    pub target_capacity_increase: u32,

    /// Fixed capital to add on completion.
    #[serde(default)]
    pub target_capital_increase: f64,

    /// True if this is a brand-new building (vs expanding an existing one).
    #[serde(default)]
    pub is_new_building: bool,

    /// Total estimated cost (for budgeting reference).
    #[serde(default)]
    pub total_cost: f64,

    /// Cost spent so far (cash paid for delivered materials).
    #[serde(default)]
    pub cost_spent: f64,

    /// Estimated duration in turns (for planning only; actual completion
    /// depends on material delivery).
    #[serde(default)]
    pub duration_turns: u32,

    /// Turns elapsed since project start.
    #[serde(default)]
    pub turns_elapsed: u32,

    /// Progress 0.0–1.0, computed from material fulfillment.
    #[serde(default)]
    pub progress: f64,

    /// Whether project is on hold due to material shortage.
    #[serde(default)]
    pub on_hold: bool,

    /// Reason for being on hold (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_reason: Option<String>,

    /// Phase 29: Consecutive turns the project has been on hold without
    /// any material consumption. Used to cancel permanently stalled projects.
    #[serde(default)]
    pub consecutive_hold_turns: u32,

    // ── Phase 22A: Contractor linkage ──
    /// Investor entity ID (company ID or "STATE:{region_id}").
    /// Empty for legacy self-build projects (investor = building owner).
    #[serde(default)]
    pub investor_id: String,

    /// Main contractor company ID (who builds the project).
    /// Empty for legacy self-build projects.
    #[serde(default)]
    pub main_contractor_id: String,

    /// Subcontractor assignments (empty if solo contractor).
    #[serde(default)]
    pub subcontractors: Vec<SubcontractorAssignment>,

    /// Milestone payment tranches (empty for legacy self-build).
    #[serde(default)]
    pub tranches: Vec<Tranche>,

    /// Number of tranches released so far.
    #[serde(default)]
    pub paid_tranches: u32,

    /// Total contract price the investor pays the contractor.
    #[serde(default)]
    pub contract_price: f64,

    /// Profit margin retained by the contractor (fraction of contract_price).
    #[serde(default)]
    pub contractor_margin: f64,

    // ── Phase 22B: Defects & OHS ──
    /// Accumulated structural defect (0.0 = sound, 1.0 = catastrophic).
    /// Hidden field — not visible without inspection.
    #[serde(default)]
    pub structural_defect: f64,

    /// HealthCapacity units needed per turn for OHS compliance.
    #[serde(default)]
    pub ohs_health_required: f64,

    /// EducationSlots needed per turn for OHS compliance (safety training).
    #[serde(default)]
    pub ohs_education_required: f64,

    /// HealthCapacity actually procured this turn.
    #[serde(default)]
    pub ohs_health_delivered: f64,

    /// EducationSlots actually procured this turn.
    #[serde(default)]
    pub ohs_education_delivered: f64,

    /// OHS coverage ratio (0.0–1.0). 1.0 = full safety, 0.0 = no coverage.
    #[serde(default)]
    pub ohs_coverage_ratio: f64,

    /// Accumulated workplace accident count.
    #[serde(default)]
    pub ohs_accidents: u32,

    // ── Phase 23B: Network link target ──
    /// Region pair this network link connects (None for non-network projects).
    /// Tuple: (region_a_id, region_b_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_link_target: Option<(String, String)>,

    /// Network level to build/upgrade to (None for non-network projects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_target_level: Option<NetworkLevel>,
}

impl ConstructionProject {
    /// Compute progress as the minimum fulfillment ratio across all required materials.
    ///
    /// # Rules
    /// * If any required material has zero delivery, progress is 0 for that material.
    /// * Overall progress = min(delivered[mat] / required[mat]) across all materials.
    /// * If `required_materials` is empty, progress is 1.0 (no materials needed).
    pub fn compute_progress(&self) -> f64 {
        if self.required_materials.is_empty() {
            return 1.0;
        }
        let mut min_ratio = f64::MAX;
        for (&commodity, &required) in &self.required_materials {
            if required <= 0.0 {
                continue;
            }
            let delivered = self
                .delivered_materials
                .get(&commodity)
                .copied()
                .unwrap_or(0.0);
            let ratio = (delivered / required).min(1.0);
            if ratio < min_ratio {
                min_ratio = ratio;
            }
        }
        if min_ratio == f64::MAX {
            1.0
        } else {
            min_ratio
        }
    }

    /// Check if project is complete (all required materials delivered).
    pub fn is_complete(&self) -> bool {
        self.compute_progress() >= 1.0
    }

    /// Consume delivered materials from the building's inventory into the project.
    ///
    /// # Arguments
    /// * `inventory` - The building's physical inventory (mutated — materials are removed).
    ///
    /// # Rules
    /// * For each required material, move available quantity from inventory to
    ///   `delivered_materials`, capped at the remaining requirement.
    /// * Updates `progress` and `cost_spent` after consumption.
    /// * Returns true if any materials were consumed this call.
    pub fn consume_delivered_materials(
        &mut self,
        inventory: &mut BTreeMap<Commodity, f64>,
        unit_costs: &BTreeMap<Commodity, f64>,
    ) -> bool {
        let mut any_consumed = false;

        for (&commodity, &required) in &self.required_materials {
            if required <= 0.0 {
                continue;
            }
            let already_delivered = self
                .delivered_materials
                .get(&commodity)
                .copied()
                .unwrap_or(0.0);
            let remaining_needed = (required - already_delivered).max(0.0);
            if remaining_needed <= 0.0 {
                continue;
            }
            let available = inventory.get(&commodity).copied().unwrap_or(0.0);
            if available <= 0.0 {
                continue;
            }
            let to_consume = available.min(remaining_needed);
            // Remove from building inventory
            let new_qty = (available - to_consume).max(0.0);
            if new_qty > 0.0 {
                inventory.insert(commodity, new_qty);
            } else {
                inventory.remove(&commodity);
            }
            // Add to delivered materials
            *self.delivered_materials.entry(commodity).or_insert(0.0) += to_consume;
            // Track cost spent
            let unit_cost = unit_costs.get(&commodity).copied().unwrap_or(0.0);
            self.cost_spent += to_consume * unit_cost;
            any_consumed = true;
        }

        if any_consumed {
            self.progress = self.compute_progress();
        }

        any_consumed
    }

    /// Put project on hold.
    pub fn put_on_hold(&mut self, reason: String) {
        self.on_hold = true;
        self.hold_reason = Some(reason);
    }

    /// Resume project from hold.
    pub fn resume(&mut self) {
        self.on_hold = false;
        self.hold_reason = None;
        self.consecutive_hold_turns = 0;
    }
}

/// Construction queue for a micro-region.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConstructionQueue {
    /// Active projects.
    #[serde(default)]
    pub active_projects: Vec<ConstructionProject>,

    /// Completed projects (for history).
    #[serde(default)]
    pub completed_projects: Vec<ConstructionProject>,

    /// Maximum concurrent projects.
    #[serde(default)]
    pub max_concurrent_projects: u32,
}

impl ConstructionQueue {
    /// Add a new project to the queue.
    pub fn add_project(&mut self, project: ConstructionProject) -> Result<(), String> {
        if self.active_projects.len() >= self.max_concurrent_projects as usize {
            return Err("Construction queue at maximum capacity".to_string());
        }
        self.active_projects.push(project);
        Ok(())
    }

    /// Get total estimated cost of all active projects.
    pub fn total_active_cost(&self) -> f64 {
        self.active_projects.iter().map(|p| p.total_cost).sum()
    }
}
