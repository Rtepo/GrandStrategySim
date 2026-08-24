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

pub use ai_doctrines::{
    GeopoliticalDoctrine, DoctrineConfig, evaluate_doctrine, execute_doctrine,
};
pub use diplomacy::{generate_diplomacy, process_diplomacy_turn, compute_diplomat_modifiers};
pub use fog_of_war::{
    IntelLevel, ForeignIntelligence, FogOfWarConfig, FogOfWarResult, DiplomaticConfig,
    compute_intel_level, apply_fog_of_war, process_intel_turn,
};
pub use organizations::{
    InternationalOrganization, IntegrationLevel, VotingMechanism, OrgCouncil, OrgParliament,
    CouncilMember, Directive, MandateType, OrgConfig, OrganizationRegistry,
};
pub use reputation::{
    GlobalReputation, TreatyViolation, ReputationConfig,
};
pub use sanctions::{
    Sanction, SanctionType, SanctionConfig, SanctionRegistry,
};
pub use trade::{
    balance_global_trade, CommodityTradeEntry, DiplomaticRelation, TradeBalanceResult, TradeDelta,
};
pub use treaties::{
    Treaty, TreatyClause, TreatyStatus, TreatyConfig, TreatyRegistry,
};
