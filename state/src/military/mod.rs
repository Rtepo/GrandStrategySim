//! Military units and combat system

pub mod combat;
pub mod commander_traits;
pub mod config;
pub mod fleet;
pub mod fronts;
pub mod modernization;
pub mod morale;
pub mod multi_domain;
pub mod occupation;
pub mod oob;
pub mod pows;
pub mod propaganda;
pub mod proxy_wars;
pub mod retreat;
pub mod turn;
pub mod units;
pub mod upkeep;
pub mod war_declarations;
pub mod war_economy;

pub use crate::infrastructure::maritime::ShipType;
pub use combat::{process_dead, process_deserters, process_wounded, resolve_battle};
pub use commander_traits::{
    apply_attack_modifier, apply_defense_modifier, apply_organization_modifier,
    evaluate_military_tactics, to_retreat_profile, AirDoctrine, MilitaryTacticModifiers,
};
pub use config::MilitaryCombatConfig;
pub use fleet::{
    apply_maritime_capacity_constraint, create_fleet, process_fleet_upkeep, Fleet, FleetMission,
    Ship,
};
pub use fronts::{Battle, BattleResult, Casualties, Front, RegionControl};
pub use modernization::{
    apply_scrap_to_stockpile, available_upgrades, modernize_unit, EquipmentUpgrade,
    ModernizationConfig, ModernizationResult,
};
pub use morale::{
    apply_casualty_morale_impact, apply_casualty_morale_to_classes, calculate_desertions,
    initialize_morale, recover_morale, recover_morale_for_classes, strike_production_factor,
    MoraleConfig, MoraleImpactResult,
};
pub use multi_domain::{
    resolve_multi_domain_battle, CombatDomain, DomainModifiers, MultiDomainBattleResult,
};
pub use occupation::{
    compute_cultural_distance, create_occupation_states, process_occupation_turn, OccupationConfig,
    OccupationState, OccupationTurnResult,
};
pub use oob::{
    generate_asymmetric_oob, generate_oob, Army, Division, OobGenerationConfig, OrderOfBattle,
    Regiment,
};
pub use pows::{
    calculate_lease_fee_per_pow, capture_pows_from_casualties, process_forced_labor_lease_fees,
    repatriate_pows_from_country, ForcedLaborLeaseResult, PowCamp, PowCaptureConfig, PowStatus,
    PrisonerOfWar,
};
pub use propaganda::{
    apply_propaganda_boost, execute_propaganda, PropagandaCampaign, PropagandaConfig,
    PropagandaResult, PropagandaTarget,
};
pub use proxy_wars::{
    arm_rebels, fund_separatists, ProxyWarAction, ProxyWarConfig, ProxyWarResult,
};
pub use retreat::{
    apply_captured_equipment_to_stockpile, evaluate_retreat, process_retreat,
    CommanderRetraitProfile, RetreatEvaluation, RetreatResult,
};
pub use turn::process_military_turn;
pub use units::{EquipmentReserve, MilitaryUnit, PeasantBattalion, UnitStats, UnitType};
pub use upkeep::{
    add_fleet_demand_to_market, add_military_demand_to_market,
    calculate_total_military_equipment_volume, degrade_military_equipment,
    deliver_military_supplies, deliver_military_supplies_and_equipment, process_military_upkeep,
    process_mod_storage_costs, submit_defense_b2b_orders, ModStorageResult,
};
pub use war_declarations::{
    check_tension_escalations, decay_all_tensions, declare_war, settle_peace, tension_key,
    BilateralTension, PeaceSettlementResult, PeaceTerms, WarDeclarationConfig,
    WarDeclarationResult, WarReason, WarState,
};
pub use war_economy::{
    apply_production_decree, conversions_for_sector, demobilize_unit, execute_conscription,
    find_military_conversion, issue_war_bonds, lift_production_decree, military_conversion_methods,
    process_expired_decrees, ConscriptionLevel, MilitaryConversion, ProductionDecree,
    WarEconomyConfig, WarEconomyState,
};
