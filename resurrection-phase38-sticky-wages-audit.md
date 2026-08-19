# Phase 38 Audit: Sticky Wages, Tax Blackout, Bank Boom/Bust & UI Overhaul

**Summary:** A read-only audit tracing five critical edge-case crises identified from the user's latest long-run simulation: 30% single-turn wage cuts, zero tax revenue on the Finance tab, bank hiring boom/bust cycles, eternal 4.5% bond yields with non-functional DSPW, and Government/Regions tab UI defects.

---

## PART 1: Downward Wage Rigidity (Sticky Wages)

### Root Cause

`set_wage_offers` in `state/src/corporate/manager.rs:873` recomputes `offered_wage_per_fte` from scratch every turn:

```rust
let computed_wage = wage_budget / effective_fte;
company.offered_wage_per_fte = computed_wage.min(sane_max);
```

There is **no memory of the previous turn's wage**. When a company's `brokerage_account.cash` drops (e.g., after a bad B2B settlement), the formula produces a wage 20-40% lower than last turn. The labor market then clears at this slashed wage — workers take the cut instead of being laid off.

### Fix Plan

1. **Add `prev_offered_wage_per_fte: f64` to `Company`** in `state/src/entities/mod.rs` (with `#[serde(default)]` for save compatibility).
2. **In `set_wage_offers`**, after computing `computed_wage`, enforce a downward rigidity cap:
   - If `prev_offered_wage_per_fte > 0.0`:
     - `max_drop = prev_offered_wage_per_fte * (1.0 - STICKY_WAGE_CAP)` where `STICKY_WAGE_CAP = 0.03` (3% max cut per turn).
     - `final_wage = computed_wage.max(max_drop)`.
   - This means the wage offer can rise freely but cannot drop more than 3% per turn.
3. **If the company cannot afford the sticky wage** (i.e., `effective_cash * fraction / effective_fte < max_drop`):
   - Keep `offered_wage_per_fte = max_drop` (the sticky wage).
   - The labor market clearing will naturally hire fewer workers because `max_affordable_fte = brokerage_cash / wage` will be lower.
   - This is the correct Keynesian behavior: wages stay sticky, employment bears the adjustment.
4. **Save `prev_offered_wage_per_fte`** at the end of `set_wage_offers` (or at the same time as `prev_fulfilled_fte` is saved in `turn.rs:1940`).

### Files to Modify
- `state/src/entities/mod.rs` — Add `prev_offered_wage_per_fte` field + all initializers
- `state/src/corporate/manager.rs` — Sticky wage logic in `set_wage_offers`
- `state/src/engine/turn.rs` — Save `prev_offered_wage_per_fte` after labor clearing

### Test Plan
- New test: company with cash drop → wage drops ≤3%, not 30%
- New test: company with zero cash → wage stays at prev * 0.97, FTE drops to 0
- Existing wage tests should still pass (they start with `prev_offered_wage_per_fte = 0.0`, so no stickiness applies on first turn)

---

## PART 2: Tax Revenue Blackout & Finance UI

### Root Cause

**The tax revenue blackout is a hardcoded zero bug.** In `state/src/ui/snapshot.rs:790-794`:

```rust
pit_revenue: 0.0, // Filled from last tax collection result if available
cit_revenue: 0.0,
vat_revenue: 0.0,
wealth_tax_revenue: 0.0,
capital_gains_revenue: 0.0,
```

The comment says "Filled from last tax collection result if available" — but this was never implemented. The `TaxCollectionResult` is computed in `turn.rs:2676` as a local variable `tax_result`, used for customs evasion recovery, then **dropped**. It is never stored on `Country`.

### Fix Plan

1. **Add `last_tax_result: Option<TaxCollectionResult>` to `Country`** in `state/src/state/mod.rs` (with `#[serde(default)]` and `#[serde(skip)]` to avoid save bloat — this is ephemeral telemetry, not game state).
2. **Store the result** in `turn.rs` after `process_tax_collection_turn`:
   ```rust
   task.ctx.country.last_tax_result = Some(tax_result.clone());
   ```
   Wait — `tax_result` is used by reference after this for `tax_result.taxes_evaded`. So clone it before storing, or store first and use the stored copy.
3. **Read it in `build_finance_snapshot`** in `snapshot.rs`:
   ```rust
   let last_tax = country.last_tax_result.as_ref();
   pit_revenue: last_tax.map(|t| t.pit_collected).unwrap_or(0.0),
   cit_revenue: last_tax.map(|t| t.cit_collected).unwrap_or(0.0),
   // etc.
   ```
4. **Add tax rate display** to the Finance tab in `render.rs`. The rates are on `country.tax_rates`:
   - PIT rate: `country.tax_rates.income_tax.rate`
   - CIT rate: `country.tax_rates.corporate_tax`
   - VAT rate: average of `country.tax_rates.vat` brackets (or just the "standard" bracket)
   - Wealth tax: top bracket rate from `country.tax_rates.wealth_tax.brackets`
   - Capital gains: top bracket rate from `country.tax_rates.capital_gains_tax.brackets`
   
   Add these to `FinanceSnapshot` as new fields (`pit_rate`, `cit_rate`, `vat_rate`, etc.) and display in parentheses: `"PIT Revenue (18%)"`.

### Files to Modify
- `state/src/state/mod.rs` — Add `last_tax_result` field to `Country`
- `state/src/engine/turn.rs` — Store `tax_result` on country
- `state/src/ui/snapshot.rs` — Read `last_tax_result` + tax rates into `FinanceSnapshot`
- `state/src/ui/tui/render.rs` — Display rates in parentheses

### Test Plan
- Existing `real_game_state_struct_round_trip` test must still pass (the field is `#[serde(skip)]`)
- New test: after `process_tax_collection_turn`, `country.last_tax_result` is `Some(...)` with nonzero PIT

---

## PART 3: The Banking "Boom & Bust"

### Root Cause

In `state/src/state/banking.rs:2587-2614`, bank FTE demand is recomputed every turn:

```rust
let fte_demand = (portfolio / 100_000.0).ceil();
let payroll_budget = bank_cash * 0.3;
let max_affordable = if bank_wage > 0.0 { payroll_budget / bank_wage } else { 0.0 };
bank.target_fte_demand = fte_demand.min(max_affordable).max(2.0);
```

**Problem 1:** When a bank has zero loans (start of game or after loan repayments), `portfolio = 0`, so `fte_demand = 0`, but the `.max(2.0)` floor forces 2 FTE. Next turn, if the bank issues loans, `portfolio` jumps, `fte_demand` jumps to e.g. 50, and the bank hires 50 people. The `payroll_budget = bank_cash * 0.3` allows this if the bank has cash. But next turn, those 50 employees consume all the cash as wages, `bank_cash` drops, `max_affordable` drops to 2, and all 50 are fired.

**Problem 2:** There is no growth cap on bank FTE demand (unlike the 15% cap on regular companies in `labor_market.rs:216`). Banks can 25x their workforce in a single turn.

**Problem 3:** The 30% payroll budget fraction is too high — it leaves nothing for operations, loan disbursement, or reserve requirements.

### Fix Plan

1. **Cap bank FTE growth at 10% per turn** (tighter than the 15% for regular companies, since banks should be more conservative):
   ```rust
   let prev_fte = bank.prev_fulfilled_fte.max(2.0);
   let max_growth = prev_fte * 1.10;
   bank.target_fte_demand = fte_demand.min(max_affordable).min(max_growth).max(2.0);
   ```
2. **Reduce payroll budget fraction from 30% to 15%** — banks should allocate no more than 15% of cash to payroll, leaving 85% for lending operations and reserves.
3. **Use a smoothed portfolio average** instead of instantaneous portfolio, similar to the charity donation smoothing in `set_wage_offers`. This prevents FTE demand from spiking when a batch of loans is issued and crashing when they're repaid.
4. **Ensure `prev_fulfilled_fte` is saved for banks too** — it already is, since the save loop in `turn.rs:1940` iterates all companies. But banks need to be exempt from the `SMALL_COMPANY_FTE_THRESHOLD` check or use a higher threshold (e.g., 2.0 instead of 10.0) since banks start at 2 FTE.

### Files to Modify
- `state/src/state/banking.rs` — Cap FTE growth, reduce payroll fraction, smooth portfolio
- `state/src/economy/labor/labor_market.rs` — Ensure banks get the growth cap (they already do via `prev_fulfilled_fte`, but verify the threshold)

### Test Plan
- New test: bank with 2 FTE and large portfolio → FTE demand grows by max 10%, not 25x
- New test: bank with 50 FTE and zero cash → FTE demand drops gradually, not to 2 instantly

---

## PART 4: The 4.5% Yield & Election Deadlock

### 4A: Sovereign Bond Yields

#### Root Cause

The yield formula in `debt_market.rs:375-388` is **correct** — it computes `cb_reference_rate + credit_spread`. However, the Central Bank's `reference_rate` defaults to `0.04` (4%) in `build_central_bank` (`generator/mod.rs:~860`), and with the 0.5% base credit spread, the yield becomes 4.5%.

The issue is that the **CB reference rate never changes** because `update_reference_rate` may not be called, or the Taylor Rule inputs are at their defaults (2% inflation target, 2% neutral rate, 0% actual inflation → rate stays at neutral = 2%... but the default `reference_rate` is 0.04, not 0.02).

**The real bug:** The `reference_rate` is initialized to `0.04` but `neutral_rate` is `0.02`. The Taylor Rule should set `reference_rate = neutral_rate + inflation + 0.5 * (inflation - target) + 0.5 * (growth_gap)` ≈ 2% + 0% + 0 + 0 = 2%. But if `update_reference_rate` is never called (or called after the first bond issuance), the rate stays at the hardcoded 4%.

#### Fix Plan

1. **Verify `update_reference_rate` is called every turn** before any debt issuance. Search for its call site in `turn.rs`.
2. **Set the initial `reference_rate` to `neutral_rate` (0.02)** instead of 0.04 in `build_central_bank`, so the first turn's bonds are issued at ~2.5%, not 4.5%.
3. **The DSPW logic in `debt_market.rs:411-413` is broken:**
   ```rust
   let eligible = !country.debt_market.dspw_enabled || country.debt_market.primary_dealers.is_empty();
   if !eligible && country.debt_market.primary_dealers.is_empty() {
       return;
   }
   ```
   When `dspw_enabled = true` and `primary_dealers` is non-empty: `eligible = false`, and the guard `!eligible && primary_dealers.is_empty()` = `true && false` = `false`, so it falls through. But the DSPW dealers are **never actually used as buyers** — the code falls through to `citizen_savings * 0.05` regardless. The DSPW banks should be the actual buyers, using their `balance_sheet.reserves_at_central_bank`.

4. **Fix DSPW buyer logic via REVERSED transaction flow (borrow-checker safe):**
   
   **CRITICAL ARCHITECTURAL RULE:** The `companies` vector MUST NOT be passed down into `debt_market.rs` or `issue_treasury_securities`. Doing so inside the `par_iter_mut` turn loop would cause a catastrophic borrow-checker violation or thread lock.
   
   **The correct pattern is a reversed pull-based flow:**
   
   a. **In `issue_treasury_securities` (`debt_market.rs`):** When `dspw_enabled` and `primary_dealers` is non-empty, create the securities as **unpurchased "Auction Inventory"** — set their `holders` to empty and mark them with a new field `is_auction_inventory: bool` (or use a pending status). The treasury does NOT receive cash yet. The securities sit in `debt_market.outstanding_securities` awaiting buyer pull.
   
   b. **Add a new `dspw_auction_settlement` step in `turn.rs`** that runs AFTER `issue_treasury_securities` (line ~2745) and has access to both `&mut [Company]` and `&mut Country` via `tasks.par_iter_mut`. This is a **second banking pass** dedicated to DSPW auction settlement — it does NOT modify the existing `process_banking_turn` call at line 390 (which runs early for loan/deposit operations). The new step:
      - Iterates over banks where `is_dspw == true`.
      - Each DSPW bank reads `country.debt_market.outstanding_securities` for auction-inventory bonds.
      - Evaluates purchase capacity from `balance_sheet.reserves_at_central_bank` (e.g., up to 5% of reserves).
      - Performs strict double-entry: debit `bs.reserves_at_central_bank`, credit `bs.securities`, add itself as a `SecurityHolder`, and credit `country.budget.liquid_reserves` with the purchase price.
      - Marks the security as purchased (clears auction-inventory flag).
   
   c. **Turn ordering:** `issue_treasury_securities` (line 2740) creates auction inventory → `dspw_auction_settlement` (new, line ~2746) pulls from inventory → `allocate_cash_to_ministries` (line 2755) uses the now-funded treasury.
   
   d. **Fallback:** If no DSPW dealers exist or they lack reserves, `issue_treasury_securities` falls back to the existing citizen-savings pathway (immediate purchase) instead of creating auction inventory.
   
   This keeps entity encapsulation clean: `debt_market.rs` creates inventory, `banking.rs` pulls from it. No cross-module `&mut [Company]` passing required.

### Files to Modify
- `state/src/engine/generator/mod.rs` — Fix initial `reference_rate` to `neutral_rate`
- `state/src/economy/finance/debt_market.rs` — Create auction inventory instead of immediate citizen purchase when DSPW enabled; add `is_auction_inventory: bool` field to `TreasurySecurity`
- `state/src/state/banking.rs` — Add `dspw_auction_settlement` function (pull-purchase from auction inventory)
- `state/src/engine/turn.rs` — Call `dspw_auction_settlement` after `issue_treasury_securities` (line ~2746); verify `update_reference_rate` call ordering

### 4B: Election Deadlock

#### Root Cause

The election escape hatches in `politics/turn.rs:134-176` and `178-210` are **gated on `years_to_elections > 0`** (line 139). But `years_to_elections` is set to `form.election_cycle()` (typically 4) after each election (line 302). The escape hatch only fires when `years_to_elections > 0`, meaning it fires during the 3 years between elections — but it only **generates parties**, it doesn't **trigger an election**. The actual election at line 289 only fires when `years_to_elections == 0`.

So the sequence is:
1. Turn 0 (bootstrap): `years_to_elections = 0` → election fires → provisional government wins (only party) → `years_to_elections = 4`.
2. Year 1: `years_to_elections = 3` → escape hatch fires (if `ruling_party == "Provisional..."` and `active_parties.len() == 1`) → injects real parties → but **no election is held** because `years_to_elections != 0`.
3. Years 2-3: Same — parties exist but no election.
4. Year 4: `years_to_elections = 0` → election fires → real parties compete → democracy works.

**The problem:** If the escape hatch at line 134 fires but the `active_parties.len() == 1` check fails (because the safety net at line 183 already injected parties), the provisional government remains in power with `years_to_elections > 0` and no mechanism to trigger a snap election.

Also, the safety net at line 183 requires `active_parties.len() < 3` — but if `regenerate_parties` produces 2 parties with zero support (because `ig_power` is zero), `total_support == 0.0`, and the provisional government stub is created at line 654, **replacing** the 2 parties. Then `active_parties.len() == 1` again.

#### Fix Plan

1. **Add a snap election trigger** in `process_political_year`, after party regeneration:
   ```rust
   // Phase 38: Force snap election if democratic country has a Provisional Government
   // or fewer than 2 real parties with nonzero support.
   if form.is_democratic() {
       let has_provisional = country.politics.ruling_party == "Provisional Technocratic Government";
       let real_parties = country.politics.active_parties.values()
           .filter(|p| p.support > 0.0 && p.leader.name != "Provisional Technocratic Government")
           .count();
       if has_provisional || real_parties < 2 {
           country.politics.years_to_elections = 0; // Force snap election
           messages.push("[SNAP ELECTION] Forced election to break provisional government deadlock.".to_string());
       }
   }
   ```
   This must be placed **before** the election check at line 285, so the snap election fires in the same turn.

2. **Fix the escape hatch condition** at line 138-140: remove the `years_to_elections > 0` guard so the escape hatch fires even on year 0 (bootstrap).

3. **Ensure `regenerate_parties` doesn't replace real parties with the provisional stub.** When `total_support == 0.0` but parties already exist, inject default support values instead of clearing and replacing.

### Files to Modify
- `state/src/politics/turn.rs` — Snap election trigger, fix escape hatch condition, fix regenerate_parties fallback

### Test Plan
- New test: democratic country with provisional government → snap election fires within 1 year
- New test: democratic country with 1 party → safety net injects parties → election fires same turn

---

## PART 5: UI/UX Reconstruction (Tabs 6 & 8)

### 5A: Government Tab (Tab 6) Fix

#### Root Cause

In Phase 37, the inline bold column headers (Ministry, Minister, Party, Ideology, Allocated, Cash, Spent) were removed from the row list, leaving only the `.header()` call at line 449. However, the `.header()` row labels are: "Role", "Name", "Party", "Ideology", "Allocated", "Cash", "Spent" — these are **generic and don't match the dual-purpose table**. The top rows show Head of State, PM, and Political Capital (which use "Role"/"Name"/"Party"/"Ideology" columns), while the bottom rows show ministries (which need "Ministry"/"Minister"/"Party"/"Ideology"/"Allocated"/"Cash"/"Spent").

The user sees:
- Top: Head of State, PM, Political Capital — these use the first 2-3 columns, leaving 4-5 empty cells.
- Bottom: Cabinet ministers — these use all 7 columns.
- The `.header()` row says "Role / Name / Party / Ideology / Allocated / Cash / Spent" which is confusing for the top section.

Also, "Political Capital" is awkwardly placed as a row in the main table body.

#### Fix Plan

1. **Restructure the Government tab into two sections:**
   - **Top section (header info):** Head of State, PM, Political Capital — rendered as a separate 2-column key-value block above the main table, OR as styled rows with a section divider.
   - **Bottom section (cabinet table):** A proper table with headers: "Ministry / Minister / Party / Ideology / Allocated / Cash / Spent".

2. **Implementation approach:** Since `render_government` returns a single `Table`, the simplest fix is:
   - Keep the single table but add a **section divider row** between the header info and the cabinet.
   - Add an inline header row (bold) right before the cabinet rows: `"Ministry" / "Minister" / "Party" / "Ideology" / "Allocated" / "Cash" / "Spent"`.
   - Change the `.header()` to show generic labels or remove it entirely and rely on the inline header.
   - Move "Political Capital" to be displayed next to the PM row (in the "Party" column or as a suffix).

3. **Widen columns:** The current constraints are `[18, 22, 15, 18, 12, 12, 12]`. Increase Party to 20 and Ideology to 22 to prevent truncation.

### Files to Modify
- `state/src/ui/tui/render.rs` — `render_government` function

### 5B: Regions Tab (Tab 8) Rework

#### Current State

`render_regions` in `render.rs:621` lists all regions sorted by GDP descending, with a national total at the top. The megaregion is shown as a column value but regions are not grouped by it.

#### Fix Plan

1. **Group regions by megaregion** before rendering:
   ```rust
   let mut grouped: BTreeMap<String, Vec<&RegionRow>> = BTreeMap::new();
   for r in &sorted_regions {
       grouped.entry(r.megaregion.clone()).or_default().push(r);
   }
   ```

2. **For each megaregion group:**
   - Render a **megaregion header row** (bold, colored): `"▶ {megaregion_name}"` spanning the first column, with sub-total population and GDP in the appropriate columns.
   - Render the constituent regions indented under the header.
   - Add a **sub-total row** after the group: `"  Sub-total"` with summed population and GDP.

3. **Keep the national total** at the very top.

4. **Sort megaregion groups** by total GDP descending (same as current region sort).

### Files to Modify
- `state/src/ui/tui/render.rs` — `render_regions` function

### Test Plan
- Manual verification: Regions tab shows grouped rows with sub-totals
- No new unit tests needed (UI rendering is visual)

---

## Implementation Order

1. **Part 1 (Sticky Wages)** — Add `prev_offered_wage_per_fte`, implement downward rigidity cap
2. **Part 2 (Tax Blackout)** — Store `last_tax_result` on Country, wire to FinanceSnapshot, add rate display
3. **Part 3 (Bank Boom/Bust)** — Cap bank FTE growth, reduce payroll fraction, smooth portfolio
4. **Part 4A (Yields)** — Fix initial reference_rate, fix DSPW buyer logic
5. **Part 4B (Elections)** — Add snap election trigger, fix escape hatch
6. **Part 5A (Gov Tab)** — Restore cabinet headers, restructure Political Capital
7. **Part 5B (Regions Tab)** — Group by megaregion with sub-totals
8. **Build & Test** — `cargo build`, `cargo test --lib -- --test-threads=1 --nocapture`

## Risks & Considerations

- **Sticky wages** could cause employment to drop more sharply during downturns (by design). This is correct Keynesian behavior but may increase UI-reported unemployment. The user should be informed this is intentional.
- **Tax result storage** uses `#[serde(skip)]` to avoid save bloat. Old saves will have `last_tax_result = None`, showing 0.00 until the next tax collection turn.
- **Bank FTE cap** at 10% growth means banks scale slowly from 2 FTE. A bank starting at 2 FTE needs ~40 turns to reach 100 FTE. This is realistic but may seem slow. Consider a higher initial FTE (e.g., 10) for the main state bank.
- **DSPW buyer logic** uses a reversed pull-based transaction flow: `issue_treasury_securities` creates unpurchased auction inventory, and `process_banking_turn` (which already has `&mut [Company]` + `&mut Country`) pulls bonds from that inventory using bank reserves. This avoids passing `companies` into `debt_market.rs` and is fully borrow-checker safe inside `par_iter_mut`. The `process_banking_turn` call must be sequenced AFTER `issue_treasury_securities` in the turn loop.
- **Election snap trigger** must be carefully placed to avoid infinite election loops (if the snap election still produces a provisional government, it would trigger again next year). Add a guard: only trigger snap election if `years_to_elections > 0` (i.e., not already election year).
