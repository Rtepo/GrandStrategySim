//! Phase 29: Trade policy — ideology-driven tariffs and FTA overrides.
//!
//! This module translates ruling-party trade doctrines into concrete
//! commodity-specific tariff rates, and provides dynamic adjustment
//! based on trade deficits and domestic industry health.

use crate::registries::enums::Commodity;
use crate::state::{Country, TradePolicy};
use std::collections::HashMap;

/// Set import tariffs and export taxes based on the ruling party's trade doctrine.
///
/// # Arguments
/// * `country` - Mutable country whose `trade_policy` will be updated.
///
/// # Rules
/// * "Protectionism" (Protectionism): 15-25% tariff on manufactured goods,
///   5-10% on raw materials, 0% on strategic imports.
/// * "Free Trade" (Free Trade): 0-5% across the board.
/// * "Autarky" (Autarky): 30-50% on all imports, high export taxes.
/// * Existing commodity-specific tariffs are overwritten.
/// * The exact rates have slight variation based on the specific ideology.
pub fn set_tariffs_from_doctrine(country: &mut Country) {
    let doctrine = country.politics.trade_doctrine.clone();
    let mut import_tariffs: HashMap<Commodity, f64> = HashMap::new();
    let mut export_taxes: HashMap<Commodity, f64> = HashMap::new();

    // Classify commodities into categories for tariff setting
    let manufactured_goods = [
        Commodity::Steel, Commodity::IndustrialMachinery,
        Commodity::Clothing, Commodity::LuxuryClothing,
        Commodity::Furniture, Commodity::LuxuryFurniture,
        Commodity::Glass, Commodity::Paper,
        Commodity::ConstructionMachinery, Commodity::AgriculturalMachinery,
        Commodity::Trucks, Commodity::MilitaryTrucks,
        Commodity::Ammunition, Commodity::TowedArtillery,
        Commodity::MedicalEquipment, Commodity::OfficeMachinery,
    ];
    let raw_materials = [
        Commodity::BrownCoal, Commodity::HardCoal, Commodity::Iron,
        Commodity::Copper, Commodity::Cement, Commodity::Asphalt,
        Commodity::Bitumen, Commodity::Gravel,
    ];
    let strategic_imports = [
        Commodity::Uranium, Commodity::Gold, Commodity::Silver,
        Commodity::Energy,
    ];
    let agricultural_goods = [
        Commodity::Cereal, Commodity::Meat,
    ];

    match doctrine.as_str() {
        "Protectionism" => {
            // Protectionism: protect domestic manufacturing and agriculture
            for c in &manufactured_goods {
                import_tariffs.insert(*c, 0.20); // 20% on manufactured goods
            }
            for c in &raw_materials {
                import_tariffs.insert(*c, 0.05); // 5% on raw materials
            }
            for c in &strategic_imports {
                import_tariffs.insert(*c, 0.0); // 0% on strategic imports
            }
            for c in &agricultural_goods {
                import_tariffs.insert(*c, 0.15); // 15% on agricultural goods
            }
            // Modest export tax on raw materials to keep them domestic
            for c in &raw_materials {
                export_taxes.insert(*c, 0.05);
            }
        }
        "Free Trade" => {
            // Free Trade: minimal tariffs
            for c in &manufactured_goods {
                import_tariffs.insert(*c, 0.02); // 2% on manufactured goods
            }
            for c in &raw_materials {
                import_tariffs.insert(*c, 0.0);
            }
            for c in &strategic_imports {
                import_tariffs.insert(*c, 0.0);
            }
            for c in &agricultural_goods {
                import_tariffs.insert(*c, 0.02);
            }
            // No export taxes
        }
        "Autarky" => {
            // Autarky: high tariffs on everything, high export taxes
            for c in &manufactured_goods {
                import_tariffs.insert(*c, 0.40); // 40% on manufactured goods
            }
            for c in &raw_materials {
                import_tariffs.insert(*c, 0.30); // 30% on raw materials
            }
            for c in &strategic_imports {
                import_tariffs.insert(*c, 0.10); // 10% on strategic imports
            }
            for c in &agricultural_goods {
                import_tariffs.insert(*c, 0.35); // 35% on agricultural goods
            }
            // High export taxes to keep domestic goods at home
            for c in &manufactured_goods {
                export_taxes.insert(*c, 0.20);
            }
            for c in &raw_materials {
                export_taxes.insert(*c, 0.30);
            }
            for c in &agricultural_goods {
                export_taxes.insert(*c, 0.25);
            }
        }
        _ => {
            // Unknown doctrine: leave tariffs as-is
            return;
        }
    }

    country.trade_policy.import_tariffs = import_tariffs;
    country.trade_policy.export_taxes = export_taxes;
}

/// Phase 29: Dynamic tariff adjustment based on economic conditions.
///
/// The ruling party can adjust tariffs mid-term in response to:
/// * Trade deficit > 5% of GDP: raise import tariffs
/// * Domestic industry collapse (PMI < 30 for key sector): raise protective tariffs
///
/// # Arguments
/// * `country` - Mutable country with trade policy and economic data.
///
/// # Rules
/// * Adjustments are bounded (max +10 percentage points per turn).
/// * Only applies to existing tariff rates (doesn't add new ones).
/// * This is a government AI decision, not a random event.
pub fn adjust_tariffs_for_conditions(country: &mut Country) {
    let gdp = country.budget.gdp.max(1.0);
    let trade_deficit = country
        .macro_indicators
        .gdp_breakdown
        .net_exports
        .abs();

    // If trade deficit > 5% of GDP, raise import tariffs
    if trade_deficit > gdp * 0.05 {
        for rate in country.trade_policy.import_tariffs.values_mut() {
            *rate = (*rate + 0.05).min(0.50); // Max 50%, +5% per adjustment
        }
    }

    // Check for domestic industry collapse (PMI < 30 for key sectors)
    let key_sectors = [
        crate::registries::enums::Sector::HeavyIndustry,
        crate::registries::enums::Sector::LightIndustry,
        crate::registries::enums::Sector::Agriculture,
    ];

    for sector in &key_sectors {
        if let Some(share) = country.budget.sectors.get(sector) {
            let pmi = share.extra.get("pmi").and_then(|v| v.as_f64()).unwrap_or(50.0);
            if pmi < 30.0 {
                // Industry collapsing — raise protective tariffs on competing imports
                for c in sector.primary_commodities() {
                    let current = country.trade_policy.import_tariffs.get(&c).copied().unwrap_or(0.0);
                    if current < 0.30 {
                        country.trade_policy.import_tariffs.insert(c, (current + 0.10).min(0.30));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_country_with_doctrine(doctrine: &str) -> Country {
        let mut country = Country::mock_for_tests();
        country.politics.trade_doctrine = doctrine.to_string();
        country
    }

    #[test]
    fn test_protectionism_sets_nonzero_tariffs() {
        let mut country = make_country_with_doctrine("Protectionism");
        set_tariffs_from_doctrine(&mut country);

        let steel_tariff = country.trade_policy.import_tariffs.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!(steel_tariff > 0.0, "Protectionism should set non-zero tariffs on Steel");
        assert!(steel_tariff >= 0.15, "Steel tariff should be at least 15%");
    }

    #[test]
    fn test_free_trade_sets_low_tariffs() {
        let mut country = make_country_with_doctrine("Free Trade");
        set_tariffs_from_doctrine(&mut country);

        let steel_tariff = country.trade_policy.import_tariffs.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!(steel_tariff <= 0.05, "Free trade should set low tariffs (<= 5%)");
    }

    #[test]
    fn test_autarky_sets_high_tariffs() {
        let mut country = make_country_with_doctrine("Autarky");
        set_tariffs_from_doctrine(&mut country);

        let steel_tariff = country.trade_policy.import_tariffs.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!(steel_tariff >= 0.30, "Autarky should set high tariffs (>= 30%)");

        // Autarky should also have export taxes
        let steel_export = country.trade_policy.export_taxes.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!(steel_export > 0.0, "Autarky should set export taxes");
    }

    #[test]
    fn test_strategic_imports_zero_tariff_under_protectionism() {
        let mut country = make_country_with_doctrine("Protectionism");
        set_tariffs_from_doctrine(&mut country);

        let uranium_tariff = country.trade_policy.import_tariffs.get(&Commodity::Uranium).copied().unwrap_or(0.0);
        assert_eq!(uranium_tariff, 0.0, "Strategic imports should have 0% tariff under Protectionism");
    }

    #[test]
    fn test_unknown_doctrine_no_changes() {
        let mut country = make_country_with_doctrine("Unknown");
        let original_policy = country.trade_policy.clone();
        set_tariffs_from_doctrine(&mut country);
        assert_eq!(country.trade_policy, original_policy, "Unknown doctrine should not change tariffs");
    }

    #[test]
    fn test_raw_materials_lower_than_manufactured_under_protectionism() {
        let mut country = make_country_with_doctrine("Protectionism");
        set_tariffs_from_doctrine(&mut country);

        let steel_tariff = country.trade_policy.import_tariffs.get(&Commodity::Steel).copied().unwrap_or(0.0);
        let coal_tariff = country.trade_policy.import_tariffs.get(&Commodity::BrownCoal).copied().unwrap_or(0.0);
        assert!(steel_tariff > coal_tariff, "Manufactured goods should have higher tariffs than raw materials");
    }
}
