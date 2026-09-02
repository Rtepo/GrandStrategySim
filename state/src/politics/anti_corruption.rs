//! Phase 29: Anti-Corruption State Reaction AI.
//!
//! This module implements a feedback loop where the government reacts to
//! rising corruption and tax leakage by shifting budget priorities toward
//! Justice and InternalSecurity. It also triggers inspectorate building
//! construction through the tender market.

use crate::politics::ministries::{GovernmentCompetency, Ministry};
use crate::state::Country;

/// Result of evaluating the anti-corruption response.
///
/// Contains proposed reallocation deltas for each ministry that should
/// receive more or less funding based on corruption levels.
#[derive(Debug, Clone, Default)]
pub struct BudgetReallocation {
    /// Ministry ID → delta to apply to `allocated_cash` (positive = increase).
    pub deltas: std::collections::HashMap<String, f64>,
    /// Severity score [0.0, 1.0] driving the reallocation magnitude.
    pub severity: f64,
}

/// Evaluate whether the government should reallocate budget toward
/// Justice/InternalSecurity based on corruption and tax leakage.
///
/// # Arguments
/// * `country` - Read-only country state for reading corruption metrics.
///
/// # Returns
/// A `BudgetReallocation` with proposed deltas per ministry.
///
/// # Rules
/// * If `corruption_index > 0.3` OR `total_pit_evaded > 5% of tax revenue`:
///   - Proposes shifting up to 15% of other ministries' allocations to
///     Justice/InternalSecurity.
///   - The shift magnitude scales with corruption severity.
/// * Low corruption (< 0.1) does not trigger reallocation.
/// * Reallocation is constrained by available treasury cash.
pub fn evaluate_anti_corruption_response(country: &Country) -> BudgetReallocation {
    let mut result = BudgetReallocation::default();

    // Read corruption index from inspectorate state
    let corruption_index = country
        .politics
        .inspectorate_state
        .as_ref()
        .map(|ist| ist.corruption_index)
        .unwrap_or(0.0);

    // Read shadow economy PIT evasion
    let total_pit_evaded = country
        .politics
        .shadow_economy_state
        .as_ref()
        .map(|s| s.total_pit_evaded)
        .unwrap_or(0.0);

    // Read recent tax revenue (approximate from liquid reserves + spent)
    let tax_revenue = country.budget.liquid_reserves
        + country
            .politics
            .ministry_config
            .as_ref()
            .map(|mc| mc.ministries.iter().map(|m| m.spent_cash).sum::<f64>())
            .unwrap_or(0.0);

    // Calculate severity from corruption index
    let corruption_severity = if corruption_index > 0.3 {
        ((corruption_index - 0.3) / 0.7).min(1.0)
    } else {
        0.0
    };

    // Calculate severity from PIT evasion (if > 5% of tax revenue)
    let evasion_severity = if tax_revenue > 0.0 {
        let evasion_ratio = total_pit_evaded / tax_revenue;
        if evasion_ratio > 0.05 {
            ((evasion_ratio - 0.05) / 0.20).min(1.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Combined severity is the max of both signals
    result.severity = corruption_severity.max(evasion_severity);

    if result.severity <= 0.0 {
        return result;
    }

    // Find ministries with Justice or InternalSecurity competencies
    let ministry_config = match &country.politics.ministry_config {
        Some(mc) => mc,
        None => return result,
    };

    let security_ministries: Vec<&Ministry> = ministry_config
        .ministries
        .iter()
        .filter(|m| {
            m.competencies.iter().any(|c| {
                matches!(
                    c,
                    GovernmentCompetency::Justice | GovernmentCompetency::InternalSecurity
                )
            })
        })
        .collect();

    if security_ministries.is_empty() {
        return result;
    }

    // Calculate the total allocation of non-security ministries
    let non_security_total: f64 = ministry_config
        .ministries
        .iter()
        .filter(|m| {
            !m.competencies.iter().any(|c| {
                matches!(
                    c,
                    GovernmentCompetency::Justice | GovernmentCompetency::InternalSecurity
                )
            })
        })
        .map(|m| m.allocated_cash)
        .sum();

    if non_security_total <= 0.0 {
        return result;
    }

    // Shift up to 15% of non-security allocations to security ministries
    let shift_fraction = 0.15 * result.severity;
    let total_shift = non_security_total * shift_fraction;

    // Distribute the shift: reduce non-security ministries proportionally
    for m in &ministry_config.ministries {
        let is_security = m.competencies.iter().any(|c| {
            matches!(
                c,
                GovernmentCompetency::Justice | GovernmentCompetency::InternalSecurity
            )
        });
        if !is_security && m.allocated_cash > 0.0 {
            let reduction = m.allocated_cash * shift_fraction;
            result.deltas.insert(m.id.clone(), -reduction);
        }
    }

    // Increase security ministries equally
    let per_security_increase = total_shift / security_ministries.len() as f64;
    for m in &security_ministries {
        let current = result.deltas.get(&m.id).copied().unwrap_or(0.0);
        result
            .deltas
            .insert(m.id.clone(), current + per_security_increase);
    }

    result
}

/// Apply a budget reallocation to the ministry config.
///
/// # Arguments
/// * `country` - Mutable country with ministry config to update.
/// * `reallocation` - The reallocation deltas to apply.
///
/// # Rules
/// * Deltas are applied to `allocated_cash` (the promised allocation).
/// * Allocations cannot go below zero.
/// * This affects the next `allocate_cash_to_ministries` call.
pub fn apply_budget_reallocation(country: &mut Country, reallocation: &BudgetReallocation) {
    if reallocation.deltas.is_empty() {
        return;
    }
    if let Some(mc) = country.politics.ministry_config.as_mut() {
        for m in mc.ministries.iter_mut() {
            if let Some(&delta) = reallocation.deltas.get(&m.id) {
                m.allocated_cash = (m.allocated_cash + delta).max(0.0);
            }
        }
    }
}

/// Phase 29: Run the anti-corruption feedback loop.
///
/// Evaluates corruption levels and applies budget reallocation if needed.
/// Called once per turn after tax collection and before ministry allocation.
///
/// # Arguments
/// * `country` - Mutable country state.
///
/// # Returns
/// The severity score that drove the reallocation (0.0 = no action).
pub fn run_anti_corruption_feedback(country: &mut Country) -> f64 {
    let reallocation = evaluate_anti_corruption_response(country);
    let severity = reallocation.severity;
    if severity > 0.0 {
        apply_budget_reallocation(country, &reallocation);
    }
    severity
}

/// Phase 29: State construction of inspectorate buildings.
///
/// When the Justice or InternalSecurity ministry has surplus allocated cash
/// AND corruption_index > 0.2, the minister publishes a ConstructionTender
/// for a new inspectorate building. The building is constructed through the
/// tender market — no magical spawns.
///
/// # Arguments
/// * `country` - Mutable country with ministry config and tenders.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// The number of inspectorate tenders published.
///
/// # Rules
/// * Only triggers if `corruption_index > 0.2`.
/// * Only one inspectorate tender per turn (cooldown).
/// * Requires sufficient allocated cash (minimum 50,000).
/// * Uses real construction tender mechanism.
/// * Building types: "Sanitary Inspectorate", "Building Inspectorate", "Environmental Inspectorate".
pub fn maybe_publish_inspectorate_tender(country: &mut Country, current_turn: u32) -> usize {
    let corruption_index = country
        .politics
        .inspectorate_state
        .as_ref()
        .map(|ist| ist.corruption_index)
        .unwrap_or(0.0);

    if corruption_index <= 0.2 {
        return 0;
    }

    // Cooldown: check if there's already a pending inspectorate tender
    let has_pending = country
        .phase22_tenders
        .iter()
        .any(|t| t.target_building_type.contains("Inspectorate"));
    if has_pending {
        return 0;
    }

    // Find the Justice or InternalSecurity ministry with the most surplus cash
    let mc = match country.politics.ministry_config.as_ref() {
        Some(mc) => mc,
        None => return 0,
    };

    let security_ministries: Vec<&Ministry> = mc
        .ministries
        .iter()
        .filter(|m| {
            m.competencies.iter().any(|c| {
                matches!(
                    c,
                    GovernmentCompetency::Justice | GovernmentCompetency::InternalSecurity
                )
            })
        })
        .collect();

    if security_ministries.is_empty() {
        return 0;
    }

    // Find the security ministry with the most available cash
    let best_ministry = security_ministries.iter().max_by(|a, b| {
        let a_surplus = a.allocated_cash - a.spent_cash;
        let b_surplus = b.allocated_cash - b.spent_cash;
        a_surplus
            .partial_cmp(&b_surplus)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let ministry = match best_ministry {
        Some(m) => *m,
        None => return 0,
    };

    let surplus = ministry.allocated_cash - ministry.spent_cash;
    // D.4.4: Scale min surplus by average_wage (Rule 2: no magic numbers)
    let avg_wage = country.macro_indicators.average_wage;
    let min_surplus = avg_wage * 50.0;
    if surplus < min_surplus {
        return 0;
    }

    // Pick inspectorate building type based on what's most needed
    // (simplified: rotate through the three types)
    let building_types = [
        "Sanitary Inspectorate",
        "Building Inspectorate",
        "Environmental Inspectorate",
    ];
    let type_idx = (current_turn as usize) % building_types.len();
    let building_type = building_types[type_idx].to_string();

    // Get the first region ID for the tender
    let region_id = country
        .regions
        .first()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| "CENTRAL".to_string());

    // D.4.4: Scale cost bounds by average_wage (Rule 2: no magic numbers)
    let max_cost = avg_wage * 200.0;
    let min_cost = avg_wage * 50.0;
    let estimated_cost = surplus.min(max_cost).max(min_cost);

    let tender = crate::construction::tender_market::publish_tender(
        format!("STATE:{}", ministry.id),
        crate::construction::tenders::TenderInvestorType::State,
        crate::construction::ConstructionProjectType::Commercial,
        region_id,
        building_type,
        50, // target capacity (inspectors) — physical unit, not a magic number
        estimated_cost,
        estimated_cost,
        3, // 3-turn bidding window — temporal unit, not a magic number
        current_turn,
        crate::registries::enums::Sector::PublicAdministration,
        // Derive year from turn: 24 turns per year, default start year 1925
        1925 + (current_turn / 24),
    );

    country.phase22_tenders.push(tender);
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::legal_status::ShadowEconomyState;
    use crate::politics::laws::InspectorateState;
    use crate::politics::ministries::{Ministry, MinistryConfig};
    use crate::politics::system::Politics;

    fn make_country_with_corruption(corruption: f64) -> Country {
        let mut country = Country::default();
        country.politics = Politics::default();
        country.politics.inspectorate_state = Some(InspectorateState {
            corruption_index: corruption,
            ..Default::default()
        });
        country.politics.shadow_economy_state = Some(ShadowEconomyState {
            total_pit_evaded: 0.0,
            ..Default::default()
        });
        country.budget.liquid_reserves = 1_000_000.0;
        // D.4.4: Set realistic average_wage so dynamic thresholds work
        country.macro_indicators.average_wage = 1000.0;
        country
    }

    fn add_ministries(country: &mut Country, security_alloc: f64, other_alloc: f64) {
        let mc = MinistryConfig {
            ministries: vec![
                Ministry {
                    id: "MIN-SEC".to_string(),
                    name: "Ministry of Justice".to_string(),
                    competencies: vec![GovernmentCompetency::Justice],
                    minister_party: "PARTY-A".to_string(),
                    minister_name: "Minister A".to_string(),
                    allocated_cash: security_alloc,
                    spent_cash: 0.0,
                    spending_actions: Vec::new(),
                    ministry_cash: 0.0,
                },
                Ministry {
                    id: "MIN-EDU".to_string(),
                    name: "Ministry of Education".to_string(),
                    competencies: vec![GovernmentCompetency::Education],
                    minister_party: "PARTY-B".to_string(),
                    minister_name: "Minister B".to_string(),
                    allocated_cash: other_alloc,
                    spent_cash: 0.0,
                    spending_actions: Vec::new(),
                    ministry_cash: 0.0,
                },
            ],
            formation_turn: 0,
            pm_party: "PARTY-A".to_string(),
        };
        country.politics.ministry_config = Some(mc);
    }

    #[test]
    fn test_high_corruption_triggers_reallocation() {
        let mut country = make_country_with_corruption(0.6);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let reallocation = evaluate_anti_corruption_response(&country);
        assert!(reallocation.severity > 0.0);

        // Security ministry should get a positive delta
        let sec_delta = reallocation.deltas.get("MIN-SEC").copied().unwrap_or(0.0);
        assert!(sec_delta > 0.0, "Security ministry should gain funding");

        // Education ministry should get a negative delta
        let edu_delta = reallocation.deltas.get("MIN-EDU").copied().unwrap_or(0.0);
        assert!(edu_delta < 0.0, "Other ministry should lose funding");
    }

    #[test]
    fn test_low_corruption_no_reallocation() {
        let mut country = make_country_with_corruption(0.05);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let reallocation = evaluate_anti_corruption_response(&country);
        assert_eq!(reallocation.severity, 0.0);
        assert!(reallocation.deltas.is_empty());
    }

    #[test]
    fn test_reallocation_increases_security_allocation() {
        let mut country = make_country_with_corruption(0.6);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let reallocation = evaluate_anti_corruption_response(&country);
        apply_budget_reallocation(&mut country, &reallocation);

        let mc = country.politics.ministry_config.as_ref().unwrap();
        let sec_ministry = mc.ministries.iter().find(|m| m.id == "MIN-SEC").unwrap();
        assert!(
            sec_ministry.allocated_cash > 100_000.0,
            "Security allocation should increase"
        );
    }

    #[test]
    fn test_reallocation_decreases_other_allocation() {
        let mut country = make_country_with_corruption(0.6);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let reallocation = evaluate_anti_corruption_response(&country);
        apply_budget_reallocation(&mut country, &reallocation);

        let mc = country.politics.ministry_config.as_ref().unwrap();
        let edu_ministry = mc.ministries.iter().find(|m| m.id == "MIN-EDU").unwrap();
        assert!(
            edu_ministry.allocated_cash < 200_000.0,
            "Other allocation should decrease"
        );
    }

    #[test]
    fn test_no_ministry_config_no_crash() {
        let country = make_country_with_corruption(0.8);
        // No ministry_config set
        let reallocation = evaluate_anti_corruption_response(&country);
        assert!(reallocation.deltas.is_empty());
    }

    #[test]
    fn test_run_feedback_applies_reallocation() {
        let mut country = make_country_with_corruption(0.6);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let severity = run_anti_corruption_feedback(&mut country);
        assert!(severity > 0.0);

        let mc = country.politics.ministry_config.as_ref().unwrap();
        let sec_ministry = mc.ministries.iter().find(|m| m.id == "MIN-SEC").unwrap();
        assert!(sec_ministry.allocated_cash > 100_000.0);
    }

    #[test]
    fn test_inspectorate_tender_published_when_corruption_high() {
        let mut country = make_country_with_corruption(0.5);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let count = maybe_publish_inspectorate_tender(&mut country, 10);
        assert_eq!(count, 1);
        assert_eq!(country.phase22_tenders.len(), 1);
        let tender = &country.phase22_tenders[0];
        assert!(tender.target_building_type.contains("Inspectorate"));
    }

    #[test]
    fn test_no_inspectorate_tender_when_corruption_low() {
        let mut country = make_country_with_corruption(0.1);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        let count = maybe_publish_inspectorate_tender(&mut country, 10);
        assert_eq!(count, 0);
        assert!(country.phase22_tenders.is_empty());
    }

    #[test]
    fn test_no_inspectorate_tender_when_insufficient_cash() {
        let mut country = make_country_with_corruption(0.5);
        add_ministries(&mut country, 10_000.0, 200_000.0); // Security ministry has only 10k

        let count = maybe_publish_inspectorate_tender(&mut country, 10);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_inspectorate_tender_cooldown() {
        let mut country = make_country_with_corruption(0.5);
        add_ministries(&mut country, 100_000.0, 200_000.0);

        // First call publishes a tender
        let count1 = maybe_publish_inspectorate_tender(&mut country, 10);
        assert_eq!(count1, 1);

        // Second call should not publish another (cooldown)
        let count2 = maybe_publish_inspectorate_tender(&mut country, 11);
        assert_eq!(count2, 0);
    }
}
