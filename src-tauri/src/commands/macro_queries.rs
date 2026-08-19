use crate::state::AppState;
use sim_engine::ui::snapshot::{
    build_country_snapshot, MacroIndicatorsResponse, TreasurySummary,
    FinanceSnapshot, CommodityRow, SectorRow, ViewQuery,
};

fn get_country_and_build(
    state: &AppState,
    country: &str,
) -> Result<sim_engine::ui::snapshot::CountrySnapshot, String> {
    let engine_guard = state.engine.blocking_read();
    let engine_state = engine_guard
        .as_ref()
        .ok_or("No game loaded")?;

    let country_ref = engine_state
        .game_state
        .countries
        .get(country)
        .ok_or(format!("Country '{}' not found", country))?;

    let entities = engine_state
        .turn_context
        .entities
        .get(country)
        .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
        .unwrap_or((&[], &[]));

    let snapshot = build_country_snapshot(
        country_ref,
        &engine_state.game_state.market_history,
        &engine_state.turn_context.market,
        entities.1,
        entities.0,
        &ViewQuery::default(),
    );

    Ok(snapshot)
}

#[tauri::command]
pub async fn get_macro_indicators(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<MacroIndicatorsResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let snap = get_country_and_build(&state_clone, &country)?;
        let md = &snap;
        Ok(MacroIndicatorsResponse {
            gdp: md.gdp_breakdown.official_gdp,
            gdp_per_capita: if md.treasury.population > 0 {
                md.gdp_breakdown.official_gdp / md.treasury.population as f64
            } else {
                0.0
            },
            population: md.treasury.population,
            unemployment_rate: md.labor.unemployment_rate,
            inflation_rate: 0.0,
            average_wage: md.labor.average_wage,
            money_supply_m0: md.money_supply.m0,
            money_supply_m3: md.money_supply.m3,
            consumption: md.gdp_breakdown.consumption,
            investment: md.gdp_breakdown.investment,
            government_spending: md.gdp_breakdown.government_spending,
            net_exports: md.gdp_breakdown.net_exports,
            cpi: md.inflation_indices.cpi_index,
            ppi: md.inflation_indices.ppi_index,
            deltas: md.deltas.clone(),
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_treasury(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<TreasurySummary, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let snap = get_country_and_build(&state_clone, &country)?;
        Ok(snap.treasury)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_finance(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<FinanceSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let snap = get_country_and_build(&state_clone, &country)?;
        Ok(snap.finance)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_commodities(
    state: tauri::State<'_, AppState>,
    country: String,
    show_inactive: bool,
) -> Result<Vec<CommodityRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let snap = get_country_and_build(&state_clone, &country)?;
        if show_inactive {
            Ok(snap.commodities)
        } else {
            Ok(snap.commodities.into_iter().filter(|c| c.active).collect())
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_sectors(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<SectorRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let snap = get_country_and_build(&state_clone, &country)?;
        Ok(snap.sectors)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
