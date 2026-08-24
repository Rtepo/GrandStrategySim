//! Consumption basket registry for B2C market (Phase 6.5).
//!
//! Provides per-demographic-class consumption baskets organized by need tier
//! (Subsistence, Standard, Luxury), with substitution rules for payment-in-kind.

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Need tier for consumption (evaluated in order: Subsistence → Standard → Luxury)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedTier {
    /// Subsistence needs (food, basic shelter - must be satisfied first)
    Subsistence,
    /// Standard needs (clothing, household goods, variety)
    Standard,
    /// Luxury needs (entertainment, premium goods, status items)
    Luxury,
}

/// Consumption basket for a demographic class
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsumptionBasket {
    /// Per capita, per turn consumption by need tier
    /// Maps NeedTier → (Commodity → units per capita per turn)

    pub tiers: BTreeMap<NeedTier, BTreeMap<Commodity, f64>>,
    
    /// Max fraction of spending power committed to each tier
    /// Maps NeedTier → budget share (0.0-1.0), must sum to 1.0

    pub tier_budget_share: BTreeMap<NeedTier, f64>,
}

/// Substitution rule for payment-in-kind (Phase 6.5)
///
/// Defines how surplus of one commodity can substitute for a deficit of another
/// in the subsistence basket (e.g., extra grain partially covers protein need).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Substitution {
    /// Donor commodity that can substitute for the deficit
    pub donor: Commodity,
    /// Ratio: donor units required per 1 unit of deficit covered
    /// e.g., ratio=2.0 means 2 units of grain cover 1 unit of protein deficit
    pub ratio: f64,
}

/// Configuration for payment-in-kind mechanics (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubsistenceConfig {
    /// Max fraction of a deficit that can be covered by substitution
    /// e.g., 0.5 means at most 50% of protein need can be covered by grain surplus

    pub substitution_cap: f64,
    
    /// VWAP wage offset for FreePeasant/LandlessLaborer (class-dependent)
    /// Serfs receive in-kind INSTEAD of wages (no cash offset)
    /// Aristocracy receives no in-kind at all

    pub vwap_wage_offset: bool,
    
    /// Nutritional penalty for substituted consumption
    /// Reduces quality-of-life when subsistence is met via substitution

    pub nutritional_penalty: f64,
}

impl Default for SubsistenceConfig {
    fn default() -> Self {
        Self {
            substitution_cap: 0.5,
            vwap_wage_offset: true,
            nutritional_penalty: 0.2,
        }
    }
}

/// Returns the static consumption basket registry.
///
/// # Returns
/// * `&'static BTreeMap<String, ConsumptionBasket>` — keyed by class_id
///
/// # Rules
/// * Registry is exhaustive over demographic classes
/// * Units are per capita, per turn
/// * NeedTiers are evaluated in order (Subsistence → Standard → Luxury)
pub fn consumption_registry() -> &'static BTreeMap<String, ConsumptionBasket> {
    static REGISTRY: OnceLock<BTreeMap<String, ConsumptionBasket>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m = BTreeMap::new();

        // Rural classes
        m.insert(
            "Serf".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.15); // kg per turn
                            subsistence.insert(Commodity::Vegetable, 0.10);
                            subsistence.insert(Commodity::Meat, 0.03);
                            subsistence.insert(Commodity::HealthCapacity, 0.02); // Phase 7: Health service need
                            subsistence
                        },
                    );
                    tiers.insert(NeedTier::Standard, BTreeMap::new()); // Minimal standard needs
                    tiers.insert(NeedTier::Luxury, BTreeMap::new()); // No luxury
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 1.0); // 100% to subsistence
                    shares.insert(NeedTier::Standard, 0.0);
                    shares.insert(NeedTier::Luxury, 0.0);
                    shares
                },
            },
        );

        m.insert(
            "FreePeasant".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.18);
                            subsistence.insert(Commodity::Vegetable, 0.12);
                            subsistence.insert(Commodity::Meat, 0.07); // Phase 76: merged Protein (0.08) + Meat (0.02), reduced for density
                            subsistence.insert(Commodity::Fruit, 0.02); // Phase 74: modest fruit consumption
                            subsistence.insert(Commodity::HealthCapacity, 0.03); // Phase 7: Health service need
                            subsistence.insert(Commodity::EducationSlots, 0.01); // Phase 7: Education service need
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.02);
                            standard.insert(Commodity::Furniture, 0.01); // Using Furniture for household goods
                            standard.insert(Commodity::Radio, 0.003); // Phase 20: basic consumer electronics
                            standard
                        },
                    );
                    tiers.insert(NeedTier::Luxury, BTreeMap::new());
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.7);
                    shares.insert(NeedTier::Standard, 0.3);
                    shares.insert(NeedTier::Luxury, 0.0);
                    shares
                },
            },
        );

        m.insert(
            "LandlessLaborer".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.16);
                            subsistence.insert(Commodity::Vegetable, 0.11);
                            subsistence.insert(Commodity::Meat, 0.05); // Phase 76: merged Protein (0.06) + Meat (0.01)
                            subsistence.insert(Commodity::HealthCapacity, 0.025); // Phase 7: Health service need
                            subsistence.insert(Commodity::EducationSlots, 0.008); // Phase 7: Education service need
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.015);
                            standard.insert(Commodity::Furniture, 0.008); // Using Furniture for household goods
                            standard
                        },
                    );
                    tiers.insert(NeedTier::Luxury, BTreeMap::new());
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.75);
                    shares.insert(NeedTier::Standard, 0.25);
                    shares.insert(NeedTier::Luxury, 0.0);
                    shares
                },
            },
        );

        m.insert(
            "Aristocracy".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.25);
                            subsistence.insert(Commodity::Vegetable, 0.20);
                            subsistence.insert(Commodity::Luxury, 0.05);
                            subsistence.insert(Commodity::HealthCapacity, 0.05); // Phase 7: Health service need
                            subsistence.insert(Commodity::EducationSlots, 0.02); // Phase 7: Education service need
                            subsistence.insert(Commodity::Meat, 0.17); // Phase 76: merged Protein (0.15) + Meat (0.08), reduced for density
                            subsistence.insert(Commodity::Fruit, 0.05); // Phase 20: Fruit
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.05);
                            standard.insert(Commodity::Furniture, 0.03); // Using Furniture for household goods
                            standard.insert(Commodity::OfficeMachinery, 0.01); // Using OfficeMachinery for electronics
                            standard.insert(Commodity::Televisions, 0.005); // Phase 20: Televisions
                            standard.insert(Commodity::Agd, 0.008); // Phase 20: Agd
                            standard.insert(Commodity::Fuels, 0.03); // Phase 30: Motor fuel for car owners
                            standard
                        },
                    );
                    tiers.insert(
                        NeedTier::Luxury,
                        {
                            let mut luxury = BTreeMap::new();
                            luxury.insert(Commodity::Luxury, 0.10);
                            luxury.insert(Commodity::LuxuryFurniture, 0.02); // Phase 20: LuxuryFurniture
                            luxury.insert(Commodity::LuxuryClothing, 0.03); // Phase 20: LuxuryClothing
                            luxury.insert(Commodity::Cars, 0.005); // Phase 20: Cars
                            luxury
                        },
                    );
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.3);
                    shares.insert(NeedTier::Standard, 0.4);
                    shares.insert(NeedTier::Luxury, 0.3);
                    shares
                },
            },
        );

        // Urban classes (simplified - would be expanded in full implementation)
        m.insert(
            "Worker".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.20);
                            subsistence.insert(Commodity::Vegetable, 0.15);
                            subsistence.insert(Commodity::HealthCapacity, 0.04); // Phase 7: Health service need
                            subsistence.insert(Commodity::EducationSlots, 0.015); // Phase 7: Education service need
                            subsistence.insert(Commodity::Meat, 0.09); // Phase 76: merged Protein (0.10) + Meat (0.03), reduced for density
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.03);
                            standard.insert(Commodity::Furniture, 0.02); // Using Furniture for household goods
                            standard.insert(Commodity::Radio, 0.005); // Phase 20: Radio
                            standard.insert(Commodity::Televisions, 0.003); // Phase 20: Televisions
                            standard.insert(Commodity::Agd, 0.005); // Phase 20: Agd
                            standard
                        },
                    );
                    tiers.insert(NeedTier::Luxury, BTreeMap::new());
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.6);
                    shares.insert(NeedTier::Standard, 0.4);
                    shares.insert(NeedTier::Luxury, 0.0);
                    shares
                },
            },
        );

        // Phase 20: New urban class baskets
        m.insert(
            "Bourgeoisie".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.22);
                            subsistence.insert(Commodity::Vegetable, 0.18);
                            subsistence.insert(Commodity::HealthCapacity, 0.04);
                            subsistence.insert(Commodity::EducationSlots, 0.02);
                            subsistence.insert(Commodity::Meat, 0.15); // Phase 76: merged Protein (0.12) + Meat (0.06), reduced for density
                            subsistence.insert(Commodity::Fruit, 0.04);
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.04);
                            standard.insert(Commodity::Furniture, 0.03);
                            standard.insert(Commodity::Radio, 0.005);
                            standard.insert(Commodity::Televisions, 0.008);
                            standard.insert(Commodity::Agd, 0.01);
                            standard.insert(Commodity::Cars, 0.003);
                            standard.insert(Commodity::Fuels, 0.02); // Phase 30: Motor fuel for car owners
                            standard
                        },
                    );
                    tiers.insert(
                        NeedTier::Luxury,
                        {
                            let mut luxury = BTreeMap::new();
                            luxury.insert(Commodity::Luxury, 0.05);
                            luxury.insert(Commodity::LuxuryFurniture, 0.01);
                            luxury.insert(Commodity::LuxuryClothing, 0.015);
                            luxury
                        },
                    );
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.4);
                    shares.insert(NeedTier::Standard, 0.4);
                    shares.insert(NeedTier::Luxury, 0.2);
                    shares
                },
            },
        );

        m.insert(
            "PettyBourgeoisie".to_string(),
            ConsumptionBasket {
                tiers: {
                    let mut tiers = BTreeMap::new();
                    tiers.insert(
                        NeedTier::Subsistence,
                        {
                            let mut subsistence = BTreeMap::new();
                            subsistence.insert(Commodity::Cereal, 0.21);
                            subsistence.insert(Commodity::Vegetable, 0.16);
                            subsistence.insert(Commodity::HealthCapacity, 0.04);
                            subsistence.insert(Commodity::EducationSlots, 0.018);
                            subsistence.insert(Commodity::Meat, 0.12); // Phase 76: merged Protein (0.11) + Meat (0.04), reduced for density
                            subsistence.insert(Commodity::Fruit, 0.03);
                            subsistence
                        },
                    );
                    tiers.insert(
                        NeedTier::Standard,
                        {
                            let mut standard = BTreeMap::new();
                            standard.insert(Commodity::Clothing, 0.035);
                            standard.insert(Commodity::Furniture, 0.025);
                            standard.insert(Commodity::Radio, 0.004);
                            standard.insert(Commodity::Televisions, 0.005);
                            standard.insert(Commodity::Agd, 0.008);
                            standard.insert(Commodity::Cars, 0.002);
                            standard.insert(Commodity::Fuels, 0.015); // Phase 30: Motor fuel for car owners
                            standard
                        },
                    );
                    tiers.insert(
                        NeedTier::Luxury,
                        {
                            let mut luxury = BTreeMap::new();
                            luxury.insert(Commodity::Luxury, 0.02);
                            luxury.insert(Commodity::LuxuryClothing, 0.005);
                            luxury
                        },
                    );
                    tiers
                },
                tier_budget_share: {
                    let mut shares = BTreeMap::new();
                    shares.insert(NeedTier::Subsistence, 0.5);
                    shares.insert(NeedTier::Standard, 0.4);
                    shares.insert(NeedTier::Luxury, 0.1);
                    shares
                },
            },
        );

        m
    })
}

/// Returns the static substitution matrix for payment-in-kind.
///
/// # Returns
/// * `&'static BTreeMap<Commodity, Vec<Substitution>>` — deficit commodity → ordered donor candidates
///
/// # Rules
/// * Donors are ordered by preference (first in list is preferred)
/// * Ratio > 1.0 means donor is less efficient than target (e.g., 2 grain for 1 protein)
/// * Used only for Subsistence tier in payment-in-kind
pub fn substitution_matrix() -> &'static BTreeMap<Commodity, Vec<Substitution>> {
    static MATRIX: OnceLock<BTreeMap<Commodity, Vec<Substitution>>> = OnceLock::new();
    MATRIX.get_or_init(|| {
        let mut m = BTreeMap::new();

        // Protein deficit can be covered by Cereal (monoculture peasant diet)
        // Phase 76: Protein merged into Meat — substitution entries removed.

        // Vegetable deficit can be covered by Cereal
        m.insert(
            Commodity::Vegetable,
            vec![Substitution {
                donor: Commodity::Cereal,
                ratio: 1.5,
            }],
        );

        // Cereal deficit has no good substitutes (staple crop)
        m.insert(Commodity::Cereal, vec![]);

        m
    })
}

/// Returns the subsistence configuration for payment-in-kind.
///
/// # Returns
/// * `&'static SubsistenceConfig`
pub fn subsistence_config() -> &'static SubsistenceConfig {
    static CONFIG: OnceLock<SubsistenceConfig> = OnceLock::new();
    CONFIG.get_or_init(SubsistenceConfig::default)
}

/// Phase 74: Price-driven substitution rule for perishable goods.
///
/// When a primary good's price exceeds the affordability threshold (relative
/// to average_wage), a calculated percentage of demand shifts to a cheaper
/// substitute. This is distinct from the payment-in-kind `Substitution` matrix
/// — this matrix is driven by market prices, not by deficit coverage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceSubstitution {
    /// Primary commodity that may be substituted away from
    pub primary: Commodity,
    /// Substitute commodity that absorbs shifted demand
    pub substitute: Commodity,
    /// Nutritional equivalence ratio: substitute units needed per 1 unit of primary
    /// e.g., 0.4 means 2.5 kg cereal ≈ 1 kg meat nutritionally
    pub equivalence_ratio: f64,
    /// Price elasticity coefficient: how aggressively demand shifts when price rises
    /// substitution_fraction = ((price_ratio - 1.0) * coefficient).clamp(0, max_substitution)
    pub elasticity_coefficient: f64,
    /// Maximum fraction of demand that can be substituted (0.0–1.0)
    pub max_substitution: f64,
}

/// Phase 74: Returns the static price-driven substitution matrix.
///
/// # Returns
/// * `&'static BTreeMap<Commodity, Vec<PriceSubstitution>>` — primary commodity → substitute candidates
///
/// # Rules
/// * Substitutes are ordered by preference (first is preferred)
/// * When a primary good's price_ratio > 1.0, demand shifts to substitutes
/// * substitution_fraction = ((price_ratio - 1.0) * coefficient).clamp(0, max_substitution)
/// * Shifted demand is scaled by equivalence_ratio before adding to substitute
pub fn price_substitution_matrix() -> &'static BTreeMap<Commodity, Vec<PriceSubstitution>> {
    static MATRIX: OnceLock<BTreeMap<Commodity, Vec<PriceSubstitution>>> = OnceLock::new();
    MATRIX.get_or_init(|| {
        let mut m = BTreeMap::new();

        // Meat → Cereal / Vegetable (when meat is too expensive)
        m.insert(
            Commodity::Meat,
            vec![
                PriceSubstitution {
                    primary: Commodity::Meat,
                    substitute: Commodity::Cereal,
                    equivalence_ratio: 0.4,  // 2.5 kg cereal ≈ 1 kg meat
                    elasticity_coefficient: 0.8,
                    max_substitution: 0.6,
                },
                PriceSubstitution {
                    primary: Commodity::Meat,
                    substitute: Commodity::Vegetable,
                    equivalence_ratio: 0.33, // 3 kg veg ≈ 1 kg meat
                    elasticity_coefficient: 0.8,
                    max_substitution: 0.6,
                },
            ],
        );

        // Fruit → Cereal / Vegetable
        m.insert(
            Commodity::Fruit,
            vec![
                PriceSubstitution {
                    primary: Commodity::Fruit,
                    substitute: Commodity::Cereal,
                    equivalence_ratio: 0.5,
                    elasticity_coefficient: 0.7,
                    max_substitution: 0.5,
                },
                PriceSubstitution {
                    primary: Commodity::Fruit,
                    substitute: Commodity::Vegetable,
                    equivalence_ratio: 0.6,
                    elasticity_coefficient: 0.7,
                    max_substitution: 0.5,
                },
            ],
        );

        // Phase 76: Protein merged into Meat — price substitution entries removed.

        m
    })
}
