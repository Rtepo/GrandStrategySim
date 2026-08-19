# Phase 42 — The "G" Component Black Hole, Tax Evasion, UI Overhaul & Genesis Stabilization

A comprehensive read-only audit tracing five critical systemic failures: (1) government spending vanishing from GDP, (2) all taxes except VAT collecting 0.00, (3) Finance/Parliament UI ignoring layout instructions, (4) party names with numeric suffixes and VIP clones persisting, (5) labor genesis crash and mining deposit disconnect, plus (6) dormant currency/FX reserves.

---

## PART 1: The Government Spending (G) Black Hole & 0.00 Taxes

### 1.1 Where is G Going? — Missing Accumulator Links

**Root cause confirmed:** Ministry spending flows through `execute_ministry_spending` (`state/src/politics/ministries.rs:760-894`) which debits `ministry.ministry_cash` and records `spending_actions`. But only **three** spending paths accumulate into `task.gdp_acc.government_spending`:

| Spending Action | File:Line | Accumulated into G? |
|---|---|---|
| B2B Procurement trades | `turn.rs:1099` | YES — `task.gdp_acc.government_spending += ministry_spend` |
| State Employer wages | `turn.rs:1968` | YES — `task.gdp_acc.government_spending += state_wages` |
| Church fund | `turn.rs:3639` | YES — `task.gdp_acc.government_spending += church_fund_result.total_paid` |
| **Subsidies** (Agriculture) | `ministries.rs:829-836` | **NO** — debits `ministry_cash`, credits `company.liquid_capital`, no G accumulation |
| **Infrastructure funding** | `ministries.rs:842-850` | **NO** — debits `ministry_cash`, no G accumulation |
| **Direct transfers** (Treasury, Justice, etc.) | `ministries.rs:884-892` | **NO** — debits `ministry_cash`, no G accumulation |
| **Public service wages** (Healthcare, Education) | `ministries.rs:858-867` | **INDIRECT** — goes to `ministry_public_service_pool`, then through State Employer → G |

**The black hole:** Subsidies, infrastructure funding, and direct transfers (the majority of ministry spending) are debited from `ministry_cash` and `spent_cash` is incremented, but **nothing is added to `gdp_acc.government_spending`**. The ~15M spent by ministries becomes ~600K in G because only the B2B procurement trades and state employer payroll flow into the accumulator.

**Proposed fix:**
1. In `execute_ministry_spending` (`ministries.rs`), return the total amount spent per call (subsidies + infrastructure + direct transfers).
2. In the turn loop where `execute_ministry_spending` is called, accumulate the returned total into `task.gdp_acc.government_spending`.
3. Alternatively, add `task.gdp_acc.add_government(&region.id, actual)` after each spending action in the turn loop.
4. For subsidies: the amount credited to `company.liquid_capital` is a transfer (G → I), so it should be recorded as G (government consumption) AND the company's subsequent spending becomes I (investment). This is standard GDP accounting.

**File to modify:** `state/src/politics/ministries.rs` — `execute_ministry_spending` (return total spent) and `state/src/engine/turn.rs` — caller (accumulate into G).

### 1.2 Why Are Taxes 0.00? — Triple Systemic Failure

#### 1.2.1 PIT: Average Wage Cascade to Zero

**File:** `state/src/state/tax.rs:1247-1288`

PIT is computed as:
```rust
let total_wages: f64 = buildings.iter()
    .map(|b| b.current_employment as f64 * avg_wage)
    .sum();
let pit_owed = total_wages * tax_rates.income_tax.rate;
```

Where `avg_wage = country.macro_indicators.average_wage`.

**The cascade:** At `turn.rs:1934-1949`:
```rust
let actual_avg_wage = if total_fulfilled > 0.0 {
    total_wages / total_fulfilled
} else {
    0.0  // ← THIS IS THE KILLER
};
task.ctx.country.macro_indicators.average_wage = actual_avg_wage;
```

If no workers are hired on turn 1 (because all companies start with `fulfilled_fte: 0.0` and the labor market fails to clear), `actual_avg_wage` is set to `0.0`. On the next tax turn, `total_wages = 0`, so `pit_owed = 0`, so `pit_collected = 0`.

**Even if workers ARE hired:** `buildings.iter().map(|b| b.current_employment)` uses building employment, but the labor market clears on `Company.fulfilled_fte`, not `Building.current_employment`. If `current_employment` is not updated after labor clearing, PIT remains 0 even with workers hired.

**Proposed fix:**
1. In the average wage computation (`turn.rs:1934`), if `total_fulfilled == 0`, fall back to the previous turn's `average_wage` (or the initial `gdp_pc * 800.0`) instead of 0.0. This prevents the cascade.
2. After labor market clearing, update `building.current_employment` from the company's `fulfilled_fte` so PIT sees real employment.
3. Alternatively, compute PIT from `company.offered_wage_per_fte * company.fulfilled_fte` (same source as the average wage calculation) instead of from building employment.

#### 1.2.2 CIT: Computed But Never Collected

**File:** `state/src/state/tax.rs:1290-1337`

CIT is computed per company:
```rust
let company_profit: f64 = buildings.iter()
    .filter(|b| b.owner_id == company.id)
    .map(|b| b.last_profit.max(0.0))
    .sum();
```

**Bug 1:** All buildings are generated with `last_profit: 0.0` (`generator/corporate.rs:197,275,661,1533,1821`). On turn 1, `last_profit` is 0 for all buildings, so CIT = 0. Even after production runs, `last_profit` may not be updated correctly.

**Bug 2 (critical):** The function signature takes `companies: &[Company]` (immutable slice). The CIT amount is computed and stored in `result.cit_collected`, but the money is **never actually deducted from companies**:
```rust
let actual_cit = cit_collected.min(company.available_cash);
// Note: we can't mutate companies here (immutable slice), so
// the caller must handle the deduction. We record the amount.
let _ = actual_cit; // Suppress unused variable warning
```

The `let _ = actual_cit;` line explicitly discards the amount. The tax routing at line 1448 credits `result.cit_collected` to the treasury, but the money was never debited from companies. This is **money creation** — the treasury gets credited with CIT that was never collected from anyone.

**Bug 3:** The caller in `turn.rs` does NOT handle the deduction (the comment says "the caller must handle the deduction" but no caller does).

**Proposed fix (Read-Only Tax Module — Strict Architectural Rule):**

> **STRICT RULE:** The `tax.rs` module must REMAIN read-only regarding entities. It must NOT accept `&mut [Company]`. Mutating companies inside `tax.rs` would risk a mutable borrowing disaster inside the Rayon parallel country loop where `task.companies` is already borrowed mutably for the task. Instead, `process_tax_collection_turn` calculates tax liabilities and returns a structured map of what each Company/Citizen owes. The main loop in `turn.rs` iterates over `companies` and `class_demographics` and physically applies the cash deduction (clamped by liquidity), recording the actual collections vs. evaded amounts.

1. `process_tax_collection_turn` stays read-only (`&[Company]`, `&[Building]`). It returns a `TaxCollectionResult` that now includes a `Vec<TaxLiability>` — one entry per company with `{ company_id, cit_owed, wealth_tax_owed, actual_cit, actual_wealth_tax }` (clamped to liquidity).
2. The caller in `turn.rs` iterates the `TaxLiability` list and physically debits `company.available_cash` / `brokerage_account.cash` for the actual amounts. It also debits `citizen_savings` for PIT (already done in tax.rs, but verify the clamped amount).
3. The caller accumulates the ACTUAL collected amounts (post-clamp) and routes only those to the treasury via `route_tax_collection_to_country`.
4. Ensure `building.last_profit` is updated after each production cycle.
5. The `TaxCollectionResult` records both theoretical and actual amounts so `taxes_evaded` is accurate.

#### 1.2.3 Wealth Tax: Same Immutable Slice Bug

**File:** `state/src/state/tax.rs:1346-1389`

Same issue as CIT: `companies` is `&[Company]` (immutable). The wealth tax is computed and `result.wealth_tax_collected` is recorded, but:
```rust
let actually_collected = wealth_collected.min(company_liquid);
result.wealth_tax_collected += actually_collected;
// Routing handles treasury credit
```

The comment says "Routing handles treasury credit" — and it does, at line 1448. But the money is never debited from companies. The treasury gets credited with wealth tax that was never collected. **Money creation.**

**Proposed fix:** Same read-only pattern as CIT — `process_tax_collection_turn` records the wealth tax liability in the returned `TaxLiability` list. The caller in `turn.rs` physically debits `company.available_cash` + `brokerage_account.cash` for the clamped amount and routes only the actual collected total to the treasury.

#### 1.2.4 Capital Gains Tax: Not Implemented

**File:** `state/src/state/tax.rs:1391-1394`

```rust
// ── Capital Gains Tax ───────────────────────────────────────────
// Intercepted before dividend distribution (simplified: no dividends this turn)
// This would be called during dividend processing, not here directly.
// Placeholder for future wiring.
```

Capital gains tax is a placeholder. `result.capital_gains_tax_collected` is always 0.0. The `calculate_capital_gains_tax` function exists (`tax.rs:1035`) but is never called from `process_tax_collection_turn`.

**Proposed fix:** Wire `calculate_capital_gains_tax` into the dividend distribution flow, or at minimum compute it from company dividend payments during the turn.

#### 1.2.5 VAT: Correctly Fixed in Phase 41

VAT is now transactional (Phase 41). Treasury is credited once during B2C clearing. `accumulated_vat` is read for reporting only. This is working correctly.

#### 1.2.6 Tax Routing: Credits Full Amount, Not Actual Collected

**File:** `state/src/state/tax.rs:1444-1455`

```rust
let total_collected = result.pit_collected + result.cit_collected + result.vat_collected
    + result.wealth_tax_collected + result.exit_tax_collected
    + result.customs_revenue + result.state_property_revenue;
route_tax_collection_to_country(total_collected, &tax_routing, country, ...);
```

**Bug:** `result.pit_collected` is the theoretical amount (before clamping to `citizen_savings`). `result.cit_collected` is the theoretical amount (before clamping to `company.available_cash`). The routing credits the FULL theoretical amount to the treasury, but only a fraction was actually debited from entities. This is **money creation**.

**Proposed fix:**
1. `process_tax_collection_turn` returns both theoretical and actual (clamped) amounts in `TaxCollectionResult`. The routing inside `tax.rs` is REMOVED — it must NOT credit the treasury directly.
2. The caller in `turn.rs` iterates the `TaxLiability` list, physically debits entities, accumulates the ACTUAL collected total, and THEN calls `route_tax_collection_to_country` with only the actual collected amount.
3. Record the difference between theoretical and actual as `taxes_evaded` (uncollectable due to illiquidity).
4. This ensures perfect double-entry: every dollar credited to the treasury was first debited from an entity.

---

## PART 2: UI Overhaul Enforcement (Finance & Parliament)

### 2.1 Finance Tab — Layout Issues

**File:** `state/src/ui/tui/render.rs:893-1126`

**Current state:** The Finance tab uses a 3-column layout (Item, Value, Detail). The Central Bank section already uses the Detail column for Lombard/Discount/Rediscount/Deposit rates. However:

**Issue 1 — DSPW count disappeared:** The `dspw_bank_count` field exists in `FinanceSnapshot` (line 312) and is populated in the snapshot builder (line 790). But in `render_finance`, the DSPW row was modified in Phase 41 to show "Active" or "NONE — check persistence" in the Detail column. The count itself is in the Value column. This should be visible. If it's showing 0, it's because `is_dspw` is being lost on save/reload — but Phase 41 fixed `CompanyDef` persistence for `is_dspw`. Need to verify the fix is working.

**Issue 2 — Total Outstanding Debt ignores CB holdings:** `total_public_debt = country.debt_market.total_outstanding_debt` (snapshot.rs:801). This is computed as `wholesale + retail` (debt_market.rs:331). The wholesale portion sums `outstanding_securities` holder quantities. The CB's `omo_bond_holdings` is tracked separately (snapshot.rs:805). If CB bonds are NOT included in `outstanding_securities` holders, then `total_public_debt` understates the true debt. Need to check if OMO purchases create entries in `outstanding_securities` or only in `cb.omo_bond_holdings`.

**Proposed fix for debt:** Display `total_public_debt + debt_held_by_central_bank` as the true total outstanding, OR verify that OMO purchases update `outstanding_securities` holders. If they don't, add CB holdings to the displayed total.

**Issue 3 — FX Reserves not shown as basket:** `cb_fx_reserves_total` is shown as a single sum (`cb.fx_reserves.values().sum()`). The user wants the top 3 currencies by value displayed. The `fx_reserves` field is a `HashMap<String, f64>` keyed by currency code.

**Proposed fix:** Add a new snapshot field `cb_fx_basket: Vec<(String, f64)>` (top 3 currencies by value). Display them in the Detail column: "USD: 12M | EUR: 8M | GBP: 3M".

**Issue 4 — CB/Banking layout:** The user wants CB and Banking sections to use the Detail column more aggressively. The current layout already does this for CB rates. For Banking, Phase 41 added reserve ratio and LDR to the Detail column. This is partially done but could be improved by moving more CB diagnostics (OMO history, liquidity injected) into the Detail column.

### 2.2 Parliament Tab — VIPs Still Present, Committees Missing

**File:** `state/src/ui/tui/render.rs:586-716`

**Current state:** Phase 41 moved VIPs from `ParliamentSnapshot` to `GovernmentSnapshot` and removed the VIP section from `render_parliament`. However, the user reports VIPs are still in the Parliament tab. This could be because:
1. The Phase 41 change set `parl.vips` to `Vec::new()` but didn't remove the `vips` field from `ParliamentSnapshot`. If the render code still checks `!parl.vips.is_empty()`, it won't render them (since the vec is empty). But if there's a fallback or the field is populated elsewhere, they could reappear.
2. The render code was updated to remove the VIP section, but there might be another render path.

**Proposed fix:** Remove the `vips` field from `ParliamentSnapshot` entirely. Verify no other code path populates it.

**Committees not displayed:** The `CommitteeSystem` exists in `country.politics.committee_system` (an `Option<CommitteeSystem>`) with `Committee` structs containing `name`, `chair`, `members` (HashMap of party → seat count), and `bills_under_review`. This data is NOT surfaced in the Parliament snapshot.

**Proposed fix:**
1. Add `CommitteeRow` struct to snapshot.rs: `{ name, chair, member_count, bills_under_review }`.
2. Add `committees: Vec<CommitteeRow>` to `ParliamentSnapshot`.
3. In `build_parliament_snapshot`, read from `country.politics.committee_system` and populate the committee rows.
4. In `render_parliament`, add a "Active Committees" section showing name, chair, and member count.

**Political Capital always 0.0:**

**File:** `state/src/politics/turn.rs:665`
```rust
country.politics.political_capital = 50.0 + ruling_support * 0.5 * coalition_stability;
```
This runs in `process_political_year` (once per year).

**File:** `state/src/engine/turn.rs:4887`
```rust
country.politics.political_capital = (country.politics.political_capital - 20.0 / 24.0).max(0.0);
```
This runs EVERY turn when parliament payroll fails.

**The cascade:** If the treasury is chronically broke (because taxes collect 0.00 — see Part 1), parliament payroll fails every turn. Political capital drops by ~0.83/turn. After 24 turns without a yearly reset, it's at 0. With the yearly reset, it goes to ~75, then drops by ~0.83 × 24 = ~20 over the year, ending at ~55. But if `process_political_year` doesn't run (e.g., provisional government, no parliament), it stays at 0 forever.

**Proposed fix:**
1. Fix the tax collection (Part 1) so the treasury has money to pay parliament payroll.
2. Add a minimum political capital floor (e.g., 10.0) so the government can still function even when broke.
3. Add a per-turn regeneration of `+1.0` political capital (representing baseline institutional support) independent of the yearly reset.
4. Display political capital in the Parliament tab (currently it's in the Government tab).

---

## PART 3: Name Generators (No More Numbers) & VIP Clones

### 3.1 Party Names — Numeric Suffixes

**File:** `state/src/politics/turn.rs:784-786`

```rust
if new_parties.contains_key(&name) {
    party.id = format!("[PRT-{}]", new_parties.len() + 1);
    new_parties.insert(format!("{} {}", name, new_parties.len() + 1), party);
} else {
    new_parties.insert(name, party);
}
```

**Root cause:** When a party name collision occurs (same generated name already exists), the code appends a number: `"Eldorian League 7"`. This is the exact source of the numeric suffixes.

**Proposed fix (Clean Party Names — Strict Architectural Rule):**

> **STRICT RULE:** Do NOT chain multiple fallback qualifiers to create absurd names like "New True Free League". The generator must randomly combine `[Country Adjective]` + `[Ideological Adjective]` + `[Noun]` in a single pass, and strictly use a `HashSet` to reject duplicates during generation. If a collision occurs, redraw the FULL name from the expanded pool — do not append qualifiers to an existing name.

1. Expand the cultural pattern pools in `generator.rs`: more prefixes, more nouns, more themes, more ideological adjectives. Double the pool sizes so collision probability drops.
2. The `generate_party_name` function already combines `[Country Adjective]?` + `[Prefix]?` + `[Noun]` + `[Theme]?` + `[Modifier]?`. Ensure each component is drawn from an expanded pool.
3. In `politics/turn.rs:784`, replace the numeric suffix collision handler with a redraw loop:
   ```rust
   let mut name = generator::generate_party_name(&country.name, cultural_group, ideo, &mut rng);
   let mut attempts = 0;
   while new_parties.contains_key(&name) && attempts < 20 {
       name = generator::generate_party_name(&country.name, cultural_group, ideo, &mut rng);
       attempts += 1;
   }
   // If still colliding after 20 attempts (extremely unlikely with expanded pools),
   // skip this ideology — it's already represented.
   if new_parties.contains_key(&name) { continue; }
   party.id = format!("[PRT-{}]", ideo.as_str());
   new_parties.insert(name, party);
   ```
4. NEVER append a bare number or a chained qualifier. The name is either accepted (unique) or redrawn (full regeneration from the pool).

### 3.2 VIP Clones — HashSet Not Shared Across Functions

**File:** `state/src/politics/ministries.rs:418-490` and `state/src/politics/parliament.rs:499-580`

**Root cause confirmed:** The `used_names` HashSet in `form_government` is **separate** from the `used_names` HashSet in `build_vips`. A minister named "Jan Kowalski" generated in `form_government` will NOT be deduplicated against the VIP list generated by `build_vips`, and vice versa.

Additionally, within `form_government` itself:
- The coalition branch creates `used_names` at line 419.
- The single-party branch creates a **different** `used_names` at line 465.
- These are in mutually exclusive `if/else` branches, so only one runs per formation — this is OK.
- BUT: party leader names are inserted into `used_names` only when the leader is used (line 437). If two parties have leaders with the same name (possible if names were generated independently), both would be used without collision detection.

**The real fix:**
1. Create a **shared** `HashSet<String>` that persists across both `form_government` and `build_vips` calls.
2. Store it on `Country` as a transient field (e.g., `country.politics.used_vip_names`) that is populated during government formation and read by `build_vips`.
3. Alternatively, have `form_government` return the `used_names` set, and pass it to `build_vips`.
4. Also: when resolving party leader names, check them against `used_names` before using them. If a leader name collides, generate a new VIP for that minister instead.

**Name pool size:** Current pools have 5000-6400 combinations per culture (50 first names × 50-64 surnames, doubled for gender). This is adequate for 15-20 ministries. The issue is NOT pool size — it's the lack of cross-function deduplication. However, to be safe, doubling the pools to 100 names each would reduce collision probability from ~3% to ~0.7% for 20 draws.

---

## PART 4: Labor Genesis Crash & Mining Disconnect

### 4.1 Labor Genesis — 60%+ Unemployment on Turn 1

**File:** `state/src/engine/generator/corporate.rs` — all company generators

**Root cause:** Every company is generated with:
```rust
fulfilled_fte: 0.0,
prev_fulfilled_fte: 0.0,
```

This means at game start, NO companies have any workers. The labor market clearing in `labor_market.rs` must fill ALL positions from scratch. But:

1. Companies start with `offered_wage_per_fte` computed from cash (`company_liquid * 0.6 / capacity`), which may be very low.
2. The labor market clearing uses `max_affordable_fte = cash / wage` — if cash is low and wage is low, companies can afford few workers.
3. With `fulfilled_fte = 0` for all companies, `actual_avg_wage = 0.0` after turn 1 (turn.rs:1934-1937), which cascades to PIT = 0 (Part 1.2.1).
4. Workers have no wages → no savings → no B2C consumption → no GDP → no tax revenue → treasury collapse.

**Proposed fix (with Genesis Payroll Grant — Strict Rule):**

> **STRICT RULE:** If you artificially assign a massive workforce to a newly generated company, it will instantly run out of cash on Turn 1 to pay their wages, plunging immediately into Wage Arrears and collapsing anyway. You MUST simultaneously inject a "Genesis Payroll Grant" into the company's `available_cash` (or `brokerage_account.cash`) during generation. This grant must be sufficient to cover at least 3 turns of wages for that specific initial `fulfilled_fte` at the starting `offered_wage_per_fte`.

1. In each company generator, set `fulfilled_fte` to a fraction of `target_fte_demand` (e.g., 50-70% of capacity) to simulate pre-existing employment.
2. Set `prev_fulfilled_fte` to the same value so sticky wage floors work on turn 1.
3. Set `building.current_employment` to match `fulfilled_fte` so PIT sees real employment on turn 1.
4. **Genesis Payroll Grant:** Compute `payroll_grant = fulfilled_fte * offered_wage_per_fte * 3.0` (3 turns of wages). Add this to the company's `available_cash` during generation. This ensures the company can pay its initial workforce for at least 3 turns before needing revenue.
5. This represents the existing economy at game start — companies don't start with zero workers or zero payroll in reality.
6. Ensure the initial `fulfilled_fte` is consistent with the company's cash (don't set it higher than `(cash + payroll_grant) / wage`).

**Estimated impact:** This single fix would prevent the average_wage → 0 cascade, restore PIT revenue, and stabilize the early game economy — without causing immediate wage arrears.

### 4.2 Mining Deposit Disconnect

**File:** `state/src/engine/generator/corporate.rs:1225-1291` — `seed_geology_based_mines`

**Current state:** The generator correctly links mining buildings to deposits:
```rust
building.deposit_id = Some(deposit_id.clone());  // deposit_id = format!("{}/{}", formation.id, key)
```

The snapshot (`snapshot.rs:555`) also uses the correct format:
```rust
let full_id = format!("{}/{}", formation_id, dep_id);
```

**Possible issue:** The buildings passed to `build_country_snapshot` come from `buildings_by_country` (snapshot.rs:1260), which is a `BTreeMap<String, Vec<Building>>`. If mining buildings are not included in this map (e.g., they're stored in a different building list or filtered out), the deposit miner count will be 0.

**Debugging needed:**
1. Check how `buildings_by_country` is populated in the caller. Are mining buildings included?
2. Check if `building.deposit_id` survives save/reload (it should — it's a serialized field on `Building`).
3. Check if the `resource_deposits` key (`dep_id`) in the snapshot matches the key used in the generator. The generator iterates `formation.resource_deposits` and uses the key directly. The snapshot also iterates `formation.resource_deposits` and uses the key. These should match.

**Alternative cause:** If the geological formations are regenerated on load (different IDs), the building's `deposit_id` (stored as `formation.id/key`) would no longer match the new formation IDs. Check if formation IDs are deterministic across save/reload.

**Proposed fix:**
1. Add debug logging in the snapshot builder to print `building.deposit_id` and `full_id` for mining buildings.
2. Verify that `buildings_by_country` includes ALL buildings (not just commercial/residential).
3. If formation IDs are non-deterministic, ensure they are persisted and not regenerated on load.

---

## PART 5: Currencies & FX Reserves Concept

### 5.1 Current State — Dormant Currency System

**File:** `state/data/currencies.json`

17 currencies exist with exchange rates (e.g., ILI: 1.05, ELD: 4.44, ANG: 0.60). Each has a "Fluid" policy regime with target 0.0.

**File:** `state/src/economy/trade/b2b_orders.rs`

The `settle_trades_with_tariffs` function handles cross-border trade settlement but does **NOT** use exchange rates. Trade values are computed as `trade.quantity * trade.execution_price` with no currency conversion. Tariffs are applied to the raw trade value.

**File:** `state/src/state/gold.rs:303-308`

FX reserves are only modified during gold trading:
```rust
*fx_reserves.entry(order.payment_currency.clone()).or_insert(0.0) += payment_amount;
```

**There is no mechanism for the Central Bank to accumulate FX reserves from export revenues.** The `cb.fx_reserves` HashMap starts empty and only grows through gold trading.

### 5.2 How Exchange Rates Should Affect Trade

Currently, exchange rates are defined but never used. In a proper implementation:

1. **Cross-border B2B trades:** When a buyer in country A (currency ILI) buys from a seller in country B (currency ELD), the trade value should be converted: `buyer_cost = trade_value * (seller_currency_rate / buyer_currency_rate)`. This makes imports from strong-currency countries more expensive.

2. **Export revenue → FX reserves (Forced Currency Conversion):** When a domestic company exports, the foreign buyer pays in Foreign Currency. The domestic company must immediately exchange 100% of these foreign earnings at the Central Bank at the official exchange rate. The CB credits the domestic company with domestic cash (expanding M0/reserves) and physically adds the Foreign Currency to the CB's `fx_reserves` basket. This maintains perfect double-entry integrity.

3. **Exchange rate impact on GDP:** Net exports (NX) should be reported in domestic currency, with the exchange rate affecting the conversion.

### 5.3 Proposed Fix Plan (Forced Currency Conversion — Strict Double-Entry Rule)

> **STRICT RULE:** The Central Bank cannot magically skim 5% of export revenue. Instead, implement a mandatory "Forced Currency Conversion" regime for exporters. When a domestic company exports, the buyer pays in Foreign Currency. The domestic company must immediately exchange 100% of these foreign earnings at the Central Bank at the official exchange rate. The CB credits the domestic company's domestic cash (expanding M0/reserves) and physically adds the Foreign Currency to `cb.fx_reserves`. This maintains perfect double-entry integrity — no money is created or destroyed.

**Step 1 — Forced Currency Conversion on Exports:**
- In `settle_trades_with_tariffs` (`b2b_orders.rs`), after settling a cross-border trade where the **seller is domestic** (export):
  - The foreign buyer pays `trade_value` in the **buyer's currency** (foreign currency).
  - The domestic seller does NOT receive foreign currency directly. Instead, the full foreign currency amount flows to the Central Bank: `cb.fx_reserves[buyer_currency] += trade_value_in_buyer_currency`.
  - The Central Bank credits the domestic seller with domestic cash at the official exchange rate: `seller_credit = trade_value_in_buyer_currency / foreign_rate * domestic_rate`.
  - Debit: CB gains foreign currency (asset). Credit: CB creates domestic currency (liability) to pay the seller. The seller's `available_cash += seller_credit`.
  - Double-entry check: CB FX reserves (asset) increase by foreign amount; CB domestic liabilities (M0) increase by domestic amount. Seller's cash increases by domestic amount. No money created or destroyed — the CB has issued domestic currency backed by foreign currency reserves.

**Step 2 — Exchange Rate in Import Valuation (with FX Reserve Hard Floor):**

> **STRICT RULE:** A Central Bank CANNOT print foreign currency. `fx_reserves` cannot go negative. When settling an import trade, the CB must check if it has enough of the specific `seller_currency`. If `cb.fx_reserves.get(seller_currency).unwrap_or(0.0) < required_amount`, the B2B trade MUST FAIL immediately due to a "Lack of Foreign Exchange". This introduces a highly realistic Balance of Payments constraint — countries must successfully export to earn the foreign currency needed to import goods.

- When a domestic company imports (buyer is domestic, seller is foreign), the buyer must pay in the seller's foreign currency. The domestic buyer pays domestic currency to the Central Bank, which converts it and debits its FX reserves:
  - **Pre-check:** `required_fx = trade_value_in_seller_currency`. If `cb.fx_reserves.get(seller_currency).unwrap_or(0.0) < required_fx`, the trade FAILS. The buyer's encumbered cash is released. A telemetry message is logged: `"BOP CRISIS: Import trade failed — insufficient {seller_currency} reserves"`. The buyer must find a domestic alternative or wait until the CB accumulates enough FX through exports.
  - **If FX is sufficient:** `cb.fx_reserves[seller_currency] -= trade_value_in_seller_currency`.
  - `buyer_debit = trade_value_in_seller_currency / foreign_rate * domestic_rate`.
  - Debit: Buyer's cash decreases by domestic amount. Credit: CB FX reserves decrease (asset sold), CB domestic liabilities decrease (currency withdrawn). Double-entry intact.
  - **Invariant:** `cb.fx_reserves[currency] >= 0.0` for all currencies at all times. No negative reserves. No printing foreign currency.

**Step 3 — Display FX Basket in Finance Tab:**
- Add `cb_fx_basket: Vec<(String, f64)>` to `FinanceSnapshot` — top 3 currencies by value.
- Display in the Detail column: "TOP3: ELD 12M | ANG 8M | IBE 3M".

**Step 4 — Exchange Rate Dynamics (Optional):**
- Exchange rates should drift based on trade balance: countries with trade surpluses see their currency appreciate, deficits depreciate.
- The Central Bank can intervene by buying/selling FX reserves to maintain a target rate.

---

## Implementation Priority Order

1. **CRITICAL — Labor Genesis (Part 4.1):** Set initial `fulfilled_fte` to 50-70% of capacity AND inject a Genesis Payroll Grant (3 turns of wages) into `available_cash`. This prevents the average_wage → 0 cascade that kills PIT, B2C consumption, and the entire economy — without causing immediate wage arrears.

2. **CRITICAL — Tax Collection (Part 1.2):** Keep `process_tax_collection_turn` read-only (`&[Company]`). Have it return a `TaxLiability` list. The caller in `turn.rs` physically debits companies and routes only actual collected amounts to the treasury. Fix the average_wage fallback to prevent PIT = 0.

3. **CRITICAL — G Component (Part 1.1):** Add `gdp_acc.government_spending` accumulation for subsidies, infrastructure, and direct transfers. This restores the G component of GDP.

4. **HIGH — Party Name Suffixes (Part 3.1):** Replace numeric suffix with full-name redraw from expanded pools using a `HashSet` for deduplication. No chained qualifiers.

5. **HIGH — VIP Clone Fix (Part 3.2):** Share `used_names` HashSet across `form_government` and `build_vips`.

6. **HIGH — Parliament Tab (Part 2.2):** Remove VIPs, add committees, display political capital.

7. **MEDIUM — Finance Tab (Part 2.1):** Fix debt total to include CB holdings, add FX basket display, verify DSPW count.

8. **MEDIUM — Mining Deposit Display (Part 4.2):** Debug why active_miners = 0 despite correct deposit_id format.

9. **MEDIUM — Currency/FX System (Part 5):** Implement Forced Currency Conversion for cross-border trades — 100% of export earnings converted at CB, FX reserves accumulate as CB assets. Import trades FAIL if CB lacks sufficient foreign currency (Balance of Payments constraint). No magical skimming, no negative reserves.

10. **LOW — Political Capital (Part 2.2):** Add per-turn regeneration and minimum floor.

---

## Files to Modify

| File | Changes |
|---|---|
| `state/src/engine/generator/corporate.rs` | Set initial `fulfilled_fte` to 50-70% of capacity AND inject Genesis Payroll Grant (3 turns of wages) into `available_cash` for all company generators |
| `state/src/state/tax.rs` | Keep read-only (`&[Company]`). Return `TaxLiability` list with theoretical + clamped amounts. Remove internal treasury routing. Fix average_wage fallback. |
| `state/src/engine/turn.rs` | Iterate `TaxLiability` list, physically debit companies, route only actual collected to treasury. Accumulate ministry subsidies/infrastructure/transfers into G. Update `building.current_employment` after labor clearing. Fix average_wage fallback. |
| `state/src/politics/ministries.rs` | Return total spent from `execute_ministry_spending` for G accumulation |
| `state/src/politics/turn.rs` | Replace numeric party name suffix with full-name redraw from expanded pools using `HashSet` deduplication |
| `state/src/politics/names.rs` | Expand name pools (double size) to reduce collision probability |
| `state/src/politics/parliament.rs` | Share `used_names` across `form_government` and `build_vips` |
| `state/src/ui/snapshot.rs` | Add `CommitteeRow` and `committees` to `ParliamentSnapshot`, add `cb_fx_basket` to `FinanceSnapshot`, fix `total_public_debt` to include CB holdings |
| `state/src/ui/tui/render.rs` | Remove VIPs from Parliament tab, add committee section, add FX basket to Finance tab, verify DSPW count display |
| `state/src/economy/trade/b2b_orders.rs` | Implement Forced Currency Conversion: 100% of export earnings converted at CB, FX reserves accumulate as CB assets. Import payments debit FX reserves — trade FAILS if CB lacks sufficient foreign currency (BOP constraint). No negative reserves. |

---

## Verification Plan

1. **Labor Genesis:** Start a new game, verify turn 1 unemployment < 30% (was 60%+). Verify no wage arrears on turn 1 (Genesis Payroll Grant covers initial payroll).
2. **Tax Collection:** Run 7 turns, verify PIT > 0, CIT > 0, Wealth Tax > 0 in Finance tab. Verify treasury cash increases match actual debits from companies.
3. **G Component:** Run 7 turns, verify G component of GDP is proportional to total ministry spending (not just procurement).
4. **Party Names:** Generate 20 parties, verify no numeric suffixes and no absurd chained qualifiers.
5. **VIP Clones:** Form 10 governments, verify no duplicate names across ministries and VIP lists.
6. **Parliament Tab:** Verify no VIPs, committees displayed with chair and member count.
7. **Finance Tab:** Verify DSPW count > 0, debt total includes CB holdings, FX basket shows top 3.
8. **Mining:** Verify active_miners > 0 for deposits that have mining buildings.
9. **FX Reserves:** Verify export trades credit CB FX reserves. Verify import trades FAIL when CB lacks sufficient foreign currency (BOP crisis). Verify `fx_reserves[currency] >= 0.0` invariant holds at all times.
10. **Build & Test:** `cargo build` succeeds, `cargo test --lib -- --test-threads=1` all pass.
