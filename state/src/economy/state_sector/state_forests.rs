//! State Forests timber management (Phase 15C).
//!
//! Implements the State Forests mechanic: timber growth,
//! sustainable harvest, and profit remittance from a `StateMonopoly` company
//! to the Central Treasury.
//!
//! ## Double-Entry Rules
//!
//! - Timber is physically harvested from `timber_stock` (no phantom creation).
//! - Harvested timber enters the forest_district building's inventory for B2B sale.
//! - Revenue from B2B sales accrues to the company's `available_cash`.
//! - Profit remittance: routed through `settle_transfer` to sync bank reserves.
//!   Clean portion → Treasury, corruption leakage → ForeignEntity (money leaves system).
//! - `corruption_level` reduces the actual transfer (leakage to phantom).

use crate::entities::Company;
use crate::entities::legal_form::StateMonopolyData;
use crate::economy::transfer_settler::{settle_transfer, TransferRecipient};
use crate::registries::enums::Commodity;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// A single forest tract managed by State Forests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForestDistrictTract {
    /// Region where this tract is located.
    #[serde(default)]
    pub region_id: String,
    /// Total hectares of forest land.
    #[serde(default)]
    pub hectares: f64,
    /// Current standing timber stock (in cubic metres).
    #[serde(default)]
    pub timber_stock: f64,
    /// Annual growth rate per hectare (fraction, e.g. 0.03 = 3% per year).
    #[serde(default)]
    pub growth_rate: f64,
    /// Maximum harvest permitted per turn (cubic metres). Set by policy.
    #[serde(default)]
    pub harvest_permitted: f64,
}

/// State Forests runtime state (on `Country`, Phase 15C).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForestDistrictState {
    /// All forest tracts under management.
    #[serde(default)]
    pub tracts: Vec<ForestDistrictTract>,
    /// Total hectares across all tracts.
    #[serde(default)]
    pub total_hectares: f64,
    /// Total timber harvested this turn.
    #[serde(default)]
    pub annual_harvest: f64,
    /// Total revenue from timber sales this turn.
    #[serde(default)]
    pub timber_revenue: f64,
    /// Total profit remitted to Treasury this turn.
    #[serde(default)]
    pub treasury_remittance: f64,
}

/// Result of processing one State Forests turn.
#[derive(Debug, Clone, Default)]
pub struct ForestDistrictTurnResult {
    /// Total timber grown this turn (cubic metres).
    pub timber_growth: f64,
    /// Total timber harvested this turn (cubic metres).
    pub timber_harvested: f64,
    /// Profit remitted to Treasury.
    pub treasury_remittance: f64,
}

/// Weather modifier for timber growth.
///
/// Drought reduces growth by 50%, favourable weather boosts by 20%.
fn weather_growth_modifier(country: &Country) -> f64 {
    let active_disasters = country.weather_state.active_events.len();
    if active_disasters > 0 {
        0.5
    } else {
        1.0
    }
}

/// Processes one State Forests turn: grow timber, harvest, and remit profits.
///
/// # Arguments
/// * `country` - Mutable country (for `ForestDistrictState` and `Treasury`).
/// * `companies` - Mutable companies (to find the StateMonopoly forestry company
///   and debit its `available_cash` for the treasury transfer).
/// * `buildings` - Buildings (to find forest_district buildings and inject timber
///   into their inventory).
///
/// # Returns
/// `ForestDistrictTurnResult` with growth, harvest, and remittance statistics.
///
/// # Rules
/// - Timber growth: `timber_stock += hectares * growth_rate * weather_modifier`.
/// - Harvest: `harvested = min(timber_stock, harvest_permitted)`.
/// - Timber enters forest_district building inventory as `Commodity::Timber`.
/// - Profit remittance: routed through `settle_transfer` — clean portion to Treasury,
///   corruption leakage to ForeignEntity. Bank reserves stay synchronized.
/// - Strict double-entry: no phantom money or timber.
pub fn process_state_forests_turn(
    country: &mut Country,
    companies: &mut [Company],
    buildings: &mut [crate::entities::Building],
) -> ForestDistrictTurnResult {
    let mut result = ForestDistrictTurnResult::default();
    let weather_mod = weather_growth_modifier(country);

    // 1. Grow timber on all tracts
    let tracts = &mut country.state_forest_state.tracts;
    for tract in tracts.iter_mut() {
        let growth = tract.hectares * tract.growth_rate * weather_mod * 0.5;
        tract.timber_stock += growth;
        result.timber_growth += growth;
    }

    // 2. Harvest timber (respecting harvest_permitted per tract)
    let mut total_harvested = 0.0_f64;
    for tract in tracts.iter_mut() {
        let harvest = tract.timber_stock.min(tract.harvest_permitted);
        tract.timber_stock -= harvest;
        total_harvested += harvest;
    }
    result.timber_harvested = total_harvested;
    country.state_forest_state.annual_harvest = total_harvested;

    // 3. Inject harvested timber into forest_district buildings
    let state_forest_buildings: Vec<usize> = buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "forest_district")
        .map(|(i, _)| i)
        .collect();

    if !state_forest_buildings.is_empty() && total_harvested > 0.0 {
        let per_building = total_harvested / state_forest_buildings.len() as f64;
        for &idx in &state_forest_buildings {
            let b = &mut buildings[idx];
            let current = b.inventory.get(&Commodity::Timber).copied().unwrap_or(0.0);
            b.inventory.insert(Commodity::Timber, current + per_building);
        }
    }

    // 4. Profit remittance from StateMonopoly forestry company to Treasury
    let forestry_idx = companies.iter().position(|c| {
        matches!(&c.legal_form, crate::entities::legal_form::LegalForm::StateMonopoly(d) if d.controlled_sector == "Forestry")
    });

    if let Some(idx) = forestry_idx {
        let data = match &companies[idx].legal_form {
            crate::entities::legal_form::LegalForm::StateMonopoly(d) => d.clone(),
            _ => {
                // unreachable due to position check above
                return result;
            }
        };

        // Remit available cash above a minimum operating reserve
        let available = companies[idx].brokerage_account.as_ref().map(|b| b.cash).unwrap_or(companies[idx].available_cash);
        let operating_reserve = 10_000.0;
        let transferable = (available - operating_reserve).max(0.0);
        let transfer = transferable.min(data.direct_treasury_transfer);

        if transfer > 0.01 {
            // Corruption leakage: (1 - corruption_level) reaches Treasury
            let leakage = transfer * data.corruption_level;
            let to_treasury = transfer - leakage;

            // Clean portion to Treasury via TransferSettler
            if to_treasury > 0.01 {
                let _ = settle_transfer(
                    companies, idx, to_treasury,
                    &TransferRecipient::Treasury, country,
                );
            }
            // Corruption leakage: money leaves the system.
            // Phase 46: If CB has insufficient FX reserves (capital controls),
            // the transfer is rejected and the corrupt official keeps the money.
            // This is realistic — capital flight is blocked during currency crises.
            if leakage > 0.01 {
                let _ = settle_transfer(
                    companies, idx, leakage,
                    &TransferRecipient::ForeignEntity, country,
                );
            }

            result.treasury_remittance += to_treasury;
            country.state_forest_state.treasury_remittance = to_treasury;
        }
    }

    // Update total hectares
    country.state_forest_state.total_hectares = country
        .state_forest_state
        .tracts
        .iter()
        .map(|t| t.hectares)
        .sum();

    result
}

/// Creates a default `ForestDistrictState` with sample tracts for a new country.
///
/// # Arguments
/// * `region_ids` - Region IDs to create tracts for.
/// * `hectares_per_region` - Hectares of forest per region.
///
/// # Returns
/// A `ForestDistrictState` populated with one tract per region.
pub fn create_default_state_forests(
    region_ids: &[String],
    hectares_per_region: f64,
) -> ForestDistrictState {
    let tracts: Vec<ForestDistrictTract> = region_ids
        .iter()
        .map(|rid| ForestDistrictTract {
            region_id: rid.clone(),
            hectares: hectares_per_region,
            timber_stock: hectares_per_region * 100.0, // Initial stock
            growth_rate: 0.03,                          // 3% per year
            harvest_permitted: hectares_per_region * 3.0, // 3 m³/ha/turn
        })
        .collect();

    let total_hectares = tracts.iter().map(|t| t.hectares).sum();

    ForestDistrictState {
        tracts,
        total_hectares,
        annual_harvest: 0.0,
        timber_revenue: 0.0,
        treasury_remittance: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Company;
    use crate::entities::legal_form::{LegalForm, StateMonopolyData};
    use crate::state::Country;

    #[test]
    fn test_timber_growth_and_harvest() {
        let mut country = Country::mock_for_tests();
        country.state_forest_state = ForestDistrictState {
            tracts: vec![ForestDistrictTract {
                region_id: "R1".to_string(),
                hectares: 1000.0,
                timber_stock: 50000.0,
                growth_rate: 0.03,
                harvest_permitted: 3000.0,
            }],
            total_hectares: 1000.0,
            ..Default::default()
        };

        let mut companies: Vec<Company> = Vec::new();
        let mut buildings: Vec<crate::entities::Building> = Vec::new();

        let result = process_state_forests_turn(&mut country, &mut companies, &mut buildings);

        // Growth: 1000 * 0.03 * 1.0 * 0.5 = 15.0
        assert!((result.timber_growth - 15.0).abs() < 0.01, "timber growth should be ~15, got {}", result.timber_growth);
        // Harvest: min(50015, 3000) = 3000
        assert!((result.timber_harvested - 3000.0).abs() < 0.01, "harvest should be 3000, got {}", result.timber_harvested);
        // Stock after: 50015 - 3000 = 47015
        assert!((country.state_forest_state.tracts[0].timber_stock - 47015.0).abs() < 0.01, "remaining stock should be ~47015");
    }

    #[test]
    fn test_treasury_remittance_double_entry() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        // Phase 46: Add FX reserves so corruption leakage transfer can succeed
        country.central_bank.fx_reserves.insert("USD".to_string(), 100_000.0);

        let mut company = Company::default();
        company.available_cash = 50_000.0;
        company.legal_form = LegalForm::StateMonopoly(StateMonopolyData {
            controlled_sector: "Forestry".to_string(),
            direct_treasury_transfer: 20_000.0,
            corruption_level: 0.1,
            ..Default::default()
        });

        let mut companies = vec![company];
        let mut buildings: Vec<crate::entities::Building> = Vec::new();

        let result = process_state_forests_turn(&mut country, &mut companies, &mut buildings);

        // Transferable = 50000 - 10000 = 40000, capped at 20000
        // To treasury = 20000 * (1 - 0.1) = 18000
        // Leakage = 20000 * 0.1 = 2000 (routed to ForeignEntity, drains FX reserves)
        assert!((result.treasury_remittance - 18_000.0).abs() < 0.01, "remittance should be 18000, got {}", result.treasury_remittance);
        assert!((country.budget.liquid_reserves - 18_000.0).abs() < 0.01, "treasury should have 18000");
        assert!((companies[0].available_cash - 30_000.0).abs() < 0.01, "company should have 30000 left, got {}", companies[0].available_cash);
    }

    #[test]
    fn test_timber_injected_into_state_forest() {
        let mut country = Country::mock_for_tests();
        country.state_forest_state = ForestDistrictState {
            tracts: vec![ForestDistrictTract {
                region_id: "R1".to_string(),
                hectares: 1000.0,
                timber_stock: 5000.0,
                growth_rate: 0.03,
                harvest_permitted: 1000.0,
            }],
            ..Default::default()
        };

        let mut buildings = vec![
            crate::entities::Building {
                name: "forest_district".to_string(),
                ..Default::default()
            },
            crate::entities::Building {
                name: "Factory".to_string(),
                ..Default::default()
            },
        ];

        let mut companies: Vec<Company> = Vec::new();
        process_state_forests_turn(&mut country, &mut companies, &mut buildings);

        let timber_in_building = buildings[0].inventory.get(&Commodity::Timber).copied().unwrap_or(0.0);
        assert!(timber_in_building > 0.0, "forest_district should have timber in inventory");
        let timber_in_factory = buildings[1].inventory.get(&Commodity::Timber).copied().unwrap_or(0.0);
        assert_eq!(timber_in_factory, 0.0, "non-forest_district building should have no timber");
    }

    #[test]
    fn test_no_harvest_when_stock_zero() {
        let mut country = Country::mock_for_tests();
        country.state_forest_state = ForestDistrictState {
            tracts: vec![ForestDistrictTract {
                region_id: "R1".to_string(),
                hectares: 1000.0,
                timber_stock: 0.0,
                growth_rate: 0.03,
                harvest_permitted: 0.0,
            }],
            ..Default::default()
        };

        let mut companies: Vec<Company> = Vec::new();
        let mut buildings: Vec<crate::entities::Building> = Vec::new();

        let result = process_state_forests_turn(&mut country, &mut companies, &mut buildings);

        assert!((result.timber_harvested - 0.0).abs() < 0.01, "no harvest from empty stock");
        assert!(result.timber_growth > 0.0, "should still grow timber");
    }

    #[test]
    fn test_create_default_state_forests() {
        let state = create_default_state_forests(&["R1".to_string(), "R2".to_string()], 500.0);
        assert_eq!(state.tracts.len(), 2);
        assert!((state.total_hectares - 1000.0).abs() < 0.01);
        assert!((state.tracts[0].timber_stock - 50000.0).abs() < 0.01);
    }
}
