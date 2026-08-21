//! # `sim_engine` — Rust Migration of the Grand Strategy Simulation Engine
//!
//! This crate is the Rust port of a Python grand-strategy economic simulator.
//! It is being built incrementally following the roadmap in
//! `RUST_MIGRATION_BLUEPRINT.md`.
//!
//! ## Stage 0 — Static Registries & Core Math
//!
//! Stage 0 establishes the foundation with **zero mutable global state**:
//!
//! - [`registries`] — immutable, load-once game data (tech tree, production
//!   methods, building templates, government forms) plus the categorical
//!   [`registries::enums`] that replace Python's stringly-typed dictionary keys.
//! - [`math`] — pure numeric helpers (decay curves, experience gain, ratio
//!   normalization) shared across all future gameplay systems.
//!
//! ## Stage 1 — State Structs & Serde Interop Bridge
//!
//! - [`state`] — the typed replacements for Python's dynamic per-country
//!   dictionaries ([`state::Treasury`], [`state::MacroData`],
//!   [`state::TaxRates`], joined into [`state::Country`] under
//!   [`state::GameState`]). All fields map to the exact Polish JSON keys via
//!   `#[serde(rename)]`, with `#[serde(flatten)]` catch-alls guaranteeing
//!   lossless round-trips against existing Python saves.
//! - [`io`] — the serde interop bridge that loads the Python engine's split
//!   JSON save files into the typed state.
//!
//! ## Stage 2 — Deterministic Economy Turn & Golden-master Parity (current)
//!
//! - [`economy::CountryTurnCtx`] — the split-borrow context: a mutable
//!   `Country` and an immutable `&Registries`.
//! - [`state::Bank`] — the legacy commercial banking struct (now superseded by
//!   `Company` entities with `BankBalanceSheet` in Phase 5).
//! - [`economy::indicators::update_gdp_shares_from_employment`] — the first
//!   ported formula, a pure Python port from `economy/indicators/core.py`.
//! - [`state::banking::process_banking_turn`] — Phase 2 banking orchestrator
//!   with double-entry lending, interbank market, CB Lombard, and bank resolution.
//! - [`economy::process_demographics_and_labor`] — labor supply by education
//!   tier, demographic growth, migration, and wage/friction updates.
//! - [`entities::Building`] and [`entities::Company`] — typed corporate
//!   actors with the [`io::EntityStore`] trait for safe disk/memory loading.
//! - [`economy::process_building_cycle`] — per-building production loop that
//!   consumes inputs, produces outputs, and tallies orders in
//!   [`economy::market::MarketOrders`].
//! - [`economy::indicators::run_economic_turn`] — the per-country turn
//!   orchestrator (calls the GDP-share update and returns the result).
//! - `tests/phase75_dynamic_integration_test.rs` — the 24-turn dynamic
//!   integration test that asserts behavioral invariants.
//!
//! ## Documentation Standard
//!
//! Every public and private item is documented. Functions with gameplay
//! semantics follow the `# Arguments` / `# Returns` / `# Rules` template.
//! The crate denies missing docs and warns on broken intra-doc links so the
//! `cargo doc` portal is always complete and internally consistent.
#![deny(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod agriculture;
pub mod corporate;
pub mod construction;
pub mod data;
pub mod economy;
pub mod engine;
pub mod entities;
pub mod government;
pub mod i18n;
pub mod infrastructure;
pub mod international;
pub mod io;
pub mod math;
pub mod military;
pub mod politics;
pub mod registries;
pub mod securities;
pub mod society;
pub mod state;
pub mod ui;
pub mod utilities;
