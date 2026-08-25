use crate::state::{AppState, EngineState};
use sim_engine::engine::{generate_world, GenerateOptions, StartYear, run_turn_in_memory, InMemoryTurnContext};
use sim_engine::io::save_manager::load_game_state;
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
        let engine_state = engine_guard
            .as_mut()
            .ok_or("No game loaded")?;

        run_turn_in_memory(
            &mut engine_state.game_state,
            &registries,
            &mut engine_state.turn_context,
        ).map_err(|e| format!("Turn failed: {e:?}"))?;

        let turn = engine_state.game_state.calendar.global_turn;
        let year = engine_state.game_state.calendar.current_year;
        Ok::<TurnResult, String>(TurnResult { turn, year, status: "ok".to_string() })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    {
        let mut processing = state.processing.write().await;
        *processing = false;
    }

    result
}

#[tauri::command]
pub async fn save_game(
    state: tauri::State<'_, AppState>,
    save_name: String,
) -> Result<(), String> {
    let engine_arc = state.engine.clone();
    let data_dir = state.data_dir.clone();

    let save_dir = data_dir.join("saves").join(&save_name);

    tokio::task::spawn_blocking(move || {
        let engine_guard = engine_arc.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        std::fs::create_dir_all(&save_dir)
            .map_err(|e| format!("Failed to create save dir: {e}"))?;

        sim_engine::io::save_manager::save_game_state(&save_dir, &engine_state.game_state)
            .map_err(|e| format!("Failed to save state: {e}"))?;

        let global_orders = sim_engine::economy::market::MarketOrders::default();
        let trade_result = sim_engine::international::TradeBalanceResult::default();
        engine_state.turn_context.save_to_disk(&save_dir, &engine_state.game_state, &global_orders, &trade_result)
            .map_err(|e| format!("Failed to save context: {e}"))?;

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn load_game(
    state: tauri::State<'_, AppState>,
    save_name: String,
) -> Result<(), String> {
    let data_dir = state.data_dir.clone();
    let save_dir = data_dir.join("saves").join(&save_name);

    let mut game_state = tokio::task::spawn_blocking(move || {
        load_game_state(&save_dir)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
    .map_err(|e| format!("Failed to load state: {e}"))?;

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
pub async fn list_saves(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let saves_dir = state.data_dir.join("saves");
    if !saves_dir.exists() {
        return Ok(Vec::new());
    }

    let mut saves = Vec::new();
    let entries = std::fs::read_dir(&saves_dir)
        .map_err(|e| format!("Failed to read saves dir: {e}"))?;

    for entry in entries.flatten() {
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                saves.push(name.to_string());
            }
        }
    }

    saves.sort();
    Ok(saves)
}

#[tauri::command]
pub async fn get_game_status(
    state: tauri::State<'_, AppState>,
) -> Result<GameStatus, String> {
    let engine_guard = state.engine.read().await;
    let processing = *state.processing.read().await;

    match engine_guard.as_ref() {
        Some(engine_state) => {
            let countries: Vec<String> = engine_state
                .game_state
                .countries
                .keys()
                .cloned()
                .collect();

            let calendar = &engine_state.game_state.calendar;
            let season = match calendar.get_season() {
                sim_engine::state::Season::Winter => "Winter",
                sim_engine::state::Season::Spring => "Spring",
                sim_engine::state::Season::Summer => "Summer",
                sim_engine::state::Season::Autumn => "Autumn",
            };

            Ok(GameStatus {
                has_game: true,
                turn: calendar.global_turn,
                year: calendar.current_year,
                month: calendar.current_month,
                season: season.to_string(),
                countries,
                processing,
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
        }),
    }
}
