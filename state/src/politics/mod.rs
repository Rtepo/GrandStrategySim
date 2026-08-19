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

pub mod bill_lifecycle;
pub mod campaign;
pub mod committees;
pub mod lobbying;
pub mod elections;
pub mod espionage;
pub mod fiscal_transfers;
pub mod funding;
pub mod generator;
pub mod ideology;
pub mod interest_groups;
pub mod legislation;
pub mod laws;
pub mod local_council;
pub mod local_government;
pub mod rebellions;
pub mod system;
pub mod turn;
pub mod conservation;
pub mod chaos_config;
pub mod mass_movements;
pub mod traits;
pub mod ministries;
pub mod budget_lifecycle;
pub mod social_programs;
pub mod citizenship;
pub mod free_speech;
pub mod anti_corruption;
pub mod trade_policy;
pub mod crisis_management;
pub mod names;
pub mod parliament;
pub mod vip_registry;
pub mod succession;
pub mod advisory_council;
pub mod local_legislation;

pub use bill_lifecycle::{process_bill_lifecycle, process_committee_stage, process_floor_vote, process_bicameral_review, process_executive_review, process_legislation_turn, deterministic_roll};
pub use committees::{Committee, CommitteeSystem, CommitteeType};
pub use elections::{calculate_seats, build_coalition, build_coalition_with_concessions, check_coalition_stability, calculate_upper_house_composition, ideological_distance, ConcessionClause};
pub use espionage::{EspionageOperation, EspionageType, EspionageState};
pub use fiscal_transfers::{process_regional_taxes, process_fiscal_transfers, check_commissary_administration, process_municipal_debt_service, process_local_elections, update_curial_faction_alignments};
pub use legislation::{Bill, Clause, Concession, LegislativeStage, LegislativeSession};
pub use local_council::{LocalCouncil, Councilor, CouncilorTrait, Faction, FactionDistribution, LocalElectionSystem, ElectionConfig, calculate_curial_faction_alignment, calculate_seat_count, calculate_vote_probability};
pub use lobbying::{LobbyingGroup, LobbyingOperation, LobbyingGroupType, LobbyingTarget, LobbyingOperationType, LobbyingStatus, collect_membership_dues, execute_legal_lobbying, execute_councilor_bribery, execute_black_money_financing, process_lobbying_turn};
pub use rebellions::{RebellionType, RebellionTrigger, spawn_rebel_proto_state, check_rebellion_risk, process_rebellion_spawning};
pub use system::{Constitution, FiscalTransferConfig, Judiciary, Leader, Party, Politics, UpperHouse};
pub use turn::{bootstrap_politics, process_political_year, process_political_turn, apply_ruling_ideology_policies, check_snap_election, run_election_if_due};
pub use campaign::{ElectionState, ElectoralCommission, CampaignAction, AuditStatus, BlackMoneyPool, BlackMoneySource, CampaignError, CampaignExecution, execute_national_ad_campaign, execute_regional_rally, execute_television_campaign, execute_digital_campaign, generate_corporate_lobbying_black_money, generate_organized_crime_black_money, generate_money_laundering_black_money, process_election_cycle, process_campaign_spending};
pub use conservation::{ConservationPolicy, ConservationPolicyType, ZoningRule, NationalPark, LandscapePark, create_national_park, create_landscape_park, process_conservation_turn};
pub use chaos_config::ChaosConfig;
pub use mass_movements::{MassMovement, MassMovementType, MassMovementStatus, check_mass_movement_spawn, apply_mass_movement_disruption, suppress_mass_movement, process_union_strike_fund, process_mass_movements_turn, SuppressionError, SuppressionResult, MovementError};
pub use traits::{LeaderTrait, TraitModifier, ModifierType, TraitRegistry, apply_leader_modifiers, process_leader_traits_turn};
pub use ministries::{GovernmentCompetency, BudgetPriorities, IdeologyBudgetPriorities, Ministry, MinistryAllocation, MinistryConfig, MinistrySpendingAction, form_government, allocate_cash_to_ministries, calculate_budget_needs, sum_ministry_allocations, prepare_minister_strategies, process_minister_post_clearing, migrate_legacy_budget};
pub use budget_lifecycle::{BudgetBill, BudgetAmendment, BudgetBillStage, process_budget_lifecycle, process_budget_amendments, draft_budget_bill, apply_budget_failure_consequence, BudgetFailureConsequence};
pub use laws::{HealthcareLaw, EducationLaw, LawType, enact_law, MigrationLaw, DeportationPolicy, BorderState, CustomsState, MigrationFlow, MigrationReason, InspectorateState, Violation, ViolationType};
pub use free_speech::{FreeSpeechLaw, FreeSpeechLevel, AssemblyRights, PressFreedom};
pub use names::{NamePool, VipName, generate_person_name, generate_full_vip, name_pool_for_culture, vip_to_leader};
pub use parliament::{Parliament, Chamber, ChamberPresidium, NamedVip, VipRole, ParliamentaryClub, VoteRecord, SplinterEvent, StateOfEmergency, initialize_parliament, check_faction_splintering};
pub use vip_registry::{Vip, VipRegistry, VipRoleExtended, IncapacityStatus, DeathCause, PendingDeath, age_health_degradation, death_probability, assign_core_traits, CORE_TRAITS};
pub use succession::{RoyalDynasty, RoyalFamilyMember, RoyalRelation, SuccessionOutcome, RegentBehavior, regent_behavior};
pub use advisory_council::{AdvisoryCouncil, CouncilMember, CouncilType, CouncilOpinion, FactionType, calculate_council_opinion, apply_decree_against_council};
pub use local_legislation::{UnfundedMandate, MandateFundingDecision, LocalBill, LocalProvision, LocalBillStage, vote_on_mandate_funding};
pub use legislation::{BillProvision, SunsetProvision, enact_bill, process_sunset_expirations};
