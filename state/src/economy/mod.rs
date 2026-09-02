//! Economic turn simulation.
//!
//! This module hosts the deterministic per-country economy step, including
//! GDP share updates, infrastructure effects, and market clearing.
//!
//! Phase 24B: Files have been reorganized into subdirectories.

// Subdirectory declarations (Phase 24B)
pub mod config;
pub mod finance;
pub mod justice;
pub mod labor;
pub mod logistics;
pub mod market;
pub mod production;
pub mod religion;
pub mod society;
pub mod state_sector;
pub mod trade;

// Modules that remain at the top level (not moved in Phase 24B)
pub mod banking_history;
pub mod corporate_rd;
pub mod cottage_industry;
pub mod guild_system;
pub mod indicators;
#[cfg(test)]
mod phase85_tests;

#[cfg(test)]
mod phase85b_tests;
pub mod real_estate;
pub mod telemetry;

// Backward-compatible re-exports: make `crate::economy::<file>` resolve to the
// moved module inside its subdirectory. This preserves all existing use statements.
// Note: files whose name matches their subdirectory (market.rs, production.rs,
// labor.rs, logistics.rs) are re-exported via `pub use <self>::*;` in the
// subdirectory's mod.rs, so they don't need a re-export here.
pub use config::{
    b2b_config, corporate_config, generative_goods_config, innovation_config, service_config,
};
pub use finance::{debt_market, payment_in_kind};
pub use justice::{
    bribery, civil_lawsuits, inspectorate_fleet, inspectorates, justice_system, legal_status,
    prison_labor, sentencing,
};
pub use labor::{assimilation, labor_market, migration};
pub use logistics::{commuting, transport_networks};
pub use market::{clearing, market_history, order_book};
pub use production::{disasters, fixed_assets, geology, maintenance, weather};
pub use religion::{media, propaganda, religious_economy};
pub use society::ethnic_violence;
pub use state_sector::{
    fishing, fishing_config, infrastructure, infrastructure_config, osp, smuggling, state_forests,
    state_research,
};
pub use trade::{
    b2b_orders, b2c_services, blueprints, innovation_trading, retail, retail_registry, royalties,
    transfer_settler, wholesale,
};

pub use assimilation::{
    process_assimilation_turn, process_religious_conversion_turn, AssimilationTurnResult,
    ConversionTurnResult,
};
pub use b2b_config::B2bOrderConfig;
pub use b2b_orders::{
    calculate_dynamic_markup, calculate_unit_cost, compute_company_inventory,
    execute_production_cycle, refund_unfilled_bids as refund_unfilled_b2b_bids,
    refund_unfilled_defense_bids_per_country, settle_defense_trades,
    settle_maintenance_service_trades, settle_trades, settle_trades_with_tariffs,
    submit_company_b2b_orders, submit_fixed_asset_purchase_bids, submit_maintenance_service_bids,
};
pub use b2c_services::{
    clear_education_slots_b2c, clear_health_capacity_b2c, clear_passenger_transport_b2c,
    populate_commute_service_needs, populate_education_service_needs,
    populate_health_service_needs,
};
pub use blueprints::{
    compute_blueprint_royalty_fee, design_blueprint, design_score, CrossBorderRoyaltyQueueEntry,
    LicensedBlueprint, ProductBlueprint,
};
pub use clearing::resolve_market_prices;
pub use commuting::{
    build_commute_map, can_afford_commute, compute_commute_demand_for_target, CommuteOption,
    CommuterFteEntry, CommutingConfig, TransportLaw, TransportOwnership,
};
pub use corporate_config::CorporateTechConfig;
pub use corporate_rd::{
    allocate_corporate_rd_budget, check_patent_expiration, evaluate_licensing_opportunities,
    execute_corporate_method_research,
};
pub use debt_market::{
    clear_arrears, clear_savings_bonds_b2c, clear_secondary_debt_market, issue_treasury_securities,
    process_debt_service, CouponFrequency, CreditRating, DebtMarket, DebtOrder, DebtOrderType,
    DefaultEvent, SavingsBond, SecondaryMarketState, SecurityHolder, SecurityHolderType,
    TreasurySecurity, TreasurySecurityType,
};
pub use disasters::{
    check_disaster_triggers, sum_fire_protection_capacity, sum_shelter_capacity, DisasterEvent,
    DisasterTurnResult, DisasterType,
};
pub use ethnic_violence::{check_pogrom_triggers, PogromConfig, PogromResult};
pub use fishing::{
    create_fish_farm, create_fish_stock, process_fishing_turn, FishFarm, FishFarmType, FishStock,
    FishingPolicy, FishingPolicyType, FishingQuota, FishingQuotaType, FishingTreaty,
};
pub use fishing_config::FishingConfig;
pub use fixed_assets::{
    compact_cohorts, compact_inventory_cohorts, degrade_cohorts, draft_animal_maintenance_needed,
    feed_draft_animals, install_fixed_asset, machine_unit_capacity_for_commodity, machinery_factor,
    maintenance_services_needed, obsolescence_factor, remove_scrapped, restore_cohort_condition,
    FixedAssetCohort, InventoryCohort,
};
pub use generative_goods_config::GenerativeGoodsConfig;
pub use indicators::{run_economic_turn, update_gdp_shares_from_employment};
pub use infrastructure::{
    allocate_owner_infrastructure_funding, execute_infrastructure_production,
    submit_infrastructure_procurement_orders,
};
pub use infrastructure_config::InfrastructureConfig;
pub use innovation_config::InnovationConfig;
pub use innovation_trading::trade_innovation_points_b2b;
pub use inspectorates::{process_inspectorates_turn, InspectorateTurnResult};
pub use justice_system::{calculate_national_demand, process_justice_turn, JusticeTurnResult};
pub use labor::process_demographics_and_labor;
pub use legal_status::{
    process_amnesty_turn, process_shadow_economy_turn, AmnestyLaw, AmnestyTurnResult, LegalStatus,
    ShadowEconomyState, ShadowEconomyTurnResult, ShadowEmployment,
};
pub use logistics::{
    assign_geographic_traits_from_edges, compute_freight_route, expire_deferred_trades,
    freight_capacity_required, freight_cost, increment_deferral_counters,
    procure_freight_and_split_trades, DeferredReason, DeferredTrade, FreightLogisticsConfig,
    FreightRoute,
};
pub use maintenance::{
    process_condition_degradation, process_maintenance_spending, MaintenanceConfig,
};
pub use media::{clear_information_b2c, populate_information_service_needs, InformationB2cResult};
pub use migration::{
    apply_migration_flows, calculate_emigrants, calculate_migration_pressure,
    collect_migration_flows, process_deportations, sum_border_enforcement_capacity,
    MigrationConfig,
};
pub use osp::{is_osp, process_osp_volunteer_allocation};
pub use payment_in_kind::{apply_payment_in_kind, InKindLedger, NutritionalDeficit};
pub use prison_labor::process_prison_labor_turn;
pub use production::{process_building_cycle, process_building_cycle_with_geology};
pub use propaganda::{
    check_terrorism_triggers, compute_intelligence_state, compute_propaganda_subsidy_rate,
    process_propaganda_turn, MediaState, PropagandaCampaign, PropagandaConfig,
    PropagandaTurnResult, PropagandaType, TerrorismTurnResult,
};
pub use real_estate::{
    accrue_retail_rents, calculate_diversity_bonus, sign_retail_leases, update_anchor_tenant,
};
pub use religious_economy::{
    process_church_fund, process_monastery_production, process_see_reinvestment,
    process_see_remittance, ApostolicSeeConfig, ChurchFundResult, SeeReinvestmentResult,
    SeeRemittanceResult,
};
pub use retail::{
    apply_rationing_to_demand, build_consumer_demand, calculate_retail_price, clear_b2c_markets,
    generate_store_offers, settle_b2c_clearing, B2cClearingResult, ConsumerDemand, StoreOffer,
};
pub use retail_registry::{
    commodity_profile_map, is_compatible, retail_config, retail_upgrade_bonus,
};
pub use royalties::{
    calculate_royalty_fulfillment_ratio, integrate_royalty_payments, process_all_royalty_payments,
    process_blueprint_royalty_payments, process_cross_border_royalty_queue,
    process_royalty_payment,
};
pub use sentencing::{
    can_execute_state_action, check_vigilante_justice, compute_garnishment_rates,
    determine_crime_category, generate_sentence, process_death_penalties, process_ombudsman_turn,
    AdministrativeCourtState, CrimeCategory, OmbudsmanState, OmbudsmanTurnResult, SentenceOutcome,
    SentencingLaw, VigilanteJusticeResult,
};
pub use service_config::ServicePricingConfig;
pub use smuggling::{
    process_customs_evasion_recovery, process_smuggling_turn, sum_customs_capacity,
    SmugglingTurnResult,
};
pub use state_forests::{
    create_default_state_forests, process_state_forests_turn, ForestDistrictState,
    ForestDistrictTract, ForestDistrictTurnResult,
};
pub use state_research::execute_state_research;
pub use transfer_settler::{
    company_liquid_cash, credit_company_by_id, debit_citizen_savings_region, settle_b2c_purchase,
    settle_company_to_company, settle_transfer, settle_transfer_to_treasury, settle_wage_payment,
    TransferError, TransferRecipient, TransferResult,
};
pub use transport_networks::{
    degrade_networks, process_network_maintenance, NetworkLevel, NetworkLink,
    TransportNetworkOverlay,
};
pub use weather::{
    get_region_weather_modifier, process_weather_turn, WeatherEvent, WeatherEventType,
    WeatherModifier, WeatherState,
};
pub use wholesale::{
    apply_clearance_discount, apply_consolidation, enforce_procurement_cap,
    reset_procurement_commitment, LogisticsConfig,
};

use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::registries::Registries;
use crate::state::Country;
use rustc_hash::FxHashMap;

/// Context required to execute one economic turn for a single country.
///
/// This uses the disjoint split-borrow pattern: a mutable borrow of the
/// per-country [`Country`] state and an immutable borrow of the shared
/// [`Registries`]. This lets the engine mutate `country.budget` and
/// `country.macro_indicators` while still reading static definitions from
/// `registries` (and later the global market) without aliasing.
///
/// # Rules
/// * `country` is the mutable per-country state.
/// * `registries` is read-only immutable game data.
/// * `turn` and `year` are duplicated from [`crate::state::GameState`]
///   storage until the engine loop is fully ported.
/// * `buildings` holds the in-memory sector snapshot for tax and OPEX calculations.
/// * `market_prices` maps commodity to the cleared local price.
#[derive(Debug)]
pub struct CountryTurnCtx<'a> {
    /// Canonical country name, used for trace labels.
    pub country_name: String,
    /// Zero-based turn counter.
    pub turn: u32,
    /// In-game year (e.g., 2020).
    pub year: u32,
    /// Immutable game registries (tech tree, PMs, buildings, government forms).
    pub registries: &'a Registries,
    /// Mutable country state to update.
    pub country: &'a mut Country,
    /// In-memory buildings for the current turn.
    pub buildings: Vec<Building>,
    /// Cleared local market prices by commodity.
    pub market_prices: FxHashMap<Commodity, f64>,
}
