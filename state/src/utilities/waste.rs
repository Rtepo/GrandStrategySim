//! Modular waste management infrastructure
//!
//! This module implements landfills with modular upgrades (IncineratorModule, RecyclingModule)
//! that process waste and recover commodities back to the market.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Modular landfill upgrade
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandfillUpgrade {
    /// Incinerator module - destroys non-recyclable waste
    IncineratorModule,
    /// Recycling module - recovers basic commodities
    RecyclingModule,
}

/// Waste processing result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WasteProcessingResult {
    /// Waste destroyed (tons)
    #[serde(default)]
    pub waste_destroyed: f64,
    
    /// Commodities recovered (Commodity -> tons)
    #[serde(default)]
    pub commodities_recovered: BTreeMap<String, f64>,
    
    /// Pollution generated (0-1)
    #[serde(default)]
    pub pollution_generated: f64,
}

/// Modular landfill infrastructure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Landfill {
    /// Unique landfill ID
    #[serde(default)]
    pub id: String,
    
    /// Micro-region where landfill is located
    #[serde(default)]
    pub micro_region_id: String,
    
    /// Total capacity (tons)
    #[serde(default)]
    pub total_capacity: f64,
    
    /// Current waste volume (tons)
    #[serde(default)]
    pub current_volume: f64,
    
    /// Modular upgrades installed
    #[serde(default)]
    pub upgrades: Vec<LandfillUpgrade>,
    
    /// Processing capacity per turn (tons)
    #[serde(default)]
    pub processing_capacity: f64,
    
    /// Operating cost per turn
    #[serde(default)]
    pub operating_cost: f64,
}

impl Landfill {
    /// Process waste for one turn with modular upgrades
    ///
    /// # Arguments
    /// * `waste_input` - Waste input for this turn (tons)
    ///
    /// # Returns
    /// * Waste processing result with destroyed waste, recovered commodities, and pollution
    pub fn process_waste(&mut self, waste_input: f64) -> WasteProcessingResult {
        let has_incinerator = self.upgrades.contains(&LandfillUpgrade::IncineratorModule);
        let has_recycling = self.upgrades.contains(&LandfillUpgrade::RecyclingModule);

        let mut commodities_recovered = BTreeMap::new();

        // Base processing (compaction only)
        let base_processed = waste_input.min(self.processing_capacity);
        let mut waste_destroyed = base_processed * 0.3; // 30% volume reduction via compaction
        let mut pollution = base_processed * 0.05; // Low pollution from compaction
        
        // Incinerator module: destroys 90% of waste, generates pollution
        if has_incinerator {
            let incinerated = base_processed * 0.9;
            waste_destroyed += incinerated;
            pollution += incinerated * 0.15; // Higher pollution from incineration
        }
        
        // Recycling module: recovers 40% of waste as commodities
        if has_recycling {
            let recycled = base_processed * 0.4;
            waste_destroyed += recycled;
            commodities_recovered.insert("metal".to_string(), recycled * 0.3);
            commodities_recovered.insert("plastic".to_string(), recycled * 0.2);
            commodities_recovered.insert("paper".to_string(), recycled * 0.5);
            pollution -= recycled * 0.02; // Slightly reduces pollution
        }
        
        // Update landfill volume
        let remaining_waste = waste_input - waste_destroyed;
        self.current_volume = (self.current_volume + remaining_waste).min(self.total_capacity);
        
        WasteProcessingResult {
            waste_destroyed,
            commodities_recovered,
            pollution_generated: pollution.max(0.0).min(1.0),
        }
    }
    
    /// Install a modular upgrade
    ///
    /// # Arguments
    /// * `upgrade` - The upgrade to install
    pub fn install_upgrade(&mut self, upgrade: LandfillUpgrade) {
        if !self.upgrades.contains(&upgrade) {
            self.upgrades.push(upgrade);
            self.processing_capacity *= 1.5; // Each upgrade increases capacity
            self.operating_cost *= 1.2; // Upgrades increase operating cost
        }
    }
    
    /// Check if landfill has capacity for more waste
    ///
    /// # Returns
    /// * true if landfill has remaining capacity
    pub fn has_capacity(&self) -> bool {
        self.current_volume < self.total_capacity
    }
    
    /// Get remaining capacity
    ///
    /// # Returns
    /// * Remaining capacity in tons
    pub fn remaining_capacity(&self) -> f64 {
        self.total_capacity - self.current_volume
    }
}

/// Typed landfill metadata embedded on `Building` as `Option<LandfillData>`.
///
/// This struct stores the physical state of a landfill building (capacity, volume,
/// upgrades) with full Rust type safety, following the pattern of
/// `Option<ConstructionProject>` on `Building`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LandfillData {
    /// Total capacity (tons)
    #[serde(default)]
    pub total_capacity: f64,

    /// Current waste volume (tons)
    #[serde(default)]
    pub current_volume: f64,

    /// Processing capacity per turn (tons)
    #[serde(default)]
    pub processing_capacity: f64,

    /// Operating cost per turn
    #[serde(default)]
    pub operating_cost: f64,

    /// Modular upgrades installed
    #[serde(default)]
    pub upgrades: Vec<LandfillUpgrade>,
}

impl LandfillData {
    /// Process waste for one turn with modular upgrades.
    ///
    /// # Arguments
    /// * `waste_input` - Waste input for this turn (tons)
    ///
    /// # Returns
    /// * Waste processing result with destroyed waste, recovered commodities, and pollution
    pub fn process_waste(&mut self, waste_input: f64) -> WasteProcessingResult {
        let has_incinerator = self.upgrades.contains(&LandfillUpgrade::IncineratorModule);
        let has_recycling = self.upgrades.contains(&LandfillUpgrade::RecyclingModule);

        let mut commodities_recovered = BTreeMap::new();

        let base_processed = waste_input.min(self.processing_capacity);
        let mut waste_destroyed = base_processed * 0.3;
        let mut pollution = base_processed * 0.05;

        if has_incinerator {
            let incinerated = base_processed * 0.9;
            waste_destroyed += incinerated;
            pollution += incinerated * 0.15;
        }

        if has_recycling {
            let recycled = base_processed * 0.4;
            waste_destroyed += recycled;
            commodities_recovered.insert("metal".to_string(), recycled * 0.3);
            commodities_recovered.insert("plastic".to_string(), recycled * 0.2);
            commodities_recovered.insert("paper".to_string(), recycled * 0.5);
            pollution -= recycled * 0.02;
        }

        let remaining_waste = waste_input - waste_destroyed;
        self.current_volume = (self.current_volume + remaining_waste).min(self.total_capacity);

        WasteProcessingResult {
            waste_destroyed,
            commodities_recovered,
            pollution_generated: pollution.max(0.0).min(1.0),
        }
    }

    /// Install a modular upgrade.
    pub fn install_upgrade(&mut self, upgrade: LandfillUpgrade) {
        if !self.upgrades.contains(&upgrade) {
            self.upgrades.push(upgrade);
            self.processing_capacity *= 1.5;
            self.operating_cost *= 1.2;
        }
    }

    /// Check if landfill has capacity for more waste.
    pub fn has_capacity(&self) -> bool {
        self.current_volume < self.total_capacity
    }

    /// Get remaining capacity in tons.
    pub fn remaining_capacity(&self) -> f64 {
        self.total_capacity - self.current_volume
    }
}
