//! Infrastructure effects processing
//!
//! This module handles the application of infrastructure capacity effects
//! to population demographics, including health, dependency care, and education.

use crate::infrastructure::CapacityType;
use crate::society::geography::{DependencyLevel, HealthStatus, RuralClass};

/// Apply infrastructure capacity effects to population
/// Called after all companies have been processed
pub fn apply_infrastructure_effects(region: &mut crate::society::geography::Region, _year: u32) {
    // 1. Calculate capacity utilization
    calculate_capacity_utilization(region);

    // 2. Apply healthcare effects
    apply_healthcare_effects(region);

    // 3. Process dependency care
    process_dependency_care(region);

    // 4. Apply education effects
    apply_education_effects(region);

    // 5. Calculate funding requirements
    let funding = calculate_infrastructure_funding(region);

    // 6. Clear infrastructure revenue (transfer to companies)
    clear_infrastructure_revenue(region, &funding);

    // 7. Deduct funding from budgets
    deduct_infrastructure_costs(region, &funding);
}

/// Calculate capacity utilization based on demand
fn calculate_capacity_utilization(region: &mut crate::society::geography::Region) {
    let healthcare_demand = calculate_healthcare_demand(region);
    let education_demand = calculate_education_demand(region);
    let care_demand = calculate_care_demand(region);

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
        region.capacity_utilization.insert(*capacity_type, utilization);
    }
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

    for class in [RuralClass::Aristocracy, RuralClass::FreePeasant, RuralClass::LandlessLaborer] {
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
    let partially_dependent = count_dependent_population(region, DependencyLevel::PartiallyDependent);

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
    if let Some(demographics) = region.class_demographics.get_class_mut(RuralClass::LandlessLaborer) {
        let available_workers = (demographics.population as f64 * demographics.labor_participation) as i64;
        let to_extract = caregivers_needed.min(available_workers);
        
        if to_extract > 0 {
            // Reduce labor participation
            let new_participation = ((available_workers - to_extract) as f64 / demographics.population as f64).max(0.0);
            demographics.labor_participation = new_participation;
            
            // Update dependent count
            demographics.dependent_count += to_extract;
        }
    }
    
    // If still need more caregivers, extract from FreePeasant (unemployed Skilled equivalent)
    let remaining_needed = caregivers_needed - (caregivers_needed.min(
        region.class_demographics.get_class(RuralClass::LandlessLaborer)
            .map(|d| (d.population as f64 * d.labor_participation) as i64)
            .unwrap_or(0)
    ));
    
    if remaining_needed > 0 {
        if let Some(demographics) = region.class_demographics.get_class_mut(RuralClass::FreePeasant) {
            let available_workers = (demographics.population as f64 * demographics.labor_participation) as i64;
            let to_extract = remaining_needed.min(available_workers);
            
            if to_extract > 0 {
                let new_participation = ((available_workers - to_extract) as f64 / demographics.population as f64).max(0.0);
                demographics.labor_participation = new_participation;
                demographics.dependent_count += to_extract;
            }
        }
    }
    
    // NOTE: Aristocracy (Expert tier equivalent) is NEVER extracted - immune to death spiral
}

/// Apply education effects (class mobility)
fn apply_education_effects(region: &mut crate::society::geography::Region) {
    let university_capacity = region
        .capacity_pool
        .get(&CapacityType::UniversitySlots)
        .copied()
        .unwrap_or(0.0);
    let high_school_capacity = region
        .capacity_pool
        .get(&CapacityType::HighSchoolSeats)
        .copied()
        .unwrap_or(0.0);
    let primary_capacity = region
        .capacity_pool
        .get(&CapacityType::PrimarySeats)
        .copied()
        .unwrap_or(0.0);

    // University access enables class mobility (max 5% per turn)
    let university_access_ratio = if region.population > 0 {
        university_capacity / region.population as f64
    } else {
        0.0
    };
    let mobility_probability = (university_access_ratio * 0.05).min(0.05); // Max 5% mobility per turn

    // High school enables basic literacy and skilled labor qualification
    let high_school_access_ratio = if region.population > 0 {
        high_school_capacity / region.population as f64
    } else {
        0.0
    };

    // Primary school enables basic literacy
    let primary_access_ratio = if region.population > 0 {
        primary_capacity / region.population as f64
    } else {
        0.0
    };

    // Apply class mobility: FreePeasant -> Aristocracy (merit-based advancement)
    if let Some(peasant_demographics) = region.class_demographics.get_class_mut(RuralClass::FreePeasant) {
        if rand::random::<f64>() < mobility_probability && peasant_demographics.population > 0 {
            let advancing = ((peasant_demographics.population as f64 * mobility_probability) as i64).min(peasant_demographics.population);
            
            if advancing > 0 {
                // Transfer peasants to Aristocracy
                peasant_demographics.population -= advancing;
                
                if let Some(aristocracy_demographics) = region.class_demographics.get_class_mut(RuralClass::Aristocracy) {
                    aristocracy_demographics.population += advancing;
                }
            }
        }
    }

    // High school enables LandlessLaborer -> FreePeasant mobility
    if let Some(laborer_demographics) = region.class_demographics.get_class_mut(RuralClass::LandlessLaborer) {
        let laborer_mobility = (high_school_access_ratio * 0.03).min(0.03); // Max 3% per turn
        
        if rand::random::<f64>() < laborer_mobility && laborer_demographics.population > 0 {
            let advancing = ((laborer_demographics.population as f64 * laborer_mobility) as i64).min(laborer_demographics.population);
            
            if advancing > 0 {
                laborer_demographics.population -= advancing;
                
                if let Some(peasant_demographics) = region.class_demographics.get_class_mut(RuralClass::FreePeasant) {
                    peasant_demographics.population += advancing;
                }
            }
        }
    }

    // Primary school improves literacy (affects future mobility potential)
    // This is tracked in the region's education statistics
    // For now, we'll just update a placeholder effect
    if primary_access_ratio > 0.5 {
        // High primary education access improves long-term mobility prospects
        // This would affect future turns' mobility calculations
    }
}

/// Calculate infrastructure funding requirements
fn calculate_infrastructure_funding(region: &crate::society::geography::Region) -> InfrastructureFunding {
    let healthcare_funding = calculate_healthcare_funding(region);
    let education_funding = calculate_education_funding(region);
    let care_funding = calculate_care_funding(region);

    InfrastructureFunding {
        healthcare: healthcare_funding,
        education: education_funding,
        care: care_funding,
    }
}

/// Clear infrastructure revenue - transfer funding to companies
/// Called after calculate_infrastructure_funding and before deduct_infrastructure_costs
/// CRITICAL: This prevents mass bankruptcy of infrastructure companies
fn clear_infrastructure_revenue(
    region: &mut crate::society::geography::Region,
    funding: &InfrastructureFunding,
) {
    // Calculate revenue based on capacity utilization and funding model
    // For Public funding: revenue = budget allocation * utilization
    // For Private funding: revenue = capacity * market price * utilization
    // For Mixed funding: revenue = (subsidy + private payments) * utilization
    
    // This is a placeholder implementation - actual company integration will happen
    // when the corporate module is fully integrated with infrastructure
    
    // For now, we'll track the revenue that would be transferred
    let healthcare_revenue = calculate_healthcare_revenue(region, funding.healthcare);
    let education_revenue = calculate_education_revenue(region, funding.education);
    let care_revenue = calculate_care_revenue(region, funding.care);
    
    // Phase 13: Requires company registry to transfer revenues to company liquid_reserves
    let _total_revenue = healthcare_revenue + education_revenue + care_revenue;
}

/// Calculate healthcare revenue based on funding model and utilization
fn calculate_healthcare_revenue(
    region: &crate::society::geography::Region,
    budget_allocation: f64,
) -> f64 {
    let hospital_utilization = region
        .capacity_utilization
        .get(&CapacityType::HospitalBeds)
        .copied()
        .unwrap_or(0.0);
    
    // Revenue = budget * utilization (for public funding)
    // For private funding, this would be: capacity * price * utilization
    budget_allocation * hospital_utilization
}

/// Calculate education revenue based on funding model and utilization
fn calculate_education_revenue(
    region: &crate::society::geography::Region,
    budget_allocation: f64,
) -> f64 {
    let university_utilization = region
        .capacity_utilization
        .get(&CapacityType::UniversitySlots)
        .copied()
        .unwrap_or(0.0);
    
    let high_school_utilization = region
        .capacity_utilization
        .get(&CapacityType::HighSchoolSeats)
        .copied()
        .unwrap_or(0.0);
    
    let average_utilization = (university_utilization + high_school_utilization) / 2.0;
    budget_allocation * average_utilization
}

/// Calculate care facility revenue based on funding model and utilization
fn calculate_care_revenue(
    region: &crate::society::geography::Region,
    budget_allocation: f64,
) -> f64 {
    let dps_utilization = region
        .capacity_utilization
        .get(&CapacityType::DPSCapacity)
        .copied()
        .unwrap_or(0.0);
    
    let ddp_utilization = region
        .capacity_utilization
        .get(&CapacityType::DDPCapacity)
        .copied()
        .unwrap_or(0.0);
    
    let average_utilization = (dps_utilization + ddp_utilization) / 2.0;
    budget_allocation * average_utilization
}

/// Deduct infrastructure costs from budgets
fn deduct_infrastructure_costs(
    _region: &mut crate::society::geography::Region,
    _funding: &InfrastructureFunding,
) {
    // This will be implemented when budget structure is available
    // Deducts from central/regional budgets based on budget source
}

// Helper functions (stubs for now)

fn calculate_healthcare_demand(_region: &crate::society::geography::Region) -> HealthcareDemand {
    HealthcareDemand {
        acute: 0.0,
        routine: 0.0,
    }
}

fn calculate_education_demand(_region: &crate::society::geography::Region) -> EducationDemand {
    EducationDemand {
        primary: 0.0,
        high_school: 0.0,
        university: 0.0,
    }
}

fn calculate_care_demand(_region: &crate::society::geography::Region) -> CareDemand {
    CareDemand { dps: 0.0, ddp: 0.0 }
}

fn count_dependent_population(
    _region: &crate::society::geography::Region,
    _level: DependencyLevel,
) -> i64 {
    0
}

fn improve_health(current: HealthStatus, _improvement: f64, _limit: HealthStatus) -> HealthStatus {
    current
}

fn calculate_biological_limit(_demographics: &crate::society::geography::ClassDemographics) -> HealthStatus {
    HealthStatus::Excellent
}

fn calculate_healthcare_funding(_region: &crate::society::geography::Region) -> f64 {
    0.0
}

fn calculate_education_funding(_region: &crate::society::geography::Region) -> f64 {
    0.0
}

fn calculate_care_funding(_region: &crate::society::geography::Region) -> f64 {
    0.0
}

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

#[derive(Debug, Clone)]
struct InfrastructureFunding {
    healthcare: f64,
    education: f64,
    care: f64,
}
