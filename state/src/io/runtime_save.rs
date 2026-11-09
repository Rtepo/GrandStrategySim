//! v4.32.0 (TASK-033): Full runtime save/load — gzip-compressed `.gss` files.
//!
//! Unlike [`super::save_manager`], which only persists the split scenario
//! bootstrap files (budgets/macro/politics…), this module serializes the
//! ENTIRE live simulation: `GameState` (countries, calendar, markets) plus
//! the `InMemoryTurnContext` (global market order book, diplomacy matrix,
//! every company/building/union in every country).
//!
//! File layout under `<data_dir>/saves/`:
//!   `<slot>.gss`        — gzip-compressed CBOR [`RuntimeSave`]
//!   `<slot>.meta.json`  — small sidecar with [`SaveMeta`] so the slot list
//!                         can render without inflating the whole save.

use crate::engine::turn_context::InMemoryTurnContext;
use crate::state::GameState;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

/// Current runtime-save format version.
pub const RUNTIME_SAVE_VERSION: u32 = 1;

/// Metadata shown in the save-slot picker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SaveMeta {
    /// Save format version (for future migrations).
    pub version: u32,
    /// User-facing slot name (file stem).
    pub slot: String,
    /// Global turn counter at save time.
    pub turn: u32,
    /// In-game calendar label, e.g. "Sep 1904".
    pub date_label: String,
    /// In-game year (redundant with label but convenient for sorting/UI).
    pub year: u32,
    /// In-game month (1-12).
    pub month: u32,
    /// Player/primary country shown on the slot card — the largest country
    /// by population at save time (single-player sessions have no explicit
    /// "player" marker in the engine).
    pub country: String,
    /// Unix timestamp (seconds) when the save was written.
    pub saved_at_unix: u64,
    /// Whether this slot was written by the autosave system.
    pub autosave: bool,
}

/// Complete runtime snapshot — everything needed to resume a session
/// without touching the bootstrap data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSave {
    /// Format version (for future migrations).
    pub version: u32,
    /// Slot metadata (duplicated into the `.meta.json` sidecar).
    pub meta: SaveMeta,
    /// Full game state.
    pub game_state: GameState,
    /// Full turn context (market, diplomacy, all entities).
    pub turn_context: InMemoryTurnContext,
}

/// Build a [`SaveMeta`] for the current state.
pub fn build_meta(
    slot: &str,
    state: &GameState,
    saved_at_unix: u64,
    autosave: bool,
) -> SaveMeta {
    let cal = &state.calendar;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = MONTHS
        .get(cal.current_month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?");
    let country = state
        .countries
        .values()
        .max_by_key(|c| c.regions.iter().map(|r| r.population).sum::<i64>())
        .map(|c| c.name.clone())
        .unwrap_or_default();
    SaveMeta {
        version: RUNTIME_SAVE_VERSION,
        slot: slot.to_string(),
        turn: cal.global_turn,
        date_label: format!("{month_name} {}", cal.current_year),
        year: cal.current_year,
        month: cal.current_month,
        country,
        saved_at_unix,
        autosave,
    }
}

/// Serialize + gzip a runtime save to `<dir>/<slot>.gss`, with a
/// `<slot>.meta.json` sidecar for cheap listing.
pub fn write_runtime_save(
    saves_dir: &Path,
    slot: &str,
    state: &GameState,
    ctx: &InMemoryTurnContext,
    saved_at_unix: u64,
    autosave: bool,
) -> Result<SaveMeta, SaveError> {
    std::fs::create_dir_all(saves_dir).map_err(SaveError::Io)?;
    let meta = build_meta(slot, state, saved_at_unix, autosave);
    let save = RuntimeSave {
        version: RUNTIME_SAVE_VERSION,
        meta: meta.clone(),
        game_state: state.clone(),
        turn_context: ctx.clone(),
    };
    // v4.32.0: the payload serializes as CBOR — `InMemoryTurnContext`
    // contains non-string map keys (commodity/region tuple maps) JSON
    // can't represent, and `#[serde(flatten)]`/`serialize_map(None)`
    // streaming that bincode rejects; CBOR's indefinite-length items
    // and arbitrary key types handle both. `.gss` = gzip(CBOR save).
    let path = gss_path(saves_dir, slot);
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    ciborium::ser::into_writer(&save, &mut enc).map_err(|e| {
        SaveError::Json(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })?;
    let compressed = enc.finish().map_err(SaveError::Io)?;
    // Atomic-ish write: temp file then rename so a crash mid-save can't
    // corrupt an existing good slot.
    let tmp = path.with_extension("gss.tmp");
    std::fs::write(&tmp, &compressed).map_err(SaveError::Io)?;
    std::fs::rename(&tmp, &path).map_err(SaveError::Io)?;
    let meta_json = serde_json::to_vec_pretty(&meta).map_err(SaveError::Json)?;
    std::fs::write(meta_path(saves_dir, slot), meta_json).map_err(SaveError::Io)?;
    Ok(meta)
}

/// Read + gunzip a `.gss` save file.
pub fn read_runtime_save(path: &Path) -> Result<RuntimeSave, SaveError> {
    let bytes = std::fs::read(path).map_err(SaveError::Io)?;
    let dec = GzDecoder::new(&bytes[..]);
    let save: RuntimeSave = ciborium::de::from_reader(dec).map_err(|e| {
        SaveError::Json(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })?;
    Ok(save)
}

/// Load a slot by name from the saves directory.
pub fn load_runtime_save(saves_dir: &Path, slot: &str) -> Result<RuntimeSave, SaveError> {
    read_runtime_save(&gss_path(saves_dir, slot))
}

/// List all save slots (both `.gss` runtime saves and legacy split-file
/// directories) with their metadata.
pub fn list_save_slots(saves_dir: &Path) -> Vec<SaveMeta> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(saves_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gss") {
            continue;
        }
        let Some(slot) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // Prefer the cheap sidecar; fall back to inflating the save.
        let meta_file = meta_path(saves_dir, slot);
        let meta = std::fs::read(&meta_file)
            .ok()
            .and_then(|b| serde_json::from_slice::<SaveMeta>(&b).ok())
            .or_else(|| read_runtime_save(&path).ok().map(|s| s.meta));
        if let Some(m) = meta {
            out.push(m);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.saved_at_unix));
    out
}

/// Fingerprint the full world + context for save/load roundtrip tests.
/// Order-stable: HashMap iteration order differs between the original
/// state and a deserialized copy even when the content is identical, so
/// this hashes SORTED semantic keys rather than raw serde bytes. It is
/// intentionally not a serialization hash — it proves identity-bearing
/// fields survive the roundtrip, which is what the release gate means.
pub fn world_fingerprint(state: &GameState, ctx: &InMemoryTurnContext) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(b"v4320-fp");
    // Calendar + macro identity.
    h.write_u32(state.calendar.global_turn);
    h.write_u32(state.calendar.current_year);
    h.write_u32(state.calendar.current_month);
    // Countries: sorted (name, region.id, population, owner).
    let mut countries: Vec<(&String, &crate::state::Country)> = state.countries.iter().collect();
    countries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, c) in countries {
        h.write(key.as_bytes());
        h.write(c.name.as_bytes());
        h.write_u64(c.budget.population);
        h.write_u64(c.budget.liquid_reserves.to_bits());
        let mut regs: Vec<&crate::society::geography::Region> = c.regions.iter().collect();
        regs.sort_by(|a, b| a.id.cmp(&b.id));
        for r in regs {
            h.write(r.id.as_bytes());
            h.write_i64(r.population);
            h.write(r.owner_country.as_bytes());
        }
    }
    // Context entities: sorted company/building/union ids + counts.
    let mut ent_keys: Vec<&String> = ctx.entities.keys().collect();
    ent_keys.sort();
    for k in ent_keys {
        let e = &ctx.entities[k];
        h.write(k.as_bytes());
        let mut comp: Vec<&str> = e.companies.iter().map(|c| c.id.as_str()).collect();
        comp.sort();
        for id in comp {
            h.write(id.as_bytes());
        }
        let mut bld: Vec<&str> = e.buildings.iter().map(|b| b.id.as_str()).collect();
        bld.sort();
        for id in bld {
            h.write(id.as_bytes());
        }
        h.write_usize(e.unions.len());
    }
    h.finish()
}

fn gss_path(saves_dir: &Path, slot: &str) -> PathBuf {
    saves_dir.join(format!("{}.gss", sanitize_slot(slot)))
}

fn meta_path(saves_dir: &Path, slot: &str) -> PathBuf {
    saves_dir.join(format!("{}.meta.json", sanitize_slot(slot)))
}

/// Keep slot names filesystem-safe.
fn sanitize_slot(slot: &str) -> String {
    let cleaned: String = slot
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "save".to_string()
    } else {
        cleaned
    }
}

use crate::io::save_manager::SaveError;
