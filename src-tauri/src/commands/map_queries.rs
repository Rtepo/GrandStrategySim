use crate::state::AppState;
use sim_engine::ui::snapshot_map::{RegionParcelMapDto, WorldMapSnapshot};

/// v4.24.0 WS5: Deterministic procedural world-map snapshot — per-country
/// region polygons, adjacency edges, war/front overlays, devastation,
/// GDP-per-capita and population stats for the Map tab and mini-map embeds.
#[tauri::command]
pub async fn get_world_map(
    state: tauri::State<'_, AppState>,
) -> Result<WorldMapSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let snapshot =
            sim_engine::ui::snapshot_map::build_world_map(&engine_state.game_state);
        Ok(snapshot)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// v4.26.0 TASK-002: Regional cadastral parcel grid — one zoning-classed
/// quad per `ParcelChunk` in the region, rendered by RegionsPage when a
/// region is selected.
#[tauri::command]
pub async fn get_region_parcel_map(
    state: tauri::State<'_, AppState>,
    country: String,
    region_id: String,
) -> Result<Option<RegionParcelMapDto>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        Ok(sim_engine::ui::snapshot_map::build_region_parcel_map(
            country_ref,
            &region_id,
        ))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
