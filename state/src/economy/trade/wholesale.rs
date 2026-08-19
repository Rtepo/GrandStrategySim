//! Wholesale distribution system for B2C market (Phase 6.5).
//!
//! Implements logistics consolidation, transport cost savings, procurement caps,
//! and escalating clearance for wholesalers to prevent bankruptcy from rotting inventory.

use crate::registries::enums::Commodity;
use crate::society::housing::{CommercialBuilding, WholesaleProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Logistics configuration for wholesale operations (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogisticsConfig {
    /// Transport cost per ton-km
    #[serde(rename = "koszt_transportu_ton_km")]
    pub transport_cost_per_ton_km: f64,
    
    /// Consolidation discount (0.0-1.0) for bulk shipments
    #[serde(rename = "zniżka_konsolidacji")]
    pub consolidation_discount: f64,
    
    /// Minimum tons for consolidation eligibility
    #[serde(rename = "minimum_konsolidacji_tony")]
    pub min_consolidation_tons: f64,
}

impl Default for LogisticsConfig {
    fn default() -> Self {
        Self {
            transport_cost_per_ton_km: 0.5,
            consolidation_discount: 0.2,
            min_consolidation_tons: 50.0,
        }
    }
}

/// Procurement request from a retailer to a wholesaler
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcurementRequest {
    /// Retailer building ID
    #[serde(rename = "id_detalisty")]
    pub retailer_building_id: String,
    
    /// Commodity requested
    #[serde(rename = "towar")]
    pub commodity: Commodity,
    
    /// Quantity requested (tons)
    #[serde(rename = "ilość")]
    pub quantity: f64,
    
    /// Maximum price per unit
    #[serde(rename = "cena_maksymalna")]
    pub max_price_per_unit: f64,
}

/// Consolidated shipment for transport cost savings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsolidatedShipment {
    /// Source wholesaler building ID
    #[serde(rename = "id_hurtownika")]
    pub wholesaler_id: String,
    
    /// Destination micro-region
    #[serde(rename = "mikroregion_docelowy")]
    pub destination_micro_region: String,
    
    /// Total tons in shipment
    #[serde(rename = "całkowite_tony")]
    pub total_tons: f64,
    
    /// Commodities in shipment
    #[serde(rename = "towary")]
    pub commodities: BTreeMap<Commodity, f64>,
    
    /// Original transport cost (before consolidation)
    #[serde(rename = "koszt_transportu_oryginalny")]
    pub original_transport_cost: f64,
    
    /// Consolidated transport cost (after discount)
    #[serde(rename = "koszt_transportu_skonsolidowany")]
    pub consolidated_transport_cost: f64,
}

/// Apply transport cost savings through consolidation (Phase 6.5, Phase R4).
///
/// # Arguments
/// * `requests` - Procurement requests from retailers
/// * `wholesalers` - Wholesaler buildings with inventory
/// * `config` - Logistics configuration
///
/// # Returns
/// * `Vec<ConsolidatedShipment>` - Consolidated shipments with cost savings
///
/// # Rules
/// * Requests are grouped by destination micro-region
/// * Shipments meeting min_consolidation_tons get discount
/// * Discount reduces transport cost by consolidation_discount fraction
/// * Used in R4 phase before B2C clearing
pub fn apply_consolidation(
    requests: &[ProcurementRequest],
    wholesalers: &[CommercialBuilding],
    config: &LogisticsConfig,
) -> Vec<ConsolidatedShipment> {
    let mut shipments = Vec::new();
    
    // Group requests by destination micro-region
    let mut by_destination: BTreeMap<String, Vec<&ProcurementRequest>> = BTreeMap::new();
    for req in requests {
        // Extract destination from retailer building (simplified - would need building lookup)
        // For now, use a placeholder key
        let destination = "placeholder_region".to_string();
        by_destination.entry(destination).or_default().push(req);
    }
    
    // Create consolidated shipments
    for (destination, reqs) in by_destination {
        let total_tons: f64 = reqs.iter().map(|r| r.quantity).sum();
        
        if total_tons >= config.min_consolidation_tons {
            // Apply consolidation discount
            let original_cost = total_tons * config.transport_cost_per_ton_km * 100.0; // 100km average
            let consolidated_cost = original_cost * (1.0 - config.consolidation_discount);
            
            let mut commodities: BTreeMap<Commodity, f64> = BTreeMap::new();
            for req in reqs {
                *commodities.entry(req.commodity).or_insert(0.0) += req.quantity;
            }
            
            shipments.push(ConsolidatedShipment {
                wholesaler_id: "placeholder_wholesaler".to_string(),
                destination_micro_region: destination,
                total_tons,
                commodities,
                original_transport_cost: original_cost,
                consolidated_transport_cost: consolidated_cost,
            });
        }
    }
    
    shipments
}

/// Enforce procurement cap for wholesalers (Phase 6.5, Phase R4).
///
/// # Arguments
/// * `wholesaler` - Wholesaler building with profile
/// * `commodity` - Commodity to check
/// * `requested_quantity` - Quantity requested by retailers
///
/// # Returns
/// * `f64` - Approved quantity (may be less than requested)
///
/// # Rules
/// * Wholesalers cannot exceed consolidation_capacity_tons per turn
/// * Prevents overcommitment and inventory rot
/// * Used in R4 phase to clamp procurement requests
pub fn enforce_procurement_cap(
    wholesaler: &CommercialBuilding,
    commodity: Commodity,
    requested_quantity: f64,
) -> f64 {
    if let Some(profile) = &wholesaler.wholesale_profile {
        let remaining_capacity = profile.consolidation_capacity_tons - profile.committed_tons_this_turn;
        requested_quantity.min(remaining_capacity)
    } else {
        requested_quantity
    }
}

/// Apply escalating clearance for stale inventory (Phase 6.5, Phase R5).
///
/// # Arguments
/// * `wholesaler` - Wholesaler building with profile
/// * `commodity` - Commodity to check
/// * `current_turn` - Current turn number
/// * `market_price` - Current market price
///
/// # Returns
/// * `Option<f64>` - Discount to apply (None = no clearance needed)
///
/// # Rules
/// * Track consecutive turns commodity sits above stock target
/// * Escalating discounts: 10% → 20% → 30% → 40% → 50% (forced sale)
/// * Prevents bankruptcy from rotting inventory
/// * Used in R5 phase before B2C clearing
pub fn apply_clearance_discount(
    wholesaler: &mut CommercialBuilding,
    commodity: Commodity,
    current_turn: u32,
    market_price: f64,
) -> Option<f64> {
    if let Some(profile) = &mut wholesaler.wholesale_profile {
        let commodity_key = commodity.to_string();
        let total_inventory: f64 = wholesaler
            .current_inventory
            .get(&commodity_key)
            .map(|batches| batches.iter().map(|b| b.quantity).sum())
            .unwrap_or(0.0);
        let stock_target = profile.consolidation_capacity_tons * 0.5;
        let is_above_target = total_inventory > stock_target;
        
        if is_above_target {
            let stale_turns = profile.stale_turns.entry(commodity).or_insert(0);
            *stale_turns += 1;
            
            // Escalating discount based on stale turns
            let discount = match *stale_turns {
                0 => 0.0, // No discount if just became stale
                1 => 0.10,
                2 => 0.20,
                3 => 0.30,
                4 => 0.40,
                5.. => 0.50, // Forced sale at 50% discount
            };
            
            Some(discount)
        } else {
            // Reset stale counter if below target
            profile.stale_turns.remove(&commodity);
            None
        }
    } else {
        None
    }
}

/// Reset committed tons at start of each turn (Phase 6.5).
///
/// # Arguments
/// * `wholesaler` - Wholesaler building with profile
///
/// # Rules
/// * Called at start of R4 phase
/// * Resets committed_tons_this_turn to 0.0
pub fn reset_procurement_commitment(wholesaler: &mut CommercialBuilding) {
    if let Some(profile) = &mut wholesaler.wholesale_profile {
        profile.committed_tons_this_turn = 0.0;
    }
}
