# STATE OF THE ENGINE — INITIALIZATION MANIFEST
**Project:** `SillyElaborateState` — Rust grand-strategy macroeconomic simulation
**Crate:** `sim_engine` (lib) at `state/`
**Checkpoint:** End of Phase 6.3.5 (Data Refactor & Commodity Abstraction) — compiles clean (`cargo check` exit 0)
**Purpose of this document:** Prime a fresh AI assistant with the engine's architecture, invariants, and simulation rules. Read this fully before proposing any change.

---

## 0. ENVIRONMENT

| Item | Value |
|---|---|
| Host OS | Windows, PowerShell |
| Toolchain | `stable-x86_64-pc-windows-gnu` (MSVC linker absent — GNU is mandatory) |
| Cargo path | Not on PATH. Invoke as `& "$env:USERPROFILE\.cargo\bin\cargo.exe"` |
| Working dir for cargo | `c:\Users\netse\Downloads\SillyElaborateState\state` |
| Core deps | `serde`, `serde_json`, `rayon` |
| Lints | `#![deny(missing_docs)]` — **every** public item requires a doc comment or the build fails |
| Docstring standard | `# Arguments` / `# Returns` / `# Rules` sections |

**Serialization contract:** All persisted structs use `#[serde(rename = "polish_key")]` to match the legacy Python save format verbatim, plus `#[serde(flatten)] extra: Map<String, Value>` catch-alls for lossless round-tripping. Never rename a serde key without checking `state/data/*.json`.

---

## 1. PROJECT CORE PHILOSOPHY

These are hard invariants. Violating any of them is a bug, not a design choice.

### 1.1 No Magic Numbers
Simulation constants belong in registries, config structs, or explicitly named module-level constants — never inlined in logic. Where placeholders are unavoidable during phased development, they are isolated in a single dedicated module (currently `src/economy/commodity_pricing.rs`) and marked with the phase that will replace them.

Current sanctioned placeholders (Phase 6.3.5 only, to be deleted in Phase 6.4):
- `PLACEHOLDER_TRANSPORT_FEE_PER_TON: f64 = 15.0`
- `STATE_OWNER_ID: &str = "STATE_TREASURY"`
- `get_base_commodity_price(&Commodity) -> f64` — static price table
- Two hardcoded `200.0` fallback prices in `agriculture.rs` state-transition revenue paths
- Base wage `100.0` in `calculate_agricultural_fte_demand`

### 1.2 Strict Double-Entry Accounting
**Money is never created or destroyed outside the Treasury.** Every debit has a matching credit. Concretely:
- Farmer pays seed cost → `company.liquid_capital -=` **and** `treasury.liquid_reserves +=` (B2B flow)
- Farmer pays transport → `company.liquid_capital -=` **and** `treasury.logistics_revenue +=`
- Liquidator seizes revenue → `treasury.liquid_reserves +=` from the sold inventory
- Storage fees accrue as `batch.accumulated_fees` (a *debt counter*), settled exactly once at extraction or rot. **Never** debit `liquid_capital` at accrual time — this was a real double-dipping bug and the guard comment remains in `accumulate_storage_fees`.

### 1.3 Physical Limitations
- Goods occupy **physical warehouse space**. `deposit_inventory` returns the *excess* that did not fit; callers must handle it.
- A company with zero warehouses cannot store anything — 100% of its harvest overflows.
- Goods **cannot teleport**. Any movement of tonnage between a producer and a market incurs `PLACEHOLDER_TRANSPORT_FEE_PER_TON`, credited to `treasury.logistics_revenue`. This applies to fire sales *and* auto-sells; omitting it creates a logistics exploit.
- Inventory is **owner-tagged**. A warehouse may hold batches belonging to many companies simultaneously (multi-tenant). All reads and writes must filter by `owner_id` or you will bill/rob the wrong company.

### 1.4 No AI Bypasses
Entities are subject to the same rules as the player. A bankrupt company does not vanish with its assets — buildings and land are explicitly transferred to `STATE_OWNER_ID` before the shell is despawned. Orphaned entities are a correctness failure.

### 1.5 Determinism
Parallel execution uses `rayon`, but ordering-sensitive aggregations sort explicitly (see `price_samples` sorting in `engine/turn.rs`) so float accumulation is independent of thread scheduling. `BTreeMap` is preferred over `HashMap` for anything that feeds a float reduction or a save file.

---

## 2. TURN SEQUENCE (THE MAIN LOOP)

### 2.1 Global Orchestrator — `src/engine/turn.rs::run_turn`
Loads `market.json`, `diplomacy.json`, turn/year counters, then per-country `Company`/`Building`/`Union` entities (sorted by ID for determinism). Builds one `CountryTask` per country and executes the following phases, each as a **full `rayon` barrier across all countries** before the next begins:

```
 1. process_demographics_and_labor      // population, labor supply
 2. process_banking_system              // credit, interest, solvency
 3. update_gdp_shares_from_employment   // sector share recalculation
 4. process_unions                      // collective bargaining
 5. process_building_cycle              // industrial production -> MarketOrders
 6. resolve_market_prices               // local clearing + MarketSignal
 7. process_companies                   // corporate decisions vs. market signal
 8. CompanyLifecycle::process_lifecycle // founding, bankruptcy, restructuring
 9. collect_taxes -> process_government_spending
10. process_political_year
11. add_military_demand_to_market
12. [persist] save companies/buildings/unions
13. merge_orders -> global price update -> balance_global_trade
```

### 2.2 Agricultural Sub-Sequence — `src/agriculture.rs`
Phase 6.3.5 built and unit-verified this pipeline. **It is now wired into `run_turn` as Phase 6.5** (between market clearing and corporate processing). The intended order, per company, per 24-tick calendar turn:

```
A. transition_agricultural_states     // Idle -> Sowing -> Growing -> Harvesting -> Idle
B. calculate_agricultural_fte_demand  // sets physical_fte_demand, target_fte_demand, offered_wage_per_fte
C. resolve_regional_labor_market      // competitive bidding -> sets fulfilled_fte
D. calculate_harvest_yield_and_rot    // rot accrual + multi-yield FEFO deposit + overflow fire sale
E. accumulate_storage_fees            // debt counter on every batch
F. apply_perishability                // FEFO decay, rot fee settlement
G. auto_sell_inventory_placeholder    // 20% solvent / 100% receivership + logistics fee + asset transfer
H. financial settlement / despawn     // consume the Option<String> company_id returned by (G)
```

**Ordering constraint:** (B) must run before (C), and (C) before (D), because rot is computed from `fulfilled_fte / physical_fte_demand`. Running (D) before labor clears silently zeroes labor efficiency and rots every crop.

### 2.3 Calendar
24 ticks per year. `calendar.global_turn % 24` yields the in-year turn; **turn 0 is treated as invalid and skipped**. Crop schedules are `TurnRange { start_turn, end_turn }` and harvest duration arithmetic is wrap-around safe (`if duration < 0 { duration += 24 }`).

---

## 3. KEY DATA STRUCTURES

### 3.1 `Treasury` — `src/state/treasury.rs`
The single sink and source of legitimate money creation. Relevant fields:

| Field | Serde key | Meaning |
|---|---|---|
| `liquid_reserves` | `płynne_rezerwy` | Spendable state cash. Receives B2B seed purchases and liquidation proceeds. |
| `liquidation_expenses` | — | **Accumulative tracker.** Never reset, never decremented. Records the cost of running receiverships. |
| `logistics_revenue` | `przychód_logistyczny` | **Phase 6.3.5.** Credit side of every transport fee debited from a company. Enables verification that no tonnage teleported. |
| `gdp`, `population`, `nominal_budget`, `citizen_savings`, `private_capital` | Polish keys | Macro aggregates |
| `sectors`, `allocations`, `science`, `banks`, `outstanding_corporate_debts` | Polish keys | Nested state |

Any new `Treasury` field must be added to **both** the struct and the literal initializer in `src/engine/generator/mod.rs`, or the generator fails to compile (E0063).

### 3.2 `Company` — `src/entities/mod.rs`
No `inventory` field. This is deliberate — the "ghost `Inventory` struct" was removed. A company's goods live exclusively inside `CommercialBuilding`s it owns.

| Field | Serde key | Meaning |
|---|---|---|
| `liquid_capital` | `liquid_capital` | Cash. Clamped at 0; shortfalls become `liabilities`. |
| `liabilities` | `liabilities` | Debt accrued when the company cannot cover a settlement |
| `physical_fte_demand` | `zapotrzebowanie_fizyczne` | **Raw** labor requirement, *before* liquidity clamping. Rot is measured against this to prevent the "Broke Farmer Exploit" (a company that cannot afford labor still suffers neglect damage). |
| `target_fte_demand` | `zapotrzebowanie_fte` | Liquidity-clamped bid. Equals `physical_fte_demand` when Treasury-funded under receivership. |
| `offered_wage_per_fte` | `płaca_za_fte` | Bid price, scaled by crop-phase wage multipliers |
| `fulfilled_fte` | `zrealizowane_fte` | FTE actually won at market clearing |
| `worker_capacity` | `worker_capacity` | Structural headcount ceiling |
| `building_ids` | `building_ids` | **The only link to physical storage.** Iterate these to reach warehouses. |
| `is_in_receivership` | `zarząd_komisaryczny` | Active Liquidator flag |
| `agricultural_profile` | `profil_rolny` | `Option<AgriculturalProfile>` containing `Vec<CropBatch>` |

### 3.3 `CommercialBuilding` — `src/society/housing.rs`
Owns inventory and exposes it **only** through encapsulated methods. Direct external mutation of `current_inventory` is forbidden.

**Storage location:** `entities/<country>/commercial/<type>.json` (e.g., `entities/Poland/commercial/warehouse.json`)

```rust
pub struct CommercialBuilding {
    pub id: String,                                                   // "id_budynku"
    pub building_type: CommercialBuildingType,                        // "typ_budynku"
    pub owner_id: String,                                             // "właściciel" — Phase 6.3.5, target of State asset transfer
    pub storage_capacity: f64,                                        // "pojemność_magazynowa"
    pub current_inventory: BTreeMap<String, Vec<InventoryBatch>>,     // "aktualny_inwentarz" — FEFO batches keyed by commodity
    pub storage_type: StorageType,                                    // General | Cold | LiquidTanks | Hazardous
    pub utilization_rate: f64,                                        // "wskaźnik_wykorzystania"
    pub utility_connections: UtilityConnections,
    // + office_capacity, retail_capacity, tenants, rent_per_sqm
}
```

`InventoryBatch { quantity, storage_turn, owner_id, accumulated_fees, warehouse_id, fire_sale_discount }`

**Public API:**
- `deposit_inventory(commodity_key, quantity, owner_id, current_turn) -> f64` — creates a new batch stamped with `current_turn`; **returns the excess** that exceeded `storage_capacity`. Returns the full quantity if the building is full.
- `withdraw_inventory(commodity_key, quantity, owner_id) -> f64` — sorts batches by `storage_turn` (FEFO), withdraws **only** from batches matching `owner_id`, prunes empty batches and empty commodity keys, returns the amount actually withdrawn.
- `calculate_storage_fee() -> f64` — OPEX-derived per-unit fee: `(sum of utility capacities / storage_capacity) × type_multiplier × (0.5 + utilization_rate × 1.5)`. Type multipliers: General 1.0, LiquidTanks 1.5, Cold 2.0, Hazardous 3.0.
- `update_utilization_rate()` — recomputes from total stored tonnage, clamped to 1.0.
- `apply_perishability(current_turn) -> (f64, Vec<InventoryBatch>)` — FEFO decay by commodity and storage type; returns tonnage destined for landfill and destroyed batches for rot fee settlement. Uses static `perishability_registry()` for commodity-specific decay rates.

### 3.4 Crop Registry & Commodity Abstraction

**`crop_registry()` — `src/data/crop_registry.rs`**
A `OnceLock<HashMap<String, CropDefinition>>` global, thread-safe and lazily initialized on first call. This **replaced** the deleted `state/data/crops.json`. Adding a crop is a compile-time-checked code change, not a JSON edit. Currently registered: `wheat`, `corn`, `potatoes`, `cotton`, `alfalfa`.

**`perishability_registry()` — `src/data/perishability_registry.rs`**
A `OnceLock<HashMap<Commodity, PerishabilityProfile>>` global providing commodity-specific shelf life and decay rates. Replaced magic numbers in `apply_perishability`. Includes entries for all six agricultural commodities (Vegetable, Cereal, Protein, Fodder, IndustrialFiber, Luxury) with distinct general/cold storage parameters.

**`CropDefinition` — `src/registries/crops.rs`**
```rust
pub struct CropDefinition {
    pub id: String,
    pub name: String,
    pub category: CropCategory,          // Root | Cereal | Legume | Industrial | Fodder | Orchard
    pub land_type: LandType,             // Arable (annual, must re-sow) | Plantation (perennial)
    pub compatible_climates: Vec<ClimateProfile>,
    pub sowing_schedule: TurnRange,
    pub harvest_schedule: TurnRange,
    pub labor_demand: LaborDemandProfile, // sowing / growing / harvesting FTE per hectare
    pub yields: HashMap<Commodity, f64>,  // MULTI-YIELD: tons per hectare per commodity
    pub seed_cost_per_hectare: f64,
    pub sowing_wage_multiplier: f64,
    pub harvesting_wage_multiplier: f64,
}
```

**The multi-yield abstraction is the central Phase 6.3.5 change.** The old scalar `base_yield_per_hectare` and `base_price_per_ton` fields are **deleted**. One hectare produces a *bundle* of commodities — corn yields 5.5 t Cereal (grain) **and** 8.2 t Fodder (silage); cotton yields 1.8 t IndustrialFiber **and** 3.5 t Fodder (seed meal). Harvest logic must iterate `crop_def.yields`; any code referencing a single yield scalar is stale.

**`Commodity` — `src/registries/enums.rs`** (108 variants, `Commodity::all()` array length must stay in sync)
Six abstract agricultural commodities were added, replacing the over-specific `MedicinalPlants`, `Oilseeds`, `SpecialtyCrops` (and `Chemicals`, which never existed):

| Variant | Snake key | Covers |
|---|---|---|
| `Cereal` | `cereal` | wheat, corn, rice, barley |
| `Vegetable` | `vegetable` | potatoes, carrots, tomatoes |
| `Protein` | `protein` | beans, soybeans, peas |
| `Fodder` | `fodder` | alfalfa, clover, beet pulp, crop residues |
| `IndustrialFiber` | `industrial_fiber` | cotton, flax, hemp |
| `Luxury` | `luxury` | sugar, coffee, tobacco, spices |

Inventory keys are produced by `format!("{:?}", commodity)` — i.e. the **PascalCase Debug string**, not the snake_case serde key. `auto_sell_inventory_placeholder` parses back with a `match` on those PascalCase strings. Keep both sides consistent.

**Seasonal multipliers:** `ClimateConfig::get_modifiers(climate_profile, season)` returns `SeasonalModifiers`, whose `agriculture_multiplier` scales every yield. Yield formula per commodity:
```
base   = active_hectares × tons_per_hectare × agriculture_multiplier
turn   = base / harvest_duration_turns          // spread across the harvest window
final  = turn × (1.0 - rot_accumulator)
```

---

## 4. CORE MECHANICS ACHIEVED

### 4.1 Labor Market — `src/economy/labor_market.rs`
Companies submit a `LaborBid { company_id, target_fte_demand, offered_wage_per_fte, sector }`. Bids are filtered by `region_id`, liquidity-clamped, rejected below statutory minimum wage (if any), then sorted by `offered_wage_per_fte` descending — **highest bidder consumes FTE first**. A `RegionalLaborPool` holds per-class `ClassLaborLedger`s; wages are tracked **per demographic class, never pooled**, which is what preserves modeled inequality. Class-to-sector fit comes from a data-driven `LaborConfig::suitability_matrix` (missing entries default to 1.0 neutral); the multiplier affects *share of the bid*, not raw FTE count.

Agricultural demand is dynamic: `calculate_agricultural_fte_demand` sums per-batch FTE by crop state (`sowing_fte_per_hectare` / `growing_fte_per_hectare` / `harvesting_fte_per_hectare` × `active_hectares`) and sets the wage offer using the crop's phase multiplier (harvest multipliers run 1.8–3.0×, which is what makes seasonal labor competition bite).

### 4.2 Active Liquidator (Receivership)
When `is_in_receivership == true`:
1. **Treasury funds maintenance.** If any batch is Sowing/Growing/Harvesting, `target_fte_demand = physical_fte_demand` — the state pays the full wage bill to protect the standing crop rather than let a national food asset rot. If all batches are Idle, demand and wage offer drop to zero.
2. **No new sowing.** `Idle -> Sowing` is gated on `!is_in_receivership`. The cycle runs down; it does not restart.
3. **Accumulators preserved.** Solvent companies zero `accumulated_yield` and `rot_accumulator` at `Harvesting -> Idle`; bankrupt ones retain them so the liquidator can value the estate.
4. **Revenue seized.** `auto_sell_inventory_placeholder` with `is_in_receivership = true` liquidates **100%** of inventory in one turn and credits `treasury.liquid_reserves` instead of `company.liquid_capital`.
5. **Physical assets transferred.** Every building in `company.building_ids` has `building.owner_id = STATE_OWNER_ID` before despawn. Land is likewise reclaimed to the state land bank. Nothing is orphaned.
6. **Despawn signalled by return value.** The function returns `Option<String>` — `Some(company_id)` means the caller must remove the shell. Ignoring this return leaks zombie companies.

### 4.3 Agricultural Cycle
- **Fractional sowing.** A farmer sows `min(planned_hectares, liquid_capital / seed_cost_per_hectare)`. A zero-guard handles free seed (`seed_cost == 0` → sow the full plan). Partial liquidity yields a partial field, not a failed turn. Seed cost flows to `treasury.liquid_reserves` as a B2B purchase.
- **Rot accrues in all active states.** `neglect_penalty = (1 - labor_efficiency) × 0.1` is applied every turn in Sowing, Growing **and** Harvesting, where `labor_efficiency = fulfilled_fte / physical_fte_demand` clamped to 1.0. `rot_accumulator` is capped at 1.0. Using `target_fte_demand` here instead of `physical_fte_demand` would reintroduce the Broke Farmer Exploit.
- **Multi-yield FEFO deposit.** For each `(commodity, tons_per_hectare)` in `crop_def.yields`, the turn's yield is offered to each owned `Warehouse` in `building_ids` via `deposit_inventory`, chaining the returned excess to the next warehouse until it reaches zero.
- **Overflow fire sale.** Whatever remains after all warehouses are full is sold from the field at **50% of base price**, minus a full transport fee. A company with no warehouses fire-sales 100% of its harvest — this is intentional brutality that makes storage infrastructure a real strategic investment. Net revenue credits the company; the transport fee credits `treasury.logistics_revenue`.
- **Gradual market absorption.** Solvent companies auto-sell only **20% of inventory per turn**, simulating finite demand and preventing instant liquidation of an entire harvest. Receivership overrides this to 100%.
- **Plantation vs. Arable.** Arable resets `active_hectares = 0` after harvest (must re-sow, must re-pay seed). Plantation preserves `active_hectares` across cycles and skips the Sowing state entirely (`seed_cost_per_hectare = 0`, established once).

### 4.4 Storage Fee Settlement
`accumulate_storage_fees` adds `calculate_storage_fee() × batch.quantity` to `batch.accumulated_fees` — a **debt counter only**. Actual cash moves exactly once, at either extraction (`process_storage_transactions`) or rot (`settle_rot_fees`).

Both settlement paths use **identical strict double-entry logic**:
1. Sale proceeds (`revenue - transport_cost`, extraction only) are credited to the batch owner first — this is the legitimate external market inflow.
2. The withheld `transport_cost` is credited to `treasury.logistics_revenue`. The haulage fee is **transferred to the State, never destroyed** (same convention as the fire-sale path in `agriculture.rs`).
3. The fee is then drained from the owner: `amount_drained = min(accumulated_fees, owner.liquid_capital)`.
4. The warehouse owner is credited **only `amount_drained`** — never the nominal fee. Cash is never conjured to make the warehouse whole.
5. Any unpaid remainder drains `liquid_capital` to zero and is booked as `owner.liabilities` (bad debt borne by the warehouse owner).
6. A self-storage check (owner == warehouse owner) makes the storage fee internal — no money moves. Transport is still levied, because haulage is a real physical cost.
7. If the batch owner cannot be found, the whole transaction is skipped and **no** transport fee is levied, because no sale was settled.

**Invariant:** `Σ(company.liquid_capital) + treasury.logistics_revenue` changes only by the gross sale value. Regression tests in `src/government/treasury.rs::tests` lock this in — notably `insolvent_settlement_conserves_cash`, `insolvent_owner_credits_warehouse_only_what_was_drained`, `transport_cost_is_transferred_not_destroyed`, and `combined_transport_and_insolvent_fee_conserves_cash`.

**Known asymmetry:** the bad debt recorded in `liabilities` has no matching receivable asset on the warehouse owner's books. Cash is conserved; the loss is simply recognized by the creditor. Formal receivables are deferred to the B2B credit system.

---

## 5. KNOWN GAPS / IMMEDIATE NEXT WORK

1. **Phase 6.4 — B2B market.** Delete `commodity_pricing.rs` placeholders and the two `200.0` literals in `agriculture.rs`; route all agricultural sales through real price clearing.
2. **`process_storage_transactions` has no production call site.** The function is fully implemented and tested but is not yet invoked from `run_turn`. Wire it in as part of Phase 6.4, when warehouse extraction is driven by real price clearing.
3. **`GameState.calendar` is not maintained by `run_turn`.** The turn loop derives its own calendar from the `turn`/`year` pair returned by `load_turn_and_year` and hard-codes `start_year: 1900`, while `state.calendar` sits untouched with its own `advance()` method. Two parallel time sources is a latent divergence — collapse them into one.
4. **89 compiler warnings** (unused variables/params across `securities`, `banking`, `waste`). Non-blocking, mostly stubs awaiting implementation.

**Test suite status:** fully green — 237 tests (208 lib + 26 integration + 3 doc), exit code 0.

**Golden master baseline:** `tests/snapshots/full_turn_parity_test__turn_1.snap` was regenerated once the test binary compiled again. The previous baseline predated a regeneration of `state/data/` and described a completely different world (e.g. Anatolia at 3.9M population against the current save's 8.24M), so it was stale rather than a regression signal. The current baseline was accepted only after verifying every country's one-turn population delta falls within ±0.5% and GDP delta is 0.000%. Regenerate with `INSTA_UPDATE=always`, but **always diff against `data/budgets.json` before accepting** — the snapshot is a drift detector, not a correctness oracle.

---

## 6. FILE MAP (agriculture / inventory subsystem)

| Path | Role |
|---|---|
| `src/agriculture.rs` | Crop state machine, FTE demand, harvest + rot, auto-sell/liquidator, land reclamation |
| `src/data/crop_registry.rs` | `OnceLock` static crop definitions (replaced `crops.json`) |
| `src/data/perishability_registry.rs` | `OnceLock` static perishability profiles (replaced magic numbers) |
| `src/registries/crops.rs` | `CropDefinition`, `CropCategory`, `LandType`, `TurnRange`, `LaborDemandProfile` |
| `src/registries/enums.rs` | `Commodity` (108 variants), `Sector`, `Commodity::all()`, `Commodity::inventory_key()` |
| `src/economy/commodity_pricing.rs` | Placeholder prices, transport fee, `STATE_OWNER_ID` |
| `src/society/housing.rs` | `CommercialBuilding`, `InventoryBatch`, deposit/withdraw/perishability/fees |
| `src/state/treasury.rs` | `Treasury` incl. `logistics_revenue` |
| `src/government/treasury.rs` | Fee accumulation, rot settlement, storage transactions, taxes, spending |
| `src/economy/labor_market.rs` | Regional competitive bidding |
| `src/engine/turn.rs` | Global orchestrator `run_turn` with Phase 6.5 agricultural sub-sequence |
| `src/engine/generator/mod.rs` | World generator — **must** mirror every new `Treasury` field |
| `src/io/entity_store.rs` | `Entity` trait for `CommercialBuilding` storage |

---

## 7. RULES OF ENGAGEMENT FOR THE ASSISTANT

- **Verify before asserting.** Read the file. This codebase has had hallucinated field names (`turn_base_yield`, `conversion_rate`, `base_price_per_ton`) cause compile failures. Do not reference a field you have not seen.
- **Minimal upstream fixes.** Fix root causes, not symptoms. Prefer a one-line change to a new abstraction layer.
- **Blueprint before code** for any multi-file architectural change. The user reviews blueprints rigorously and expects mathematical and type-level correctness in pseudo-code.
- **Never weaken a test** to make it pass.
- **Run `cargo check`** after any non-trivial edit. Exit code 0 is the bar.
- **Preserve serde keys.** Renaming one silently breaks every existing save.
