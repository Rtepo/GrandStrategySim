//! Phase 19A: Blueprint specifications — the generative substitutes engine.
//!
//! For each commodity that is blueprint-eligible (fixed assets + quality
//! consumer durables), a `BlueprintSpec` defines the *ideal* material roles and
//! their acceptable substitutes. When a company designs a `ProductBlueprint`,
//! it chooses, per role, either the ideal material or one of the substitutes.
//! Each choice carries a `quality_factor` and a `durability_factor`:
//!
//! * `quality   = base_quality   × Σ(role.share × material_quality_factor)`
//! * `durability = base_durability × Σ(role.share × material_durability_factor)`
//!
//! Substituting a cheaper material (e.g. Iron for Aluminum) lowers both factors
//! but the material itself is cheaper on the B2B market, so the designer trades
//! production cost against quality/durability. **This is the generative axis.**

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single material role in a blueprint's bill of materials.
///
/// # Rules
/// * `ideal` is the premium material for this role (factor 1.0 implied).
/// * Each substitute carries its own `(quality_factor, durability_factor)` —
///   values in `(0.0, 1.0]` mean the substitute is worse than ideal on that
///   axis; values `> 1.0` are allowed (a substitute could exceed the ideal on
///   one axis, e.g. titanium is more durable than aluminum).
/// * `share` is the fraction of the bill this role occupies; shares across all
///   roles of a spec should sum to ~1.0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialRole {
    /// Premium material for this role.
    pub ideal: Commodity,
    /// `(substitute, quality_factor, durability_factor)` tuples.
    pub substitutes: Vec<(Commodity, f64, f64)>,
    /// Fraction of the total bill this role occupies (0.0–1.0).
    pub share: f64,
}

/// The blueprint specification for one output commodity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlueprintSpec {
    /// Output commodity this spec designs (e.g. `IndustrialMachinery`, `Cars`).
    pub commodity: Commodity,
    /// Material roles that compose the bill of materials.
    pub roles: Vec<MaterialRole>,
    /// Base quality before material factors are applied.
    pub base_quality: f64,
    /// Base durability (in turns) before material factors are applied.
    pub base_durability_turns: f64,
}

impl BlueprintSpec {
    /// Compute the `(quality, durability)` for a chosen bill of materials.
    ///
    /// # Arguments
    /// * `choices` — one `MaterialChoice` per role, in the same order as `roles`.
    ///
    /// # Rules
    /// * Ideal material ⇒ factors (1.0, 1.0).
    /// * A substitute ⇒ its declared `(quality_factor, durability_factor)`.
    /// * Result is clamped to non-negative.
    pub fn compute_stats(&self, choices: &[MaterialChoice]) -> (f64, f64) {
        let mut q_acc = 0.0;
        let mut d_acc = 0.0;
        for (i, role) in self.roles.iter().enumerate() {
            let (qf, df) = match choices.get(i) {
                Some(MaterialChoice::Ideal) => (1.0, 1.0),
                Some(MaterialChoice::Substitute(idx)) => {
                    role.substitutes
                        .get(*idx)
                        .map(|(_, q, d)| (*q, *d))
                        .unwrap_or((1.0, 1.0))
                }
                None => (1.0, 1.0),
            };
            q_acc += role.share * qf;
            d_acc += role.share * df;
        }
        let quality = (self.base_quality * q_acc).max(0.0);
        let durability = (self.base_durability_turns * d_acc).max(0.0);
        (quality, durability)
    }

    /// Materialize the actual input bill of materials for a set of choices.
    ///
    /// Returns a `HashMap<Commodity, share>` where the share is the role's
    /// `share` (the per-1k-worker quantity is scaled by the production method).
    pub fn bill_of_materials(&self, choices: &[MaterialChoice]) -> HashMap<Commodity, f64> {
        let mut bom = HashMap::new();
        for (i, role) in self.roles.iter().enumerate() {
            let material = match choices.get(i) {
                Some(MaterialChoice::Substitute(idx)) => {
                    role.substitutes.get(*idx).map(|(c, _, _)| *c).unwrap_or(role.ideal)
                }
                _ => role.ideal,
            };
            *bom.entry(material).or_insert(0.0) += role.share;
        }
        bom
    }
}

/// A per-role material choice made during blueprint design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialChoice {
    /// Use the role's ideal material.
    Ideal,
    /// Use the substitute at the given index in `MaterialRole.substitutes`.
    Substitute(usize),
}

/// Build the canonical `BlueprintSpec` registry for all blueprint-eligible
/// commodities.
///
/// # Rules
/// * Covers fixed-asset commodities and quality consumer durables.
/// * Each spec's role shares sum to ~1.0.
/// * Substitutes are strictly worse on at least one axis (cheaper material).
pub fn blueprint_specs() -> HashMap<Commodity, BlueprintSpec> {
    let mut m = HashMap::new();
    m.insert(Commodity::IndustrialMachinery, industrial_machinery_spec());
    m.insert(Commodity::ConstructionMachinery, construction_machinery_spec());
    m.insert(Commodity::AgriculturalMachinery, agricultural_machinery_spec());
    m.insert(Commodity::OfficeMachinery, office_machinery_spec());
    m.insert(Commodity::Trucks, trucks_spec());
    m.insert(Commodity::Cars, cars_spec());
    m.insert(Commodity::Agd, agd_spec());
    m.insert(Commodity::Televisions, televisions_spec());
    m.insert(Commodity::Radio, radio_spec());
    m.insert(Commodity::Furniture, furniture_spec());
    m.insert(Commodity::LuxuryFurniture, luxury_furniture_spec());
    m.insert(Commodity::Clothing, clothing_spec());
    m.insert(Commodity::LuxuryClothing, luxury_clothing_spec());
    m
}

// ── Spec definitions ──────────────────────────────────────────────────────
// Ideal material = premium; substitutes trade quality/durability for cost.

fn industrial_machinery_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::IndustrialMachinery,
        base_quality: 1.2,
        base_durability_turns: 240.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.6,
            },
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![
                    (Commodity::MechanicalComponents, 0.8, 0.9),
                    (Commodity::Semiconductors, 1.15, 0.85),  // Phase 20: better quality, worse durability
                ],
                share: 0.3,
            },
            MaterialRole {
                ideal: Commodity::Aluminum,
                substitutes: vec![(Commodity::Iron, 0.6, 0.5)],
                share: 0.1,
            },
        ],
    }
}

fn construction_machinery_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::ConstructionMachinery,
        base_quality: 1.1,
        base_durability_turns: 300.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.7,
            },
            MaterialRole {
                ideal: Commodity::MechanicalComponents,
                substitutes: vec![(Commodity::Iron, 0.6, 0.7)],
                share: 0.3,
            },
        ],
    }
}

fn agricultural_machinery_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::AgriculturalMachinery,
        base_quality: 1.1,
        base_durability_turns: 200.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.6,
            },
            MaterialRole {
                ideal: Commodity::MechanicalComponents,
                substitutes: vec![(Commodity::Iron, 0.6, 0.7)],
                share: 0.4,
            },
        ],
    }
}

fn office_machinery_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::OfficeMachinery,
        base_quality: 1.0,
        base_durability_turns: 180.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.7, 0.9)],
                share: 0.5,
            },
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.3,
            },
            MaterialRole {
                ideal: Commodity::Chemicals,
                substitutes: vec![(Commodity::Iron, 0.5, 0.5)],
                share: 0.2,
            },
        ],
    }
}

fn trucks_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Trucks,
        base_quality: 1.1,
        base_durability_turns: 180.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.5,
            },
            MaterialRole {
                ideal: Commodity::MechanicalComponents,
                substitutes: vec![(Commodity::Iron, 0.6, 0.7)],
                share: 0.25,
            },
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)],
                share: 0.15,
            },
            MaterialRole {
                ideal: Commodity::Batteries,
                substitutes: vec![(Commodity::Fuels, 0.5, 0.7)],
                share: 0.1,  // Phase 20: Batteries
            },
        ],
    }
}

fn cars_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Cars,
        base_quality: 1.15,
        base_durability_turns: 150.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.3,
            },
            MaterialRole {
                ideal: Commodity::Aluminum,
                substitutes: vec![(Commodity::Iron, 0.6, 0.5)],
                share: 0.15,
            },
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)],
                share: 0.15,
            },
            MaterialRole {
                ideal: Commodity::Plastics,
                substitutes: vec![(Commodity::Steel, 0.7, 0.8)],
                share: 0.2,  // Phase 20: Plastics
            },
            MaterialRole {
                ideal: Commodity::Batteries,
                substitutes: vec![(Commodity::Fuels, 0.5, 0.7)],
                share: 0.2,  // Phase 20: Batteries (EV variant)
            },
        ],
    }
}

fn agd_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Agd,
        base_quality: 1.1,
        base_durability_turns: 120.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.3,
            },
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)],
                share: 0.35,
            },
            MaterialRole {
                ideal: Commodity::Plastics,
                substitutes: vec![(Commodity::Steel, 0.6, 0.7)],
                share: 0.2,  // Phase 20: Plastics
            },
            MaterialRole {
                ideal: Commodity::Chemicals,
                substitutes: vec![(Commodity::Iron, 0.5, 0.5)],
                share: 0.15,
            },
        ],
    }
}

fn televisions_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Televisions,
        base_quality: 1.2,
        base_durability_turns: 100.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.6, 0.8)],
                share: 0.4,
            },
            MaterialRole {
                ideal: Commodity::Semiconductors,
                substitutes: vec![(Commodity::ElectronicComponents, 0.7, 0.8)],
                share: 0.2,  // Phase 20: Semiconductors
            },
            MaterialRole {
                ideal: Commodity::Plastics,
                substitutes: vec![(Commodity::Steel, 0.5, 0.7)],
                share: 0.15,  // Phase 20: Plastics
            },
            MaterialRole {
                ideal: Commodity::Glass,
                substitutes: vec![(Commodity::Chemicals, 0.7, 0.6)],
                share: 0.15,
            },
            MaterialRole {
                ideal: Commodity::Chemicals,
                substitutes: vec![(Commodity::Steel, 0.6, 0.7)],
                share: 0.1,
            },
        ],
    }
}

fn radio_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Radio,
        base_quality: 1.1,
        base_durability_turns: 110.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::ElectronicComponents,
                substitutes: vec![(Commodity::MechanicalComponents, 0.6, 0.8)],
                share: 0.7,
            },
            MaterialRole {
                ideal: Commodity::Chemicals,
                substitutes: vec![(Commodity::Steel, 0.6, 0.7)],
                share: 0.3,
            },
        ],
    }
}

fn furniture_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Furniture,
        base_quality: 1.0,
        base_durability_turns: 200.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Planks,
                substitutes: vec![(Commodity::Steel, 0.6, 1.1)],
                share: 0.7,
            },
            MaterialRole {
                ideal: Commodity::Steel,
                substitutes: vec![(Commodity::Iron, 0.7, 0.6)],
                share: 0.3,
            },
        ],
    }
}

fn luxury_furniture_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::LuxuryFurniture,
        base_quality: 1.6,
        base_durability_turns: 300.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Planks,
                substitutes: vec![(Commodity::Steel, 0.5, 1.1)],
                share: 0.6,
            },
            MaterialRole {
                ideal: Commodity::Luxury,
                substitutes: vec![(Commodity::Planks, 0.6, 0.7)],
                share: 0.3,
            },
            MaterialRole {
                ideal: Commodity::Gold,
                substitutes: vec![(Commodity::Steel, 0.4, 0.8)],
                share: 0.1,
            },
        ],
    }
}

fn clothing_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::Clothing,
        base_quality: 1.0,
        base_durability_turns: 90.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Fibers,
                substitutes: vec![(Commodity::IndustrialFiber, 0.7, 0.8)],
                share: 0.8,
            },
            MaterialRole {
                ideal: Commodity::Chemicals,
                substitutes: vec![(Commodity::Iron, 0.4, 0.6)],
                share: 0.2,
            },
        ],
    }
}

fn luxury_clothing_spec() -> BlueprintSpec {
    BlueprintSpec {
        commodity: Commodity::LuxuryClothing,
        base_quality: 1.7,
        base_durability_turns: 150.0,
        roles: vec![
            MaterialRole {
                ideal: Commodity::Luxury,
                substitutes: vec![(Commodity::Fibers, 0.6, 0.7)],
                share: 0.7,
            },
            MaterialRole {
                ideal: Commodity::Fibers,
                substitutes: vec![(Commodity::IndustrialFiber, 0.6, 0.8)],
                share: 0.3,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ideal_choice_yields_base_stats() {
        let spec = industrial_machinery_spec();
        let choices = vec![MaterialChoice::Ideal; spec.roles.len()];
        let (q, d) = spec.compute_stats(&choices);
        // Σ share = 1.0, all factors 1.0 → quality == base_quality.
        assert!((q - spec.base_quality).abs() < 1e-9);
        assert!((d - spec.base_durability_turns).abs() < 1e-9);
    }

    #[test]
    fn iron_substitute_lowers_quality_and_durability() {
        let spec = industrial_machinery_spec();
        // Substitute Iron for Steel in role 0 (share 0.6, factors 0.7/0.6).
        let choices = vec![
            MaterialChoice::Substitute(0),
            MaterialChoice::Ideal,
            MaterialChoice::Ideal,
        ];
        let (q, d) = spec.compute_stats(&choices);
        let (q_ideal, d_ideal) = spec.compute_stats(&[MaterialChoice::Ideal; 3]);
        assert!(q < q_ideal, "quality must drop with Iron substitute");
        assert!(d < d_ideal, "durability must drop with Iron substitute");
    }

    #[test]
    fn bill_of_materials_picks_substitute() {
        let spec = cars_spec();
        let choices = vec![
            MaterialChoice::Ideal,
            MaterialChoice::Substitute(0), // Aluminum→Iron
            MaterialChoice::Ideal,
            MaterialChoice::Ideal,
        ];
        let bom = spec.bill_of_materials(&choices);
        // Iron must appear (the substitute for Aluminum).
        assert!(bom.contains_key(&Commodity::Iron));
        // Aluminum must NOT appear (it was substituted out).
        assert!(!bom.contains_key(&Commodity::Aluminum));
    }

    #[test]
    fn registry_covers_all_eligible_commodities() {
        let specs = blueprint_specs();
        assert!(specs.contains_key(&Commodity::IndustrialMachinery));
        assert!(specs.contains_key(&Commodity::Cars));
        assert!(specs.contains_key(&Commodity::LuxuryClothing));
        assert!(specs.len() >= 13);
    }
}
