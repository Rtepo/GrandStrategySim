//! Military units and combat system

pub mod combat;
pub mod config;
pub mod fronts;
pub mod turn;
pub mod units;
pub mod upkeep;
pub mod fleet;

pub use combat::{resolve_battle, process_wounded, process_dead, process_deserters};
pub use config::MilitaryCombatConfig;
pub use fronts::{Front, RegionControl, Battle, BattleResult, Casualties};
pub use turn::process_military_turn;
pub use units::{MilitaryUnit, UnitType, UnitStats, PeasantBattalion, EquipmentReserve};
pub use upkeep::{process_military_upkeep, add_military_demand_to_market, add_fleet_demand_to_market, submit_defense_b2b_orders, deliver_military_supplies, degrade_military_equipment, deliver_military_supplies_and_equipment};
pub use fleet::{Fleet, Ship, FleetMission, apply_maritime_capacity_constraint, create_fleet, process_fleet_upkeep};
pub use crate::infrastructure::maritime::ShipType;
