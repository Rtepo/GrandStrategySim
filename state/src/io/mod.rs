//! Input/output: save file serialization and entity persistence.

pub mod entity_store;
pub mod runtime_save;
pub mod save_manager;
pub mod telemetry_export;

pub use entity_store::{DiskEntityStore, Entity, EntityStore, EntityStoreError, MemoryEntityStore};
pub use runtime_save::{
    list_save_slots, load_runtime_save, world_fingerprint, write_runtime_save, RuntimeSave,
    SaveMeta,
};
pub use save_manager::{load_game_state, load_named_map, save_named_map, SaveError};
