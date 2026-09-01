mod commands;
mod state;

use sim_engine::registries::Registries;
use state::AppState;
use std::sync::Arc;

fn main() {
    let registries = Registries::native_only();
    let data_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("../state/data");

    let app_state = AppState {
        engine: Arc::new(tokio::sync::RwLock::new(None)),
        registries,
        data_dir,
        processing: Arc::new(tokio::sync::RwLock::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::actions::new_game,
            commands::actions::advance_turn,
            commands::actions::save_game,
            commands::actions::load_game,
            commands::actions::list_saves,
            commands::actions::get_game_status,
            commands::macro_queries::get_macro_indicators,
            commands::macro_queries::get_treasury,
            commands::macro_queries::get_finance,
            commands::macro_queries::get_commodities,
            commands::macro_queries::get_sectors,
            commands::vip_queries::get_paginated_vips,
            commands::vip_queries::get_vip_dossier,
            commands::vip_queries::get_available_roles,
            commands::company_queries::get_paginated_companies,
            commands::company_queries::get_company_detail,
            commands::company_queries::get_available_sectors,
            commands::company_queries::get_available_regions,
            commands::governance_queries::get_governance_detail,
            commands::securities_queries::get_stock_exchange,
            commands::securities_queries::get_listed_companies,
            commands::securities_queries::get_company_market_detail,
            commands::securities_queries::get_funds,
            commands::securities_queries::get_fund_detail,
            commands::securities_queries::get_knf_findings,
            commands::securities_queries::get_capital_gains_summary,
            commands::bank_queries::get_paginated_banks,
            commands::bank_queries::get_banking_aggregates,
            commands::bank_queries::get_banking_history,
            commands::region_queries::get_regions,
            commands::region_queries::get_region_detail,
            commands::region_queries::get_megaregion_detail,
            commands::parliament_queries::get_parliament,
            commands::parliament_queries::get_government,
            // Phase 60: Cadastre / Land / Courts
            commands::cadastre_queries::get_cadastre_summary,
            commands::cadastre_queries::get_zoning_plans,
            commands::cadastre_queries::get_court_backlog,
            commands::cadastre_queries::get_arbitration_cases,
            commands::cadastre_queries::get_ministry_land_report,
            // Phase 66: Diplomacy & Fog of War
            commands::diplomacy_queries::get_diplomacy_snapshot,
            commands::diplomacy_queries::get_foreign_countries,
            commands::diplomacy_queries::assign_diplomat,
            commands::diplomacy_queries::recall_diplomat,
            commands::diplomacy_queries::expel_diplomat,
            commands::diplomacy_queries::send_economic_aid,
            commands::diplomacy_queries::border_provocation,
            // Phase 67: Treaties & reputation
            commands::diplomacy_queries::get_active_treaties,
            commands::diplomacy_queries::propose_treaty,
            commands::diplomacy_queries::sign_treaty,
            commands::diplomacy_queries::abrogate_treaty,
            // Phase 68: Organizations & sanctions
            commands::diplomacy_queries::get_organizations,
            commands::diplomacy_queries::propose_sanction,
            commands::diplomacy_queries::lift_sanction,
            // Phase 73: Military & Crisis dashboard (read-only)
            commands::military_queries::get_military_dashboard,
            commands::energy_queries::get_energy_grid,
            // Phase 85: Factional domains, cottage industry, and guilds
            commands::phase85_queries::get_factional_domains_snapshot,
            commands::phase85_queries::get_guilds_snapshot,
            commands::phase85_queries::get_cities_snapshot,
            commands::phase85_queries::get_municipal_ai_snapshot,
            commands::phase85_queries::get_organizations_snapshot,
            commands::phase85_queries::get_organization_detail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
