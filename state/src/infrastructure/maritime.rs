#![allow(missing_docs)]

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::economy::order_book::{Bid, OrderBook};
use crate::economy::market::GlobalMarket;
use crate::registries::enums::Commodity;

/// Maritime configuration (no magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MaritimeConfig {
    /// BOM for shipyard construction per ship type (commodity -> quantity)
    pub shipyard_construction_bom: BTreeMap<ShipType, BTreeMap<Commodity, f64>>,
    /// Shipyard maintenance cost per turn
    pub shipyard_maintenance_cost: f64,
    /// Port maintenance cost per turn
    pub port_maintenance_cost: f64,
    /// Ship condition degradation rate per turn
    pub ship_degradation_rate: f64,
    /// Minimum ship condition (floor)
    pub ship_min_condition: f64,
    /// Port utilization recovery rate when underused
    pub port_recovery_rate: f64,
    /// Port utilization decay rate when overused
    pub port_decay_rate: f64,
    /// Bid price fraction for shipyard construction materials (fraction of global base price)
    pub construction_bid_price_fraction: f64,
}

impl Default for MaritimeConfig {
    fn default() -> Self {
        let mut bom = BTreeMap::new();
        // Cargo vessel: steel + machinery
        let mut cargo_bom = BTreeMap::new();
        cargo_bom.insert(Commodity::Steel, 500.0);
        cargo_bom.insert(Commodity::IndustrialMachinery, 100.0);
        bom.insert(ShipType::CargoVessel, cargo_bom);
        // Passenger liner: steel + machinery
        let mut passenger_bom = BTreeMap::new();
        passenger_bom.insert(Commodity::Steel, 700.0);
        passenger_bom.insert(Commodity::IndustrialMachinery, 150.0);
        bom.insert(ShipType::PassengerLiner, passenger_bom);
        // Fishing boat: timber + machinery
        let mut fishing_bom = BTreeMap::new();
        fishing_bom.insert(Commodity::Timber, 50.0);
        fishing_bom.insert(Commodity::IndustrialMachinery, 20.0);
        bom.insert(ShipType::FishingBoat, fishing_bom);
        // Naval vessel: steel + machinery
        let mut naval_bom = BTreeMap::new();
        naval_bom.insert(Commodity::Steel, 2000.0);
        naval_bom.insert(Commodity::IndustrialMachinery, 500.0);
        bom.insert(ShipType::NavalVessel, naval_bom);

        Self {
            shipyard_construction_bom: bom,
            shipyard_maintenance_cost: 50_000.0,
            port_maintenance_cost: 25_000.0,
            ship_degradation_rate: 0.005,
            ship_min_condition: 0.5,
            port_recovery_rate: 1.05,
            port_decay_rate: 0.95,
            construction_bid_price_fraction: 1.2,
        }
    }
}

/// Aggregate maritime infrastructure for a country.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MaritimeInfrastructure {
    #[serde(default)]
    pub shipyards: Vec<Shipyard>,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub docks: Vec<Dock>,
    /// Cash reserve for maritime maintenance and construction
    #[serde(default)]
    pub available_cash: f64,
}

/// Ship type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipType {
    /// Cargo vessel for trade
    CargoVessel,
    /// Passenger liner for tourism
    PassengerLiner,
    /// Fishing boat for fishery
    FishingBoat,
    /// Naval vessel for military
    NavalVessel,
}

/// Ship construction project in a shipyard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShipConstructionProject {
    /// Unique project ID

    pub id: String,
    /// Type of ship being built

    pub ship_type: ShipType,
    /// Progress 0-1

    pub progress: f64,
    /// Total cost

    pub total_cost: f64,
    /// Cost spent so far

    pub cost_spent: f64,
    /// Duration in turns

    pub duration_turns: u32,
    /// Turns completed

    pub turns_completed: u32,
}

impl ShipConstructionProject {
    /// Process one turn of construction.
    ///
    /// # Arguments
    /// * cost_per_turn - Cost to deduct this turn
    ///
    /// # Returns
    /// * true if construction completed this turn
    /// * false if still in progress
    pub fn process_turn(&mut self, cost_per_turn: f64) -> bool {
        self.cost_spent += cost_per_turn;
        self.turns_completed += 1;
        self.progress = (self.cost_spent / self.total_cost).min(1.0);
        self.progress >= 1.0
    }
}

/// Shipyard for building ships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Shipyard {
    /// Unique shipyard ID

    pub id: String,
    /// Region where shipyard is located

    pub region_id: String,
    /// Maximum concurrent construction projects

    pub max_concurrent_projects: u32,
    /// Active construction projects
    #[serde(default)]
    pub construction_projects: Vec<ShipConstructionProject>,
    /// Construction capacity per turn

    pub construction_capacity: f64,
    /// Maintenance cost per turn

    pub maintenance_cost: f64,
}

impl Shipyard {
    /// Start a new ship construction project.
    ///
    /// # Arguments
    /// * ship_type - Type of ship to build
    /// * rng - Random number generator for unique ID
    ///
    /// # Returns
    /// * true if project started successfully
    /// * false if shipyard at capacity
    pub fn start_construction(&mut self, ship_type: ShipType, rng: &mut impl Rng) -> bool {
        if self.construction_projects.len() >= self.max_concurrent_projects as usize {
            return false;
        }

        let (total_cost, duration) = match ship_type {
            ShipType::CargoVessel => (10_000_000.0, 5),
            ShipType::PassengerLiner => (15_000_000.0, 7),
            ShipType::FishingBoat => (500_000.0, 2),
            ShipType::NavalVessel => (50_000_000.0, 10),
        };

        let unique_id: u64 = rng.gen();
        let project = ShipConstructionProject {
            id: format!("Construction-{}-{}", self.id, unique_id),
            ship_type,
            progress: 0.0,
            total_cost,
            cost_spent: 0.0,
            duration_turns: duration,
            turns_completed: 0,
        };

        self.construction_projects.push(project);
        true
    }

    /// Process all construction projects for one turn.
    ///
    /// # Returns
    /// Vector of completed ship types
    pub fn process_construction(&mut self) -> Vec<ShipType> {
        let mut completed = Vec::new();
        let cost_per_project = self.construction_capacity / self.construction_projects.len() as f64;

        self.construction_projects.retain(|project| {
            let mut project_clone = project.clone();
            let completed_this_turn = project_clone.process_turn(cost_per_project);
            if completed_this_turn {
                completed.push(project_clone.ship_type);
                false
            } else {
                true
            }
        });

        completed
    }
}

/// Port for maritime trade operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Port {
    /// Unique port ID

    pub id: String,
    /// Region where port is located

    pub region_id: String,
    /// Cargo throughput capacity (tons per turn)

    pub cargo_throughput: f64,
    /// Loading speed (tons per ship per turn)

    pub loading_speed: f64,
    /// Number of berths

    pub berth_count: u32,
    /// Utilization 0-1

    pub utilization: f64,
    /// Maintenance cost per turn

    pub maintenance_cost: f64,
}

impl Port {
    /// Calculate effective cargo throughput based on utilization.
    pub fn effective_throughput(&self) -> f64 {
        self.cargo_throughput * self.utilization
    }

    /// Process port operations for one turn.
    pub fn process_turn(&mut self) {
        // Utilization decays slightly if overused
        if self.utilization > 0.9 {
            self.utilization *= 0.95;
        } else if self.utilization < 0.5 {
            self.utilization *= 1.05; // Recover if underused
        }
        self.utilization = self.utilization.clamp(0.0, 1.0);
    }
}

/// Dock for storing and repairing idle ships.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Dock {
    /// Unique dock ID

    pub id: String,
    /// Region where dock is located

    pub region_id: String,
    /// Maximum ship capacity

    pub max_capacity: u32,
    /// Ships currently docked (ship IDs)
    #[serde(default)]
    pub docked_ships: Vec<String>,
    /// Ships under repair (ship IDs with repair progress)
    #[serde(default)]
    pub ships_under_repair: BTreeMap<String, f64>, // ship_id -> repair_progress
    /// Repair capacity per turn

    pub repair_capacity: f64,
    /// Maintenance cost per turn

    pub maintenance_cost: f64,
}

impl Dock {
    /// Dock a ship for storage.
    ///
    /// # Arguments
    /// * ship_id - ID of ship to dock
    ///
    /// # Returns
    /// * true if ship docked successfully
    /// * false if dock at capacity
    pub fn dock_ship(&mut self, ship_id: String) -> bool {
        if self.docked_ships.len() >= self.max_capacity as usize {
            return false;
        }
        self.docked_ships.push(ship_id);
        true
    }

    /// Undock a ship for active duty.
    ///
    /// # Arguments
    /// * ship_id - ID of ship to undock
    ///
    /// # Returns
    /// * true if ship undocked successfully
    /// * false if ship not found
    pub fn undock_ship(&mut self, ship_id: &str) -> bool {
        if let Some(pos) = self.docked_ships.iter().position(|id| id == ship_id) {
            self.docked_ships.remove(pos);
            true
        } else {
            false
        }
    }

    /// Start repairing a ship.
    ///
    /// # Arguments
    /// * ship_id - ID of ship to repair
    ///
    /// # Returns
    /// * true if repair started successfully
    /// * false if ship not docked or already under repair
    pub fn start_repair(&mut self, ship_id: String) -> bool {
        if !self.docked_ships.contains(&ship_id) {
            return false;
        }
        if self.ships_under_repair.contains_key(&ship_id) {
            return false;
        }
        self.ships_under_repair.insert(ship_id, 0.0);
        true
    }

    /// Process repairs for one turn.
    ///
    /// # Returns
    /// Vector of ship IDs that completed repairs
    pub fn process_repairs(&mut self) -> Vec<String> {
        let mut completed = Vec::new();
        let repair_per_ship = self.repair_capacity / self.ships_under_repair.len().max(1) as f64;

        self.ships_under_repair.retain(|ship_id, progress| {
            *progress = (*progress + repair_per_ship).min(1.0);
            if *progress >= 1.0 {
                completed.push(ship_id.clone());
                false
            } else {
                true
            }
        });

        completed
    }
}

// ============================================================================
// MARITIME TURN LOGIC
// ============================================================================

/// Phase 3.7b: Submit B2B buy orders for shipyard construction materials.
///
/// For each active construction project, submit buy orders for the required
/// BOM commodities. Encumbers maritime cash immediately.
pub fn submit_shipyard_construction_orders(
    maritime: &mut MaritimeInfrastructure,
    order_book: &mut OrderBook,
    global_market: &GlobalMarket,
    config: &MaritimeConfig,
) {
    for shipyard in &maritime.shipyards {
        for project in &shipyard.construction_projects {
            if project.turns_completed >= project.duration_turns {
                continue;
            }
            let remaining_turns = (project.duration_turns - project.turns_completed) as f64;
            if remaining_turns <= 0.0 {
                continue;
            }
            let per_turn_budget = (project.total_cost - project.cost_spent) / remaining_turns;
            if per_turn_budget <= 0.0 {
                continue;
            }

            if let Some(bom) = config.shipyard_construction_bom.get(&project.ship_type) {
                for (commodity, total_qty) in bom {
                    let per_turn_qty = total_qty / project.duration_turns as f64;
                    let base_price = global_market.base_prices.get(commodity).copied().unwrap_or(100.0);
                    let limit_price = base_price * config.construction_bid_price_fraction;
                    if limit_price <= 0.0 || per_turn_qty <= 0.0 {
                        continue;
                    }
                    let affordable = (per_turn_budget / limit_price).min(per_turn_qty);
                    if affordable <= 0.0 {
                        continue;
                    }
                    let encumbrance = affordable * limit_price;
                    maritime.available_cash -= encumbrance;

                    order_book
                        .bids
                        .entry(*commodity)
                        .or_default()
                        .push(Bid {
                            buyer_id: format!("shipyard_{}", shipyard.id),
                            commodity: *commodity,
                            quantity: affordable,
                            limit_price,
                            blueprint_id: None,
                            min_quality: None,
                        });
                }
            }
        }
    }
}

/// Post-clearing: Advance shipyard construction projects by one turn.
pub fn advance_shipyard_projects(
    maritime: &mut MaritimeInfrastructure,
    order_book: &OrderBook,
) {
    let mut filled: BTreeMap<String, BTreeMap<Commodity, f64>> = BTreeMap::new();
    for trade in &order_book.trades {
        if trade.buyer_id.starts_with("shipyard_") {
            let shipyard_id = trade.buyer_id.strip_prefix("shipyard_").unwrap_or("");
            filled
                .entry(shipyard_id.to_string())
                .or_default()
                .entry(trade.commodity)
                .and_modify(|q| *q += trade.quantity)
                .or_insert(trade.quantity);
        }
    }

    for shipyard in &mut maritime.shipyards {
        let cost_per_project = shipyard.construction_capacity
            / shipyard.construction_projects.len().max(1) as f64;

        shipyard.construction_projects.retain_mut(|project| {
            project.cost_spent += cost_per_project;
            project.turns_completed += 1;
            project.progress = (project.cost_spent / project.total_cost).min(1.0);
            project.progress >= 1.0
        });
    }
}

/// Post-clearing: Refund unfilled shipyard construction bids.
pub fn refund_unfilled_shipyard_bids(
    order_book: &OrderBook,
    maritime: &mut MaritimeInfrastructure,
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if bid.buyer_id.starts_with("shipyard_") {
                let refund = bid.quantity * bid.limit_price;
                maritime.available_cash += refund;
            }
        }
    }
}

/// Calculate total effective port throughput for a country.
pub fn total_port_throughput(maritime: &MaritimeInfrastructure) -> f64 {
    maritime.ports.iter().map(|p| p.effective_throughput()).sum()
}

/// Process port operations for one turn using config-driven rates.
pub fn process_ports_turn(
    maritime: &mut MaritimeInfrastructure,
    config: &MaritimeConfig,
) {
    for port in &mut maritime.ports {
        if port.utilization > 0.9 {
            port.utilization *= config.port_decay_rate;
        } else if port.utilization < 0.5 {
            port.utilization *= config.port_recovery_rate;
        }
        port.utilization = port.utilization.clamp(0.0, 1.0);
    }
}

/// Process shipyard maintenance: deduct cash for upkeep and credit a Construction contractor.
pub fn process_shipyard_maintenance(
    maritime: &mut MaritimeInfrastructure,
    config: &MaritimeConfig,
    companies: &mut [crate::entities::Company],
) {
    let total_maintenance = maritime.shipyards.len() as f64 * config.shipyard_maintenance_cost
        + maritime.ports.len() as f64 * config.port_maintenance_cost;
    let affordable = total_maintenance.min(maritime.available_cash);
    maritime.available_cash -= affordable;
    if affordable > 0.0 {
        let contractor_id = companies
            .iter()
            .find(|c| c.sector == crate::registries::enums::Sector::Construction)
            .map(|c| c.id.clone());
        if let Some(cid) = contractor_id {
            crate::economy::transfer_settler::credit_company_by_id(companies, &cid, affordable);
        }
    }
}
