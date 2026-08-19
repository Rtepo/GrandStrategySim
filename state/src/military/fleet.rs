#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::infrastructure::maritime::ShipType;

/// Fleet mission type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetMission {
    /// Idle at port
    Idle,
    /// Trade route mission
    TradeRoute,
    /// Fishing operation
    Fishing,
    /// Naval patrol
    Patrol,
    /// Naval combat
    Combat,
}

/// Individual ship in a fleet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ship {
    /// Unique ship ID
    #[serde(rename = "id_statku")]
    pub id: String,
    /// Type of ship
    #[serde(rename = "typ_statku")]
    pub ship_type: ShipType,
    /// Cargo capacity in tons
    #[serde(rename = "pojemność_ładunku")]
    pub cargo_capacity: f64,
    /// Current cargo load in tons
    #[serde(rename = "aktualny_ładunek")]
    pub current_cargo: f64,
    /// Fuel capacity
    #[serde(rename = "pojemność_paliwa")]
    pub fuel_capacity: f64,
    /// Current fuel
    #[serde(rename = "aktualne_paliwo")]
    pub current_fuel: f64,
    /// Condition 0-1 (affects performance)
    #[serde(rename = "stan")]
    pub condition: f64,
    /// Crew required
    #[serde(rename = "załoga_wymagana")]
    pub crew_required: u32,
    /// Current crew
    #[serde(rename = "aktualna_załoga")]
    pub current_crew: u32,
    /// Maintenance cost per turn
    #[serde(rename = "koszt_utrzymania")]
    pub maintenance_cost: f64,
}

impl Ship {
    /// Create a new ship of the specified type.
    ///
    /// # Arguments
    /// * ship_type - Type of ship to create
    ///
    /// # Returns
    /// New Ship instance
    pub fn new(ship_type: ShipType) -> Self {
        static SHIP_COUNTER: AtomicU64 = AtomicU64::new(1);
        let unique_id: u64 = SHIP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let (cargo_capacity, fuel_capacity, crew_required, maintenance_cost) = match ship_type {
            ShipType::CargoVessel => (10_000.0, 50_000.0, 30, 5_000.0),
            ShipType::PassengerLiner => (2_000.0, 40_000.0, 100, 15_000.0),
            ShipType::FishingBoat => (500.0, 5_000.0, 5, 1_000.0),
            ShipType::NavalVessel => (1_000.0, 100_000.0, 200, 50_000.0),
        };

        Ship {
            id: format!("Ship-{}-{}", unique_id, ship_type as u8),
            ship_type,
            cargo_capacity,
            current_cargo: 0.0,
            fuel_capacity,
            current_fuel: fuel_capacity,
            condition: 1.0,
            crew_required,
            current_crew: crew_required,
            maintenance_cost,
        }
    }

    /// Load cargo onto the ship.
    ///
    /// # Arguments
    /// * amount - Amount of cargo to load
    ///
    /// # Returns
    /// * true if cargo loaded successfully
    /// * false if insufficient capacity
    pub fn load_cargo(&mut self, amount: f64) -> bool {
        if self.current_cargo + amount <= self.cargo_capacity {
            self.current_cargo += amount;
            true
        } else {
            false
        }
    }

    /// Unload cargo from the ship.
    ///
    /// # Arguments
    /// * amount - Amount of cargo to unload
    ///
    /// # Returns
    /// * true if cargo unloaded successfully
    /// * false if insufficient cargo
    pub fn unload_cargo(&mut self, amount: f64) -> bool {
        if self.current_cargo >= amount {
            self.current_cargo -= amount;
            true
        } else {
            false
        }
    }

    /// Process ship maintenance for one turn.
    pub fn process_maintenance(&mut self) {
        // Condition degrades slightly each turn
        self.condition *= 0.995;
        self.condition = self.condition.max(0.5);
    }
}

/// Fleet of ships assigned to a mission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fleet {
    /// Unique fleet ID
    #[serde(rename = "id_floty")]
    pub id: String,
    /// Name of the fleet
    #[serde(rename = "nazwa_floty")]
    pub name: String,
    /// Owner country
    #[serde(rename = "kraj_właściciel")]
    pub owner_country: String,
    /// Home port ID
    #[serde(rename = "port_domowy")]
    pub home_port: String,
    /// Current mission
    #[serde(rename = "aktualna_misja")]
    pub current_mission: FleetMission,
    /// Ships in the fleet
    #[serde(rename = "statki", default)]
    pub ships: Vec<Ship>,
    /// Trade route path (if on trade mission)
    #[serde(rename = "trasa_handlowa", default)]
    pub trade_route: Vec<String>,
    /// Fishing region ID (if on fishing mission)
    #[serde(rename = "region_rybacki", default)]
    pub fishing_region: Option<String>,
    /// Operational status
    #[serde(rename = "status_operacyjny")]
    pub operational_status: bool,
}

impl Fleet {
    /// Calculate total cargo capacity of the fleet.
    pub fn total_cargo_capacity(&self) -> f64 {
        self.ships.iter().map(|s| s.cargo_capacity).sum()
    }

    /// Calculate total current cargo load of the fleet.
    pub fn total_current_cargo(&self) -> f64 {
        self.ships.iter().map(|s| s.current_cargo).sum()
    }

    /// Add a ship to the fleet.
    pub fn add_ship(&mut self, ship: Ship) {
        self.ships.push(ship);
    }

    /// Remove a ship from the fleet.
    ///
    /// # Arguments
    /// * ship_id - ID of ship to remove
    ///
    /// # Returns
    /// * true if ship removed successfully
    /// * false if ship not found
    pub fn remove_ship(&mut self, ship_id: &str) -> bool {
        if let Some(pos) = self.ships.iter().position(|s| s.id == ship_id) {
            self.ships.remove(pos);
            true
        } else {
            false
        }
    }

    /// Process fleet operations for one turn.
    pub fn process_turn(&mut self) {
        for ship in &mut self.ships {
            ship.process_maintenance();
        }

        // Fleet is operational if at least 50% of ships are in good condition
        let operational_ships = self.ships.iter().filter(|s| s.condition > 0.7).count();
        self.operational_status = operational_ships >= self.ships.len() / 2;
    }

    /// Assign fleet to a trade route.
    ///
    /// # Arguments
    /// * route - Path of region IDs for the trade route
    pub fn assign_trade_route(&mut self, route: Vec<String>) {
        self.current_mission = FleetMission::TradeRoute;
        self.trade_route = route;
        self.fishing_region = None;
    }

    /// Assign fleet to fishing operation.
    ///
    /// # Arguments
    /// * region_id - ID of region to fish in
    pub fn assign_fishing(&mut self, region_id: String) {
        self.current_mission = FleetMission::Fishing;
        self.fishing_region = Some(region_id);
        self.trade_route.clear();
    }

    /// Set fleet to idle.
    pub fn set_idle(&mut self) {
        self.current_mission = FleetMission::Idle;
        self.trade_route.clear();
        self.fishing_region = None;
    }
}

/// Apply maritime capacity constraint to market clearing.
///
/// This function enforces the physical bottleneck: overseas trade volume
/// cannot exceed the total cargo capacity of ships assigned to that route.
///
/// # Arguments
/// * trade_volume - Desired trade volume from market clearing
/// * route - The maritime trade route (path of region IDs)
/// * fleets - All fleets in the system
///
/// # Returns
/// Actual trade volume capped by physical ship capacity
pub fn apply_maritime_capacity_constraint(
    trade_volume: f64,
    route: &[String],
    fleets: &[Fleet],
) -> f64 {
    // Calculate total physical cargo capacity on this route
    let total_capacity: f64 = fleets
        .iter()
        .filter(|f| {
            f.current_mission == FleetMission::TradeRoute
                && f.operational_status
                && route_matches(&f.trade_route, route)
        })
        .map(|f| f.total_cargo_capacity())
        .sum();

    // Hard bottleneck: actual trade cannot exceed physical capacity
    trade_volume.min(total_capacity)
}

/// Check if a fleet's trade route matches the target route.
///
/// # Arguments
/// * fleet_route - The fleet's assigned route
/// * target_route - The target route to check against
///
/// # Returns
/// true if routes match (same path in same order)
fn route_matches(fleet_route: &[String], target_route: &[String]) -> bool {
    if fleet_route.len() != target_route.len() {
        return false;
    }
    fleet_route.iter().zip(target_route.iter()).all(|(a, b)| a == b)
}

/// Create a new fleet.
///
/// # Arguments
/// * name - Fleet name
/// * owner_country - Owner country
/// * home_port - Home port ID
///
/// # Returns
/// New Fleet instance
pub fn create_fleet(
    name: String,
    owner_country: String,
    home_port: String,
) -> Fleet {
    static FLEET_COUNTER: AtomicU64 = AtomicU64::new(1);
    let unique_id: u64 = FLEET_COUNTER.fetch_add(1, Ordering::SeqCst);
    Fleet {
        id: format!("Fleet-{}-{}", unique_id, name),
        name,
        owner_country,
        home_port,
        current_mission: FleetMission::Idle,
        ships: Vec::new(),
        trade_route: Vec::new(),
        fishing_region: None,
        operational_status: true,
    }
}

/// Process fleet upkeep: calculate total maintenance cost and commodity demand.
///
/// # Arguments
/// * fleets - All fleets for a country
/// * commodity_demand - Commodity demand map to populate (for market clearing)
///
/// # Returns
/// (total_maintenance_cost, total_crew_wages)
///
/// # Rules
/// * Ship maintenance_cost is deducted from the country's budget
/// * Crew wages are proportional to crew size
/// * Ships below 0.5 condition are non-operational and cost more to repair
pub fn process_fleet_upkeep(
    fleets: &[Fleet],
    commodity_demand: &mut std::collections::HashMap<crate::registries::enums::Commodity, f64>,
) -> (f64, f64) {
    let mut total_maintenance = 0.0;
    let mut total_wages = 0.0;

    for fleet in fleets {
        if !fleet.operational_status {
            continue;
        }
        for ship in &fleet.ships {
            total_maintenance += ship.maintenance_cost;
            total_wages += ship.current_crew as f64 * 100.0;

            // Ships in poor condition need more materials for repair
            if ship.condition < 0.7 {
                let repair_demand = (1.0 - ship.condition) * 50.0;
                *commodity_demand
                    .entry(crate::registries::enums::Commodity::Steel)
                    .or_insert(0.0) += repair_demand;
            }
        }
    }

    (total_maintenance, total_wages)
}
