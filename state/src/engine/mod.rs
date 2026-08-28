//! Turn orchestration, the game loop and the world generator.

pub mod generator;
pub mod turn;
pub mod turn_config;
pub mod turn_context;

pub use generator::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
pub use turn::{run_turn_in_memory, TurnError};
pub use turn_config::TurnConfig;
pub use turn_context::{CountryEntities, InMemoryTurnContext};
