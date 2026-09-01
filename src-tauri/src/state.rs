use sim_engine::engine::InMemoryTurnContext;
use sim_engine::registries::Registries;
use sim_engine::state::GameState;
use std::sync::Arc;
use tokio::sync::RwLock;

/// The complete in-memory engine state, persisted in Tauri's managed state.
pub struct EngineState {
    pub game_state: GameState,
    pub turn_context: InMemoryTurnContext,
}

/// Tauri-managed application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RwLock<Option<EngineState>>>,
    pub registries: Arc<Registries>,
    pub data_dir: std::path::PathBuf,
    pub processing: Arc<RwLock<bool>>,
}
