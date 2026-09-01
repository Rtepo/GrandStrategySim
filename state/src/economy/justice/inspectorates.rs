//! Inspectorate violation detection and fining (Phase 15C).
//!
//! Implements physical inspectorate mechanics: sanepid (sanitary), Building
//! Inspectorate, and Environmental Inspectorate. Each inspectorate produces
//! inspection capacity from its buildings. Violations are detected based on
//! building condition, sector, and production volume. Detected violations
//! trigger fines (reusing Phase 14.5 `levy_fines` double-entry) and increase
//! `justice_demand` in the justice system.
//!
//! ## Double-Entry Rules
//!
//! - Fines: routed through `settle_transfer_to_treasury` to sync bank reserves.
//! - No phantom money creation — fines are clamped to available cash.
//! - Violations increase `JusticeSystemState.justice_demand` (each violation = 1 case).

use crate::economy::legal_status::LegalStatus;
use crate::economy::transfer_settler::settle_transfer_to_treasury;
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use crate::state::Country;

/// Result of processing one inspectorate turn.
#[derive(Debug, Clone, Default)]
pub struct InspectorateTurnResult {
    /// Sanitary inspection capacity this turn.
    pub sanepid_capacity: f64,
    /// Building inspection capacity this turn.
    pub building_inspection_capacity: f64,
    /// Environmental inspection capacity this turn.
    pub environmental_inspection_capacity: f64,
    /// Total violations detected.
    pub violations_detected: u32,
    /// Total fines collected (strict double-entry).
    pub fines_collected: f64,
    /// Additional justice demand from violations.
    pub justice_demand_added: f64,
    /// Phase 18A: Shadow employment raids conducted.
    pub shadow_employment_raids: u32,
    /// Phase 18A: Total shadow employment fines collected.
    pub shadow_employment_fines: f64,
    /// Phase 18A: Illegals deported after shadow employment raids.
    pub illegals_deported: i64,
}

/// Sums a specific inspection capacity commodity from all buildings.
fn sum_inspection_capacity(buildings: &[Building], commodity: Commodity) -> f64 {
    buildings
        .iter()
        .map(|b| b.last_production.get(&commodity).copied().unwrap_or(0.0))
        .sum()
}

/// Determines if a company's sector is subject to sanitary inspection.
fn is_sanitary_target(company: &Company) -> bool {
    matches!(
        company.sector,
        Sector::Agriculture | Sector::LightIndustry | Sector::MedicalServices
    )
}

/// Determines if a company's sector is subject to environmental inspection.
fn is_environmental_target(company: &Company) -> bool {
    matches!(
        company.sector,
        Sector::Mining | Sector::HeavyIndustry | Sector::Energy | Sector::WasteManagement
    )
}

/// Calculates the pollution proxy for a company based on production output.
fn pollution_proxy(company: &Company, buildings: &[Building]) -> f64 {
    let company_buildings: Vec<&Building> = buildings
        .iter()
        .filter(|b| b.owner_id == company.id)
        .collect();

    company_buildings
        .iter()
        .flat_map(|b| b.last_production.values().copied())
        .sum::<f64>()
}

/// Calculates the condition-based violation severity for a building.
fn condition_violation_severity(condition: f64) -> f64 {
    if condition < 0.3 {
        (0.3 - condition) / 0.3
    } else {
        0.0
    }
}

/// Processes one inspectorate turn: detect violations, issue fines, update justice demand.
///
/// # Arguments
/// * `country` - Mutable country (for Treasury, justice state, inspectorate state).
/// * `companies` - Mutable companies (to fine violators).
/// * `buildings` - Buildings (to inspect condition and sector).
/// * `turn` - Current turn number.
///
/// # Returns
/// `InspectorateTurnResult` with capacity, violation, and fine statistics.
///
/// # Rules
/// - sanepid: inspects food/pharma/agriculture companies with condition < 0.5.
/// - Building Inspectorate: inspects buildings with condition < 0.3.
/// - Environmental Inspectorate: inspects mining/heavy industry/energy companies.
/// - Detection probability = `min(1.0, capacity / total_inspectable_entities)`.
/// - Fines: routed through `settle_transfer_to_treasury` to sync bank reserves.
/// - Each detected violation adds 1.0 to `JusticeSystemState.justice_demand`.
pub fn process_inspectorates_turn(
    country: &mut Country,
    companies: &mut [Company],
    buildings: &[Building],
    _turn: u32,
) -> InspectorateTurnResult {
    let mut result = InspectorateTurnResult::default();

    // 1. Sum inspection capacities from building outputs
    result.sanepid_capacity =
        sum_inspection_capacity(buildings, Commodity::SanitaryInspectionCapacity);
    result.building_inspection_capacity =
        sum_inspection_capacity(buildings, Commodity::BuildingInspectionCapacity);
    result.environmental_inspection_capacity =
        sum_inspection_capacity(buildings, Commodity::EnvironmentalInspectionCapacity);

    // Phase 29: Also sum dedicated labor inspection capacity from PIP buildings.
    // This was previously hardcoded to 0.0, making dedicated labor inspectorates
    // ineffective for shadow employment detection.
    let pip_capacity = sum_inspection_capacity(buildings, Commodity::LaborInspectionCapacity);

    // 2. Count inspectable entities per inspectorate type
    let sanitary_targets: Vec<usize> = companies
        .iter()
        .enumerate()
        .filter(|(_, c)| is_sanitary_target(c))
        .map(|(i, _)| i)
        .collect();

    let building_targets: usize = buildings.iter().filter(|b| b.condition < 0.3).count();

    let environmental_targets: Vec<usize> = companies
        .iter()
        .enumerate()
        .filter(|(_, c)| is_environmental_target(c))
        .map(|(i, _)| i)
        .collect();

    // 3. Calculate coverage ratios
    let sanitary_coverage = if sanitary_targets.is_empty() {
        1.0
    } else {
        (result.sanepid_capacity / sanitary_targets.len() as f64).min(1.0)
    };

    let building_coverage = if building_targets == 0 {
        1.0
    } else {
        (result.building_inspection_capacity / building_targets as f64).min(1.0)
    };

    let environmental_coverage = if environmental_targets.is_empty() {
        1.0
    } else {
        (result.environmental_inspection_capacity / environmental_targets.len() as f64).min(1.0)
    };

    // 4. Detect violations and issue fines
    let mut total_fines = 0.0_f64;
    let mut violations = 0_u32;
    let mut justice_demand_added = 0.0_f64;

    // --- sanepid: health code violations ---
    for &idx in &sanitary_targets {
        // Check if any building owned by this company has condition < 0.5
        let has_health_violation = buildings
            .iter()
            .filter(|b| b.owner_id == companies[idx].id)
            .any(|b| b.condition < 0.5);

        if has_health_violation {
            // Detection probability based on coverage
            if sanitary_coverage <= 0.0 {
                continue;
            }
            // Fine: base 5,000 + severity scaling
            let worst_condition = buildings
                .iter()
                .filter(|b| b.owner_id == companies[idx].id)
                .map(|b| b.condition)
                .fold(1.0, f64::min);
            let severity = (0.5 - worst_condition) / 0.5;
            let fine = 5_000.0 + severity * 15_000.0;

            let available = companies[idx]
                .brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(companies[idx].available_cash);
            let actual_fine = fine.min(available);
            if actual_fine > 0.01
                && settle_transfer_to_treasury(companies, idx, actual_fine, country).is_ok()
            {
                total_fines += actual_fine;
                violations += 1;
                justice_demand_added += 1.0;
            }
        }
    }

    // --- Building Inspectorate: building code violations ---
    for b in buildings.iter() {
        if b.condition < 0.3 {
            if building_coverage <= 0.0 {
                continue;
            }
            let severity = condition_violation_severity(b.condition);
            let fine = 2_000.0 + severity * 8_000.0;

            // Find the owning company and fine it
            if !b.owner_id.is_empty() {
                if let Some(idx) = companies.iter().position(|c| c.id == b.owner_id) {
                    let available = companies[idx]
                        .brokerage_account
                        .as_ref()
                        .map(|b| b.cash)
                        .unwrap_or(companies[idx].available_cash);
                    let actual_fine = fine.min(available);
                    if actual_fine > 0.01
                        && settle_transfer_to_treasury(companies, idx, actual_fine, country).is_ok()
                    {
                        total_fines += actual_fine;
                        violations += 1;
                        justice_demand_added += 1.0;
                    }
                }
            }
        }
    }

    // --- Environmental Inspectorate: pollution violations ---
    for &idx in &environmental_targets {
        let pollution = pollution_proxy(&companies[idx], buildings);
        if pollution > 100.0 {
            if environmental_coverage <= 0.0 {
                continue;
            }
            // Fine scales with pollution proxy
            let fine = (pollution * 100.0).min(50_000.0);
            let available = companies[idx]
                .brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(companies[idx].available_cash);
            let actual_fine = fine.min(available);
            if actual_fine > 0.01
                && settle_transfer_to_treasury(companies, idx, actual_fine, country).is_ok()
            {
                total_fines += actual_fine;
                violations += 1;
                justice_demand_added += 1.0;
            }
        }
    }

    result.violations_detected = violations;
    result.fines_collected = total_fines;
    result.justice_demand_added = justice_demand_added;

    // --- Phase 18A: Labor Violation / Shadow Employment Raids ---
    // Inspectorates with SanitaryInspectionCapacity or BuildingInspectionCapacity
    // can raid companies for shadow employment (off-the-books undocumented workers).
    // Phase 29: Also include dedicated PIP (LaborInspectionCapacity) buildings.
    let labor_inspection_capacity =
        result.sanepid_capacity + result.building_inspection_capacity + pip_capacity;
    let labor_intensive_companies: Vec<usize> = companies
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            matches!(
                c.sector,
                Sector::Agriculture
                    | Sector::LightIndustry
                    | Sector::Construction
                    | Sector::Hospitality
            ) && c
                .shadow_employment
                .as_ref()
                .map(|s| s.hidden_fte > 0.0)
                .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .collect();

    let mut shadow_raids = 0_u32;
    let mut shadow_fines = 0.0_f64;
    let mut deported_total = 0_i64;

    if !labor_intensive_companies.is_empty() && labor_inspection_capacity > 0.0 {
        let detection_probability =
            (labor_inspection_capacity / labor_intensive_companies.len() as f64).min(1.0);

        for &idx in &labor_intensive_companies {
            // Extract shadow employment data before mutable borrows
            let (hidden_fte, pit_evaded, shadow_wage_per_fte, turns_since_inspection) =
                match companies[idx].shadow_employment.as_ref() {
                    Some(s) => (
                        s.hidden_fte,
                        s.pit_evaded,
                        s.shadow_wage_per_fte,
                        s.turns_since_inspection,
                    ),
                    None => continue,
                };

            // Detection probability increases with turns since last inspection
            let effective_prob =
                (detection_probability + turns_since_inspection as f64 * 0.05).min(1.0);
            // Use a simple threshold: if effective_prob > 0.5, detected
            if effective_prob <= 0.5 {
                continue;
            }

            shadow_raids += 1;

            // Fine: triple PIT evaded + penalty on shadow wages
            let shadow_wage_penalty = shadow_wage_per_fte * hidden_fte * 0.5;
            let fine = (3.0 * pit_evaded + shadow_wage_penalty).max(1000.0);

            let available = companies[idx]
                .brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(companies[idx].available_cash);
            let actual_fine = fine.min(available);
            if actual_fine > 0.01
                && settle_transfer_to_treasury(companies, idx, actual_fine, country).is_ok()
            {
                shadow_fines += actual_fine;
            }

            // Deportation: if DeportationPolicy is not None, deport the illegal workers
            let deportation_policy = country
                .politics
                .migration_law
                .as_ref()
                .map(|ml| ml.deportation_policy.clone())
                .unwrap_or_default();

            if deportation_policy != crate::politics::laws::DeportationPolicy::None {
                let deported_count = (hidden_fte * 0.8) as i64; // 80% of hidden workers are caught and deported
                if deported_count > 0 {
                    // Find the region and class for deportation wealth extraction
                    let region_id = companies[idx].region_id.clone();
                    // Try to find a class with Illegal status in this region
                    if let Some(region) = country.regions.iter_mut().find(|r| r.id == region_id) {
                        // Find the first rural or urban class with Illegal status
                        let class_key = region
                            .class_demographics
                            .rural_classes
                            .iter()
                            .find(|(_, d)| d.legal_status == LegalStatus::Illegal)
                            .map(|(k, _)| (k.clone(), true))
                            .or_else(|| {
                                region
                                    .class_demographics
                                    .urban_classes
                                    .iter()
                                    .find(|(_, d)| d.legal_status == LegalStatus::Illegal)
                                    .map(|(k, _)| (k.clone(), false))
                            });

                        if let Some((ck, ir)) = class_key {
                            let class = if ir {
                                region.class_demographics.rural_classes.get_mut(&ck)
                            } else {
                                region.class_demographics.urban_classes.get_mut(&ck)
                            };
                            if let Some(class) = class {
                                if class.population > 0 {
                                    let per_capita = class.savings / class.population as f64;
                                    let deported_wealth = per_capita * deported_count as f64;
                                    class.savings -= deported_wealth;
                                    class.illegal_population =
                                        (class.illegal_population - deported_count).max(0);
                                    class.population -= deported_count;
                                    deported_total += deported_count;
                                }
                            }
                        }
                    }
                }
            }

            // Zero out shadow employment on detection
            if let Some(ref mut s) = companies[idx].shadow_employment {
                s.hidden_fte = 0.0;
                s.pit_evaded = 0.0;
                s.turns_since_inspection = 0;
            }
        }
    }

    result.shadow_employment_raids = shadow_raids;
    result.shadow_employment_fines = shadow_fines;
    result.illegals_deported = deported_total;

    // Update shadow economy state
    if let Some(ref mut state) = country.politics.shadow_economy_state {
        state.raids_conducted = shadow_raids;
        state.fines_collected += shadow_fines;
    }

    // 5. Update inspectorate state on Politics
    if let Some(ref mut ist) = country.politics.inspectorate_state {
        ist.sanepid_capacity = result.sanepid_capacity;
        ist.building_inspectorate_capacity = result.building_inspection_capacity;
        ist.environmental_inspectorate_capacity = result.environmental_inspection_capacity;
        ist.violations_detected = violations;
        ist.fines_issued = total_fines;
        ist.labor_inspection_capacity = pip_capacity;
    } else {
        country.politics.inspectorate_state = Some(crate::politics::laws::InspectorateState {
            sanepid_capacity: result.sanepid_capacity,
            building_inspectorate_capacity: result.building_inspection_capacity,
            environmental_inspectorate_capacity: result.environmental_inspection_capacity,
            violations_detected: violations,
            fines_issued: total_fines,
            recent_violations: Vec::new(),
            labor_inspection_capacity: pip_capacity,
            pip_fleet_range_km: 0.0,
            corruption_index: 0.05, // Phase 28: Seed non-zero so bribes can be accepted
            bribes_accepted_this_turn: 0,
            bribes_total_value: 0.0,
        });
    }

    // 6. Update justice demand
    if justice_demand_added > 0.0 {
        if let Some(ref mut js) = country.politics.justice_state {
            js.justice_demand += justice_demand_added;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Building, Company};
    use crate::registries::enums::Sector;
    use crate::state::Country;

    fn make_building(name: &str, condition: f64, owner_id: Option<String>) -> Building {
        Building {
            name: name.to_string(),
            condition,
            owner_id: owner_id.unwrap_or_default(),
            ..Default::default()
        }
    }

    #[test]
    fn test_no_violations_with_good_condition() {
        let mut country = Country::mock_for_tests();
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::LightIndustry,
            available_cash: 100_000.0,
            ..Default::default()
        }];
        let buildings = vec![make_building("Factory", 0.9, Some("C1".to_string()))];

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert_eq!(result.violations_detected, 0);
        assert!((result.fines_collected - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_sanitary_inspectorate_fines_food_company_low_condition() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::LightIndustry,
            available_cash: 100_000.0,
            ..Default::default()
        }];
        let buildings = vec![
            make_building("Factory", 0.2, Some("C1".to_string())),
            make_building("sanitary_inspectorate", 1.0, None),
        ];
        // Give sanitary inspectorate building some inspection capacity output
        let mut buildings = buildings;
        {
            let sanitary_building = &mut buildings[1];
            sanitary_building
                .last_production
                .insert(Commodity::SanitaryInspectionCapacity, 10.0);
        }

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!(result.violations_detected > 0, "should detect violations");
        assert!(result.fines_collected > 0.0, "should collect fines");
        assert!(
            (country.budget.liquid_reserves - result.fines_collected).abs() < 0.01,
            "treasury should match fines"
        );
        assert!(
            companies[0].available_cash < 100_000.0,
            "company should lose cash"
        );
    }

    #[test]
    fn test_building_inspectorate_fines_low_condition() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::LocalServices,
            available_cash: 50_000.0,
            ..Default::default()
        }];
        let mut buildings = vec![
            make_building("Factory", 0.1, Some("C1".to_string())),
            make_building("Building Inspectorate", 1.0, None),
        ];
        buildings[1]
            .last_production
            .insert(Commodity::BuildingInspectionCapacity, 10.0);

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!(
            result.violations_detected > 0,
            "should detect building violations"
        );
        assert!(result.fines_collected > 0.0);
        assert!((country.budget.liquid_reserves - result.fines_collected).abs() < 0.01);
    }

    #[test]
    fn test_environmental_inspectorate_fines_polluter() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::Mining,
            available_cash: 200_000.0,
            ..Default::default()
        }];
        let mut buildings = vec![
            make_building("Mine", 0.8, Some("C1".to_string())),
            make_building("environmental_inspectorate", 1.0, None),
        ];
        // High production output = high pollution proxy
        buildings[0]
            .last_production
            .insert(Commodity::HardCoal, 500.0);
        buildings[1]
            .last_production
            .insert(Commodity::EnvironmentalInspectionCapacity, 10.0);

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!(
            result.violations_detected > 0,
            "should detect environmental violations"
        );
        assert!(result.fines_collected > 0.0);
        assert!((country.budget.liquid_reserves - result.fines_collected).abs() < 0.01);
    }

    #[test]
    fn test_fine_clamped_to_available_cash() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::Agriculture,
            available_cash: 100.0, // Very low cash
            ..Default::default()
        }];
        let mut buildings = vec![
            make_building("Factory", 0.1, Some("C1".to_string())),
            make_building("sanitary_inspectorate", 1.0, None),
        ];
        buildings[1]
            .last_production
            .insert(Commodity::SanitaryInspectionCapacity, 10.0);

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!(
            result.fines_collected <= 100.0,
            "fine should be clamped to available cash"
        );
        assert!(
            (companies[0].available_cash - 0.0).abs() < 0.01,
            "company should be drained"
        );
    }

    #[test]
    fn test_justice_demand_increased() {
        let mut country = Country::mock_for_tests();
        country.politics.justice_state = Some(crate::politics::system::JusticeSystemState {
            justice_demand: 10.0,
            ..Default::default()
        });
        let mut companies = vec![Company {
            id: "C1".to_string(),
            sector: Sector::LightIndustry,
            available_cash: 100_000.0,
            ..Default::default()
        }];
        let mut buildings = vec![
            make_building("Factory", 0.2, Some("C1".to_string())),
            make_building("sanitary_inspectorate", 1.0, None),
        ];
        buildings[1]
            .last_production
            .insert(Commodity::SanitaryInspectionCapacity, 10.0);

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!(
            result.justice_demand_added > 0.0,
            "justice demand should increase"
        );
        let js = country.politics.justice_state.as_ref().unwrap();
        assert!(
            js.justice_demand > 10.0,
            "justice demand in state should have increased"
        );
    }

    #[test]
    fn test_inspectorate_state_updated() {
        let mut country = Country::mock_for_tests();
        let mut companies = vec![Company::default()];
        let mut buildings = vec![
            make_building("sanitary_inspectorate", 1.0, None),
            make_building("Construction Supervision", 1.0, None),
            make_building("environmental_inspectorate", 1.0, None),
        ];
        buildings[0]
            .last_production
            .insert(Commodity::SanitaryInspectionCapacity, 15.0);
        buildings[1]
            .last_production
            .insert(Commodity::BuildingInspectionCapacity, 8.0);
        buildings[2]
            .last_production
            .insert(Commodity::EnvironmentalInspectionCapacity, 12.0);

        let result = process_inspectorates_turn(&mut country, &mut companies, &buildings, 1);

        assert!((result.sanepid_capacity - 15.0).abs() < 0.01);
        assert!((result.building_inspection_capacity - 8.0).abs() < 0.01);
        assert!((result.environmental_inspection_capacity - 12.0).abs() < 0.01);

        let ist = country.politics.inspectorate_state.as_ref().unwrap();
        assert!((ist.sanepid_capacity - 15.0).abs() < 0.01);
        assert!((ist.building_inspectorate_capacity - 8.0).abs() < 0.01);
        assert!((ist.environmental_inspectorate_capacity - 12.0).abs() < 0.01);
    }
}
