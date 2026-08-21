//! Military units and combat system

pub mod combat;
pub mod config;
pub mod fronts;
pub mod turn;
pub mod units;
pub mod upkeep;
pub mod fleet;
pub mod war_economy;
pub mod oob;
pub mod modernization;
pub mod multi_domain;
pub mod pows;
pub mod retreat;
pub mod commander_traits;
pub mod war_declarations;
pub mod occupation;
pub mod morale;
pub mod propaganda;
pub mod proxy_wars;

pub use combat::{resolve_battle, process_wounded, process_dead, process_deserters};
pub use config::MilitaryCombatConfig;
pub use fronts::{Front, RegionControl, Battle, BattleResult, Casualties};
pub use turn::process_military_turn;
pub use units::{MilitaryUnit, UnitType, UnitStats, PeasantBattalion, EquipmentReserve};
pub use upkeep::{process_military_upkeep, add_military_demand_to_market, add_fleet_demand_to_market, submit_defense_b2b_orders, deliver_military_supplies, degrade_military_equipment, deliver_military_supplies_and_equipment};
pub use fleet::{Fleet, Ship, FleetMission, apply_maritime_capacity_constraint, create_fleet, process_fleet_upkeep};
pub use war_economy::{
    WarEconomyState, WarEconomyConfig, ConscriptionLevel, ProductionDecree,
    apply_production_decree, lift_production_decree, process_expired_decrees,
    execute_conscription, demobilize_unit, issue_war_bonds,
    MilitaryConversion, military_conversion_methods, find_military_conversion,
    conversions_for_sector,
};
pub use oob::{
    OrderOfBattle, Army, Division, Regiment,
    OobGenerationConfig, generate_oob, generate_asymmetric_oob,
};
pub use modernization::{
    ModernizationConfig, EquipmentUpgrade, ModernizationResult,
    modernize_unit, available_upgrades, apply_scrap_to_stockpile,
};
pub use multi_domain::{
    CombatDomain, DomainModifiers, MultiDomainBattleResult,
    resolve_multi_domain_battle,
};
pub use pows::{
    PrisonerOfWar, PowStatus, PowCamp, PowCaptureConfig,
    ForcedLaborLeaseResult,
    capture_pows_from_casualties, calculate_lease_fee_per_pow,
    process_forced_labor_lease_fees, repatriate_pows_from_country,
};
pub use retreat::{
    CommanderRetraitProfile, RetreatEvaluation, RetreatResult,
    evaluate_retreat, process_retreat, apply_captured_equipment_to_stockpile,
};
pub use commander_traits::{
    MilitaryTacticModifiers, AirDoctrine,
    evaluate_military_tactics,
    apply_attack_modifier, apply_defense_modifier, apply_organization_modifier,
    to_retreat_profile,
};
pub use war_declarations::{
    WarReason, PeaceTerms, WarState, BilateralTension, WarDeclarationConfig,
    WarDeclarationResult, PeaceSettlementResult,
    declare_war, check_tension_escalations, decay_all_tensions,
    settle_peace, tension_key,
};
pub use occupation::{
    OccupationState, OccupationConfig, OccupationTurnResult,
    compute_cultural_distance, process_occupation_turn, create_occupation_states,
};
pub use morale::{
    MoraleConfig, MoraleImpactResult,
    apply_casualty_morale_impact, apply_casualty_morale_to_classes,
    recover_morale, recover_morale_for_classes,
    strike_production_factor, calculate_desertions, initialize_morale,
};
pub use propaganda::{
    PropagandaTarget, PropagandaCampaign, PropagandaConfig, PropagandaResult,
    execute_propaganda, apply_propaganda_boost,
};
pub use proxy_wars::{
    ProxyWarAction, ProxyWarResult, ProxyWarConfig,
    fund_separatists, arm_rebels,
};
pub use crate::infrastructure::maritime::ShipType;
