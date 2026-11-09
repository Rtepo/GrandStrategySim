use crate::state::{AppState, EngineState};
use sim_engine::engine::{
    generate_world, run_turn_in_memory, GenerateOptions, InMemoryTurnContext, StartYear,
};
use sim_engine::ui::snapshot::{GameStatus, TurnResult};
use std::panic::AssertUnwindSafe;

#[tauri::command]
pub async fn new_game(
    state: tauri::State<'_, AppState>,
    country_count: usize,
    start_year: String,
) -> Result<(), String> {
    let year = match start_year.as_str() {
        "1900" => StartYear::Y1900,
        "1925" => StartYear::Y1925,
        "1950" => StartYear::Y1950,
        "1975" => StartYear::Y1975,
        _ => StartYear::Y1950,
    };

    let options = GenerateOptions {
        country_count,
        start_year: year,
        seed: None,
    };

    let registries = state.registries.clone();
    let data_dir = state.data_dir.clone();

    let generated = tokio::task::spawn_blocking(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            generate_world(&data_dir, options, &registries)
        }));
        match result {
            Ok(Ok(generated)) => Ok(generated),
            Ok(Err(e)) => Err(format!("World generation failed: {e}")),
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic during world generation".to_string()
                };
                Err(format!("World generation panicked: {msg}"))
            }
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    let mut game_state = generated.state;
    let turn_context = InMemoryTurnContext::load_from_disk(&state.data_dir, &mut game_state)
        .map_err(|e| format!("Context load failed: {e}"))?;

    let mut engine = state.engine.write().await;
    *engine = Some(EngineState {
        game_state,
        turn_context,
    });

    Ok(())
}

#[tauri::command]
pub async fn advance_turn(state: tauri::State<'_, AppState>) -> Result<TurnResult, String> {
    {
        let mut processing = state.processing.write().await;
        if *processing {
            return Err("A turn is already being processed".to_string());
        }
        *processing = true;
    }

    let engine_arc = state.engine.clone();
    let registries = state.registries.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut engine_guard = engine_arc.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        run_turn_in_memory(
            &mut engine_state.game_state,
            &registries,
            &mut engine_state.turn_context,
        )
        .map_err(|e| format!("Turn failed: {e:?}"))?;

        let turn = engine_state.game_state.calendar.global_turn;
        let year = engine_state.game_state.calendar.current_year;

        // v4.22 WS4: append the finalized GDP expenditure breakdown to each
        // country's `gdp_composition_history` ring buffer (the turn loop is
        // owned by another workstream, so this buffer is appended at the
        // post-turn boundary; `push_gdp_composition_sample` dedups on turn).
        for country in engine_state.game_state.countries.values_mut() {
            country
                .macro_indicators
                .push_gdp_composition_sample(turn, year);
        }
        Ok::<TurnResult, String>(TurnResult {
            turn,
            year,
            status: "ok".to_string(),
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    // v4.32.0 (TASK-033): Autosave — after a successful turn, if autosave is
    // enabled and the interval boundary was crossed, write a rotating
    // `autosave-1`/`autosave-2` slot so a crash mid-write can never destroy
    // the only backup.
    if let Ok(ref tr) = result {
        let cfg = state.autosave.read().await.clone();
        if cfg.enabled && tr.turn > 0 && tr.turn % cfg.interval_turns == 0 {
            let engine_arc = state.engine.clone();
            let saves_dir = state.data_dir.join("saves");
            let slot = if (tr.turn / cfg.interval_turns) % 2 == 0 {
                "autosave-1"
            } else {
                "autosave-2"
            }
            .to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = tokio::task::spawn_blocking(move || {
                let engine_guard = engine_arc.blocking_read();
                if let Some(engine_state) = engine_guard.as_ref() {
                    let _ = sim_engine::io::runtime_save::write_runtime_save(
                        &saves_dir,
                        &slot,
                        &engine_state.game_state,
                        &engine_state.turn_context,
                        now,
                        true,
                    );
                }
            })
            .await;
        }
    }

    {
        let mut processing = state.processing.write().await;
        *processing = false;
    }

    result
}

/// v4.32.0 (TASK-033): Write a full runtime save — the ENTIRE GameState plus
/// the complete InMemoryTurnContext (market, diplomacy, all entities) as a
/// gzip-compressed `.gss` file plus a `.meta.json` sidecar for the slot list.
#[tauri::command]
pub async fn save_game(state: tauri::State<'_, AppState>, save_name: String) -> Result<(), String> {
    let engine_arc = state.engine.clone();
    let saves_dir = state.data_dir.join("saves");

    tokio::task::spawn_blocking(move || {
        let engine_guard = engine_arc.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        sim_engine::io::runtime_save::write_runtime_save(
            &saves_dir,
            &save_name,
            &engine_state.game_state,
            &engine_state.turn_context,
            now,
            false,
        )
        .map_err(|e| format!("Failed to save: {e}"))?;

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// v4.32.0 (TASK-033): Restore a `.gss` runtime save — GameState AND the full
/// turn context come back exactly as saved (no bootstrap re-load, so
/// companies/buildings/market state are preserved).
#[tauri::command]
pub async fn load_game(state: tauri::State<'_, AppState>, save_name: String) -> Result<(), String> {
    let saves_dir = state.data_dir.join("saves");

    let save = tokio::task::spawn_blocking(move || {
        sim_engine::io::runtime_save::load_runtime_save(&saves_dir, &save_name)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
    .map_err(|e| format!("Failed to load save: {e}"))?;

    let mut engine = state.engine.write().await;
    *engine = Some(EngineState {
        game_state: save.game_state,
        turn_context: save.turn_context,
    });

    Ok(())
}

/// v4.32.0 (TASK-033): List save slots with turn/date/country metadata.
#[tauri::command]
pub async fn list_saves(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sim_engine::io::runtime_save::SaveMeta>, String> {
    let saves_dir = state.data_dir.join("saves");
    Ok(sim_engine::io::runtime_save::list_save_slots(&saves_dir))
}

/// v4.32.0 (TASK-033): Read the autosave configuration.
#[tauri::command]
pub async fn get_autosave_settings(
    state: tauri::State<'_, AppState>,
) -> Result<crate::state::AutosaveSettings, String> {
    Ok(state.autosave.read().await.clone())
}

/// v4.32.0 (TASK-033): Update the autosave configuration (persisted to disk).
#[tauri::command]
pub async fn set_autosave_settings(
    state: tauri::State<'_, AppState>,
    enabled: bool,
    interval_turns: u32,
) -> Result<(), String> {
    let interval = match interval_turns {
        4 | 12 | 24 => interval_turns,
        _ => 24,
    };
    {
        let mut cfg = state.autosave.write().await;
        cfg.enabled = enabled;
        cfg.interval_turns = interval;
    }
    let cfg = state.autosave.read().await.clone();
    let json = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(state.data_dir.join("autosave_settings.json"), json)
        .map_err(|e| format!("Failed to persist settings: {e}"))
}

#[tauri::command]
pub async fn get_game_status(state: tauri::State<'_, AppState>) -> Result<GameStatus, String> {
    let engine_guard = state.engine.read().await;
    let processing = *state.processing.read().await;

    match engine_guard.as_ref() {
        Some(engine_state) => {
            let countries: Vec<String> =
                engine_state.game_state.countries.keys().cloned().collect();

            let calendar = &engine_state.game_state.calendar;
            let season = match calendar.get_season() {
                sim_engine::state::Season::Winter => "Winter",
                sim_engine::state::Season::Spring => "Spring",
                sim_engine::state::Season::Summer => "Summer",
                sim_engine::state::Season::Autumn => "Autumn",
            };

            let legislature_names = engine_state
                .game_state
                .countries
                .iter()
                .map(|(name, c)| {
                    (
                        name.clone(),
                        c.politics.government_form.legislature_name().to_string(),
                    )
                })
                .collect();

            let annexed_countries = engine_state
                .game_state
                .countries
                .iter()
                .filter(|(_, c)| c.annexed_by.is_some())
                .map(|(name, _)| name.clone())
                .collect();

            Ok(GameStatus {
                has_game: true,
                turn: calendar.global_turn,
                year: calendar.current_year,
                month: calendar.current_month,
                season: season.to_string(),
                countries,
                processing,
                legislature_names,
                annexed_countries,
            })
        }
        None => Ok(GameStatus {
            has_game: false,
            turn: 0,
            year: 0,
            month: 0,
            season: String::new(),
            countries: Vec::new(),
            processing,
            legislature_names: Default::default(),
            annexed_countries: Vec::new(),
        }),
    }
}
