//! Input/output: the serde interop bridge to the Python engine's save files.

pub mod entity_store;
pub mod save_manager;
pub mod telemetry_export;

pub use entity_store::{
    DiskEntityStore, Entity, EntityStore, EntityStoreError, MemoryEntityStore,
};
pub use save_manager::{
    load_country_data, load_game_state, load_named_map, save_named_map, SaveError,
};
