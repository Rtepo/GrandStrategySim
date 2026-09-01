use crate::state::AppState;
use sim_engine::ui::snapshot::EnergyGridSnapshot;

/// Phase 81: Get the energy grid snapshot for the Energy dashboard.
/// Role-gated: foreign observers see only public aggregate data.
#[tauri::command]
pub async fn get_energy_grid(
    state: tauri::State<'_, AppState>,
) -> Result<EnergyGridSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let game = &engine_state.game_state;

        // Find the player's country (first country for now).
        // TODO: Pass actual observer country and role from the frontend.
        let player_country_name = game.countries.keys().next().cloned();
        let player_country = game.countries.values().next();

        if let Some(country) = player_country {
            // Get buildings for this country from the turn context.
            let buildings: Vec<sim_engine::entities::Building> = engine_state
                .turn_context
                .entities
                .get(&country.name)
                .map(|ce| ce.buildings.clone())
                .unwrap_or_default();

            let snapshot = sim_engine::ui::snapshot::build_energy_grid_snapshot(
                country,
                &buildings,
                player_country_name.as_deref(),
                None,
            );
            Ok(snapshot)
        } else {
            Err("No country found".to_string())
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
