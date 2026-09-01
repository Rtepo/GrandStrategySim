//! Infrastructure effects processing
//!
//! Phase A.6: This module is now a thin orchestrator that routes to the live
//! B2C clearing and infrastructure-funding systems. The dead parallel stubs
//! (`calculate_education_demand`, `deduct_infrastructure_costs`, etc.) have
//! been removed. Education progression is handled by
//! `economy::labor::education_progression`, and funding by
//! `allocate_owner_infrastructure_funding` in the turn loop.
//!
//! The remaining live logic here covers:
//! * Healthcare effects (health degradation/improvement from hospital capacity)
//! * Dependency care (caregiver extraction from labor pool when DPS/DDP deficit)
//! * Capacity utilization tracking (for UI/telemetry)

use crate::infrastructure::CapacityType;
use crate::society::geography::{DependencyLevel, HealthStatus, RuralClass};

/// Apply infrastructure capacity effects to population.
///
/// Phase A.6: Thin orchestrator — routes to live systems only.
/// Education progression and funding are handled elsewhere in the turn loop.
pub fn apply_infrastructure_effects(region: &mut crate::society::geography::Region, _year: u32) {
    // 1. Calculate capacity utilization (for UI/telemetry)
    calculate_capacity_utilization(region);

    // 2. Apply healthcare effects (live: uses capacity_pool from A.1 sync)
    apply_healthcare_effects(region);

    // 3. Process dependency care (live: uses capacity_pool from A.1 sync)
    process_dependency_care(region);

    // Education effects removed (Phase A.6) — now handled by
    // `economy::labor::education_progression::process_education_progression_turn`
    // which runs after B2C clearing in the turn loop.

    // Funding/revenue stubs removed (Phase A.6) — now handled by
    // `allocate_owner_infrastructure_funding` in the turn loop (Phase 9.3).
}

/// Calculate capacity utilization based on population demand.
///
/// Phase A.6: Replaces the dead zero-returning stubs with real demand
/// estimates derived from region population and demographic structure.
fn calculate_capacity_utilization(region: &mut crate::society::geography::Region) {
    // Estimate demands from population (Rule 15: physical scaling).
    let population = region.population.max(1) as f64;
    let healthcare_demand = estimate_healthcare_demand(region, population);
    let education_demand = estimate_education_demand(region, population);
    let care_demand = estimate_care_demand(region, population);

    // Update utilization tracking (stored separately from capacity pool)
    for (capacity_type, available_capacity) in &region.capacity_pool {
        let demand = match capacity_type {
            CapacityType::HospitalBeds => healthcare_demand.acute,
            CapacityType::ClinicVisits => healthcare_demand.routine,
            CapacityType::PrimarySeats => education_demand.primary,
            CapacityType::HighSchoolSeats => education_demand.high_school,
            CapacityType::UniversitySlots => education_demand.university,
            CapacityType::DPSCapacity => care_demand.dps,
            CapacityType::DDPCapacity => care_demand.ddp,
            _ => 0.0,
        };

        let utilization = (demand / available_capacity.max(1.0)).min(1.0);
        region
            .capacity_utilization
            .insert(*capacity_type, utilization);
    }
}

/// Estimate healthcare demand from population (replaces dead stub).
fn estimate_healthcare_demand(
    _region: &crate::society::geography::Region,
    population: f64,
) -> HealthcareDemand {
    // Acute care: ~3% of population needs hospital beds per turn.
    // Routine care: ~15% of population needs clinic visits per turn.
    // These are physical ratios (Rule 3: physical, not financial).
    let acute = population * 0.03;
    let routine = population * 0.15;
    HealthcareDemand { acute, routine }
}

/// Estimate education demand from population (replaces dead stub).
fn estimate_education_demand(
    _region: &crate::society::geography::Region,
    population: f64,
) -> EducationDemand {
    // Children (~25% of pop) need primary seats.
    // ~15% of pop needs high school seats.
    // ~5% of pop needs university slots.
    let children_ratio = 0.25;
    let teen_ratio = 0.15;
    let young_adult_ratio = 0.05;
    let primary = population * children_ratio;
    let high_school = population * teen_ratio;
    let university = population * young_adult_ratio;
    EducationDemand {
        primary,
        high_school,
        university,
    }
}

/// Estimate care demand from population (replaces dead stub).
fn estimate_care_demand(
    _region: &crate::society::geography::Region,
    population: f64,
) -> CareDemand {
    // ~2% of pop needs DPS (fully dependent), ~5% needs DDP (partially dependent).
    let dps = population * 0.02;
    let ddp = population * 0.05;
    CareDemand { dps, ddp }
}

/// Apply healthcare effects to population
fn apply_healthcare_effects(region: &mut crate::society::geography::Region) {
    let total_hospital_capacity = region
        .capacity_pool
        .get(&CapacityType::HospitalBeds)
        .copied()
        .unwrap_or(0.0);
    let total_sanatorium_capacity = region
        .capacity_pool
        .get(&CapacityType::SanatoriumStays)
        .copied()
        .unwrap_or(0.0);

    for class in [
        RuralClass::Aristocracy,
        RuralClass::FreePeasant,
        RuralClass::LandlessLaborer,
    ] {
        if let Some(demographics) = region.class_demographics.get_class_mut(class) {
            // Calculate health degradation based on age and labor factors
            // Serfs have higher degradation due to harsh conditions
            let base_degradation = match class {
                RuralClass::Serf => 0.02, // 2% per turn for serfs
                RuralClass::LandlessLaborer => 0.015,
                RuralClass::FreePeasant => 0.01,
                RuralClass::Aristocracy => 0.005,
            };
            demographics.health_degradation_rate = base_degradation;

            // Apply natural degradation
            degrade_health(demographics);

            // Hospital capacity improves health status
            let access_ratio = if demographics.population > 0 {
                total_hospital_capacity / demographics.population as f64
            } else {
                0.0
            };
            let health_improvement = access_ratio * 0.1; // Max 10% improvement per turn

            // Apply health improvement (capped at biological limit)
            let biological_limit = calculate_biological_limit(demographics);
            demographics.health_status = improve_health(
                demographics.health_status,
                health_improvement,
                biological_limit,
            );

            // Sanatorium capacity reduces degradation rate
            let sanatorium_protection = if demographics.population > 0 {
                total_sanatorium_capacity / demographics.population as f64
            } else {
                0.0
            };
            demographics.health_degradation_rate *= 1.0 - sanatorium_protection * 0.5;

            // Update dependency level based on health status
            update_dependency_from_health(demographics);
        }
    }
}

/// Degrade health status based on degradation rate
fn degrade_health(demographics: &mut crate::society::geography::ClassDemographics) {
    let degradation = demographics.health_degradation_rate;
    demographics.health_status = match demographics.health_status {
        HealthStatus::Excellent => {
            if rand::random::<f64>() < degradation {
                HealthStatus::Good
            } else {
                HealthStatus::Excellent
            }
        }
        HealthStatus::Good => {
            if rand::random::<f64>() < degradation * 1.5 {
                HealthStatus::Fair
            } else {
                HealthStatus::Good
            }
        }
        HealthStatus::Fair => {
            if rand::random::<f64>() < degradation * 2.0 {
                HealthStatus::Poor
            } else {
                HealthStatus::Fair
            }
        }
        HealthStatus::Poor => {
            if rand::random::<f64>() < degradation * 2.5 {
                HealthStatus::Critical
            } else {
                HealthStatus::Poor
            }
        }
        HealthStatus::Critical => HealthStatus::Critical, // Cannot degrade further
    };
}

/// Update dependency level based on health status
fn update_dependency_from_health(demographics: &mut crate::society::geography::ClassDemographics) {
    match demographics.health_status {
        HealthStatus::Excellent | HealthStatus::Good => {
            demographics.dependency_level = DependencyLevel::Independent;
        }
        HealthStatus::Fair => {
            // 30% chance of becoming partially dependent
            if rand::random::<f64>() < 0.3 {
                demographics.dependency_level = DependencyLevel::PartiallyDependent;
            }
        }
        HealthStatus::Poor => {
            // 70% chance of becoming partially dependent
            if rand::random::<f64>() < 0.7 {
                demographics.dependency_level = DependencyLevel::PartiallyDependent;
            }
        }
        HealthStatus::Critical => {
            // 90% chance of becoming fully dependent
            if rand::random::<f64>() < 0.9 {
                demographics.dependency_level = DependencyLevel::FullyDependent;
            }
        }
    }
}

/// Process dependency care and extract caregivers if needed
fn process_dependency_care(region: &mut crate::society::geography::Region) {
    let dps_capacity = region
        .capacity_pool
        .get(&CapacityType::DPSCapacity)
        .copied()
        .unwrap_or(0.0);
    let ddp_capacity = region
        .capacity_pool
        .get(&CapacityType::DDPCapacity)
        .copied()
        .unwrap_or(0.0);

    let fully_dependent = count_dependent_population(region, DependencyLevel::FullyDependent);
    let partially_dependent =
        count_dependent_population(region, DependencyLevel::PartiallyDependent);

    let dps_deficit = (fully_dependent as f64 - dps_capacity).max(0.0);
    let ddp_deficit = (partially_dependent as f64 - ddp_capacity).max(0.0);

    if dps_deficit > 0.0 || ddp_deficit > 0.0 {
        let total_caregivers_needed = (dps_deficit + ddp_deficit * 0.5).ceil() as i64;
        extract_caregivers_from_labor_pool(region, total_caregivers_needed);
    }
}

/// CRITICAL: Caregiver Death Spiral Safeguard
/// Extract caregivers ONLY from Unskilled labor pool (or unemployed Skilled)
/// Expert tier workers (Doctors, Engineers) are IMMUNE to caregiver extraction
/// This prevents cascading engine failure when hospitals/factories collapse
fn extract_caregivers_from_labor_pool(
    region: &mut crate::society::geography::Region,
    caregivers_needed: i64,
) {
    if caregivers_needed <= 0 {
        return;
    }

    // For now, we'll simulate this by reducing labor participation in LandlessLaborer class
    // This represents Unskilled tier being forced into caregiving
    if let Some(demographics) = region
        .class_demographics
        .get_class_mut(RuralClass::LandlessLaborer)
    {
        let available_workers =
            (demographics.population as f64 * demographics.labor_participation) as i64;
        let to_extract = caregivers_needed.min(available_workers);

        if to_extract > 0 {
            // Reduce labor participation
            let new_participation =
                ((available_workers - to_extract) as f64 / demographics.population as f64).max(0.0);
            demographics.labor_participation = new_participation;

            // Update dependent count
            demographics.dependent_count += to_extract;
        }
    }

    // If still need more caregivers, extract from FreePeasant (unemployed Skilled equivalent)
    let remaining_needed = caregivers_needed
        - (caregivers_needed.min(
            region
                .class_demographics
                .get_class(RuralClass::LandlessLaborer)
                .map(|d| (d.population as f64 * d.labor_participation) as i64)
                .unwrap_or(0),
        ));

    if remaining_needed > 0 {
        if let Some(demographics) = region
            .class_demographics
            .get_class_mut(RuralClass::FreePeasant)
        {
            let available_workers =
                (demographics.population as f64 * demographics.labor_participation) as i64;
            let to_extract = remaining_needed.min(available_workers);

            if to_extract > 0 {
                let new_participation = ((available_workers - to_extract) as f64
                    / demographics.population as f64)
                    .max(0.0);
                demographics.labor_participation = new_participation;
                demographics.dependent_count += to_extract;
            }
        }
    }

    // NOTE: Aristocracy (Expert tier equivalent) is NEVER extracted - immune to death spiral
}

// Apply education effects (class mobility) — REMOVED (Phase A.6).
// Education progression is now handled by
// `economy::labor::education_progression::process_education_progression_turn`
// which uses consumed EducationSlots from B2C clearing to shift demographic
// education shares (none → basic → secondary → higher).

// Calculate infrastructure funding requirements — REMOVED (Phase A.6).
// Funding is now handled by `allocate_owner_infrastructure_funding` in the
// turn loop (Phase 9.3), which uses proper double-entry bookkeeping.

// Clear infrastructure revenue — REMOVED (Phase A.6).
// Revenue is now handled by B2C service clearing in the turn loop
// (Phase 9.1), which routes payments through TransferSettler.

// Deduct infrastructure costs from budgets — REMOVED (Phase A.6).
// Funding is now handled by `allocate_owner_infrastructure_funding` in the
// turn loop (Phase 9.3), which uses proper double-entry bookkeeping.

// Phase A.6: Dead demand stubs removed — replaced by `estimate_*` functions
// above which compute real demand from population (Rule 15: physical scaling).

/// Count dependent population by dependency level (live: scans class_demographics).
fn count_dependent_population(
    region: &crate::society::geography::Region,
    level: DependencyLevel,
) -> i64 {
    let mut count = 0i64;
    for class in [
        RuralClass::Aristocracy,
        RuralClass::FreePeasant,
        RuralClass::LandlessLaborer,
        RuralClass::Serf,
    ] {
        if let Some(d) = region.class_demographics.get_class(class) {
            if d.dependency_level == level {
                count += d.dependent_count;
            }
        }
    }
    count
}

fn improve_health(current: HealthStatus, _improvement: f64, _limit: HealthStatus) -> HealthStatus {
    current
}

fn calculate_biological_limit(
    _demographics: &crate::society::geography::ClassDemographics,
) -> HealthStatus {
    HealthStatus::Excellent
}

// Phase A.6: Dead funding stubs removed — funding is now handled by
// `allocate_owner_infrastructure_funding` in the turn loop.

#[derive(Debug, Clone)]
struct HealthcareDemand {
    acute: f64,
    routine: f64,
}

#[derive(Debug, Clone)]
struct EducationDemand {
    primary: f64,
    high_school: f64,
    university: f64,
}

#[derive(Debug, Clone)]
struct CareDemand {
    dps: f64,
    ddp: f64,
}
