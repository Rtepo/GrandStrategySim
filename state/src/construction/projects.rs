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

/// v4.17 FS-7: Scheduled duration (in turns) for a project type. This is
/// the single source of truth — `corporate::development` previously held a
/// private copy used only for planning, while `award_tender` left
/// `ConstructionProject.duration_turns` at 0, so the labor-demand pacer had
/// no schedule to crew against. Residential jobs run ~5 turns, megaprojects
/// up to 20.
pub fn project_type_duration_turns(project_type: ConstructionProjectType) -> u32 {
    match project_type {
        ConstructionProjectType::Residential => 5,
        ConstructionProjectType::Commercial => 8,
        ConstructionProjectType::UtilityNetwork => 12,
        ConstructionProjectType::Infrastructure => 15,
        ConstructionProjectType::SocialHousing => 6,
        ConstructionProjectType::Factory => 10,
        ConstructionProjectType::TransportNetwork => 20,
        ConstructionProjectType::Court => 8,
        ConstructionProjectType::CustomsOffice => 6,
        ConstructionProjectType::Embassy => 10,
        ConstructionProjectType::ResearchInstitute => 12,
        ConstructionProjectType::LaborInspectorate => 7,
        ConstructionProjectType::PublicWorksSite => 5,
        ConstructionProjectType::NationalTheater => 10,
        ConstructionProjectType::NationalLibrary => 9,
        ConstructionProjectType::TransportDepot => 8,
        ConstructionProjectType::ThermalGridPipe => 10,
        ConstructionProjectType::ThermalHeatingPlant => 15,
        ConstructionProjectType::CHPRetrofit => 8,
        ConstructionProjectType::WaterMainPipe => 8,
        ConstructionProjectType::SewerMainPipe => 10,
        ConstructionProjectType::WaterTreatmentPlant => 12,
        ConstructionProjectType::WastewaterTreatmentPlant => 14,
        ConstructionProjectType::Landfill => 10,
        ConstructionProjectType::WasteSeparationPlant => 12,
        ConstructionProjectType::RecyclingFacility => 14,
        ConstructionProjectType::WasteToEnergyPlant => 18,
        ConstructionProjectType::CivicAmenitySite => 8,
    }
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
    /// Phase 1 fix (C2): This field tracks ONLY material costs consumed
    /// from the building inventory. It does NOT include tranche payments
    /// to the contractor (those are tracked in `tranches_paid`).
    #[serde(default)]
    pub cost_spent: f64,

    /// Phase 1 fix (C1/C3): Actual cash debited from the investor via
    /// tender escrow at publication time. This is the amount that can be
    /// refunded on cancellation. Equals `estimated_cost` on award.
    #[serde(default)]
    pub investor_cash_debited: f64,

    /// Phase 1 fix (C2): Total tranche payments released to the contractor.
    /// Separated from `cost_spent` to avoid double-counting. The refund
    /// on cancellation = `investor_cash_debited - tranches_paid`.
    #[serde(default)]
    pub tranches_paid: f64,

    /// v4.18 WS2: paid cost basis (dollars) of materials staged for this
    /// project — B2B deliveries credit `share × execution_price`, material
    /// deploys from contractor stock credit `qty × anchored_base_price`.
    /// `cost_spent` must track acquisition cost: valuing installed work at
    /// the spot reference chain booked ~46× the cash the contractor ever
    /// paid (thin-market ratcheted ConstructionServices prices) — the
    /// phantom-COGS / phantom-investment pattern MACRO-3 gates on.
    /// `staging_qty` shadows the project's share of the site pile so the
    /// basis average stays exact on expansion sites whose inventory mixes
    /// operating stock. Units consumed draw their basis out of this pool
    /// into `cost_spent` FIFO-agnostic (pool average).
    #[serde(default)]
    pub staging_cost_basis: std::collections::BTreeMap<Commodity, f64>,
    /// Project-attributed staged quantity shadowing the site pile (see
    /// `staging_cost_basis`) — denominator of the paid-basis average.
    #[serde(default)]
    pub staging_qty: std::collections::BTreeMap<Commodity, f64>,

    /// v4.17: For expansion-tender projects, the investor's `liquid_capital`
    /// committed at publish time (`apply_action` debited the full planned
    /// investment; the uncommitted share was already returned). Under
    /// progress financing the escrow is only the mobilization share, so
    /// this commitment — not the escrow balance — is what cancellation or
    /// completion must release back to the capital book:
    /// `max(committed − tranches_paid, 0)`. Zero for non-expansion
    /// investors, which never debit `liquid_capital`.
    #[serde(default)]
    pub investor_capital_committed: f64,

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

    /// v4.18 WS2: the physical work is done (`progress` reached 1.0 and the
    /// completion bookkeeping — capacity, capital, sector rename — was
    /// applied exactly once). The project object then survives only as the
    /// contractor's live payment claim on the investor: the completion
    /// tranche may defer on investor cash, and clearing the record would
    /// erase the unpaid remainder. The building's `active_project` clears
    /// once all tranches settle (or the contractor no longer exists).
    #[serde(default)]
    pub work_complete: bool,

    // ── Phase 22A: Contractor linkage ──
    /// Investor entity ID (company ID or "STATE:{region_id}").
    /// Empty for legacy self-build projects (investor = building owner).
    #[serde(default)]
    pub investor_id: String,

    /// Phase 8 fix (M4): Cadastre parcel ID reserved for this project.
    /// Empty if no parcel has been acquired/reserved yet.
    /// For State projects, this is a Treasury-owned parcel reallocated
    /// (NOT purchased) to the project.
    #[serde(default)]
    pub parcel_id: String,

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

    // ── Phase 3: Weather/disaster effects (C5) ──
    /// Construction productivity multiplier from weather (1.0 = normal,
    /// 0.0 = halted). Updated each turn by `advance_construction_projects`
    /// from the region's active weather events. Reduces the rate at which
    /// materials can be consumed (e.g., 0.5 = half-speed construction).
    #[serde(default = "one_f64")]
    pub weather_productivity: f64,

    /// Accumulated material loss from disasters (total units destroyed).
    /// Tracked for accounting and diagnostics.
    #[serde(default)]
    pub disaster_material_loss: f64,
}

fn one_f64() -> f64 {
    1.0
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
    /// * `unit_costs` - Map of commodity to unit cost for tracking `cost_spent`.
    /// * `productivity` - Weather/productivity multiplier (1.0 = normal,
    ///   0.0 = halted). Reduces the amount of material that can be consumed
    ///   this turn. Phase 3 fix (C5).
    ///
    /// # Rules
    /// * For each required material, move available quantity from inventory to
    ///   `delivered_materials`, capped at the remaining requirement AND at
    ///   `available * productivity` (weather slows construction).
    /// * Updates `progress` and `cost_spent` after consumption.
    /// * Returns true if any materials were consumed this call.
    ///
    /// `input_reserve` is the building's production reserve: quantities of the
    /// active method's inputs that construction must NOT cannibalize. Without
    /// it an expansion tender silently eats the seeded input buffer — Steel,
    /// Planks and Cement vanish into `delivered_materials` in one turn and the
    /// plant starves with `fulfillment_ratio = 0` despite full genesis stock.
    pub fn consume_delivered_materials(
        &mut self,
        inventory: &mut BTreeMap<Commodity, f64>,
        unit_costs: &BTreeMap<Commodity, f64>,
        productivity: f64,
        input_reserve: &BTreeMap<Commodity, f64>,
    ) -> bool {
        let mut any_consumed = false;
        let p = productivity.clamp(0.0, 1.0);

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
            let held = inventory.get(&commodity).copied().unwrap_or(0.0);
            let reserve = input_reserve.get(&commodity).copied().unwrap_or(0.0);
            let available = (held - reserve).max(0.0);
            if available <= 0.0 {
                continue;
            }
            // v4.18 WS2: install capacity is a per-turn RATE, not a
            // fraction of the remaining stockpile. `available * p`
            // coupled the crew's throughput to the size of the staged
            // pile, so under any crew fraction p < 1 the final units were
            // consumed geometrically — `delivered` approached `required`
            // asymptotically but never reached it, `is_complete()` could
            // never fire, and the progress-1.0 completion tranche never
            // released (observed: sites parked at progress 0.998 forever,
            // zero CONST_DBG[COMPLETE] events).
            //
            // A full crew (p = 1) installs `required / INSTALL_HORIZON`
            // units per turn, so a fully-staged line completes in
            // `INSTALL_HORIZON` turns and a fraction-p crew in
            // `INSTALL_HORIZON / p` — still linear convergence (the rate
            // is proportional to the constant requirement, not the
            // shrinking remainder), but the physical installation now
            // spans several turns so construction investment books into
            // GDP as continuous work-in-progress rather than a single
            // per-project burst.
            const INSTALL_HORIZON: f64 = 4.0;
            let rate_cap = required * p / INSTALL_HORIZON;
            let to_consume = remaining_needed.min(available).min(rate_cap);
            if to_consume <= 0.0 {
                continue;
            }
            // Remove from building inventory — subtract from `held`, not the
            // reserve-adjusted `available`, so the protected reserve remains.
            let new_qty = (held - to_consume).max(0.0);
            if new_qty > 0.0 {
                inventory.insert(commodity, new_qty);
            } else {
                inventory.remove(&commodity);
            }
            // Add to delivered materials
            *self.delivered_materials.entry(commodity).or_insert(0.0) += to_consume;
            // Track cost spent at ACQUISITION basis — the staged pool's
            // average paid price (`staging_cost_basis / staging_qty`), with
            // the `unit_costs` reference only for the unbased remainder
            // (legacy saves, genesis-seeded stock). Spot-reference pricing
            // here booked ~46× the contractor's real outlay (MACRO-3).
            let staged_q = self.staging_qty.get(&commodity).copied().unwrap_or(0.0);
            let basis = self
                .staging_cost_basis
                .get(&commodity)
                .copied()
                .unwrap_or(0.0);
            let (based_q, based_cost) = if staged_q > 0.0 && basis > 0.0 {
                let q = to_consume.min(staged_q);
                (q, q * (basis / staged_q))
            } else {
                (0.0, 0.0)
            };
            self.cost_spent +=
                based_cost + (to_consume - based_q) * unit_costs.get(&commodity).copied().unwrap_or(0.0);
            if based_q > 0.0 {
                *self.staging_cost_basis.entry(commodity).or_insert(0.0) =
                    (basis - based_cost).max(0.0);
                *self.staging_qty.entry(commodity).or_insert(0.0) =
                    (staged_q - based_q).max(0.0);
            }
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
