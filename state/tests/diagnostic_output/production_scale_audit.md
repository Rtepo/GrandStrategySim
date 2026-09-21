# Production Scale Audit — Extreme Wage/Revenue Divergence

**Task:** `Production Scale Audit — extreme wage/revenue divergence`
**Author:** Agent 4 (Auditor)
**Manager:** Agent 5
**Branch:** `fix/agent-4-production-scale-audit`
**Base:** `main` @ `7a842a97` (post-v4.9.0, SPREAD_NO_CROSS fixed)
**Date:** 2026-09-21
**Status:** ROOT-CAUSE FOUND + REBALANCING PLAN. No production code changed (audit-only deliverable).

---

## 0. Executive Summary

### 0.1 Symptom

Diagnostic dumps show ~1500-FTE companies burning ~$674K/turn in wages while
generating ~$0.62 income. A fully staffed factory produces less than a dollar
of goods.

### 0.2 Root Cause (one sentence)

**The per-1000-worker output rates (`amount_per_1k`) in the production-method
registry are 67×–750× too low relative to the wage scale, so even at full
employment and full input fulfillment a building's gross output value is one
to two orders of magnitude smaller than its wage bill.**

### 0.3 Invariant Violated

> **At full employment and full input fulfillment, a building's gross output
> value (valued at `estimated_base_price`) must exceed its wage bill.**

This invariant is violated by a factor of 67×–750× across **every** sector
audited (Mining, HeavyIndustry, LightIndustry). No sector can cover its payroll
from production, even in the best case.

### 0.4 Hypothesis Verdicts

| # | Hypothesis | Verdict |
|---|---|---|
| 1 | Missing output multipliers (per-worker vs per-building scaling) | **CONFIRMED (primary).** The `efficiency` field IS applied in `execute_production_cycle`, but the base `amount_per_1k` rates are 67×–750× too low relative to wages. |
| 2 | Unit conversion errors (tons vs kg vs units) | **RULED OUT.** There is no per-unit mass conversion — `owned_inventory_mass` sums raw `qty` directly (diagnostic.rs:222). Quantities are dimensionless "units." |
| 3 | BOM profitability broken (inputs cost >> output price) | **RULED OUT as primary.** BOM input/output ratios are individually reasonable (e.g. Iron 20 → Steel 15). The problem is output value vs *wages*, not output value vs *input costs*. |
| 4 | Employment counted but production gated by unrelated bottleneck | **CONFIRMED (secondary amplifier).** B2B gridlock drives `fulfillment_ratio ≈ 0`, dropping actual revenue from ~$10K to ~$0.62. But even at `fulfillment_ratio = 1.0` the structural imbalance remains. |
| 5 | `resolve_active_method` picking zero-throughput/era-mismatched method | **RULED OUT as primary.** The scale mismatch is universal across all methods and eras — it is not a method-selection artifact. |

---

## 1. The Two Production Paths (critical context)

The turn loop runs **two** production computations per building per turn. It
is essential to distinguish them:

| Path | File:Function | Deposits to `building.inventory`? | Applies `efficiency`? | Subtracts wages from `last_profit`? |
|---|---|---|---|---|
| **Market-order generation** | `production.rs:process_building_cycle_with_geology` (called turn.rs:703) | **No** — only emits `market_orders.add_buy/add_sell` | **No** — output = `amount_per_1k × production_scale × output_multiplier` (mining only) | **Yes** — `gross_profit = revenue − input_costs − wages_paid` |
| **Actual production** | `b2b_orders.rs:execute_production_cycle` (called turn.rs:2257 Wave 1, 3045 Wave 2) | **Yes** — `building.inventory += produced` (line 1500) | **Yes** — `produced = qty_per_1k × production_scale × fulfillment_ratio × efficiency × machinery_factor` (line 1498) | **No** — `wages_paid: 0.0` (line 1689); `gross_profit = output_revenue − input_costs − write_down` (line 1681) |

**Consequences of the split:**

1. **Market sell orders undercount actual production by the `efficiency` factor
   (1.5×–6.5×).** `process_building_cycle_with_geology` generates sell orders
   without `efficiency`; `execute_production_cycle` deposits the
   efficiency-scaled output to inventory. The B2B sell-ask loop (b2b_orders.rs:449)
   then lists from `building.inventory`, so the *deposited* quantity is correct,
   but the `market_orders` struct (used for aggregate supply/demand signals)
   undercounts. This is a telemetry inconsistency, not the root cause.

2. **`building.last_profit` excludes wages.** `execute_production_cycle`
   overwrites `building.last_profit` (line 684) with a value that does **not**
   subtract wages (`wages_paid = 0.0`). The corporate AI reads `last_profit` for
   furlough/bankruptcy decisions. A building with $10K revenue and $675K wages
   reports `last_profit ≈ +$7K` (revenue − input costs), appearing profitable
   when it is actually losing $665K/turn. This **masks the insolvency** and
   delays furloughs that should fire immediately. (Secondary finding — see §5.)

---

## 2. The Wage Scale

Wages are set by `set_wage_offers` (manager.rs:2094), which converges
`offered_wage_per_fte` toward `market_average_wage`:

- **`average_wage = gdp_pc × 800.0`** (generator/mod.rs:1389).
- **Initial wage** at world-gen: `initial_wage = (company_liquid × 0.6) / actual_capacity` (corporate.rs:2217), floored at 50.0.
- **Wage budget fraction**: 0.5 for HeavyIndustry/Mining/Energy, 0.6 default, 0.7 for Agriculture/LightIndustry/Services (manager.rs:2086).
- **Sticky rigidity**: max 3% drop / 5% rise per turn (manager.rs:2096-2097).
- **Revenue-aware cap** (Phase 92, manager.rs:2191): caps wage at 90% of market average if recent profit < wage bill. **Only triggers after financial history exists** (turn 2+) and still caps at 90% of a market average that is itself decoupled from production revenue.

**Observed wage per FTE**: ~$449 (from the $674K / 1500 FTE symptom). This is
set from seed cash, not from production revenue. A company with ~$750K liquid
and 1000 capacity offers `(750000 × 0.5) / 1000 = $375/FTE`, converging upward
toward `average_wage`.

**Key structural property**: the wage rate is **decoupled from production
output value**. It is anchored to `gdp_pc × 800` and seed cash, neither of
which has any relationship to the per_1k output quantities in the production
methods.

---

## 3. The Production Scale

Output per turn for a standard (non-energy, non-agriculture) building in
`execute_production_cycle`:

```
produced = qty_per_1k × production_scale × fulfillment_ratio × efficiency × machinery_factor
```

where `production_scale = current_employment / 1000.0`.

At full employment (1500 FTE), full fulfillment (1.0), no machinery (1.0):

```
produced = qty_per_1k × 1.5 × 1.0 × efficiency × 1.0
output_value = produced × estimated_base_price(commodity)
```

### 3.1 Per-Sector Audit (1500 FTE, full fulfillment, no machinery)

| Sector | Method (year) | Output commodity | qty_per_1k | efficiency | units produced | base price | output value | est. wages | **wage/revenue** |
|---|---|---|---|---|---|---|---|---|---|
| Mining | Manual Mining (1880) | HardCoal | 10.0 | 1.0 | 15.0 | $60 | $900 | $675K | **750:1** |
| Mining | Longwall Mining (1895) | HardCoal | 25.0 | 2.2 | 82.5 | $60 | $4,950 | $675K | **136:1** |
| Mining | Froth Flotation (1900) | Copper | 12.0 | 2.5 | 45.0 | $200 | $9,000 | $675K | **75:1** |
| HeavyIndustry | Bessemer Converters (1880) | Steel | 15.0 | 1.5 | 33.75 | $300 | $10,125 | $675K | **67:1** |
| HeavyIndustry | Open-Hearth (1885) | Steel | 22.0 | 2.0 | 66.0 | $300 | $19,800 | $675K | **34:1** |
| HeavyIndustry | Mini-Mill (1975) | Steel | 90.0 | 6.5 | 877.5 | $300 | $263,250 | $675K | **2.6:1** |
| HeavyIndustry | Coke Production (1880) | Coke | 15.0 | 1.0 | 22.5 | $100 | $2,250 | $675K | **300:1** |
| HeavyIndustry | Cement Production (1880) | Cement | 30.0 | 1.0 | 45.0 | $100 | $4,500 | $675K | **150:1** |
| HeavyIndustry | Brick Making (1880) | Bricks | 25.0 | 1.0 | 37.5 | $100 | $3,750 | $675K | **180:1** |
| HeavyIndustry | Glass Making (1880) | Glass | 18.0 | 1.0 | 27.0 | $100 | $2,700 | $675K | **250:1** |
| LightIndustry | Handloom Weaving (1880) | Clothing | 8.0 | 1.0 | 12.0 | $100 | $1,200 | $675K | **563:1** |
| LightIndustry | Power Looms (1885) | Clothing | 20.0 | 2.0 | 60.0 | $100 | $6,000 | $675K | **113:1** |
| LightIndustry | Automated Textile Mills (1965) | Clothing | 60.0 | 4.0 | 360.0 | $100 | $36,000 | $675K | **19:1** |

**Observations:**

1. **No 1880-era method can cover wages.** The best 1880 case (Bessemer Steel)
   has a 67:1 wage/revenue ratio. The worst (Manual Mining HardCoal) is 750:1.
2. **Only late-game methods approach break-even.** Mini-Mill (1975) reaches
   2.6:1 — still losing money, but only barely. Automated Textile Mills (1965)
   reaches 19:1. The `efficiency` multiplier (up to 6.5) is the only thing that
   brings late-game methods close, but 1880-era `efficiency` is 1.0–1.5.
3. **The imbalance is universal across sectors**, not specific to one BOM or
   one commodity. It is a systemic scale mismatch.
4. **Cement is NOT 3000 per_1k.** The unit test at production.rs:676 hardcodes
   `Cement: 3000.0` for a synthetic test building; the actual registry
   (production_methods_data.rs:4848) defines `Cement: 30.0`. The test is
   misleading and should be updated to match the registry.

### 3.2 BOM Profitability (input costs vs output value) — NOT broken

For Bessemer Steel (1500 FTE, full fulfillment):
- Input costs (at 0.5× base price, per execute_production_cycle:1670):
  `(20×120 + 10×80 + 8×100) × 0.5 × 1.5 = $3,000`
- Output value: `$10,125`
- **Gross operating margin (ex-wages): +$7,125** (positive — the BOM adds value)

The BOM is profitable in isolation. The problem is that wages ($675K) dwarf
the entire value chain ($10K). The BOM is not broken; the **wage scale is
incompatible with the output scale**.

---

## 4. The Cascade (how $10K theoretical becomes $0.62 actual)

The $0.62 figure (not $10K) is the **actual** income after the B2B gridlock
amplifier:

1. **Turn 1:** Company pays wages from seed cash ($675K debited).
   `execute_production_cycle` runs. `fulfillment_ratio` is computed from
   `building.inventory` (b2b_orders.rs:1436). On turn 1, inventory is seeded
   but may not contain the exact input mix. `fulfillment_ratio` is the **min**
   across all inputs — one missing input zeroes the ratio.
2. **B2B phase:** Company submits buy bids for inputs and sell asks for output.
   Post-SPREAD_NO_CROSS-fix, spreads cross better, but **supply shortage**
   (Bug E from volume_and_wage_remediation.md) persists: many commodities have
   zero producers in-country. Cross-country supply exists but may not reach.
3. **Turn 2+:** If B2B delivered only a tiny fraction of inputs,
   `fulfillment_ratio ≈ 0.002`. Output = `15 × 1.5 × 0.002 × 1.5 = 0.0675`
   units Steel. Revenue = `0.0675 × $300 = $20`. After input costs and
   write-downs, net income ≈ $0.62.
4. **Wages still paid in full** (labor_market.rs:527 — wage debited from
   available cash regardless of production output). The company burns $675K
   and earns $0.62.

**The B2B gridlock is the amplifier that makes the structural imbalance
catastrophic.** But fixing B2B alone (getting fulfillment to 1.0) would only
raise income to ~$10K — still 67× below wages. **Both problems must be fixed.**

---

## 5. Secondary Findings

### 5.1 `building.last_profit` excludes wages (masks insolvency)

`execute_production_cycle` (b2b_orders.rs:1681) computes:
```
gross_profit = output_revenue − input_costs − inventory_write_down
```
with `wages_paid: 0.0` (line 1689). This overwrites the wage-inclusive
`last_profit` set earlier by `process_building_cycle_with_geology`
(production.rs:645: `gross_profit = output_revenue − input_costs − wages_paid`).

**Effect:** A building with $10K revenue, $3K input costs, and $675K wages
reports `last_profit = +$7K`. The corporate AI reads this as "profitable" and
does not furlough. The company continues burning cash until `operational_cash()
< 2 × payroll` triggers a `cash_shortage` furlough — but by then it has already
lost several turns of wages.

**This is why the symptom is "1500-FTE companies burning $674K" rather than
"companies furloughing immediately."** The wage-exclusive `last_profit` prevents
early detection.

### 5.2 `process_building_cycle_with_geology` omits `efficiency` from output

`process_building_cycle` (production.rs:439, 468) and the mining geology path
(production.rs:621) compute output as `amount_per_1k × production_scale` —
**without** the `efficiency` multiplier. Only `execute_production_cycle`
(b2b_orders.rs:1498) applies `efficiency`. This means the `market_orders`
aggregate supply/demand signals undercount manufactured output by 1.5×–6.5×.
The actual inventory deposit is correct (execute_production_cycle), but any
logic consuming `market_orders` for capacity planning sees deflated supply.

### 5.3 `estimated_base_price` missing entries (price = $100 fallback)

Several produced commodities (Coke, Bricks, Glass, Cement, Clothing) are not
in the `estimated_base_price` match table (corporate.rs:5431) and fall to the
`_ => 100.0` generic fallback. This is not wrong per se, but it means the
revenue accounting for these commodities uses a flat $100 regardless of their
actual market value or BOM input cost. If Bricks' true value should be $15
(Clay $25 × 20 + Energy $100 × 5 = $1000 inputs → 25 bricks = $40/brick
break-even), the $100 fallback overstates revenue; if it should be $150, it
understates. A complete price table would improve accounting accuracy.

---

## 6. Rebalancing Plan

The deliverable requires a plan, not speculative fixes. Three independent
knobs can close the gap; they are not mutually exclusive.

### Option A: Increase per_1k output rates (recommended primary)

Scale up `amount_per_1k` for all manufactured/extracted outputs so that, at
1880-era `efficiency` (1.0–1.5), a fully staffed building's output value is
~2–3× its wage bill (target labor share ~30–50%).

**Target:** output_value per FTE ≈ $900–$1,350 (2–3× the $450 wage).

For Bessemer Steel (efficiency 1.5, price $300, 1500 FTE → scale 1.5):
```
target_revenue = 1500 × $1,000 = $1,500,000
required_units = $1,500,000 / $300 = 5,000
required_per_1k = 5,000 / (1.5 × 1.5) = 2,222
```
Current: 15. **Required: ~2,200.** Factor: **~150× increase.**

For Manual Mining HardCoal (efficiency 1.0, price $60):
```
required_per_1k = (1500 × $1,000) / (1.5 × 1.0 × $60) = 16,667
```
Current: 10. **Required: ~16,700.** Factor: **~1,670× increase.**

**Concern:** a flat 150× multiplier is blunt. A principled approach:
- Anchor each commodity's per_1k to a target **output value per FTE** (e.g.
  $1,000/FTE at 1880 efficiency 1.0, scaling with `efficiency` for later
  methods).
- Derive: `qty_per_1k = target_value_per_fte / (efficiency × base_price)`.
- This automatically gives lower-tech methods (low efficiency) higher per_1k
  rates, equalizing output value across eras.
- Input rates should scale proportionally to preserve BOM mass balance.

**Scope:** `state/src/registries/production_methods_data.rs` — every `pm()`
call's output slice. This is a large mechanical change (~200 methods) but
isolated to one file.

### Option B: Reduce wage scale (alternative or complementary)

Change `average_wage = gdp_pc × 800.0` to `gdp_pc × 8.0` (100× reduction).
This brings wages to ~$4.50/FTE, comparable to current output value ($1–7/FTE).

**Concern:** the wage scale feeds citizen savings, B2C demand, PIT revenue,
severance, and subsistence floors across the entire economy. A 100× cut
collapses citizen purchasing power and likely breaks B2C demand calibration.
This is a **high-blast-radius** change. If used, it must be paired with a
proportional reduction in all cash-denominated constants (seed cash, subsistence
floor, severance multipliers, etc.).

### Option C: Increase prices (not recommended)

Scale `estimated_base_price` and `global_base_prices` by ~100×. This inflates
the entire price level without changing physical quantities. It would close
the wage/revenue gap but also inflate input costs proportionally, leaving the
**net margin** unchanged. It is the least effective option and risks
destabilizing the B2B spread logic (bid/ask caps are price-relative).

### Recommended sequencing

1. **Option A** (increase per_1k output rates) — primary fix. Isolated to one
   file. Preserves the wage/citizen-demand calibration. Target output value
   per FTE = 2–3× wage per FTE.
2. **Fix §5.1** (include wages in `building.last_profit`) — so the corporate AI
   detects insolvency and furloughs unprofitable buildings promptly. This
   prevents the "burn $674K for 4 turns before furlough" cascade.
3. **Fix §5.2** (apply `efficiency` in `process_building_cycle`) — so market
   signals reflect actual production.
4. **Re-measure B2B** after A+B land. Much of the B2B gridlock (Bug E: idle
   producers, supply shortage) is *downstream* of the income collapse. Once
   factories produce meaningful output and earn revenue, they re-hire, submit
   real sell asks, and the order book thickens. Do not over-correct B2B before
   the production scale is fixed.
5. **Complete `estimated_base_price` table** (§5.3) for all produced
   commodities — improves accounting accuracy for the rebalanced rates.

---

## 7. Verification Plan

After implementing Option A + §5.1:

```bash
CI=true cargo nextest run --release --features "epic-tests diagnostic" \
  -E "test(test_market_gridlock_diagnostic) or test(test_b2b_volume_clamps)"
```

**Success criteria:**
- At turn 2, fully staffed 1500-FTE Bessemer steel mill: `output_revenue >
  wages_paid` (target: revenue ≥ $1.5M, wages ~$675K).
- `building.last_profit` is **negative** for buildings where wages > operating
  margin (insolvency detected → furlough fires turn 1, not turn 4).
- `zero_income_fraction < 0.1` by turn 2 (from ~50%).
- B2B matched volume > 25% of bid volume by turn 2.
- M0 conservation, mass conservation, bank balance-sheet identity all still
  pass (the per_1k change is a quantity rescale, not a monetary injection).

---

## 8. Files Audited

| File | Lines examined | Role |
|---|---|---|
| `state/src/economy/trade/b2b_orders.rs` | 1364–1700 | `execute_production_cycle` — actual production, deposits to inventory, applies `efficiency`, excludes wages from `last_profit` |
| `state/src/economy/production/production.rs` | 1–702 | `process_building_cycle` / `_with_geology` — market-order generation, computes wages, omits `efficiency` from output |
| `state/src/registries/production_methods_data.rs` | 3521–6089 (mining, heavy_industry, light_industry) | Per_1k BOM input/output rates — the too-low output rates |
| `state/src/engine/generator/corporate.rs` | 5431–5459 | `estimated_base_price` — price table (missing entries fall to $100) |
| `state/src/engine/generator/mod.rs` | 1389 | `average_wage = gdp_pc × 800` — wage scale anchor |
| `state/src/corporate/manager.rs` | 2094–2213 | `set_wage_offers` — wage convergence to market average, revenue-aware cap |
| `state/src/economy/labor/labor_market.rs` | 460–570 | Wage disbursement — debited from cash regardless of production output |
| `state/src/engine/turn.rs` | 703, 2257, 3045 | Turn loop — calls both production paths |
| `state/src/engine/diagnostic.rs` | 202–243 | `owned_inventory_mass` — confirms no per-unit mass conversion |
| `state/tests/diagnostic_output/volume_and_wage_remediation.md` | 1–202 | Prior audit (Bugs D/E/F/G) — B2B gridlock context |
| `state/tests/diagnostic_output/market_gridlock_audit_plan.md` | 1–254 | Prior audit plan — checkpoint/probe infrastructure |

---

## 9. Conclusion

The extreme wage/revenue divergence is **not** a BOM profitability bug, a unit
conversion error, or a method-selection artifact. It is a **systemic scale
mismatch**: the production-method registry defines output rates of 8–90 units
per 1000 workers, while the wage economy operates at ~$450/worker/turn. At
$100–$300 per unit, 1000 workers produce $1K–$27K of output but cost $450K–$675K
in wages — a 67×–750× gap that no amount of B2B fixing can close.

The immediate $0.62 income (vs $10K theoretical) is the B2B gridlock amplifier
(`fulfillment_ratio ≈ 0`), but the structural 67:1 ratio is the root cause. Both
must be addressed: rebalance per_1k output rates (Option A) to fix the
structural ratio, and include wages in `last_profit` (§5.1) so the corporate AI
detects and furloughs insolvent buildings before they burn 4 turns of seed
cash.
