//! Phase 72: Propaganda campaigns.
//!
//! The state can spend treasury funds on propaganda to boost `war_morale`
//! and/or `mental_health`. Propaganda effectiveness scales with the
//! MediaAndEntertainment sector capacity.
//!
//! # Cash Flow (Rule 1 compliance)
//! `treasury.liquid_reserves → media companies` (double-entry).
//! The propaganda spending flows to the media sector as revenue.
//! No free morale boost — the state must pay, and the media sector profits.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// PROPAGANDA TARGET
// ============================================================================

/// Target of a propaganda campaign.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PropagandaTarget {
    /// Boost war morale only.
    WarMorale,
    /// Boost general mental health only.
    GeneralHappiness,
    /// Boost both war morale and mental health.
    #[default]
    Both,
}

// ============================================================================
// PROPAGANDA CAMPAIGN
// ============================================================================

/// A propaganda campaign launched by the state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PropagandaCampaign {
    /// Unique campaign ID.
    pub id: String,
    /// Turn when the campaign was launched.
    pub launch_turn: u32,
    /// Budget allocated to the campaign (debited from treasury).
    pub budget_allocation: f64,
    /// Target of the campaign.
    pub target: PropagandaTarget,
    /// Effectiveness multiplier (scales with media sector capacity).
    pub effectiveness: f64,
    /// Morale boost applied (computed from budget and effectiveness).
    pub morale_boost: f64,
    /// Mental health boost applied.
    pub mental_health_boost: f64,
    /// Media company IDs that received the propaganda spending.
    pub media_companies_paid: Vec<String>,
    /// Total amount paid to media companies.
    pub total_paid_to_media: f64,
}

// ============================================================================
// PROPAGANDA CONFIG
// ============================================================================

/// Configuration for propaganda campaigns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropagandaConfig {
    /// Morale boost per unit of budget (scaled by effectiveness).
    pub morale_boost_per_budget_unit: f64,
    /// Maximum morale boost from a single campaign (cap).
    pub max_morale_boost: f64,
    /// Effectiveness baseline (1.0 = average media capacity).
    pub baseline_effectiveness: f64,
    /// Effectiveness scaling with media sector capacity.
    /// Higher media capacity → more effective propaganda.
    pub media_capacity_scaling: f64,
    /// Propaganda effect decay rate per turn.
    pub effect_decay_rate: f64,
}

impl Default for PropagandaConfig {
    fn default() -> Self {
        Self {
            morale_boost_per_budget_unit: 0.001,
            max_morale_boost: 20.0,
            baseline_effectiveness: 1.0,
            media_capacity_scaling: 1.0,
            effect_decay_rate: 0.1,
        }
    }
}

// ============================================================================
// PROPAGANDA EXECUTION
// ============================================================================

/// Result of executing a propaganda campaign.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropagandaResult {
    /// Whether the campaign was successfully executed.
    pub executed: bool,
    /// The campaign (if executed).
    pub campaign: Option<PropagandaCampaign>,
    /// Amount debited from treasury.
    pub treasury_debited: f64,
    /// Amount credited to media companies.
    pub media_credited: f64,
    /// Morale boost applied.
    pub morale_boost: f64,
    /// Mental health boost applied.
    pub mental_health_boost: f64,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Executes a propaganda campaign.
///
/// # Cash Flow (Rule 1 — double-entry)
/// 1. Treasury is debited: `treasury.liquid_reserves -= budget`
/// 2. Media companies are credited: each company receives a share of the budget
///    proportional to their capacity (pro-rata, Rule 5).
/// 3. No money is created or destroyed.
///
/// # Effectiveness
/// Effectiveness scales with the MediaAndEntertainment sector capacity.
/// More media infrastructure → more effective propaganda per budget unit.
///
/// # Arguments
/// * `treasury_reserves` - Mutable treasury liquid reserves (will be debited).
/// * `media_companies` - Map of company_id → (liquid_capital, media_capacity).
///   Companies with media capacity receive the propaganda spending.
/// * `budget` - Budget for the campaign.
/// * `target` - Target of the campaign.
/// * `config` - Propaganda configuration.
/// * `turn` - Current turn.
/// * `campaign_id` - Unique campaign ID.
///
/// # Returns
/// `PropagandaResult` with the campaign details.
pub fn execute_propaganda(
    treasury_reserves: &mut f64,
    media_companies: &mut HashMap<String, (f64, f64)>, // (liquid_capital, media_capacity)
    budget: f64,
    target: PropagandaTarget,
    config: &PropagandaConfig,
    turn: u32,
    campaign_id: String,
) -> PropagandaResult {
    let mut result = PropagandaResult::default();

    // Check if treasury has sufficient funds
    if *treasury_reserves < budget {
        result.messages.push(format!(
            "[PROPAGANDA] Insufficient treasury funds: have {:.2}, need {:.2}",
            *treasury_reserves, budget
        ));
        return result;
    }

    // Check if there are media companies to receive the spending
    let total_media_capacity: f64 = media_companies.values().map(|(_, cap)| *cap).sum();
    if total_media_capacity <= 0.0 {
        result.messages.push("[PROPAGANDA] No media companies with capacity — campaign aborted".to_string());
        return result;
    }

    // Debit treasury
    *treasury_reserves -= budget;
    result.treasury_debited = budget;

    // Credit media companies (pro-rata by capacity — Rule 5)
    let mut media_paid = Vec::new();
    let mut total_paid = 0.0;
    for (company_id, (liquid_capital, capacity)) in media_companies.iter_mut() {
        let share = (*capacity / total_media_capacity) * budget;
        *liquid_capital += share;
        media_paid.push(company_id.clone());
        total_paid += share;
    }
    result.media_credited = total_paid;

    // Calculate effectiveness
    let effectiveness = config.baseline_effectiveness
        + (total_media_capacity * config.media_capacity_scaling).min(2.0);

    // Calculate morale boost
    let raw_boost = budget * config.morale_boost_per_budget_unit * effectiveness;
    let morale_boost = raw_boost.min(config.max_morale_boost);

    let (morale_apply, mental_health_apply) = match target {
        PropagandaTarget::WarMorale => (morale_boost, 0.0),
        PropagandaTarget::GeneralHappiness => (0.0, morale_boost),
        PropagandaTarget::Both => (morale_boost * 0.6, morale_boost * 0.4),
    };

    result.morale_boost = morale_apply;
    result.mental_health_boost = mental_health_apply;

    let campaign = PropagandaCampaign {
        id: campaign_id.clone(),
        launch_turn: turn,
        budget_allocation: budget,
        target,
        effectiveness,
        morale_boost: morale_apply,
        mental_health_boost: mental_health_apply,
        media_companies_paid: media_paid,
        total_paid_to_media: total_paid,
    };

    result.campaign = Some(campaign);
    result.executed = true;
    result.messages.push(format!(
        "[PROPAGANDA] Campaign {} launched: budget {:.2}, morale +{:.2}, mental_health +{:.2}, paid {:.2} to {} media companies",
        campaign_id, budget, morale_apply, mental_health_apply, total_paid, media_companies.len()
    ));

    result
}

/// Applies propaganda boosts to demographic classes.
///
/// # Arguments
/// * `rural_classes` - Mutable demographics (will have morale boosted).
/// * `morale_boost` - War morale boost to apply.
/// * `mental_health_boost` - Mental health boost to apply.
/// * `baseline_war_morale` - Cap for war morale.
/// * `baseline_mental_health` - Cap for mental health.
pub fn apply_propaganda_boost(
    rural_classes: &mut std::collections::BTreeMap<String, crate::society::geography::ClassDemographics>,
    morale_boost: f64,
    mental_health_boost: f64,
    baseline_war_morale: f64,
    baseline_mental_health: f64,
) {
    for demographics in rural_classes.values_mut() {
        if morale_boost > 0.0 {
            demographics.war_morale = (demographics.war_morale + morale_boost).min(baseline_war_morale);
        }
        if mental_health_boost > 0.0 {
            demographics.mental_health = (demographics.mental_health + mental_health_boost).min(baseline_mental_health);
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::ClassDemographics;
    use std::collections::BTreeMap;

    fn make_media_companies() -> HashMap<String, (f64, f64)> {
        let mut m = HashMap::new();
        m.insert("MEDIA-1".to_string(), (1000.0, 10.0));
        m.insert("MEDIA-2".to_string(), (2000.0, 20.0));
        m.insert("MEDIA-3".to_string(), (500.0, 5.0));
        m
    }

    #[test]
    fn test_propaganda_executes_with_sufficient_funds() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        assert!(result.executed);
        assert_eq!(result.treasury_debited, 1000.0);
        assert!(result.media_credited > 0.0);
        assert!(treasury < 10_000.0, "Treasury must be debited");
    }

    #[test]
    fn test_propaganda_insufficient_funds() {
        let mut treasury = 100.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        assert!(!result.executed);
        assert_eq!(treasury, 100.0, "Treasury must not be debited on failure");
    }

    #[test]
    fn test_propaganda_no_media_companies() {
        let mut treasury = 10_000.0;
        let mut media = HashMap::new();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        assert!(!result.executed);
        assert_eq!(treasury, 10_000.0, "Treasury must not be debited with no media");
    }

    #[test]
    fn test_propaganda_double_entry() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let initial_treasury = treasury;
        let initial_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();

        let _result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        let final_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();

        // Double-entry: treasury decrease must equal media increase
        let treasury_decrease = initial_treasury - treasury;
        let media_increase = final_media_total - initial_media_total;

        assert!((treasury_decrease - media_increase).abs() < 0.01,
            "Double-entry: treasury decrease ({}) must equal media increase ({})",
            treasury_decrease, media_increase);
    }

    #[test]
    fn test_propaganda_pro_rata_distribution() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let _ = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        // MEDIA-2 has 20.0 capacity out of 35.0 total → should get ~571.43
        let media2 = media.get("MEDIA-2").unwrap();
        let media2_gain = media2.0 - 2000.0;
        let expected = 1000.0 * (20.0 / 35.0);
        assert!((media2_gain - expected).abs() < 1.0,
            "MEDIA-2 should receive pro-rata share: expected {:.2}, got {:.2}",
            expected, media2_gain);
    }

    #[test]
    fn test_propaganda_war_morale_only() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::WarMorale, &config, 1, "CAMP-1".to_string(),
        );

        assert!(result.morale_boost > 0.0);
        assert_eq!(result.mental_health_boost, 0.0, "WarMorale target must not boost mental health");
    }

    #[test]
    fn test_propaganda_general_happiness_only() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::GeneralHappiness, &config, 1, "CAMP-1".to_string(),
        );

        assert_eq!(result.morale_boost, 0.0, "GeneralHappiness target must not boost war morale");
        assert!(result.mental_health_boost > 0.0);
    }

    #[test]
    fn test_propaganda_both_targets() {
        let mut treasury = 10_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        assert!(result.morale_boost > 0.0);
        assert!(result.mental_health_boost > 0.0);
    }

    #[test]
    fn test_propaganda_boost_capped() {
        let mut treasury = 1_000_000.0;
        let mut media = make_media_companies();
        let config = PropagandaConfig::default();

        let result = execute_propaganda(
            &mut treasury, &mut media, 1_000_000.0,
            PropagandaTarget::Both, &config, 1, "CAMP-1".to_string(),
        );

        // Morale boost must be capped
        assert!(result.morale_boost <= config.max_morale_boost,
            "Morale boost must be capped at max_morale_boost");
    }

    #[test]
    fn test_apply_propaganda_boost_to_demographics() {
        let mut classes = BTreeMap::new();
        let mut d1 = ClassDemographics::default();
        d1.war_morale = 40.0;
        d1.mental_health = 50.0;
        classes.insert("FreePeasant".to_string(), d1);

        apply_propaganda_boost(&mut classes, 10.0, 5.0, 70.0, 70.0);

        let d = classes.get("FreePeasant").unwrap();
        assert_eq!(d.war_morale, 50.0, "War morale must be boosted by 10");
        assert_eq!(d.mental_health, 55.0, "Mental health must be boosted by 5");
    }

    #[test]
    fn test_apply_propaganda_boost_capped_at_baseline() {
        let mut classes = BTreeMap::new();
        let mut d1 = ClassDemographics::default();
        d1.war_morale = 65.0;
        d1.mental_health = 68.0;
        classes.insert("FreePeasant".to_string(), d1);

        apply_propaganda_boost(&mut classes, 10.0, 5.0, 70.0, 70.0);

        let d = classes.get("FreePeasant").unwrap();
        assert_eq!(d.war_morale, 70.0, "War morale must be capped at baseline");
        assert_eq!(d.mental_health, 70.0, "Mental health must be capped at baseline");
    }
}
