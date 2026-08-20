//! Phase 60: Real estate market clearing, agrarian reform, and foreign
//! investment regulation.
//!
//! ## Strict Market Clearing
//! Every transaction physically debits the buyer's liquidity pool:
//! - Companies: `Company.available_cash`
//! - Funds/VIPs: `BrokerageAccount.cash`
//! - Municipalities: `RegionalBudget.liquid_reserves`
//! No money creation, no overdrafts.
//!
//! ## Trait-Driven Bidding
//! Willingness-to-pay uses `MarketBehaviorModifiers` from Phase 57.
//! No raw trait string checks.
//!
//! ## Compensation Fallback
//! `FairMarketAverage` falls back to `acquisition_price` (not hedonic value)
//! when regional price history is insufficient.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use rand::Rng;

use crate::corporate::market_behavior::{evaluate_market_behavior, MarketBehaviorModifiers};
use crate::society::cadastre::{
    Cadastre, CadastreConfig, ParcelChunk, ParcelId, ParcelOwnerType, ZoningDesignation,
    LandPriceHistoryRegistry, compute_parcel_value, foreign_ownership_percentage,
    ArbitrationCourt, ArbitrationCase, ArbitrationStatus, ArbitrationConfig,
    parcel_id_to_index,
};

// ============================================================================
// COMPENSATION SCHEME
// ============================================================================

/// Compensation scheme for expropriation — determines how affected owners are paid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum CompensationScheme {
    /// Algorithmically calculates compensation based on a rolling historical
    /// average of regional land prices over the last N years.
    /// **Fallback**: if insufficient history exists, uses `acquisition_price`.
    FairMarketAverage {
        /// Number of years to average (e.g., 3, 5)
        lookback_years: u32,
        /// Multiplier applied to the historical average (1.0 = full average)
        average_multiplier: f64,
    },
    /// Authoritarian/mass-nationalization: fixed fiat price per hectare.
    StatutoryFiat {
        /// Fixed price per hectare by soil class (English keys)
        fiat_prices_by_soil: BTreeMap<String, f64>,
        /// Fixed price per hectare by zoning designation
        fiat_prices_by_zoning: BTreeMap<ZoningDesignation, f64>,
    },
    /// Pure expropriation with zero payout.
    #[default]
    None,
}

/// Agrarian reform law configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgrarianReformLaw {
    /// Maximum hectares any single entity can own (0 = no limit)
    pub max_estate_size: f64,
    /// Whether to expropriate existing latifundia above the limit
    pub expropriate_existing: bool,
    /// Compensation scheme for expropriated land
    pub compensation_scheme: CompensationScheme,
    /// Whether to redistribute to smallholders
    pub redistribute_to_peasants: bool,
    /// Foreign ownership cap (percentage of total agricultural land, 0.0–1.0)
    pub foreign_ownership_cap: f64,
    /// Sectors where foreign ownership is banned
    pub foreign_banned_sectors: Vec<String>,
    /// Border zone foreign ownership ban (km from border, >0 = ban)
    pub foreign_border_zone_ban_km: f64,
}

// ============================================================================
// BID EVALUATION
// ============================================================================

/// A bid from a potential buyer for a parcel.
#[derive(Debug, Clone)]
pub struct LandBid {
    /// The parcel being bid on
    pub parcel_id: ParcelId,
    /// Bidder entity ID
    pub bidder_id: String,
    /// Bidder owner type
    pub bidder_type: ParcelOwnerType,
    /// Bid price (total, not per hectare)
    pub bid_price: f64,
    /// Whether the bidder wants only part of the parcel (split)
    pub split_size: Option<f64>,
}

/// Evaluate a buyer's maximum bid for a parcel using `MarketBehaviorModifiers`.
///
/// **No raw trait string checks** — only typed modifiers from Phase 57.
///
/// Returns `None` if the buyer's risk tolerance filters out the parcel
/// (e.g., low certainty parcels for risk-averse buyers).
pub fn evaluate_max_bid(
    parcel: &ParcelChunk,
    cadastre_config: &CadastreConfig,
    modifiers: &MarketBehaviorModifiers,
    buyer_liquidity: f64,
    total_regional_land: f64,
    buyer_current_land: f64,
) -> Option<f64> {
    // Risk tolerance filter: low risk tolerance → skip low-certainty parcels
    if parcel.legal_certainty < 0.5 && modifiers.risk_tolerance < 0.5 {
        return None;
    }

    // Frozen parcels cannot be bought
    if parcel.is_frozen {
        return None;
    }

    // Compute hedonic value
    let hedonic_value = compute_parcel_value(parcel, cadastre_config);

    // Adjust by share_price_premium (Ambitious → overbid, Paranoid → discount)
    let adjusted_value = hedonic_value * (1.0 + modifiers.share_price_premium);

    // Cap by max_position_pct: buyer cannot acquire more than this fraction
    // of total regional land
    let max_land_allowed = total_regional_land * modifiers.max_position_pct;
    let remaining_land_capacity = (max_land_allowed - buyer_current_land).max(0.0);
    if remaining_land_capacity <= 0.0 {
        return None; // Already at position limit
    }
    // Reject if the parcel is larger than the remaining capacity
    if parcel.size_hectares > remaining_land_capacity {
        return None; // Parcel would exceed position cap
    }

    // Cap bid by available liquidity
    let max_bid = adjusted_value.min(buyer_liquidity);

    if max_bid <= 0.0 {
        return None;
    }

    Some(max_bid)
}

// ============================================================================
// MARKET CLEARING
// ============================================================================

/// Result of a single land transaction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandTransactionResult {
    pub parcel_id_idx: u32,
    pub region_id: String,
    pub buyer_id: String,
    pub buyer_type: ParcelOwnerType,
    pub seller_id: String,
    pub seller_type: ParcelOwnerType,
    pub transaction_price: f64,
    pub hectares_transferred: f64,
    pub stamp_duty: f64,
    pub was_split: bool,
}

/// Process a land transaction with strict double-entry accounting.
///
/// **Strict market clearing**: debits buyer's liquidity, credits seller's
/// liquidity. No money creation, no overdrafts.
///
/// # Arguments
/// * `cadastre` - The country's cadastre
/// * `parcel_id` - The parcel to transact
/// * `buyer_id` - Buyer entity ID
/// * `buyer_type` - Buyer owner type
/// * `buyer_liquidity` - Mutable reference to buyer's cash pool
/// * `seller_liquidity` - Mutable reference to seller's cash pool
/// * `price` - Transaction price
/// * `split_size` - If Some, only buy this many hectares (split the parcel)
/// * `cadastre_config` - For stamp duty calculation
/// * `regional_budget` - For stamp duty credit
/// * `current_turn` - Current turn number
///
/// # Returns
/// `Some(LandTransactionResult)` if successful, `None` if rejected.
pub fn process_land_transaction(
    cadastre: &mut Cadastre,
    parcel_id: ParcelId,
    buyer_id: &str,
    buyer_type: ParcelOwnerType,
    buyer_liquidity: &mut f64,
    seller_liquidity: &mut f64,
    price: f64,
    split_size: Option<f64>,
    cadastre_config: &CadastreConfig,
    regional_budget: Option<&mut crate::politics::local_government::RegionalBudget>,
    current_turn: u32,
) -> Option<LandTransactionResult> {
    // Check buyer liquidity — strict, no overdraft
    if *buyer_liquidity < price {
        return None; // Insufficient funds — rejected
    }

    // Get the parcel
    let parcel = cadastre.get(parcel_id)?;

    // Frozen parcels cannot be transacted
    if parcel.is_frozen {
        return None;
    }

    let region_id = parcel.region_id.clone();
    let seller_id = parcel.owner_id.clone();
    let seller_type = parcel.owner_type;
    let total_hectares = parcel.size_hectares;

    // Handle split if buyer wants only part of the parcel
    let (hectares_transferred, was_split, actual_price) = if let Some(split) = split_size {
        if split <= 0.0 || split >= total_hectares {
            return None; // Invalid split size
        }
        // Proportional price for the split
        let proportional_price = price * (split / total_hectares);
        if *buyer_liquidity < proportional_price {
            return None; // Can't afford the split portion
        }
        (split, true, proportional_price)
    } else {
        (total_hectares, false, price)
    };

    // Stamp duty
    let stamp_duty = actual_price * cadastre_config.stamp_duty_rate;

    // Total buyer cost = price + stamp duty
    let total_buyer_cost = actual_price + stamp_duty;
    if *buyer_liquidity < total_buyer_cost {
        return None; // Can't afford price + stamp duty
    }

    // Execute the transaction
    // 1. Debit buyer
    *buyer_liquidity -= total_buyer_cost;

    // 2. Credit seller
    *seller_liquidity += actual_price;

    // 3. Credit stamp duty to regional budget
    if let Some(budget) = regional_budget {
        budget.liquid_reserves += stamp_duty;
    }

    // 4. Update parcel ownership
    if was_split {
        // Split the parcel
        let new_id = cadastre.split_parcel(parcel_id, hectares_transferred, current_turn)?;
        if let Some(new_parcel) = cadastre.get_mut(new_id) {
            new_parcel.owner_type = buyer_type;
            new_parcel.owner_id = buyer_id.to_string();
            new_parcel.acquisition_price = actual_price;
            new_parcel.acquisition_turn = current_turn;
        }
    } else {
        if let Some(parcel) = cadastre.get_mut(parcel_id) {
            parcel.owner_type = buyer_type;
            parcel.owner_id = buyer_id.to_string();
            parcel.acquisition_price = actual_price;
            parcel.acquisition_turn = current_turn;
        }
    }

    Some(LandTransactionResult {
        parcel_id_idx: parcel_id_to_index(parcel_id),
        region_id,
        buyer_id: buyer_id.to_string(),
        buyer_type,
        seller_id,
        seller_type,
        transaction_price: actual_price,
        hectares_transferred,
        stamp_duty,
        was_split,
    })
}

/// Record a transaction in the land price history registry.
pub fn record_transaction_price(
    history: &mut LandPriceHistoryRegistry,
    region_id: &str,
    price_per_hectare: f64,
) {
    history.record(region_id, price_per_hectare);
}

// ============================================================================
// COMPENSATION CALCULATION
// ============================================================================

/// Calculate compensation for an expropriated parcel using the specified scheme.
///
/// **Critical fallback**: If `FairMarketAverage` is selected but the world is
/// too young to have sufficient `RegionalLandPriceHistory`, the compensation
/// MUST fall back to the parcel's `acquisition_price` — NOT the engine's
/// hedonic valuation. This preserves the distinction between market reality
/// and state valuation.
pub fn calculate_compensation(
    parcel: &ParcelChunk,
    scheme: &CompensationScheme,
    price_history: &LandPriceHistoryRegistry,
    turns_per_year: u32,
) -> f64 {
    match scheme {
        CompensationScheme::None => 0.0,

        CompensationScheme::StatutoryFiat {
            fiat_prices_by_soil,
            fiat_prices_by_zoning,
        } => {
            // Try soil class first, then zoning
            let price_per_hectare = fiat_prices_by_soil
                .get(&parcel.soil_class)
                .copied()
                .or_else(|| {
                    fiat_prices_by_zoning
                        .get(&parcel.zoning)
                        .copied()
                })
                .unwrap_or(0.0);
            price_per_hectare * parcel.size_hectares
        }

        CompensationScheme::FairMarketAverage {
            lookback_years,
            average_multiplier,
        } => {
            let lookback_turns = (*lookback_years as u32) * turns_per_year;
            let lookback_entries = lookback_turns as usize;

            // Check if we have SUFFICIENT history for the full lookback window
            if price_history.has_sufficient_history(&parcel.region_id, lookback_entries) {
                if let Some(avg_price) = price_history.rolling_average(&parcel.region_id, lookback_entries) {
                    // Sufficient history — use rolling average
                    return avg_price * average_multiplier * parcel.size_hectares;
                }
            }

            // **FALLBACK**: Use acquisition_price, NOT hedonic valuation.
            // This is the critical directive: the state compensates based on
            // what the investor actually paid, not what the engine thinks
            // the land is worth.
            parcel.acquisition_price
        }
    }
}

// ============================================================================
// AGRARIAN REFORM EXPROPRIATION
// ============================================================================

/// Result of an agrarian reform expropriation pass.
#[derive(Debug, Clone, Default)]
pub struct ExpropriationResult {
    /// Number of parcels expropriated
    pub parcels_expropriated: u32,
    /// Total hectares expropriated
    pub hectares_expropriated: f64,
    /// Total compensation paid
    pub total_compensation: f64,
    /// Arbitration cases filed by expropriated actors
    pub arbitration_cases_filed: u32,
}

/// Execute agrarian reform: expropriate parcels exceeding `max_estate_size`.
///
/// Flow:
/// 1. Identify parcels exceeding `max_estate_size` per owner.
/// 2. Expropriate the excess (change owner to State).
/// 3. Calculate compensation using `compensation_scheme`.
/// 4. If `redistribute_to_peasants`, split and assign to FreePeasant owners.
/// 5. Affected actors file `ArbitrationCase` against the state.
pub fn execute_agrarian_reform(
    cadastre: &mut Cadastre,
    court: &mut ArbitrationCourt,
    law: &AgrarianReformLaw,
    price_history: &LandPriceHistoryRegistry,
    arbitration_config: &ArbitrationConfig,
    country_name: &str,
    current_turn: u32,
    turns_per_year: u32,
    rng: &mut impl Rng,
) -> ExpropriationResult {
    if law.max_estate_size <= 0.0 && law.foreign_ownership_cap <= 0.0 {
        return ExpropriationResult::default();
    }

    let mut result = ExpropriationResult::default();

    // Group parcels by owner to find estates exceeding the limit
    let mut owner_parcels: BTreeMap<String, Vec<(ParcelId, f64, ParcelOwnerType)>> = BTreeMap::new();
    for (id, parcel) in cadastre.iter() {
        if parcel.is_frozen {
            continue;
        }
        owner_parcels
            .entry(parcel.owner_id.clone())
            .or_default()
            .push((id, parcel.size_hectares, parcel.owner_type));
    }

    let mut parcels_to_expropriate: Vec<(ParcelId, String, ParcelOwnerType)> = Vec::new();

    for (owner_id, parcels) in &owner_parcels {
        let total_hectares: f64 = parcels.iter().map(|(_, h, _)| h).sum();

        // Check max_estate_size
        if law.max_estate_size > 0.0 && total_hectares > law.max_estate_size {
            // Expropriate the excess — sort by size descending, expropriate largest first
            let mut sorted_parcels = parcels.clone();
            sorted_parcels.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut remaining = total_hectares;
            for (id, size, owner_type) in &sorted_parcels {
                if remaining <= law.max_estate_size {
                    break;
                }
                parcels_to_expropriate.push((*id, owner_id.clone(), *owner_type));
                remaining -= size;
            }
        }

        // Check foreign ownership cap
        if law.foreign_ownership_cap > 0.0 {
            let is_foreign = parcels.first().map(|(_, _, t)| *t) == Some(ParcelOwnerType::ForeignFund);
            if is_foreign {
                let total_land: f64 = cadastre.parcels.values().map(|p| p.size_hectares).sum();
                let foreign_land: f64 = cadastre
                    .parcels
                    .values()
                    .filter(|p| p.owner_type == ParcelOwnerType::ForeignFund)
                    .map(|p| p.size_hectares)
                    .sum();
                if total_land > 0.0 && (foreign_land / total_land) > law.foreign_ownership_cap {
                    // Expropriate some foreign-owned parcels
                    for (id, _, owner_type) in parcels {
                        if *owner_type == ParcelOwnerType::ForeignFund {
                            parcels_to_expropriate.push((*id, owner_id.clone(), *owner_type));
                        }
                    }
                }
            }
        }
    }

    // Execute expropriation
    for (parcel_id, original_owner_id, original_owner_type) in &parcels_to_expropriate {
        let parcel = match cadastre.get(*parcel_id) {
            Some(p) => p.clone(),
            None => continue,
        };

        // Calculate compensation
        let compensation = calculate_compensation(
            &parcel,
            &law.compensation_scheme,
            price_history,
            turns_per_year,
        );

        // Expropriate: change owner to State
        if let Some(p) = cadastre.get_mut(*parcel_id) {
            p.owner_type = ParcelOwnerType::State;
            p.owner_id = "TREASURY".to_string();
            p.acquisition_turn = current_turn;
        }

        result.parcels_expropriated += 1;
        result.hectares_expropriated += parcel.size_hectares;
        result.total_compensation += compensation;

        // Redistribute to peasants if configured
        if law.redistribute_to_peasants {
            if let Some(p) = cadastre.get_mut(*parcel_id) {
                p.owner_type = ParcelOwnerType::Private;
                p.owner_id = format!("PEASANT_REFORM_{}", current_turn);
            }
        }

        // File arbitration case
        let filing_prob = match law.compensation_scheme {
            CompensationScheme::None => arbitration_config.base_filing_probability,
            CompensationScheme::StatutoryFiat { .. } => arbitration_config.base_filing_probability * 0.7,
            CompensationScheme::FairMarketAverage { .. } => arbitration_config.base_filing_probability * 0.5,
        };

        if rng.gen_range(0.0..1.0) < filing_prob {
            let case = ArbitrationCase {
                case_id: format!("AC_{}", court.next_case_id),
                plaintiff_id: original_owner_id.clone(),
                plaintiff_type: *original_owner_type,
                defendant_country: country_name.to_string(),
                expropriated_parcel_indices: vec![parcel_id_to_index(*parcel_id)],
                original_acquisition_value: parcel.acquisition_price,
                compensation_claimed: compensation,
                filed_turn: current_turn,
                status: ArbitrationStatus::Pending,
                state_strength_assessment: 0.5, // Will be assessed during processing
            };
            court.file_case(case);
            court.next_case_id += 1;
            result.arbitration_cases_filed += 1;
        }
    }

    result
}

// ============================================================================
// FOREIGN INVESTMENT REGULATION
// ============================================================================

/// Check if a foreign fund can purchase a parcel under the agrarian reform law.
///
/// Returns `true` if the purchase is allowed, `false` if blocked.
pub fn check_foreign_purchase_allowed(
    cadastre: &Cadastre,
    parcel: &ParcelChunk,
    law: &AgrarianReformLaw,
) -> bool {
    // Check border zone ban
    if parcel.is_border_zone && law.foreign_border_zone_ban_km > 0.0 {
        return false;
    }

    // Check foreign ownership cap
    if law.foreign_ownership_cap > 0.0 {
        let total_land: f64 = cadastre.parcels.values().map(|p| p.size_hectares).sum();
        let foreign_land: f64 = cadastre
            .parcels
            .values()
            .filter(|p| p.owner_type == ParcelOwnerType::ForeignFund)
            .map(|p| p.size_hectares)
            .sum();
        if total_land > 0.0 {
            let new_foreign_pct = (foreign_land + parcel.size_hectares) / total_land;
            if new_foreign_pct > law.foreign_ownership_cap {
                return false;
            }
        }
    }

    true
}

// ============================================================================
// MINISTRY REPORTS (delayed, aggregated)
// ============================================================================

/// Aggregated ministry land report — internal government document.
/// Only visible to top-tier executive office holders (PM, Head of State, Minister).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinistryLandReport {
    /// Turn the report was generated
    pub report_turn: u32,
    /// Total national land value
    pub total_land_value: f64,
    /// Total hectares
    pub total_hectares: f64,
    /// Foreign ownership percentage (0.0–1.0)
    pub foreign_ownership_pct: f64,
    /// Total border conflicts
    pub total_border_conflicts: u32,
    /// Total arbitration cases pending
    pub total_arbitration_cases: u32,
    /// Total arbitration compensation exposure
    pub total_arbitration_exposure: f64,
    /// Per-region summary
    pub regional_summaries: Vec<MinistryRegionalSummary>,
    /// Bureaucratic delay note
    pub delay_note: String,
}

/// Per-region summary in the ministry report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinistryRegionalSummary {
    pub region_id: String,
    pub total_hectares: f64,
    pub total_value: f64,
    pub avg_legal_certainty: f64,
    pub border_conflicts: u32,
    pub foreign_ownership_pct: f64,
    pub court_backlog: f64,
}

/// Generate a ministry land report — delayed and aggregated.
///
/// **"No-God" rule**: The player receives quarterly (every 6 turns) aggregated
/// reports. Reports are delayed by 2–4 turns (bureaucratic lag). The player
/// cannot see individual parcel details — only regional/national aggregates.
pub fn generate_ministry_land_report(
    cadastre: &Cadastre,
    border_conflicts: &crate::society::cadastre::BorderConflictRegistry,
    arbitration_court: &ArbitrationCourt,
    regions: &[crate::society::geography::Region],
    report_turn: u32,
) -> MinistryLandReport {
    let total_land_value: f64 = cadastre.parcels.values().map(|p| p.current_value).sum();
    let total_hectares: f64 = cadastre.parcels.values().map(|p| p.size_hectares).sum();
    let foreign_pct = foreign_ownership_percentage(cadastre);

    let mut regional_summaries = Vec::new();
    for region in regions {
        if region.node_type != crate::society::geography::NodeType::LandRegion {
            continue;
        }
        let region_hectares: f64 = cadastre
            .parcels
            .values()
            .filter(|p| p.region_id == region.id)
            .map(|p| p.size_hectares)
            .sum();
        let region_value: f64 = cadastre
            .parcels
            .values()
            .filter(|p| p.region_id == region.id)
            .map(|p| p.current_value)
            .sum();
        let region_certainty: f64 = {
            let mut sum = 0.0;
            let mut count = 0;
            for p in cadastre.parcels.values() {
                if p.region_id == region.id {
                    sum += p.legal_certainty;
                    count += 1;
                }
            }
            if count > 0 { sum / count as f64 } else { 0.0 }
        };
        let region_foreign: f64 = {
            let foreign: f64 = cadastre
                .parcels
                .values()
                .filter(|p| p.region_id == region.id && p.owner_type == ParcelOwnerType::ForeignFund)
                .map(|p| p.size_hectares)
                .sum();
            if region_hectares > 0.0 { foreign / region_hectares } else { 0.0 }
        };

        regional_summaries.push(MinistryRegionalSummary {
            region_id: region.id.clone(),
            total_hectares: region_hectares,
            total_value: region_value,
            avg_legal_certainty: region_certainty,
            border_conflicts: border_conflicts.count_for_region(&region.id) as u32,
            foreign_ownership_pct: region_foreign,
            court_backlog: border_conflicts.court_load_for_region(&region.id),
        });
    }

    let total_border = border_conflicts.conflicts.len() as u32;
    let total_arb = arbitration_court.pending_count() as u32;
    let total_exposure = arbitration_court.unresolved_liabilities();

    MinistryLandReport {
        report_turn,
        total_land_value,
        total_hectares,
        foreign_ownership_pct: foreign_pct,
        total_border_conflicts: total_border,
        total_arbitration_cases: total_arb,
        total_arbitration_exposure: total_exposure,
        regional_summaries,
        delay_note: format!("Report delayed by bureaucratic processing. Data as of turn {}.", report_turn),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::cadastre::{Cadastre, CadastreConfig, ParcelChunk, ParcelOwnerType, ZoningDesignation, LandPriceHistoryRegistry};

    fn make_parcel(region_id: &str, size: f64, owner_type: ParcelOwnerType, owner_id: &str, certainty: f64) -> ParcelChunk {
        ParcelChunk {
            region_id: region_id.to_string(),
            size_hectares: size,
            owner_type,
            owner_id: owner_id.to_string(),
            legal_certainty: certainty,
            soil_class: "Class_II".to_string(),
            zoning: ZoningDesignation::Agricultural,
            acquisition_price: 500_000.0,
            ..Default::default()
        }
    }

    #[test]
    fn test_evaluate_max_bid_basic() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let config = CadastreConfig::default();
        let modifiers = MarketBehaviorModifiers::default();
        let bid = evaluate_max_bid(&parcel, &config, &modifiers, 1_000_000.0, 1000.0, 0.0);
        assert!(bid.is_some(), "Should produce a bid for a good parcel");
        assert!(bid.unwrap() > 0.0);
    }

    #[test]
    fn test_evaluate_max_bid_ambitious_overbids() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let config = CadastreConfig::default();
        let normal_modifiers = MarketBehaviorModifiers::default();
        let ambitious_modifiers = MarketBehaviorModifiers {
            share_price_premium: 0.3, // 30% premium
            ..Default::default()
        };
        let normal_bid = evaluate_max_bid(&parcel, &config, &normal_modifiers, 10_000_000.0, 10000.0, 0.0).unwrap();
        let ambitious_bid = evaluate_max_bid(&parcel, &config, &ambitious_modifiers, 10_000_000.0, 10000.0, 0.0).unwrap();
        assert!(ambitious_bid > normal_bid, "Ambitious buyer should bid more");
    }

    #[test]
    fn test_evaluate_max_bid_paranoid_discount() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let config = CadastreConfig::default();
        let normal_modifiers = MarketBehaviorModifiers::default();
        let paranoid_modifiers = MarketBehaviorModifiers {
            share_price_premium: -0.2, // 20% discount
            ..Default::default()
        };
        let normal_bid = evaluate_max_bid(&parcel, &config, &normal_modifiers, 10_000_000.0, 10000.0, 0.0).unwrap();
        let paranoid_bid = evaluate_max_bid(&parcel, &config, &paranoid_modifiers, 10_000_000.0, 10000.0, 0.0).unwrap();
        assert!(paranoid_bid < normal_bid, "Paranoid buyer should bid less");
    }

    #[test]
    fn test_evaluate_max_bid_risk_filter() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.2); // Low certainty
        let config = CadastreConfig::default();
        let risk_averse = MarketBehaviorModifiers {
            risk_tolerance: 0.3,
            ..Default::default()
        };
        let bid = evaluate_max_bid(&parcel, &config, &risk_averse, 1_000_000.0, 1000.0, 0.0);
        assert!(bid.is_none(), "Risk-averse buyer should skip low-certainty parcel");
    }

    #[test]
    fn test_evaluate_max_bid_position_cap() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let config = CadastreConfig::default();
        let modifiers = MarketBehaviorModifiers {
            max_position_pct: 0.1, // 10% cap
            ..Default::default()
        };
        // Buyer already owns 95 out of 1000 hectares → cap is 100, remaining = 5
        // But parcel is 100 hectares → can't fit
        let bid = evaluate_max_bid(&parcel, &config, &modifiers, 10_000_000.0, 1000.0, 95.0);
        assert!(bid.is_none(), "Should reject when position cap exceeded");
    }

    #[test]
    fn test_evaluate_max_bid_frozen_parcel() {
        let mut parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        parcel.is_frozen = true;
        let config = CadastreConfig::default();
        let modifiers = MarketBehaviorModifiers::default();
        let bid = evaluate_max_bid(&parcel, &config, &modifiers, 1_000_000.0, 1000.0, 0.0);
        assert!(bid.is_none(), "Frozen parcel should not be biddable");
    }

    #[test]
    fn test_land_transaction_debits_buyer() {
        let mut cadastre = Cadastre::default();
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let parcel_id = cadastre.insert(parcel);

        let mut buyer_cash = 2_000_000.0_f64;
        let mut seller_cash = 0.0_f64;
        let config = CadastreConfig::default();

        let result = process_land_transaction(
            &mut cadastre,
            parcel_id,
            "CORP_1",
            ParcelOwnerType::Corporate,
            &mut buyer_cash,
            &mut seller_cash,
            500_000.0,
            None,
            &config,
            None,
            5,
        );

        assert!(result.is_some(), "Transaction should succeed");
        let r = result.unwrap();
        assert!((buyer_cash - (2_000_000.0 - 500_000.0 - r.stamp_duty)).abs() < 0.01,
            "Buyer cash should be debited by price + stamp duty");
        assert!((seller_cash - 500_000.0).abs() < 0.01,
            "Seller cash should be credited with price");
    }

    #[test]
    fn test_land_transaction_insufficient_funds() {
        let mut cadastre = Cadastre::default();
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let parcel_id = cadastre.insert(parcel);

        let mut buyer_cash = 100_000.0_f64; // Not enough
        let mut seller_cash = 0.0_f64;
        let config = CadastreConfig::default();

        let result = process_land_transaction(
            &mut cadastre,
            parcel_id,
            "CORP_1",
            ParcelOwnerType::Corporate,
            &mut buyer_cash,
            &mut seller_cash,
            500_000.0,
            None,
            &config,
            None,
            5,
        );

        assert!(result.is_none(), "Transaction should be rejected with insufficient funds");
        assert!((buyer_cash - 100_000.0).abs() < 0.01, "Buyer cash should be unchanged");
        assert!((seller_cash - 0.0).abs() < 0.01, "Seller cash should be unchanged");
    }

    #[test]
    fn test_land_transaction_updates_acquisition_price() {
        let mut cadastre = Cadastre::default();
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let parcel_id = cadastre.insert(parcel);

        let mut buyer_cash = 2_000_000.0_f64;
        let mut seller_cash = 0.0_f64;
        let config = CadastreConfig::default();

        process_land_transaction(
            &mut cadastre,
            parcel_id,
            "CORP_1",
            ParcelOwnerType::Corporate,
            &mut buyer_cash,
            &mut seller_cash,
            750_000.0,
            None,
            &config,
            None,
            10,
        ).unwrap();

        let p = cadastre.get(parcel_id).unwrap();
        assert!((p.acquisition_price - 750_000.0).abs() < 0.01, "acquisition_price should be updated");
        assert_eq!(p.acquisition_turn, 10);
        assert_eq!(p.owner_type, ParcelOwnerType::Corporate);
        assert_eq!(p.owner_id, "CORP_1");
    }

    #[test]
    fn test_land_transaction_with_split() {
        let mut cadastre = Cadastre::default();
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let parcel_id = cadastre.insert(parcel);

        let mut buyer_cash = 2_000_000.0_f64;
        let mut seller_cash = 0.0_f64;
        let config = CadastreConfig::default();

        let result = process_land_transaction(
            &mut cadastre,
            parcel_id,
            "CORP_1",
            ParcelOwnerType::Corporate,
            &mut buyer_cash,
            &mut seller_cash,
            500_000.0,
            Some(30.0), // Buy only 30 hectares
            &config,
            None,
            5,
        );

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.was_split);
        assert!((r.hectares_transferred - 30.0).abs() < 0.01);
        // Original parcel should be 70 hectares
        let original = cadastre.get(parcel_id).unwrap();
        assert!((original.size_hectares - 70.0).abs() < 0.01);
        assert_eq!(original.owner_type, ParcelOwnerType::State);
    }

    #[test]
    fn test_land_transaction_stamp_duty() {
        let mut cadastre = Cadastre::default();
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        let parcel_id = cadastre.insert(parcel);

        let mut buyer_cash = 2_000_000.0_f64;
        let mut seller_cash = 0.0_f64;
        let mut budget = crate::politics::local_government::RegionalBudget::default();
        let initial_reserves = budget.liquid_reserves;
        let config = CadastreConfig::default();

        let result = process_land_transaction(
            &mut cadastre,
            parcel_id,
            "CORP_1",
            ParcelOwnerType::Corporate,
            &mut buyer_cash,
            &mut seller_cash,
            500_000.0,
            None,
            &config,
            Some(&mut budget),
            5,
        ).unwrap();

        let expected_stamp = 500_000.0 * config.stamp_duty_rate;
        assert!((result.stamp_duty - expected_stamp).abs() < 0.01);
        assert!((budget.liquid_reserves - (initial_reserves + expected_stamp)).abs() < 0.01,
            "Stamp duty should be credited to regional budget");
    }

    #[test]
    fn test_compensation_none() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5);
        let history = LandPriceHistoryRegistry::default();
        let comp = calculate_compensation(&parcel, &CompensationScheme::None, &history, 24);
        assert_eq!(comp, 0.0, "None scheme = zero compensation");
    }

    #[test]
    fn test_compensation_statutory_fiat() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5);
        let history = LandPriceHistoryRegistry::default();
        let mut soil_prices = BTreeMap::new();
        soil_prices.insert("Class_II".to_string(), 20_000.0);
        let scheme = CompensationScheme::StatutoryFiat {
            fiat_prices_by_soil: soil_prices,
            fiat_prices_by_zoning: BTreeMap::new(),
        };
        let comp = calculate_compensation(&parcel, &scheme, &history, 24);
        // 20_000 per hectare × 100 hectares = 2_000_000
        assert!((comp - 2_000_000.0).abs() < 0.01, "Expected 2M, got {}", comp);
    }

    #[test]
    fn test_compensation_fair_market_with_history() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5);
        let mut history = LandPriceHistoryRegistry::default();
        // Record 100 turns of price history at 30_000/ha
        for _ in 0..100 {
            history.record("R1", 30_000.0);
        }
        let scheme = CompensationScheme::FairMarketAverage {
            lookback_years: 3,
            average_multiplier: 1.0,
        };
        let comp = calculate_compensation(&parcel, &scheme, &history, 24);
        // 3 years × 24 turns = 72 turns lookback
        // Average = 30_000, × 1.0 × 100 hectares = 3_000_000
        assert!((comp - 3_000_000.0).abs() < 0.01, "Expected 3M, got {}", comp);
    }

    #[test]
    fn test_compensation_fair_market_fallback_to_acquisition_price() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5);
        // acquisition_price is 500_000
        let history = LandPriceHistoryRegistry::default(); // No history — young world
        let scheme = CompensationScheme::FairMarketAverage {
            lookback_years: 3,
            average_multiplier: 1.0,
        };
        let comp = calculate_compensation(&parcel, &scheme, &history, 24);
        // **FALLBACK**: Should use acquisition_price (500_000), NOT hedonic valuation
        assert!((comp - 500_000.0).abs() < 0.01,
            "FairMarketAverage fallback should use acquisition_price, got {}", comp);
    }

    #[test]
    fn test_compensation_fair_market_insufficient_history_fallback() {
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5);
        let mut history = LandPriceHistoryRegistry::default();
        // Only 10 turns of history — need 72 for 3-year lookback
        for _ in 0..10 {
            history.record("R1", 30_000.0);
        }
        let scheme = CompensationScheme::FairMarketAverage {
            lookback_years: 3,
            average_multiplier: 1.0,
        };
        let comp = calculate_compensation(&parcel, &scheme, &history, 24);
        // Insufficient history → fallback to acquisition_price
        assert!((comp - 500_000.0).abs() < 0.01,
            "Insufficient history should fallback to acquisition_price, got {}", comp);
    }

    #[test]
    fn test_foreign_purchase_allowed_normal() {
        let mut cadastre = Cadastre::default();
        // 1000 hectares state, 0 foreign. Cap is 20% = 200 hectares.
        cadastre.insert(make_parcel("R1", 1000.0, ParcelOwnerType::State, "TREASURY", 0.8));
        let law = AgrarianReformLaw {
            foreign_ownership_cap: 0.2,
            foreign_border_zone_ban_km: 10.0,
            ..Default::default()
        };
        // Buying 50 hectares → 50/1050 = ~4.8% foreign, well under 20% cap
        let parcel = make_parcel("R1", 50.0, ParcelOwnerType::State, "TREASURY", 0.8);
        assert!(check_foreign_purchase_allowed(&cadastre, &parcel, &law),
            "Normal purchase should be allowed");
    }

    #[test]
    fn test_foreign_purchase_blocked_border_zone() {
        let cadastre = Cadastre::default();
        let law = AgrarianReformLaw {
            foreign_border_zone_ban_km: 10.0,
            ..Default::default()
        };
        let mut parcel = make_parcel("R1", 50.0, ParcelOwnerType::State, "TREASURY", 0.8);
        parcel.is_border_zone = true;
        assert!(!check_foreign_purchase_allowed(&cadastre, &parcel, &law),
            "Border zone purchase should be blocked for foreign funds");
    }

    #[test]
    fn test_foreign_purchase_blocked_cap_exceeded() {
        let mut cadastre = Cadastre::default();
        // 800 hectares state, 200 hectares foreign = 20% foreign
        cadastre.insert(make_parcel("R1", 800.0, ParcelOwnerType::State, "TREASURY", 0.8));
        cadastre.insert(ParcelChunk {
            region_id: "R1".to_string(),
            size_hectares: 200.0,
            owner_type: ParcelOwnerType::ForeignFund,
            owner_id: "FF_1".to_string(),
            ..Default::default()
        });
        let law = AgrarianReformLaw {
            foreign_ownership_cap: 0.2, // 20% cap — already at cap
            ..Default::default()
        };
        let parcel = make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8);
        assert!(!check_foreign_purchase_allowed(&cadastre, &parcel, &law),
            "Purchase should be blocked when foreign cap exceeded");
    }

    #[test]
    fn test_agrarian_reform_expropriation() {
        let mut cadastre = Cadastre::default();
        // One owner with 1000 hectares — exceeds 500 limit
        cadastre.insert(make_parcel("R1", 600.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5));
        cadastre.insert(make_parcel("R1", 400.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5));

        let mut court = ArbitrationCourt::default();
        let law = AgrarianReformLaw {
            max_estate_size: 500.0,
            expropriate_existing: true,
            compensation_scheme: CompensationScheme::None,
            redistribute_to_peasants: false,
            ..Default::default()
        };
        let history = LandPriceHistoryRegistry::default();
        // Use a config with 100% filing probability to make the test deterministic
        let arb_config = ArbitrationConfig {
            base_filing_probability: 1.0,
            ..Default::default()
        };

        let mut rng = rand::thread_rng();
        let result = execute_agrarian_reform(
            &mut cadastre,
            &mut court,
            &law,
            &history,
            &arb_config,
            "TestLand",
            10,
            24,
            &mut rng,
        );

        assert!(result.parcels_expropriated > 0, "Should expropriate parcels exceeding limit");
        assert!(result.hectares_expropriated > 0.0);
        // With 100% filing probability and None compensation, cases should be filed
        assert!(result.arbitration_cases_filed > 0, "Should file arbitration cases");
    }

    #[test]
    fn test_agrarian_reform_redistribute_to_peasants() {
        let mut cadastre = Cadastre::default();
        cadastre.insert(make_parcel("R1", 600.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5));

        let mut court = ArbitrationCourt::default();
        let law = AgrarianReformLaw {
            max_estate_size: 500.0,
            expropriate_existing: true,
            compensation_scheme: CompensationScheme::None,
            redistribute_to_peasants: true,
            ..Default::default()
        };
        let history = LandPriceHistoryRegistry::default();
        let arb_config = ArbitrationConfig::default();
        let mut rng = rand::thread_rng();

        execute_agrarian_reform(
            &mut cadastre,
            &mut court,
            &law,
            &history,
            &arb_config,
            "TestLand",
            10,
            24,
            &mut rng,
        );

        // Expropriated parcel should now be owned by a peasant
        let p = cadastre.parcels.values().next().unwrap();
        assert_eq!(p.owner_type, ParcelOwnerType::Private);
        assert!(p.owner_id.starts_with("PEASANT_REFORM_"), "Should be redistributed to peasant");
    }

    #[test]
    fn test_agrarian_reform_no_expropriation_under_limit() {
        let mut cadastre = Cadastre::default();
        cadastre.insert(make_parcel("R1", 300.0, ParcelOwnerType::Private, "DYNASTY_1", 0.5));

        let mut court = ArbitrationCourt::default();
        let law = AgrarianReformLaw {
            max_estate_size: 500.0,
            expropriate_existing: true,
            compensation_scheme: CompensationScheme::None,
            ..Default::default()
        };
        let history = LandPriceHistoryRegistry::default();
        let arb_config = ArbitrationConfig::default();
        let mut rng = rand::thread_rng();

        let result = execute_agrarian_reform(
            &mut cadastre,
            &mut court,
            &law,
            &history,
            &arb_config,
            "TestLand",
            10,
            24,
            &mut rng,
        );

        assert_eq!(result.parcels_expropriated, 0, "Should not expropriate under limit");
    }

    #[test]
    fn test_record_transaction_price() {
        let mut history = LandPriceHistoryRegistry::default();
        record_transaction_price(&mut history, "R1", 25_000.0);
        record_transaction_price(&mut history, "R1", 30_000.0);
        let avg = history.rolling_average("R1", 2).unwrap();
        assert!((avg - 27_500.0).abs() < 0.01, "Expected 27500, got {}", avg);
    }

    #[test]
    fn test_ministry_land_report_generation() {
        let mut cadastre = Cadastre::default();
        cadastre.insert(make_parcel("R1", 100.0, ParcelOwnerType::State, "TREASURY", 0.8));
        cadastre.insert(make_parcel("R1", 50.0, ParcelOwnerType::ForeignFund, "FF_1", 0.6));

        let border_conflicts = crate::society::cadastre::BorderConflictRegistry::default();
        let court = ArbitrationCourt::default();
        let regions = vec![crate::society::geography::Region {
            id: "R1".to_string(),
            node_type: crate::society::geography::NodeType::LandRegion,
            ..Default::default()
        }];

        let report = generate_ministry_land_report(&cadastre, &border_conflicts, &court, &regions, 10);
        assert_eq!(report.report_turn, 10);
        assert!(report.total_hectares > 0.0);
        assert!(report.foreign_ownership_pct > 0.0);
        assert_eq!(report.regional_summaries.len(), 1);
    }
}
