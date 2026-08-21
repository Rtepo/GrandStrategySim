use crate::state::AppState;
use sim_engine::ui::snapshot::MilitaryDashboardResponse;

/// Phase 73: Get the full military dashboard snapshot.
/// Read-only — no manual action endpoints.
#[tauri::command]
pub async fn get_military_dashboard(
    state: tauri::State<'_, AppState>,
) -> Result<MilitaryDashboardResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let snapshot = sim_engine::ui::snapshot::build_military_dashboard(
            &engine_state.game_state,
        );
        Ok(snapshot)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
