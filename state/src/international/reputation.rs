//! Phase 67: Global Reputation system.
//!
//! Tracks each country's international standing. Unilateral treaty abrogation
//! crashes reputation, which has immediate systemic consequences:
//! - Increases diplomatic capacity cost for future treaties
//! - Increases sovereign debt interest rates (risk premium)
//! - AI nations reject low-reputation partners more often
//! - Reputation recovers slowly over time if no new violations occur

use serde::{Deserialize, Serialize};

/// A recorded treaty violation by a country.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TreatyViolation {
    /// Treaty that was violated.
    pub treaty_id: String,
    /// Turn when the violation occurred.
    pub turn: u32,
    /// Severity of the violation (0.0 = minor, 1.0 = severe abrogation).
    pub severity: f64,
    /// Human-readable description.
    pub description: String,
}

/// Global reputation for a single country, ranging from -100 to +100.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalReputation {
    /// Reputation score (-100 to +100, default 0).
    pub score: f64,
    /// History of treaty violations by this country.
    #[serde(default)]
    pub violation_history: Vec<TreatyViolation>,
}

impl Default for GlobalReputation {
    fn default() -> Self {
        Self {
            score: 0.0,
            violation_history: Vec::new(),
        }
    }
}

impl GlobalReputation {
    /// Creates a new reputation with the default neutral score.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the reputation is below the low-reputation threshold.
    pub fn is_low(&self, config: &ReputationConfig) -> bool {
        self.score < config.low_reputation_threshold
    }

    /// Applies a treaty violation penalty to the reputation score.
    pub fn apply_violation(&mut self, violation: TreatyViolation, config: &ReputationConfig) {
        let penalty = config.unilateral_abrogation_penalty * violation.severity;
        self.score = (self.score - penalty).max(-100.0);
        self.violation_history.push(violation);
    }

    /// Recovers reputation by `recovery_per_turn` if no new violations occurred.
    pub fn recover(&mut self, config: &ReputationConfig) {
        self.score = (self.score + config.recovery_per_turn).min(100.0);
    }

    /// Computes the diplomatic capacity cost multiplier based on reputation.
    /// Low reputation → higher multiplier → more expensive treaties.
    pub fn diplomatic_capacity_multiplier(&self, config: &ReputationConfig) -> f64 {
        if self.score >= config.low_reputation_threshold {
            return 1.0;
        }
        // reputation_penalty scales with how far below threshold
        let deficit = config.low_reputation_threshold - self.score;
        let max_deficit = config.low_reputation_threshold + 100.0; // worst case: -100
        let penalty_fraction = deficit / max_deficit;
        1.0 + penalty_fraction * config.diplomatic_capacity_penalty_multiplier
    }

    /// Computes the sovereign debt interest rate penalty based on reputation.
    /// Low reputation → higher penalty → more expensive borrowing.
    pub fn debt_interest_penalty(&self, config: &ReputationConfig) -> f64 {
        if self.score >= config.low_reputation_threshold {
            return 0.0;
        }
        let deficit = config.low_reputation_threshold - self.score;
        let max_deficit = config.low_reputation_threshold + 100.0;
        let penalty_fraction = deficit / max_deficit;
        penalty_fraction * config.debt_interest_penalty_multiplier
    }

    /// Returns the effective diplomatic capacity cost for a base cost.
    pub fn effective_diplomatic_capacity_cost(
        &self,
        base_cost: u32,
        config: &ReputationConfig,
    ) -> u32 {
        let multiplier = self.diplomatic_capacity_multiplier(config);
        ((base_cost as f64) * multiplier).ceil() as u32
    }
}

/// Configuration for the reputation system. No magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReputationConfig {
    /// Penalty applied to reputation score for unilateral abrogation (severity 1.0).
    pub unilateral_abrogation_penalty: f64,
    /// Reputation recovery per turn when no new violations occur.
    pub recovery_per_turn: f64,
    /// Threshold below which reputation is considered "low" (triggers penalties).
    pub low_reputation_threshold: f64,
    /// Maximum multiplier added to diplomatic capacity cost at worst reputation.
    pub diplomatic_capacity_penalty_multiplier: f64,
    /// Maximum interest rate penalty added to sovereign debt at worst reputation.
    pub debt_interest_penalty_multiplier: f64,
    /// Phase E.10: Penalty applied to reputation score for cross-border IP theft
    /// detection (severity 1.0). Scaled by tech cost at call site.
    pub ip_theft_penalty: f64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            unilateral_abrogation_penalty: 30.0,
            recovery_per_turn: 0.5,
            low_reputation_threshold: -20.0,
            diplomatic_capacity_penalty_multiplier: 2.0,
            debt_interest_penalty_multiplier: 0.05, // up to 5% extra interest
            ip_theft_penalty: 15.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_reputation_neutral() {
        let rep = GlobalReputation::new();
        assert_eq!(rep.score, 0.0);
        assert!(rep.violation_history.is_empty());
    }

    #[test]
    fn test_violation_crashes_reputation() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        let violation = TreatyViolation {
            treaty_id: "TREATY-000001".to_string(),
            turn: 10,
            severity: 1.0,
            description: "Unilateral abrogation".to_string(),
        };
        rep.apply_violation(violation, &config);
        assert!(
            rep.score < 0.0,
            "Reputation should be negative after violation"
        );
        assert!(
            (rep.score - (-30.0)).abs() < 0.01,
            "Score should be -30, got {}",
            rep.score
        );
        assert_eq!(rep.violation_history.len(), 1);
    }

    #[test]
    fn test_reputation_recovery() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        rep.score = -50.0;

        // Recover 10 turns
        for _ in 0..10 {
            rep.recover(&config);
        }
        assert!(rep.score > -50.0, "Reputation should recover");
        assert!(
            (rep.score - (-45.0)).abs() < 0.01,
            "Score should be -45, got {}",
            rep.score
        );
    }

    #[test]
    fn test_reputation_recovery_capped() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        rep.score = 99.0;
        rep.recover(&config);
        assert!(rep.score <= 100.0, "Recovery should cap at 100");
    }

    #[test]
    fn test_diplomatic_capacity_multiplier_low_rep() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        rep.score = -100.0; // Worst possible

        let multiplier = rep.diplomatic_capacity_multiplier(&config);
        assert!(
            multiplier > 1.0,
            "Low reputation should increase cost multiplier"
        );
        assert!(
            (multiplier - 3.0).abs() < 0.01,
            "Worst case multiplier should be 3.0, got {}",
            multiplier
        );
    }

    #[test]
    fn test_diplomatic_capacity_multiplier_good_rep() {
        let rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        let multiplier = rep.diplomatic_capacity_multiplier(&config);
        assert_eq!(multiplier, 1.0, "Good reputation should have no penalty");
    }

    #[test]
    fn test_debt_interest_penalty_low_rep() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        rep.score = -100.0;

        let penalty = rep.debt_interest_penalty(&config);
        assert!(
            penalty > 0.0,
            "Low reputation should add debt interest penalty"
        );
        assert!(
            (penalty - 0.05).abs() < 0.001,
            "Worst case penalty should be 0.05, got {}",
            penalty
        );
    }

    #[test]
    fn test_debt_interest_penalty_good_rep() {
        let rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        let penalty = rep.debt_interest_penalty(&config);
        assert_eq!(penalty, 0.0, "Good reputation should have no debt penalty");
    }

    #[test]
    fn test_effective_diplomatic_capacity_cost() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        rep.score = -100.0;

        let effective = rep.effective_diplomatic_capacity_cost(10, &config);
        assert!(
            effective > 10,
            "Low reputation should increase effective cost"
        );
        assert_eq!(effective, 30, "10 * 3.0 = 30, got {}", effective);
    }

    #[test]
    fn test_is_low_reputation() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();

        rep.score = -10.0;
        assert!(
            !rep.is_low(&config),
            "-10 should not be low (threshold is -20)"
        );

        rep.score = -30.0;
        assert!(rep.is_low(&config), "-30 should be low (threshold is -20)");
    }

    #[test]
    fn test_partial_severity_violation() {
        let mut rep = GlobalReputation::new();
        let config = ReputationConfig::default();
        let violation = TreatyViolation {
            treaty_id: "TREATY-000001".to_string(),
            turn: 10,
            severity: 0.5,
            description: "Minor breach".to_string(),
        };
        rep.apply_violation(violation, &config);
        assert!(
            (rep.score - (-15.0)).abs() < 0.01,
            "0.5 severity should give -15, got {}",
            rep.score
        );
    }
}
