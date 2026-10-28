use crate::state::AppState;
use sim_engine::ui::snapshot_map::WorldMapSnapshot;

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
