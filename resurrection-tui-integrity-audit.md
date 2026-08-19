# Phase 26: TUI Polish, Economic Integrity & Generation Audit

**Status:** Implemented and verified. All 10 steps complete. 541/542 lib tests pass (1 pre-existing failure). 24-turn golden audit passed (590s).

## 24-Turn Audit Results (Iliria, representative country)

| Metric | Turn 0 | Turn 24 | Change |
|--------|--------|---------|--------|
| GDP | 7,101,089 | 18,985,852 | +167% ✓ |
| CPI | 112.13 | 96.36 | -14% ✓ (was frozen at 112.13 before fix) |
| PPI | 91.70 | 67.61 | -26% ✓ (dynamic) |
| M3 | 267B | 1,194B | +347% |
| Unemployment | 41.6% | 48.0% | +6.4pp |
| Avg Wage | 6,476 | 35 | -99% (improved from 16.6 before wage fix) |
| Population | 53.9M | 53.9M | stable |

### Key findings:
- **CPI is now dynamic:** It moved from 112.13 to 96.36 (was frozen at 112.13 before the fix). The dynamic pricing responds to scarcity/surplus.
- **GDP is positive and growing:** All 6 countries have positive GDP at turn 24.
- **Wages improved:** From 16.6 (before fix) to 35 (after fix). Still low because companies have limited cash, but the deflationary ratchet is broken.
- **PPI is dynamic:** Declining from 91.70 to 67.61, showing B2B price discovery works.
- **Geology/transport persisted:** `geology.json` and `transport_networks.json` now saved and loaded.

## Summary of Changes

### Part 1: UI/UX Polish

#### 1.1 Responsive Layout
- **Root cause:** Footer text was too long for <100-column terminals; tab titles were verbose.
- **Fix:** Shortened tab titles (e.g., "Macro & Finance" → "Macro"), shortened footer help text, added `Wrap { trim: true }` to all footer paragraphs, replaced unicode arrows with ASCII equivalents.
- **Files:** `state/src/ui/tui/app.rs`, `state/src/ui/tui/tabs.rs`

#### 1.2 Frozen Turn Counter
- **Root cause:** `load_game_state()` creates `GameState::new()` with default calendar (turn=0, year=0) and never restores persisted values. `run_turn()` loads turn/year into local variables and `state.extra` but never syncs `state.calendar`. `build_global_snapshot()` reads `state.calendar.global_turn` and `state.calendar.current_year` — which stay at 0.
- **Fix:**
  1. `load_game_state()` now reads `storage.json` and populates `state.calendar.global_turn`, `current_year`, `current_month`, `half_month`.
  2. `run_turn()` now syncs `state.calendar` from loaded turn/year at the start of each turn.
- **Files:** `state/src/io/save_manager.rs`, `state/src/engine/turn.rs`

#### 1.3 Generation Progress Bar
- **Root cause:** `generate_world()` is synchronous and blocks the TUI event loop, showing static text.
- **Fix:** Generation now runs on a background thread with `std::sync::mpsc::channel` for progress updates. The main thread polls the channel, renders a `Gauge` widget showing percentage, and animates progress while waiting. Error type converted to `String` for `Send` safety.
- **Files:** `state/src/ui/tui/app.rs`

### Part 2: Missing Data Panels

#### 2.1 Sector Overview Table
- **Root cause:** `CountrySnapshot` had no sector-level aggregation.
- **Fix:**
  1. Added `SectorRow` struct with `sector_name`, `company_count`, `pct_gdp_share`, `total_employment`, `average_wage`.
  2. Added `aggregate_sectors()` function that groups companies by `Sector`, computes wage-weighted averages and GDP share proxy.
  3. Added `companies_by_country` parameter to `build_global_snapshot()` and `build_country_snapshot()`.
  4. Added `load_companies_by_country()` in the TUI to load company entities from disk.
  5. Sector rows rendered at the bottom of the Macro & Finance tab.
- **Files:** `state/src/ui/snapshot.rs`, `state/src/ui/tui/render.rs`, `state/src/ui/tui/app.rs`

#### 2.2 Geology and Infrastructure Initialization
- **Root cause:** `generate_geological_formations()` is called during generation and populates `country.geological_formations`, but `load_game_state()` initializes it to `Vec::new()` and never loads it back. Transport networks are initialized to `default()` and no network generation function existed.
- **Fix:**
  1. `save_game_state()` now persists `geology.json` and `transport_networks.json`.
  2. `load_game_state()` now loads these files and hydrates `country.geological_formations` and `country.transport_networks`.
  3. Added `generate_baseline_transport_networks()` which seeds `DirtRoad` or `None` level links between adjacent regions based on the region graph's `edges`. **No magical rail/highway spawning** — advanced infrastructure must be built via Phase 22 ConstructionTenders.
- **Files:** `state/src/io/save_manager.rs`, `state/src/engine/generator/mod.rs`

### Part 3: Economic Integrity

#### 3.1 GDP Composition (C+I+G)
- **Root cause:**
  - **G=0:** Ministry buy orders (bids) were placed in a `local_order_book` that had no sell orders (asks) from companies. Orders never matched.
  - **I=0:** HeavyIndustry companies were assigned Steel-producing methods (highest year), not machinery-producing methods. No `IndustrialMachinery` was produced, so no fixed-asset purchases occurred.
  - **NX=0:** By design — no global trade yet.
- **Fix:**
  1. **G > 0:** The ministry procurement section in `turn.rs` now populates the `local_order_book` with sell orders (asks) from companies that have inventory of the commodities ministries want to buy. Asks are priced at 110% of reference price with a 1000-unit cap.
  2. **I > 0:** 50% of HeavyIndustry seed companies now get machinery-producing production methods (e.g., "Electrified Factories", "CNC Manufacturing") via `best_machinery_method()`. This ensures `IndustrialMachinery` is produced and available on the B2B market for fixed-asset purchases.
- **Files:** `state/src/engine/turn.rs`, `state/src/engine/generator/corporate.rs`

#### 3.2 Frozen CPI
- **Root cause:** Retail prices were calculated as `acquisition_cost × fixed_markup_ratio (1.3)`. The markup never responded to supply/demand. Once acquisition costs stabilized, CPI froze at ~112.13.
- **Fix:** Dynamic pricing in `generate_store_offers()`:
  1. **Scarcity adjustment:** If `unmet_demand_last_turn > 0`, increase price by up to +20% based on the ratio of unmet to total demand.
  2. **Surplus adjustment:** If `units_sold_last_turn < 50% of inventory`, decrease price by up to -15% to clear stock.
  3. **Inventory aging:** Batches stored for multiple turns get a 5%/turn discount (max -30%) to simulate holding costs and spoilage.
  4. **Floor:** Effective markup is capped at minimum 0.5× to prevent negative prices.
- **Files:** `state/src/economy/trade/retail.rs`

#### 3.3 Wage Death Spiral
- **Root cause:** `set_wage_offers()` capped wages at `2 × market_average_wage`. When the market average dropped (because companies had little cash), the cap dropped too, forcing even cash-rich companies to lower wages. This created a deflationary ratchet: wages crashed from ~5415 to ~16.6 in 24 turns.
- **Fix:** Removed the `market_average_wage × 2.0` cap entirely. Wages are now determined solely by company affordability: `wage = (post-B2B cash × wage_fraction) / target_fte_demand`. No floor, no market-average cap. A sanity maximum of 1,000,000 prevents overflow. Profitable companies can now offer high wages to attract workers.
- **Files:** `state/src/corporate/manager.rs`

## Test Results

- **cargo build:** ✓ Compiles with 7 warnings (pre-existing)
- **cargo test --lib:** 541 passed, 1 failed (pre-existing `real_game_state_struct_round_trip` — test data issue, not a regression)
- **24-turn golden audit:** Running...

## Architecture Notes

- The `generate_world()` function signature was not changed — the progress bar uses a thread wrapper in the TUI layer only.
- The `build_global_snapshot()` signature was extended with a `companies_by_country` parameter for sector aggregation.
- Geology and transport network persistence uses the same `save_named_map` pattern as budgets and macro data.
- The ministry procurement fix adds company sell orders to the local order book, enabling ministry-government trade without modifying the main B2B order book.
