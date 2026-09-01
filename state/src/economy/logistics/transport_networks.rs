//! Phase 23B: Transport network infrastructure overlay.
//!
//! Defines the `TransportNetworkOverlay` — a bidirectional mapping of
//! region-pair connections to `NetworkLevel` (roads, rail, highways, canals).
//! The overlay is an edge-quality layer that sits on top of the existing
//! geography graph (`Region.edges`) and reduces the spatial friction of
//! freight routes that traverse improved links.
//!
//! # Construction
//! Networks are built via the Phase 22 `ConstructionTender` system by the
//! State. On project completion, a `NetworkLink` is installed into the
//! overlay. See `construction::orders` for the completion hook.
//!
//! # Maintenance
//! `NetworkLink.condition` degrades each turn. Low-condition links lose
//! their friction bonus. Maintenance is funded by the owning treasury.

use crate::society::geography::EdgeType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Level of transport infrastructure on a connection between two regions.
///
/// Higher levels reduce the spatial friction (logistics cost) of freight
/// routes traversing the link, and unlock advanced production methods
/// (e.g., Electric Trams require `ElectrifiedRail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkLevel {
    /// No improved infrastructure (baseline dirt paths).
    #[default]
    None,
    /// Gravel/dirt road (slight friction reduction).
    DirtRoad,
    /// Paved road (moderate reduction).
    PavedRoad,
    /// Rail network (large reduction; unlocks trains).
    RailNetwork,
    /// Electrified rail (unlocks electric trains; requires Energy).
    ElectrifiedRail,
    /// Highway (modern; large reduction for trucks).
    Highway,
    /// Canal (waterborne; for non-coastal freight).
    Canal,
}

impl NetworkLevel {
    /// Friction multiplier applied to `LandBorder` edges at this network level.
    ///
    /// Lower = cheaper/faster freight. `None` = 1.0 (baseline, no bonus).
    pub fn land_friction_multiplier(&self) -> f64 {
        match self {
            NetworkLevel::None => 1.0,
            NetworkLevel::DirtRoad => 0.8,
            NetworkLevel::PavedRoad => 0.6,
            NetworkLevel::RailNetwork => 0.35,
            NetworkLevel::ElectrifiedRail => 0.30,
            NetworkLevel::Highway => 0.40,
            NetworkLevel::Canal => 0.25,
        }
    }

    /// Returns `true` if this level is at least as advanced as `other`.
    pub fn at_least(&self, other: NetworkLevel) -> bool {
        self.rank() >= other.rank()
    }

    /// Ordinal rank for comparison (higher = more advanced).
    pub fn rank(&self) -> u8 {
        match self {
            NetworkLevel::None => 0,
            NetworkLevel::DirtRoad => 1,
            NetworkLevel::PavedRoad => 2,
            NetworkLevel::RailNetwork => 3,
            NetworkLevel::ElectrifiedRail => 4,
            NetworkLevel::Highway => 3, // parallel to rail, different modality
            NetworkLevel::Canal => 2,   // parallel to paved road, waterborne
        }
    }

    /// Phase 30: Fuel consumption rate per km for this network level.
    ///
    /// Returns the fuel units consumed per ton-km of freight transported
    /// on this network type. Used by the multi-modal Dijkstra edge weight
    /// to compute fuel costs alongside friction.
    ///
    /// Lower = more fuel-efficient. Rail is the most efficient; dirt roads
    /// are the least efficient (trucks burn more fuel on bad surfaces).
    /// ElectrifiedRail consumes Energy, not Fuels — the caller must handle
    /// this distinction when looking up the fuel price.
    pub fn fuel_consumption_per_km(&self) -> f64 {
        match self {
            NetworkLevel::None => 0.08,             // unimproved: trucks on dirt
            NetworkLevel::DirtRoad => 0.06,         // slight improvement
            NetworkLevel::PavedRoad => 0.04,        // smooth surface
            NetworkLevel::RailNetwork => 0.02,      // rail is very efficient
            NetworkLevel::ElectrifiedRail => 0.015, // electric, lowest fuel
            NetworkLevel::Highway => 0.035,         // fast but fuel-hungry
            NetworkLevel::Canal => 0.01,            // barge: most fuel-efficient
        }
    }

    /// Phase 30: Returns the commodity this network level consumes as fuel.
    ///
    /// ElectrifiedRail consumes `Energy`; all other levels consume `Fuels`.
    pub fn fuel_commodity(&self) -> crate::registries::enums::Commodity {
        match self {
            NetworkLevel::ElectrifiedRail => crate::registries::enums::Commodity::Energy,
            _ => crate::registries::enums::Commodity::Fuels,
        }
    }
}

/// A bidirectional transport network link between two regions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NetworkLink {
    /// One endpoint region ID.
    pub region_a: String,
    /// Other endpoint region ID.
    pub region_b: String,
    /// Infrastructure level of this link.
    pub level: NetworkLevel,
    /// Condition 0.0–1.0 (degrades each turn; requires maintenance).
    /// Low condition reduces the effective friction bonus.
    pub condition: f64,
    /// Turn the link was constructed.
    pub built_turn: u32,
    /// Phase 30: Congestion level 0.0–1.0 (0.0 = empty, 1.0 = gridlocked).
    /// Builds up from freight traffic passing through this link, decays
    /// each turn. Scales effective friction by (1 + congestion_penalty × congestion).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub congestion: f64,
}

impl NetworkLink {
    /// Effective friction multiplier, scaled by condition and congestion.
    /// At condition 1.0 → full bonus. At condition 0.0 → no bonus (1.0).
    /// Congestion adds a penalty: effective_friction × (1 + congestion × penalty).
    pub fn effective_friction(&self) -> f64 {
        let base = self.level.land_friction_multiplier();
        // Linearly interpolate from base (condition=1.0) to 1.0 (condition=0.0)
        let condition_friction = 1.0 + (base - 1.0) * self.condition.max(0.0).min(1.0);
        // Phase 30: Congestion penalty — each unit of congestion adds 50% friction.
        let congestion_factor = 1.0 + self.congestion.max(0.0).min(1.0) * 0.5;
        condition_friction * congestion_factor
    }

    /// Phase 30: Add congestion to this link from freight traffic.
    /// Each unit of freight capacity consumed adds a small congestion increment.
    pub fn add_congestion(&mut self, freight_units: f64) {
        // 100 freight units → +0.1 congestion (tuned for gradual buildup)
        self.congestion = (self.congestion + freight_units * 0.001).min(1.0);
    }

    /// Phase 30: Decay congestion by a given rate (e.g., 0.10 = 10% per turn).
    pub fn decay_congestion(&mut self, decay_rate: f64) {
        self.congestion = (self.congestion - decay_rate).max(0.0);
    }
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// Overlay mapping region-pair keys → `NetworkLink`.
///
/// Stored on `Country` (national networks). The key is a canonical
/// bidirectional string: `"min(a,b)|max(a,b)"` so lookups work regardless
/// of traversal direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TransportNetworkOverlay {
    /// Canonical region-pair key → network link.
    pub links: BTreeMap<String, NetworkLink>,
}

impl TransportNetworkOverlay {
    /// Canonical bidirectional key for a region pair.
    pub fn link_key(a: &str, b: &str) -> String {
        if a <= b {
            format!("{a}|{b}")
        } else {
            format!("{b}|{a}")
        }
    }

    /// Look up the network link between two regions (read-only).
    pub fn get_link(&self, a: &str, b: &str) -> Option<&NetworkLink> {
        self.links.get(&Self::link_key(a, b))
    }

    /// Look up the network link between two regions (mutable).
    pub fn get_link_mut(&mut self, a: &str, b: &str) -> Option<&mut NetworkLink> {
        self.links.get_mut(&Self::link_key(a, b))
    }

    /// Install or upgrade a network link between two regions.
    pub fn install_link(&mut self, a: &str, b: &str, level: NetworkLevel, turn: u32) {
        let key = Self::link_key(a, b);
        self.links.insert(
            key,
            NetworkLink {
                region_a: a.to_string(),
                region_b: b.to_string(),
                level,
                condition: 1.0,
                built_turn: turn,
                congestion: 0.0,
            },
        );
    }

    /// Friction multiplier for traversing between regions `a` and `b`,
    /// considering the edge type and any network overlay.
    ///
    /// # Rules
    /// * If a `NetworkLink` exists between `a` and `b`, its effective
    ///   friction (scaled by condition) is returned for `LandBorder` edges.
    /// * For `Coastline`/`SeaLane` edges, the network overlay is ignored
    ///   (waterborne transport has its own friction, applied by the caller).
    /// * If no link exists, returns 1.0 (baseline).
    pub fn friction_multiplier(&self, a: &str, b: &str, edge_type: &EdgeType) -> f64 {
        match edge_type {
            EdgeType::LandBorder | EdgeType::River => self
                .get_link(a, b)
                .map(|link| link.effective_friction())
                .unwrap_or(1.0),
            // Waterborne edges are not affected by land network overlays.
            EdgeType::Coastline | EdgeType::SeaLane => 1.0,
        }
    }
}

/// Phase 23B: Degrade all network links by one turn.
///
/// Each link's `condition` decreases by `degradation_rate` per turn.
/// Links at condition 0.0 provide no friction bonus.
///
/// # Arguments
/// * `overlay` - Mutable overlay to degrade.
/// * `degradation_rate` - Condition loss per turn (e.g., 0.01 = 1% per turn).
pub fn degrade_networks(overlay: &mut TransportNetworkOverlay, degradation_rate: f64) {
    for link in overlay.links.values_mut() {
        link.condition = (link.condition - degradation_rate).max(0.0);
    }
}

/// Phase 30: Decay congestion on all network links by a given rate.
///
/// Called at the end of each turn to prevent permanent gridlock.
/// A decay rate of 0.10 reduces congestion by 10% per turn.
pub fn decay_congestion(overlay: &mut TransportNetworkOverlay, decay_rate: f64) {
    for link in overlay.links.values_mut() {
        link.decay_congestion(decay_rate);
    }
}

/// Phase 23B: Restore network link conditions by spending treasury funds.
///
/// Links with condition < 1.0 are repaired proportionally. The cost is
/// `repair_cost_per_condition_point × condition_deficit × distance_factor`,
/// charged to the provided treasury. Maintenance is capped by available funds.
///
/// # Arguments
/// * `overlay` - Mutable overlay to repair.
/// * `treasury_cash` - Available treasury funds (mutated — debited for repairs).
/// * `repair_cost_per_condition_point` - Cost per 0.1 condition restoration.
///
/// # Returns
/// Total cash spent on repairs.
pub fn process_network_maintenance(
    overlay: &mut TransportNetworkOverlay,
    treasury_cash: &mut f64,
    repair_cost_per_condition_point: f64,
) -> f64 {
    let mut total_spent = 0.0;

    for link in overlay.links.values_mut() {
        if link.condition >= 1.0 {
            continue;
        }
        let deficit = 1.0 - link.condition;
        let repair_cost = deficit * repair_cost_per_condition_point;
        if *treasury_cash >= repair_cost {
            *treasury_cash -= repair_cost;
            link.condition = 1.0;
            total_spent += repair_cost;
        } else if *treasury_cash > 0.0 {
            // Partial repair with remaining funds.
            let partial = *treasury_cash / repair_cost_per_condition_point;
            link.condition = (link.condition + partial).min(1.0);
            total_spent += *treasury_cash;
            *treasury_cash = 0.0;
        }
    }

    total_spent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_key_is_canonical_bidirectional() {
        assert_eq!(
            TransportNetworkOverlay::link_key("region_a", "region_b"),
            TransportNetworkOverlay::link_key("region_b", "region_a")
        );
    }

    #[test]
    fn none_level_has_baseline_friction() {
        assert!((NetworkLevel::None.land_friction_multiplier() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rail_reduces_friction() {
        assert!(NetworkLevel::RailNetwork.land_friction_multiplier() < 1.0);
        assert!(
            NetworkLevel::RailNetwork.land_friction_multiplier()
                < NetworkLevel::DirtRoad.land_friction_multiplier()
        );
    }

    #[test]
    fn install_and_lookup_link() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::RailNetwork, 10);
        assert!(overlay.get_link("r1", "r2").is_some());
        assert!(overlay.get_link("r2", "r1").is_some()); // bidirectional
        assert_eq!(
            overlay.get_link("r1", "r2").unwrap().level,
            NetworkLevel::RailNetwork
        );
    }

    #[test]
    fn condition_scales_friction() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 10);
        // Full condition → full bonus
        let full = overlay.get_link("r1", "r2").unwrap().effective_friction();
        assert!((full - 0.40).abs() < 1e-9);
        // Zero condition → no bonus
        overlay.get_link_mut("r1", "r2").unwrap().condition = 0.0;
        let degraded = overlay.get_link("r1", "r2").unwrap().effective_friction();
        assert!((degraded - 1.0).abs() < 1e-9);
    }

    #[test]
    fn friction_multiplier_no_link_returns_baseline() {
        let overlay = TransportNetworkOverlay::default();
        let friction = overlay.friction_multiplier("r1", "r2", &EdgeType::LandBorder);
        assert!((friction - 1.0).abs() < 1e-9);
    }

    #[test]
    fn degrade_networks_lowers_condition() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        let initial = overlay.get_link("r1", "r2").unwrap().condition;
        degrade_networks(&mut overlay, 0.05);
        let after = overlay.get_link("r1", "r2").unwrap().condition;
        assert!(after < initial);
        assert!((after - 0.95).abs() < 1e-9);
    }

    #[test]
    fn process_network_maintenance_restores_condition() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        // Degrade to 0.5
        overlay.get_link_mut("r1", "r2").unwrap().condition = 0.5;
        let mut cash = 1000.0_f64;
        let spent = process_network_maintenance(&mut overlay, &mut cash, 100.0);
        // Deficit = 0.5, cost = 0.5 * 100.0 = 50.0
        assert!((spent - 50.0).abs() < 1e-9);
        assert!((overlay.get_link("r1", "r2").unwrap().condition - 1.0).abs() < 1e-9);
    }

    #[test]
    fn process_network_maintenance_partial_with_limited_funds() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        overlay.get_link_mut("r1", "r2").unwrap().condition = 0.5;
        let mut cash = 25.0_f64; // only enough for 0.25 restoration
        let spent = process_network_maintenance(&mut overlay, &mut cash, 100.0);
        assert!((spent - 25.0).abs() < 1e-9);
        assert!((overlay.get_link("r1", "r2").unwrap().condition - 0.75).abs() < 1e-9);
    }

    // ═══════════════════════════════════════════════════════════
    // Phase 30: Congestion and fuel consumption tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn congestion_increases_friction() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        let base_friction = overlay.get_link("r1", "r2").unwrap().effective_friction();
        // Add congestion
        overlay.get_link_mut("r1", "r2").unwrap().congestion = 0.5;
        let congested_friction = overlay.get_link("r1", "r2").unwrap().effective_friction();
        // Congestion factor = 1 + 0.5 * 0.5 = 1.25
        assert!((congested_friction - base_friction * 1.25).abs() < 1e-9);
        assert!(congested_friction > base_friction);
    }

    #[test]
    fn congestion_decays_per_turn() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        overlay.get_link_mut("r1", "r2").unwrap().congestion = 0.5;
        decay_congestion(&mut overlay, 0.10);
        assert!((overlay.get_link("r1", "r2").unwrap().congestion - 0.4).abs() < 1e-9);
        // Decay again
        decay_congestion(&mut overlay, 0.10);
        assert!((overlay.get_link("r1", "r2").unwrap().congestion - 0.3).abs() < 1e-9);
    }

    #[test]
    fn add_congestion_from_freight() {
        let mut overlay = TransportNetworkOverlay::default();
        overlay.install_link("r1", "r2", NetworkLevel::Highway, 1);
        let link = overlay.get_link_mut("r1", "r2").unwrap();
        link.add_congestion(100.0); // 100 freight units → +0.1 congestion
        assert!((link.congestion - 0.1).abs() < 1e-9);
    }

    #[test]
    fn fuel_consumption_varies_by_mode() {
        // Rail is more fuel-efficient than dirt roads
        assert!(
            NetworkLevel::RailNetwork.fuel_consumption_per_km()
                < NetworkLevel::DirtRoad.fuel_consumption_per_km()
        );
        // Highway is more efficient than None (unimproved)
        assert!(
            NetworkLevel::Highway.fuel_consumption_per_km()
                < NetworkLevel::None.fuel_consumption_per_km()
        );
        // Canal (barge) is the most fuel-efficient
        assert!(
            NetworkLevel::Canal.fuel_consumption_per_km()
                < NetworkLevel::RailNetwork.fuel_consumption_per_km()
        );
    }

    #[test]
    fn electrified_rail_uses_energy_not_fuels() {
        use crate::registries::enums::Commodity;
        assert_eq!(
            NetworkLevel::ElectrifiedRail.fuel_commodity(),
            Commodity::Energy
        );
        assert_eq!(NetworkLevel::Highway.fuel_commodity(), Commodity::Fuels);
        assert_eq!(NetworkLevel::RailNetwork.fuel_commodity(), Commodity::Fuels);
    }
}
