//! Military units and unit types with demographic tracking

use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;

type HashMap<K, V> = FxHashMap<K, V>;

use crate::registries::enums::Commodity;
use crate::society::geography::RuralClass;
use crate::military::config::MilitaryCombatConfig;

/// Combat statistics for a military unit
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct UnitStats {
    /// Attack power
    pub attack: f64,

    /// Defense power
    pub defense: f64,

    /// Organization (morale/cohesion)
    pub organization: f64,

    /// Supply level (logistics)
    pub supply: f64,

    /// Maneuver capability
    pub maneuver: f64,

    /// Health/Hit points
    pub health: f64,
}

/// Type of military unit
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UnitType {
    /// Standard infantry
    Infantry,
    /// Armored tank units
    Tanks,
    /// Artillery
    Artillery,
    /// Air force
    AirForce,
    /// Naval vessels
    Naval,
    /// Peasant militia (zero upkeep, causes local devastation)
    PeasantBattalion,
}

impl UnitType {
    /// Get base stats for this unit type
    /// 
    /// # Returns
    /// Default combat statistics
    pub fn base_stats(&self) -> UnitStats {
        match self {
            UnitType::Infantry => UnitStats {
                attack: 10.0,
                defense: 8.0,
                organization: 80.0,
                supply: 100.0,
                maneuver: 5.0,
                health: 100.0,
            },
            UnitType::Tanks => UnitStats {
                attack: 25.0,
                defense: 20.0,
                organization: 70.0,
                supply: 80.0,
                maneuver: 8.0,
                health: 150.0,
            },
            UnitType::Artillery => UnitStats {
                attack: 30.0,
                defense: 5.0,
                organization: 60.0,
                supply: 60.0,
                maneuver: 2.0,
                health: 80.0,
            },
            UnitType::AirForce => UnitStats {
                attack: 40.0,
                defense: 15.0,
                organization: 90.0,
                supply: 50.0,
                maneuver: 20.0,
                health: 100.0,
            },
            UnitType::Naval => UnitStats {
                attack: 35.0,
                defense: 25.0,
                organization: 85.0,
                supply: 70.0,
                maneuver: 10.0,
                health: 200.0,
            },
            UnitType::PeasantBattalion => UnitStats {
                attack: 3.0,
                defense: 2.0,
                organization: 40.0,
                supply: 50.0,
                maneuver: 4.0,
                health: 60.0,
            },
        }
    }
    
    /// Get commodity upkeep requirements
    /// 
    /// # Returns
    /// Map of commodity to consumption rate per turn
    pub fn commodity_upkeep(&self) -> HashMap<Commodity, f64> {
        match self {
            UnitType::Infantry => {
                let mut upkeep = HashMap::default();
                upkeep.insert(Commodity::Ammunition, 5.0);
                upkeep.insert(Commodity::Fuels, 1.0);
                upkeep
            }
            UnitType::Tanks => {
                let mut upkeep = HashMap::default();
                upkeep.insert(Commodity::Ammunition, 15.0);
                upkeep.insert(Commodity::Fuels, 20.0);
                upkeep.insert(Commodity::Steel, 5.0);
                upkeep
            }
            UnitType::Artillery => {
                let mut upkeep = HashMap::default();
                upkeep.insert(Commodity::Ammunition, 25.0);
                upkeep.insert(Commodity::Fuels, 2.0);
                upkeep
            }
            UnitType::AirForce => {
                let mut upkeep = HashMap::default();
                upkeep.insert(Commodity::Ammunition, 10.0);
                upkeep.insert(Commodity::Fuels, 50.0);
                upkeep.insert(Commodity::ElectronicComponents, 5.0);
                upkeep
            }
            UnitType::Naval => {
                let mut upkeep = HashMap::default();
                upkeep.insert(Commodity::Ammunition, 20.0);
                upkeep.insert(Commodity::Fuels, 40.0);
                upkeep.insert(Commodity::Steel, 10.0);
                upkeep
            }
            UnitType::PeasantBattalion => {
                // CRITICAL: Peasant battalions have ZERO commodity upkeep
                HashMap::default()
            }
        }
    }
    
    /// Get wage cost per turn
    ///
    /// # Returns
    /// Cash cost for soldier wages per turn
    pub fn wage_cost(&self) -> f64 {
        match self {
            UnitType::Infantry => 10.0,
            UnitType::Tanks => 25.0,
            UnitType::Artillery => 20.0,
            UnitType::AirForce => 50.0,
            UnitType::Naval => 40.0,
            UnitType::PeasantBattalion => 0.0, // CRITICAL: Peasant battalions have ZERO wage cost
        }
    }

    /// Phase 45: Returns the Table of Equipment (ToE) for this unit type.
    ///
    /// Defines the capital equipment a full-strength unit requires.
    /// Quantities are per 1000 soldiers (scaled by manpower at spawn).
    ///
    /// Era gating: `year` determines which equipment is available.
    ///   year < 1880: Infantry gets Rifles + Clothing only.
    ///   year >= 1880: Infantry adds TowedArtillery.
    ///   year >= 1916: Tanks gets LightTanks.
    ///   year >= 1935: Tanks adds MediumTanks; Naval adds Submarines.
    ///   year >= 1940: AirForce gets Fighters + Bombers.
    ///   year >= 1942: Tanks adds HeavyTanks.
    ///   year >= 1960: AirForce adds Helicopters.
    pub fn table_of_equipment(&self, year: u32) -> Vec<EquipmentReserve> {
        match self {
            UnitType::Infantry => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Rifles,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.01,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 5000.0,
                        current_quantity: 5000.0,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1880 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::TowedArtillery,
                        toe_quantity: 20.0,
                        current_quantity: 20.0,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    });
                }
                if year >= 1965 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::SupportEquipment,
                        toe_quantity: 50.0,
                        current_quantity: 50.0,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    });
                }
                toe
            }
            UnitType::Artillery => {
                vec![
                    EquipmentReserve {
                        commodity: Commodity::TowedArtillery,
                        toe_quantity: 100.0,
                        current_quantity: 100.0,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 20000.0,
                        current_quantity: 20000.0,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                ]
            }
            UnitType::Tanks => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 15000.0,
                        current_quantity: 15000.0,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1916 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::LightTanks,
                        toe_quantity: 50.0,
                        current_quantity: 50.0,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                if year >= 1935 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::MediumTanks,
                        toe_quantity: 30.0,
                        current_quantity: 30.0,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                if year >= 1942 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::HeavyTanks,
                        toe_quantity: 15.0,
                        current_quantity: 15.0,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                toe
            }
            UnitType::AirForce => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 10000.0,
                        current_quantity: 10000.0,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1940 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Fighters,
                        toe_quantity: 20.0,
                        current_quantity: 20.0,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Bombers,
                        toe_quantity: 10.0,
                        current_quantity: 10.0,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                }
                if year >= 1960 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Helicopters,
                        toe_quantity: 15.0,
                        current_quantity: 15.0,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                }
                toe
            }
            UnitType::Naval => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0,
                        current_quantity: 1000.0,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 20000.0,
                        current_quantity: 20000.0,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1935 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Submarines,
                        toe_quantity: 5.0,
                        current_quantity: 5.0,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    });
                }
                toe
            }
            UnitType::PeasantBattalion => {
                Vec::new()
            }
        }
    }
}

/// Phase 45: A single equipment reserve entry for a military unit.
///
/// Represents the unit's installed capital equipment (rifles, uniforms,
/// artillery pieces, vessels). Equipment degrades over time and must be
/// replaced via B2B procurement.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EquipmentReserve {
    /// The commodity type of this equipment.
    pub commodity: Commodity,
    /// Target quantity per the unit's Table of Equipment (ToE).
    pub toe_quantity: f64,
    /// Currently installed quantity (may be < toe_quantity due to losses/wear).
    pub current_quantity: f64,
    /// Average condition in [0.0, 1.0]. Degrades by `depreciation_rate` per turn.
    pub condition: f64,
    /// Per-turn depreciation rate (fraction of condition lost each turn).
    pub depreciation_rate: f64,
}

impl Default for EquipmentReserve {
    fn default() -> Self {
        EquipmentReserve {
            commodity: Commodity::Food, // Safe default commodity
            toe_quantity: 0.0,
            current_quantity: 0.0,
            condition: 1.0,
            depreciation_rate: 0.0,
        }
    }
}

impl EquipmentReserve {
    /// plus quantity needed to restore condition to 1.0.
    ///
    /// replacement_demand = (toe_quantity - current_quantity) + current_quantity * (1.0 - condition)
    pub fn replacement_demand(&self) -> f64 {
        let quantity_deficit = (self.toe_quantity - self.current_quantity).max(0.0);
        let condition_deficit = self.current_quantity * (1.0 - self.condition);
        quantity_deficit + condition_deficit
    }

    /// Degrade condition by one turn.
    pub fn degrade(&mut self) {
        if self.depreciation_rate <= 0.0 {
            return;
        }
        self.condition = (self.condition - self.depreciation_rate).max(0.0);
        // If condition reaches 0, the equipment is scrapped (quantity drops).
        if self.condition <= 0.0 {
            self.current_quantity = 0.0;
        }
    }

    /// Install new equipment (from B2B deliveries). Restores quantity and condition.
    pub fn install(&mut self, quantity: f64) {
        if quantity <= 0.0 {
            return;
        }
        let old_total = self.current_quantity;
        self.current_quantity += quantity;
        // New equipment arrives at condition 1.0; blend with existing.
        let total = self.current_quantity;
        if total > 0.0 {
            self.condition = (self.condition * old_total + 1.0 * quantity) / total;
        }
    }
}

/// A military unit with demographic tracking
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MilitaryUnit {
    /// Unique unit ID
    pub id: String,

    /// Unit type
    pub unit_type: UnitType,

    /// Current combat statistics
    pub stats: UnitStats,

    /// Current manpower count
    pub manpower: i64,

    /// CRITICAL: Demographic origin of manpower (for casualty routing)
    pub manpower_origin: HashMap<RuralClass, i64>,

    /// Home region
    pub home_region: String,

    /// Current location
    pub location: String,

    /// Experience level (0-100)
    pub experience: f64,

    /// Equipment quality (0-100)
    pub equipment_quality: f64,

    /// Field supply carried by this unit (refilled from country depot each turn).
    /// Key = Commodity, Value = quantity on hand.
    pub stockpile: HashMap<Commodity, f64>,

    /// Phase 45: Table of Equipment (ToE) — the unit's installed capital equipment.
    /// Each entry tracks target quantity, current quantity, and condition.
    /// B2B procurement orders are generated to fill replacement_demand().
    pub equipment_reserves: Vec<EquipmentReserve>,
}

impl MilitaryUnit {
    /// Create a new military unit
    /// 
    /// # Arguments
    /// * `id` - Unique unit identifier
    /// * `unit_type` - Type of unit
    /// * `manpower` - Initial manpower count
    /// * `manpower_origin` - Demographic breakdown of manpower
    /// * `home_region` - Home region
    /// 
    /// # Returns
    /// New MilitaryUnit instance
    pub fn new(
        id: String,
        unit_type: UnitType,
        manpower: i64,
        manpower_origin: HashMap<RuralClass, i64>,
        home_region: String,
    ) -> Self {
        MilitaryUnit {
            id,
            unit_type,
            stats: unit_type.base_stats(),
            manpower,
            manpower_origin,
            home_region: home_region.clone(),
            location: home_region,
            experience: 0.0,
            equipment_quality: 50.0,
            stockpile: HashMap::default(),
            equipment_reserves: Vec::new(),
        }
    }
    
    /// Calculate total commodity upkeep cost
    /// 
    /// # Returns
    /// Map of commodity to total consumption (manpower * per-soldier rate)
    pub fn calculate_commodity_upkeep(&self) -> HashMap<Commodity, f64> {
        let base_upkeep = self.unit_type.commodity_upkeep();
        let multiplier = self.manpower as f64 / 1000.0; // Per 1000 soldiers
        
        base_upkeep.into_iter()
            .map(|(commodity, rate)| (commodity, rate * multiplier))
            .collect()
    }
    
    /// Calculate total wage cost
    /// 
    /// # Returns
    /// Total cash cost for wages (manpower * per-soldier rate)
    pub fn calculate_wage_cost(&self) -> f64 {
        let base_cost = self.unit_type.wage_cost();
        let multiplier = self.manpower as f64 / 1000.0; // Per 1000 soldiers
        base_cost * multiplier
    }
    
    /// Apply casualties and return demographic breakdown
    /// 
    /// # Arguments
    /// * `casualties` - Total number of casualties
    /// 
    /// # Returns
    /// HashMap of RuralClass to casualties deducted from that class
    pub fn apply_casualties(&mut self, casualties: i64) -> HashMap<RuralClass, i64> {
        if casualties <= 0 || self.manpower <= 0 {
            return HashMap::default();
        }
        
        let actual_casualties = casualties.min(self.manpower);
        self.manpower -= actual_casualties;
        
        // Deduct casualties proportionally from demographic origin
        let mut demographic_casualties = HashMap::default();
        let total_origin: i64 = self.manpower_origin.values().sum();
        
        if total_origin > 0 {
            for (rural_class, &origin_count) in &self.manpower_origin {
                let proportion = origin_count as f64 / total_origin as f64;
                let class_casualties = (actual_casualties as f64 * proportion) as i64;
                demographic_casualties.insert(*rural_class, class_casualties);
            }
        }
        
        // Update manpower_origin to reflect remaining
        for (rural_class, class_casualties) in &demographic_casualties {
            if let Some(origin_count) = self.manpower_origin.get_mut(rural_class) {
                *origin_count = (*origin_count - class_casualties).max(0);
            }
        }
        
        demographic_casualties
    }
    
    /// Check if unit is peasant battalion
    /// 
    /// # Returns
    /// True if this is a peasant battalion
    pub fn is_peasant_battalion(&self) -> bool {
        matches!(self.unit_type, UnitType::PeasantBattalion)
    }

    /// Refill field supply from country depot, up to unit's capacity
    /// (supply_capacity_turns * per-turn upkeep rate).
    ///
    /// # Arguments
    /// * `depot` - Mutable reference to country military stockpile
    /// * `config` - Military combat config for capacity calculation
    ///
    /// # Returns
    /// HashMap of commodity to actual quantities drawn from depot
    pub fn resupply(
        &mut self,
        depot: &mut HashMap<Commodity, f64>,
        config: &MilitaryCombatConfig,
    ) -> HashMap<Commodity, f64> {
        let per_turn_upkeep = self.calculate_commodity_upkeep();
        let mut drawn = HashMap::default();

        for (commodity, per_turn_rate) in &per_turn_upkeep {
            let capacity = per_turn_rate * config.unit_supply_capacity_turns;
            let current = self.stockpile.get(commodity).copied().unwrap_or(0.0);
            let needed = (capacity - current).max(0.0);
            let available = depot.get(commodity).copied().unwrap_or(0.0);
            let actual_drawn = needed.min(available);

            if actual_drawn > 0.0 {
                *depot.get_mut(commodity).unwrap() -= actual_drawn;
                *self.stockpile.entry(*commodity).or_insert(0.0) += actual_drawn;
                drawn.insert(*commodity, actual_drawn);
            }
        }

        drawn
    }

    /// Burn commodities during combat.
    ///
    /// # Arguments
    /// * `ammo_required` - Ammunition needed for this battle
    /// * `fuel_required` - Fuel needed for this battle
    ///
    /// # Returns
    /// (actual_ammo_burned, actual_fuel_burned)
    pub fn burn_combat_supplies(
        &mut self,
        ammo_required: f64,
        fuel_required: f64,
    ) -> (f64, f64) {
        let ammo_on_hand = self.stockpile.get(&Commodity::Ammunition).copied().unwrap_or(0.0);
        let fuel_on_hand = self.stockpile.get(&Commodity::Fuels).copied().unwrap_or(0.0);

        let ammo_burned = ammo_required.min(ammo_on_hand);
        let fuel_burned = fuel_required.min(fuel_on_hand);

        if ammo_burned > 0.0 {
            *self.stockpile.get_mut(&Commodity::Ammunition).unwrap() -= ammo_burned;
        }
        if fuel_burned > 0.0 {
            *self.stockpile.get_mut(&Commodity::Fuels).unwrap() -= fuel_burned;
        }

        (ammo_burned, fuel_burned)
    }

    /// Calculate effective combat power with supply and organization factors.
    ///
    /// # Arguments
    /// * `config` - Military combat config
    /// * `is_defender` - True if this unit is defending
    /// * `terrain` - Terrain type string ("mountain", "forest", "plains", or other)
    ///
    /// # Returns
    /// Effective combat power value
    pub fn combat_power(
        &self,
        config: &MilitaryCombatConfig,
        is_defender: bool,
        terrain: &str,
    ) -> f64 {
        let base = if is_defender { self.stats.defense } else { self.stats.attack };
        let org_factor = self.stats.organization / 100.0;

        let supply_factor = if self.stats.supply >= config.supply_full_threshold {
            1.0
        } else if self.stats.supply <= 0.0 {
            config.supply_zero_penalty
        } else {
            config.supply_zero_penalty
                + (1.0 - config.supply_zero_penalty)
                * (self.stats.supply / config.supply_full_threshold)
        };

        let terrain_factor = if is_defender {
            match terrain {
                "mountain" => config.terrain_mountain_defense_bonus,
                "forest" => config.terrain_forest_defense_bonus,
                _ => config.terrain_plains_defense_bonus,
            }
        } else {
            1.0
        };

        let manpower_ratio = self.manpower as f64 / 1000.0;

        base * org_factor * supply_factor * terrain_factor * manpower_ratio
    }

    /// Disband unit: return ALL surviving manpower to demographics.
    ///
    /// # Returns
    /// HashMap of RuralClass to survivor count (to be re-added to population)
    pub fn disband(&mut self) -> HashMap<RuralClass, i64> {
        let mut survivors = HashMap::default();
        let total_origin: i64 = self.manpower_origin.values().sum();

        if total_origin > 0 && self.manpower > 0 {
            for (rural_class, &origin_count) in &self.manpower_origin {
                let proportion = origin_count as f64 / total_origin as f64;
                let class_survivors = (self.manpower as f64 * proportion) as i64;
                survivors.insert(*rural_class, class_survivors);
            }
        }

        self.manpower = 0;
        survivors
    }
}

/// Peasant battalion - special unit type with zero upkeep
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PeasantBattalion {
    /// Base military unit
    #[serde(flatten)]
    pub unit: MilitaryUnit,

    /// Foraging intensity (how much local devastation caused)
    pub foraging_intensity: f64,
}

impl PeasantBattalion {
    /// Create a new peasant battalion
    /// 
    /// # Arguments
    /// * `id` - Unique unit identifier
    /// * `manpower` - Initial manpower count
    /// * `manpower_origin` - Demographic breakdown (should be mostly Serfs/Peasants)
    /// * `home_region` - Home region
    /// * `foraging_intensity` - Foraging intensity (0-1)
    /// 
    /// # Returns
    /// New PeasantBattalion instance
    pub fn new(
        id: String,
        manpower: i64,
        manpower_origin: HashMap<RuralClass, i64>,
        home_region: String,
        foraging_intensity: f64,
    ) -> Self {
        let unit = MilitaryUnit::new(
            id,
            UnitType::PeasantBattalion,
            manpower,
            manpower_origin,
            home_region,
        );
        
        PeasantBattalion {
            unit,
            foraging_intensity: foraging_intensity.clamp(0.0, 1.0),
        }
    }
    
    /// Calculate local economic damage from foraging
    /// 
    /// # Returns
    /// Economic damage multiplier (0-1, higher = more damage)
    pub fn calculate_economic_damage(&self) -> f64 {
        self.foraging_intensity * 0.3 // Up to 30% GDP damage in region
    }
    
    /// Get the underlying military unit
    /// 
    /// # Returns
    /// Reference to the base MilitaryUnit
    pub fn as_military_unit(&self) -> &MilitaryUnit {
        &self.unit
    }
    
    /// Get mutable reference to the underlying military unit
    /// 
    /// # Returns
    /// Mutable reference to the base MilitaryUnit
    pub fn as_military_unit_mut(&mut self) -> &mut MilitaryUnit {
        &mut self.unit
    }
}
