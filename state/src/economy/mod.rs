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
pub mod indicators;
pub mod real_estate;
pub mod telemetry;

// Backward-compatible re-exports: make `crate::economy::<file>` resolve to the
// moved module inside its subdirectory. This preserves all existing use statements.
// Note: files whose name matches their subdirectory (market.rs, production.rs,
// labor.rs, logistics.rs) are re-exported via `pub use <self>::*;` in the
// subdirectory's mod.rs, so they don't need a re-export here.
pub use config::{b2b_config, corporate_config, generative_goods_config, innovation_config, service_config};
pub use finance::{debt_market, payment_in_kind};
pub use justice::{bribery, civil_lawsuits, inspectorates, inspectorate_fleet, justice_system, legal_status, prison_labor, sentencing};
pub use labor::{assimilation, labor_market, migration};
pub use logistics::{commuting, transport_networks};
pub use market::{clearing, market_history, order_book};
pub use production::{disasters, fixed_assets, geology, maintenance, weather};
pub use religion::{media, propaganda, religious_economy};
pub use society::ethnic_violence;
pub use state_sector::{fishing, fishing_config, infrastructure, infrastructure_config, osp, smuggling, state_forests, state_research};
pub use trade::{b2b_orders, b2c_services, blueprints, innovation_trading, retail, retail_registry, royalties, transfer_settler, wholesale};

pub use b2b_config::B2bOrderConfig;
pub use b2b_orders::{compute_company_inventory, calculate_dynamic_markup, calculate_unit_cost, submit_company_b2b_orders, settle_trades, settle_trades_with_tariffs, refund_unfilled_bids as refund_unfilled_b2b_bids, execute_production_cycle, settle_defense_trades, refund_unfilled_defense_bids_per_country, submit_maintenance_service_bids, settle_maintenance_service_trades, submit_fixed_asset_purchase_bids};
pub use b2c_services::{clear_education_slots_b2c, clear_health_capacity_b2c, populate_education_service_needs, populate_health_service_needs, clear_passenger_transport_b2c, populate_commute_service_needs};
pub use clearing::resolve_market_prices;
pub use corporate_config::CorporateTechConfig;
pub use corporate_rd::{allocate_corporate_rd_budget, check_patent_expiration, evaluate_licensing_opportunities, execute_corporate_method_research};
pub use fishing_config::FishingConfig;
pub use indicators::{run_economic_turn, update_gdp_shares_from_employment};
pub use infrastructure::{allocate_owner_infrastructure_funding, execute_infrastructure_production, submit_infrastructure_procurement_orders};
pub use infrastructure_config::InfrastructureConfig;
pub use innovation_config::InnovationConfig;
pub use innovation_trading::trade_innovation_points_b2b;
pub use labor::process_demographics_and_labor;
pub use payment_in_kind::{apply_payment_in_kind, InKindLedger, NutritionalDeficit};
pub use production::{process_building_cycle, process_building_cycle_with_geology};
pub use real_estate::{accrue_retail_rents, calculate_diversity_bonus, sign_retail_leases, update_anchor_tenant};
pub use retail::{build_consumer_demand, calculate_retail_price, clear_b2c_markets, settle_b2c_clearing, generate_store_offers, apply_rationing_to_demand, ConsumerDemand, StoreOffer, B2cClearingResult};
pub use retail_registry::{commodity_profile_map, is_compatible, retail_config, retail_upgrade_bonus};
pub use royalties::{calculate_royalty_fulfillment_ratio, integrate_royalty_payments, process_royalty_payment, process_all_royalty_payments, process_blueprint_royalty_payments, process_cross_border_royalty_queue};
pub use service_config::ServicePricingConfig;
pub use state_research::execute_state_research;
pub use wholesale::{apply_clearance_discount, apply_consolidation, enforce_procurement_cap, reset_procurement_commitment, LogisticsConfig};
pub use fishing::{FishStock, FishingQuota, FishingPolicy, FishingTreaty, FishFarm, FishFarmType, FishingPolicyType, FishingQuotaType, create_fish_stock, create_fish_farm, process_fishing_turn};
pub use debt_market::{DebtMarket, TreasurySecurity, TreasurySecurityType, CouponFrequency, SecurityHolder, SecurityHolderType, SavingsBond, SecondaryMarketState, DebtOrder, DebtOrderType, CreditRating, DefaultEvent, issue_treasury_securities, clear_savings_bonds_b2c, process_debt_service, clear_arrears, clear_secondary_debt_market};
pub use justice_system::{process_justice_turn, calculate_national_demand, JusticeTurnResult};
pub use prison_labor::process_prison_labor_turn;
pub use weather::{process_weather_turn, get_region_weather_modifier, WeatherState, WeatherEvent, WeatherEventType, WeatherModifier};
pub use maintenance::{process_condition_degradation, process_maintenance_spending, MaintenanceConfig};
pub use disasters::{check_disaster_triggers, sum_fire_protection_capacity, sum_shelter_capacity, DisasterEvent, DisasterType, DisasterTurnResult};
pub use osp::{process_osp_volunteer_allocation, is_osp};
pub use migration::{sum_border_enforcement_capacity, calculate_migration_pressure, calculate_emigrants, collect_migration_flows, apply_migration_flows, process_deportations};
pub use smuggling::{sum_customs_capacity, process_smuggling_turn, process_customs_evasion_recovery, SmugglingTurnResult};
pub use inspectorates::{process_inspectorates_turn, InspectorateTurnResult};
pub use state_forests::{process_state_forests_turn, create_default_state_forests, ForestDistrictState, ForestDistrictTract, ForestDistrictTurnResult};
pub use assimilation::{process_assimilation_turn, process_religious_conversion_turn, AssimilationTurnResult, ConversionTurnResult};
pub use religious_economy::{process_see_remittance, process_church_fund, process_see_reinvestment, process_monastery_production, ApostolicSeeConfig, SeeRemittanceResult, ChurchFundResult, SeeReinvestmentResult};
pub use ethnic_violence::{check_pogrom_triggers, PogromConfig, PogromResult};
pub use legal_status::{LegalStatus, ShadowEmployment, ShadowEconomyState, AmnestyLaw, process_shadow_economy_turn, process_remittances_turn, process_amnesty_turn, process_deportation_wealth_extraction, ShadowEconomyTurnResult, AmnestyTurnResult};
pub use sentencing::{CrimeCategory, SentenceOutcome, SentencingLaw, AdministrativeCourtState, OmbudsmanState, OmbudsmanTurnResult, VigilanteJusticeResult, determine_crime_category, generate_sentence, process_death_penalties, compute_garnishment_rates, can_execute_state_action, process_ombudsman_turn, check_vigilante_justice};
pub use media::{populate_information_service_needs, clear_information_b2c, InformationB2cResult};
pub use propaganda::{PropagandaConfig, MediaState, PropagandaCampaign, PropagandaType, PropagandaTurnResult, TerrorismTurnResult, process_propaganda_turn, check_terrorism_triggers, compute_intelligence_state, compute_propaganda_subsidy_rate};
pub use transfer_settler::{settle_transfer, settle_b2c_purchase, settle_transfer_to_treasury, settle_wage_payment, settle_company_to_company, debit_citizen_savings_region, credit_company_by_id, company_liquid_cash, TransferRecipient, TransferError, TransferResult};
pub use generative_goods_config::GenerativeGoodsConfig;
pub use blueprints::{ProductBlueprint, LicensedBlueprint, CrossBorderRoyaltyQueueEntry, design_blueprint, compute_blueprint_royalty_fee, design_score};
pub use fixed_assets::{FixedAssetCohort, machinery_factor, obsolescence_factor, degrade_cohorts, remove_scrapped, maintenance_services_needed, restore_cohort_condition, install_fixed_asset, compact_cohorts, draft_animal_maintenance_needed, feed_draft_animals};
pub use logistics::{FreightRoute, FreightLogisticsConfig, DeferredTrade, DeferredReason, compute_freight_route, freight_cost, freight_capacity_required, procure_freight_and_split_trades, expire_deferred_trades, increment_deferral_counters, assign_geographic_traits_from_edges};
pub use transport_networks::{NetworkLevel, NetworkLink, TransportNetworkOverlay, degrade_networks, process_network_maintenance};
pub use commuting::{CommuteOption, CommutingConfig, CommuterFteEntry, TransportLaw, TransportOwnership, build_commute_map, can_afford_commute, compute_commute_demand_for_target};

use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::registries::Registries;
use crate::state::Country;
use std::collections::HashMap;
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
