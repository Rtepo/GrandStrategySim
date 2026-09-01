//! Retail registry for B2C market configuration (Phase 6.5).
//!
//! Provides commodity profiles for retail stores, retail upgrade effects,
//! and retail configuration parameters for pricing and consumer behavior.

use crate::registries::enums::Commodity;
use crate::society::housing::{RetailUpgrade, StoreProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Commodity profile for retail stores
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommodityProfile {
    /// Base attractiveness multiplier for this commodity
    pub attractiveness_multiplier: f64,

    /// Per-unit storage requirements (sq meters per unit)
    pub storage_sqm_per_unit: f64,

    /// Perishability flag (true = requires cold storage)
    pub perishable: bool,
}

/// Retail configuration parameters (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetailConfig {
    /// Consumer inertia weight (0.0-1.0) for brand loyalty
    pub inertia_weight: f64,

    /// Grace period turns for new stores before inertia applies
    pub newcomer_grace_turns: u32,

    /// Expected turnover rate for capacity amortization
    pub expected_turnover_rate: f64,

    /// Minimum throughput units for pricing floor
    pub min_throughput_units: f64,

    /// Default markup ratio for new stores
    pub default_markup_ratio: f64,
}

impl Default for RetailConfig {
    fn default() -> Self {
        Self {
            inertia_weight: 0.3,
            newcomer_grace_turns: 5,
            expected_turnover_rate: 0.5,
            min_throughput_units: 10.0,
            default_markup_ratio: 1.5,
        }
    }
}

/// Returns the static commodity profile map for retail stores.
///
/// # Returns
/// * `&'static BTreeMap<Commodity, CommodityProfile>` — commodity → profile
///
/// # Rules
/// * Registry is exhaustive over the Commodity enum
/// * Used for storage requirements and attractiveness calculations
pub fn commodity_profile_map() -> &'static BTreeMap<Commodity, CommodityProfile> {
    static MAP: OnceLock<BTreeMap<Commodity, CommodityProfile>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = BTreeMap::new();

        // Agricultural commodities
        m.insert(
            Commodity::Cereal,
            CommodityProfile {
                attractiveness_multiplier: 1.0,
                storage_sqm_per_unit: 0.01,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Vegetable,
            CommodityProfile {
                attractiveness_multiplier: 1.2,
                storage_sqm_per_unit: 0.02,
                perishable: true,
            },
        );
        m.insert(
            Commodity::Fruit,
            CommodityProfile {
                attractiveness_multiplier: 1.15,
                storage_sqm_per_unit: 0.02,
                perishable: true,
            },
        );
        m.insert(
            Commodity::Meat,
            CommodityProfile {
                attractiveness_multiplier: 1.4,
                storage_sqm_per_unit: 0.04,
                perishable: true,
            },
        );

        // Industrial commodities (lower retail attractiveness)
        m.insert(
            Commodity::Timber,
            CommodityProfile {
                attractiveness_multiplier: 0.8,
                storage_sqm_per_unit: 0.1,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Stone,
            CommodityProfile {
                attractiveness_multiplier: 0.7,
                storage_sqm_per_unit: 0.2,
                perishable: false,
            },
        );

        // Luxury goods
        m.insert(
            Commodity::LuxuryFurniture,
            CommodityProfile {
                attractiveness_multiplier: 2.0,
                storage_sqm_per_unit: 0.5,
                perishable: false,
            },
        );
        m.insert(
            Commodity::LuxuryClothing,
            CommodityProfile {
                attractiveness_multiplier: 2.5,
                storage_sqm_per_unit: 0.05,
                perishable: false,
            },
        );

        // Phase 20: Consumer durables and electronics
        m.insert(
            Commodity::Radio,
            CommodityProfile {
                attractiveness_multiplier: 1.5,
                storage_sqm_per_unit: 0.1,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Televisions,
            CommodityProfile {
                attractiveness_multiplier: 1.8,
                storage_sqm_per_unit: 0.3,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Agd,
            CommodityProfile {
                attractiveness_multiplier: 1.6,
                storage_sqm_per_unit: 0.4,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Cars,
            CommodityProfile {
                attractiveness_multiplier: 3.0,
                storage_sqm_per_unit: 5.0,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Clothing,
            CommodityProfile {
                attractiveness_multiplier: 1.2,
                storage_sqm_per_unit: 0.05,
                perishable: false,
            },
        );
        m.insert(
            Commodity::Furniture,
            CommodityProfile {
                attractiveness_multiplier: 1.4,
                storage_sqm_per_unit: 0.3,
                perishable: false,
            },
        );

        // Phase 30: Motor fuel commodities for gas station retail.
        m.insert(
            Commodity::Fuels,
            CommodityProfile {
                attractiveness_multiplier: 0.9,
                storage_sqm_per_unit: 0.05,
                perishable: false,
            },
        );
        m.insert(
            Commodity::RefinedFuel,
            CommodityProfile {
                attractiveness_multiplier: 0.9,
                storage_sqm_per_unit: 0.05,
                perishable: false,
            },
        );

        m
    })
}

/// Returns the retail upgrade effectiveness map.
///
/// # Returns
/// * `&'static BTreeMap<RetailUpgrade, f64>` — upgrade → attractiveness bonus
///
/// # Rules
/// * Bonus is additive to base attractiveness
/// * Used in R2 phase to compute effective_attractiveness
pub fn retail_upgrade_bonus() -> &'static BTreeMap<RetailUpgrade, f64> {
    static BONUS: OnceLock<BTreeMap<RetailUpgrade, f64>> = OnceLock::new();
    BONUS.get_or_init(|| {
        let mut m = BTreeMap::new();
        m.insert(RetailUpgrade::CityScales, 0.5);
        m.insert(RetailUpgrade::PavedSquare, 0.3);
        m.insert(RetailUpgrade::CoveredHall, 0.4);
        m.insert(RetailUpgrade::ColdCounter, 0.6);
        m.insert(RetailUpgrade::Advertising, 0.7);
        m
    })
}

/// Returns the retail configuration.
///
/// # Returns
/// * `&'static RetailConfig`
pub fn retail_config() -> &'static RetailConfig {
    static CONFIG: OnceLock<RetailConfig> = OnceLock::new();
    CONFIG.get_or_init(RetailConfig::default)
}

/// Get store profile compatibility with commodity.
///
/// # Arguments
/// * `profile` - Store profile type
/// * `commodity` - Commodity to check
///
/// # Returns
/// * `true` if the store profile can sell this commodity
///
/// # Rules
/// * Grocery stores sell food staples
/// * Butcher shops sell protein
/// * Bakeries sell cereal-based goods
/// * Clothing stores sell clothing
/// * Household stores sell furniture/household goods
/// /// Electronics stores sell machinery/electronics
/// /// Luxury stores sell luxury goods
pub fn is_compatible(profile: StoreProfile, commodity: Commodity) -> bool {
    match profile {
        StoreProfile::Grocery => matches!(
            commodity,
            Commodity::Cereal
                | Commodity::Vegetable
                | Commodity::Fruit
                | Commodity::Food
                | Commodity::Meat // Phase 20: Meat in grocery
        ),
        StoreProfile::Butcher => matches!(commodity, Commodity::Meat | Commodity::Fish),
        StoreProfile::Bakery => matches!(commodity, Commodity::Cereal | Commodity::Food),
        StoreProfile::Clothing => matches!(
            commodity,
            Commodity::Clothing | Commodity::LuxuryClothing // Phase 20: LuxuryClothing
        ),
        StoreProfile::Household => matches!(
            commodity,
            Commodity::Furniture | Commodity::LuxuryFurniture | Commodity::Agd // Phase 20: Agd
        ),
        StoreProfile::Electronics => matches!(
            commodity,
            Commodity::OfficeMachinery
                | Commodity::ElectronicComponents
                | Commodity::Radio
                | Commodity::Televisions // Phase 20: Radio, Televisions
        ),
        StoreProfile::Luxury => matches!(
            commodity,
            Commodity::LuxuryFurniture | Commodity::Luxury | Commodity::LuxuryClothing // Phase 20: LuxuryClothing
        ),
        StoreProfile::CarDealer => matches!(
            commodity,
            Commodity::Cars | Commodity::Trucks // Phase 20: Car dealer profile
        ),
        StoreProfile::GasStation => matches!(
            commodity,
            Commodity::Fuels | Commodity::RefinedFuel // Phase 30: Motor fuel retail
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 47: Retail Format Registry
// ═══════════════════════════════════════════════════════════════════════

use crate::society::housing::CommercialBuildingType;

/// Phase 47: Retail format specification for Genesis selection.
#[derive(Debug, Clone)]
pub struct RetailFormatSpec {
    /// Building type for this format
    pub building_type: CommercialBuildingType,
    /// Minimum development_level required (0.0–1.0)
    pub min_development: f64,
    /// Minimum era year required (e.g. 1950 for supermarkets)
    pub min_year: u32,
    /// Base attractiveness multiplier
    pub attractiveness: f64,
    /// Markup ratio over wholesale cost
    pub markup: f64,
    /// Upkeep multiplier (relative to RetailStore = 1.0)
    pub upkeep_mult: f64,
    /// Capacity multiplier (relative to RetailStore = 1.0)
    pub capacity_mult: f64,
    /// Allowed store profiles
    pub allowed_profiles: Vec<StoreProfile>,
    /// Whether this format requires capital/urban status
    pub requires_capital: bool,
}

/// Phase 47: Select a retail format for a region based on development, wealth, era,
/// and capital status. Returns the format specification.
///
/// # Selection Logic
/// - Poor/rural/early-era regions → Marketplace (traditional open-air)
/// - Mid-development → RetailStore (small independent shop)
/// - Wealthy/urban/modern-era → supermarket or DepartmentStore
/// - Capital + high development + modern era → ShoppingCenter
pub fn select_retail_format(
    development_level: f64,
    year: u32,
    is_capital: bool,
    wealth_per_capita: f64,
) -> RetailFormatSpec {
    // Try formats from most advanced to most basic; pick the first that qualifies.
    // ShoppingCenter: capital + high development + modern era + high wealth
    if is_capital && development_level >= 0.85 && year >= 1975 && wealth_per_capita >= 2000.0 {
        return RetailFormatSpec {
            building_type: CommercialBuildingType::ShoppingCenter,
            min_development: 0.85,
            min_year: 1975,
            attractiveness: 1.5,
            markup: 1.4,
            upkeep_mult: 2.5,
            capacity_mult: 5.0,
            allowed_profiles: vec![
                StoreProfile::Grocery,
                StoreProfile::Clothing,
                StoreProfile::Household,
                StoreProfile::Electronics,
                StoreProfile::Luxury,
            ],
            requires_capital: true,
        };
    }

    // DepartmentStore: high development + mid-modern era
    if development_level >= 0.70 && year >= 1950 && wealth_per_capita >= 1000.0 {
        return RetailFormatSpec {
            building_type: CommercialBuildingType::DepartmentStore,
            min_development: 0.70,
            min_year: 1950,
            attractiveness: 1.3,
            markup: 1.35,
            upkeep_mult: 2.0,
            capacity_mult: 3.5,
            allowed_profiles: vec![
                StoreProfile::Grocery,
                StoreProfile::Clothing,
                StoreProfile::Household,
                StoreProfile::Electronics,
                StoreProfile::Luxury,
            ],
            requires_capital: false,
        };
    }

    // supermarket: mid development + modern era
    if development_level >= 0.50 && year >= 1940 && wealth_per_capita >= 500.0 {
        return RetailFormatSpec {
            building_type: CommercialBuildingType::Supermarket,
            min_development: 0.50,
            min_year: 1940,
            attractiveness: 1.15,
            markup: 1.25,
            upkeep_mult: 1.5,
            capacity_mult: 2.5,
            allowed_profiles: vec![
                StoreProfile::Grocery,
                StoreProfile::Butcher,
                StoreProfile::Bakery,
                StoreProfile::Household,
            ],
            requires_capital: false,
        };
    }

    // RetailStore: mid development (default small shop)
    if development_level >= 0.25 {
        return RetailFormatSpec {
            building_type: CommercialBuildingType::RetailStore,
            min_development: 0.25,
            min_year: 0,
            attractiveness: 0.8,
            markup: 1.3,
            upkeep_mult: 1.0,
            capacity_mult: 1.0,
            allowed_profiles: vec![StoreProfile::Grocery, StoreProfile::Clothing],
            requires_capital: false,
        };
    }

    // Marketplace: low development / rural / pre-industrial (fallback)
    RetailFormatSpec {
        building_type: CommercialBuildingType::Marketplace,
        min_development: 0.0,
        min_year: 0,
        attractiveness: 0.5,
        markup: 1.2,
        upkeep_mult: 0.3,
        capacity_mult: 0.5,
        allowed_profiles: vec![StoreProfile::Grocery, StoreProfile::Clothing],
        requires_capital: false,
    }
}
