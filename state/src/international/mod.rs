//! International systems — diplomacy, trade, fog of war, treaties, reputation,
//! AI doctrines, international organizations, and sanctions.
//!
//! This module contains the global, cross-country mechanics that cannot be
//! handled inside a single [`crate::economy::CountryTurnCtx`].  The core entry point is
//! [`trade::balance_global_trade`], which uses the two-phase Collect-Then-Apply
//! mutation pattern so the Rust borrow checker can safely update all nations
//! from a shared global market snapshot.

pub mod ai_doctrines;
pub mod diplomacy;
pub mod fog_of_war;
pub mod organizations;
pub mod reputation;
pub mod sanctions;
pub mod trade;
pub mod treaties;

pub use ai_doctrines::{evaluate_doctrine, execute_doctrine, DoctrineConfig, GeopoliticalDoctrine};
pub use diplomacy::{compute_diplomat_modifiers, generate_diplomacy, process_diplomacy_turn};
pub use fog_of_war::{
    apply_fog_of_war, compute_intel_level, process_intel_turn, DiplomaticConfig, FogOfWarConfig,
    FogOfWarResult, ForeignIntelligence, IntelLevel,
};
pub use organizations::{
    CouncilMember, Directive, IntegrationLevel, InternationalOrganization, MandateType, OrgConfig,
    OrgCouncil, OrgParliament, OrganizationRegistry, VotingMechanism,
};
pub use reputation::{GlobalReputation, ReputationConfig, TreatyViolation};
pub use sanctions::{Sanction, SanctionConfig, SanctionRegistry, SanctionType};
pub use trade::{
    balance_global_trade, CommodityTradeEntry, DiplomaticRelation, TradeBalanceResult, TradeDelta,
};
pub use treaties::{Treaty, TreatyClause, TreatyConfig, TreatyRegistry, TreatyStatus};
