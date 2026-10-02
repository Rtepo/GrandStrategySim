# Macro-Viability Expansion Plan — Rebuilding Economic Viability Beyond Conservation Tests

**Task:** `macro-viability-expansion`
**Author:** Agent 5 (Manager)
**Date:** 2026-10-02
**Status:** PLANNING ONLY — no production code or test suite implemented by this document.
**Base:** `main @ 7bb3a695` (v4.11.1)

---

## 0. Executive Summary

### 0.1 Symptom

Epic/diagnostic tests pass 13/13 (M0 conservation, mass conservation, market
clearing, gridlock < thresholds) yet real-world simulation collapses by
**Turn 3 with 56.64% unemployment**. The suite proves the accounting is
honest while the economy inside the ledger dies. Four structural failures:

| Pillar | Observed | Mechanism (verified in code) |
|---|---|---|
| **Energy black hole** | Grid generates 2770 MW; *every* region reports effective supply **0.0 MW**; spot prices at the annualized ceiling (~$1.5M scale) | `distribute_grid_power` computes `effective_supply = supply.min(lv_cap.min(mv_cap))` where `lv_cap`/`mv_cap` are `HashMap` lookups that **silently default to 0.0** on missing keys (`grid.rs:646-660`). Any region-ID key mismatch, empty persisted map, or re-init with an empty region list zeroes the national distribution bottleneck while generation totals 2770 MW. |
| **Adaptive production** | Companies furlough ~100% of workforce on input shortage (-20M Timber deficit) | `evaluate_furlough` fires `material_shortage` when `avg_fulfillment_ratio < 0.1` and furloughs `fulfilled × (1 - ratio)` ≈ the entire workforce (`strategy.rs:1068-1086`). Blackout penalty 1.0 forces `fulfillment_ratio = 0` (`b2b_orders.rs:1476-1490`), so the grid bug *is* the mass-furlough trigger. Method-switching exists (`manager.rs:723-808`) but only fires when **all** inputs have zero inventory and only considers strictly-fewer-input methods — no partial-shortage path, no substitution tiers, no shift reduction. |
| **Banking LDR 379%** | Cooperative banks at 379% loan-to-deposit | `issue_loan` (`banking.rs:961-1011`) enforces only the reserve constraint `effective_reserves ≥ (deposits + principal) × rr`. Loans create deposits 1:1, so LDR > 100% can only arise if (a) origination paths bypass `issue_loan`, (b) deposits migrate interbank while the loan asset stays behind, or (c) loan proceeds credit borrower `available_cash` without a deposit liability leg. No explicit LDR cap exists anywhere. |
| **Static demographics** | "Peasants" pinned at ~47-48% in every iteration | `geography.rs:2403-2422`: `rural_share` and the rural class split are pure `match start_year` constants. `development_level` (assigned per region at `geography.rs:2122-2137`) only scales *savings* (`dev_savings_mult`, line 2396) — never population structure. Runtime transitions exist (`class_transitions.rs` FreePeasant→Worker) but worldgen seeds the same fixed mix regardless of development tier. |

### 0.2 The Collapse Cascade (Turn 1 → Turn 3)

```
LV/MV capacity maps miss region keys  (serde-default empty map OR key-space
        │                              mismatch OR re-init with empty regions)
        ▼
grid_cap = 0 → effective_supply = 0.0 MW in every region
        │
        ▼
LoadShedTier::Blackout → building_efficiency_penalties = 1.0
        │                              (load_shedding.rs:130-131)
        ▼
fulfillment_ratio *= (1 - 1.0) = 0   (b2b_orders.rs:1476-1485)
        │
        ▼
last_fulfillment_ratio = 0 → avg_fulfillment_ratio < 0.1
        │
        ▼
evaluate_furlough → material_shortage → furlough ~100% of workforce
        │                              (strategy.rs:1068-1086)
        ▼
Production halts → zero revenue → cash burn → M&A/liquidation pressure
        │
        ▼
56.64% unemployment by Turn 3
```

The unemployment is **not** a labor-market failure — it is a grid-keying bug
amplified by a binary furlough rule that has no partial-operation mode.

### 0.3 Deliverable

This document specifies: root-cause probes to disambiguate the grid failure
(§1.3), the architecture for each fix (§1.4, §2.4, §3.4, §4.4), exact test
assertions per pillar (§*.5), turn-loop phase placement (§5), the new epic
test files (§6), and the agent delegation matrix (§7).

---

## 1. Pillar 1 — The Energy Distribution Black Hole

### 1.1 Verified Mechanism

`distribute_grid_power` (`state/src/energy/grid.rs:642-660`):

```rust
let lv_cap = grid.region_lv_capacity.get(region_id).copied().unwrap_or(0.0);
let mv_cap = grid.region_mv_capacity.get(region_id).copied().unwrap_or(0.0);
let grid_cap = lv_cap.min(mv_cap);
let effective_supply = supply.min(grid_cap);   // == 0 whenever a key is missing
```

`unwrap_or(0.0)` is a silent-failure anti-pattern: a missing key is
indistinguishable from a genuinely unwired region. National supply = 2770 MW
while every region reports `effective_supply = 0.0` implies **one** of:

- **H1 — Empty maps at runtime.** `PowerGridState` fields are all
  `#[serde(default)]` (`types.rs:170-207`). A save written before
  `region_lv_capacity` existed deserializes to `{}` → all lookups miss →
  grid_cap = 0 everywhere. `init_power_grid` *clears* then repopulates the
  maps (`grid.rs:86-91`); if it is invoked at load with an empty `regions`
  slice (the exact trap documented at `grid.rs:96-100`), it *wipes* a good map.
- **H2 — Key-space mismatch.** `collect_regional_supply_demand` keys supply by
  `building.region_id` (`grid.rs:357-388`) while capacity is keyed by
  `region.id` and housing demand is resolved through `micro_region_id`
  (`grid.rs:397-406`). If energy buildings carry a different region namespace
  than `country.regions[].id` (e.g., micro-region IDs, stale IDs after region
  regeneration at load), `supply_mw` accumulates under keys the LV/MV loop
  never sees — national total 2770, per-region effective 0.
- **H3 — LV cap starved by demand scale.** LV capacity is seeded at worldgen
  from *projected* demand (`demand × 1.2`, `grid.rs:145-154`). If runtime
  demand grew past the projection (employment scale-factor ×10⁷ corrections),
  `grid_cap` throttles real supply — but that yields *small* nonzero effective
  supply, not 0.0. H3 is a secondary (throttling) defect, not the black hole.

### 1.2 Root-Cause Probes (diagnostic-gated, before any fix)

Add under `#[cfg(feature = "diagnostic")]` in `distribute_grid_power`:

```rust
// GRIDKEY probe — fires once per region per turn:
eprintln!("GRIDKEY[{}]: supply={:.1} lv={:?} mv={:?} maps_lv={} maps_mv={}",
    region_id, supply,
    grid.region_lv_capacity.get(region_id),      // None = key miss (H1/H2)
    grid.region_mv_capacity.get(region_id),
    grid.region_lv_capacity.len(),
    grid.region_mv_capacity.len());
// GRIDORPHAN — supply keyed under IDs absent from country.regions:
for k in supply_mw.keys() {
    if !country.regions.iter().any(|r| &r.id == k) {
        eprintln!("GRIDORPHAN: supply_mw has key '{}' not in regions", k);
    }
}
```

`maps_lv == 0` → H1 (persistence/re-init). `lv/mv` `Some` but `supply` lookup
0 with GRIDORPHAN entries → H2 (key-space). Both `Some` and matched keys → H3.

### 1.3 Fix Specification (per probe outcome)

**If H1 (persistence):**
- On `Country` load (turn_context assembly), run a `reconcile_grid_state`
  pass: for every `region.id` missing from `region_lv_capacity`, seed LV cap
  = `max(runtime_demand_mw × LV_HEADROOM, MIN_PRE_ELECTRIFICATION_MW)` and
  MV = `3 × LV` — the same formulas `init_power_grid` uses — and emit a
  `GRID96-RECONCILE` diagnostic line. Never silently default.
- Guard `init_power_grid`: `debug_assert!(!regions.is_empty())` and refuse to
  clear existing maps when `regions.is_empty()` (defensive — an empty
  repopulation is always a caller bug).
- Add `spot_prices`/`lv_capacity` to the save-version migration checklist so
  old saves get reconstructed caps rather than `#[serde(default)]` emptiness.

**If H2 (key-space):**
- Single source of truth: all grid maps keyed by `Region::id`. In
  `collect_regional_supply_demand`, resolve `building.region_id` through the
  same `resolve_building_region_slice` used for housing (or verify at
  building-spawn that `region_id` is always a `Region::id`). Any unresolved
  key → diagnostic `eprintln!` + counted orphan total, **never** silent drop.

**Regardless of root cause:**
- Replace `unwrap_or(0.0)` with `unwrap_or_else(|| { diagnostic-warn; 0.0 })`
  so missing keys are visible in every future run.
- LV/MV capacity must be **maintained**, not seeded once: each turn, grow
  `lv_cap` toward `max(lv_cap, demand_mw × LV_HEADROOM)` at a construction-
  bounded rate (grid expansion costs materials/labor — see Directive 5:
  cost in `average_wage` units, rate-limited by a `grid_build_rate_mw_per_turn`
  derived from Construction-sector capacity, not a magic number). Persistent
  deficit → `region_grid_investment_gap` telemetry, not silent blackout.

### 1.4 Assertions (new epic test — `grid_distribution_viability_test.rs`)

```rust
// A1 — the headline invariant: generated power reaches the distribution layer.
for (region_id, supply) in &grid_result.region_supply_mw {
    let effective = effective_supply_for(region_id);
    assert!(*supply == 0.0 || effective > 0.0,
        "region {} generated {:.1} MW but distributed 0.0", region_id, supply);
}
// A2 — national reconciliation: Σ effective ≥ 90% of Σ supply when no
//      legitimate curtailment/shedding event fired.
assert!(sum_effective >= 0.9 * sum_generated || shed_event_recorded);
// A3 — no silent keys: every key in supply_mw exists in country.regions,
//      every region has Some(lv_cap) and Some(mv_cap).
// A4 — blackout containment: if LoadShedTier::Blackout fires in any region
//      with supply_mw > 0, fail. Blackout is legal only when supply == 0.
// A5 — spot sanity: spot_price <= annualized ceiling AND spot_price == 0.0
//      in regions where demand == 0.
```

### 1.5 Phase Placement

`distribute_grid_power` runs in the turn at `turn.rs:2583`, before production
consumes `efficiency_penalties` (merged at `turn.rs:2606-2631`) — correct
causal order already; the fix adds the reconcile pass **inside** load/turn-
context assembly (before first `distribute_grid_power` call), never after.

---

## 2. Pillar 2 — Adaptive Production & BOM Flexibility

### 2.1 Verified Mechanism

Two partial-mechanisms exist that never compound:

1. **Physical partial production already exists.** `b2b_orders.rs:1504`
   consumes `required × production_scale × fulfillment_ratio` — a building
   with 30% of its inputs produces 30% output. The sim already models
   reduced-rate operation.
2. **The labor response is binary.** `evaluate_furlough`
   (`strategy.rs:1068-1086`) furloughs `fulfilled × (1 - avg_ratio)` across the
   *whole company* from a *company-wide average* of per-building ratios — one
   starved building drags every building's workers home. At ratio ≈ 0
   (blackout or zero-inventory), that is ~100% furlough.
3. **Method switching is a dead end.** `manager.rs:723-808` switches only when
   *zero* of all inputs are in inventory, and only to methods with strictly
   fewer inputs producing an overlapping output. No substitution (Timber→
   Planks→biomass-fuel chains), no tier downgrade, no partial-shortage
   trigger, and it runs on inventory — not on *market* fulfillment.

### 2.2 Architecture — three mechanisms, in evaluation order

Each step runs inside `process_company`'s strategy evaluation, **before**
`evaluate_furlough`, so furlough becomes the *last* resort on a residual
that survives adaptation:

**M1 — Per-building shortage decomposition (replaces company-average gating).**
`material_shortage` becomes per-building: furlough only the workforce of
buildings whose own `last_fulfillment_ratio < 0.1` — `furlough_i =
employment_i × (1 - ratio_i)` summed over owned buildings. A company with one
starved plant and three supplied ones sheds ~25% of its labor, not ~100%.

**M2 — Substitution-aware method selection (extends the existing switcher).**
- Trigger widens: consider switching when `last_fulfillment_ratio < 0.5`
  (partial shortage), not only at zero inventory.
- Candidate filter extends from "fewer inputs" to "**feasible now**: every
  input either in inventory, purchasable on the region's B2B book at ≤
  `substitution_price_ceiling` (derived from `reference_price`, not a magic
  constant), or substitutable via a new `substitution_groups` table in the
  production-method registry (e.g., `{Energy-input class: HardCoal|Peat|
  Firewood}`, `{Binder: Timber|Planks}`)."
- Pick the feasible method maximizing `output_value − input_cost` at current
  market prices (rational-actor compliance), tie-broken by method name for
  determinism.
- Methods remain registry-resident — technological matrices intact; no
  per-instance ad-hoc recipes.

**M3 — Shift reorganization before furlough.**
If no method switch restores >50% fulfillment, reduce `target_fte_demand` to
`fulfilled × max(fulfillment_ratio, MIN_OPERATING_SHIFT)` where
`MIN_OPERATING_SHIFT = 0.25` (one shift of four — a derived floor: smallest
viable crew fraction per `production_methods` `seat_type` requirements)
rather than zeroing demand. Furlough only the residual workers that no
reduced-shift schedule can carry.

### 2.3 Assertions (extend `market_gridlock_diagnostic_test.rs` + new unit tests)

```rust
// B1 — blackouts do not imply full furlough: over the 4-turn diagnostic,
assert!(unemployment_rate < 0.15);
// B2 — adaptation occurs: count of buildings with a *changed* active_method
//      during shortage turns > 0 when registry contains a feasible
//      alternative (test fixture must guarantee one exists).
// B3 — proportionality: a company whose buildings average 40% fulfillment
//      retains ≥ 30% of its pre-shortage fulfilled_fte (allows M3 floor).
// B4 — no free lunch: substituted inputs are consumed from inventory/market
//      at real prices — mass and M0 conservation audits stay green.
// B5 — determinism: identical seeds produce identical method-switch choices
//      (BTreeMap iteration + name tie-breaks).
```

### 2.4 Out-of-scope guard

M2 must not turn substitution into a supply *generator*: substituted inputs
are still bought/consumed physically (mass conservation), freight still
applies (no teleportation), and methods unavailable before `prod_method.year`
remain gated.

---

## 3. Pillar 3 — Banking LDR & Liquidity Constraints

### 3.1 Verified Mechanism

`issue_loan` (`state/src/state/banking.rs:961-1011`) checks:

```rust
let new_deposits = balance_sheet.deposits + principal;
let required_reserves = new_deposits * central_bank.reserve_requirement_ratio;
if effective_reserves < required_reserves { return Err(...); }
```

This bounds *deposit expansion* to `reserves / rr` (≈10× at rr=0.10) but
never binds `loans ≤ deposits`. Three paths can produce LDR = 379%:

- **P1 — Origination bypass.** Phase 77 fixed `manager.rs` to route corporate
  `BankLoan` through `issue_loan`, but cooperative/member banks may originate
  through other sites (direct `loans_issued.push`, `liabilities` mutations in
  `manager.rs`, ELA facilities, IPO seeding). Every site must be audited.
- **P2 — Deposit flight without reserve settlement.** A borrower spends loan
  proceeds → deposit liability transfers to the seller's bank; if interbank
  settlement moves the deposit but not `reserves_at_central_bank` (or moves
  reserves without capping the losing bank's asset side), LDR inflates
  mechanically.
- **P3 — Proceeds bypass deposits.** If loan disbursement credits borrower
  `available_cash` *without* recording a deposit liability on the lending
  bank, assets grow with no matching liability → LDR diverges and M0
  accounting splits.

### 3.2 Fix Specification

**R1 — Hard LDR gate in `issue_loan` (the single choke point).**
Post-loan invariant:

```rust
let loans_after = total_loans_outstanding(bs) + principal;
let deposits_after = bs.deposits + principal;
let ldr_cap = 1.0 - central_bank.reserve_requirement_ratio; // era-appropriate: 0.90 @ rr=0.10
if loans_after > deposits_after * ldr_cap { return Err(...); }
```

Deriving the cap from `reserve_requirement_ratio` keeps the constraint
*regulatory*, era-scaled, and magic-number-free. Effect: banks lend from
deposits net of required reserves — the classic fractional-reserve bound —
while credit creation still expands both legs 1:1 (double-entry preserved:
debit loan asset, credit deposit liability; no money to/from the void).

**R2 — Origination audit.** Enumerate every `loans_issued` writer and every
site mutating `Company::liabilities`/`available_cash` labeled as credit;
route all through `issue_loan` or a documented state-credit facility.
Cooperative banks get the same gate — "cooperative" is an ownership form,
not a reserve exemption.

**R3 — Settlement completeness.** When a borrower's deposit spends at another
bank, interbank clearing moves `reserves_at_central_bank` with the deposit
(and fires ELA if the paying bank breaches its floor — ELA mechanics already
exist per the prior banking remediation). This keeps LDR stationary under
deposit flight instead of letting it ratchet.

**R4 — Existing-book reconciliation.** For legacy/broken states, a one-time
`reconcile_bank_books` at load: banks with `LDR > ldr_cap` queue an orderly
`deleveraging_schedule` (call in maturing loans pro-rata; no instant
destruction) — never a silent write-off (rational actors; losses post
through equity → existing negative-equity lifecycle handles true insolvency).

### 3.3 Assertions (new epic test — `banking_regulatory_viability_test.rs`)

```rust
// C1 — per-bank regulatory bound every turn:
for bank in banks {
    let ldr = loans_outstanding(bank) / deposits(bank).max(eps);
    assert!(ldr <= 1.0 - rr + eps, "bank {} LDR {:.1}% exceeds cap", bank.id, ldr*100.0);
}
// C2 — aggregate bound: total_loans ≤ total_deposits.
// C3 — refused credit leaves no trace: rejected issue_loan must not change
//      loans_issued, deposits, borrower liabilities, or M0.
// C4 — interbank settlement moves reserves: after any cross-bank payment,
//      Σ reserves_at_central_bank is conserved across the banking system.
// C5 — ELA path: a bank at the reserve floor triggers ELA rather than
//      issuing unbacked credit; ELA loans appear on *both* CB and bank
//      balance sheets (double-entry) and count as cb_lombard_loans
//      (excluded from effective reserves — Phase 77 semantics preserved).
// C6 — deleveraging is orderly: reconciled banks converge below the cap
//      within a bounded window without negative-equity cascades.
```

---

## 4. Pillar 4 — Dynamic Demographics

### 4.1 Verified Mechanism

`geography.rs:2403-2422` seeds class structure from `start_year` alone:

```rust
let rural_share = match start_year { <=1900 => 0.75, <=1925 => 0.65, ... };
let (serf_pct, free_peasant_pct, landless_pct, _) = match start_year { ... };
```

`development_level` (per-region, `geography.rs:2122-2137`) only multiplies
*savings* via `dev_savings_mult` (line 2396). Result: every region at a given
start year gets an identical structural mix — "Peasants" ~47-48% always —
and downstream urbanization (`class_transitions.rs` FreePeasant→Worker when
urban wage > rural subsistence) has nothing differentiated to act on.

### 4.2 Fix Specification

**D1 — Development-conditioned structure (worldgen).**
Replace the scalar era tables with `f(start_year, development_level)`:

- `rural_share(year, dev)` = era baseline interpolated toward urban by
  development: `rural = era_rural × (1 - dev × URBAN_DEV_ELASTICITY)` clamped
  to `[MIN_RURAL_SHARE, era_rural]`, where `URBAN_DEV_ELASTICITY` is derived
  from observed era urbanization deltas (documented derivation, not a bare
  constant) and `MIN_RURAL_SHARE` = subsistence-agriculture floor implied by
  food demand / agricultural labor productivity at the era's technology —
  no economy may drop below the workforce its food BOM requires.
- Within rural: shift serf→free-peasant→landless shares with development
  (land reform proxy): `serf_pct *= (1 - dev)`; freed share splits between
  FreePeasant and LandlessLaborer by era-consistent weights.
- Within urban: worker/middle-class shares scale with industrial GDP share
  (`development_level × era_industrial_share`) — the same weighting worldgen
  already uses for employment allocation, keeping demographics and the
  corporate census consistent.

**D2 — Runtime transitions must actually fire.**
`process_rural_urban_class_transitions` (rural→urban on wage premium)
already exists; verify it runs every turn and that `available_fte` is
credited at the destination class (the "receiving end" accounting that
previously zeroed pools). Add hysteresis so migrants don't oscillate:
a migration only fires when the wage premium persists ≥ N turns.

**D3 — Education/skill coupling.** New Worker arrivals enter at `basic`
skill unless education coverage (agent-4's regionalized shares) lifts them —
keeps demographics honest instead of teleporting peasants into skilled jobs.

### 4.3 Assertions (new epic test — `demographic_dynamics_test.rs`)

```rust
// D1 — structure varies with development: at fixed seed + start_year, a
//      dev=0.1 vs dev=0.9 country differ in peasant share by ≥ 10pp.
// D2 — conservation: total population invariant across the redistribution;
//      Σ class populations == region population (no demographic void).
// D3 — transitions fire: over T turns with urban wage premium, rural→urban
//      migration > 0 and Worker population grows.
// D4 — direction: migration requires premium > threshold; reverse the wages
//      and migration stalls (no unconditional drift).
// D5 — bounds: every class share ∈ [0,1]; rural share ≥ food-system floor.
// D6 — determinism: same seed → identical demographic trajectory.
```

---

## 5. Turn-Loop Phase Placement (Temporal Causality)

| Step | Mechanism | Phase (existing hook) |
|---|---|---|
| Grid reconcile (missing LV/MV keys) | §1.3 | turn_context assembly — *before* `distribute_grid_power` (`turn.rs:2583`) |
| LV/MV growth toward demand | §1.3 | same call site, after demand collection, before clearing |
| Blackout penalties → fulfillment | existing | unchanged (`b2b_orders` settlement wave) |
| Method-switch evaluation (M2) | §2.2 | inside `process_company`, before strategy eval — so `evaluate_furlough` sees post-adaptation `avg_fulfillment_ratio` |
| Per-building shortage furlough (M1), shift floor (M3) | §2.2 | `evaluate_furlough` — replaces the scalar shortcut |
| LDR gate (R1) | §3.2 | inside `issue_loan` — all origination paths converge there |
| Interbank reserve settlement (R3) | §3.2 | wherever deposit transfers settle (banking clear pass) |
| Rural→urban transitions (D2) | §4.2 | existing `class_transitions` phase — verified to credit destination pools |

No mechanism reads a value produced later in the same turn; no buff is
applied to a phase that already ran.

---

## 6. New Epic Test Files

| File | `state/Cargo.toml` `[[test]]` | Features |
|---|---|---|
| `epics/grid_distribution_viability_test.rs` | required | `epic-tests`, `diagnostic` |
| `epics/banking_regulatory_viability_test.rs` | required | `epic-tests`, `diagnostic` |
| `epics/demographic_dynamics_test.rs` | required | `epic-tests`, `diagnostic` |
| `epics/adaptive_production_test.rs` | required | `epic-tests`, `diagnostic` |

Plus extensions inside `market_gridlock_diagnostic_test.rs`: `unemployment < 15%`
(hard assert, was implicitly 20%), grid-distribution reconciliation,
LDR-bound iteration. Keep fast unit tests in `state/tests/` per the v4
feature-flag convention; epic tests gated via `[[test]]` blocks only.

---

## 7. Agent Delegation Strategy

Respecting current `agents_sync.json` locks (agent-1: `economy/trade/`,
`military/supply/`; agent-2: `construction/`, `real_estate_market.rs`;
agent-3: care/social-programs; agent-4: education domains):

| Workstream | Suggested agent | Domain | Lock conflicts |
|---|---|---|---|
| **W1: Grid black hole** — probes, key reconcile, `unwrap_or` hardening, LV/MV growth, grid viability test | agent-5 (manager) or new worker | `energy/`, `engine/turn_context.rs` | none — energy/ is unowned |
| **W2: Adaptive production** — per-building shortage, substitution registry, shift floor | new worker (`fix/agent-6-adaptive-production`) | `corporate/strategy.rs`, `corporate/manager.rs`, `registries/production_methods.rs` | `manager.rs` is agent-2 shared-file → post a blocker, coordinate hunks, land after agent-2's 1-line turn.rs fix |
| **W3: Banking LDR** — origination audit, LDR gate, settlement completeness, ELA tests | agent-1 extension or new worker | `state/banking.rs`, `corporate/manager.rs` (loan call site) | borders agent-1's M0-leak work in `economy/trade/` — assign as agent-1 follow-on to keep accounting coherent |
| **W4: Demographics** — dev-tier structure, transition verification | new worker (`feat/agent-7-dynamic-demographics`) | `society/geography.rs`, `economy/labor/class_transitions.rs` | none — both unowned |
| **W5: Test harness** — new epic files + gridlock extensions | whichever agent lands first in each pillar's domain; manager merges | `state/tests/epics/`, `state/Cargo.toml` | `Cargo.toml` shared — additive `[[test]]` blocks only |

**Sequencing:** W1 first — the grid bug is the upstream trigger of the
furlough cascade, and its probes must land before W2/W3 measurements are
meaningful. W2 and W4 are parallel-safe. W3 is parallel-safe in code but its
tests should run after W1 so bank stress isn't conflated with blackout-driven
insolvency. Manager (agent-5) runs the Iron CI/CD + macro audit before each
merge; workers push `fix/*`/`feat/*` branches only (RBAC).

---

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | Method substitution consumes real substitute inputs from inventory/market (§2.4); grid reconcile adds *accounting* keys, no energy created — supply still clamps to nameplate (`grid.rs:384-388`); demographic redistribution conserves total population (§4.3 D2). |
| Double-Entry Bookkeeping | PASS | LDR gate keeps loan↔deposit paired creation (§3.2 R1); ELA legs on both balance sheets (C5); deleveraging posts losses through equity, no write-off void (R4); every rejected loan leaves zero ledger trace (C3). |
| No Teleportation | PASS | Substituted inputs still flow through B2B with existing freight logic; rural→urban migration is a class/population move with wage-premium trigger and persistence hysteresis — no instant relocation of physical goods (§4.2 D2). |
| Clamping | PASS | `ldr_cap`, fulfillment ratios, class shares ∈ [0,1], rural share ≥ food floor, LV/MV caps bounded by demand×headroom, furlough counts clamped to `fulfilled_fte` (existing `min`/`max` kept). |
| No Magic Numbers | PASS | LDR cap derived from `reserve_requirement_ratio`; LV floor = existing `LV_HEADROOM_FACTOR` & 0.1 MW pre-electrification floor (documented); `MIN_OPERATING_SHIFT` derived from seat-type crew minima; substitution price ceiling from `reference_price`; demographic elasticities require documented derivation (§4.2). |
| Technological Matrices | PASS | Substitution lives in the production-method registry as data (substitution groups), methods remain Production/Automation/Organization slotted; no ad-hoc per-building recipes (§2.2 M2). |
| Architectural Parsimony | PASS | Every mechanism extends existing systems: `distribute_grid_power`, `issue_loan` (single choke point), `evaluate_furlough`, existing method-switch registry, existing `class_transitions`. No parallel economy introduced. |
| Temporal Causality | PASS | §5 table places each computation before its consumer; grid reconcile precedes first `distribute_grid_power`; method switching precedes furlough evaluation; no post-hoc buffs. |
| Asymmetric Information | PASS | All additions are backend mechanics + diagnostic telemetry; no hidden-state leakage to snapshot DTOs is introduced; new snapshot fields (effective supply, LDR) are public macro aggregates. |
| Full-Stack Accountability | PASS | New telemetry: `region_effective_supply_mw`, per-bank LDR, method-switch counters, migration flows — each listed in §*.3 for snapshot/snapshot-DTO exposure at implementation time. |
| Complete Entity Lifecycle | PASS | No new entity types; extends lifecycle of existing companies (adaptation precedes furlough precedes liquidation — existing lifecycle untouched), banks (deleverage→ELA→existing insolvency), population classes (birth/migration/death transitions). |
| Market Forces | PASS | Method selection maximizes margin at market prices; loans allocated by excess-reserve ordering (existing Phase 77 mechanism); deleveraging pro-rata; no hardcoded splits anywhere in the plan. |
| Rational Actors | PASS | Companies adapt (substitute/downscale) before shedding labor — profit-maximizing; banks bound by regulation, not charity; migration requires real wage premia; no forgiveness outside explicit ELA/state mechanics. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: none blocking implementation. Watch-items: (i) W2 touches
  `manager.rs`, an agent-2 shared file — coordinate via blocker bus before
  editing; (ii) `state/Cargo.toml` gets additive `[[test]]` blocks only, to
  avoid manifest merge churn; (iii) all new probes must be
  `#[cfg(feature = "diagnostic")]`-gated to keep release builds clean.
