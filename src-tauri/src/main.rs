mod commands;
mod state;

use std::sync::Arc;
use state::AppState;
use sim_engine::registries::Registries;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
