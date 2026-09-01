//! Phase 68: Economic Sanctions system.
//!
//! International Organizations can enact sanctions against bad-faith actors:
//! - `TradeEmbargo`: blocks target from GlobalMarket commodity exchange
//! - `AssetFreeze`: freezes target's foreign BrokerageAccounts
//! - `FinancialIsolation`: blocks aid and investment flows
//! - `FullEmbargo`: all three combined
//!
//! Sanctions require a vote using the organization's VotingMechanism.
//! Sanctions expire after `duration_turns` but can be renewed.
//! Sanctioned countries suffer compounded GlobalReputation damage.

use serde::{Deserialize, Serialize};

/// Type of economic sanction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum SanctionType {
    /// Blocks target from GlobalMarket — sets export/import weight to near-zero.
    TradeEmbargo,
    /// Freezes target's foreign BrokerageAccounts — cannot buy or sell.
    AssetFreeze,
    /// Blocks target from receiving economic aid or foreign investment.
    FinancialIsolation,
    /// All three combined — full economic blockade.
    #[default]
    FullEmbargo,
}

impl SanctionType {
    /// Returns a human-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            SanctionType::TradeEmbargo => "Trade Embargo",
            SanctionType::AssetFreeze => "Asset Freeze",
            SanctionType::FinancialIsolation => "Financial Isolation",
            SanctionType::FullEmbargo => "Full Embargo",
        }
    }

    /// Returns true if this sanction type includes a trade embargo.
    pub fn includes_trade_embargo(&self) -> bool {
        matches!(self, SanctionType::TradeEmbargo | SanctionType::FullEmbargo)
    }

    /// Returns true if this sanction type includes an asset freeze.
    pub fn includes_asset_freeze(&self) -> bool {
        matches!(self, SanctionType::AssetFreeze | SanctionType::FullEmbargo)
    }

    /// Returns true if this sanction type includes financial isolation.
    pub fn includes_financial_isolation(&self) -> bool {
        matches!(
            self,
            SanctionType::FinancialIsolation | SanctionType::FullEmbargo
        )
    }
}

/// An active or expired sanction against a country.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Sanction {
    /// Unique sanction ID.
    pub id: String,
    /// Country being sanctioned.
    pub target_country: String,
    /// Organization that enacted the sanction.
    pub sanctioning_org: String,
    /// Type of sanction.
    pub sanction_type: SanctionType,
    /// Turn the sanction was enacted.
    pub enacted_turn: u32,
    /// Duration in turns (sanction expires after enacted_turn + duration_turns).
    pub duration_turns: u32,
    /// Reason for the sanction.
    pub reason: String,
    /// Whether the sanction has been lifted manually.
    pub lifted: bool,
}

impl Sanction {
    /// Creates a new sanction.
    pub fn new(
        id: String,
        target_country: String,
        sanctioning_org: String,
        sanction_type: SanctionType,
        enacted_turn: u32,
        duration_turns: u32,
        reason: String,
    ) -> Self {
        Self {
            id,
            target_country,
            sanctioning_org,
            sanction_type,
            enacted_turn,
            duration_turns,
            reason,
            lifted: false,
        }
    }

    /// Returns the turn when the sanction expires.
    pub fn expires_at(&self) -> u32 {
        self.enacted_turn + self.duration_turns
    }

    /// Returns true if the sanction is still active at the given turn.
    pub fn is_active(&self) -> bool {
        !self.lifted
    }

    /// Returns true if the sanction is active and has not expired.
    pub fn is_active_at(&self, current_turn: u32) -> bool {
        !self.lifted && current_turn < self.expires_at()
    }

    /// Lifts the sanction manually.
    pub fn lift(&mut self) {
        self.lifted = true;
    }
}

/// Configuration for the sanctions system. No magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SanctionConfig {
    /// Minimum fraction of votes required to enact a sanction.
    pub min_votes_for_sanction: f64,
    /// Default duration of sanctions in turns.
    pub default_duration_turns: u32,
    /// Trade block modifier — multiplier applied to export/import weight (0.0 = full block, 0.05 = smuggling leakage).
    pub trade_block_modifier: f64,
    /// Reputation damage per turn while sanctioned.
    pub reputation_damage_per_turn: f64,
}

impl Default for SanctionConfig {
    fn default() -> Self {
        Self {
            min_votes_for_sanction: 0.65,
            default_duration_turns: 50,
            trade_block_modifier: 0.02, // 2% smuggling leakage
            reputation_damage_per_turn: 1.0,
        }
    }
}

/// Registry tracking all active and expired sanctions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SanctionRegistry {
    /// All sanctions (active, expired, lifted).
    pub sanctions: Vec<Sanction>,
    /// Next auto-increment ID counter.
    pub next_id: u64,
}

impl SanctionRegistry {
    /// Generates the next sanction ID.
    pub fn next_sanction_id(&mut self) -> String {
        self.next_id += 1;
        format!("SANCTION-{:06}", self.next_id)
    }

    /// Returns all active sanctions against a country.
    pub fn active_sanctions_against(&self, country: &str, current_turn: u32) -> Vec<&Sanction> {
        self.sanctions
            .iter()
            .filter(|s| s.target_country == country && s.is_active_at(current_turn))
            .collect()
    }

    /// Returns true if a country has an active trade embargo.
    pub fn has_trade_embargo(&self, country: &str, current_turn: u32) -> bool {
        self.active_sanctions_against(country, current_turn)
            .iter()
            .any(|s| s.sanction_type.includes_trade_embargo())
    }

    /// Returns true if a country has an active asset freeze.
    pub fn has_asset_freeze(&self, country: &str, current_turn: u32) -> bool {
        self.active_sanctions_against(country, current_turn)
            .iter()
            .any(|s| s.sanction_type.includes_asset_freeze())
    }

    /// Returns true if a country has active financial isolation.
    pub fn has_financial_isolation(&self, country: &str, current_turn: u32) -> bool {
        self.active_sanctions_against(country, current_turn)
            .iter()
            .any(|s| s.sanction_type.includes_financial_isolation())
    }

    /// Returns true if a country is under any active sanction.
    pub fn is_sanctioned(&self, country: &str, current_turn: u32) -> bool {
        !self
            .active_sanctions_against(country, current_turn)
            .is_empty()
    }

    /// Enacts a new sanction.
    pub fn enact_sanction(&mut self, sanction: Sanction) {
        self.sanctions.push(sanction);
    }

    /// Lifts a sanction by ID.
    pub fn lift_sanction(&mut self, sanction_id: &str) -> bool {
        if let Some(s) = self.sanctions.iter_mut().find(|s| s.id == sanction_id) {
            s.lift();
            return true;
        }
        false
    }

    /// Expires sanctions that have reached their duration.
    pub fn expire_finished_sanctions(&mut self, current_turn: u32) {
        for s in &mut self.sanctions {
            if !s.lifted && current_turn >= s.expires_at() {
                s.lifted = true; // Mark as expired (no longer active)
            }
        }
    }

    /// Returns all active sanctions (for DTO/snapshot).
    pub fn active_sanctions(&self, current_turn: u32) -> Vec<&Sanction> {
        self.sanctions
            .iter()
            .filter(|s| s.is_active_at(current_turn))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanction_type_includes() {
        assert!(SanctionType::FullEmbargo.includes_trade_embargo());
        assert!(SanctionType::FullEmbargo.includes_asset_freeze());
        assert!(SanctionType::FullEmbargo.includes_financial_isolation());

        assert!(SanctionType::TradeEmbargo.includes_trade_embargo());
        assert!(!SanctionType::TradeEmbargo.includes_asset_freeze());

        assert!(SanctionType::AssetFreeze.includes_asset_freeze());
        assert!(!SanctionType::AssetFreeze.includes_trade_embargo());

        assert!(SanctionType::FinancialIsolation.includes_financial_isolation());
        assert!(!SanctionType::FinancialIsolation.includes_asset_freeze());
    }

    #[test]
    fn test_sanction_active_and_expired() {
        let sanction = Sanction::new(
            "SANCTION-000001".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::TradeEmbargo,
            10,
            20,
            "Treaty violation".to_string(),
        );

        // Active at turn 29
        assert!(sanction.is_active_at(29));

        // Expired at turn 30
        assert!(!sanction.is_active_at(30));
    }

    #[test]
    fn test_sanction_lifted() {
        let mut sanction = Sanction::new(
            "SANCTION-000001".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::AssetFreeze,
            10,
            100,
            "Corruption".to_string(),
        );
        assert!(sanction.is_active());

        sanction.lift();
        assert!(!sanction.is_active());
    }

    #[test]
    fn test_sanction_registry_active_sanctions() {
        let mut registry = SanctionRegistry::default();
        registry.enact_sanction(Sanction::new(
            "S1".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::TradeEmbargo,
            1,
            100,
            "Test".to_string(),
        ));
        registry.enact_sanction(Sanction::new(
            "S2".to_string(),
            "Badland".to_string(),
            "Pacific Pact".to_string(),
            SanctionType::AssetFreeze,
            1,
            100,
            "Test".to_string(),
        ));
        registry.enact_sanction(Sanction::new(
            "S3".to_string(),
            "Otherland".to_string(),
            "World Forum".to_string(),
            SanctionType::FullEmbargo,
            1,
            100,
            "Test".to_string(),
        ));

        let badland_sanctions = registry.active_sanctions_against("Badland", 50);
        assert_eq!(badland_sanctions.len(), 2);

        assert!(registry.has_trade_embargo("Badland", 50));
        assert!(registry.has_asset_freeze("Badland", 50));
        assert!(!registry.has_financial_isolation("Badland", 50));

        assert!(registry.has_financial_isolation("Otherland", 50));
        assert!(registry.has_trade_embargo("Otherland", 50));
    }

    #[test]
    fn test_sanction_registry_expiry() {
        let mut registry = SanctionRegistry::default();
        registry.enact_sanction(Sanction::new(
            "S1".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::TradeEmbargo,
            1,
            10,
            "Test".to_string(),
        ));

        // Active at turn 10
        assert!(registry.has_trade_embargo("Badland", 10));

        // Expire at turn 11
        registry.expire_finished_sanctions(11);
        assert!(!registry.has_trade_embargo("Badland", 11));
    }

    #[test]
    fn test_sanction_registry_lift() {
        let mut registry = SanctionRegistry::default();
        registry.enact_sanction(Sanction::new(
            "S1".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::FullEmbargo,
            1,
            100,
            "Test".to_string(),
        ));

        assert!(registry.is_sanctioned("Badland", 50));
        assert!(registry.lift_sanction("S1"));
        assert!(!registry.is_sanctioned("Badland", 50));
    }

    #[test]
    fn test_sanction_serialization() {
        let sanction = Sanction::new(
            "SANCTION-000001".to_string(),
            "Badland".to_string(),
            "World Forum".to_string(),
            SanctionType::FullEmbargo,
            10,
            50,
            "Treaty violation".to_string(),
        );
        let json = serde_json::to_string(&sanction).unwrap();
        let deserialized: Sanction = serde_json::from_str(&json).unwrap();
        assert_eq!(sanction, deserialized);
    }
}
