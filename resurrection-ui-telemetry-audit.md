# Resurrection — UI, Telemetry & Macroeconomic Indicators Audit

**Date:** 2026-08-12
**Scope:** Read-only audit of the internal UI, telemetry presentation, and macroeconomic indicator calculations across Phases 1–24. No Rust source was modified during this audit.
**Verdict:** The simulation engine has grown enormously (24 phases, 130+ commodities, double-entry banking, geology, construction fraud, KIO, freight, commuting, shadow economy), but **the player is flying almost completely blind**. Three of the four headline macroeconomic indicators (GDP, Inflation, Money Supply M0/M3) are **not recalculated during the turn loop** — they are written once at world-generation and then frozen. The terminal UI is a 1970s-style numbered menu that surfaces none of the Phase 18–24 mechanics.

---

## PART 1 — The Internal UI Audit

### 1.1 Location and form factor

The UI lives in `state/src/ui/` and is a **plain `println!`-based numbered console menu** — there is no TUI library in the dependency tree.

| File | Lines | Role |
|------|-------|------|
| `state/src/ui/mod.rs` | 4 | Module declaration (`pub mod console; pub mod reports;`) |
| `state/src/ui/console.rs` | 113 | Top-level menu loop, turn/report/generate dispatch |
| `state/src/ui/reports.rs` | 421 | Per-country nested report menu + 5 report printers |

`state/Cargo.toml` (lines 16–22) lists only `rayon`, `rand`, `serde`, `serde_json`, `uuid`, `getrandom`. No `crossterm`, no `ratatui`, no `tui`, no web server. The binary entry point is `state/src/main.rs` (5 lines) which simply calls `ui::console::run()`.

### 1.2 Does it compile and run?

**Yes.** `state/target/debug/sim_engine.exe` was rebuilt on 2026-08-12 23:09 and is present. The menu loop in `console.rs::run` (lines 26–54) presents four choices:

```
=== SIM ENGINE RUST ===
1. Run Turn
2. Show Reports
3. Generate New World
4. Exit
```

Option 1 loads the save, calls `engine::run_turn` with `TurnOptions { persist: true }`, and persists. Option 3 prompts for country count (4–16) and a start-year scenario (1900/1925/1950/1975) and generates a fresh world via `engine::generate_world`.

### 1.3 Where does the UI gather its data?

The UI is **strictly read-only against persisted state** — it never reads the in-flight turn context.

- **Country reports** (`reports.rs::run_country_report_menu`, line 48): load the entire `GameState` once via `io::save_manager::load_game_state(data_dir)` (called in `console.rs::process_report`, line 67), then index `state.countries[&country_name]`. All macro/budget/politics figures come from the deserialized `Country` struct.
- **Corporate report** (`print_corporate_summary`, line 319): bypasses `GameState` entirely and re-reads `data/entities/<country>/companies/*.json` from disk via `DiskEntityStore::load_sector`. It aggregates per-sector `fixed_capital`, `liquid_capital`, `company_capital` and prints the top-5 companies by capital.
- **Global market report** (`print_market_summary`, line 396): reads `data/market.json` directly with `fs::read_to_string` and deserializes a local `MarketJson` shape (`prices`, `orders`, `res_stats`). This is a **second, hand-rolled schema** that duplicates the engine's `GlobalMarket` and is not kept in sync with it — for example it knows nothing about `apostolic_see_ledger`, `offshore_capital`, or `market_history.vwap_per_commodity`.

**Critical consequence:** because the UI reads from disk *after* the turn has been persisted, it sees the post-turn snapshot — but only the slices that `save_game_state` writes (`budgets.json`, `macro.json`, `tax_rates.json`, `currencies.json`, `market.json`, `diplomacy.json`, entity JSONs). Anything that lives only on the in-memory `CountryTurnCtx` / `CountryTask` (e.g. `task.labor_allocation`, `task.commute_coverage`, `task.see_remittance`, `task.market_signal`) is **invisible to the UI** because it is never persisted.

### 1.4 How is state presented?

A hierarchical numbered menu: Main → Show Reports → Select Country → Select Category. Five categories (reports.rs lines 59–65):

1. **Macroeconomic** — prints `country.budget.*` scalars, `country.macro_indicators.*`, the energy mix, and a per-sector table (`gdp_share`, `zatrudnienie`, `pmi`).
2. **Budget & Treasury** — allocations, tax rates, VAT brackets, science state.
3. **Politics & Justice** — government form, ruling party, constitution, judiciary, active parties, interest groups.
4. **Corporate & Industry** — per-sector company counts and capital totals, top-5 companies.
5. **Global Market** — per-commodity price/buy/sell and per-country export/import/net.

Output is fixed-width `printf`-style tables (`{:<32} | {:>15}`). There is no scrolling, no filtering, no time-series, no drill-down, no export. Pressing Enter after each screen is the only navigation primitive.

### 1.5 UI gaps relative to the engine

The reports menu was last meaningfully extended in Phase 2. It predates essentially every Resurrection-phase system. None of the following are surfaced anywhere in the UI:

- Banking (Phase D / 5): central bank rates, interbank market, BFG/SOBK funds, bank resolution, KNF, stock exchange indices, MBS/CDS/futures.
- Forex & gold (Phase E.1), sovereign default status.
- Phase 9 tourism, Phase 13 social programs, Phase 14 justice/prison labor, Phase 15 weather/maintenance/OSP, Phase 17 religion/assimilation/pogroms, Phase 18 amnesty/terrorism, Phase 19 generative goods & fixed-asset cohorts, Phase 21 geology, Phase 22 construction tenders/fraud/KIO, Phase 23 freight/transport networks/commuting, Phase 24 property developers/dividends/IPO/bankruptcy auctions.

---

## PART 2 — The Macroeconomic Indicators Audit

The headline finding: **the indicator layer is largely a stub.** `state/src/economy/indicators.rs` (305 lines) contains exactly one real formula — `update_gdp_shares_from_employment` (line 80) — and a `run_economic_turn` (line 151) that calls it, applies infrastructure effects, and returns `ParityResult::MissingExpected`. There is no GDP level update, no inflation update, no money-supply aggregation anywhere in the turn loop.

### 2.1 GDP (PKB)

**Status: BROKEN — frozen at world-gen.**

- The headline GDP lives at `Country.budget.gdp` (`state/treasury.rs:223`, `pub gdp: f64`).
- A `grep` for `.gdp =` across `state/src` finds **zero assignments inside the turn loop**. The only writes are in test setup (`economy/labor/migration.rs:527,581,594`, `corporate/unions.rs:198`) and in the world generator (`engine/generator/mod.rs`).
- `update_gdp_shares_from_employment` (`economy/indicators.rs:80`) rebalances the **share** each sector holds (`SectorShare.gdp_share`) from the `extra["employment"]` integer, but it does **not** recompute the total `budget.gdp`. It also has a latent bug: it writes `extra["zatrudnienie"]` from `building.current_employment` (lines 86–93) but then sums `extra["employment"]` (lines 104–113) — two different keys — so the share rebalance often operates on stale or zero values.
- `run_economic_turn` (line 151) is called from `engine/turn.rs:331` and does nothing beyond the share update + `infrastructure::effects::apply_infrastructure_effects`.

**Empirical proof:** `state/SIMULATION_100_TURNS_RESULTS.md` shows total capital going from 182B (turn 0) to 132.7T (turn 40) and then **flat at 132.7T for turns 40–90** — a 50-turn plateau with sub-cent drift in `state_liquid_reserves`. That is not a working economy; that is a frozen balance sheet.

**Does it include Phase 20–23 B2B services and the shadow economy?** No, because it is never recomputed at all. Even if it were, the current `SectorShare` model only knows about the canonical `Sector` enum; the Phase 20 B2B services (Freight, HealthCapacity, EducationSlots, MaintenanceServices, PassengerTransport, ConstructionServices) are `Commodity` variants, not sectors, so a value-added GDP sum would have to aggregate trade settlement values — which the engine *does* produce (`Trade` records with `execution_price` and `quantity`) but never rolls up.

**Recommendation (P0):** Before displaying GDP anywhere, implement a real GDP computation. Two viable approaches, in order of correctness:
1. **Expenditure approach** — sum final B2C consumption (`economy/trade/retail.rs` clearing results) + government spending (ministry procurement) + investment (construction project spend + fixed-asset purchases) + net trade (`balance_global_trade` deltas). This is the most defensible because all four legs already produce auditable cash flows through `TransferSettler`.
2. **Value-added approach** — sum `(output_value − input_cost)` per building from `process_building_cycle` results, plus B2B service margins. Harder because intermediate goods must be netted out.

Either way, **the shadow economy must be added explicitly**: `ShadowEconomyState.total_hidden_fte` and shadow wages (`economy/justice/legal_status.rs:42`) are real economic activity that conventional GDP misses; a parallel "shadow GDP" line should be computed and reported alongside the official figure.

### 2.2 Inflation

**Status: BROKEN — never computed.**

- `MacroData.inflation` (`state/macro_data.rs:477`, `pub inflation: f64`) is read in three places: `state/banking.rs:1881` (central-bank rate setting), `economy/finance/debt_market.rs:581` (real-rate deflation), `international/trade.rs:292` (tariff escalation). It is **written** nowhere outside world-gen and test setup.
- There is no CPI basket, no price index, no Paasche/Laspeyres/Fisher computation anywhere in the codebase. A `grep` for `cpi|price_index|compute_inflation|update_inflation` returns only the `vwap_*` royalty machinery and a single `inflation_rate` local in `debt_market.rs`.
- The raw material for a proper inflation figure **does exist**: `economy/market/market_history.rs::update_vwap` (line 59) is called every turn at `engine/turn.rs:1234` and maintains `MarketHistory.vwap_per_commodity` across all 130+ commodities. The engine also tracks `last_trade_price` and `global_base_prices`. None of this is rolled up into a headline index.

**Does it track VWAP of the 130+ commodities?** No — VWAP is tracked per commodity but no weighted aggregate is formed. A correct implementation would:
1. Define a CPI consumption basket with weights (derivable from `data/consumption_registry.rs` B2C demand profiles).
2. Each turn, compute `index_t = Σ(weight_i × vwap_i,t) / Σ(weight_i × vwap_i,t-1)`.
3. Set `macro_indicators.inflation = (index_t − index_{t-1}) × 100`.
4. Track a separate **producer price index** from B2B input VWAPs (Freight, MaintenanceServices, HardCoal, Steel, Fuels) since those drive the cost-push channel into `labor.rs::minimum_egzystencjalne`.

**Recommendation (P0):** Inflation is consumed by the central bank, debt market, and trade systems, so a frozen value silently corrupts monetary policy, debt service, and tariffs. This must be wired before the UI displays it — otherwise the UI would lend false credibility to a number that is driving real downstream behavior.

### 2.3 Unemployment & Demographics

**Status: PARTIALLY WORKING — but blind to OHS casualties and commuters.**

- The unemployment rate **is** recomputed each turn in `economy/labor/labor.rs::process_demographics_and_labor` (lines 300–364). The formula:
  - `bezrobotni = sila_robocza − labor_market.employed_total` (line 300)
  - `stopa_bezrobocia = bezrobotni / sila_robocza × 100`, minus 2 pp if the labor agency is active, floored at frictional `1.5`/`3.0` pp (lines 301–306).
  - Tier employment is then re-derived from `employment_factor = 1 − stopa/100` (line 330) and the headline rate is recomputed from the tier sums (lines 355–364) so the three figures stay mutually consistent.
- This is called from `engine/turn.rs:306` (`process_demographics_and_labor`) and again implicitly through `economy/labor_market::resolve_regional_labor_market` at line 1334.

**Gaps relative to Phase 18–24 mechanics:**

1. **OHS casualties do not feed back into the labor pool.** `economy/production/disasters.rs` (lines 250–251) subtracts building-collapse casualties from `region.population`, but it does **not** update `class_demographics.*.available_fte` or `labor_market.unable_to_work`/`active_disabled`. The disabled/dead cohorts from OHS accidents (Phase 22B `construction/fraud.rs` OHS cutting, Phase 15A industrial fires) are therefore invisible to the unemployment rate — a worker killed in a collapsed building simply vanishes from the headcount, which mechanically *improves* the unemployment rate rather than worsening it. The `LaborMarket.active_disabled` and `unable_to_work` fields exist (`macro_data.rs:360–364`) but are never written from the disaster path.
2. **Commuters are double-counted.** `engine/turn.rs:1311–1329` injects `commuter_inflow_fte` into the host region's labor clearing, and `labor_allocation.commuter_wages` are remitted back to adjacent regions (lines 1352–1399). But the commuter FTE is not subtracted from the home region's `employed_total`, so the same worker can appear as employed in both regions — inflating `employed_total` and deflating the unemployment rate.
3. **Shadow workers are excluded by design** (`legal_status.rs` pays shadow wages off-books), which is correct for the *official* rate, but the UI should also surface a "true labor utilization" figure that includes `ShadowEconomyState.total_hidden_fte`.
4. **Demographics** (`macro_data.rs:172` `Demographics`) tracks `last_births`/`last_deaths`/`last_migration` but the UI only prints `birth_rate`/`death_rate`/`net_migration`/`average_age`/`urban`/`rural` — no immigrant cohort breakdown, no legal-status composition, no brain-drain index.

**Recommendation (P1):** Fix the casualty→labor feedback (disasters should decrement `available_fte` and increment `unable_to_work`/`active_disabled` on the affected class demographics). Fix the commuter double-count by marking home-region FTE as "commuting out" before the host clears. Then the UI can display a trustworthy unemployment figure plus a "true utilization" line.

### 2.4 Money Supply (M0 / M3)

**Status: BROKEN — formulas exist but are never called.**

- `state/central_bank.rs::CentralBank` defines `calculate_m0` (line 201), `calculate_m3` (line 216), and `calculate_money_multiplier` (line 229). The math is correct and documented.
- A `grep` for `calculate_m0|calculate_m3|calculate_money_multiplier` across `state/src` finds **12 matches, all inside `central_bank.rs` itself** — 4 are the definitions, 8 are unit tests. **The turn loop never calls them.** There is no `m0`/`m3` field on `CentralBank` or `MacroData` to store the result, and no aggregation step that sums cash-in-circulation, bank reserves, demand deposits, and time deposits across all `Company.brokerage_account` and `BankBalanceSheet` ledgers.
- The `TransferSettler` overhauls (Phase 5+) produce perfectly double-entry-consistent ledgers — every `settle_transfer`, `settle_wage_payment`, `settle_company_to_company` moves cash between two named accounts. The raw material for M0/M3 is therefore present; only the aggregation is missing.

**Recommendation (P0):** Add a `MoneySupplySnapshot { m0, m3, multiplier, cash_in_circulation, bank_reserves, demand_deposits, time_deposits }` struct, compute it once at the end of each turn by walking all `Company.brokerage_account.cash` + `BankBalanceSheet.*` + `Treasury.liquid_reserves` + `MacroData` citizen savings, store it on `CentralBank` (or `MacroData.extra`), and surface it in the UI. This is cheap (one pass over already-resident data) and closes a major blind spot — the central bank is currently setting reference rates against a frozen inflation number with no idea how much money it has created.

### 2.5 Indicator audit summary table

| Indicator | Field | Recomputed in turn loop? | Source of truth | Verdict |
|-----------|-------|--------------------------|-----------------|---------|
| GDP (PKB) | `budget.gdp` | **No** | world-gen only | **P0 broken** |
| GDP sector shares | `SectorShare.gdp_share` | Yes (`indicators.rs:80`) | employment extra | Works, but key mismatch (`zatrudnienie` vs `employment`) |
| Inflation | `macro_indicators.inflation` | **No** | world-gen only | **P0 broken** |
| VWAP per commodity | `market_history.vwap_per_commodity` | Yes (`market_history.rs:59`) | trade clears | Works; not rolled up into an index |
| Unemployment | `labor_market.unemployment_rate` | Yes (`labor/labor.rs:300`) | labor clearing | Works but ignores OHS casualties & commuter double-count |
| Demographics | `macro_indicators.demographics` | Partially | `process_demographics_and_labor` | Births/deaths/migration tracked; disabled cohort not fed from disasters |
| M0 | — | **No** | `calculate_m0` only in tests | **P0 broken** |
| M3 | — | **No** | `calculate_m3` only in tests | **P0 broken** |
| Money multiplier | — | **No** | `calculate_money_multiplier` only in tests | **P0 broken** |

---

## PART 3 — The UI Expansion Blueprint

### 3.1 Strategic framing

Per the audit findings above, **the indicator layer must be fixed before the UI displays it.** Surfacing a frozen GDP or a never-updated inflation in a shiny new TUI would be actively harmful — it would lend false credibility to numbers that are silently corrupting monetary policy, debt service, and tariffs downstream. The blueprint therefore sequences the work as:

1. **P0 — Wire the math** (GDP, Inflation, M0/M3, casualty→labor feedback). No UI work yet.
2. **P1 — Build the lightweight TUI shell** with `ratatui` + `crossterm`, displaying the *now-correct* headline indicators.
3. **P2 — Add Phase 18–24 panels** inside the TUI shell.

The user has approved a **lightweight TUI based on `ratatui` + `crossterm`** (no full graphical client). This is the right call: the current 540-line `println!` menu has hit its complexity ceiling, every new phase adds another opaque subsystem, and a tabbed TUI with scrollable tables is the minimum viable presentation for 130+ commodities and ~30 subsystems.

### 3.2 Proposed dependency additions

Add to `state/Cargo.toml`:

```toml
ratatui = "0.28"      # stable since 2024; 0.29 also fine, pick a ≥7-day-old release
crossterm = "0.27"    # ratatui's default backend; matches ratatui 0.28
```

Both are pure-Rust, MIT/Apache-2.0, no native deps, work on Windows (the user's platform). They are widely vetted (ratatui is the maintained fork of `tui-rs`). Pin to a release at least 7 days old per the supply-chain rule.

### 3.3 Structural refactor — the "Snapshot + Renderer" pattern

The core problem with `reports.rs` is that **data gathering and presentation are interleaved** inside 100+ line `print_*` functions that each re-derive their own aggregates. Adding a panel means copying that pattern, which is why nothing has been added since Phase 2.

The refactor separates three concerns:

#### A. A pure `CountrySnapshot` aggregator struct

A single function `build_country_snapshot(country, companies, buildings, market, history) -> CountrySnapshot` that walks all the state once and produces a flat, UI-ready struct. This is the **only** place that knows how to compute "total structural defects across all buildings" or "average network condition" — the UI just reads fields. Adding a new panel becomes "add a field to `CountrySnapshot` + a line in `build_*`", not "write a new 100-line printer".

```rust
// proposed: state/src/ui/snapshot.rs
pub struct CountrySnapshot {
    // headline
    pub gdp: f64,
    pub gdp_growth_yoy: f64,
    pub inflation: f64,
    pub unemployment: f64,
    pub true_labor_utilization: f64, // includes shadow FTE
    pub money_supply: MoneySupplySnapshot,
    // phase 18-24
    pub corruption_index: f64,
    pub structural_defects: StructuralDefectSummary,
    pub geology: GeologySummary,
    pub ohs: OhsSummary,
    pub infrastructure: InfrastructureSummary,
    pub kio: KioSummary,
    pub shadow_economy: ShadowEconomySummary,
    // ... one sub-struct per panel
}
```

#### B. A reusable table renderer

A small helper that takes `&[(label, value, delta)]` rows and renders a `ratatui` `Block` + `Row`s, with column widths auto-sized. This kills the `{:<32} | {:>15}` printf duplication and makes every panel look consistent. Roughly:

```rust
fn render_kv_table(frame: &mut Frame, area: Rect, title: &str, rows: &[(String, String, Option<f64>)]);
fn render_commodity_table(frame: &mut Frame, area: Rect, title: &str, rows: &[(Commodity, f64, f64, f64)]);
```

The `Option<f64>` delta column lets the UI show ▲/▼ turn-over-turn for any indicator that `build_country_snapshot` can compute a previous-turn delta for (store last snapshot on `GameState.extra` or a new `last_snapshot` field).

#### C. A tabbed TUI shell

Replace `ui::console::run` with a `ratatui`-based event loop. Layout:

```
┌─ SIM ENGINE ──────────────────────── Turn 42 / 1992 ──┐
│ [1]Macro [2]Budget [3]Banking [4]Justice [5]Corporate │  ← tab bar
│ [6]Market [7]Geology [8]Construction [9]Logistics     │
│ [10]Society [11]Military [12]Diplomacy                │
├───────────────────────────────────────────────────────┤
│                                                       │
│            active tab content (scrollable)            │
│                                                       │
│                                                       │
├───────────────────────────────────────────────────────┤
│ q:quit  n:next turn  r:reload  /:filter  h:help       │  ← status bar
└───────────────────────────────────────────────────────┘
```

- **Country selector** as a left sidebar or a `Tab`-style popup when multiple countries exist.
- **`n` runs a turn in-place** (calls `engine::run_turn` then rebuilds the snapshot) — this fixes the current UX where running a turn drops you back to the text menu with no feedback.
- **`/` opens a filter row** for the commodity/company tables (essential with 130+ commodities).
- **`h` help**, **`q` quit**, **`r` reload from disk**.
- The legacy `println!` menu can be kept behind a `--legacy` flag for one release as a fallback, then removed.

### 3.4 Phase 18–24 panels — concrete content

Each panel maps directly to existing state fields; the work is reading them in `build_country_snapshot`, not building new engine systems.

#### Panel: Corruption & Justice (Phase 14, 18B, 22D)
- `JusticeLaw.corruption_index` (`politics/laws.rs:185`) — headline gauge 0.0–1.0.
- `ShadowEconomyState` (`economy/justice/legal_status.rs:59`): `total_hidden_fte`, `total_pit_evaded`, `total_remittances_outbound`, `raids_conducted`, `fines_collected`, `legalized_this_turn`.
- `Politics.justice_state` coverage ratio, court wait time category, pardon authority.
- `Country.phase22_kio_appeals` (`state/mod.rs:467`): pending vs. upheld vs. rejected counts, recent appeal list with tender_id / appellant / respondent / grounds.
- Blacklist: companies with `reputation_score` below the KIO threshold (`government/kio.rs`), listed with their sector and last fraud/loss record.

#### Panel: Structural Defects (Phase 22B, 24A.9)
- Aggregate `Building.structural_defect` (`entities/mod.rs:1050`) across all buildings: count with defect > 0, mean defect, worst-10 buildings (id, sector, defect, condition).
- Active `ConstructionProject.structural_defect` (`construction/projects.rs:145`) per project: project id, contractor, defect level, OHS coverage ratio, material-substitution fraud flag.
- Collapse risk projection: `defect × 0.05` per-turn probability (`economy/production/disasters.rs:240`) summed to a national "expected collapses/turn".
- Demolition/halt queues (`Country.demolition_queue`, `halt_queue`) — pending counts.

#### Panel: Geology & Depletion (Phase 21A)
- Per-formation table (`Country.geological_formations`, `society/geography.rs:139`): formation name, type, overlapping regions, total area.
- Per-deposit sub-table (`ResourceDeposit`, `geography.rs:110`): commodity, `estimated_reserves`, `current_reserves`, **depletion %** = `1 − current/estimated`, `quality` vs `current_quality` (quality decay), `depth`, `discovered` flag, accessibility given current method year (`geology.rs::deposit_is_accessible`).
- Highlight deposits > 80% depleted in red; flag deposits that are discovered but inaccessible (depth > `max_depth_for_method_year(year)`).

#### Panel: OHS & Accidents (Phase 15A, 22B)
- Current-turn `DisasterEvent` log (`economy/production/disasters.rs:38`): type, region, severity, buildings destroyed, casualties, economic damage. Filterable by type.
- Rolling accident rate: casualties per turn over last N turns (store a ring buffer on `Country` or compute from a new `disaster_history` field — currently disasters are fire-and-forget, so this needs a small persistence addition).
- OHS coverage per active construction project (`ConstructionProject.ohs_coverage_ratio`, `projects.rs:163`): mean, min, projects below compliance threshold.
- OSP volunteer FTE allocation summary (`economy/state_sector/osp.rs`): total volunteer FTE, per-region coverage.

#### Panel: Infrastructure Network (Phase 23B, 23C)
- `TransportNetworkOverlay` (`economy/logistics/transport_networks.rs`): per-`NetworkLink` table — region_a → region_b, `NetworkLevel` (None/DirtRoad/PavedRoad/Rail/ElectrifiedRail/Highway/Canal), `condition` 0–1, `built_turn`, effective friction multiplier.
- Aggregate: km of paved/rail/highway, mean condition, links below 0.5 condition (maintenance backlog).
- `FreightLogisticsConfig` + `Country.deferred_trades`: deferred-trade count, total deferred value, reason breakdown (`DeferredReason`).
- Commuting: `CommutingConfig`, last-turn `commute_coverage` map (region → 0–1), commuter FTE inflow, commuter wages remitted.

#### Panel: KIO & Tender Market (Phase 22A, 22D)
- `Country.phase22_tenders`: open tenders with target building type, micro-region, investor, status.
- `Country.phase22_kio_appeals`: as above.
- Recent tender awards (from `construction::tender_market::process_tender_awards` — currently not persisted; recommend persisting a small `tender_history` for the UI).
- Contractor reputation league table: top/bottom 10 by `reputation_score`.

### 3.5 Migration plan (ordered, low-risk)

1. **Add `ratatui` + `crossterm` deps**; verify Windows build.
2. **Create `state/src/ui/snapshot.rs`** with `CountrySnapshot` + `build_country_snapshot` + unit tests (pure function over `&Country` + `&[Company]` + `&[Building]` + `&GlobalMarket` + `&MarketHistory`). No presentation code here.
3. **Create `state/src/ui/tui/`** with `app.rs` (event loop), `render.rs` (the two table helpers), and one `panel_*.rs` file per tab. Start by porting the existing 5 `print_*` reports verbatim into panels so behavior is preserved.
4. **Wire `n` to run a turn** and rebuild the snapshot; store the previous snapshot for delta arrows.
5. **Add the Phase 18–24 panels** from §3.4, one per commit, each backed by new fields on `CountrySnapshot`.
6. **Keep `ui::console::run` as `ui::console::run_legacy`** behind `--legacy` for one release; then delete.
7. **Persist the missing telemetry** that the UI will need: `disaster_history` (ring buffer), `tender_history`, `labor_allocation`/`commute_coverage`/`see_remittance` from `CountryTask` onto `Country` at end-of-turn. These are small additions to the persist step in `engine/turn.rs`.

### 3.6 What this blueprint deliberately does NOT do

- **No full graphical client.** Per the task constraint, this stays terminal-only. A future web/egui client can reuse `CountrySnapshot` as its data contract, so the refactor is forward-compatible.
- **No new economic simulation.** The blueprint only reads existing state. The P0 indicator fixes (GDP/Inflation/M0-M3 math) are called out as prerequisites but are **out of scope for the UI work itself** — they are separate follow-up tasks.
- **No removal of the `extra` catch-alls.** `MacroData.extra` / `Treasury.extra` remain the lossless round-trip boundary; `CountrySnapshot` is a derived read model, not a replacement.

---

## Appendix A — Key file:line references

| Subject | File | Lines |
|---------|------|-------|
| UI entry | `state/src/main.rs` | 1–5 |
| UI menu loop | `state/src/ui/console.rs` | 26–54 |
| Report menu | `state/src/ui/reports.rs` | 48–88 |
| Macro report printer | `state/src/ui/reports.rs` | 130–197 |
| Market report printer (hand-rolled schema) | `state/src/ui/reports.rs` | 19–38, 396–421 |
| GDP field | `state/src/state/treasury.rs` | 223 |
| GDP share update (only working indicator fn) | `state/src/economy/indicators.rs` | 80–133 |
| `run_economic_turn` stub | `state/src/economy/indicators.rs` | 151–161 |
| Inflation field | `state/src/state/macro_data.rs` | 477 |
| Inflation readers (no writer) | `state/src/state/banking.rs` | 1881; `economy/finance/debt_market.rs` 581; `international/trade.rs` 292 |
| VWAP update (called every turn) | `state/src/economy/market/market_history.rs` | 59–80; called at `engine/turn.rs` 1234 |
| Unemployment computation | `state/src/economy/labor/labor.rs` | 300–364 |
| Labor market clearing call | `state/src/engine/turn.rs` | 1334 |
| Commuter inflow injection | `state/src/engine/turn.rs` | 1311–1329 |
| Commuter wage remittance | `state/src/engine/turn.rs` | 1352–1399 |
| Disaster casualties (not fed to labor) | `state/src/economy/production/disasters.rs` | 250–251 |
| M0/M3 formulas (tests only) | `state/src/state/central_bank.rs` | 201, 216, 229 |
| LaborMarket disabled fields (unused) | `state/src/state/macro_data.rs` | 360–364 |
| Corruption index | `state/src/politics/laws.rs` | 185 |
| Building structural defect | `state/src/entities/mod.rs` | 1050 |
| Construction project defect/OHS | `state/src/construction/projects.rs` | 140–163 |
| OHS fraud (corner-cutting) | `state/src/construction/fraud.rs` | 1–9, 149–186 |
| Geological formation | `state/src/society/geography.rs` | 139–158 |
| Resource deposit + depletion | `state/src/society/geography.rs` | 110–132; `economy/production/geology.rs` 120–152 |
| Transport network overlay | `state/src/economy/logistics/transport_networks.rs` | 27–96 |
| KIO appeals | `state/src/government/kio.rs` | 1–176; `state/src/state/mod.rs` 467 |
| Shadow economy state | `state/src/economy/justice/legal_status.rs` | 42–78 |
| 100-turn sim plateau evidence | `state/SIMULATION_100_TURNS_RESULTS.md` | 9–21 |

## Appendix B — Recommended P0 fix order (prerequisites for trustworthy UI)

1. **GDP recompute** — expenditure-side roll-up from `TransferSettler` cash flows + shadow GDP parallel line.
2. **Inflation** — CPI basket from `consumption_registry` weights × VWAP; store index history for delta display.
3. **M0/M3 snapshot** — one pass over `Company.brokerage_account` + `BankBalanceSheet` + `Treasury.liquid_reserves` + citizen savings; store on `CentralBank`.
4. **Casualty→labor feedback** — disasters decrement `available_fte`, increment `unable_to_work`/`active_disabled`.
5. **Commuter double-count fix** — mark home-region FTE as "commuting out" before host clears.

Only after these five land should the TUI display the headline numbers without a "(STATIC)" disclaimer.
