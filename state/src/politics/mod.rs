//! Political systems, elections, and ideological power.
//!
//! This module ports the Python `politics/` tree into strictly typed Rust
//! structures: constitutions, legislatures, parties, leaders, and the turn
//! logic that runs elections, builds coalitions, and updates national policy.
//!
//! The module is intentionally allowed to have missing documentation while
//! the political structures are being stabilized and the parity tests are
//! being refined.
#![allow(missing_docs)]

pub mod advisory_council;
pub mod anti_corruption;
pub mod attendance;
pub mod bill_lifecycle;
pub mod budget_lifecycle;
pub mod campaign;
pub mod chaos_config;
pub mod citizenship;
pub mod committees;
pub mod conservation;
pub mod crisis_management;
pub mod elections;
pub mod espionage;
pub mod fiscal_transfers;
pub mod free_speech;
pub mod funding;
pub mod generator;
pub mod ideology;
pub mod interest_groups;
pub mod laws;
pub mod legislation;
pub mod legislative_weight;
pub mod lobbying;
pub mod local_council;
pub mod local_government;
pub mod local_legislation;
pub mod jst_spending;
pub mod equalization;
pub mod mass_movements;
pub mod ministries;
pub mod names;
pub mod parliament;
pub mod rebellions;
pub mod social_programs;
pub mod state_structure;
pub mod succession;
pub mod system;
pub mod trade_policy;
pub mod traits;
pub mod turn;
pub mod vip_registry;

pub use advisory_council::{
    apply_decree_against_council, calculate_council_opinion, AdvisoryCouncil,
    CouncilInfluenceModifier, CouncilMember, CouncilOpinion, CouncilType, FactionType,
};
pub use attendance::{calculate_attendance, AttendanceModel, AttendanceResult, QuorumType};
pub use bill_lifecycle::{
    deterministic_roll, process_bicameral_review, process_bill_lifecycle, process_committee_stage,
    process_executive_review, process_floor_vote, process_legislation_turn,
};
pub use budget_lifecycle::{
    apply_budget_failure_consequence, draft_budget_bill, process_budget_amendments,
    process_budget_lifecycle, BudgetAmendment, BudgetBill, BudgetBillStage,
    BudgetFailureConsequence,
};
pub use campaign::{
    execute_digital_campaign, execute_national_ad_campaign, execute_regional_rally,
    execute_television_campaign, generate_corporate_lobbying_black_money,
    generate_money_laundering_black_money, generate_organized_crime_black_money,
    process_campaign_spending, process_election_cycle, AuditStatus, BlackMoneyPool,
    BlackMoneySource, CampaignAction, CampaignError, CampaignExecution, ElectionState,
    ElectoralCommission,
};
pub use chaos_config::ChaosConfig;
pub use committees::{Committee, CommitteeSystem, CommitteeType};
pub use conservation::{
    create_landscape_park, create_national_park, process_conservation_turn, ConservationPolicy,
    ConservationPolicyType, LandscapePark, NationalPark, ZoningRule,
};
pub use elections::{
    build_coalition, build_coalition_with_concessions, calculate_seats,
    calculate_upper_house_composition, check_coalition_stability, ideological_distance,
    ConcessionClause,
};
pub use espionage::{EspionageOperation, EspionageState, EspionageType};
pub use fiscal_transfers::{
    check_commissary_administration, process_fiscal_transfers, process_local_elections,
    process_municipal_debt_service, process_regional_taxes, update_curial_faction_alignments,
};
pub use free_speech::{AssemblyRights, FreeSpeechLaw, FreeSpeechLevel, PressFreedom};
pub use laws::{
    enact_law, BorderState, CustomsState, DeportationPolicy, EducationLaw, HealthcareLaw,
    InspectorateState, LawType, MigrationFlow, MigrationLaw, MigrationReason, Violation,
    ViolationType,
};
pub use legislation::{enact_bill, process_sunset_expirations, BillProvision, SunsetProvision};
pub use legislation::{Bill, Clause, Concession, LegislativeSession, LegislativeStage};
pub use legislative_weight::{derive_weight_from_provisions, LegislativeWeight};
pub use lobbying::{
    collect_membership_dues, execute_black_money_financing, execute_councilor_bribery,
    execute_legal_lobbying, process_lobbying_turn, LobbyingGroup, LobbyingGroupType,
    LobbyingOperation, LobbyingOperationType, LobbyingStatus, LobbyingTarget,
};
pub use local_council::{
    calculate_curial_faction_alignment, calculate_seat_count, calculate_vote_probability,
    Councilor, CouncilorTrait, ElectionConfig, Faction, FactionDistribution, LocalCouncil,
    LocalElectionSystem,
};
pub use local_legislation::{
    vote_on_mandate_funding, LocalBill, LocalBillStage, LocalProvision, MandateFundingDecision,
    UnfundedMandate,
};
pub use mass_movements::{
    apply_mass_movement_disruption, check_mass_movement_spawn, process_mass_movements_turn,
    process_union_strike_fund, suppress_mass_movement, MassMovement, MassMovementStatus,
    MassMovementType, MovementError, SuppressionError, SuppressionResult,
};
pub use ministries::{
    allocate_cash_to_ministries, calculate_budget_needs, form_government,
    prepare_minister_strategies, process_minister_post_clearing, sum_ministry_allocations,
    BudgetPriorities, GovernmentCompetency, IdeologyBudgetPriorities, Ministry, MinistryAllocation,
    MinistryConfig, MinistrySpendingAction,
};
pub use names::{
    generate_full_vip, generate_key_vip, generate_person_name, generate_unique_vip,
    name_pool_for_culture, vip_to_leader, NamePool, VipName,
};
pub use parliament::{
    assign_club_chairpersons, check_faction_splintering, initialize_parliament, Chamber,
    ChamberPresidium, NamedVip, Parliament, ParliamentaryClub, SplinterEvent, StateOfEmergency,
    VipRole, VoteRecord,
};
pub use rebellions::{
    check_rebellion_risk, process_rebellion_spawning, spawn_rebel_proto_state, RebellionTrigger,
    RebellionType,
};
pub use state_structure::{RegionalLaw, RegionalLawType, StateStructure, StateStructureConfig};
pub use succession::{
    process_dynasty_turn as process_dynasty_turn_succession, regent_behavior, MarriageSignificance,
    RegentBehavior, RoyalBirth, RoyalDynasty, RoyalFamilyMember, RoyalMarriage, RoyalRelation,
    SuccessionOutcome,
};
pub use system::{
    Constitution, FiscalTransferConfig, Judiciary, Leader, Party, Politics, UpperHouse,
};
pub use traits::{
    apply_leader_modifiers, process_leader_traits_turn, LeaderTrait, ModifierType, TraitModifier,
    TraitRegistry,
};
pub use turn::{
    apply_ruling_ideology_policies, assign_regional_heads, bootstrap_politics, check_snap_election,
    process_political_turn, process_political_year, run_election_if_due,
};
pub use vip_registry::{
    age_health_degradation, assign_core_traits, death_probability, DeathCause, DiplomaticPost,
    DiplomaticPostType, IncapacityStatus, PendingDeath, Vip, VipRegistry, VipRoleExtended,
    CORE_TRAITS,
};
