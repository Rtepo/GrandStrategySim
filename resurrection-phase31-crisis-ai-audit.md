# Phase 31 — Crisis Management AI, Ideologies & Panic Audit

**Read-only audit and technical blueprint.** No Rust code to be written until this blueprint is explicitly approved.

### Architectural Corrections (User-Mandated)

The following five strict rules override the original blueprint design and are non-negotiable:

1. **DEFER Legislative Voting — Executive Decrees Only.** The parliament/legislative voting mechanics are NOT functional. Crisis Management AI actions (tax changes, bond issuance, subsidies) are executed as **Executive Decrees** directly by the ruling government in `process_political_turn`. The `bill_lifecycle` stub is bypassed entirely. No legislative engine is built in this phase.

2. **Strict Double-Entry for Sovereign Bonds.** Sovereign bonds must be physically purchased by Banks, Funds, or Citizens through the `DebtMarket`. If the private sector lacks liquidity to buy the bonds, the auction FAILS and the State gets no money. The existing `issue_treasury_securities` function has a critical flaw: it uses `country.budget.liquid_reserves` as buyer capacity (the government buying its own bonds with its own reserves — circular money printing). This must be fixed to source real private-sector liquidity from bank reserves and citizen savings.

3. **True Starvation & Emigration.** During deep crises (wages below subsistence, or unemployment > 40% for multiple turns), emigration and starvation mechanics must actively reduce `ClassDemographics` population. Population must not magically grow during famine.

4. **No Polish Words — Use Proper Enums.** All commodity lookups must use the `Commodity` enum (e.g., `Commodity::Food`), not hardcoded string keys (especially not Polish strings like `"Zywnosc"`). The codebase is strictly English-only. Market prices are keyed by `HashMap<Commodity, f64>` and `MarketHistory` uses `HashMap<Commodity, f64>` for all price maps (`vwap_per_commodity`, `last_trade_price`, `global_base_prices`, `retail_vwap_per_commodity`).

5. **Realistic Per-Turn Mortality & Emigration Rates.** The engine runs 24 turns per year. Per-turn rates must be divided by ~24 to reflect realistic annual scaling. Maximum starvation mortality: 0.1%–0.5% per turn (compounds to 2.4%–12% annual mortality in severe famine). Maximum famine emigration: 0.10% per turn (compounds to ~2.4% annual exodus). The originally proposed 1%–6% mortality and 8% emigration per turn would yield 24%–144% mortality and 192% emigration per year — mathematically apocalyptic.

---

## Summary

Phase 30 introduced physical logistics with fuel costs, but a **dimensional inconsistency bug** in the composite edge-weight function causes freight costs to explode 5–7×. This single bug cascades through the entire economy: companies can't afford transport, so trades are deferred and cancelled; without settled trades, companies lose revenue and can't afford fixed-asset purchases (I=0); without cross-region trade, the global market order book is empty and `balance_global_trade` produces NX=0; and companies resort to shadow employment to cut costs, causing the shadow economy to explode. Meanwhile, the government AI is completely passive — it has no automatic tax adjustment, no sovereign bond issuance, and no crisis-response legislation. This blueprint fixes the root-cause bug, introduces a Crisis Management AI with ideology-driven fiscal policy, and adds bounded-rationality fallbacks to prevent algorithmic panic loops.

---

## PART 1: The "Algorithmic Panic" & Economic Shock Audit

### 1.1 Root Cause: The Freight Cost Dimensional Bug (CRITICAL)

**File:** `state/src/economy/logistics/logistics.rs`

The Phase 30 `edge_weight()` function (line 168) computes a composite weight:

```
weight = friction + fuel_cost_per_km + toll_cost
```

where `fuel_cost_per_km = fuel_consumption_per_km × fuel_price` (e.g., `0.08 × 80.0 = 6.4` for unimproved roads).

The Dijkstra algorithm sums these weights into `total_cost`, then at line 424:

```rust
let avg_friction = total_cost / total_distance;
```

This `avg_friction` is stored as `route.friction_multiplier`, which is then used in `freight_cost()` at line 520:

```rust
quantity * route.distance_km * route.friction_multiplier * base_rate
```

**The bug:** `fuel_cost_per_km` is in **currency per km** (e.g., 6.4), while `friction` is **dimensionless** (e.g., 1.0). Adding them produces a hybrid quantity that is then treated as a dimensionless multiplier and multiplied by `base_rate` (which is in currency per ton-km). The fuel cost is effectively multiplied by `base_rate` a second time.

**Numerical impact (land border, NetworkLevel::None):**

| Component | Before Phase 30 | After Phase 30 |
|---|---|---|
| friction | 1.0 | 1.0 |
| fuel_cost_per_km | — | 0.08 × 80 = 6.4 |
| edge_weight | 1.0 | 7.4 |
| avg_friction (route) | 1.0 | 7.4 |
| freight_cost per ton-km | 1.0 × 0.05 = **0.05** | 7.4 × 0.05 = **0.37** |
| **Cost increase** | — | **7.4×** |

**Maritime impact (SeaLane):**

| Component | Before Phase 30 | After Phase 30 |
|---|---|---|
| friction | 0.3 | 0.3 |
| fuel_cost_per_km | — | 0.015 × 80 = 1.2 |
| edge_weight | 0.3 | 1.5 |
| freight_cost per ton-km | 0.3 × 0.05 = **0.015** | 1.5 × 0.05 = **0.075** |
| **Cost increase** | — | **5×** |

**This is the single root cause of the entire economic shock.**

### 1.2 Shadow Economy Explosion — Why 300% of Official GDP?

**Files:** `state/src/economy/justice/legal_status.rs`, `state/src/economy/justice/inspectorates.rs`

The shadow economy explosion is a **downstream symptom** of the freight cost bug, not an independent bug:

1. **Trigger mechanism** (`trigger_shadow_employment`, line 239): Companies enter the shadow economy when `unmet_demand >= target_fte_demand × 0.5`. With freight costs 7× higher, companies can't transport goods, can't sell outputs, can't pay legal wages, and fulfill less labor demand → the 50% unmet threshold is easily crossed.

2. **Shadow wage calculation**: Shadow workers are paid 50% of the legal wage (`DEFAULT_SHADOW_WAGE_FRACTION = 0.50`). This is cheaper, so companies resort to it to survive.

3. **Shadow GDP accumulation** (turn.rs line 1966): `task.gdp_acc.shadow_gdp += shadow_result.total_shadow_wages`. With many companies in shadow employment, shadow wages accumulate rapidly.

4. **Why the State doesn't detect it** (`inspectorates.rs` line 277–290):
   - Detection requires `labor_inspection_capacity > 0` AND `effective_prob > 0.5`.
   - `effective_prob = detection_probability + turns_since_inspection × 0.05`.
   - `detection_probability = labor_inspection_capacity / num_labor_intensive_companies`.
   - If there are many shadow companies and few inspectorate buildings, `detection_probability` is very low.
   - It takes many turns for `turns_since_inspection × 0.05` to push `effective_prob` above 0.5.
   - **The inspectorate capacity is underfunded** because `allocate_cash_to_ministries` (ministries.rs line 537) scales allocations by `liquid_reserves / promised`. If the treasury is depleted (due to the economic crisis), inspectorates get almost no cash, can't produce inspection capacity, and can't detect shadow employment.

5. **Why corruption indices remain low (~0.05)**: The corruption index measures *bribery*, not shadow employment. Shadow employment is a separate mechanic. Companies paying below-market wages off-the-books doesn't trigger bribery — it's a labor violation, not a corruption act.

### 1.3 I=0 — Why Did Investment Collapse?

**Files:** `state/src/engine/turn.rs` (line 877), `state/src/corporate/strategy.rs`, `state/src/corporate/development.rs`

Investment (I) is accumulated at turn.rs line 877–881:
```rust
let investment: f64 = secured_trades.iter()
    .filter(|t| t.commodity.is_fixed_asset())
    .map(|t| t.quantity * t.execution_price)
    .sum();
task.gdp_acc.investment += investment;
```

**Only secured B2B trades in fixed-asset commodities count as investment.** Fixed assets are: `IndustrialMachinery`, `ConstructionMachinery`, `AgriculturalMachinery`, `OfficeMachinery`, `Trucks`, `Cars`, `DraftAnimals`.

**Why I=0:**
1. With freight costs 7× higher, the `freight_cost_reserve_ratio` (0.30) in the B2B encumbrance is insufficient. Buyers encumber `commodity_cost × (1 + 0.30)`, but actual freight costs are 7× the commodity cost, not 0.3×.
2. When `procure_freight_and_split_trades` runs, most trades fail with `UnaffordableFreight` and are deferred.
3. After `max_deferred_turns` (3), deferred trades are cancelled with bid refunds.
4. No fixed-asset trades are ever secured → `investment = 0`.
5. Additionally, `family_expansion` (strategy.rs line 638) requires `gross_profit > 0`. With freight costs eating all margins, `gross_profit < 0` → expansion is `Idle` → no new construction tenders.
6. The `publish_gas_station_tenders` function (development.rs line 530) requires `company.available_cash >= 10_000.0`. With companies bleeding cash to freight, this threshold is never met.

### 1.4 NX=0 — Why Is Global Trade Failing?

**Files:** `state/src/international/trade.rs`, `state/src/engine/turn.rs` (line 3603)

`balance_global_trade` (trade.rs line 108) works as follows:
1. Collects `global_supply` and `global_demand` from `market_orders_total_sell/buy(market_orders)`.
2. Allocates supply/demand to countries by competitiveness weights.
3. `global_volume = min(global_supply, adjusted_global_demand)`.
4. Each country's `trade_balance = actual_export - actual_import`.

**Why NX=0:**
- `market_orders` is built from `task.orders` (turn.rs line 3488), which is populated by the B2B order submission and production cycles.
- With freight costs making most trades unaffordable, companies submit fewer orders. But more critically, the **global market orders** come from per-country `task.orders` which include both domestic and international orders.
- The key issue: `import_demand` is capped by `liquid_reserves` (trade.rs line 193). With the economic crisis depleting treasuries, `liquid_reserves` → 0, so `import_demand` → 0.
- Similarly, `export_weight = country.budget.gdp × comp`. With GDP dropping 2–3%, export weight drops.
- If both `global_supply` and `adjusted_global_demand` are very low, `global_volume` → 0, and all `trade_balance` values → 0.
- **NX=0 is a symptom of the freight cost bug depleting treasuries and suppressing trade volume.**

### 1.5 Turn 1 Consistency Check — Are Generated Countries Structurally Doomed?

**File:** `state/src/engine/generator/mod.rs` (line 586), `state/src/engine/generator/corporate.rs` (line 1806)

**Initial treasury:** `liquid_reserves = gdp_total × rand(0.02..0.10)` — only 2–10% of GDP. This is thin.

**Freight capacity seed** (corporate.rs line 1806–1822): Transport companies get one turn of seeded `FreightCapacity` output. This is sufficient for Turn 1.

**Fuel seed**: The `estimated_base_price` for `Fuels` is 80.0 (corporate.rs line 1838). Fuel is seeded as an input for transport companies. However, the **amount** seeded is based on `method.inputs × production_scale`, which may be inadequate if the freight cost bug makes fuel consumption appear 7× higher.

**Structural doom assessment:**
- Countries are NOT structurally doomed on Turn 1 — the freight capacity seed handles the cold-start.
- The doom arrives on Turns 2–4 as the freight cost bug makes ongoing transport unaffordable, depleting company cash and treasury reserves.
- The `freight_cost_reserve_ratio = 0.30` is calibrated for pre-Phase-30 freight costs, not the 7× inflated costs.

---

## PART 2: Government "Crisis Management" AI

### 2.1 Current State — The Sleepwalker Government

**The government AI is almost entirely passive:**

1. **No automatic tax adjustment**: `TaxRateChange` laws exist (laws.rs line 347) but are never proposed. The `process_bill_lifecycle` function (bill_lifecycle.rs line 386) is a **stub** that outputs `"[LEGISLATION] No active legislative session"` and does nothing.

2. **No sovereign bond issuance**: `issue_treasury_securities` exists (debt_market.rs line 360) but is never called from the turn engine. When `liquid_reserves` hits 0, the government simply freezes procurement (`allocate_cash_to_ministries` allocates `available / promised × promised = 0`).

3. **No targeted subsidies**: The `MinistryAction::Subsidy` variant exists (ministries.rs line 231) but is never used for crisis response. Subsidies only appear in:
   - B2C services (healthcare, education, transport) — static, not crisis-responsive
   - Propaganda media subsidy — static
   - Commuting public transport subsidy — static

4. **No crisis detection**: The only "crisis" flag is `politics.budget_crisis` (system.rs line 810), which triggers snap elections but does NOT trigger any fiscal response.

5. **Tariff adjustment exists** (trade_policy.rs `adjust_tariffs_for_conditions`) but only adjusts tariffs, not taxes or spending.

### 2.2 Proposed Crisis Management AI

Create a new module: `state/src/politics/crisis_management.rs`

#### 2.2.1 Crisis Detection

```rust
pub struct CrisisIndicators {
    pub gdp_decline_pct: f64,           // GDP growth rate (negative = decline)
    pub shadow_gdp_ratio: f64,          // shadow_gdp / official_gdp
    pub treasury_coverage_months: f64,  // liquid_reserves / monthly_spending
    pub investment_collapse: bool,      // I == 0 for 2+ consecutive turns
    pub trade_collapse: bool,           // NX == 0 for 2+ consecutive turns
    pub sector_collapse: Vec<(Sector, f64)>, // sectors with PMI < 30
}

pub fn detect_crisis(country: &Country, turn: u32) -> CrisisIndicators
```

#### 2.2.2 Fiscal Policy Response (Tax Adjustment)

```rust
pub fn execute_fiscal_response(
    country: &mut Country,
    indicators: &CrisisIndicators,
    ideology: Ideology,
) -> Vec<String>
```

**Ideology-driven tax adjustments:**

| Ideology Group | GDP Decline | Shadow > 50% | Treasury < 1 month |
|---|---|---|---|
| **Left** (Marxist, SocDem, Green) | Raise PIT +2%, raise CIT +3% | Raise inspectorate budget | Issue sovereign bonds |
| **Liberal** (ClassLib, SocLib, Neolib) | Cut PIT -1%, cut CIT -2% | Reduce regulations (lower inspection) | Cut spending 10% |
| **Conservative** (SocCon, NeoCon, NatCon, ChristDem) | Hold rates, cut spending 5% | Hold rates, raise inspectorate budget | Issue bonds cautiously |
| **Radical** (AnCap, Fascism, Maoism) | Ideology-specific extreme response | | |

**Tax adjustment bounds:**
- PIT: 0%–60% (clamp)
- CIT: 0%–40% (clamp)
- VAT: 0%–25% (clamp)
- Maximum change per turn: ±3 percentage points

#### 2.2.3 Sovereign Bond Issuance (Strict Double-Entry)

When `treasury_coverage_months < 2.0` (treasury can't fund 2 months of spending):

```rust
pub fn issue_crisis_bonds(
    country: &mut Country,
    companies: &mut [Company],  // banks are Company entities
    amount_needed: f64,
    current_turn: u32,
) -> f64  // returns actual amount raised (may be < amount_needed if auction fails)
```

**CRITICAL FIX:** The existing `issue_treasury_securities` (debt_market.rs line 360) has a fatal flaw: it uses `country.budget.liquid_reserves` as `total_capacity` (line 407) — the government buying its own bonds with its own reserves. This is circular money printing. The crisis bond function must NOT use this broken path.

**New auction mechanism:**

1. **Identify eligible buyers** with real private-sector liquidity:
   - **Commercial/Universal banks**: `balance_sheet.reserves_at_central_bank` (excess reserves above reserve requirement). Use `excess_reserves()` (banking.rs line 348).
   - **Citizens**: Aggregate `class_demographics.savings` across all regions (same pattern as `clear_retail_savings_bond_window` at debt_market.rs line 492). Citizens allocate up to 5% of savings to bonds.

2. **Calculate total buyer capacity:**
   ```rust
   let bank_capacity: f64 = companies.iter()
       .filter(|c| c.bank_type.is_some())
       .map(|c| c.balance_sheet.as_ref().map(|bs| bs.excess_reserves(cb_reserve_ratio)).unwrap_or(0.0))
       .sum();
   let citizen_capacity: f64 = country.regions.iter()
       .flat_map(|r| r.class_demographics.rural_classes.values().chain(r.class_demographics.urban_classes.values()))
       .map(|cd| cd.savings * 0.05)  // citizens allocate up to 5% of savings
       .sum();
   let total_capacity = bank_capacity + citizen_capacity;
   ```

3. **If `total_capacity <= 0.0`, the auction FAILS.** The State gets no money. This is the correct behavior — if the private sector has no liquidity during a crisis, the government cannot borrow. The crisis response must then fall back to spending cuts only.

4. **If `total_capacity > 0.0`, issue bonds up to `min(amount_needed, total_capacity, ideology_cap)`:**
   - Left ideologies: cap at 15% of GDP.
   - Liberal ideologies: cap at 5% of GDP.
   - Conservative ideologies: cap at 8% of GDP.

5. **Double-entry settlement:**
   - Banks: `balance_sheet.reserves_at_central_bank -= purchase_amount`, `balance_sheet.securities += purchase_amount`. The bank's asset composition shifts from reserves to treasury securities.
   - Citizens: `class_demographics.savings -= purchase_amount`. Savings bond records created via existing `SavingsBond` mechanism.
   - Treasury: `country.budget.liquid_reserves += total_raised`. The government receives real private-sector cash.
   - **No money is created.** The same cash that was in bank reserves or citizen savings is now in the treasury. The private sector has less liquidity; the government has more.

6. **Bond records:** Use existing `TreasurySecurity` and `SavingsBond` structures. Bank purchases are recorded as `SecurityHolderType::CommercialBank` holders. Citizen purchases are recorded as `SavingsBond` entries.

**This prevents the "procurement freeze death spiral"** where no cash → no inspectorates → no shadow detection → more shadow economy → less tax revenue → even less cash — but ONLY if the private sector has liquidity. If the crisis is so severe that banks and citizens are also broke, the government must cut spending.

#### 2.2.4 Targeted Emergency Subsidies

When a vital sector has PMI < 30 for 2+ turns:

```rust
pub fn allocate_emergency_subsidies(
    country: &mut Country,
    companies: &mut [Company],
    indicators: &CrisisIndicators,
    ideology: Ideology,
) -> Vec<String>
```

- Identify companies in collapsing sectors.
- Subsidy amount = `company.fulfilled_fte × avg_wage × 0.5` (covers 50% of payroll).
- Funded from `liquid_reserves` (or bond proceeds if reserves are insufficient).
- Left ideologies: subsidize up to 80% of payroll, include agriculture + light industry.
- Liberal ideologies: subsidize up to 20%, only "strategic" sectors (energy, heavy industry).
- Conservative ideologies: subsidize up to 40%, focus on agriculture and traditional industries.
- Uses the existing `MinistryAction::Subsidy` mechanism (ministries.rs line 231).
- **Double-entry**: Debit `country.budget.liquid_reserves`, credit `company.available_cash` via `settle_transfer_to_treasury` in reverse.

#### 2.2.5 Integration Point — Executive Decrees

Called from `process_political_turn` (politics/turn.rs) as **Executive Decrees** — NO legislative voting. The ruling government acts directly:

```rust
// Phase 31: Crisis Management AI — Executive Decrees
// Bypass bill_lifecycle entirely. The ruling party's ideology determines
// the crisis response, applied directly by executive authority.
let crisis = crisis_management::detect_crisis(country, current_turn);
if crisis.is_crisis() {
    let ideology = ruling_ideology(country);
    // 1. Fiscal response: adjust PIT/CIT/VAT by executive decree
    let msgs = crisis_management::execute_fiscal_response(country, &crisis, ideology);
    messages.extend(msgs);
    // 2. Sovereign bond issuance: auction to private sector (may fail)
    let bond_msgs = crisis_management::issue_crisis_bonds_if_needed(
        country, companies, current_turn,
    );
    messages.extend(bond_msgs);
    // 3. Emergency subsidies: direct transfer to collapsing sectors
    let subsidy_msgs = crisis_management::allocate_emergency_subsidies(
        country, companies, &crisis, ideology
    );
    messages.extend(subsidy_msgs);
}
```

**No `Bill` objects are created. No committee/floor vote is attempted.** The `bill_lifecycle` stub remains untouched. This is a deliberate architectural decision: the legislative engine is a future phase, not this one.

---

## PART 3: Ideologies & Political Realism

### 3.1 Current State — Cosmetic Ideologies

Ideologies currently affect:
- **Budget priorities** (ministries.rs `budget_priorities`) — weights for ministry allocation
- **Social programs** (social_programs.rs line 311) — program type and amount multipliers
- **Trade doctrine** (trade_policy.rs) — tariff levels
- **Policy preferences** (ideology.rs `preferences`) — static policy strings (religion, citizenship, etc.)
- **Coalition formation** — compass distance for stability checks

**What's missing:** Ideologies do NOT affect:
- Tax rates (PIT, CIT, VAT)
- Crisis response strategy
- Sovereign debt policy
- Subsidy allocation during crises
- Inspectorate funding levels

### 3.2 Proposed Ideology-Crisis Integration

#### 3.2.1 Crisis Response Profile

Add to `state/src/politics/ideology.rs`:

```rust
pub struct CrisisResponseProfile {
    pub pit_adjustment: f64,          // % points to adjust PIT during crisis
    pub cit_adjustment: f64,          // % points to adjust CIT during crisis
    pub vat_adjustment: f64,          // % points to adjust VAT during crisis
    pub spending_cut_pct: f64,        // % of budget to cut during crisis
    pub bond_issuance_cap_gdp: f64,   // max sovereign bonds as % of GDP
    pub subsidy_pct_of_payroll: f64,  // emergency subsidy as % of payroll
    pub subsidized_sectors: Vec<Sector>, // sectors eligible for emergency subsidy
    pub inspectorate_priority: f64,   // 0.0–1.0, funding priority for inspectorates
}

impl Ideology {
    pub fn crisis_response(self) -> CrisisResponseProfile { ... }
}
```

**Profiles by ideology group:**

| Ideology | PIT | CIT | VAT | Spending Cut | Bond Cap | Subsidy % | Inspectorate |
|---|---|---|---|---|---|---|---|
| OrthodoxMarxism | +3% | +5% | +2% | 0% | 15% GDP | 80% | 1.0 |
| MarxismLeninism | +3% | +5% | +2% | 0% | 15% GDP | 80% | 1.0 |
| Maoism | +2% | +4% | +1% | 0% | 12% GDP | 70% | 0.9 |
| SocialDemocracy | +2% | +3% | +1% | 0% | 10% GDP | 60% | 0.8 |
| GreenPolitics | +1% | +2% | 0% | 5% (military) | 8% GDP | 50% | 0.7 |
| SocialLiberalism | +1% | +1% | 0% | 5% | 8% GDP | 40% | 0.6 |
| ChristianDemocracy | 0% | 0% | 0% | 5% | 8% GDP | 40% | 0.5 |
| Agrarianism | 0% | -1% | 0% | 5% (military) | 6% GDP | 50% (agri) | 0.4 |
| ClassicalLiberalism | -2% | -3% | -1% | 15% | 3% GDP | 10% | 0.2 |
| Neoliberalism | -2% | -3% | -1% | 20% | 3% GDP | 5% | 0.1 |
| SocialConservatism | 0% | 0% | 0% | 10% | 6% GDP | 30% | 0.4 |
| Neoconservatism | 0% | -1% | 0% | 10% (welfare) | 8% GDP | 20% (military) | 0.3 |
| NationalConservatism | +1% | 0% | +1% | 5% (welfare) | 8% GDP | 40% (domestic) | 0.5 |
| AnarchoCapitalism | -5% | -5% | -5% | 50% | 0% GDP | 0% | 0.0 |
| Fascism | +2% | +3% | +2% | 0% | 12% GDP | 60% (military) | 0.8 |

#### 3.2.2 Coalition Moderation (No Legislative Voting)

**Architectural correction:** The crisis response is an **Executive Decree**, NOT a legislative bill. There is no committee, floor vote, or amendment process. However, the ruling coalition's ideological composition still moderates the decree:

1. **Ruling party ideology** determines the base `CrisisResponseProfile`.
2. **Coalition moderation**: If the ruling party governs in coalition with partners who have a different ideology, the crisis response is moderated:
   - Compute the weighted-average ideology of the ruling coalition (weighted by seat count).
   - If the coalition's average ideology diverges from the ruling party's ideology by > 0.3 on the economic compass axis, tax adjustments are halved and bond cap is reduced by 30%.
   - This represents the practical reality that coalition partners constrain the ruling party's most extreme actions, even without a formal vote.
3. **Minority government**: If `politics.minority_government == true`, the crisis response is further weakened:
   - Subsidy amounts are reduced by 50% (the government can't spend boldly without parliamentary support).
   - Tax adjustments are capped at ±1% (instead of ±3%).
4. **Non-democratic regimes** (Autocracy, Dictatorship): No moderation. The executive applies the full crisis response profile unilaterally.

**No `Bill` objects are created. No `bill_lifecycle` processing is triggered.** The moderation is computed mathematically from coalition composition and applied directly to the executive decree.

---

## PART 4: "Algorithm Panic" Code Sweep

### 4.1 Identified Panic Behaviors

#### 4.1.1 Freight Procurement Panic (logistics.rs)

**Current behavior:** When a trade can't secure freight (UnaffordableFreight, NoFreightCapacity, ImpassableRoute), it is deferred. After 3 turns, it is cancelled with a bid refund.

**Problem:** The company doesn't know WHY freight failed. It re-submits the same order next turn, which fails again, creating a wasteful loop. The company doesn't:
- Reduce order quantity to fit available freight budget.
- Switch to a closer supplier.
- Scale down production to match transport capacity.

**Fix — Bounded Rationality Fallback:**
- Track per-company freight failure history.
- If a company has 3+ consecutive freight failures for the same commodity:
  1. Reduce order quantity by 30% (try to fit within freight budget).
  2. If still failing after 2 more turns, seek suppliers in the same region (distance = 0, no freight needed).
  3. If no local supplier exists, scale down production by 20% (reduce `target_fte_demand`).
  4. Log a "production curtailment" event for telemetry.

#### 4.1.2 Company Cash Panic (b2b_orders.rs)

**Current behavior:** When `available_cash` is insufficient for a bid, the company submits a partial bid or skips entirely (`continue` at line 243).

**Problem:** The company doesn't communicate the cash shortage to the production system. It keeps producing at full capacity, building up unsold inventory, and paying wages it can't afford.

**Fix — Production Scaling:**
- If a company's `available_cash < total_input_cost × 0.5` for 2+ consecutive turns:
  - Reduce `building.current_employment` by 20% (layoffs).
  - Reduce `target_fte_demand` by 20%.
  - This prevents the company from bleeding cash while producing goods it can't transport.

#### 4.1.3 Ministry Procurement Freeze (ministries.rs)

**Current behavior:** `allocate_cash_to_ministries` (line 537) scales allocations by `available / promised`. If `liquid_reserves = 0`, all ministries get 0 cash.

**Problem:** This is a death spiral — no cash → no procurement → no economic stimulus → no tax revenue → no cash. The government doesn't attempt to issue bonds or raise taxes.

**Fix — Crisis Bond Auto-Issuance:**
- Before `allocate_cash_to_ministries`, check if `liquid_reserves < promised × 0.5`.
- If so, call `issue_crisis_bonds` to raise `liquid_reserves` to at least `promised × 0.8`.
- This ensures ministries always have at least 80% of their promised budget.

#### 4.1.4 Shadow Employment Trigger Loop (legal_status.rs)

**Current behavior:** `trigger_shadow_employment` fires when `unmet_demand >= target_fte_demand × 0.5`. Once a company enters the shadow economy, it stays there indefinitely until caught by an inspectorate raid.

**Problem:** Even after the economic crisis resolves, companies remain in the shadow economy. There's no "legalization" path except being caught and fined.

**Fix — Voluntary Legalization:**
- If a company's `gross_profit > 0` for 3+ consecutive turns AND it has shadow employment:
  - 30% chance per turn to voluntarily legalize (remove `shadow_employment`).
  - This represents the company deciding it can afford legal wages again.
- If PIT rate is reduced below 10%, increase legalization probability to 50%.

#### 4.1.5 Bankruptcy Cascade (corporate/manager.rs)

**Current behavior:** `is_distressed` (strategy.rs line 633) triggers when `company_capital < 0` OR `(net_profit < 0 AND liquid_capital == 0)`. The restructure action lays off 50% of workers.

**Problem:** If many companies go distressed simultaneously (as in a freight cost crisis), mass layoffs crash consumer demand, which crashes more companies — a cascade.

**Fix — Gradual Distress:**
- Add a `distress_level: f64` field (0.0 = healthy, 1.0 = bankrupt).
- Each turn of `net_profit < 0` increases `distress_level` by 0.1.
- Each turn of `net_profit > 0` decreases `distress_level` by 0.15.
- Layoffs are proportional to `distress_level` (not 50% all at once).
- At `distress_level >= 1.0`, trigger full bankruptcy.
- This gives companies time to recover and prevents cascade.

#### 4.1.6 Global Trade Zero-Volume Loop (trade.rs)

**Current behavior:** If `global_supply = 0` or `adjusted_global_demand = 0`, `global_volume = 0` and all trade balances are 0.

**Problem:** There's no mechanism to stimulate trade when it collapses. The government doesn't notice or respond to NX=0.

**Fix — Trade Stimulation:**
- If `global_volume < 1% of total GDP` for 2+ consecutive turns:
  - Left ideologies: reduce tariffs, subsidize export industries.
  - Liberal ideologies: remove all tariffs, seek free-trade agreements.
  - Conservative ideologies: negotiate bilateral trade deals (improve diplomatic relations).
- This is handled by the Crisis Management AI (Part 2).

---

## PART 4.5: True Starvation & Emigration (User-Mandated Correction #3)

### Current State

**Existing emigration mechanics** (`state/src/economy/labor/migration.rs`):
- `calculate_migration_pressure()` (line 78): Computes pressure from unrest (40%), poverty (30%), wage level (20%), disasters (10%).
- `calculate_emigrants()` (line 131): `emigrants = population × pressure × MAX_EMIGRATION_RATE (0.02) × (1 - enforcement)`.
- `collect_migration_flows()` (line 171): Two-pass collect-then-apply. Emigrants flow from high-pressure to low-pressure countries.
- Called from turn.rs line 3167. Population is conserved (origin loses = destination gains).

**Existing rationing consequences** (`state/src/government/treasury.rs` line 98):
- `apply_rationing_consequences()`: If essential goods (Food, Coal, Energy, Medicine) are rationed, mortality increases (+5% to +35%) and unrest increases (+10 to +40).
- `increase_mortality_from_shortage()`: Updates `macro_indicators.demographics.death_rate`.

**Existing economic status** (`state/src/society/geography.rs` line 857):
- `EconomicStatus` enum: Prosperous, Stable, Struggling, Destitute.
- `update_class_demographics()` (line 1128): Sets economic status based on `savings_per_capita`.
- `Destitute` is set when `savings_per_capita < 100.0`.

**The Gap:**
- Migration pressure uses GDP per capita and average wage, but does NOT directly check unemployment rate or subsistence wage levels.
- Rationing consequences only fire when the rationing system is explicitly active. There is no automatic starvation when wages collapse below subsistence without formal rationing.
- `EconomicStatus::Destitute` is tracked but has NO demographic consequence — Destitute classes do not lose population.
- The `winter_mortality_multiplier` field exists on `Region` (geography.rs line 584) but is only used for winter-specific mortality, not economic starvation.

### Proposed Starvation & Emigration Enhancement

#### 4.5.1 Crisis-Driven Emigration

**File:** `state/src/economy/labor/migration.rs`

Enhance `calculate_migration_pressure()` to include unemployment and subsistence wage:

```rust
pub fn calculate_migration_pressure(
    country: &Country,
    buildings: &[Building],
    disaster_count: u32,
) -> f64 {
    // ... existing unrest, poverty, wage, disaster calculations ...

    // Phase 31: Add unemployment and subsistence pressure
    let unemployment_rate = compute_unemployment_rate(country); // new helper
    let unemployment_pressure = (unemployment_rate - 0.10).max(0.0).min(1.0); // >10% unemployment increases pressure

    let avg_wage = country.macro_indicators.average_wage;
    let subsistence_wage = compute_subsistence_wage(country); // new helper, based on food basket
    let subsistence_pressure = if avg_wage < subsistence_wage {
        (1.0 - avg_wage / subsistence_wage).min(1.0)
    } else {
        0.0
    };

    // Recalibrated weights (sum to 1.0)
    let pressure = 0.25 * unrest
        + 0.20 * poverty
        + 0.15 * wage_pressure
        + 0.05 * disaster_pressure
        + 0.15 * unemployment_pressure
        + 0.20 * subsistence_pressure;

    pressure.min(1.0)
}
```

Also increase `MAX_EMIGRATION_RATE` during severe crises (per-turn, scaled for 24 turns/year):
- Default: 0.02% of population per turn (~0.5% annual).
- If `unemployment > 40%` for 2+ consecutive turns: increase to 0.05% per turn (~1.2% annual).
- If `avg_wage < subsistence_wage × 0.5`: increase to 0.10% per turn (~2.4% annual, famine-level exodus).
- Hard cap: 0.10% per turn — never higher, even in extreme crises.

**Note:** The original `MAX_EMIGRATION_RATE = 0.02` (2% per turn) in the existing code is itself too high for 24 turns/year (48% annual). The crisis enhancement also corrects this baseline to 0.0002 (0.02% per turn).

#### 4.5.2 Starvation Mortality

**New function in** `state/src/government/treasury.rs` (or `state/src/society/geography.rs`):

```rust
/// Phase 31: Apply starvation mortality when wages fall below subsistence.
///
/// Called after B2C clearing and wage computation, before population updates.
///
/// # Rules
/// * If a class's `savings_per_capita < 0` AND `economic_status == Destitute`
///   for 2+ consecutive turns, population decreases.
/// * Mortality rate is proportional to the deficit: deeper deficit = higher mortality.
/// * Population reduction is applied to `ClassDemographics.population`.
/// * This is NOT emigration (people don't leave the country) — they die.
pub fn apply_starvation_mortality(country: &mut Country) {
    for region in &mut country.regions {
        for cd in region.class_demographics.rural_classes.values_mut()
            .chain(region.class_demographics.urban_classes.values_mut())
        {
            if cd.economic_status != EconomicStatus::Destitute {
                continue;
            }
            if cd.savings_per_capita >= 0.0 {
                continue;
            }
            // Destitute with negative savings: starvation
            // Per-turn rates scaled for 24 turns/year:
            //   0.001 = 0.1% per turn ≈ 2.4% annual (moderate famine)
            //   0.005 = 0.5% per turn ≈ 12% annual (severe famine)
            let deficit_ratio = (-cd.savings_per_capita / 100.0).min(1.0);
            let mortality_rate = 0.001 + deficit_ratio * 0.004; // 0.1%–0.5% of class population per turn
            let deaths = (cd.population as f64 * mortality_rate) as i64;
            cd.population = cd.population.saturating_sub(deaths);
            // Track for telemetry
            country.macro_indicators.demographics.death_rate =
                (country.macro_indicators.demographics.death_rate + mortality_rate * 100.0).min(100.0);
        }
    }
}
```

**Key rules:**
- Only `Destitute` classes with **negative** savings_per_capita are affected.
- Maximum mortality: 0.5% of the class per turn (at full deficit) — compounds to ~12% annual in severe famine.
- Minimum mortality: 0.1% per turn (any Destitute class with negative savings) — compounds to ~2.4% annual.
- Population reduction is real — `ClassDemographics.population` decreases.
- This is separate from emigration: starvation reduces population without adding it to another country.
- **Rates are per-turn, scaled for 24 turns/year.** The originally proposed 1%–6% per turn would yield 24%–144% annual mortality — mathematically apocalyptic.

#### 4.5.3 Integration

Called from the turn engine after B2C clearing and before migration:

```rust
// Phase 31: Starvation mortality (before migration, after B2C clearing)
tasks.par_iter_mut().for_each(|task| {
    crate::government::treasury::apply_starvation_mortality(task.ctx.country);
});

// Existing migration flow (line 3167) — now enhanced with unemployment/subsistence pressure
```

#### 4.5.4 Subsistence Wage Computation

```rust
/// Compute the subsistence wage: the minimum wage needed to afford the
/// food basket at current market prices.
///
/// Uses the `Commodity::Food` enum to look up the market price — never
/// hardcoded string keys (especially not Polish strings). Market prices
/// are keyed by `HashMap<Commodity, f64>` throughout the engine.
fn compute_subsistence_wage(
    country: &Country,
    market_prices: &HashMap<Commodity, f64>,
) -> f64 {
    // Look up the Food price using the Commodity enum, not a string key.
    let food_price = market_prices
        .get(&Commodity::Food)
        .copied()
        .unwrap_or(50.0); // fallback if no market price available
    // A worker needs ~200 units of food per year for subsistence.
    // Engine runs 24 turns/year, so per-turn subsistence = 200/24 ≈ 8.33 units.
    // Subsistence wage per turn = 8.33 × food_price.
    (200.0 / 24.0) * food_price
}
```

**Strict rules:**
- Use `Commodity::Food` enum for all food price lookups. Never use string keys like `"Zywnosc"` or `"Zywność"`.
- The codebase is strictly English-only. All commodity references use the `Commodity` enum.
- Market prices are `HashMap<Commodity, f64>` (see `CountryTurnCtx.market_prices` in `economy/mod.rs` line 132).
- `MarketHistory` also uses `HashMap<Commodity, f64>` for all price maps (`vwap_per_commodity`, `last_trade_price`, `global_base_prices`, `retail_vwap_per_commodity`).

---

## PART 5: Implementation Plan

### Step 1: Fix the Freight Cost Dimensional Bug (CRITICAL — fix first)

**File:** `state/src/economy/logistics/logistics.rs`

**Option A (Recommended): Separate fuel cost from friction multiplier.**

Change `FreightRoute` to carry an explicit `fuel_cost_per_km` field:
```rust
pub struct FreightRoute {
    pub distance_km: f64,
    pub friction_multiplier: f64,      // dimensionless (friction only)
    pub fuel_cost_per_km: f64,         // currency per km (NEW)
    pub uses_waterborne: bool,
    pub impassable: bool,
    pub path_segments: Vec<RouteSegment>,
}
```

Change `freight_cost()`:
```rust
pub fn freight_cost(route: &FreightRoute, quantity: f64, base_rate: f64) -> f64 {
    if route.is_local() || route.impassable { return 0.0; }
    let friction_cost = quantity * route.distance_km * route.friction_multiplier * base_rate;
    let fuel_cost = route.fuel_cost_per_km * route.distance_km;  // NOT multiplied by base_rate
    friction_cost + fuel_cost
}
```

Change Dijkstra to track friction and fuel separately:
- `dist` accumulates only friction (for pathfinding optimization).
- `path_fuel_cost` accumulates fuel cost along the path.
- `friction_multiplier = total_friction / total_distance` (dimensionless).
- `fuel_cost_per_km = total_fuel_cost / total_distance` (currency per km).

**Option B (Minimal change): Remove fuel cost from edge_weight.**

Revert `edge_weight()` to friction-only (pre-Phase-30 behavior) and add fuel cost as a separate post-route calculation. This is simpler but loses the ability to choose fuel-efficient routes during pathfinding.

**Recommendation:** Option A — it preserves the Phase 30 design intent (fuel-efficient routing) while fixing the dimensional bug.

### Step 2: Fix the freight_cost_reserve_ratio

**File:** `state/src/economy/config/b2b_config.rs`

After fixing Step 1, recalculate `freight_cost_reserve_ratio`:
- With the bug fixed, freight cost per ton-km ≈ 0.05 (friction) + 0.08 × 80 / quantity (fuel).
- For typical trade quantities (100+ tons), fuel cost per ton-km ≈ 0.064.
- Total freight cost ≈ (0.05 + 0.064) × distance = 0.114 × distance.
- Commodity cost ≈ quantity × price ≈ 100 × 100 = 10,000.
- Freight cost for 100km ≈ 0.114 × 100 × 100 = 1,140.
- Reserve ratio should be ≈ 1,140 / 10,000 = 0.114.
- **Set `freight_cost_reserve_ratio = 0.15`** (slightly conservative).

### Step 3: Create Crisis Management Module

**New file:** `state/src/politics/crisis_management.rs`

Implement:
- `CrisisIndicators` struct and `detect_crisis()` function.
- `CrisisResponseProfile` and `Ideology::crisis_response()` method.
- `execute_fiscal_response()` — adjust PIT/CIT/VAT.
- `issue_crisis_bonds_if_needed()` — call existing `issue_treasury_securities`.
- `allocate_emergency_subsidies()` — use existing `MinistryAction::Subsidy`.
- `CrisisIndicators::is_crisis()` — threshold-based crisis detection.

**Export from:** `state/src/politics/mod.rs`

### Step 4: Wire Crisis Management into the Turn Engine

**File:** `state/src/engine/turn.rs` (in the political turn section)

Call `detect_crisis` and `execute_fiscal_response` before ministry procurement.
Call `issue_crisis_bonds_if_needed` before `allocate_cash_to_ministries`.
Call `allocate_emergency_subsidies` after ministry procurement but before B2B settlement.

### Step 5: Add Ideology Crisis Response Profiles

**File:** `state/src/politics/ideology.rs`

Add `CrisisResponseProfile` struct and `Ideology::crisis_response()` method with the profiles from Part 3.2.1.

### Step 6: Coalition Moderation for Executive Decrees

**File:** `state/src/politics/crisis_management.rs`

Implement coalition moderation logic (NO legislative voting):
- Compute weighted-average ideology of the ruling coalition.
- If coalition divergence > 0.3 on economic axis, halve tax adjustments and reduce bond cap by 30%.
- If minority government, cap tax adjustments at ±1% and reduce subsidies by 50%.
- Non-democratic regimes: no moderation (full crisis profile applied).
- **Do NOT touch `bill_lifecycle.rs`.** The stub remains as-is.

### Step 7: Starvation & Emigration Enhancement

**File:** `state/src/economy/labor/migration.rs`
- Enhance `calculate_migration_pressure()` with unemployment and subsistence wage pressure.
- Increase `MAX_EMIGRATION_RATE` during severe crises (up to 8% for famine-level).

**File:** `state/src/government/treasury.rs` (or `state/src/society/geography.rs`)
- Add `apply_starvation_mortality()` function.
- Called after B2C clearing, before migration.
- Reduces `ClassDemographics.population` for Destitute classes with negative savings.

**File:** `state/src/engine/turn.rs`
- Call `apply_starvation_mortality` in the turn loop.

### Step 8: Bounded Rationality Fallbacks

**File:** `state/src/economy/logistics/logistics.rs`
- Add `freight_failure_history: HashMap<String, u32>` to `FreightLogisticsConfig` or country state.
- Track per-company freight failures.
- Reduce order quantity after 3 consecutive failures.
- Scale down production after 5 consecutive failures.

**File:** `state/src/economy/trade/b2b_orders.rs`
- Add cash shortage detection (track consecutive turns where `available_cash < total_input_cost × 0.5`).
- Reduce `target_fte_demand` after 2 consecutive shortage turns.

**File:** `state/src/economy/justice/legal_status.rs`
- Add voluntary legalization logic in `process_shadow_economy_turn`.
- If company `gross_profit > 0` for 3+ turns, 30% chance to legalize.

**File:** `state/src/corporate/strategy.rs`
- Add `distress_level: f64` to `Company` (or track via financial_history).
- Make layoffs gradual based on `distress_level`.

### Step 9: Tests

Add tests for:
- Freight cost dimensional correctness (fuel cost not multiplied by base_rate).
- Crisis detection thresholds.
- Ideology crisis response profiles (all 15 ideologies).
- Sovereign bond auction: succeeds when banks have excess reserves.
- Sovereign bond auction: FAILS when private sector has no liquidity (no money printing).
- Sovereign bond auction: citizen savings bond purchases are double-entry (savings decrease, reserves increase).
- Emergency subsidy allocation.
- Coalition moderation: tax adjustments halved when coalition is fractured.
- Minority government: tax adjustments capped at ±1%.
- Bounded rationality: order quantity reduction after freight failures.
- Voluntary shadow employment legalization.
- Gradual distress (no instant 50% layoffs).
- Starvation mortality: Destitute class with negative savings loses population.
- Starvation mortality: Stable class with positive savings does NOT lose population.
- Emigration pressure increases with unemployment > 10%.
- Emigration rate increases to 8% during famine-level wage collapse.

---

## Files to Modify

| File | Change |
|---|---|
| `state/src/economy/logistics/logistics.rs` | Fix freight cost dimensional bug; add bounded rationality fallbacks |
| `state/src/economy/config/b2b_config.rs` | Adjust `freight_cost_reserve_ratio` default |
| `state/src/politics/crisis_management.rs` | **NEW** — Crisis detection, executive decrees, bond auctions, subsidies |
| `state/src/politics/ideology.rs` | Add `CrisisResponseProfile` and `crisis_response()` |
| `state/src/politics/mod.rs` | Export crisis_management module |
| `state/src/engine/turn.rs` | Integrate crisis management + starvation into turn loop |
| `state/src/economy/finance/debt_market.rs` | Fix `issue_treasury_securities` to use real private-sector liquidity (not treasury's own reserves) |
| `state/src/economy/labor/migration.rs` | Enhance migration pressure with unemployment + subsistence wage |
| `state/src/government/treasury.rs` | Add `apply_starvation_mortality()` |
| `state/src/economy/justice/legal_status.rs` | Add voluntary legalization |
| `state/src/corporate/strategy.rs` | Add gradual distress mechanism |
| `state/src/entities/mod.rs` | Add `distress_level` field to `Company` (if needed) |

**NOT modified:** `state/src/politics/bill_lifecycle.rs` — the legislative stub remains untouched. Crisis responses are executive decrees.

---

## Verification

- [ ] `cargo build --lib` succeeds
- [ ] `cargo test --lib` — all existing tests pass
- [ ] New test: freight cost dimensional correctness (fuel cost not multiplied by base_rate)
- [ ] New test: crisis detection triggers on GDP decline + shadow ratio
- [ ] New test: ideology crisis response profiles (15 ideologies)
- [ ] New test: sovereign bond auction succeeds when banks have excess reserves
- [ ] New test: sovereign bond auction FAILS when private sector has no liquidity (no money printing)
- [ ] New test: sovereign bond double-entry (bank reserves decrease, treasury reserves increase)
- [ ] New test: emergency subsidy allocation to collapsing sector
- [ ] New test: coalition moderation — tax adjustments halved when coalition is fractured
- [ ] New test: minority government — tax adjustments capped at ±1%
- [ ] New test: starvation mortality — Destitute class with negative savings loses population (0.1%–0.5% per turn)
- [ ] New test: starvation mortality — Stable class with positive savings does NOT lose population
- [ ] New test: emigration pressure increases with unemployment > 10%
- [ ] New test: emigration rate increases during famine-level wage collapse (capped at 0.10% per turn)
- [ ] New test: subsistence wage uses `Commodity::Food` enum (not Polish string keys)
- [ ] New test: voluntary shadow legalization after profitability recovery
- [ ] New test: gradual distress (no instant 50% layoffs)
- [ ] New test: bounded rationality — order quantity reduction after freight failures
- [ ] Manual verification: run 10 turns and confirm:
  - Official GDP does not drop more than 5% after freight fix
  - Shadow GDP ratio stays below 50% with crisis AI active
  - Investment (I) > 0 after freight fix
  - Net exports (NX) > 0 after freight fix + trade stimulation
  - Treasury does not hit 0 (bond issuance prevents procurement freeze, IF private sector has liquidity)
  - Population decreases during severe crisis (starvation + emigration)
  - No money is printed — bond auctions fail when private sector is illiquid

---

## Risks/Considerations

1. **Freight cost fix may over-correct**: If fuel costs are removed from the pathfinding weight, routes may be chosen that are fuel-inefficient. Option A (separate fields) preserves fuel-aware routing while fixing the dimensional bug.

2. **Crisis AI may be too aggressive**: If tax adjustments are too large, they can worsen the recession. The ±3% per-turn bound and ideology-specific profiles mitigate this. Coalition moderation further constrains extreme responses.

3. **Bond auction failure during severe crises**: If the private sector is illiquid (banks have no excess reserves, citizens have no savings), the bond auction fails and the government gets no money. This is the correct behavior per the strict double-entry rule, but it means the government may be unable to respond to a crisis if the private sector is also broke. In that case, the only available response is spending cuts (especially for liberal ideologies).

4. **Executive decrees bypass democratic process**: By design, crisis responses are executive decrees without legislative voting. This is a pragmatic choice — the legislative engine doesn't exist yet. In a future phase, when parliament mechanics are built, crisis responses can be routed through the legislative process. The coalition moderation mechanism provides a partial check on executive power in the meantime.

5. **Starvation mortality tuning**: The 0.1%–0.5% per-turn rate (compounding to ~2.4%–12% annual) is calibrated for 24 turns/year. This is severe but not apocalyptic. The minimum threshold (Destitute + negative savings) ensures only truly impoverished classes are affected. Rates may need further tuning after empirical testing.

6. **Emigration rate correction**: The existing `MAX_EMIGRATION_RATE = 0.02` (2% per turn = 48% annual) is itself too high for 24 turns/year and must be corrected to 0.0002 (0.02% per turn = ~0.5% annual). The famine-level cap of 0.10% per turn (~2.4% annual) prevents death spirals while still representing a significant exodus. The `MIN_POPULATION = 100` floor prevents total depopulation.

7. **Gradual distress may slow down necessary bankruptcies**: If a company is truly insolvent, gradual distress delays the inevitable and wastes resources. The `distress_level >= 1.0` bankruptcy trigger ensures eventual resolution.

8. **Voluntary legalization may be unrealistic**: Some companies may never legalize voluntarily. The 30% chance per turn means ~95% legalize within 8 turns of profitability, which may be too fast. This can be tuned.
