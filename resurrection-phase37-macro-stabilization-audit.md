# Phase 37 Audit: Macro Stabilization, Labor Frictions, Investment Deadlock & Deep UI Overhaul

**Summary:** A read-only audit of the codebase revealing five root causes: (1) unbounded corporate hiring/firing with no frictions or severance, (2) a frozen construction supply chain where producers can't sell and contractors can't buy, (3) a VIP-cloning bug in single-party government formation, (4) DSPW primary dealers never linked to the DebtMarket, and (5) multiple UI defects including duplicate headers, missing tax breakdown, truncated GDP/capita, and a single-megaregion dump.

---

## PART 1: Labor Market Hyper-Volatility & Deflationary Spiral

### 1.1 Root Cause Analysis

**The volatility source is NOT `target_fte_demand` swings — it's `fulfilled_fte` swings driven by cash-based labor clearing.**

The labor market clearing in `resolve_regional_labor_market` (labor_market.rs:161) clamps each company's bid to `max_affordable_fte = cash / offered_wage_per_fte`. A company that had a profitable turn can suddenly afford 2x more workers; a company that lost cash can suddenly afford 0. There are **no hiring frictions, no severance costs, and no per-turn hiring caps**.

Additionally, `CorporateAction::Restructure` (strategy.rs:499-514) can lay off up to **50% of `worker_capacity`** in a single turn:
```rust
let layoffs = match self {
    LegalForm::Cooperative(_) => ctx.company.worker_capacity / 4,
    _ => ctx.company.worker_capacity / 2,  // 50% instant layoff!
};
```

And `CorporateAction::Expand` (strategy.rs:644) can add unlimited `new_workers` based on `gross_profit / 1000`.

### 1.2 Implementation Plan: Hiring/Firing Frictions

**File:** `state/src/economy/labor/labor_market.rs`

1. **Add per-turn hiring cap:** In `resolve_regional_labor_market`, before submitting a bid, clamp `target_fte_demand` to `previous_fte * (1 + MAX_HIRING_GROWTH_RATE)`. Use a constant `MAX_HIRING_GROWTH_RATE = 0.15` (15% per turn). This requires tracking `previous_fte` on the Company struct (new field `prev_fulfilled_fte: f64`, updated at end of labor clearing).

2. **Add severance pay:** When `fulfilled_fte < prev_fulfilled_fte`, the company must pay severance = `(prev_fulfilled_fte - fulfilled_fte) * offered_wage_per_fte * SEVERANCE_MULTIPLIER` (e.g., 2.0 = 2 weeks of wages per laid-off FTE). Debit from `company.brokerage_account.cash` or `available_cash`. Credit to the region's class savings (workers take home severance).

3. **Add `prev_fulfilled_fte` field to `Company`:** New serialized field `#[serde(default)] pub prev_fulfilled_fte: f64`. Updated at the end of each labor clearing pass.

**File:** `state/src/corporate/strategy.rs`

4. **Cap Restructure layoffs:** Change `worker_capacity / 2` to `worker_capacity / 10` (max 10% capacity reduction per turn). Change `worker_capacity / 4` for cooperatives to `worker_capacity / 8`.

5. **Cap Expand new_workers:** Clamp `new_workers` to `(worker_capacity as f32 * 0.20) as u32 + 1` (max 20% growth per turn).

### 1.3 Expected Impact

- Employment swings dampened from ±50-116% to ±15-20% per turn.
- Severance costs create a natural brake on mass firing during short-term shocks.
- Consumer demand stabilizes, reducing deflationary pressure.

---

## PART 2: The 0.00 VWAP & Investment (I) Deadlock

### 2.1 Root Cause Analysis

**The construction supply chain is frozen in a vicious cycle:**

1. **Producers can't sell:** B2B sell asks are submitted in `submit_company_b2b_orders` (b2b_orders.rs:355-399) with `sell_price = unit_cost * (1 + markup)`. But `unit_cost` is calculated from input reference prices. If input reference prices are 0 (no VWAP, no last_trade), `unit_cost = 0`, and the code hits `continue` at line 377 — **no sell ask is submitted**.

2. **Consumers can't buy:** Construction B2B orders in `submit_construction_b2b_orders` (orders.rs:115-124) require `get_reference_price(commodity, market_history)`. If no VWAP/last_trade/base_price exists, the bid is skipped. The fallback chain in `get_reference_price` (market_history.rs:44-56) goes VWAP → last_trade → global_base. But `global_base_prices` has 140 entries — so this should work. The issue is that **base prices exist but producers still don't submit asks** because their `unit_cost` calculation fails.

3. **The cascade:** No asks → no trades → no VWAP → `unit_cost = 0` → no asks. The market is stuck at zero.

4. **Construction tenders:** Tenders are published (91 construction companies exist), bids are submitted, tenders are awarded. But after award, `advance_construction_projects` (orders.rs:186) consumes materials from `building.inventory`. The inventory is empty because the contractor's B2B buy bids (orders.rs:38-163) fail when there are no sell asks to match against. Projects go `on_hold` for 5 turns, then get cancelled (orders.rs:226).

5. **VWAP = 0.00 in UI:** The snapshot (snapshot.rs:425) reads `vwap_per_commodity.get(&c).unwrap_or(0.0)`. Only 8 commodities have VWAP entries. The rest show 0.00.

### 2.2 Implementation Plan: Unclog the Supply Chain

**File:** `state/src/economy/trade/b2b_orders.rs`

1. **Fix the unit_cost=0 fallback:** When `unit_cost == 0` and `get_reference_price` for the output returns None, fall back to `global_base_prices` for the output commodity (not just the input). If a base price exists, use `base_price * (1 + markup)` as the sell price. Currently the code does `continue` (line 377) — change it to use the base price.

2. **Seed initial VWAP from base prices:** On the first turn (when `vwap_per_commodity` is empty), the market clearing should use `global_base_prices` as the VWAP. This is already the case via `get_reference_price`'s fallback chain, but the issue is that **no asks are submitted** because producers' `unit_cost` is 0. Fix #1 above resolves this.

**File:** `state/src/economy/market/market_history.rs`

3. **After market clearing, update VWAP from base prices for commodities with no trades:** If a commodity had no trades this turn but has a `global_base_price`, set its `vwap_per_commodity` to the base price. This prevents the "0.00 VWAP" display and ensures the fallback chain works next turn.

**File:** `state/src/construction/orders.rs`

4. **Construction contractor advance funding:** For State-backed projects, the first tranche should be released immediately on award (not after progress). This gives the contractor cash to buy materials. Currently, tranches are only released on progress milestones, creating a deadlock: no materials → no progress → no tranche release → no cash → no materials.

### 2.3 Expected Impact

- Producers submit sell asks using base prices as fallback.
- B2B trades execute, generating VWAP.
- Construction contractors buy materials, projects advance.
- Investment (I) becomes nonzero.

---

## PART 3: VIP Cloning, Election Ghosts & Dead Ministries

### 3.1 VIP Cloning Bug

**File:** `state/src/politics/ministries.rs`, lines 431-453

**Root cause:** In the single-party government branch, `resolve_minister_name` is called ONCE (line 434) and the result is `.clone()`d for every ministry:
```rust
let minister_name = resolve_minister_name(active_parties, &pm_party);  // Called once!
for comp in all_competencies.iter() {
    ministries.push(Ministry {
        minister_name: minister_name.clone(),  // Same name for every ministry!
        ...
    });
}
```

`resolve_minister_name` (line 488) returns the **party leader's name** — so every ministry gets the PM's name.

**Fix:** Call `resolve_minister_name` inside the loop, OR better: generate a unique VIP name for each ministry using `generate_full_vip` from `names.rs`. The minister should be a party member, not necessarily the party leader.

**Implementation:**
- Pass `cultural_group` and `rng` into `form_government`.
- For each ministry, call `generate_full_vip(cultural_group, rng)` to get a unique name.
- Use the party leader's name only for the PM/Head of State.

### 3.2 Parliament Initialization

**File:** `state/src/politics/turn.rs`, lines 272-321

**Current state:** The election logic at line 289 checks `form.is_democratic() && election_due`. The "regime repair" at line 273 checks `parliament.is_empty()` and sets `years_to_elections = 0`. This should trigger elections on the next political year.

**The issue:** `process_political_year` is called only at year boundaries (turn.rs:2840: `is_year_boundary = turn > 0 && (turn + 1) % 24 == 0`). If the game starts at turn 0, the first political year is at turn 23. The parliament remains empty for 23 turns.

**Fix:** In `bootstrap_politics` (turn.rs:711), after generating parties, immediately call the election logic if the form is democratic. This ensures parliament is populated at game start, not after 23 turns.

### 3.3 Dead Ministries

**File:** `state/src/politics/ministries.rs`, lines 760-770

Several competencies have no real spending logic — they just debit `ministry_cash` and record a generic `InfrastructureFunding` action:
- `Treasury`, `ForeignAffairs`, `Justice`, `Science`, `Energy`, `Culture`, `Environment`, `Labor`, `Housing`

**STRICT RULE: Do NOT delete or disable any ministries.** All default competencies are vital structural placeholders for future simulation phases (Diplomacy, Tech Trees, Energy Grid). Leave `default_competency_bundles` exactly as it is. The "dead" ministries with 0 budget will simply not spend cash until their spending logic is implemented in a future phase. This is the correct design — they exist as structural scaffolding, not as active spenders.

**No changes needed to `default_competency_bundles`.** The only fix in this area is the VIP cloning bug (Part 3.1) and the parliament initialization (Part 3.2).

---

## PART 4: Sovereign Yields, DSPW & Finance Tab UI

### 4.1 Sovereign Yields

**File:** `state/src/economy/finance/debt_market.rs`, lines 373-388

**Current state (Phase 36):** The yield is already `CB_reference_rate + credit_spread`. The 4.5% the user sees is likely from **old debt in the save file** with fixed coupon rates, or the CB reference rate happens to be ~4% + 0.5% spread.

**Fix:** When loading old saves, migrate existing `TreasurySecurity` coupon rates to the current `sovereign_yield` formula. Add a migration pass in `load_game_state` or `process_political_year` that recalculates `coupon_rate` for existing securities.

### 4.2 DSPW Primary Dealers

**File:** `state/src/engine/generator/mod.rs`, lines 867-968

**Root cause:** The generator sets `company.is_dspw = is_first` (line 968) on the first bank, but **never populates `country.debt_market.primary_dealers`** or sets `country.debt_market.dspw_enabled = true`. The `DebtMarket` is initialized with `Default::default()` (line 270) which gives `primary_dealers: Vec::new()` and `dspw_enabled: false`.

**Fix:** After building bank companies, populate the debt market:
```rust
country.debt_market.dspw_enabled = true;
country.debt_market.primary_dealers = banks.iter()
    .filter(|b| b.is_dspw)
    .map(|b| b.id.clone())
    .collect();
```

Also: designate 2-3 banks as DSPW (not just 1) for larger countries. Change the logic to:
```rust
let num_dspw = ((num_banks + 1) / 2).min(3);  // Half of banks, max 3
for i in 0..num_banks {
    company.is_dspw = i < num_dspw;
}
```

### 4.3 Finance Tab UI — Tax Revenue Breakdown

**File:** `state/src/ui/tui/render.rs`, `render_finance` (line 727)

**Current state:** The `FinanceSnapshot` struct (snapshot.rs:251) already has `pit_revenue`, `cit_revenue`, `vat_revenue`, `wealth_tax_revenue`, `capital_gains_revenue` fields. But `render_finance` does NOT display them.

**Fix:** Add a "TAX REVENUE" section between "MINISTRIES" and "PUBLIC DEBT":
```
TAX REVENUE
  PIT Revenue         $X
  CIT Revenue         $X
  VAT Revenue         $X
  Wealth Tax          $X
  Capital Gains Tax   $X
```

### 4.4 Finance Tab UI — Debt Holders

**Fix:** Add a "DEBT HOLDERS" section showing the breakdown of who holds public debt:
- Banks (sum of `bank.balance_sheet.securities`)
- Central Bank (sum of `cb.omo_bond_holdings`)
- Citizens (sum of retail bonds)
- Investment Funds (sum of fund bond holdings)

This requires adding fields to `FinanceSnapshot` and computing them in the snapshot builder.

---

## PART 5: Megaregions, Geology & UI Cleanup

### 5.1 Megaregion Clustering

**File:** `state/src/society/geography.rs`, `generate_megaregions` (line 2257)

**Current state:** ALL regions are dumped into a single megaregion:
```rust
pub fn generate_megaregions(country: &str, region_ids: &[String]) -> Megaregion {
    Megaregion {
        regions: region_ids.to_vec(),  // ALL regions!
        ...
    }
}
```

**Fix:** Replace with clustering logic:
1. If `region_ids.len() <= 3`: return a single megaregion (small country).
2. If `region_ids.len() > 3`: group regions into clusters of 3-5 by splitting the list into `ceil(len / 4)` groups. Each group becomes a megaregion with a unique generated name.
3. Return `Vec<Megaregion>` instead of a single `Megaregion`.

**File to update:** `state/src/engine/generator/mod.rs` — change the call site to handle `Vec<Megaregion>`.

### 5.2 Geology & Mines Tab Overhaul

**File:** `state/src/ui/tui/render.rs`, `render_construction_geology` (line 209)

**Current state:** The tab is a key-value table mixing tenders, KIO appeals, structural defects, and geological deposits into a single 2-column layout. It's unreadable.

**Fix:** Split into a proper multi-column table for geology:
```
Deposit ID | Formation | Commodity | Reserves | Quality | Depletion | Active Miners
```

**Active miners tracking:** Add a new field to `GeologicalDeposit` (or compute on-the-fly): count the number of Mining-sector companies whose `building.deposit_id` matches this deposit. This requires:
1. Adding `active_miner_count: u32` to the deposit snapshot.
2. In the snapshot builder, iterate companies/buildings and count matches per deposit.

### 5.3 UI Polish

#### 5.3.1 Government Tab Duplicate Headers

**File:** `state/src/ui/tui/render.rs`, lines 426-477

**Root cause:** The Government tab has inline bold headers at lines 436-443 ("Ministry", "Minister", "Party", etc.) AND a `.header()` call at lines 469-477 with the same column names. This produces duplicate headers.

**Fix:** Remove the inline bold header rows (lines 426-443). Keep only the `.header()` call.

#### 5.3.2 GDP per Capita Decimal Truncation

**File:** `state/src/ui/tui/render.rs`, line 694

**Current:** `format!("{:.0}", r.gdp_per_capita)` — truncates to integer.
**Fix:** `format!("{:.2}", r.gdp_per_capita)` — shows 2 decimal places.

#### 5.3.3 Company Name Generator

**File:** `state/src/engine/generator/corporate.rs`, lines 548, 1334, 1589

**Current:** Company names are generic: `"Seed Construction (Eldoria-Region1) #1"`, `"Retail Co KRS-ELD-0008 (Eldoria-Region1)"`.

**Fix:** Add a `generate_company_name` function that produces realistic names using:
- A prefix from a cultural name pool (e.g., "Kowalski", "Müller", "Rossi") — cultural surnames are fine and encouraged
- A sector suffix that MUST be strictly in English (e.g., "Steel Works", "Construction Co", "Mining Corp", "Energy Holdings", "Agricultural Trust")
- A legal form suffix that MUST be strictly in English (e.g., "Inc.", "Ltd.", "Corp.", "Holdings")

**STRICT RULE:** Descriptive sector suffixes and legal forms MUST be in English. Do NOT use localized terms like "Huta", "Sp. z o.o.", "S.A." (use "Inc." or "Corp." instead), or any non-English business descriptors. Cultural surname prefixes are the only non-English element allowed.

Example: `"Kowalski Steel Works Inc."` instead of `"Seed HeavyIndustry (Region1) #1"`.

---

## Implementation Steps (Ordered)

### Step 1: Labor Market Frictions (Part 1)
- Add `prev_fulfilled_fte` field to `Company`
- Add hiring cap (15% growth max) in `resolve_regional_labor_market`
- Add severance pay on FTE reduction
- Cap Restructure layoffs to 10% per turn
- Cap Expand new_workers to 20% per turn
- **Files:** `entities/mod.rs`, `economy/labor/labor_market.rs`, `corporate/strategy.rs`

### Step 2: Unclog Construction Supply Chain (Part 2)
- Fix `unit_cost=0` fallback in `submit_company_b2b_orders` to use base prices
- Seed VWAP from base prices for no-trade commodities
- Release first tranche immediately for State-backed construction projects
- **Files:** `economy/trade/b2b_orders.rs`, `economy/market/market_history.rs`, `construction/orders.rs`

### Step 3: Fix VIP Cloning & Parliament (Part 3)
- Fix `form_government` to generate unique minister names per ministry
- Call elections immediately in `bootstrap_politics` for democracies
- **DO NOT** modify `default_competency_bundles` — all ministries are structural placeholders for future phases
- **Files:** `politics/ministries.rs`, `politics/turn.rs`, `politics/names.rs`

### Step 4: DSPW & Sovereign Yields (Part 4)
- Populate `debt_market.primary_dealers` and set `dspw_enabled` in generator
- Designate 2-3 banks as DSPW for larger countries
- Add coupon rate migration for old saves
- **Files:** `engine/generator/mod.rs`, `economy/finance/debt_market.rs`, `io/save_manager.rs`

### Step 5: Finance Tab UI (Part 4)
- Add TAX REVENUE section to `render_finance`
- Add DEBT HOLDERS section
- Add debt holder fields to `FinanceSnapshot` and snapshot builder
- **Files:** `ui/tui/render.rs`, `ui/snapshot.rs`

### Step 6: Megaregion Clustering (Part 5)
- Replace `generate_megaregions` with clustering logic
- Return `Vec<Megaregion>`, update call sites
- **Files:** `society/geography.rs`, `engine/generator/mod.rs`

### Step 7: Geology Tab & UI Polish (Part 5)
- Overhaul `render_construction_geology` with multi-column geology table
- Add active miner count per deposit
- Remove duplicate Government tab headers
- Fix GDP/capita formatting to `{:.2}`
- Add company name generator (cultural surnames OK, but sector suffixes and legal forms MUST be English-only — no "Huta", "Sp. z o.o.", "S.A.")
- **Files:** `ui/tui/render.rs`, `ui/snapshot.rs`, `engine/generator/corporate.rs`

### Step 8: Build, Test, Verify
- `cargo build`
- `cargo test --lib`
- Ensure all 694 tests pass
- Verify no new infinite loops or performance regressions

---

## Risks & Considerations

1. **Labor frictions may cause initial unemployment spike:** If companies can't hire fast enough to replace attrition, production may drop temporarily. Mitigate by allowing 15% growth (not 5%) and exempting companies with <10 FTE (small companies can double instantly).

2. **Base price fallback may distort market dynamics:** If all trades happen at base prices, the market never discovers real prices. Mitigate by only using base prices as the initial seed; once VWAP exists, it takes priority.

3. **Megaregion clustering changes the `Vec<Megaregion>` type:** The `Country.megaregions` field is already `Vec<Megaregion>`, so this is backward-compatible. Old saves with one megaregion will still load.

4. **Severance pay may bankrupt companies on the edge:** If a company is already bleeding cash, severance payments push it into bankruptcy faster. This is realistic but may cause a cascade. Mitigate by capping severance at `available_cash * 0.3` (can't spend more than 30% of cash on severance).
