# Phase 40 — The Great Unclogging: Technical Audit & Implementation Blueprint

Phase 39 compiled and passed 698 tests, but a rigorous 7-turn simulation reveals that the engine's pipes are clogged at five critical junctions. This document traces each clog to its root cause and specifies the exact fix.

---

## PART 1: The Construction Tender Deadlock (`awarded=false`) & Tab 3 UI

### 1.1 Root Cause — Why Tenders Never Get Awarded

**File:** `state/src/engine/turn.rs` lines 617–714, `state/src/construction/tender_market.rs` lines 272–325

**Finding:** The tender pipeline has THREE compounding defects:

**Defect A — No State Tenders:** The State NEVER publishes tenders. Only property developers (corporations) publish tenders via `publish_developer_tenders` (line 622). There is no code path where the Treasury or any ministry publishes a `TenderInvestorType::State` tender for infrastructure, courthouses, roads, etc. The `TenderInvestorType::State` variant exists but is never used in the turn loop.

**Defect B — Deadline Expiry Timing:** `process_tender_awards` (line 684) only awards tenders that have EXPIRED: `current_turn >= published_turn + deadline_turns`. With `deadline_turns = 5` (developer tenders) or `8` (gas station tenders), and the user running only 7 turns, most tenders haven't reached their expiry. The award check runs every turn, but a tender published on turn 1 won't be awardable until turn 6.

**Defect C — Bid Submission is One-Shot:** Bids are submitted every turn (line 657–680), but the `construction_bid_decision` function (line 334) generates random bid costs that may fall below the dumping floor (`DUMPING_FLOOR_RATIO = 0.5`). The bid cost formula is:
```rust
let bid_cost = tender.estimated_cost * (0.8 + safety_factor * 0.2) * (0.9 + rng.gen::<f64>() * 0.2);
```
With `safety_factor = 1.0 - company.safety_level * 0.1`, and `safety_level` defaulting to 0, this gives `bid_cost = estimated_cost * 1.0 * [0.9–1.1]`, which is always above the dumping floor of `0.5 * estimated_cost`. So bids ARE being submitted. The issue is purely the deadline timing.

**Conclusion:** Tenders DO get bids, but the 5–8 turn deadline means they sit as `awarded=false` until expiry. After expiry, they SHOULD be awarded. If the user sees `awarded=false` after 7 turns, tenders published on turns 1–2 should have been awarded by turn 6–7. The remaining tenders (published turns 3–7) are still within their bidding window.

**However**, there's a subtle bug: `process_tender_awards` removes awarded/cancelled tenders from the list (line 319–322). So if a tender WAS awarded, it disappears from `phase22_tenders`. The user sees only the still-open tenders, all of which are `awarded=false` because they haven't expired yet. The UI doesn't show awarded projects — only open tenders.

### 1.2 Fix Plan — Tender AI & State Tenders

1. **Add State Tender Publishing:** Create a new function `publish_state_tenders` that runs during the ministry spending phase. When the Infrastructure/Transport ministry has `ministry_cash > 0`, it publishes tenders for roads, bridges, courthouses, etc. The ministry's `ministry_cash` is the `estimated_cost`. Use `TenderInvestorType::State` with `investor_id = "STATE:{region_id}"`.

2. **Reduce Default Deadline:** Change developer tender deadline from 5 to 2 turns, and state tender deadline to 1 turn (emergency procurement). This ensures tenders are awarded within 2–3 turns instead of 5–8.

3. **Aggressive Bid AI:** The current `construction_bid_decision` is fine — bids ARE submitted. But ensure at least ONE construction company bids per tender by checking if the tender has zero bids after the first pass and injecting a fallback bid from the highest-capital construction company.

4. **Award Immediately if Bids Exist:** Add an early-award path in `process_tender_awards`: if a tender has ≥3 bids, award it immediately rather than waiting for deadline expiry.

### 1.3 Fix Plan — Tab 3 Overhaul (Both Tables)

**File:** `state/src/ui/tui/render.rs` lines 208–263, `state/src/ui/snapshot.rs` lines 65–85, 472–482

**Current State:** Tab 3 renders as a single key-value list with hash-like tender IDs and crammed deposit info:
```
  ID    tender_COMPANY-001_State_Highway_3 [Open] awarded=false
  Deposit    CopperVein / DEP-003 — reserves=1200/5000 qual=0.85 depleted=24.0% miners=3
```

**Fix:** Redesign Tab 3 to contain TWO distinct, clean multi-column tables.

#### Table 1: Tenders (5 columns)

| Column | Source | Example |
|--------|--------|---------|
| Name | Generated from project type + sequence | "State Highway A1" |
| Type | `project_type` formatted | "Infrastructure" |
| Value | `estimated_cost` | "1.2M" |
| Status | `status` + `awarded` | "Open" / "Awarded" / "Cancelled" |
| Contractor | `awarded_bid` → bidder_id | "Construction Group" |

#### Table 2: Geological Deposits (5 columns)

| Column | Source | Example |
|--------|--------|---------|
| Deposit ID | `deposit_id` | "DEP-003" |
| Commodity | `formation` | "Copper Vein" |
| Reserves | `current_reserves` / `estimated_reserves` | "1.2K / 5.0K" |
| Quality | `quality` (0.0–1.0) | "0.85" |
| Active Miners | `active_miners` | "3" |

**Implementation:**
- Add `tender_name: String` field to `ConstructionTender` (or generate from project type + sequence number).
- Add a `TenderNameGenerator` that produces names like:
  - Infrastructure → "State Highway A{n}", "Bridge Project {n}"
  - Residential → "Housing Estate {n}"
  - Factory → "Industrial Park {n}"
  - Court → "Regional Courthouse {n}"
  - Embassy → "Embassy Complex {n}"
- Update `TenderRow` in snapshot.rs to include `name`, `value`, `contractor` fields.
- `DepositRow` already has all needed fields (`deposit_id`, `formation`, `current_reserves`, `estimated_reserves`, `quality`, `active_miners`).
- Rewrite `render_construction_geology` to render TWO separate `Table::new` blocks:
  1. A 5-column tender table with header row `[Name | Type | Value | Status | Contractor]`.
  2. A 5-column deposit table with header row `[Deposit ID | Commodity | Reserves | Quality | Active Miners]`.
- KIO Appeals and Structural Defects can remain as compact sub-sections below the two tables, or be moved to a different tab in a future phase.

---

## PART 2: Zero-Budget Ministries & Missing Parliament

### 2.1 Root Cause — Zero Ministry Budgets

**File:** `state/src/politics/ministries.rs` lines 356–486, 582–609; `state/src/politics/budget_lifecycle.rs` lines 112–137; `state/src/engine/turn.rs` lines 2662–2682

**Finding:** The budget pipeline has THREE compounding defects:

**Defect A — Ministries Created with 0.0:** `form_government` (line 356) creates every ministry with `allocated_cash: 0.0`. There is no `calculate_budget_needs` step. The only place non-zero allocations are set is `migrate_legacy_budget` (line 1253), which maps legacy `BudgetAllocations` percentages to cash — but only for pre-Phase 8 saves.

**Defect B — Budget Bill Echoes Zero:** `draft_budget_bill` (line 112) copies the existing `allocated_cash` from the ministry config into the bill's `proposed_ministries`. Since `allocated_cash` is 0.0, the bill proposes 0.0 for every ministry.

**Defect C — Enacted Bill Never Applied Back:** After `process_budget_lifecycle` returns `(final_bill, enacted, _msg)` (line 2670), if `enacted == true`, the final bill is DROPPED — the allocations are never written back to `ministry_config`. Only the failure path (`apply_budget_failure_consequence`) modifies `allocated_cash`, and even then it just multiplies existing 0.0 by 0.8 or 0.85.

**Defect D — Allocate Cash Sees Zero:** `allocate_cash_to_ministries` (line 587) computes `promised = sum(allocated_cash)`. Since all are 0.0, `promised <= 0.0` and the function returns immediately without allocating anything.

**Result:** Every ministry has `allocated_cash = 0.0` and `ministry_cash = 0.0`. The spending guard at line 632 (`if ministry.allocated_cash <= 0.0 { return; }`) prevents any spending.

### 2.2 Fix Plan — Budget Needs Calculation

1. **Add `calculate_budget_needs` function** in `ministries.rs`:
   ```rust
   pub fn calculate_budget_needs(country: &Country) -> HashMap<String, f64> {
       // Base budget = 15% of GDP (government spending target)
       let gdp = country.budget.gdp.max(1.0);
       let base_budget = gdp * 0.15;
       // Distribute by ideology weights of the ruling party
       let ideology = ...;
       let priorities = ideology.budget_priorities();
       // Each competency gets: base_budget * weight / sum(weights)
       ...
   }
   ```

2. **Call `calculate_budget_needs` before `draft_budget_bill`** in the turn loop (line 2664). Set each ministry's `allocated_cash` to the computed amount based on its competencies.

3. **Write back final bill allocations** after enactment:
   ```rust
   if enacted {
       for (i, ministry) in config.ministries.iter_mut().enumerate() {
           if let Some(final_alloc) = final_bill.final_ministries.get(i) {
               ministry.allocated_cash = final_alloc.allocated_cash;
           }
       }
   }
   ```

4. **Also call `calculate_budget_needs` on non-budget years** when a new government forms (after elections), so ministries get funding immediately.

### 2.3 Root Cause — Missing Parliament

**File:** `state/src/politics/turn.rs` lines 857–908, 609–619

**Finding:** `run_election_if_due` (line 859) updates the flat `parliament` HashMap and `ruling_party`/`coalition`, but:
- It does NOT call `initialize_parliament` to rebuild `parliament_struct`.
- It does NOT call `form_government` to create new ministries with the new coalition.

Only `process_political_year` (line 613) calls `initialize_parliament`, and it only runs at year boundaries (every 4 turns). `form_government` is only called inside `migrate_legacy_budget` (when `ministry_config` is `None`).

**Result:** After a snap election, the old `parliament_struct` (if any) persists with the old composition. If it was `None` (never initialized), it stays `None` — the UI shows "No Parliament".

### 2.4 Fix Plan — Parliament Instantiation

1. **Add parliament + government formation to `run_election_if_due`** (after line 890):
   ```rust
   // Rebuild parliament_struct
   let cultural_group = "Slavic"; // TODO: read from country metadata
   let mut rng = rand::thread_rng();
   let parliament = super::parliament::initialize_parliament(
       &country.politics, cultural_group, current_turn, &mut rng,
   );
   country.politics.parliament_struct = Some(parliament);

   // Reform government with new coalition
   let active_parties = country.politics.active_parties.clone();
   let new_config = form_government(country, &country.politics.coalition, &active_parties, current_turn);
   country.politics.ministry_config = Some(new_config);
   ```

2. **Also call `calculate_budget_needs`** immediately after forming the new government, so ministries don't sit at 0.0 until the next budget year.

3. **Ensure `process_political_year` also calls `form_government`** after elections, not just `initialize_parliament`. Currently it only initializes the parliament struct but doesn't reform ministries.

---

## PART 3: Symmetric Wage Rigidity & Labor Collapses

### 3.1 Root Cause — Asymmetric Wage Stickiness

**File:** `state/src/corporate/manager.rs` lines 881–1018

**Finding:** Phase 38/39 implemented only DOWNWARD wage stickiness (max 3% drop per turn). There is NO upward cap. Wages can jump from 1000 to 50000 in a single turn if a company has large cash reserves and small FTE demand. The cap at line 1001 (`sane_max = market_average_wage * 3.0`) only prevents extreme outliers — it doesn't prevent 50–200% jumps within the sane range.

**The formula:**
```rust
let computed_wage = effective_cash * wage_budget_fraction / target_fte_demand;
let capped_wage = computed_wage.min(sane_max);
let final_wage = if prev_offered_wage_per_fte > 0.0 {
    capped_wage.max(prev_offered_wage_per_fte * 0.97)  // floor only
} else {
    capped_wage
};
```

There's a floor (`prev * 0.97`) but no ceiling (`prev * 1.05`).

### 3.2 Fix Plan — Symmetric Wage Cap

Add an UPWARD stickiness cap in `set_wage_offers` (line 1010):
```rust
const STICKY_WAGE_MAX_RISE: f64 = 0.05; // 5% max rise per turn

let final_wage = if company.prev_offered_wage_per_fte > 0.0 {
    let wage_floor = company.prev_offered_wage_per_fte * (1.0 - STICKY_WAGE_MAX_DROP);
    let wage_ceiling = company.prev_offered_wage_per_fte * (1.0 + STICKY_WAGE_MAX_RISE);
    capped_wage.max(wage_floor).min(wage_ceiling)
} else {
    capped_wage
};
```

This makes wages symmetric: max 3% down, max 5% up per turn. New companies (prev_wage = 0) are exempt.

### 3.3 Root Cause — Instant Labor Collapse on Zero Cash

**File:** `state/src/economy/labor/labor_market.rs` lines 200–219

**Finding:** When a company has zero brokerage cash:
```rust
let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
    company.brokerage_account.as_ref()
        .map(|ba| ba.cash / company.offered_wage_per_fte)
        .unwrap_or(company.available_cash / company.offered_wage_per_fte)
} else { 0.0 };
// max_affordable_fte = 0.0 / wage = 0.0

let clamped_demand = company.target_fte_demand.min(max_affordable_fte);
// clamped_demand = 0.0
```

The 15% hiring growth cap (line 214) only limits UPWARD growth. There is NO floor on FTE retention. A company with 100 FTE can drop to 0 FTE in one turn if it runs out of cash. Severance pay (line 390) fires after the fact — it doesn't prevent the mass layoff.

### 3.4 Fix Plan — Wage Arrears Mechanic

Implement a "wage arrears" system that allows companies to retain workers even when cash is zero:

1. **Add `wage_arrears: f64` field to `Company`** — accumulates unpaid wages owed to workers.

2. **Add `productivity_penalty: f64` field to `Company`** — computed from arrears, reduces output.

3. **Modify labor market clearing** in `labor_market.rs`:
   ```rust
   // Phase 40: Wage arrears — if company lacks cash, retain FTE but accrue debt
   let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
       company.brokerage_account.as_ref()
           .map(|ba| ba.cash / company.offered_wage_per_fte)
           .unwrap_or(company.available_cash / company.offered_wage_per_fte)
   } else { 0.0 };

   // Phase 40: FTE retention floor — companies can retain up to 90% of
   // prev_fulfilled_fte even with zero cash, by accruing wage arrears.
   const FTE_RETENTION_FLOOR: f64 = 0.90; // 10% max layoff per turn
   let retention_floor = company.prev_fulfilled_fte * FTE_RETENTION_FLOOR;
   let clamped_demand = if max_affordable_fte < retention_floor {
       // Company can't afford full payroll — retain at retention_floor,
       // accrue the unpaid wages as arrears
       retention_floor
   } else {
       company.target_fte_demand.min(max_affordable_fte)
   };
   ```

4. **After labor clearing**, compute arrears:
   ```rust
   // If fulfilled_fte > max_affordable_fte, the company owes unpaid wages
   let affordable_fte = max_affordable_fte;
   let unpaid_fte = company.fulfilled_fte - affordable_fte;
   if unpaid_fte > 0.0 {
       let arrears_this_turn = unpaid_fte * company.offered_wage_per_fte;
       company.wage_arrears += arrears_this_turn;
   }
   ```

5. **Productivity penalty from arrears:**
   ```rust
   // Productivity drops 1% per 10K arrears, max 50%
   company.productivity_penalty = (company.wage_arrears / 10_000.0).min(0.50);
   ```

6. **Apply productivity penalty** in production calculation: `effective_output = base_output * (1.0 - company.productivity_penalty)`.

7. **Arrears repayment:** When a company has positive cash in future turns, it automatically repays arrears first (before B2B purchases): `repayment = min(arrears, cash * 0.3)`.

---

## PART 4: Central Bank 2.0 & Right-Side Finance UI

### 4.1 Root Cause — Tax Ghosting (0.00 Display)

**File:** `state/src/state/tax.rs` lines 1173–1240, `state/src/ui/snapshot.rs` lines 794–799

**Finding:** The tax display issue has two components:

**Component A — Rates are zero:** The Phase 39 fix added baseline wealth-tax and capital-gains brackets, but these may not be applied if `apply_ideology_tax_policies` runs before the tax rates are loaded. The `process_tax_collection_turn` function reads `country.tax_rates`, which may have 0% rates if the ideology policy application failed or was skipped.

**Component B — `last_tax_result` is ephemeral:** `last_tax_result` is `#[serde(skip)]` (line 507 of `state/mod.rs`), so it's lost on save/load. If the user saves and reloads, the Finance tab shows 0.00 until the next tax collection turn. Tax collection runs every turn (line 2685), so this should self-heal after 1 turn. But if the snapshot is taken before the first tax collection turn (e.g., on load), it shows 0.00.

**Fix:**
1. Verify `apply_ideology_tax_policies` actually sets non-zero PIT/VAT/wealth/capital-gains rates. Add a diagnostic log.
2. Ensure `process_tax_collection_turn` runs BEFORE the snapshot is taken (it already does — the snapshot is taken at the end of the turn).
3. The "Other Revenues" (SOE Dividends, Patents, Customs) should be added to the Finance tab's Tax Revenue section. These are already in `TaxCollectionResult` (Phase 39) but may not be populated if the collection functions don't run. Verify that customs revenue is read from `Politics.customs_state.tariff_revenue_collected` and state-property revenue from `country.state_forest_state.treasury_remittance`.

### 4.2 Root Cause — No Negative Interest Rates (NIRP)

**File:** `state/src/state/central_bank.rs` lines 313–363

**Finding:** The Taylor Rule has a hard floor at 0%:
```rust
let new_reference_rate = smoothed_rate.max(0.0).min(0.20);
```

During deflation (inflation < 0), the Taylor Rule computes:
```
taylor_rate = neutral_rate + 1.5 * (inflation - target_inflation) + 0.5 * (gdp_growth - potential_growth)
            = 0.02 + 1.5 * (-0.05 - 0.02) + 0.5 * (0.0 - 0.02)
            = 0.02 - 0.105 - 0.01
            = -0.095
```

But `.max(0.0)` clamps this to 0%, preventing NIRP.

**Fix:** Change the floor to allow negative rates:
```rust
// Phase 40: Allow NIRP (Negative Interest Rate Policy) during severe deflation.
// Floor at -2% (-0.02) to allow meaningful negative rates while preventing
// absurd deep-negative territory.
let new_reference_rate = smoothed_rate.max(-0.02).min(0.20);
```

Also update `update_rate_hierarchy` (line 372) to allow negative deposit/discount rates:
```rust
self.interest_rates.deposit_rate = (reference - 0.015).max(-0.03); // Allow negative deposit rate
self.interest_rates.discount_rate = (reference - 0.0075).max(-0.025);
```

### 4.3 Root Cause — Empty Finance Tab Right Side

**File:** `state/src/ui/tui/render.rs` lines 768–996, `state/src/ui/snapshot.rs` lines 253–292

**Finding:** The Finance tab (`render_finance`) uses a 3-column table (Item, Value, Detail), but the "Detail" column is ALWAYS empty (`Cell::from("")`). The user sees wasted space on the right.

The `FinanceSnapshot` struct has `cb_reference_rate` but is missing:
- `cb_lombard_rate`
- `cb_discount_rate`
- `cb_rediscount_rate`
- `cb_deposit_rate`
- `cb_fx_reserves_total` (sum of all FX reserves)
- `cb_gold_reserves`
- `cb_reserve_requirement_ratio`
- `soe_dividend_revenue`
- `patent_fee_revenue`

**Fix:**
1. **Add CB fields to `FinanceSnapshot`:**
   ```rust
   pub cb_lombard_rate: f64,
   pub cb_discount_rate: f64,
   pub cb_rediscount_rate: f64,
   pub cb_deposit_rate: f64,
   pub cb_fx_reserves_total: f64,
   pub cb_gold_reserves: f64,
   pub cb_reserve_requirement_ratio: f64,
   pub soe_dividend_revenue: f64,
   pub patent_fee_revenue: f64,
   ```

2. **Populate from `CentralBank`** in `build_finance_snapshot`:
   ```rust
   cb_lombard_rate: cb.interest_rates.lombard_rate,
   cb_discount_rate: cb.interest_rates.discount_rate,
   cb_rediscount_rate: cb.interest_rates.rediscount_rate,
   cb_deposit_rate: cb.interest_rates.deposit_rate,
   cb_fx_reserves_total: cb.fx_reserves.values().sum(),
   cb_gold_reserves: cb.physical_gold_reserves,
   cb_reserve_requirement_ratio: cb.reserve_requirement_ratio,
   ```

3. **Redesign `render_finance`** to use the "Detail" column for CB parameters:
   - In the "CENTRAL BANK" section, show Lombard/Discount/Rediscount/Deposit rates in the Detail column.
   - Add a "CB Balance Sheet" subsection with FX Reserves, Gold Reserves, RRR.
   - In the "TAX REVENUE" section, add "Other Revenues" row with SOE Dividends + Patent Fees + Customs in the Detail column.

4. **Layout:** Use the 3rd column ("Detail") as a parallel display column. For example:
   ```
   Item                    Value           Detail
   --- TREASURY ---
     Liquid Reserves       1.2M            CB Lombard Rate: 5.50%
     GDP                   45.2M           CB Discount Rate: 3.75%
   --- MINISTRIES ---
     Total Allocated       800K            CB Deposit Rate: 2.50%
     Total Cash Pocket     600K            CB Rediscount: 4.25%
   --- TAX REVENUE ---
     PIT Revenue (10%)     450K            FX Reserves: 12.5M
     CIT Revenue (15%)     300K            Gold Reserves: 8.3M
     VAT Revenue (20%)     900K            RRR: 10.00%
     Customs Revenue       50K             SOE Dividends: 120K
     State Property        30K             Patent Fees: 15K
   ```

---

## PART 5: The Banking Coma

### 5.1 Root Cause — Banks Can't Pay Tellers After Lending

**File:** `state/src/state/banking.rs` lines 2605–2653, 2352–2450

**Finding:** The bank labor demand logic at line 2605 computes:
```rust
let bank_cash = bank.brokerage_account.as_ref()
    .map(|ba| ba.cash)
    .unwrap_or(bank.available_cash);
let payroll_budget = bank_cash * BANK_PAYROLL_FRACTION; // 15%
let max_affordable = if bank_wage > 0.0 { payroll_budget / bank_wage } else { 0.0 };
bank.target_fte_demand = growth_capped_demand.min(max_affordable).max(2.0);
```

**The problem:** Step 12 (B2B Micro-Loans, line 2352) runs BEFORE Step 15 (Bank Labor Demand, line 2605). In Step 12, banks lend out their brokerage cash to non-bank companies. By the time Step 15 runs, `bank_cash` is near zero because the bank lent everything out.

Then in the labor market clearing (separate function, runs later):
```rust
let max_affordable_fte = ba.cash / company.offered_wage_per_fte;
// = 0.0 / 6000 = 0.0
```

The bank can't hire even the minimum 2 FTE because it has no cash. The `.max(2.0)` on `target_fte_demand` sets the DEMAND to 2, but the labor market's AFFORDABILITY check still yields 0.

**Result:** Banks hire a few tellers on turn 1 (when they still have cash), lend it all out in Step 12, then can never hire again because `brokerage_account.cash` is permanently near zero. Interest income from loan repayments (Step 6, line 2131) does credit `brokerage_account.cash`, but if loans are long-term, the interest trickle is too small to fund payroll.

### 5.2 Fix Plan — Bank Payroll Reservation

1. **Reserve payroll cash BEFORE lending:** In Step 12 (B2B Micro-Loans), before a bank issues any loan, compute the payroll reserve:
   ```rust
   // Phase 40: Reserve cash for teller payroll before lending.
   let avg_wage = country.macro_indicators.average_wage.max(1.0);
   let bank_wage = (avg_wage * 1.2).max(1.0);
   let current_fte = bank.prev_fulfilled_fte.max(2.0);
   let payroll_reserve = current_fte * bank_wage; // One turn of payroll
   let available_for_lending = (bank_cash - payroll_reserve).max(0.0);
   ```
   Then use `available_for_lending` instead of `max_credit` for the lending loop.

2. **Also reserve for consumer loans (Step 13):** Apply the same payroll reserve before consumer loan issuance.

3. **Priority order:** Bank's brokerage cash is allocated in this order:
   1. Payroll reserve (teller wages) — reserved first
   2. Reserve requirement (CB mandate) — reserved second
   3. B2B micro-loans — from remaining cash
   4. Consumer loans — from remaining cash

4. **NO Labor Market Exemption for Banks (Strict Double-Entry):** Banks must NOT receive any special-case affordability exemption in `labor_market.rs`. The previous draft proposed `cash_affordable.max(2.0)` for banks — this is REJECTED because it creates magical fiat money: if a bank has 0 cash and the labor market grants it 2 workers, the payroll debit would either fail or drive cash negative, breaking strict double-entry accounting. **The law is equal for all companies.** Banks with zero cash rely on the exact same "Wage Arrears" (FTE retention floor) mechanic designed in Part 3.4 for normal companies. If a bank has 0 brokerage cash:
   - The FTE retention floor (90% of `prev_fulfilled_fte`) applies equally to banks.
   - The unpaid wages accrue as `wage_arrears` on the bank's `Company` struct.
   - The bank's `productivity_penalty` rises with arrears, reducing loan processing efficiency.
   - The bank repays arrears from future interest income (Step 6 credits interest to brokerage cash).
   This ensures the bank's tellers are retained organically through arrears, not through a money-printing exemption.

5. **Accrue interest to brokerage cash more aggressively:** In Step 6 (loan repayment), already credits interest to `brokerage_account.cash` (Phase 39 fix). Ensure this is sufficient by also crediting a portion of the principal repayment to brokerage cash (e.g., 10% of principal) to maintain operating liquidity. This is the organic source of cash that funds both arrears repayment and new teller hiring.

---

## Implementation Order

1. **Part 2 first** (Budget Needs + Parliament) — unblocks all ministry spending, which unblocks state tenders.
2. **Part 1 second** (State Tenders + Tab 3) — depends on ministries having cash to publish tenders.
3. **Part 3 third** (Symmetric Wages + Arrears) — independent of 1 & 2, but high impact.
4. **Part 4 fourth** (Tax Display + NIRP + Finance UI) — mostly UI + one formula change.
5. **Part 5 fifth** (Bank Payroll Reservation) — independent, but benefits from all above.

## Files to Modify

| File | Changes |
|------|---------|
| `state/src/politics/ministries.rs` | Add `calculate_budget_needs`; fix `form_government` to set non-zero allocations |
| `state/src/politics/budget_lifecycle.rs` | Write back final bill allocations after enactment |
| `state/src/politics/turn.rs` | Add `initialize_parliament` + `form_government` to `run_election_if_due` |
| `state/src/engine/turn.rs` | Call `calculate_budget_needs`; add state tender publishing; wire budget writeback |
| `state/src/construction/tender_market.rs` | Add immediate-award path; add tender name generator |
| `state/src/construction/tenders.rs` | Add `tender_name` field to `ConstructionTender` |
| `state/src/corporate/development.rs` | Add `publish_state_tenders` function |
| `state/src/corporate/manager.rs` | Add upward wage cap (5%); add arrears computation |
| `state/src/economy/labor/labor_market.rs` | Add FTE retention floor (applies equally to banks — NO exemption) |
| `state/src/entities/mod.rs` | Add `wage_arrears`, `productivity_penalty` fields to `Company` |
| `state/src/state/central_bank.rs` | Change rate floor to -2% (NIRP); allow negative deposit/discount rates |
| `state/src/state/banking.rs` | Reserve payroll cash before lending; priority allocation (no labor market exemption) |
| `state/src/ui/snapshot.rs` | Add CB fields to `FinanceSnapshot`; add tender name/value/contractor to `TenderRow` |
| `state/src/ui/tui/render.rs` | Redesign Tab 3 with TWO 5-column tables (Tenders + Deposits); redesign Finance tab with Detail column |

## Verification

- [ ] `cargo build` — 0 errors
- [ ] `cargo test --lib` — all tests pass
- [ ] 7-turn simulation: ministries have non-zero budgets by turn 2
- [ ] 7-turn simulation: at least 3 tenders awarded by turn 4
- [ ] 7-turn simulation: parliament_struct is Some(...) after snap election
- [ ] 7-turn simulation: wages never jump more than 5% per turn
- [ ] 7-turn simulation: companies with zero cash retain 90% of FTE (including banks via arrears, NOT via exemption)
- [ ] 7-turn simulation: CB reference rate can go negative during deflation
- [ ] 7-turn simulation: Finance tab shows non-zero tax revenue and CB parameters in Detail column
- [ ] 7-turn simulation: banks retain tellers via wage arrears (no magical hiring)
- [ ] Tab 3 shows TWO clean tables: Tenders (Name/Type/Value/Status/Contractor) and Deposits (ID/Commodity/Reserves/Quality/Miners)

## Risks/Considerations

- **Wage arrears** is a new mechanic that adds complexity. The productivity penalty must be capped (50%) to prevent total output collapse. Arrears repayment must be prioritized but not so aggressively that it starves the company of operating cash.
- **FTE retention floor** changes labor market dynamics significantly. Companies that are genuinely bankrupt should eventually lose workers — the floor only prevents instant 100% layoffs. A company at 90% FTE with growing arrears and no revenue will still fail over 5–10 turns.
- **Banks use the SAME arrears mechanic — no exemption.** A bank with zero cash retains tellers via the FTE retention floor and accrues `wage_arrears` just like any other company. The bank's interest income (credited to brokerage cash in Step 6) is the organic source of cash for arrears repayment. If a bank has no loan portfolio and no interest income, it will eventually lose all tellers through the 10% per-turn layoff cap — this is correct and realistic. No magical hiring, no money printing, strict double-entry for all.
- **NIRP** could cause weird behavior in bank reserve management. Banks might prefer to hold physical cash rather than pay negative deposit rates. This is realistic but needs monitoring.
- **Budget needs calculation** based on GDP could cause issues if GDP is very low or zero. The formula must have a minimum floor (e.g., 10K per ministry) to ensure basic functionality.
- **State tenders** will compete with corporate tenders for construction company capacity. Ensure the tender market can handle mixed investor types.
- **Tab 3 dual-table layout** must fit within terminal width. Two 5-column tables stacked vertically may exceed vertical space on small terminals. Consider scrolling or limiting visible rows to 10 per table.
