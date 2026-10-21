use crate::state::AppState;
use sim_engine::ui::snapshot::{
    build_constitution_laws, build_country_snapshot, build_elections,
    ConstitutionLawsResponse, ElectionsResponse, GovernmentSnapshot, ParliamentResponse, ViewQuery,
};

/// v4.19 WS4: Constitution parameters + active statutes for the
/// "Constitution & Laws" sub-tab.
#[tauri::command]
pub async fn get_constitution_laws(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<ConstitutionLawsResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;
        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;
        Ok(build_constitution_laws(country_ref))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.19 WS4: Elections & Campaigns view — countdown, party standings,
/// campaign actions, current seat distribution.
#[tauri::command]
pub async fn get_elections(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<ElectionsResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;
        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;
        let current_year = engine_state.game_state.calendar.current_year;
        Ok(build_elections(country_ref, current_year))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_parliament(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<ParliamentResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let snap = build_country_snapshot(
            country_ref,
            &engine_state.game_state.market_history,
            &engine_state.turn_context.market,
            entities.1,
            entities.0,
            &ViewQuery::default(),
        );

        Ok(ParliamentResponse {
            parliament: snap.parliament,
            advisory_council: snap.advisory_council,
            royal_dynasty: snap.royal_dynasty,
            government_form: snap.government_form,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_government(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<GovernmentSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let snap = build_country_snapshot(
            country_ref,
            &engine_state.game_state.market_history,
            &engine_state.turn_context.market,
            entities.1,
            entities.0,
            &ViewQuery::default(),
        );

        Ok(snap.government)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
