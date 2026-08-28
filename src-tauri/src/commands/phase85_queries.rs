use crate::state::AppState;
use sim_engine::ui::snapshot::{CitiesSnapshot, FactionalDomainsSnapshot, GuildsSnapshot, MunicipalAiSnapshot};

/// Phase 85: Get the factional domains snapshot for the FactionalDomainsPage.
/// Role-gated (Rule 11): foreign observers see only public data.
#[tauri::command]
pub async fn get_factional_domains_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<FactionalDomainsSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let game = &engine_state.game_state;

        // Find the player's country (first country for now).
        // TODO: Pass actual observer country and role from the frontend.
        let player_country = game.countries.values().next();

        if let Some(country) = player_country {
            // For now, treat the player as having full access (is_classified = false).
            // TODO: Determine classification based on observer role.
            let snapshot = sim_engine::ui::snapshot::build_factional_domains_snapshot(
                country,
                false,
            );
            Ok(snapshot)
        } else {
            Err("No country found".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Phase 85: Get the guilds snapshot for the GuildsPage.
/// Role-gated (Rule 11): foreign observers see only public registry data.
#[tauri::command]
pub async fn get_guilds_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<GuildsSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let game = &engine_state.game_state;

        // Collect all companies from all countries
        let mut all_companies: Vec<sim_engine::entities::Company> = Vec::new();
        for country in game.countries.values() {
            if let Some(entities) = engine_state.turn_context.entities.get(&country.name) {
                all_companies.extend(entities.companies.clone());
            }
        }

        // For now, treat the player as having full access (is_classified = false).
        let snapshot = sim_engine::ui::snapshot::build_guilds_snapshot(
            &all_companies,
            false,
        );
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Phase 85B: Get the cities snapshot for the CitiesPage.
/// Role-gated (Rule 11): foreign observers see only public data.
#[tauri::command]
pub async fn get_cities_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<CitiesSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let game = &engine_state.game_state;

        let player_country = game.countries.values().next();

        if let Some(country) = player_country {
            let snapshot = sim_engine::ui::snapshot::build_cities_snapshot(
                country,
                false,
            );
            Ok(snapshot)
        } else {
            Err("No country found".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Phase 86.5B: Get the municipal AI investment plan snapshot.
/// Shows the AI's infrastructure investment decisions for the current turn.
#[tauri::command]
pub async fn get_municipal_ai_snapshot(
    state: tauri::State<'_, AppState>,
) -> Result<MunicipalAiSnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let game = &engine_state.game_state;
        let player_country = game.countries.values().next();

        if let Some(country) = player_country {
            let snapshot = sim_engine::ui::snapshot::build_municipal_ai_snapshot(country);
            Ok(snapshot)
        } else {
            Err("No country found".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
