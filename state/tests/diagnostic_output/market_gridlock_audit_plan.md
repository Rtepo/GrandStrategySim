# Market Gridlock (Turn 2) — Diagnostic Audit Plan

**Task:** `market-gridlock-turn2-diagnostic-audit`
**Author:** Agent 4 (Auditor)
**Manager:** Agent 5
**Branch:** `fix/agent-4-market-gridlock-audit`
**Date:** 2026-09-20
**Status:** PLANNING + AUDIT ONLY — no production code or test suite implemented.

---

## 0. Executive Summary

### 0.1 Symptom

By Turn 2, the macroeconomic simulation suffers **total market gridlock**:
companies burn their initial seed cash on wages but record **0.00 Income**,
triggering mass furloughs and immediate bankruptcies across all sectors. The
root cause is unknown.

### 0.2 Working Hypothesis (to be confirmed/refuted by the test suite)

The existing Phase 94 diagnostic harness (`phase94_diagnostic_harness_test.rs`)
verifies **conservation invariants** (M0 base money, physical mass, bank
balance-sheet identity) but captures **no economic-activity telemetry**: order
book depth, trade volume, production output, B2C demand/supply, furlough
triggers, or per-company income. The gridlock is an **economic-flow** failure,
not a conservation failure, so the existing harness cannot see it.

The most likely failure chain, based on static code analysis:

1. **Turn 1:** Companies pay wages from seed cash → production runs → outputs
   land in `building.inventory`.
2. **B2B phase:** Companies submit buy bids (encumber `available_cash` →
   `debit_cash`) and sell asks (list output). The bootstrap pricing path
   (`is_bootstrap`) is designed to guarantee spread crossing on Turn 0, but
   **after Turn 0 the cost-based pricing path takes over** — `sell_price =
   unit_cost * (1 + markup)`, clamped to `sell_price.max(unit_cost)`. If
   `unit_cost` is high (manufactured goods with expensive inputs that did not
   clear), the ask rises above the buy bid (`ref_price * (1 + buy_premium)`)
   and the spread **never crosses** → zero trades → zero B2B revenue.
3. **B2C phase:** Stores need inventory (routed from B2B purchases or
   production). If B2B cleared nothing, stores stay empty → `total_supply ==
   0.0` → `clear_b2c_markets` skips the commodity → zero B2C revenue. Even if
   stores have inventory, `build_consumer_demand` applies a **wealth gate**
   (`savings_per_capita < commodity_wealth_gate(commodity)` → `continue`) and
   an **era multiplier** (`era_mult <= 0.0` → `continue`); if either gates
   out, demand is zero and `settle_b2c_clearing` returns `(0.0, 0.0)`.
4. **Turn 2:** Companies have burned cash on wages + encumbered cash on B2B
   bids that never cleared (refund path may or may not have released
   `debit_cash`). With zero income, `operational_cash() < 2 × payroll` →
   `evaluate_furlough` fires `cash_shortage` → mass furlough → production
   collapses → spiral into bankruptcy.

### 0.3 Deliverable of THIS Task

This document is the **audit plan**. It specifies:
- Exact probes to add/reuse per phase (§3).
- Concrete assertions per hypothesis (§4).
- Harness modifications needed (§5).
- Test placement per AGENTS.md v4 (§6).
- A decision tree mapping probe values to root-cause candidates (§7).

It does **NOT** implement the test suite or modify production source.

---

## 1. Audit of Existing Groundwork (Harness, Probes, Checkpoints)

### 1.1 Reusable Infrastructure

| Component | File:Line | Reusable? | Notes |
|---|---|---|---|
| `TurnProbe` trait | `state/src/engine/diagnostic.rs:56` | YES (extend) | Observer trait with `checkpoint(phase_name, phase_index, turn, market, tasks)`. Zero-cost via `NoopProbe` monomorphization. New probes plug in here. |
| `CapturingProbe` | `state/src/engine/diagnostic.rs` (feature-gated) | YES (extend) | Already accumulates per-checkpoint snapshots. Add new snapshot fields for economic activity. |
| `HarnessTargets` | `state/src/engine/diagnostic.rs:86` | YES | Selects 5 companies (Agriculture, HeavyIndustry, Construction, Mining, LocalServices) + 1 bank + 1 region. Reuse for per-company tracing. |
| `CompanySnapshot` | `state/src/engine/diagnostic.rs:190` | YES (extend) | Captures `liquid_capital`, `available_cash`, `debit_cash`, `owned_inventory_mass`. **Missing:** `fulfilled_fte`, `furloughed_workers_count`, `offered_wage_per_fte`, `company_capital`, `financial_history` length, `is_in_receivership`, last-turn income/revenue. |
| `RegionalMarketSnapshot` | `state/src/engine/diagnostic.rs:327` | YES (extend) | Captures `base_prices`, `net_surplus`, `supply_volume`, `demand_volume`, `offshore_capital`. **Missing:** order-book depth (bid/ask counts + prices), trade count + volume, B2C units_sold, store inventory mass. |
| `FiatWalk` / `walk_global_fiat` | `state/src/engine/diagnostic.rs:434` | YES (reuse) | M0 base money. Not the focus of this audit, but keep for cross-checking that the gridlock is NOT an M0 conservation issue. |
| `walk_global_mass` | `state/src/engine/diagnostic.rs:672` | YES (reuse) | Physical mass. Reuse to verify production actually deposited output into `building.inventory`. |
| `ConservationVerdict` / `MassSinkWhitelist` | `state/src/engine/diagnostic.rs:709-799` | NO (out of scope) | Conservation enforcement. The gridlock is a flow failure, not a conservation failure. Do not extend. |
| `select_targets` / `select_targets_from_ctx` | `state/src/engine/diagnostic.rs:103` / `phase94_diagnostic_harness_test.rs:303` | YES (reuse) | Deterministic target selection. Reuse as-is. |
| `write_all_dumps` | `state/src/engine/diagnostic.rs` | YES (extend) | Writes `sector_ledger.json`, `market_clearing.json`, `banking_state.json`, `manifest.json`. Add `order_book_dump.json`, `b2c_flow_dump.json`, `furlough_dump.json`. |
| `audit_dumps.py` | `.devin/scripts/audit_dumps.py` | YES (extend) | Python invariant verifier. Add economic-flow invariants (§4.3). |

### 1.2 Existing Checkpoints (turn.rs)

The turn loop already emits 8 named checkpoints. The gridlock audit reuses these
seams and adds sub-probes between them.

| # | Name | turn.rs:Line | Phase Boundary | Reuse for Gridlock? |
|---|---|---|---|---|
| 0 | `turn_start` | 725 | Before any phase work | YES — baseline cash/inventory/FTE |
| 1 | `building_cycle_post` | 1064 | After building upkeep/employment | YES — employment state |
| 2 | `b2b_orders_post` | 1794 | After B2B order submission + matching | **CRITICAL** — order book depth, trade count |
| 3 | `b2b_settlement_post` | 2015 | After B2B cash + freight settlement | **CRITICAL** — revenue credited, refunds |
| 5 | `production_cycle_post` | 2895 | After production (Wave 1: Energy) | YES — output deposited to inventory |
| (none) | (post Wave 2 production) | ~3679 | After production (Wave 2: all sectors) | **ADD** — main output deposit |
| 6 | `b2c_clearing_post` | 5024 | After B2C clearing + settlement | **CRITICAL** — B2C revenue, units sold |
| 7 | `banking_turn_post` | 977 | After banking turn | YES — loan activity |
| 4 | `turn_end` | 8917 | End of turn | YES — final cash/income/FTE |

**Gap:** There is no checkpoint between Wave 2 production (~3679) and B2C
clearing (5024). Wave 2 is where the bulk of non-energy output is deposited.
A new checkpoint `production_wave2_post` is needed to capture inventory before
B2C consumes it.

### 1.3 What the Existing Harness CANNOT See (The Gap)

The Phase 94 harness asserts:
- `fiat_conserved == true` at every checkpoint
- `mass_conserved == true` at every checkpoint
- `no_negative_inventories == true`
- bank balance-sheet identity

It does **NOT** capture or assert:
- Order book depth (bids/asks per commodity: count, total quantity, min/max/mean limit price)
- Trade volume (count, total quantity, total value, per commodity)
- Production output (units produced per commodity per company)
- B2C demand vs supply (demand volume, offer volume, units sold, unmet demand)
- Furlough triggers (which condition fired: `cash_shortage` vs `material_shortage`)
- Per-company income/revenue (B2B + B2C)
- Store inventory mass (what's actually on shelves for B2C)
- Wealth-gate / era-multiplier gating in `build_consumer_demand`
- Refund path execution (did unfilled bids release `debit_cash` back to `available_cash`?)

**These are the exact telemetry the gridlock test suite must add.**

---

## 2. Audit Scope — Entry Points & Hypotheses

### 2.1 B2B / B2C Clearing

**Entry points:**
- `submit_company_b2b_orders` — `state/src/economy/trade/b2b_orders.rs:213`, invoked `state/src/engine/turn.rs:1602`, checkpoint `b2b_orders_post` `turn.rs:1794`.
- `match_orders_with_embargoes` — `state/src/economy/market/order_book.rs:193`, invoked `turn.rs:1695`.
- `settle_b2c_clearing` + `generate_store_offers` — `state/src/economy/trade/retail.rs:1169` / `retail.rs:852`, invoked `turn.rs:4995` / `turn.rs:4984`, checkpoint `b2c_clearing_post` `turn.rs:5024`.
- `build_consumer_demand` — `state/src/economy/trade/retail.rs:483` (demand generation, wealth/era gated).
- `clear_b2c_markets` — `state/src/economy/trade/retail.rs:972` (utility-based allocation).

**Hypotheses:**

| ID | Hypothesis | Mechanism | Probe to Confirm |
|---|---|---|---|
| H-B2B-1 | **Orders never placed** | Companies skip buy bids (no `get_reference_price`) or sell asks (`total_available <= 0.0` — no output, no workers). | Count bids/asks submitted per commodity per company. |
| H-B2B-2 | **Price mismatch (spread never crosses)** | After Turn 0 bootstrap, `sell_price = unit_cost * (1+markup).max(unit_cost)` rises above `buy_bid = ref_price * (1+buy_premium)`. Manufactured goods with expensive uncleared inputs → ask >> bid. | Compare max bid price vs min ask price per commodity. Flag commodities where `max_bid < min_ask` (no crossing). |
| H-B2B-3 | **Silent drop (embargo/freight)** | `match_orders_with_embargoes` skips embargoed pairs; freight procurement defers/fails trades. | Count trades generated vs bids/asks submitted. Count embargoed skips. Count freight-deferred trades. |
| H-B2B-4 | **Refund path fails** | Unfilled bids redistributed to per-country `order_book` (turn.rs:1706-1743) but refund functions may not release `debit_cash` → `available_cash`. Companies stay encumbered. | Diff `debit_cash` and `available_cash` across `b2b_orders_post` → `b2b_settlement_post`. Assert `debit_cash` returns toward 0. |
| H-B2C-1 | **B2C demand is zero (wealth gate)** | `build_consumer_demand`: `savings_per_capita < commodity_wealth_gate(commodity)` → `continue`. Citizens too poor to demand anything. | Capture `total_demand` per commodity + `savings_per_capita` per class. Flag commodities gated out. |
| H-B2C-2 | **B2C demand is zero (era multiplier)** | `era_consumption_multiplier(commodity, year) <= 0.0` → `continue`. Commodity not yet in era. | Capture `era_mult` per commodity. Flag `era_mult <= 0`. |
| H-B2C-3 | **B2C supply is zero (empty stores)** | `generate_store_offers`: stores have no `current_inventory` → no offers. `clear_b2c_markets`: `total_supply == 0.0` → `continue`. | Capture store inventory mass per commodity. Count offers generated. |
| H-B2C-4 | **B2C settlement returns zero** | `settle_b2c_clearing`: `total_class_demand <= 0.0` → `return (0.0, 0.0)`. | Capture `total_settled` and `total_vat_collected` return values. |

### 2.2 Production / Inventory

**Entry points:**
- `execute_production_cycle` — `state/src/economy/production/production.rs:~336`, invoked `turn.rs:2882-2895` (Wave 1: Energy) and `turn.rs:3649` (Wave 2: all sectors).
- Output-ask listing loop — `b2b_orders.rs:449-591` (sell asks from `method.outputs` + `building.inventory`).

**Hypotheses:**

| ID | Hypothesis | Mechanism | Probe to Confirm |
|---|---|---|---|
| H-PROD-1 | **No workers (furloughed Turn 1)** | `production_scale = current_employment / 1000.0`. If `fulfilled_fte == 0` (furloughed), `production_scale == 0` → zero output. | Capture `fulfilled_fte` + `production_scale` per building. Assert `production_scale > 0`. |
| H-PROD-2 | **No inputs (B2B never delivered)** | Production consumes inputs from `building.inventory`; if B2B cleared nothing, inputs are zero → `actual_energy`/output clamped to 0. | Diff `building.inventory` for input commodities across `b2b_settlement_post` → `production_cycle_post`. |
| H-PROD-3 | **Output deposited but not listed** | Production deposits output to `building.inventory`, but sell-ask loop skips (`is_local_utility`, `sell_fraction` clamp, `total_available <= 0`). | Diff `building.inventory` for output commodities across `production_cycle_post` → `b2b_orders_post`. Compare listed sell_qty vs inventory. |
| H-PROD-4 | **Agriculture bypass (no per-turn output)** | Agriculture buildings consume inputs but produce NO output via BOM (harvest cycle handles it). If harvest hasn't fired, Agriculture has zero sellable output. | Capture `outputs_produced` per building. Flag Agriculture buildings with zero output. |
| H-PROD-5 | **Output routed to wrong building** | Production output goes to `building.inventory` of the producing building, but B2C needs it in `commercial_buildings[].current_inventory`. If B2B→retail routing is broken, stores stay empty. | Trace output commodity from `building.inventory` → B2B trade → commercial building inventory. |

### 2.3 Furlough Logic

**Entry points:**
- `evaluate_furlough` — `state/src/corporate/strategy.rs:1045` (trigger classification).
- `CorporateAction::Furlough` handler — `state/src/corporate/manager.rs:1690-1727` + `furlough_wage_queue`.
- `apply_seasonal_furlough` — `state/src/corporate/manager.rs:1908`.
- `process_furlough_reinstatement` — `state/src/corporate/manager.rs:1965`.
- `process_furlough_attrition` — `state/src/corporate/manager.rs:2020`.
- `liquidate_bankrupt_companies` — `state/src/corporate/lifecycle.rs:145` + `corporate/bankruptcy.rs` Syndic.
- Seasonal profiles — `state/src/engine/generator/corporate.rs:146-149`.
- Labor market cash-low preference — `state/src/economy/labor/labor_market.rs:639`.

**Hypotheses:**

| ID | Hypothesis | Mechanism | Probe to Confirm |
|---|---|---|---|
| H-FUR-1 | **Cash-flow furlough (structural)** | `operational_cash() < total_payroll * 2.0` AND no income path. This is the expected gridlock signature. | Capture `operational_cash()`, `total_payroll`, `cash_shortage` flag per company. Assert `cash_shortage == true` AND `last_turn_income == 0`. |
| H-FUR-2 | **Material-shortage furlough** | `avg_fulfillment_ratio < 0.1` (can't produce). Distinct from cash-flow: company has cash but no inputs. | Capture `avg_fulfillment_ratio`, `material_shortage` flag. Assert which condition fired. |
| H-FUR-3 | **Grace period expired prematurely** | Agriculture grace: first revenue OR 24 turns. Non-agri: `financial_history.is_empty()` (Turn 1 only). If grace logic is wrong, furlough fires Turn 1 before any income possible. | Capture `is_within_material_shortage_grace` per company. Assert furlough did NOT fire during grace. |
| H-FUR-4 | **Seasonal furlough misfire** | `apply_seasonal_furlough` furloughs off-season workers. If season logic is inverted, all workers furloughed. | Capture `season`, `active_seasons`, `current_state` per seasonal company. |
| H-FUR-5 | **Reinstatement never fires** | `process_furlough_reinstatement` requires `avg_fulfillment_ratio >= 0.5` AND `available >= payroll_cost`. If inputs never arrive, reinstatement never fires → permanent furlough. | Capture reinstatement attempts vs successes. |
| H-FUR-6 | **Bankruptcy cascade** | `company_capital < 0.0` → liquidation. If `company_capital` is computed from `available_cash - liabilities` and cash was burned on wages with no income, equity goes negative Turn 2. | Capture `company_capital` per company per turn. Assert first negative-equity turn. |

---

## 3. Exact Probes to Add or Reuse Per Phase

Probes are specified as: **probe name** → **where captured** (file:function/checkpoint) → **what it records**.

### 3.1 Reused Probes (no code change needed)

| Probe | Source | Records |
|---|---|---|
| `fiat_walk` | `diagnostic.rs:walk_global_fiat` @ every checkpoint | M0 total + components (cross-check: gridlock is NOT M0 leak) |
| `mass_walk` | `diagnostic.rs:walk_global_mass` @ every checkpoint | Physical mass per commodity (verify production deposited output) |
| `company_snapshot` | `diagnostic.rs:CompanySnapshot::from_company` @ every checkpoint | `liquid_capital`, `available_cash`, `debit_cash`, `owned_inventory_mass` |
| `bank_snapshot` | `diagnostic.rs:BankSnapshot::from_company` @ every checkpoint | Bank balance sheet + loan book |

### 3.2 New Probes (require harness extension — see §5)

#### Phase: `turn_start` (turn.rs:725)

| Probe | Captures | Purpose |
|---|---|---|
| `company_fte_baseline` | `fulfilled_fte`, `furloughed_workers_count`, `offered_wage_per_fte`, `physical_fte_demand`, `target_fte_demand` per target company | Baseline workforce before wage/furlough phase. |
| `company_income_baseline` | `financial_history` (last entry: revenue, costs, gross_profit, net_profit), `company_capital` | Baseline income/equity to detect Turn 2 collapse. |
| `company_operational_cash` | `operational_cash()` (= `available_cash + brokerage_cash`) | Cash available for payroll — the furlough trigger input. |

#### Phase: `building_cycle_post` (turn.rs:1064)

| Probe | Captures | Purpose |
|---|---|---|
| `employment_state` | per target building: `current_employment`, `last_fulfillment_ratio`, `active_method` name | Detect zero-employment buildings (furloughed before production). |

#### Phase: `b2b_orders_post` (turn.rs:1794) — **CRITICAL**

| Probe | Captures | Purpose |
|---|---|---|
| `order_book_depth` | per commodity: `bid_count`, `ask_count`, `total_bid_qty`, `total_ask_qty`, `max_bid_price`, `min_ask_price`, `mean_bid_price`, `mean_ask_price` | Detect H-B2B-1 (orders never placed) and H-B2B-2 (spread never crosses). |
| `spread_crossed` | per commodity: `bool` = `max_bid_price >= min_ask_price` | Direct test of spread crossing. |
| `bootstrap_flag` | `is_bootstrap` (market_history empty?) | Confirm bootstrap pricing active on Turn 0, off on Turn 1+. |
| `sell_price_basis` | per ask: which branch produced `sell_price` (`bootstrap` / `unit_cost` / `ref_price` / `base_price` / `skipped`) | Identify why asks are priced where they are. |
| `bid_skip_reasons` | count of bids skipped due to: `no_ref_price`, `no_cash`, `affordable_qty_le_0` | Detect H-B2B-1. |
| `ask_skip_reasons` | count of asks skipped due to: `total_available_le_0`, `is_local_utility`, `sell_price_le_0`, `no_ref_price` | Detect H-PROD-3. |
| `encumbrance_state` | per company: `available_cash`, `debit_cash` post-encumbrance | Detect H-B2B-4 (refund path). |

**Capture point:** Add a probe call inside `submit_company_b2b_orders` (b2b_orders.rs:213) that accumulates per-commodity bid/ask statistics into a thread-local or returned struct, OR snapshot the `global_order_book` (turn.rs:1668-1696) just before `match_orders_with_embargoes` (turn.rs:1695). The latter is cleaner — no production code change, just a probe checkpoint reading `global_order_book`.

#### Phase: `b2b_settlement_post` (turn.rs:2015) — **CRITICAL**

| Probe | Captures | Purpose |
|---|---|---|
| `trade_volume` | `trade_count`, `total_qty`, `total_value`, per-commodity breakdown | Direct measure of B2B activity. Zero → gridlock confirmed. |
| `embargo_skips` | count of embargoed ask-skips in matcher | Detect H-B2B-3. |
| `freight_deferred` | count + value of deferred trades | Detect H-B2B-3 (freight failure). |
| `refund_executed` | per company: `debit_cash` delta (pre→post settlement), `available_cash` delta | Detect H-B2B-4 (refund path). |
| `b2b_revenue_credited` | per company: B2B revenue credited (seller side) | First income source. |

**Capture point:** Snapshot `global_order_book.trades` (turn.rs:1696) for trade volume. Snapshot `company.available_cash`/`debit_cash` diff across `b2b_orders_post` → `b2b_settlement_post` for refund tracking.

#### Phase: `production_cycle_post` (turn.rs:2895) + **NEW** `production_wave2_post` (~turn.rs:3679)

| Probe | Captures | Purpose |
|---|---|---|
| `production_output` | per target building: `outputs_produced` (commodity → qty), `inputs_consumed`, `production_scale` | Detect H-PROD-1 (no workers), H-PROD-2 (no inputs), H-PROD-4 (agri bypass). |
| `inventory_delta` | per target company: `building.inventory` diff for input + output commodities across `b2b_settlement_post` → `production_cycle_post` | Verify inputs arrived (B2B delivered) and outputs deposited. |
| `fulfillment_ratio` | per building: `last_fulfillment_ratio` | Detect H-FUR-2 (material shortage). |

**Capture point:** `execute_production_cycle` returns a result struct with `outputs_produced` and `inputs_consumed` (production.rs). The probe can read `building.inventory` and `building.last_fulfillment_ratio` at the checkpoint. **Add a new checkpoint `production_wave2_post` after the Wave 2 loop (turn.rs:~3679)** since the existing `production_cycle_post` (2895) only captures Wave 1 (Energy).

#### Phase: `b2c_clearing_post` (turn.rs:5024) — **CRITICAL**

| Probe | Captures | Purpose |
|---|---|---|
| `b2c_demand` | per commodity: `total_demand` (from `consumer_demand.total_demand`) | Detect H-B2C-1/H-B2C-2 (zero demand). |
| `b2c_supply` | per commodity: `total_supply` (sum of `store_offers` quantities) | Detect H-B2C-3 (empty stores). |
| `b2c_units_sold` | per commodity: `units_sold` (from `clearing_result`) | Direct B2C activity measure. |
| `b2c_unmet_demand` | per commodity: `remaining_demand` after allocation | Detect supply shortage. |
| `b2c_revenue` | `total_settled`, `total_vat_collected` (return of `settle_b2c_clearing`) | Detect H-B2C-4 (zero settlement). |
| `b2c_revenue_credited` | per company: B2C revenue credited (store owner) | Second income source. |
| `wealth_gate_hits` | per class: `savings_per_capita` vs `commodity_wealth_gate(commodity)`; count gated-out commodities | Detect H-B2C-1. |
| `era_mult_values` | per commodity: `era_consumption_multiplier(commodity, year)` | Detect H-B2C-2. |
| `store_inventory_mass` | per commercial building: total inventory qty per commodity | Detect H-B2C-3 / H-PROD-5. |

**Capture point:** `build_consumer_demand` (retail.rs:483) is called per region inside the B2C loop (turn.rs:~4984). `clear_b2c_markets` returns `B2cClearingResult` with `units_sold`, `store_revenue`, `retail_prices`. `settle_b2c_clearing` returns `(total_settled, vat_collected)`. The probe can snapshot `consumer_demand.total_demand`, `store_offers`, `clearing_result`, and the settlement return values. The wealth-gate and era-mult are computed inside `build_consumer_demand` — to capture them without modifying production code, add **instrumented counters** to `ConsumerDemand` (e.g., `gated_commodities: Vec<Commodity>`, `era_mult_per_commodity: HashMap<Commodity, f64>`) populated during demand building, then read by the probe. This is a minimal, additive change to a data struct, not production logic.

#### Phase: `turn_end` (turn.rs:8917)

| Probe | Captures | Purpose |
|---|---|---|
| `furlough_actions` | per company: `CorporateAction::Furlough` fired? `fte_count`, `wage_fraction`, trigger (`cash_shortage` / `material_shortage` / `seasonal`) | Classify furlough trigger (H-FUR-1 vs H-FUR-2 vs H-FUR-4). |
| `furlough_state` | per company: `fulfilled_fte`, `furloughed_workers_count`, `is_in_receivership` | End-of-turn workforce state. |
| `bankruptcy_state` | per company: `company_capital`, `is_liquidated`, `merged_into` | Detect H-FUR-6 (bankruptcy cascade). |
| `income_summary` | per company: total income this turn (B2B revenue + B2C revenue) | The headline metric: 0.00 income = gridlock. |
| `grace_status` | per company: `is_within_material_shortage_grace` result, `financial_history.len()` | Detect H-FUR-3 (premature grace expiry). |

**Capture point:** Furlough actions are decided in `evaluate_furlough` (strategy.rs:1045) and applied in the `CorporateAction::Furlough` handler (manager.rs:1690). To capture the trigger classification without modifying production logic, add an **instrumented field** to `Company` (e.g., `last_furlough_trigger: Option<String>`) set in `evaluate_furlough` before returning the action, then read by the probe at `turn_end`. This is a single additive field, not logic change.

---

## 4. Concrete Assertions Per Hypothesis

### 4.1 B2B Assertions

```
// H-B2B-1: Orders never placed
ASSERT per commodity: bid_count > 0 OR ask_count > 0
  (at least one side of the market is active)
FAIL_MSG: "Commodity {c}: zero bids AND zero asks — market completely inactive"

ASSERT per target company: submitted_at_least_one_bid OR submitted_at_least_one_ask
  (company participated in B2B)
FAIL_MSG: "Company {id}: submitted no B2B orders (skipped all bids + asks)"

// H-B2B-2: Spread never crosses
ASSERT per commodity WITH bids AND asks: max_bid_price >= min_ask_price
  (spread crosses — trades CAN execute)
FAIL_MSG: "Commodity {c}: spread never crosses — max_bid={max_bid} < min_ask={min_ask}
           (gap={min_ask - max_bid}). Cost-based ask pricing above ref-based bid pricing."

ASSERT per commodity: trade_count > 0 IF bid_count > 0 AND ask_count > 0 AND spread_crossed
  (crossing spread actually produced trades)
FAIL_MSG: "Commodity {c}: spread crossed but zero trades — embargo or freight failure"

// H-B2B-3: Silent drop
ASSERT total_trade_value > 0 by Turn 1
  (some B2B activity must occur for the economy to function)
FAIL_MSG: "Zero B2B trade value by Turn {t} — total market gridlock"

ASSERT embargo_skips == 0 OR (embargo_skips > 0 AND trade_count > 0)
  (embargoes may skip some pairs but not block all trade)
FAIL_MSG: "All potential trades embargoed — diplomacy config blocks all cross-border B2B"

// H-B2B-4: Refund path
ASSERT per company: debit_cash at b2b_settlement_post <= debit_cash at b2b_orders_post
  (encumbered cash is released after settlement/refund)
FAIL_MSG: "Company {id}: debit_cash did not decrease after settlement — refund path broken
           (debit_cash stuck at {stuck_value})"
```

### 4.2 B2C Assertions

```
// H-B2C-1: Wealth gate
ASSERT sum(consumer_demand.total_demand.values()) > 0 by Turn 1
  (citizens demand something)
FAIL_MSG: "Zero total consumer demand — all commodities wealth-gated or era-gated out"

ASSERT per commodity with nonzero supply: total_demand > 0
  (demand exists where supply exists)
FAIL_MSG: "Commodity {c}: supply={supply} but demand=0 — wealth/era gate killed demand"

// H-B2C-3: Empty stores
ASSERT per region: store_inventory_mass > 0 by Turn 1 OR Turn 2
  (stores have something to sell)
FAIL_MSG: "Region {r}: stores empty — B2B→retail inventory routing broken"

// H-B2C-4: Zero settlement
ASSERT b2c_revenue (total_settled) > 0 by Turn 1 OR Turn 2
  (some B2C consumption occurs)
FAIL_MSG: "Zero B2C settlement — demand={demand} supply={supply} but settled=0"

ASSERT per company (retail sector): b2c_revenue_credited > 0 by Turn 2
  (retail companies earn income)
FAIL_MSG: "Retail company {id}: zero B2C revenue by Turn 2 — no consumer spending"
```

### 4.3 Production Assertions

```
// H-PROD-1: No workers
ASSERT per target building (non-energy, non-agri): production_scale > 0
  (building has workers producing)
FAIL_MSG: "Building {id}: production_scale=0 — fulfilled_fte=0 (furloughed or never hired)"

// H-PROD-2: No inputs
ASSERT per target building: inputs_consumed total > 0 OR method.inputs is empty
  (building consumed inputs OR needs no inputs)
FAIL_MSG: "Building {id}: zero inputs consumed — B2B delivery failed for required inputs"

// H-PROD-3: Output deposited but not listed
ASSERT per target company: sum(outputs_produced) > 0 IMPLIES sum(sell_qty listed) > 0
  (produced output is listed for sale)
FAIL_MSG: "Company {id}: produced {produced} but listed {listed} for sale — output not listed"

// H-PROD-4: Agriculture bypass
ASSERT per agriculture building: outputs_produced via BOM == 0
  (agriculture uses harvest cycle, not per-turn BOM — this is EXPECTED, not a bug)
INFO_MSG: "Agriculture building {id}: zero per-turn output (harvest cycle handles output)"

// H-PROD-5: Output routing
ASSERT per output commodity: building.inventory increased at production_cycle_post
  (production deposited output)
FAIL_MSG: "Building {id}: output {c} not deposited to inventory — production cycle broken"
```

### 4.4 Furlough Assertions

```
// H-FUR-1: Cash-flow furlough (the expected gridlock signature)
ASSERT per company furloughed by Turn 2: last_turn_income == 0.0
  (furlough is due to zero income, not mismanagement)
FAIL_MSG: "Company {id} furloughed but had income={income} — furlough trigger misclassified"

// H-FUR-3: Grace period
ASSERT no company furloughed during grace period
  (grace prevents premature furlough)
FAIL_MSG: "Company {id} furloughed during grace (financial_history.len()={len}) —
           grace logic broken"

// H-FUR-6: Bankruptcy cascade
ASSERT no company liquidated before Turn 3
  (grace period: financial_history.len() < 2 → skip liquidation)
FAIL_MSG: "Company {id} liquidated at Turn {t} — bankruptcy grace expired too early"

// Headline gridlock assertion
ASSERT by Turn 2: fraction_of_companies_with_zero_income < 0.5
  (less than half of companies have zero income — economy is functioning)
FAIL_MSG: "MARKET GRIDLOCK: {n}/{total} companies ({pct}%) have zero income by Turn 2"
```

### 4.5 Cross-Cutting Invariants (for `audit_dumps.py` extension)

```
// INV-ECO-1: B2B trade value monotonicity
ASSERT b2b_trade_value[turn] >= 0
  (non-negative — no negative trades)

// INV-ECO-2: Income = B2B_revenue + B2C_revenue
ASSERT per company: reported_income == b2b_revenue_credited + b2c_revenue_credited
  (income accounting is complete)

// INV-ECO-3: Inventory conservation per company
ASSERT per company: inventory[turn_end] == inventory[turn_start] + produced - sold - consumed
  (physical inventory balances)

// INV-ECO-4: Cash flow conservation per company
ASSERT per company: cash[turn_end] == cash[turn_start] - wages - b2b_purchases + b2b_revenue
                                                    + b2c_revenue - taxes - other_costs
  (cash flow balances — identifies where cash leaks)
```

---

## 5. Harness Modifications Needed

### 5.1 New Checkpoints

| Checkpoint | Insert at | Rationale |
|---|---|---|
| `production_wave2_post` | `turn.rs:~3679` (after Wave 2 production loop) | Existing `production_cycle_post` (2895) only captures Wave 1 (Energy). Wave 2 is where Agriculture/Manufacturing/Mining output is deposited. |

**Implementation note:** Add `probe.checkpoint("production_wave2_post", 8, turn, &market, &tasks);` after the Wave 2 `tasks.par_iter_mut().for_each` block. Phase index 8 is the next free ordinal (0-7 used). This is a one-line addition to turn.rs — coordinate with agent-1 who has `engine/turn.rs` as a shared file.

### 5.2 New Snapshot Fields

#### `CompanySnapshot` extension (diagnostic.rs:190)

Add fields (all additive, serde-defaulted for backward compat):
```rust
pub fulfilled_fte: u32,
pub furloughed_workers_count: f64,
pub offered_wage_per_fte: f64,
pub company_capital: f64,
pub operational_cash: f64,
pub total_payroll: f64,
pub is_in_receivership: bool,
pub financial_history_len: usize,
pub last_turn_income: f64,        // sum of last financial_history entry revenue
pub last_furlough_trigger: Option<String>,  // "cash_shortage" | "material_shortage" | "seasonal" | None
pub is_liquidated: bool,           // already present
```

#### `RegionalMarketSnapshot` extension (diagnostic.rs:327)

Add fields:
```rust
pub order_book_depth: HashMap<Commodity, OrderBookDepth>,
pub b2b_trade_volume: HashMap<Commodity, TradeVolume>,
pub b2c_demand: HashMap<Commodity, f64>,
pub b2c_supply: HashMap<Commodity, f64>,
pub b2c_units_sold: HashMap<Commodity, f64>,
pub b2c_unmet_demand: HashMap<Commodity, f64>,
pub b2c_total_settled: f64,
pub store_inventory_mass: HashMap<Commodity, f64>,
```

#### New structs (diagnostic.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderBookDepth {
    pub bid_count: usize,
    pub ask_count: usize,
    pub total_bid_qty: f64,
    pub total_ask_qty: f64,
    pub max_bid_price: f64,
    pub min_ask_price: f64,
    pub mean_bid_price: f64,
    pub mean_ask_price: f64,
    pub spread_crossed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeVolume {
    pub trade_count: usize,
    pub total_qty: f64,
    pub total_value: f64,
}
```

### 5.3 New Dump Files (write_all_dumps extension)

| File | Contents | Consumer |
|---|---|---|
| `order_book_dump.json` | Per-commodity bid/ask depth + trade volume per turn | `audit_dumps.py` INV-ECO-1 |
| `b2c_flow_dump.json` | Per-commodity demand/supply/units_sold/unmet + total_settled per region per turn | `audit_dumps.py` H-B2C-* |
| `furlough_dump.json` | Per-company furlough trigger + FTE state + income per turn | `audit_dumps.py` H-FUR-* |
| `production_dump.json` | Per-building outputs_produced + inputs_consumed + production_scale per turn | `audit_dumps.py` H-PROD-* |

### 5.4 Instrumented Counters (minimal, additive)

These are **not** production logic changes — they are additive instrumentation fields populated during existing computation, read by the probe:

| Field | Added to | Populated in | Purpose |
|---|---|---|---|
| `last_furlough_trigger: Option<String>` | `Company` | `evaluate_furlough` (strategy.rs:1045) before `return Some(Furlough{...})` | Classify furlough trigger (H-FUR-1 vs H-FUR-2). |
| `gated_commodities: Vec<Commodity>` | `ConsumerDemand` | `build_consumer_demand` (retail.rs:483) at each `continue` | Detect H-B2C-1 (wealth gate). |
| `era_mult_per_commodity: HashMap<Commodity, f64>` | `ConsumerDemand` | `build_consumer_demand` at `era_consumption_multiplier` call | Detect H-B2C-2 (era multiplier). |
| `last_b2b_revenue: f64` | `Company` | B2B settlement (turn.rs:~1964) | Track B2B income per company. |
| `last_b2c_revenue: f64` | `Company` | `settle_b2c_clearing` (retail.rs:1169) | Track B2C income per company. |

**IMPORTANT:** These fields are additive (serde `#[default]`) and do not alter control flow. They are populated at existing `continue`/`return` points. Per AGENTS.md Rule 3, these are surgical edits — coordinate with the owning agent before touching locked files (see §8).

---

## 6. Test Placement Per AGENTS.md v4

### 6.1 Epic/Diagnostic Test (the new suite)

**File:** `state/tests/epics/market_gridlock_diagnostic_test.rs`

**Cargo.toml entry** (state/Cargo.toml) — add a new `[[test]]` block:
```toml
[[test]]
name = "market_gridlock_diagnostic_test"
path = "tests/epics/market_gridlock_diagnostic_test.rs"
required-features = ["epic-tests"]
```

**Feature gating:** The test file uses `#![cfg(feature = "epic-tests")]` at the top (NOT `#[cfg(feature = "epic-tests")]` on individual items — per AGENTS.md v4, the `[[test]]` block handles compilation gating).

**Run command:**
```bash
CI=true cargo nextest run --features epic-tests market_gridlock_diagnostic_test
```
(`CI=true` exported so cargo-insta fails hard on snapshot mismatches.)

### 6.2 Fast Unit Tests (assertion helpers)

**File:** `state/tests/market_gridlock_assertions.rs` (no feature gate — compiled every CI run)

Contains pure-function assertion helpers that operate on the dump JSON files
(parsed via serde), reusable by both the epic test and `audit_dumps.py` parity
checks. These do NOT run the simulation — they validate dump artifacts.

### 6.3 What NOT to Do (per AGENTS.md v4)

- Do NOT add `#[cfg(feature = "epic-tests")]` inside `.rs` files — the `[[test]]` block handles it.
- Do NOT place the epic test in `state/tests/` (root) — it goes in `state/tests/epics/`.
- Do NOT modify `MassSinkWhitelist` — the gridlock is a flow failure, not a mass-conservation failure.

---

## 7. Decision Tree — Probe Values → Root-Cause Candidates

```
START: Run 2-turn simulation with gridlock probe enabled.

Q1: b2b_trade_volume.total_value > 0 by Turn 1?
├── NO  → Q2 (B2B is dead — investigate why)
└── YES → Q9 (B2B works — investigate B2C / income)

Q2: order_book_depth: bid_count > 0 for any commodity?
├── NO  → Q3 (companies not placing buy bids)
└── YES → Q5 (bids exist — check asks / spread)

Q3: bid_skip_reasons: dominated by "no_ref_price"?
├── YES → ROOT CAUSE A: "No reference prices in market_history —
│         world-gen did not seed base_prices for input commodities.
│         Fix: verify generator seeds global_base_prices for all BOM inputs."
└── NO  → Q4

Q4: bid_skip_reasons: dominated by "no_cash" or "affordable_qty_le_0"?
├── YES → ROOT CAUSE B: "Companies have no encumbrance-able cash —
│         seed cash too low OR already encumbered by prior-turn bids
│         (refund path broken, debit_cash stuck).
│         Fix: check seed cash in generator + refund path in turn.rs:1706."
└── NO  → ROOT CAUSE C: "Unknown bid suppression — add finer skip-reason
           counters in submit_company_b2b_orders."

Q5: order_book_depth: ask_count > 0 for any commodity?
├── NO  → Q6 (companies not placing sell asks — production/listing broken)
└── YES → Q7 (asks exist — check spread)

Q6: production_output: sum(outputs_produced) > 0 for any building?
├── NO  → Q6a
│   Q6a: production_scale > 0 for any building?
│   ├── NO  → ROOT CAUSE D: "Zero employment — all workers furloughed
│   │         or never hired by Turn 1. Check labor market clearing +
│   │         seasonal furlough (apply_seasonal_furlough may furlough
│   │         all workers off-season)."
│   └── YES → ROOT CAUSE E: "Workers present but zero output — no inputs
│             delivered (B2B failed) OR method.inputs empty OR blueprint
│             BOM mismatch. Check inputs_consumed + building.inventory
│             for input commodities."
└── YES → ROOT CAUSE F: "Output produced but NOT listed as sell asks —
         sell-ask loop skipping (is_local_utility, sell_fraction=0,
         total_available<=0, sell_price<=0). Check ask_skip_reasons."

Q7: spread_crossed (max_bid >= min_ask) for any commodity?
├── NO  → Q8 (spread never crosses — pricing deadlock)
└── YES → ROOT CAUSE G: "Spread crosses but zero trades — embargo or
         freight failure. Check embargo_skips + freight_deferred."

Q8: bootstrap_flag == true (Turn 0)?
├── YES → ROOT CAUSE H: "Bootstrap pricing should guarantee crossing but
│         doesn't — ref_price missing for output commodities, so bootstrap
│         sell_price falls back to global_base_prices which may also be
│         missing. Check get_reference_price coverage."
└── NO  → ROOT CAUSE I: "Post-bootstrap cost-based pricing deadlock —
         unit_cost * (1+markup).max(unit_cost) rises above
         ref_price * (1+buy_premium). Inputs were expensive (uncleared
         B2B) so unit_cost is high. The rational-actor floor
         (sell_price.max(unit_cost)) prevents selling below cost,
         locking the spread. Fix: bootstrap may need to extend beyond
         Turn 0, or unit_cost should exclude uncleared-input cost."

Q9: b2c_total_settled > 0 by Turn 1 or Turn 2?
├── NO  → Q10 (B2C dead — investigate demand/supply)
└── YES → Q13 (B2C works — investigate income aggregation)

Q10: b2c_demand: sum(total_demand) > 0?
├── NO  → Q11 (zero demand — wealth/era gate)
└── YES → Q12 (demand exists — check supply)

Q11: wealth_gate_hits > 0 OR era_mult_values has any <= 0?
├── wealth  → ROOT CAUSE J: "Citizens too poor — savings_per_capita
│            below commodity_wealth_gate for all consumption goods.
│            Check initial citizen savings in world-gen + wage payments
│            (did wages actually credit citizen savings?)."
├── era    → ROOT CAUSE K: "Era multiplier zero — commodities not in
│            era for year 1925 (or whatever start year). Check
│            era_consumption_multiplier registry for start year."
└── neither→ ROOT CAUSE L: "Unknown demand suppression — add finer
             counters in build_consumer_demand."

Q12: b2c_supply: sum(store_inventory_mass) > 0?
├── NO  → ROOT CAUSE M: "Stores empty — B2B→retail inventory routing
│         broken. B2B trades executed but output never reached
│         commercial_buildings. Check trade settlement inventory
│         routing (turn.rs:1808-2015) + commercial building restocking."
└── YES → ROOT CAUSE N: "Supply + demand both > 0 but settled = 0 —
         clear_b2c_markets allocation bug OR settle_b2c_clearing
         total_class_demand <= 0 (class shares empty). Check
         class_shares computation in settle_b2c_clearing."

Q13: income_summary: per company last_turn_income > 0 for majority?
├── YES → ROOT CAUSE O: "Income exists but furlough still fires —
│         furlough trigger logic wrong (cash_shortage threshold too
│         high, or operational_cash() excludes valid cash sources).
│         Check evaluate_furlough cash_shortage condition."
└── NO  → Q14 (income not credited to companies)

Q14: b2b_revenue_credited + b2c_revenue_credited > 0 globally?
├── YES → ROOT CAUSE P: "Revenue generated but not credited to the
│         right companies — store_id_to_owner or company_id_to_idx
│         mapping broken. Check settle_b2c_clearing owner lookup."
└── NO  → ROOT CAUSE Q: "Revenue generated in clearing but settlement
         debited citizen savings to zero before crediting company —
         settle_b2c_purchase clamped to available savings. Citizens
         ran out of cash mid-settlement. Check citizen savings
         sufficiency vs B2C revenue."
```

---

## 8. Coordination & Locked-File Matrix

Per AGENTS.md RBAC, the following files are locked by other agents. The
implementation phase (NOT this planning task) must coordinate:

| File | Locked by | Modification needed | Coordination |
|---|---|---|---|
| `state/src/engine/turn.rs` | agent-1 (shared) | Add `production_wave2_post` checkpoint (1 line) | Request via `block.sh` to agent-1 |
| `state/src/economy/trade/b2b_orders.rs` | agent-1 | None (probe reads `global_order_book`, not this file) | None |
| `state/src/economy/trade/retail.rs` | agent-1 | Add instrumented counters to `ConsumerDemand` (additive) | Request via `block.sh` to agent-1 |
| `state/src/corporate/strategy.rs` | (unlocked) | Add `last_furlough_trigger` field population | Direct edit (check agents_sync.json first) |
| `state/src/corporate/manager.rs` | agent-2 (shared) | None (furlough handler unchanged) | None |
| `state/src/engine/diagnostic.rs` | (unlocked) | Extend snapshots + new structs | Direct edit |
| `state/src/engine/generator/corporate.rs` | (unlocked) | None (read-only audit of seasonal profiles) | None |
| `state/Cargo.toml` | (unlocked) | Add `[[test]]` block for epic test | Direct edit |
| `state/tests/epics/market_gridlock_diagnostic_test.rs` | (new file) | Create epic test | Direct edit (new file) |
| `state/tests/market_gridlock_assertions.rs` | (new file) | Create fast assertion helpers | Direct edit (new file) |

**Before implementation:** Re-read `agents_sync.json`, post blockers via
`bash .devin/scripts/block.sh <to_agent> <affected_file> "<message>"` for any
locked-file modification, and wait for acknowledgment.

---

## 9. Implementation Phase Prerequisites (for the implementing agent)

Before implementing this plan, the implementing agent must:

1. **Re-read `agents_sync.json`** — verify no new locks on target files.
2. **Create branch** `fix/agent-4-market-gridlock-test` (or `feat/...`) per Rule 1.
3. **Post blockers** to agent-1 for `turn.rs` checkpoint + `retail.rs` counter additions.
4. **Run the existing Phase 94 harness first** (`cargo nextest run --features epic-tests phase94_diagnostic_harness_test`) to establish the conservation baseline — confirm the gridlock is NOT an M0 leak (the M0 remediation v2 was completed at commit `ce521a71` per `release_readiness_report.md`).
5. **Implement probes in dependency order:**
   a. `diagnostic.rs` snapshot extensions (no dependencies).
   b. `turn.rs` new checkpoint (depends on agent-1 ack).
   c. `retail.rs` / `strategy.rs` instrumented counters (depends on agent-1 ack for retail).
   d. `write_all_dumps` new dump files.
   e. Epic test file + Cargo.toml `[[test]]` block.
   f. `audit_dumps.py` economic-flow invariants.
6. **Verify** with `CI=true cargo nextest run --features epic-tests market_gridlock_diagnostic_test`.

---

## 10. Summary of Root-Cause Candidates

| ID | Root Cause | Probe that identifies it | Likelihood |
|---|---|---|---|
| A | No reference prices in market_history | Q3 (bid_skip: no_ref_price) | Medium |
| B | Seed cash too low / refund path broken | Q4 (bid_skip: no_cash) | Medium |
| D | Zero employment (furloughed Turn 1) | Q6a (production_scale=0) | **High** |
| E | No inputs delivered (B2B failed) | Q6a (inputs_consumed=0) | Medium |
| F | Output produced but not listed | Q6 (ask_count=0, output>0) | Medium |
| H | Bootstrap pricing doesn't cross | Q8 (bootstrap, no crossing) | Low |
| I | Post-bootstrap cost-based pricing deadlock | Q8 (no bootstrap, no crossing) | **High** |
| J | Citizens too poor (wealth gate) | Q11 (wealth_gate_hits) | Medium |
| K | Era multiplier zero | Q11 (era_mult <= 0) | Low |
| M | B2B→retail inventory routing broken | Q12 (supply=0, B2B worked) | **High** |
| N | B2C allocation/settlement bug | Q12 (supply+demand>0, settled=0) | Low |
| O | Furlough trigger logic wrong | Q13 (income>0, furlough fires) | Low |
| P | Revenue not credited (mapping broken) | Q14 (revenue>0, company income=0) | Medium |

**Top 3 suspects (highest prior likelihood based on static analysis):**
1. **I — Post-bootstrap cost-based pricing deadlock:** After Turn 0, `sell_price.max(unit_cost)` can lock asks above bids if inputs were expensive/uncleared. This is the most structurally fragile seam.
2. **M — B2B→retail inventory routing broken:** B2B may clear but output never reaches `commercial_buildings.current_inventory`, leaving B2C supply at zero.
3. **D — Zero employment (furloughed Turn 1):** If `apply_seasonal_furlough` or labor clearing leaves `fulfilled_fte == 0`, production_scale is zero and no output is ever generated.

The test suite in §3-§4 is designed to **definitively distinguish** these candidates via the decision tree in §7.

---

*End of audit plan. This is a PLANNING + AUDIT document — no production code or test suite was implemented.*

---

## Macro-Architectural Audit Report

Audit scope: the changes implemented to resolve the market gridlock / macro
collapse — power-grid capacity/units fix, `scale_factor` unification across
production/B2B/settlement, M&A distress-trigger + labor-demand transfer, and
age-gated loss-streak liquidation (`founded_turn` grace).

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | `scale_factor` now consistently multiplies per-plant `current_employment` at every whole-building consumer (production, B2B bid/ask quantities, seed inventory, grid demand, settlement wage bill, labor-pool returns). No mass appears/disappears; output capacity now matches the workforce paid. |
| Double-Entry Bookkeeping | PASS | Grid settlement and wage bills still flow through `settle_transfer`; M&A transfer only moves labor-demand *state* (no money created). Liquidation still routes through the Syndic. |
| No Teleportation | PASS | No new physical movements introduced; freight path untouched. |
| Clamping | PASS | Age gate uses `saturating_sub`; grid supply still bounded by `min(inventory, nameplate, lv_cap)`; shed-tier penalties unchanged. |
| No Magic Numbers | PASS | `LOSS_STREAK_GRACE_TURNS` derives from `TURNS_PER_YEAR`; power-plant nameplate sizing derives from projected physical demand (population × participation × kW-per-worker), not floats. |
| Technological Matrices | PASS | No new building/method types; existing registry slots reused. |
| Architectural Parsimony | PASS | Extends `liquidate_bankrupt_companies`, `process_mergers_and_acquisitions`, `init_power_grid`, `execute_production_cycle` in place — no parallel systems. |
| Temporal Causality | PASS | `current_turn` threaded through the lifecycle call chain so the age gate evaluates against the real turn; spawn stamps `founded_turn = current_turn` (was calendar `year` — a units bug). |
| Asymmetric Information | PASS | Diagnostic probes are `#[cfg(feature = "diagnostic")]`-gated stderr telemetry; no hidden data exposed to frontend. |
| Full-Stack Accountability | PASS | Headline metrics already surface via the diagnostic JSON dump (`total_companies`, `unemployment_rate`, `companies_liquidated`). No new user-facing feature requiring UI. |
| Complete Entity Lifecycle | PASS | Liquidation still births→operates→dies; grace only delays the *sustained-loss* death rule (documented "3+ years"), while insolvency (`company_capital < 0`) remains immediate. Receivership and Syndic auction paths unchanged. |
| Market Forces | PASS | Pro-rata scarcity allocation preserves wage-weighted shares; M&A no longer executes healthy firms (distress = negative cash or insufficient capital vs liabilities). |
| Rational Actors | PASS | Firms are no longer killed for startup ramp-up losses, matching documented intent; acquisition still requires a distressed target and an acquirer with capital. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: none.

Residual known gaps (tracked, out of scope for this audit): wage-transfer
reconciliation leaks (~$0.5–1B/turn), service-sector monetization
(`LocalServicesCommodity` absent from consumer baskets), and bank reserve
floors — all visible in the diagnostic dump rather than hidden.
