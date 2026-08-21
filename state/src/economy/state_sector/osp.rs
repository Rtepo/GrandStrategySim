//! Phase 15A: Volunteer Fire Brigades (OSP — Ochotnicza fire_station).
//!
//! OSP are `Company` entities with `Sector::NGO` and `LegalForm::NonProfit`.
//! They are funded through the existing Phase 13 charity infrastructure
//! (`process_charity_fundraising`). This module handles the allocation of
//! volunteer FTE to OSP buildings based on regional population — volunteers
//! donate labor without wage cost.

#![allow(missing_docs)]

use crate::entities::{Building, Company, LegalForm};
use crate::registries::enums::Sector;
use crate::state::Country;

/// Default volunteer FTE per 1000 population.
const VOLUNTEER_RATE: f64 = 0.001;

/// Maximum volunteer FTE per OSP building.
const MAX_VOLUNTEER_FTE: u32 = 20;

/// Solidarity factor by cultural group (reuses Phase 13 solidarity logic).
fn solidarity_factor(cultural_group: &str) -> f64 {
    match cultural_group {
        "slavic" => 1.0,
        "germanic" => 0.8,
        "latin" => 0.9,
        "middle_eastern" => 1.2,
        "balkan" => 1.0,
        _ => 1.0,
    }
}

/// Check if a company is an OSP (NGO with NonProfit legal form).
pub fn is_osp(company: &Company) -> bool {
    company.sector == Sector::NGO
        && matches!(company.legal_form, LegalForm::NonProfit(_))
}

/// Allocate volunteer FTE to OSP buildings based on regional population.
///
/// # Arguments
/// * `companies` - Mutable companies (used to identify OSP companies).
/// * `buildings` - Mutable buildings (volunteer FTE injected into `current_employment`).
/// * `country` - Country (reads regional population and cultural group).
///
/// # Rules
/// * For each OSP company, find its buildings in each region.
/// * Volunteer FTE = `region_population * VOLUNTEER_RATE * solidarity_factor`.
/// * FTE is capped at `MAX_VOLUNTEER_FTE` per building and `worker_capacity`.
/// * Volunteers do NOT earn wages — this is labor donation, not employment.
/// * The FTE is added to `current_employment` so production can utilize it.
///
/// # Double-Entry
/// * No money is transferred. Labor is donated by the community.
/// * The FTE appears in `current_employment` for production calculation
///   but does not generate wage obligations (OSP buildings have zero wage budget).
pub fn process_osp_volunteer_allocation(
    companies: &[Company],
    buildings: &mut [Building],
    country: &Country,
) {
    let cultural_group = &country.macro_indicators.cultural_group;
    let factor = solidarity_factor(cultural_group);

    // Collect OSP company IDs.
    let osp_ids: Vec<String> = companies
        .iter()
        .filter(|c| is_osp(c))
        .map(|c| c.id.clone())
        .collect();

    if osp_ids.is_empty() {
        return;
    }

    // Map region_id → population.
    let region_pops: std::collections::HashMap<&str, i64> = country
        .regions
        .iter()
        .map(|r| (r.id.as_str(), r.population))
        .collect();

    for building in buildings.iter_mut() {
        // Check if this building belongs to an OSP company.
        if !osp_ids.iter().any(|id| id == &building.owner_id) {
            continue;
        }

        // Get region population.
        let pop = match region_pops.get(building.region_id.as_str()) {
            Some(p) => *p,
            None => continue,
        };
        if pop <= 0 {
            continue;
        }

        // Calculate volunteer FTE.
        let volunteer_fte = ((pop as f64 * VOLUNTEER_RATE * factor) as u32).min(MAX_VOLUNTEER_FTE);
        if volunteer_fte == 0 {
            continue;
        }

        // Inject volunteer FTE into current_employment, capped by worker_capacity.
        let new_employment = (building.current_employment + volunteer_fte).min(building.worker_capacity);
        building.current_employment = new_employment;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::NonProfitData;

    #[test]
    fn test_is_osp() {
        let osp = Company {
            sector: Sector::NGO,
            legal_form: LegalForm::NonProfit(NonProfitData::default()),
            ..Company::default()
        };
        assert!(is_osp(&osp));

        let not_osp = Company {
            sector: Sector::Mining,
            legal_form: LegalForm::NonProfit(NonProfitData::default()),
            ..Company::default()
        };
        assert!(!is_osp(&not_osp));
    }
}
