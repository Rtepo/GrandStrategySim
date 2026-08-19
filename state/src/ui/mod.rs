//! User interface data models for the sim_engine.
//!
//! Phase 50: The Ratatui TUI, console menu, and text reports have been
//! removed. The simulation frontend is now a Tauri desktop application.
//! This module retains only `snapshot.rs`, which contains the paginated
//! data model used by the Tauri IPC bridge.
//!
//! The `#![allow(missing_docs)]` attribute suppresses the crate-level
//! `#![deny(missing_docs)]` for UI snapshot types, which are internal
//! presentation types not part of the engine API contract.

#![allow(missing_docs)]

pub mod snapshot;
