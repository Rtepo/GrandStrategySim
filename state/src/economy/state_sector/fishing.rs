#![allow(missing_docs)]

use crate::economy::fishing_config::FishingConfig;
use crate::economy::market_history::MarketHistory;
use crate::economy::order_book::{Ask, OrderBook};
use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fish stock in a water body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishStock {
    /// Unique fish stock ID
    #[serde(rename = "id_zapasu")]
    pub id: String,
    /// Water body/region ID
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Total biomass in tons
    #[serde(rename = "biomasa_całkowita")]
    pub total_biomass: f64,
    /// Health 0-1 (affects regeneration)
    #[serde(rename = "zdrowie")]
    pub health: f64,
    /// Regeneration rate per turn (percentage of biomass)
    #[serde(rename = "tempo_regeneracji")]
    pub regeneration_rate: f64,
    /// Maximum sustainable biomass
    #[serde(rename = "biomasa_maksymalna")]
    pub max_biomass: f64,
    /// Species distribution (species -> percentage)
    #[serde(rename = "dystrybucja_gatunków", default)]
    pub species_distribution: BTreeMap<String, f64>,
}

impl FishStock {
    /// Calculate sustainable catch for this turn.
    pub fn sustainable_catch(&self) -> f64 {
        self.total_biomass * self.regeneration_rate * self.health
    }

    /// Process fish stock regeneration for one turn.
    pub fn process_regeneration(&mut self) {
        let regeneration = self.total_biomass * self.regeneration_rate * self.health;
        self.total_biomass = (self.total_biomass + regeneration).min(self.max_biomass);
    }

    /// Apply fishing catch to the stock.
    ///
    /// # Arguments
    /// * catch_amount - Amount of fish caught
    /// * config - Fishing configuration for health decay/recovery parameters
    ///
    /// # Returns
    /// * true if catch was sustainable
    /// * false if overfishing occurred
    pub fn apply_catch(&mut self, catch_amount: f64, config: &FishingConfig) -> bool {
        let sustainable = catch_amount <= self.sustainable_catch();
        self.total_biomass = (self.total_biomass - catch_amount).max(0.0);

        // Overfishing degrades health
        if !sustainable {
            self.health *= config.overfishing_health_decay;
            self.health = self.health.max(config.min_health_floor);
        } else {
            // Sustainable fishing allows recovery
            self.health = (self.health + config.sustainable_health_recovery).min(1.0);
        }

        sustainable
    }
}

/// Fishing quota for a region.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishingQuota {
    /// Unique quota ID
    #[serde(rename = "id_limitu")]
    pub id: String,
    /// Region ID
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Country issuing the quota
    #[serde(rename = "kraj_wydający")]
    pub issuing_country: String,
    /// Maximum catch per turn (tons)
    #[serde(rename = "maksymalny_połów")]
    pub max_catch: f64,
    /// Current catch this turn
    #[serde(rename = "aktualny_połów")]
    pub current_catch: f64,
    /// Quota type
    #[serde(rename = "typ_limitu")]
    pub quota_type: FishingQuotaType,
    /// Valid from turn
    #[serde(rename = "ważny_od")]
    pub valid_from: u32,
    /// Valid until turn
    #[serde(rename = "ważny_do")]
    pub valid_until: u32,
}

/// Fishing quota type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FishingQuotaType {
    /// State waters quota (domestic)
    StateWaters,
    /// International waters quota (treaty-based)
    InternationalWaters,
    /// Scientific research quota
    Scientific,
}

impl FishingQuota {
    /// Check if quota is valid for current turn.
    ///
    /// # Arguments
    /// * current_turn - Current game turn
    ///
    /// # Returns
    /// true if quota is valid
    pub fn is_valid(&self, current_turn: u32) -> bool {
        current_turn >= self.valid_from && current_turn <= self.valid_until
    }

    /// Check if catch is within quota.
    ///
    /// # Arguments
    /// * catch_amount - Amount to catch
    ///
    /// # Returns
    /// true if catch is within quota
    pub fn is_within_quota(&self, catch_amount: f64) -> bool {
        self.current_catch + catch_amount <= self.max_catch
    }

    /// Record a catch against the quota.
    ///
    /// # Arguments
    /// * catch_amount - Amount caught
    pub fn record_catch(&mut self, catch_amount: f64) {
        self.current_catch += catch_amount;
    }

    /// Reset current catch for new turn.
    pub fn reset_catch(&mut self) {
        self.current_catch = 0.0;
    }
}

/// Fishing policy for a country.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishingPolicy {
    /// Country implementing the policy
    #[serde(rename = "kraj")]
    pub country: String,
    /// Policy type
    #[serde(rename = "typ_polityki")]
    pub policy_type: FishingPolicyType,
    /// Restriction level 0-1 (0 = no restriction, 1 = total ban)
    #[serde(rename = "poziom_ograniczenia")]
    pub restriction_level: f64,
    /// Minimum fish stock health required
    #[serde(rename = "minimalne_zdrowie_zapasu")]
    pub min_stock_health: f64,
    /// Penalty for overfishing (percentage of catch value)
    #[serde(rename = "kara_za_nadmierny_połów")]
    pub overfishing_penalty: f64,
    /// Subsidy for sustainable fishing (percentage of costs)
    #[serde(rename = "subwencja_za_zrównoważony_połów")]
    pub sustainable_fishing_subsidy: f64,
}

/// Fishing policy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FishingPolicyType {
    /// Free-for-all (no restrictions)
    Unregulated,
    /// State-controlled quotas
    StateControlled,
    /// Sustainable management
    Sustainable,
    /// Conservation-focused
    Conservation,
}

impl FishingPolicy {
    /// Calculate allowable catch based on policy.
    ///
    /// # Arguments
    /// * sustainable_catch - Sustainable catch from fish stock
    ///
    /// # Returns
    /// Allowable catch under this policy
    pub fn allowable_catch(&self, sustainable_catch: f64) -> f64 {
        match self.policy_type {
            FishingPolicyType::Unregulated => sustainable_catch * 2.0, // Allow overfishing
            FishingPolicyType::StateControlled => sustainable_catch * (1.0 - self.restriction_level * 0.5),
            FishingPolicyType::Sustainable => sustainable_catch * 0.9,
            FishingPolicyType::Conservation => sustainable_catch * 0.5,
        }
    }

    /// Check if fishing is allowed given stock health.
    ///
    /// # Arguments
    /// * stock_health - Current fish stock health
    ///
    /// # Returns
    /// true if fishing is allowed
    pub fn is_fishing_allowed(&self, stock_health: f64) -> bool {
        stock_health >= self.min_stock_health
    }
}

/// International fishing treaty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishingTreaty {
    /// Unique treaty ID
    #[serde(rename = "id_traktatu")]
    pub id: String,
    /// Treaty name
    #[serde(rename = "nazwa")]
    pub name: String,
    /// Signatory countries
    #[serde(rename = "sygnatariusze", default)]
    pub signatories: Vec<String>,
    /// Region/water body covered
    #[serde(rename = "region_objęty")]
    pub covered_region: String,
    /// Total allowable catch for all signatories
    #[serde(rename = "całkowity_dopuszczalny_połów")]
    pub total_allowable_catch: f64,
    /// Country quotas (country -> tons)
    #[serde(rename = "limity_krajów", default)]
    pub country_quotas: BTreeMap<String, f64>,
    /// Enforcement level 0-1
    #[serde(rename = "poziom_egzekwowania")]
    pub enforcement_level: f64,
    /// Valid from turn
    #[serde(rename = "ważny_od")]
    pub valid_from: u32,
    /// Valid until turn
    #[serde(rename = "ważny_do")]
    pub valid_until: u32,
}

impl FishingTreaty {
    /// Check if treaty is valid for current turn.
    ///
    /// # Arguments
    /// * current_turn - Current game turn
    ///
    /// # Returns
    /// true if treaty is valid
    pub fn is_valid(&self, current_turn: u32) -> bool {
        current_turn >= self.valid_from && current_turn <= self.valid_until
    }

    /// Get quota for a specific country.
    ///
    /// # Arguments
    /// * country - Country to get quota for
    ///
    /// # Returns
    /// Quota for the country, or 0 if not a signatory
    pub fn get_country_quota(&self, country: &str) -> f64 {
        self.country_quotas.get(country).copied().unwrap_or(0.0)
    }

    /// Check if a country is a signatory.
    ///
    /// # Arguments
    /// * country - Country to check
    ///
    /// # Returns
    /// true if country is a signatory
    pub fn is_signatory(&self, country: &str) -> bool {
        self.signatories.contains(&country.to_string())
    }
}

/// Onshore fish farm for aquaculture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FishFarm {
    /// Unique fish farm ID
    #[serde(rename = "id_fermry")]
    pub id: String,
    /// Region where farm is located
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Owner (company or state)
    #[serde(rename = "właściciel")]
    pub owner: String,
    /// Farm type
    #[serde(rename = "typ_fermry")]
    pub farm_type: FishFarmType,
    /// Production capacity (tons per turn)
    #[serde(rename = "pojemność_produkcji")]
    pub production_capacity: f64,
    /// Current production
    #[serde(rename = "aktualna_produkcja")]
    pub current_production: f64,
    /// Operating cost per turn
    #[serde(rename = "koszt_operacyjny")]
    pub operating_cost: f64,
    /// Feed cost per ton
    #[serde(rename = "koszt_karmy")]
    pub feed_cost: f64,
    /// Water quality 0-1
    #[serde(rename = "jakość_wody")]
    pub water_quality: f64,
    /// Disease risk 0-1
    #[serde(rename = "ryzyko_choroby")]
    pub disease_risk: f64,
}

/// Fish farm type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FishFarmType {
    /// Freshwater fish (carp, trout)
    Freshwater,
    /// Saltwater fish (salmon, sea bass)
    Saltwater,
    /// Shellfish (shrimp, mussels)
    Shellfish,
    /// Mixed species
    Mixed,
}

impl FishFarm {
    /// Calculate effective production based on water quality.
    pub fn effective_production(&self) -> f64 {
        self.production_capacity * self.water_quality * (1.0 - self.disease_risk)
    }

    /// Process fish farm operations for one turn.
    ///
    /// # Arguments
    /// * config - Fishing configuration for water quality and disease parameters
    /// * turn - Current turn number (used for deterministic disease logic)
    ///
    /// # Returns
    /// Actual production this turn
    pub fn process_turn(&mut self, config: &FishingConfig, turn: u32) -> f64 {
        // Water quality degrades slightly (deterministic)
        self.water_quality *= config.farm_water_quality_decay;
        self.water_quality = self.water_quality.max(config.farm_min_water_quality);

        // Deterministic disease logic: disease increases every 3rd turn,
        // decreases otherwise. No RNG needed.
        if turn % 3 == 0 {
            self.disease_risk = (self.disease_risk + config.farm_disease_increase)
                .min(config.farm_max_disease_risk);
        } else {
            self.disease_risk = (self.disease_risk - config.farm_disease_decrease).max(0.0);
        }

        let production = self.effective_production();
        self.current_production = production;
        production
    }

    /// Apply maintenance to improve water quality.
    pub fn apply_maintenance(&mut self) {
        self.water_quality = (self.water_quality + 0.1).min(1.0);
        self.disease_risk = (self.disease_risk - 0.05).max(0.0);
    }
}

/// Create a new fish stock.
///
/// # Arguments
/// * region_id - Region/water body ID
/// * max_biomass - Maximum sustainable biomass
/// * regeneration_rate - Regeneration rate per turn
/// * config - Fishing configuration for initial biomass ratio
///
/// # Returns
/// New FishStock instance
pub fn create_fish_stock(
    region_id: String,
    max_biomass: f64,
    regeneration_rate: f64,
    config: &FishingConfig,
) -> FishStock {
    let mut species_distribution = BTreeMap::new();
    species_distribution.insert("Karp".to_string(), 0.3);
    species_distribution.insert("Sandacz".to_string(), 0.2);
    species_distribution.insert("Szczupak".to_string(), 0.15);
    species_distribution.insert("Leszcz".to_string(), 0.2);
    species_distribution.insert("Inne".to_string(), 0.15);

    FishStock {
        id: format!("FishStock-{}", region_id),
        region_id,
        total_biomass: max_biomass * config.initial_biomass_ratio,
        health: 1.0,
        regeneration_rate,
        max_biomass,
        species_distribution,
    }
}

/// Create a new fish farm.
///
/// # Arguments
/// * region_id - Region where farm is located
/// * owner - Owner of the farm
/// * farm_type - Type of fish farm
/// * production_capacity - Production capacity
///
/// # Returns
/// New FishFarm instance
pub fn create_fish_farm(
    region_id: String,
    owner: String,
    farm_type: FishFarmType,
    production_capacity: f64,
) -> FishFarm {
    let feed_cost = match farm_type {
        FishFarmType::Freshwater => 500.0,
        FishFarmType::Saltwater => 800.0,
        FishFarmType::Shellfish => 600.0,
        FishFarmType::Mixed => 700.0,
    };

    FishFarm {
        id: format!("FishFarm-{}-{}", region_id, farm_type as u8),
        region_id,
        owner,
        farm_type,
        production_capacity,
        current_production: 0.0,
        operating_cost: production_capacity * 0.1,
        feed_cost,
        water_quality: 1.0,
        disease_risk: 0.0,
    }
}

/// Process the fishing turn for all fish stocks, farms, and policies.
///
/// # Arguments
/// * fish_stocks - Mutable slice of all fish stocks (regenerated and fished).
/// * fish_farms - Mutable slice of all fish farms (production processed).
/// * fishing_policies - Slice of fishing policies (applied to stocks).
/// * order_book - Mutable order book (fish harvest sell asks submitted).
/// * config - Fishing configuration.
/// * market_history - Market history for reference prices.
/// * turn - Current turn number.
///
/// # Returns
/// Total fish harvest (catch + farm production) in tons.
///
/// # Rules
/// * Fish stocks regenerate based on health and regeneration rate.
/// * Fishing policies determine allowable catch per stock.
/// * Overfishing degrades stock health (configurable).
/// * Fish farms produce deterministically based on water quality and disease risk.
/// * All fish harvest is submitted as Sell Asks to the OrderBook.
pub fn process_fishing_turn(
    fish_stocks: &mut [FishStock],
    fish_farms: &mut [FishFarm],
    fishing_policies: &[FishingPolicy],
    order_book: &mut OrderBook,
    config: &FishingConfig,
    _market_history: &MarketHistory,
    turn: u32,
) -> f64 {
    let mut total_harvest = 0.0;

    // Process fish stocks: regenerate and apply fishing
    for stock in fish_stocks.iter_mut() {
        // Regenerate biomass
        stock.process_regeneration();

        // Find applicable policy for this stock's region
        let policy = fishing_policies.iter().find(|p| {
            p.min_stock_health <= stock.health
        });

        let catch_amount = if let Some(p) = policy {
            if p.is_fishing_allowed(stock.health) {
                p.allowable_catch(stock.sustainable_catch())
            } else {
                0.0
            }
        } else {
            // No policy: allow sustainable catch only
            stock.sustainable_catch()
        };

        if catch_amount > 0.0 {
            stock.apply_catch(catch_amount, config);
            total_harvest += catch_amount;
        }
    }

    // Process fish farms
    for farm in fish_farms.iter_mut() {
        let production = farm.process_turn(config, turn);
        total_harvest += production;
    }

    // Submit all fish harvest as Sell Asks
    if total_harvest > 0.0 {
        let ref_price = _market_history
            .vwap_per_commodity
            .get(&Commodity::Fish)
            .copied()
            .unwrap_or(100.0);

        order_book.asks
            .entry(Commodity::Fish)
            .or_insert_with(Vec::new)
            .push(Ask {
                seller_id: "fishing_sector".to_string(),
                commodity: Commodity::Fish,
                quantity: total_harvest,
                limit_price: ref_price,
                blueprint_id: None,
                quality: None,
                durability: None,
            });
    }

    total_harvest
}
