use crate::state::AppState;
use sim_engine::ui::snapshot::CountryTradeTreemapDto;
use sim_engine::ui::snapshot_press::PressSnapshotDto;
use sim_engine::ui::snapshot_society::{
    GdpCompositionDto, OrganizationsSummaryDto, SocietySnapshotDto,
};

/// v4.22 WS4: GDP expenditure composition (national C+I+G+NX, per-region
/// attribution, ~24-turn history) for one country.
#[tauri::command]
pub async fn get_gdp_composition(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<GdpCompositionDto, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{country}' not found"))?;

        let empty = std::collections::HashMap::new();
        let regional_flows = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| &e.gdp_flows_by_region)
            .unwrap_or(&empty);

        Ok(sim_engine::ui::snapshot_society::build_gdp_composition(
            country_ref,
            regional_flows,
        ))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.23 WS4: per-country trade treemap — REAL settled trade legs this
/// turn (order-book legs classified by endpoint ownership + the
/// FOREIGN-SECTOR export/import sweeps + domestic state procurement),
/// SITC-grouped for the Exports/Imports/Domestic toggle on the Market
/// page. Replaces the v4.22 global cleared-volume bound.
#[tauri::command]
pub async fn get_trade_treemap(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<CountryTradeTreemapDto, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{country}' not found"))?;

        Ok(sim_engine::ui::snapshot::build_country_trade_treemap(
            country_ref,
            engine_state.game_state.calendar.current_year,
        ))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.22 WS6: Organizations summary — unionization, guilds, movements,
/// radicalization — for EVERY country (the Organizations page is global).
/// One `OrganizationsSummaryDto` per country, name-sorted (deterministic).
#[tauri::command]
pub async fn get_organizations_summary(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<OrganizationsSummaryDto>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let mut names: Vec<&String> = engine_state.game_state.countries.keys().collect();
        names.sort();
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let country = &engine_state.game_state.countries[name];
            let (companies, unions) = engine_state
                .turn_context
                .entities
                .get(name)
                .map(|e| (e.companies.as_slice(), e.unions.as_slice()))
                .unwrap_or((&[], &[]));
            out.push(
                sim_engine::ui::snapshot_society::build_organizations_summary(
                    country, companies, unions,
                ),
            );
        }
        Ok(out)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.22 WS6: Society snapshot — real demographic aggregates (cohorts,
/// education, ethnicity, religion, vital telemetry, class living rows).
#[tauri::command]
pub async fn get_society_snapshot(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<SocietySnapshotDto, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{country}' not found"))?;

        Ok(sim_engine::ui::snapshot_society::build_society_snapshot(
            country_ref,
        ))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.22 WS6: Press snapshot — deterministic newspaper front pages built
/// from real events (legislative log, council resolutions, royal genealogy,
/// strikes, movements, treaties, war state, telemetry deltas).
///
/// v4.24 WS1: the builder is wrapped in `catch_unwind` — a panic used to
/// surface only as an opaque "Task join error" which the Press page rendered
/// as "Loading…" forever. Panics now return a descriptive error payload so
/// the page's error state can display (and log) the real cause.
#[tauri::command]
pub async fn get_press_snapshot(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<PressSnapshotDto, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{country}' not found"))?;

        let (companies, unions) = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| (e.companies.as_slice(), e.unions.as_slice()))
            .unwrap_or((&[], &[]));

        let snapshot = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sim_engine::ui::snapshot_press::build_press_snapshot(
                country_ref,
                companies,
                unions,
                &engine_state.game_state.treaty_registry,
                engine_state.game_state.calendar.global_turn,
                engine_state.game_state.calendar.current_year,
            )
        }));
        snapshot.map_err(|panic| {
            let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            format!("Press snapshot failed: {msg}")
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
