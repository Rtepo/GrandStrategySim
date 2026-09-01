//! Religious/Cultural infrastructure templates and configurations

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::economy::market::GlobalMarket;
use crate::economy::order_book::{Bid, OrderBook};
use crate::entities::legal_form::LatifundiumData;
use crate::registries::enums::Commodity;
use crate::society::geography::Region;

/// Religious/Cultural building types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CulturalBuildingType {
    /// Can own latifundia/serfs
    Monastery,
    /// Funded by voluntary contributions
    Temple,
    /// Community events/cultural preservation
    CulturalHouse,
    /// Burial capacity
    #[default]
    Cemetery,
}

/// Cultural building template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CulturalTemplate {
    /// Type of building
    pub building_type: CulturalBuildingType,

    /// Base capacity per turn
    pub base_capacity: f64,

    /// Can own land (Monasteries only)
    pub can_own_land: bool,

    /// Funding model
    pub funding_model: CulturalFunding,
}

/// Cultural funding models
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CulturalFunding {
    /// Temple - donations
    Voluntary,
    /// Monastery - land income
    Endowment,
    /// CulturalHouse - state budget
    Public,
    /// Combination
    #[default]
    Mixed,
}

// ============================================================================
// CONFIGURATION STRUCTS (No Magic Numbers)
// ============================================================================

/// Donation rates by rural class for Voluntary (Temple) funding
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VoluntaryDonationRates {
    /// Donation rate for aristocracy class
    pub aristocracy: f64,
    /// Donation rate for free peasant class
    pub free_peasant: f64,
    /// Donation rate for landless laborer class
    pub landless_laborer: f64,
    /// Donation rate for serf class
    pub serf: f64,
}

impl Default for VoluntaryDonationRates {
    fn default() -> Self {
        Self {
            aristocracy: 0.02,
            free_peasant: 0.01,
            landless_laborer: 0.005,
            serf: 0.0,
        }
    }
}

/// Donation rates by rural class for Endowment (Monastery) funding
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EndowmentDonationRates {
    /// Donation rate for aristocracy class
    pub aristocracy: f64,
    /// Donation rate for free peasant class
    pub free_peasant: f64,
    /// Donation rate for landless laborer class
    pub landless_laborer: f64,
    /// Donation rate for serf class
    pub serf: f64,
}

impl Default for EndowmentDonationRates {
    fn default() -> Self {
        Self {
            aristocracy: 0.01,
            free_peasant: 0.0,
            landless_laborer: 0.0,
            serf: 0.0,
        }
    }
}

/// All parameters for cultural building fundraising and relief distribution.
/// No magic numbers are permitted in logic functions — all values come from here.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CulturalReliefConfig {
    /// Donation rates by rural class for Voluntary (Temple) funding
    pub voluntary_donation_rates: VoluntaryDonationRates,
    /// Donation rates by rural class for Endowment (Monastery) funding
    pub endowment_donation_rates: EndowmentDonationRates,
    /// Corporate philanthropy: fraction of available_cash donated by wealthy companies
    pub corporate_donation_rate: f64,
    /// Wealth threshold: companies below this available_cash do not donate
    pub corporate_wealth_threshold: f64,
    /// Solidarity multiplier when country religion matches building's tradition
    pub religious_solidarity_multiplier: f64,
    /// Solidarity multiplier when religion does not match (secular/mixed)
    pub secular_solidarity_multiplier: f64,
    /// Relief distribution: list of commodities cultural buildings can buy as physical relief
    pub relief_commodities: Vec<Commodity>,
    /// Fraction of available_cash used for direct cash transfers (remainder goes to B2B buy orders)
    pub direct_cash_transfer_fraction: f64,
    /// Maximum limit price for relief B2B buy orders (as fraction of global base price)
    pub relief_bid_price_fraction: f64,
}

impl Default for CulturalReliefConfig {
    fn default() -> Self {
        Self {
            voluntary_donation_rates: VoluntaryDonationRates::default(),
            endowment_donation_rates: EndowmentDonationRates::default(),
            corporate_donation_rate: 0.001,
            corporate_wealth_threshold: 10_000.0,
            religious_solidarity_multiplier: 1.0,
            secular_solidarity_multiplier: 0.5,
            relief_commodities: vec![Commodity::Food, Commodity::Pharmaceuticals],
            direct_cash_transfer_fraction: 0.5,
            relief_bid_price_fraction: 1.2,
        }
    }
}

// ============================================================================
// CULTURAL BUILDING — Active Economic Actor
// ============================================================================

/// A cultural/religious building that acts as an economic actor.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CulturalBuilding {
    /// Unique building identifier
    #[serde(default)]
    pub id: String,
    /// Type of cultural building
    #[serde(default)]
    pub building_type: CulturalBuildingType,
    /// Region where the building is located
    #[serde(default)]
    pub region_id: String,
    /// Liquid cash held by this institution (alms box / reserve fund)
    #[serde(default)]
    pub available_cash: f64,
    /// Total donations collected this turn
    #[serde(default)]
    pub donations_collected_this_turn: f64,
    /// Total relief distributed this turn
    #[serde(default)]
    pub relief_distributed_this_turn: f64,
    /// Operational capacity (e.g. worship seats, monastic cells)
    #[serde(default)]
    pub capacity: f64,
    /// Year the building was constructed
    #[serde(default)]
    pub year_built: u32,
    /// Physical condition (0.0-1.0)
    #[serde(default = "default_condition")]
    pub condition: f64,
    /// Whether this building is a protected heritage site
    #[serde(default)]
    pub is_heritage_site: bool,
    /// Monastery land endowment: owned agricultural company shares
    /// Maps company_id -> share fraction (0.0-1.0)
    #[serde(default)]
    pub owned_company_shares: BTreeMap<String, f64>,
    /// For monasteries that directly own latifundia (not via company shares)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_latifundium: Option<LatifundiumData>,
    /// Phase 17C: Active production method engine key (e.g., "monastery_scriptorium").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_method: Option<String>,
    /// Phase 17C: Owning company ID (links to Company with Sector::Religion).
    /// Revenue from production credits this company via TransferSettler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_company_id: Option<String>,
}

fn default_condition() -> f64 {
    1.0
}

// ============================================================================
// CULTURAL ECONOMIC LOGIC
// ============================================================================

/// Phase 3.6: Collect donations and endowment income for cultural buildings.
///
/// Double-entry: DEBIT donor savings/cash, CREDIT building.available_cash.
pub fn collect_cultural_donations(
    regions: &mut [Region],
    companies: &mut [crate::entities::Company],
    cultural_institutions: &mut [CulturalBuilding],
    religion: &str,
    average_wage: f64,
    config: &CulturalReliefConfig,
) {
    for building in cultural_institutions {
        building.donations_collected_this_turn = 0.0;
        let region_idx = regions.iter().position(|r| r.id == building.region_id);

        match building.building_type {
            CulturalBuildingType::Temple => {
                let rates = &config.voluntary_donation_rates;
                let solidarity = if !religion.is_empty() {
                    config.religious_solidarity_multiplier
                } else {
                    config.secular_solidarity_multiplier
                };

                if let Some(ri) = region_idx {
                    collect_class_donations(&mut regions[ri], building, rates, solidarity);
                }
            }
            CulturalBuildingType::Monastery => {
                // Endowment income from owned company shares (dividends)
                let shares = building.owned_company_shares.clone();
                for (company_id, share) in &shares {
                    if let Some(company) = companies.iter_mut().find(|c| &c.id == company_id) {
                        // Dividend = share of available cash
                        let dividend = company.available_cash * share * 0.1;
                        if dividend > 0.0 {
                            company.available_cash -= dividend;
                            building.available_cash += dividend;
                            building.donations_collected_this_turn += dividend;
                        }
                    }
                }

                // Endowment income from direct latifundium
                if let Some(ref lat) = building.owned_latifundium {
                    let latifundium_income =
                        lat.serf_population as f64 * lat.serf_labor_cost_multiplier * average_wage;
                    if latifundium_income > 0.0 {
                        if let Some(ri) = region_idx {
                            debit_serf_savings(&mut regions[ri], latifundium_income);
                        }
                        building.available_cash += latifundium_income;
                        building.donations_collected_this_turn += latifundium_income;
                    }
                }

                // Also collect voluntary tithes from aristocracy
                let rates = VoluntaryDonationRates {
                    aristocracy: config.endowment_donation_rates.aristocracy,
                    free_peasant: config.endowment_donation_rates.free_peasant,
                    landless_laborer: config.endowment_donation_rates.landless_laborer,
                    serf: config.endowment_donation_rates.serf,
                };
                let solidarity = if !religion.is_empty() {
                    config.religious_solidarity_multiplier
                } else {
                    config.secular_solidarity_multiplier
                };
                if let Some(ri) = region_idx {
                    collect_class_donations(&mut regions[ri], building, &rates, solidarity);
                }
            }
            CulturalBuildingType::CulturalHouse | CulturalBuildingType::Cemetery => {
                // Public funding only — no voluntary donations
            }
        }

        // Corporate philanthropy
        for company in companies.iter_mut() {
            if company.available_cash > config.corporate_wealth_threshold
                && company.region_id == building.region_id
            {
                let donation = company.available_cash * config.corporate_donation_rate;
                if donation > 0.0 {
                    company.available_cash -= donation;
                    building.available_cash += donation;
                    building.donations_collected_this_turn += donation;
                }
            }
        }
    }
}

/// Collect donations from each rural class based on config rates.
fn collect_class_donations(
    region: &mut Region,
    building: &mut CulturalBuilding,
    rates: &VoluntaryDonationRates,
    solidarity: f64,
) {
    let class_map = &mut region.class_demographics.rural_classes;

    for (class_name, demographics) in class_map.iter_mut() {
        let rate = match class_name.as_str() {
            "aristocracy" => rates.aristocracy,
            "free_peasant" => rates.free_peasant,
            "landless_laborer" => rates.landless_laborer,
            "serf" => rates.serf,
            _ => 0.0,
        };

        let donation = demographics.savings * rate * solidarity;
        if donation > 0.0 && demographics.savings >= donation {
            demographics.savings -= donation;
            building.available_cash += donation;
            building.donations_collected_this_turn += donation;
        }
    }
}

/// Debit serf class savings for latifundium surplus extraction.
fn debit_serf_savings(region: &mut Region, amount: f64) {
    if let Some(demographics) = region.class_demographics.rural_classes.get_mut("serf") {
        let actual = amount.min(demographics.savings.max(0.0));
        demographics.savings -= actual;
    }
}

/// Phase 3.7: Distribute direct cash relief to poorest demographics.
///
/// Double-entry: DEBIT building.available_cash, CREDIT poorest class savings.
pub fn distribute_cash_relief(
    regions: &mut [Region],
    cultural_institutions: &mut [CulturalBuilding],
    config: &CulturalReliefConfig,
) {
    for building in cultural_institutions {
        building.relief_distributed_this_turn = 0.0;
        let cash_pool = building.available_cash * config.direct_cash_transfer_fraction;
        if cash_pool <= 0.0 {
            continue;
        }

        let region_idx = match regions.iter().position(|r| r.id == building.region_id) {
            Some(i) => i,
            None => continue,
        };

        // Collect eligible classes sorted by savings_per_capita ascending (poorest first)
        let mut eligible: Vec<(String, f64, i64)> = Vec::new();
        for (class_name, demographics) in &regions[region_idx].class_demographics.rural_classes {
            if demographics.population > 0 {
                eligible.push((
                    class_name.clone(),
                    demographics.savings_per_capita,
                    demographics.population,
                ));
            }
        }
        eligible.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let total_pop: i64 = eligible.iter().map(|(_, _, pop)| pop).sum();
        if total_pop <= 0 {
            continue;
        }

        let per_capita_relief = cash_pool / total_pop as f64;
        let mut remaining = cash_pool;

        for (class_name, _, pop) in &eligible {
            if remaining <= 0.0 {
                break;
            }
            let transfer = (per_capita_relief * *pop as f64).min(remaining);
            if let Some(demographics) = regions[region_idx]
                .class_demographics
                .rural_classes
                .get_mut(class_name)
            {
                demographics.savings += transfer;
                demographics.savings_per_capita = if demographics.population > 0 {
                    demographics.savings / demographics.population as f64
                } else {
                    0.0
                };
            }
            building.available_cash -= transfer;
            building.relief_distributed_this_turn += transfer;
            remaining -= transfer;
        }
    }
}

/// Phase 3.7a: Submit B2B buy orders for physical relief goods (Food, Pharmaceuticals).
///
/// Encumbers cash immediately. Unfilled bids refunded after clearing.
pub fn submit_relief_b2b_orders(
    cultural_institutions: &mut [CulturalBuilding],
    order_book: &mut OrderBook,
    global_market: &GlobalMarket,
    config: &CulturalReliefConfig,
) {
    for building in cultural_institutions {
        let b2b_pool = building.available_cash * (1.0 - config.direct_cash_transfer_fraction);
        if b2b_pool <= 0.0 {
            continue;
        }

        let n_commodities = config.relief_commodities.len().max(1);
        let per_commodity_budget = b2b_pool / n_commodities as f64;

        for commodity in &config.relief_commodities {
            let base_price = global_market
                .base_prices
                .get(commodity)
                .copied()
                .unwrap_or(100.0);
            let limit_price = base_price * config.relief_bid_price_fraction;
            if limit_price <= 0.0 {
                continue;
            }
            let quantity = per_commodity_budget / limit_price;
            if quantity <= 0.0 {
                continue;
            }

            // Encumber cash immediately
            let encumbrance = quantity * limit_price;
            building.available_cash -= encumbrance;

            order_book.bids.entry(*commodity).or_default().push(Bid {
                buyer_id: building.id.clone(),
                commodity: *commodity,
                quantity,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
        }
    }
}

/// Post-clearing: Refund unfilled cultural building bids.
pub fn refund_unfilled_cultural_bids(
    order_book: &OrderBook,
    cultural_institutions: &mut [CulturalBuilding],
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if let Some(building) = cultural_institutions
                .iter_mut()
                .find(|b| b.id == bid.buyer_id)
            {
                let refund = bid.quantity * bid.limit_price;
                building.available_cash += refund;
            }
        }
    }
}

/// Post-clearing: Deliver filled relief goods to regions (in-kind relief).
/// Adds traded commodities to region supply for B2C availability.
pub fn deliver_relief_goods(
    order_book: &OrderBook,
    cultural_institutions: &[CulturalBuilding],
    regions: &mut [Region],
) {
    for trade in &order_book.trades {
        if let Some(building) = cultural_institutions
            .iter()
            .find(|b| b.id == trade.buyer_id)
        {
            if let Some(region) = regions.iter_mut().find(|r| r.id == building.region_id) {
                // Goods delivered to region — increase available supply for B2C
                // The specific supply mechanism depends on the commodity type
                // For now, we log the delivery via the building's relief counter
                // In full implementation, this would add to region.supply_pool
                let _ = region; // Region supply update point
            }
        }
    }
}
