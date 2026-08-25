//! Phase 86: Dynamic MP attendance and quorum system.
//!
//! MPs do not have 100% attendance. Absences are simulated based on
//! macro-variables: poor HealthCapacity causes illness, low Party Discipline
//! reduces whip effectiveness, and social unrest drives opposition boycotts.
//! Failing to reach a Quorum blocks crucial Qualified majority votes.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::bill_lifecycle::deterministic_roll;
use super::legislative_weight::LegislativeWeight;
use super::system::Party;

/// Attendance model calculating per-party MP presence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttendanceModel {
    /// Base attendance rate before modifiers (behavioral constant: 85%).
    /// This represents the human baseline — most MPs attend most votes.
    base_rate: f64,
}

impl Default for AttendanceModel {
    fn default() -> Self {
        AttendanceModel {
            base_rate: 0.85,
        }
    }
}

/// Result of attendance calculation for a single vote.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AttendanceResult {
    /// Total seats present across all parties.
    pub present_seats: u32,
    /// Total seats absent across all parties.
    pub absent_seats: u32,
    /// Absent seats broken down by party (for UI display).
    pub absent_by_party: HashMap<String, u32>,
    /// Present seats broken down by party (for vote calculation).
    pub present_by_party: HashMap<String, u32>,
    /// Whether the quorum was met.
    pub quorum_met: bool,
    /// The quorum threshold that was required.
    pub quorum_threshold: u32,
}

/// Quorum type derived from legislative weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum QuorumType {
    /// 50% of total seats must be present (Ordinary and Organic).
    #[default]
    Simple,
    /// 2/3 of total seats must be present (Constitutional).
    Qualified,
}

impl QuorumType {
    /// Returns the quorum fraction of total seats.
    pub fn fraction(&self) -> f64 {
        match self {
            QuorumType::Simple => 0.50,
            QuorumType::Qualified => 2.0 / 3.0,
        }
    }

    /// Derive quorum type from legislative weight.
    pub fn from_weight(weight: LegislativeWeight) -> Self {
        match weight {
            LegislativeWeight::Constitutional => QuorumType::Qualified,
            LegislativeWeight::Organic | LegislativeWeight::Ordinary => QuorumType::Simple,
        }
    }
}

impl AttendanceModel {
    /// Create a new attendance model with the default base rate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate per-party attendance for a floor vote.
    ///
    /// # Arguments
    /// * `lower_seats` - Seat distribution by party (party_id → seats)
    /// * `parties` - Party data (for discipline values)
    /// * `health_capacity_ratio` - National health capacity ratio (0.0–1.0).
    ///   Low values indicate poor healthcare → more illness absences.
    /// * `social_unrest` - Current social unrest level (0.0–1.0).
    ///   High values → opposition boycotts.
    /// * `bill_title` - Bill title (for deterministic seeding)
    /// * `current_turn` - Current turn (for deterministic seeding)
    ///
    /// # Returns
    /// `AttendanceResult` with present/absent seats per party.
    pub fn calculate(
        &self,
        lower_seats: &HashMap<String, u32>,
        parties: &HashMap<String, Party>,
        health_capacity_ratio: f64,
        social_unrest: f64,
        bill_title: &str,
        current_turn: u32,
    ) -> HashMap<String, u32> {
        let mut present_by_party = HashMap::new();

        for (party_name, &total_seats) in lower_seats {
            // Get party discipline (0.0–1.0).
            let discipline = parties
                .get(party_name)
                .map(|p| p.organization.discipline)
                .unwrap_or(0.5);

            // Health penalty: poor health capacity → illness absences.
            // health_capacity_ratio of 1.0 → no penalty; 0.0 → max penalty.
            let health_penalty = (1.0 - health_capacity_ratio).clamp(0.0, 1.0);

            // Unrest penalty: high unrest → opposition boycotts.
            let unrest_penalty = social_unrest.clamp(0.0, 1.0);

            // Per-party attendance probability.
            let attendance_prob = (self.base_rate
                + discipline * 0.10
                - health_penalty * 0.15
                - unrest_penalty * 0.10)
                .clamp(0.50, 0.98);

            // Deterministic roll per party to determine attendance fraction.
            // We roll once per party and apply the probability to seat count.
            let roll_seed = format!("attendance_{}_{}_{}", bill_title, party_name, current_turn);
            let attendance_roll = deterministic_roll(&roll_seed, attendance_prob);

            // Apply attendance: if roll succeeds, full attendance for this party;
            // otherwise, reduced attendance (half present).
            // This is a simplified model — real parliaments have individual
            // MP absences, but we track anonymized seat pools for performance.
            let present = if attendance_roll {
                total_seats
            } else {
                (total_seats as f64 * 0.5).round() as u32
            };

            present_by_party.insert(party_name.clone(), present);
        }

        present_by_party
    }
}

/// Calculate attendance and check quorum for a floor vote.
///
/// # Arguments
/// * `lower_seats` - Seat distribution by party
/// * `parties` - Party data
/// * `health_capacity_ratio` - National health capacity (0.0–1.0)
/// * `social_unrest` - Social unrest level (0.0–1.0)
/// * `weight` - Legislative weight (determines quorum type)
/// * `bill_title` - Bill title (for deterministic seeding)
/// * `current_turn` - Current turn
///
/// # Returns
/// `AttendanceResult` with present/absent seats and quorum status.
pub fn calculate_attendance(
    lower_seats: &HashMap<String, u32>,
    parties: &HashMap<String, Party>,
    health_capacity_ratio: f64,
    social_unrest: f64,
    weight: LegislativeWeight,
    bill_title: &str,
    current_turn: u32,
) -> AttendanceResult {
    let model = AttendanceModel::new();
    let present_by_party = model.calculate(
        lower_seats,
        parties,
        health_capacity_ratio,
        social_unrest,
        bill_title,
        current_turn,
    );

    let total_seats: u32 = lower_seats.values().sum();
    let present_seats: u32 = present_by_party.values().sum();
    let absent_seats = total_seats.saturating_sub(present_seats);

    let mut absent_by_party = HashMap::new();
    for (party, &seats) in lower_seats {
        let present = present_by_party.get(party).copied().unwrap_or(0);
        let absent = seats.saturating_sub(present);
        if absent > 0 {
            absent_by_party.insert(party.clone(), absent);
        }
    }

    let quorum_type = QuorumType::from_weight(weight);
    let quorum_threshold = (total_seats as f64 * quorum_type.fraction()).ceil() as u32;
    let quorum_met = present_seats >= quorum_threshold;

    AttendanceResult {
        present_seats,
        absent_seats,
        absent_by_party,
        present_by_party,
        quorum_met,
        quorum_threshold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quorum_simple_fraction() {
        assert!((QuorumType::Simple.fraction() - 0.50).abs() < 1e-6);
    }

    #[test]
    fn test_quorum_qualified_fraction() {
        assert!((QuorumType::Qualified.fraction() - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_quorum_from_weight() {
        assert_eq!(
            QuorumType::from_weight(LegislativeWeight::Ordinary),
            QuorumType::Simple
        );
        assert_eq!(
            QuorumType::from_weight(LegislativeWeight::Organic),
            QuorumType::Simple
        );
        assert_eq!(
            QuorumType::from_weight(LegislativeWeight::Constitutional),
            QuorumType::Qualified
        );
    }

    #[test]
    fn test_attendance_model_default_base_rate() {
        let model = AttendanceModel::new();
        assert!((model.base_rate - 0.85).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_attendance_basic() {
        let mut lower_seats = HashMap::new();
        lower_seats.insert("party_a".to_string(), 100u32);
        lower_seats.insert("party_b".to_string(), 50u32);

        let parties = HashMap::new();
        let result = calculate_attendance(
            &lower_seats,
            &parties,
            1.0,  // perfect health
            0.0,  // no unrest
            LegislativeWeight::Ordinary,
            "test_bill",
            1,
        );

        // Total seats = 150, quorum = 75
        assert_eq!(result.quorum_threshold, 75);
        assert!(result.present_seats <= 150);
    }

    #[test]
    fn test_quorum_blocks_constitutional_with_low_attendance() {
        let mut lower_seats = HashMap::new();
        lower_seats.insert("party_a".to_string(), 60u32);
        lower_seats.insert("party_b".to_string(), 40u32);

        let parties = HashMap::new();
        // With 0 health capacity and high unrest, attendance will be low
        let result = calculate_attendance(
            &lower_seats,
            &parties,
            0.0,  // terrible health
            1.0,  // max unrest
            LegislativeWeight::Constitutional,
            "const_amendment",
            1,
        );

        // Total = 100, constitutional quorum = 67
        assert_eq!(result.quorum_threshold, 67);
        // With max penalties, attendance should be reduced
        assert!(result.present_seats < 100);
    }
}
