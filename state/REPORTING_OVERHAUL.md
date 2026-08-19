# Reporting Overhaul: Data Pipeline Audit & UI Redesign

A diagnostic audit of the four reporting anomalies (labor/demographics, politics, corporate, global market) with traced root causes and architect-approved Rust fixes, plus a `comfy-table`-centered console UI redesign. **This is an analysis document — no engine or UI code is changed here.**

> **Architectural directives incorporated (Lead Architect vetoes):**
> - **Global Market:** Do **not** abandon the `Commodity` enum for a stringly-typed namespace. Fix the enum mapping and route **all** orders through `Commodity`.
> - **Corporate:** Do **not** add an ad-hoc `run_turn` bootstrap. Port the actual `corporate/market_generator.py` logic into the generator.
> - **Politics / Labor:** `employed_total` aggregation and a political bootstrap pass during `world_generator` are approved as originally proposed.

---

## Section 1 — Data Audit Results

### 1. Labor / Demographics report all zeros

**Symptom:** `Employed Total`, `Average Age`, sectoral `Employment` and `PMI` all render as `0` / `0.00`.

**Traced root cause:**
- **`employed_total` never aggregated.** `src/economy/labor.rs` computes per-tier employment (`expert_tier.employed`, `skilled_tier.employed`, `unskilled_tier.employed`) but never writes the sum back into `labor_market.employed_total`. The field is only ever *read* (`labor.rs:298`, where `bezrobotni = sila_robocza - employed_total` — using a value permanently stuck at `0.0`). The generator's `build_macro_data` leaves it at the `LaborMarket::default()` of `0.0`.
- **Demographics scalars never populated.** `src/engine/generator/mod.rs::build_demographics` sets birth/death rates, age groups, gender and composition, but leaves `average_age`, `median_age`, `city_urban` and `rural` at `Demographics::default()` (`0.0`). No turn logic computes them either.
- **Sector `zatrudnienie` / `pmi` never written.** The report reads `share.extra["zatrudnienie"]` and `share.extra["pmi"]` (`src/ui/reports.rs:184-185`), but the generator's `sector_share()` constructs `SectorShare` with an empty `extra` map, and the turn pipeline never inserts those keys.

**Proposed Rust fix (APPROVED):**
1. In `labor.rs`, after the tier calculations, set
   `labor_market.employed_total = expert_tier.employed + skilled_tier.employed + unskilled_tier.employed;`
   and derive `bezrobotni` from labor force minus this aggregate.
2. In `build_demographics`, compute `average_age` / `median_age` as the age-group-weighted midpoint (children/adults/elderly) and split `city_urban` / `rural` from GDP-per-capita (urbanization curve), so Turn 0 is populated.
3. Populate `SectorShare.extra["zatrudnienie"]` and `["pmi"]` inside `update_gdp_shares_from_employment` (per-turn) and seed sensible Turn-0 values in the generator so both the generated world and played saves report non-zero employment.

---

### 2. Politics fields blank

**Symptom:** `Ruling Party`, `Coalition ID`, `Head of State`, `Trade Doctrine` are empty strings; the `Active Party` and `Interest Group` tables are empty.

**Traced root cause:**
- **Generator emits an empty political block.** `src/engine/generator/mod.rs:214` constructs each country with `politics: Politics::default()`. Every political field — `ruling_party`, `active_parties`, `parliament`, `interest_group_power`, `coalition_id`, policy doctrines — starts blank. A freshly *generated* world that has never run a turn therefore shows all blanks.
- **`head_of_state` / `dynasty` are generated nowhere.** The `Leader` (`glowa_panstwa`) and `dynasty` fields are neither set by the generator nor by `process_political_year` (`src/politics/turn.rs`). They remain blank even after turns are processed.
- **Note (not a bug):** `run_turn` *does* call `process_political_year` (`turn.rs:259`), which populates `interest_group_power`, `active_parties`, `ruling_party` and `coalition` on the first democratic election. So these fill in after ≥1 persisted turn — but the report is being viewed on Turn 0, before any political processing has run.

**Proposed Rust fix (APPROVED — political bootstrap in `world_generator`):**
- Run a **political bootstrap pass** during generation so Turn 0 has real data. Concretely, after building each country's economy, invoke the existing political pipeline (`calculate_interest_groups_power` → `regenerate_parties` → seat allocation → `build_coalition` → `apply_ruling_ideology_policies`) so `interest_group_power`, `active_parties`, `parliament`, `ruling_party`, `coalition`/`coalition_id`, and the policy doctrines (`trade_doctrine`, `labor_law`, ...) are all populated at generation time.
- Add **`head_of_state` generation** (name, title, and — for monarchies — `dynasty`) to the generator, since no code path currently produces it.

---

### 3. Corporate: "No corporate data"

**Symptom:** The Corporate report prints `No corporate data for <country>` for a freshly generated country.

**Traced root cause:**
- `generate_world` **deliberately** leaves `entities/` and `spatial_registry/` empty (documented at `generator/mod.rs:109-110`: *"Leaves `entities/` and `spatial_registry/` empty ... the first `run_turn` will seed them"*).
- **But nothing ever seeds them.** `run_turn` only *loads* existing companies/buildings (`load_companies` / `load_buildings` in `src/engine/turn.rs`), processes whatever it finds, and re-saves. With nothing on disk, every corporate step is a no-op. `data/entities/<country>/companies/` is therefore never created.
- The underlying gap is that **`corporate/market_generator.py` was never ported** to Rust — there is no code anywhere that manufactures the initial corporate landscape.

**Proposed Rust fix (per architect — PORT the generator logic):**
- Port `corporate/market_generator.py` into `src/engine/generator/` (e.g. a new `generator/corporate.rs`) so **Turn 0 creates mathematically sound companies**, not placeholder stubs. The port should:
  - Size the corporate sector from each sector's **GDP share × national GDP** (using the same `sectors` shares the generator already computes), so total company capital reconciles with `budget.gdp` and `private_capital`.
  - Instantiate `Company` records with consistent `fixed_capital`, `liquid_capital`, `company_capital`, `shares_count`/`share_price`, `worker_capacity`, ownership type, and `is_listed`/`is_national_champion` flags.
  - Create the backing spatial `Building` entities, wire `building_ids` / `plants`, and assign buildings to regions.
  - Persist via the existing `DiskEntityStore<Company>` / `DiskEntityStore<Building>` to `entities/<country>/companies/<sector>.json` and `spatial_registry/<country>/<region>/buildings/`.
- **Explicitly not** an ad-hoc bootstrap inside `run_turn` and **not** placeholder seeding.

---

### 4. Global Market: prices stuck at 20 / 100 / 500

**Symptom:** Prices sit at `20.00`, `100.00`, or `500.00` regardless of large `Buy`/`Sell` discrepancies; a generated country shows `0.00` for all trade.

**Traced root cause:**
- **A — Namespace / typing failure (core issue).** The registry already types production flows correctly: `ProductionMethod.inputs` / `outputs` are `HashMap<Commodity, f64>` (`src/registries/production_methods.rs:42,47`). However, `resolve_active_method` immediately **downgrades the enum to `String`** via `commodity_name()` (`src/economy/production.rs:72-73`), and the rest of the pipeline is stringly-typed: `MarketOrders.orders`, `GlobalMarket.base_prices`, `GlobalMarket.net_surplus`, and `ProductionResult` are all `HashMap<String, f64>` (`src/economy/market.rs:33,67-69`). Compounding this, the `Commodity` enum defines **only 9 variants** (`bron, amunicja, paliwo, zywnosc, mundury, pojazdy, elektronika, uslugi_b2b, papier` — `enums.rs:167-195`), while production data and `market.json` reference ~60 goods (`Drewno`, `Stal`, `Wapień`, `Cement`, `Węgiel Kamienny`, ...). Any good absent from `Commodity::all()` has no seeded base price, so `GlobalMarket::base_price(good, 100.0)` returns the hardcoded `100.0` fallback — the exact cause of the `[DEBUG] market base_prices Drewno=None ...` trace.
- **B — Feedback saturation.** `src/economy/clearing.rs` clamps cleared prices to `PRICE_FLOOR = 0.2` and `PRICE_CAP = 5.0` times the base (i.e. `20.0` and `500.0` on a `100.0` base). In `run_turn`, the cleared local prices are averaged straight back into `market.base_prices` (`turn.rs:276-285`) while `net_surplus` is recomputed from the same order book. Because the global market can never absorb the surplus/deficit (there is no real world inventory), prices saturate to the floor (`20`) or cap (`500`) each turn; `100.00` only appears when local `buy == sell`.
- **C — Generated world shows 0 trade.** With no buildings on disk (see Anomaly 3), `process_building_cycle` emits no orders, so prices stay at the seeded `100.0` and `res_stats` is empty.

**Proposed Rust fix (per architect — VETO on the string namespace; the enum stays authoritative):**
1. **Expand `Commodity`** to cover the full production-goods namespace: one variant per Polish JSON key, each with `#[serde(rename = "...")]`, plus a `FromStr` / `TryFrom<&str>` implementation for parsing legacy save keys. Extend `Commodity::all()` accordingly.
2. **Re-type the market pipeline to be `Commodity`-keyed.** Change `MarketOrders.orders`, `GlobalMarket.base_prices`, `GlobalMarket.net_surplus`, `ProductionResult.{inputs_consumed, outputs_produced}`, and `ActiveProductionMethod.{inputs, outputs}` from `HashMap<String, _>` to `HashMap<Commodity, _>`. **Delete `commodity_name()`** and the string downgrade so every order flows through the enum end-to-end.
3. **Deserialize `market.json` directly into `Commodity`-keyed maps** — serde `rename` handles the Polish keys, eliminating the `100.0` fallback and the `None` lookups. Anomaly 3's corporate port then supplies real buildings/orders so prices actually move.
4. **Damp the price feedback loop**: apply a partial adjustment toward the cleared price (e.g. exponential smoothing) instead of overwriting `base_prices` with the raw per-turn average, so prices converge rather than slam to floor/cap.
5. **Remove the leftover `[DEBUG]` / `eprintln!` traces** in `src/engine/turn.rs`.

---

## Section 2 — UI/UX Redesign Options

The current reports are single-column `Key | Value` tables printed via `println!` loops with fixed `{:<32} | {:>15}` widths. This produces a vertically bloated "store receipt": ~90 stacked rows for one macro report, most of the terminal width wasted, and no visual grouping. The goal is a **horizontally dense, multi-column, visually differentiated** layout.

### Option A — `comfy-table` (RECOMMENDED)

Adopt [`comfy-table`](https://crates.io/crates/comfy-table) for all report rendering. It provides box-drawn borders, automatic/percentage column widths, per-cell alignment and coloring, content-aware wrapping, and terminal-width detection.

**Why it wins:**
- **Horizontal density via side-by-side panels.** Render related metric groups as separate narrow tables and join them column-wise into one row (e.g. `Fiscal │ Macro │ Labor` across the top of the macro report) instead of 60 stacked lines.
- **Dynamic widths** (`set_width` / `Percentage` constraints) adapt to the terminal, ending the wasted right-hand space.
- **Visual diversity** — headers, alignment, and optional color (via the `custom_styling` feature) distinguish categories at a glance.
- Mature, widely used, MIT-licensed, minimal transitive dependencies.

**`Cargo.toml`:**
```toml
[dependencies]
comfy-table = "7"
```

**Sketch — macro report as a 3-panel dashboard:**
```
┌─ MACROECONOMY: Wenedia ──────────────────────────────────────────────────┐
┌───────────────┬────────────┐ ┌──────────────┬───────┐ ┌───────────────┬──────┐
│ Fiscal        │      Value │ │ Macro        │ Value │ │ Labor         │ Val  │
╞═══════════════╪════════════╡ ╞══════════════╪═══════╡ ╞═══════════════╪══════╡
│ GDP           │ 59.3B      │ │ Inflation    │ 6.62% │ │ Employed      │ 8.1M │
│ Reserves      │  5.28B     │ │ Gini         │ 0.29  │ │ Unemployment  │ 7.9% │
│ Priv. Capital │ 12.8B      │ │ Unrest       │ 38.5  │ │ Avg Wage      │ 2784 │
└───────────────┴────────────┘ └──────────────┴───────┘ └───────────────┴──────┘
┌─ Sectors ─────────┬───────────┬────────────┬────────┐
│ Sector            │ GDP Share │ Employment │  PMI   │
╞═══════════════════╪═══════════╪════════════╪════════╡
│ Heavy Industry    │   17.63%  │  1,240,000 │  51.2  │
│ Light Industry    │   23.94%  │  1,880,000 │  48.7  │
└───────────────────┴───────────┴────────────┴────────┘
```

**Migration:** replace the hand-formatted `println!` blocks in `src/ui/reports.rs` with small helper builders (`fn fiscal_table(&Budget) -> Table`, etc.) and a `render_row(tables: &[Table])` helper that prints narrow tables side-by-side.

### Option B — `cli-table`

[`cli-table`](https://crates.io/crates/cli-table) is a lighter alternative with a `#[derive(Table)]` macro that turns a struct/`Vec<T>` directly into a table.

- **Pros:** ergonomic derive for row-shaped data (e.g. `Vec<CompanyRow>`, `Vec<SectorRow>`), smaller surface area.
- **Cons:** less control over dynamic width, side-by-side panels, and styling than `comfy-table`; the derive model fits list reports (corporate, market) better than the mixed key/value dashboards.

```toml
[dependencies]
cli-table = "0.4"
```

### Option C — Zero-dependency `println!` grid (fallback)

Keep pure `std::io` but replace the one-metric-per-line pattern with a hand-rolled **multi-column grid formatter**: a `columns(pairs: &[(&str, String)], cols: usize)` helper that lays N key/value pairs across the terminal width, plus a shared box-drawing helper.

- **Pros:** no new dependencies; full control; consistent with the current crate footprint.
- **Cons:** we re-implement width math, wrapping, and alignment that the crates already solve; more code to maintain and test.

### Recommendation

**Adopt Option A (`comfy-table`)** for all five report categories, using the side-by-side panel pattern to collapse the vertical receipts into compact dashboards, with list-style tables (`comfy-table` rows) for the corporate and global-market reports. Option C is the fallback only if adding a dependency is later rejected.

---

## Implementation Notes (follow-up work, not done here)
- This document changes **no code**. The fixes in Section 1 and the UI adoption in Section 2 are the next implementation tasks.
- Adding `comfy-table` to `Cargo.toml` is deferred to the UI implementation task.
- The `Commodity` enum expansion (Anomaly 4) is a prerequisite for meaningful market/corporate reports and should land before or with the corporate generator port (Anomaly 3).
