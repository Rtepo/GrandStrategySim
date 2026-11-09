# TASK-038 — v4.32.0 release (gates + bump + merge + tag + push)

- state/tests/v4320_release_gates.rs asserts: 240-turn sim no panic && max_region_pop@120 < 200M && save/load roundtrip fingerprint && joint-stock shareholders && UoR inventory subcategories && SpreadsheetML xmlns && snap campaign actions > 0 && commander ranks + battle attribution.
- Iron CI/CD green: cargo build, cargo test, clippy -D warnings, npm run build.
- Bump state/Cargo.toml, src-tauri, package.json, tauri.conf.json, Cargo.lock → 4.32.0; merge_worker.sh → push main + tag v4.32.0.
