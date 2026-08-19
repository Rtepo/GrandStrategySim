# Phase 35 â€” Finance Audit: Social Welfare Black Hole, Election Deadlock & Public Finance

A read-only audit of the simulation codebase revealing six root-cause defects and a plan for their remediation.

**Approved Design Decisions:**
- Part 1: Ministry Cash Account model (keep pre-debit, add `ministry_cash` field, spending debits from pocket)
- Part 2: Full per-region GDP accounting (track C+G+I+NX per region)
- Part 4: Banking micro-loans (B2B + B2C with `debt` tracking) + DSPW Primary Dealer status + CB QE for deflation fighting
- Part 6: Horizontal scrolling tab header (preserve vertical space)

**Architectural Corrections Applied (per user directive):**
- **CB Open Market Operations (QE):** When CPI < 0%, the Central Bank MUST purchase Sovereign Bonds from DSPW Banks on the secondary market, printing fresh M0. This is the only way to organically expand the monetary base and fight deflation.
- **No Free Money in B2C Loans:** Consumer loans MUST track principal via a `debt: f64` field on `ClassDemographics`. When a bank issues a loan, `savings` and `debt` increase equally. Every turn, the class repays principal + interest from `savings`, with interest flowing back to the issuing bank.

---

## PART 1: The Social Welfare Cash Leak (CRITICAL)

### 1.1 Root Cause: DebtIssuance Bypasses the `allocated_cash` Cap

**File:** `state/src/politics/social_programs.rs`, lines 517â€“624 (`execute_social_programs`)

The Ministry of Social Welfare was allocated `322.2K` but spent `42.31M`. The leak is in the `DebtIssuance` funding path:

```rust
// Line 549: available = allocated_cash - spent_cash (the correct cap)
let available = ministry.allocated_cash - ministry.spent_cash;
let funding = resolve_funding_dilemma(
    evaluation.total_cost,  // e.g. 42.31M
    available,              // e.g. 322.2K
    ruling_ideology, unrest, fiscal_health,
);

// Lines 558-574: actual_payout is set to evaluation.total_cost, NOT capped by available!
let (actual_payout, debt_issued) = match funding {
    FundingResponse::FullyFunded => (evaluation.total_cost, 0.0),
    FundingResponse::Haircut { payout_ratio } => {
        (evaluation.total_cost * payout_ratio, 0.0)  // OK â€” ratio = available/cost
    }
    FundingResponse::DebtIssuance { shortfall } => {
        // Issues sovereign debt for the shortfall, then pays the FULL total_cost
        issue_treasury_securities(country, shortfall, current_turn);
        (evaluation.total_cost, shortfall)  // BUG: total_cost >> available!
    }
};

// Line 616: spent_cash inflated to total_cost (42.31M), far beyond allocated_cash (322.2K)
ministry.spent_cash += actual_payout;
```

**The `DebtIssuance` path triggers when** (`resolve_funding_dilemma`, lines 438â€“498):
- The ideology is populist-left (Marxism, SocialDemocracy, GreenPolitics, etc.), OR
- Social unrest > 60, OR
- Centrist ideology AND fiscal_health â‰Ą 0.15.

When triggered, the ministry pays the **full program cost** (`evaluation.total_cost`) regardless of its `allocated_cash` cap. The shortfall is covered by issuing sovereign debt (`issue_treasury_securities`), which moves cash from bank reserves into `liquid_reserves`, then the full `total_cost` is debited from `liquid_reserves` and credited to `ClassDemographics.savings`.

### 1.2 The Systemic Double-Entry Bug

**File:** `state/src/politics/ministries.rs`, lines 521â€“554 (`allocate_cash_to_ministries`)

The cash leak is compounded by a **systemic double-debit** across the entire ministry spending system:

1. **At allocation** (line 552â€“553): `country.budget.liquid_reserves -= total_allocated` â€” cash is moved OUT of liquid_reserves into the ministry's conceptual "pocket."
2. **At spending** (lines 678, 714, 729, 776, and social_programs.rs lines 591/612): `country.budget.liquid_reserves -= spend` â€” cash is debited AGAIN from liquid_reserves.

The **Healthcare/Education branch** (line 746) correctly notes: *"allocate_cash_to_ministries already debited liquid_reserves, so we do NOT debit here"* â€” it routes through `ministry_public_service_pool` instead. But **all other branches** (B2B Procurement, Subsidies, Infrastructure, DirectTransfer, and `execute_social_programs`) debit `liquid_reserves` a second time.

**Net effect:** Total hit to `liquid_reserves` = `allocated_cash` (at allocation) + `spend` (at execution). When `spend â‰ allocated_cash`, the treasury is debited **2Ă— the intended amount**.

### 1.3 Fix Plan: Ministry Cash Account Model (Approved)

**Approach: Keep the pre-debit, add a `ministry_cash` field, spending debits from `ministry_cash` instead of `liquid_reserves`.**

This preserves the existing allocation timing (cash moves to the ministry's pocket at allocation time) and eliminates the double-debit by making all spending functions debit from the ministry's own cash account rather than `liquid_reserves` again.

**Step 1: Add `ministry_cash` field to `Ministry` struct.**
```rust
// state/src/politics/ministries.rs, Ministry struct (line 285+):
/// Cash currently held by the ministry (debited from liquid_reserves at allocation).
/// All spending debits from this field, NOT from liquid_reserves.
#[serde(default)]
pub ministry_cash: f64,
```

**Step 2: Credit `ministry_cash` at allocation time.**
In `allocate_cash_to_ministries` (line 545â€“553), after setting `ministry.allocated_cash`:
```rust
for ministry in &mut config.ministries {
    let allocated = ministry.allocated_cash * ratio;
    ministry.allocated_cash = allocated;
    ministry.ministry_cash = allocated;  // NEW: credit the pocket
    ministry.spent_cash = 0.0;
    ministry.spending_actions.clear();
}
let total_allocated: f64 = config.ministries.iter().map(|m| m.allocated_cash).sum();
country.budget.liquid_reserves -= total_allocated;  // Pre-debit stays
```

**Step 3: All spending functions debit `ministry.ministry_cash` instead of `liquid_reserves`.**
In `execute_ministry_spending` (lines 618â€“781), replace every `country.budget.liquid_reserves -= spend` with `ministry.ministry_cash -= spend`:
- B2B Procurement (line 678): `ministry.ministry_cash -= encumbrance;`
- Subsidies (line 714): `ministry.ministry_cash -= actual;`
- Infrastructure (line 729): `ministry.ministry_cash -= actual;`
- DirectTransfer (line 776): `ministry.ministry_cash -= actual;`
- Healthcare/Education (line 749â€“752): `ministry.ministry_cash -= actual; country.ministry_public_service_pool += actual;` (now debits the pocket, not liquid_reserves)

**Step 4: Fix `execute_social_programs` â€” cap `actual_payout` at `available` and debit `ministry_cash`.**
```rust
// Line 549: available is the cap
let available = ministry.allocated_cash - ministry.spent_cash;

// Cap actual_payout at available in ALL funding paths:
let actual_payout = actual_payout.min(available);
if actual_payout <= 0.0 { continue; }

// Debit ministry_cash, NOT liquid_reserves:
ministry.ministry_cash -= actual_payout;
// (Remove the country.budget.liquid_reserves -= benefit lines at 591/612)
```

**Step 5: Remove or strictly limit the `DebtIssuance` path.**
Ministries should NOT issue sovereign debt beyond their allocation. Replace `DebtIssuance` with a `Haircut` when the program costs more than `available`:
```rust
// In resolve_funding_dilemma, replace DebtIssuance with Haircut:
if is_populist_left || social_unrest > 60.0 {
    return FundingResponse::Haircut {
        payout_ratio: available_cash / total_cost,
    };
}
```
This ensures `spent_cash` never exceeds `allocated_cash`.

### 1.4 Files to Modify
- `state/src/politics/ministries.rs` â€” Add `ministry_cash` field to `Ministry`; credit it in `allocate_cash_to_ministries`; change all spending branches to debit `ministry_cash` instead of `liquid_reserves`.
- `state/src/politics/social_programs.rs` â€” Cap `actual_payout` at `available`; debit `ministry_cash` instead of `liquid_reserves`; replace `DebtIssuance` with `Haircut`.

---

## PART 2: Population & GDP Aggregation Mismatch

### 2.1 Root Cause: National Population Drifts Independently of Regional Populations

**File:** `state/src/economy/labor/labor.rs`, lines 139â€“398 (`process_demographics_and_labor`)

The national population (`country.budget.population`) is recomputed each turn from births, deaths, and migration:

```rust
// Line 159-163:
let births = population * birth_rate_index;
let natural_deaths = population * winter_death_rate;
let migrants = population * migracja_wsk;
let population_change = births - natural_deaths - zgony_w_pracy + migrants;
let new_population = (population + population_change).max(1.0).floor() as u64;

// Line 398: Updates NATIONAL counter only
budget.population = new_population;
```

**Regional populations (`region.population`) are NOT updated** to reflect these births/deaths/migrations. The loop at lines 406â€“417 only updates `demo.available_fte` from the existing (stale) `demo.population` â€” it never reconciles `demo.population` or `region.population` with the new national total.

**Migration** (`state/src/economy/labor/migration.rs`, lines 379â€“397) also updates `budget.population` without touching regional populations.

**Regional population IS updated in only two narrow cases:**
- Construction casualties (`turn.rs` line 1384): `region.population -= dead`
- Ethnic violence (`ethnic_violence.rs` line 432): `region.population -= reduction`

**Result:** `sum(region.population) â‰  country.budget.population`. The national counter evolves; regional counters stay frozen at generation time (minus occasional casualties/violence).

### 2.2 Root Cause: Regional GDP Is Frozen at Generation

**File:** `state/src/society/geography.rs`, line 1358 â€” `region.gdp` is set at world generation.

**File:** `state/src/engine/turn.rs`, lines 3741â€“3746 â€” National GDP is recomputed each turn:
```rust
country.macro_indicators.gdp_breakdown.official_gdp =
    consumption + government_spending + investment + net_exports;
country.budget.gdp = country.macro_indicators.gdp_breakdown.official_gdp;
```

**`region.gdp` is NEVER updated during the simulation.** It stays frozen at the initial generation value. The national GDP evolves via C+G+I+NX aggregation, but regional GDP does not. This is why `sum(region.gdp) â‰  country.budget.gdp`.

### 2.3 Fix Plan: Full Per-Region GDP Accounting (Approved)

**Goal:** National data is STRICTLY DERIVED by summing Regional data. Full per-region tracking of C+G+I+NX.

**Step 1: Distribute national population changes to regions.**
After `process_demographics_and_labor` computes `new_population`, distribute the delta (`new_population - old_population`) proportionally across regions based on their current population share:
```rust
let delta = new_population as f64 - old_population as f64;
for region in &mut country.regions {
    let share = region.population as f64 / old_population as f64;
    let region_delta = (delta * share).round() as i64;
    region.population = (region.population + region_delta).max(0);
    // Also distribute to class demographics proportionally
}
```
This ensures `sum(region.population) == budget.population` after every turn.

**Step 2: Add per-region GDP accumulator.**
Create a `RegionalGdpAccumulator` struct (in `state/src/economy/telemetry.rs`):
```rust
#[derive(Debug, Clone, Default)]
pub struct RegionalGdpAccumulator {
    pub consumption: f64,       // C: B2C retail revenue by region
    pub government_spending: f64, // G: ministry spending by region
    pub investment: f64,        // I: construction materials consumed by region
    pub net_exports: f64,       // NX: exports minus imports by region
}

impl RegionalGdpAccumulator {
    pub fn official_gdp(&self) -> f64 {
        self.consumption + self.government_spending + self.investment + self.net_exports
    }
}
```

**Step 3: Tag GDP components with their region of origin.**
The `GdpAccumulator` (telemetry.rs) currently aggregates nationally. Add a `regional: HashMap<String, RegionalGdpAccumulator>` field to `GdpAccumulator` (or to `CountryTask`), and populate it at each GDP-accumulating site:

- **C (Consumption):** In B2C retail clearing (`retail.rs`), tag each retail transaction with the company's `region_id`. Accumulate `quantity * execution_price` into `regional[region_id].consumption`.
- **G (Government spending):** In ministry spending (`ministries.rs`), tag each spending action with the target region (for infrastructure: the building's region; for subsidies: the company's region; for social programs: the eligible class's region). Accumulate into `regional[region_id].government_spending`.
- **I (Investment):** In `advance_construction_projects` (`construction/orders.rs`), tag the `cost_spent` delta with the project's `region_id`. Accumulate into `regional[region_id].investment`.
- **NX (Net exports):** In trade balance computation (`turn.rs` line 3738), tag each country's net exports by the port/region of origin. If per-region trade routing is too complex, distribute NX proportionally by region GDP share as a fallback.

**Step 4: Reconcile at end of turn.**
After all GDP components are accumulated:
```rust
for region in &mut task.ctx.country.regions {
    if let Some(acc) = task.gdp_acc.regional.get(&region.id) {
        region.gdp = acc.official_gdp();
    }
}
// National GDP = sum of regional GDP (strict derivation)
task.ctx.country.budget.gdp = task.ctx.country.regions.iter().map(|r| r.gdp).sum();
task.ctx.country.macro_indicators.gdp_breakdown.official_gdp = task.ctx.country.budget.gdp;
```

**Step 5: Reconciliation assertion.**
Add a debug assertion at the end of each turn:
```rust
debug_assert_eq!(country.regions.iter().map(|r| r.population).sum::<i64>(), country.budget.population);
let regional_gdp_sum: f64 = country.regions.iter().map(|r| r.gdp).sum();
debug_assert!((regional_gdp_sum - country.budget.gdp).abs() < 1.0);
```

### 2.4 Files to Modify
- `state/src/economy/telemetry.rs` â€” Add `RegionalGdpAccumulator` struct; add `regional: HashMap<String, RegionalGdpAccumulator>` to `GdpAccumulator`.
- `state/src/economy/labor/labor.rs` â€” Distribute population delta to regions after computing `new_population`.
- `state/src/economy/labor/migration.rs` â€” Distribute migration flows to regional populations.
- `state/src/economy/trade/retail.rs` â€” Tag B2C consumption by `region_id` into the regional accumulator.
- `state/src/politics/ministries.rs` â€” Tag government spending by target region into the regional accumulator.
- `state/src/construction/orders.rs` â€” Tag construction investment by project `region_id` into the regional accumulator.
- `state/src/engine/turn.rs` â€” Reconcile `region.gdp` from regional accumulator; set `budget.gdp = sum(region.gdp)`.

---

## PART 3: Election Deadlock & Political Capital = 0.0

### 3.1 Root Cause: `process_political_year` Runs Every Turn Instead of Once Per Year

**File:** `state/src/engine/turn.rs`, line 2826

```rust
tasks.par_iter_mut().for_each(|task| {
    process_political_year(task.ctx.country, &mut task.companies, &mut task.unions, task.ctx.year);
});
```

This call is **unconditional** â€” it runs every turn (24 turns/year). But `process_political_year` decrements `years_to_elections` by 1 each call (line 155â€“157):

```rust
if country.politics.years_to_elections > 0 {
    country.politics.years_to_elections -= 1;
}
```

With `election_cycle() = 4`, elections fire every **4 turns** (~2 months), not every 4 years. The election timer ticks 24Ă— too fast.

**File:** `state/src/engine/turn.rs`, line 3859 â€” Year only increments every 24 turns:
```rust
if turn > 0 && turn % 24 == 0 {
    year += 1;
}
```

**The fix:** Gate `process_political_year` to run only once per year:
```rust
if turn > 0 && turn % 24 == 0 {
    tasks.par_iter_mut().for_each(|task| {
        process_political_year(task.ctx.country, &mut task.companies, &mut task.unions, task.ctx.year);
    });
}
```

This ensures `years_to_elections` decrements once per year, and elections fire every `election_cycle` years (not turns).

### 3.2 Root Cause: Political Capital = 0.0 from Payroll Failure Cascade

**File:** `state/src/politics/turn.rs`, line 392 â€” `political_capital` is regenerated:
```rust
country.politics.political_capital = 50.0 + ruling_support * 0.5 * coalition_stability;
```
This sets political_capital to â‰Ą50 every call (since `ruling_support` defaults to 50.0, `coalition_stability` is 0.5 or 1.0).

**File:** `state/src/engine/turn.rs`, line 4643 â€” Payroll failure subtracts 20:
```rust
country.politics.political_capital = (country.politics.political_capital - 20.0).max(0.0);
```
This triggers when `liquid_reserves < total_payroll` (line 4638). Due to the Social Welfare cash leak (Part 1), `liquid_reserves` is chronically depleted, so payroll fails **every turn**.

**The cascade:** With `process_political_year` running every turn:
- Turn 1: `political_capital = 75` (from ppy) â†’ payroll fails â†’ `political_capital = 55`
- Turn 2: `political_capital = 75` (from ppy) â†’ payroll fails â†’ `political_capital = 55`
- ...oscillates around 55, never 0.

**But** if `process_political_year` is fixed to run once per year (Part 3.1), then:
- Year 1, Turn 1: `political_capital = 75` (from ppy, runs once)
- Turns 2â€“24: payroll fails each turn â†’ `75 â†’ 55 â†’ 35 â†’ 15 â†’ 0 â†’ 0 â†’ ... â†’ 0`
- `political_capital` stays at 0 for ~20 turns until the next yearly reset.

**This explains the user's observation:** Political Capital is permanently frozen at 0.0 because the yearly reset is drowned out by 23 consecutive payroll failures.

**The fix:** Fixing Part 1 (Social Welfare cash leak) will restore `liquid_reserves`, allowing payroll to succeed, which stops the political_capital drain. Additionally, the payroll failure penalty should be **per-year, not per-turn** â€” move the payroll check into the yearly political block, or reduce the per-turn penalty to `20.0 / 24.0` (~0.83/turn) so the yearly total is still 20.

### 3.3 Root Cause: Provisional Government Persistence

**File:** `state/src/politics/turn.rs`, lines 372â€“479 (`regenerate_parties`)

Phase 34 added a safety net (lines 52â€“105) to inject â‰Ą3 parties for democracies. However, the provisional government stub is still created with `..Party::default()` (empty leader name) when `total_support == 0.0`. The safety net injects additional parties, but if the injected parties have very low support (10.0), the provisional government can still win the election via `calculate_seats`.

**The fix:** Once `process_political_year` runs once per year (Part 3.1), the election cycle will be correct. The safety net from Phase 34 should be verified to work with the corrected timing. If the provisional government still persists, increase the injected parties' initial support to 25â€“30 (from 10) to ensure competitive elections.

### 3.4 Files to Modify
- `state/src/engine/turn.rs` â€” Gate `process_political_year` to run only when `turn % 24 == 0`; move payroll failure penalty to yearly cadence (or scale it per-turn).
- `state/src/politics/turn.rs` â€” Verify/increase injected party support in the safety net.

---

## PART 4: Ghost Sectors & Bank Logic

### 4.1 NGO/Religion: Mass Hiring/Firing Loop

**File:** `state/src/corporate/manager.rs`, lines 873â€“948 (`set_wage_offers`)

Phase 34 fixed the 1M wage cap (now 3Ă— market average, floor 5000) and added a 0.4 wage budget fraction for charities. But the wage offer is still based on **current** `brokerage_cash`:
```rust
let brokerage_cash = company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
let wage_budget = brokerage_cash * fraction;
let computed_wage = wage_budget / effective_fte;
```

When a charity receives a large donation, `brokerage_cash` spikes â†’ high wage â†’ mass hiring. Next turn, the donation is spent â†’ `brokerage_cash` drops to ~0 â†’ wage = 0 â†’ all workers leave/fired. This creates the hiring/firing loop.

### 4.2 Fix Plan: Endowment/Smoothing Mechanic

**Step 1: Add a rolling donation history to Company.**
Add a field to `Company`:
```rust
#[serde(default)]
pub donation_history: VecDeque<f64>,  // Last 6 turns of donations received
```

**Step 2: Track donations.** In the charity donation distribution code (`state/src/society/charities.rs` and `religious_economy.rs`), push the donation amount to `company.donation_history` each turn. Keep only the last 6 entries.

**Step 3: Use rolling average for wage calculation.** In `set_wage_offers`, for NGO/Religion sectors:
```rust
if company.sector == Sector::NGO || company.sector == Sector::Religion {
    let rolling_avg_donation = if company.donation_history.is_empty() {
        brokerage_cash
    } else {
        company.donation_history.iter().sum::<f64>() / company.donation_history.len() as f64
    };
    // Use rolling average instead of current cash for wage budget
    let wage_budget = rolling_avg_donation * fraction;
    let computed_wage = wage_budget / effective_fte;
}
```

This smooths the wage offer over 6 turns, preventing the spike-and-crash pattern. Charities hire based on their **sustained** income, not a one-time donation windfall.

### 4.3 Banking: Dead at 0 Employees â€” No Operational Loop

**File:** `state/src/engine/generator/mod.rs`, lines 919â€“936 (`build_bank_companies`)

Phase 34 fixed the `region_id` assignment (banks now get the capital region). But banks still have **no operational B2B loop to earn revenue**. They sit on `operating_cash` but don't generate income, so they can't afford to hire tellers.

**File:** `state/src/state/banking.rs`, lines 1898+ (`process_banking_turn`)

The banking turn handles interbank clearing, CB facilities, loan issuance, and deposit insurance. Banks earn interest on loans (`bs.reserves_at_central_bank += interest_paid` in manager.rs line 85). But this interest accrues to `balance_sheet.reserves_at_central_bank`, NOT to `brokerage_account.cash` (which is what `set_wage_offers` checks for wage affordability).

**The disconnect:** Bank revenue â†’ `balance_sheet.reserves_at_central_bank` (not spendable on wages). Wage affordability â†’ `brokerage_account.cash` (stays at 0 because no revenue flows there).

### 4.4 Fix Plan: Micro-Loans, DSPW Primary Dealers & Operational Revenue (Approved)

**Per user directive:** Banks must implement actual micro-loans (B2B and B2C) tracked on balance sheets, AND introduce a Primary Dealer (DSPW) status â€” only DSPW banks can purchase sovereign bonds directly at auctions.

**Step 1: Implement B2B Micro-Loans.**
In `process_banking_turn`, after the existing loan issuance step (Step 7), add a micro-loan issuance phase for companies that need short-term working capital:
```rust
// For each non-bank company with insufficient brokerage_account.cash for next turn's B2B:
//   - Bank evaluates creditworthiness (fixed_capital, financial_history)
//   - If approved, issue a micro-loan: bank.brokerage_account.cash â†’ company.brokerage_account.cash
//   - Track on bank.balance_sheet as a Loan entry
//   - Company repays principal + interest over N turns (deducted from company cash)
```
The interest rate = XIBOR + bank_margin + risk_premium (existing `calculate_loan_rate` logic). The loan is tracked as a `Loan` struct on the bank's balance sheet and on the company's `outstanding_loan_bank_id`.

**Step 2: Implement B2C Micro-Loans (Consumer Credit) with Debt Tracking.**
Banks issue small consumer loans to class demographics with savings below a threshold. **Strict rule: track the principal â€” no free money.**

**Step 2a: Add `debt` field to `ClassDemographics`.**
```rust
// state/src/society/geography.rs, ClassDemographics struct (after line 852):
/// Phase 35: Outstanding consumer debt (principal owed to banks).
/// When a bank issues a consumer loan, `savings` increases AND `debt` increases
/// equally (double-entry). Every turn, the class pays down `debt` plus interest
/// from `savings`, with interest flowing back to the issuing bank as B2C revenue.
#[serde(rename = "zadĹ‚uĹĽenie_konsumenckie", default)]
pub debt: f64,
```

**Step 2b: Loan issuance (double-entry).**
```rust
// For each region, for each class demographic with per_capita_savings < threshold:
//   - Bank evaluates creditworthiness (savings, economic_status, population)
//   - If approved, issue a consumer loan:
//     class.savings += principal    // Credit savings (money to spend)
//     class.debt   += principal    // Debt increases equally (no free money)
//     bank.brokerage_account.cash -= principal  // Bank lends real cash
//     bank.balance_sheet.outstanding_consumer_loans += principal  // Track on BS
//   - Record the loan: principal, interest_rate, remaining_turns, issuing_bank_id
```

**Step 2c: Loan repayment (every turn).**
```rust
// For each class with debt > 0:
//   - Calculate repayment: principal_portion + interest_portion
//   - class.savings -= (principal_portion + interest_portion)  // Deduct from savings
//   - class.debt   -= principal_portion                        // Reduce principal
//   - issuing_bank.brokerage_account.cash += (principal_portion + interest_portion)  // Bank receives
//   - issuing_bank.balance_sheet.outstanding_consumer_loans -= principal_portion
//   - The interest_portion flows to the bank as B2C revenue (â†’ operating cash for wages)
```

This creates a sustainable B2C revenue stream: banks lend real cash â†’ classes spend it (stimulating C) â†’ classes repay principal + interest from wages â†’ banks earn interest income â†’ banks can hire tellers. The `debt` field ensures repayments are grounded in actual principal, not phantom money.

**Step 3: Transfer loan interest income to `brokerage_account.cash`.**
At the end of `process_banking_turn`, for each bank:
```rust
// Sum all interest received this turn from loan repayments
let loan_interest_income = result.total_loan_interest_received;
// Transfer 30% to operating cash for wages
let operating_transfer = loan_interest_income * 0.3;
if let Some(ref mut ba) = bank.brokerage_account {
    ba.cash += operating_transfer;
}
if let Some(ref mut bs) = bank.balance_sheet {
    bs.reserves_at_central_bank -= operating_transfer;
}
```

**Step 4: DSPW Primary Dealer Status.**
The `DebtMarket` already has a `dspw_enabled: bool` field and `dspw_dealers: Vec<String>` (bank company IDs). Enforce that **only DSPW banks** can purchase sovereign bonds at primary auctions:
```rust
// In issue_treasury_securities (debt_market.rs line 360+):
//   - If dspw_enabled: only banks in dspw_dealers can bid
//   - DSPW banks are legally obligated to absorb the entire issue at a price discount
//   - Non-DSPW banks can only buy on the secondary market
```
Assign DSPW status at generation: the 2â€“3 largest banks in each country get DSPW status. DSPW banks earn higher yields (price discount) but bear the obligation to absorb issuances.

**Step 5: Set `target_fte_demand` for banks** based on their balance sheet size and loan portfolio:
```rust
let loan_count = bs.outstanding_loans.len();
let asset_based_fte = (bs.total_assets() / 1_000_000.0).min(30.0);
let loan_based_fte = (loan_count as f64 / 10.0).min(20.0);
bank.target_fte_demand = (asset_based_fte + loan_based_fte).max(5.0);
```
Banks with more loans need more tellers/loan officers. This creates a feedback loop: more loans â†’ more FTE demand â†’ more hiring â†’ more operational capacity â†’ more loans.

### 4.5 Files to Modify
- `state/src/entities/mod.rs` â€” Add `donation_history: VecDeque<f64>` to `Company`.
- `state/src/society/charities.rs` â€” Track donations to `company.donation_history`.
- `state/src/economy/religion/religious_economy.rs` â€” Track donations to `company.donation_history`.
- `state/src/corporate/manager.rs` â€” Use rolling average donation for NGO/Religion wage calculation.
- `state/src/state/banking.rs` â€” Add B2B micro-loan issuance; add B2C consumer loan issuance with debt tracking; transfer loan interest to `brokerage_account.cash`; set `target_fte_demand` from loan portfolio.
- `state/src/economy/finance/debt_market.rs` â€” Enforce DSPW-only primary auction participation; verify DSPW obligation logic.
- `state/src/engine/generator/mod.rs` â€” Assign DSPW status to largest banks at generation.
- `state/src/society/geography.rs` â€” Add `debt: f64` field to `ClassDemographics`.

### 4.6 Central Bank Open Market Operations (QE) for Deflation Fighting (Approved)

**The Problem:** Citizen Savings steadily fall, PPI is frozen, and the economy suffers chronic deflation. Because government spending relies solely on taxes and finite domestic bonds, the total monetary base (M0) is frozen. As population grows, money per capita shrinks â€” a deflationary death spiral.

**The Strict Rule:** The Central Bank MUST create new M0 money to target inflation. If `CPI < 0%` (deflation), the CB must dynamically purchase Sovereign Bonds from the Secondary Market (from DSPW Banks), printing fresh M0 into commercial bank reserves. This is the only way to organically expand the monetary base.

**Existing Infrastructure:**
- `state/src/state/central_bank.rs` already has `execute_omo` (line 385) â€” but it only targets XIBOR (interest rate), NOT deflation. It compares `current_xibor` to `omo_target_rate` and buys/sells bonds accordingly.
- `state/src/state/central_bank.rs` already has `omo_bond_holdings` (line 166) â€” tracks bonds held by CB from OMO purchases.
- `state/src/economy/finance/debt_market.rs` has `SecondaryMarketState` (line 250) with buy/sell order matching.
- `state/src/state/banking.rs` line 1931 calls `execute_omo` and physically adjusts bank reserves/securities (lines 1940â€“1963).

**What's Missing:** A deflation-triggered QE path that fires when `CPI < 0%`, independent of the XIBOR-targeting OMO. The current OMO only adjusts for interest rate gaps, not for deflation.

**Fix Plan:**

**Step 1: Add a `execute_deflation_qe` method to `CentralBank`.**
```rust
// state/src/state/central_bank.rs:
/// Phase 35: Deflation-fighting Quantitative Easing.
/// When CPI < 0% (deflation), the CB purchases sovereign bonds from DSPW banks
/// on the secondary market, printing fresh M0 into their reserves.
///
/// # Arguments
/// * `cpi_inflation` - Current CPI inflation rate (percent, e.g., -0.5 = -0.5%)
/// * `total_bank_securities` - Total sovereign bonds held by commercial banks
/// * `gdp` - Current official GDP (for scaling the QE operation)
/// * `current_turn` - Current turn number
///
/// # Returns
/// Amount of bonds purchased (positive = M0 injected). 0.0 if no deflation.
///
/// # Rules
/// * Only fires when `cpi_inflation < 0.0` (deflation).
/// * QE size scales with deflation severity: `qe_amount = gdp * |cpi_inflation| / 100 * qe_multiplier`
///   where `qe_multiplier` is a policy parameter (default 2.0).
/// * Capped at `total_bank_securities` (can't buy more bonds than banks hold).
/// * Capped at a maximum percentage of GDP (e.g., 5%) per turn to prevent hyperinflation risk.
/// * The CB prints fresh M0 â€” this is NOT a transfer from existing reserves.
///   The CB's balance sheet expands: `omo_bond_holdings += qe_amount`.
///   Bank reserves increase: `bs.reserves_at_central_bank += qe_amount`.
///   Bank securities decrease: `bs.securities -= qe_amount`.
pub fn execute_deflation_qe(
    &mut self,
    cpi_inflation: f64,
    total_bank_securities: f64,
    gdp: f64,
    current_turn: u32,
) -> f64 {
    if cpi_inflation >= 0.0 || total_bank_securities <= 0.0 || gdp <= 0.0 {
        return 0.0;
    }

    let qe_multiplier = 2.0; // Policy parameter
    let deflation_severity = cpi_inflation.abs(); // e.g., 0.5 for -0.5%
    let mut qe_amount = gdp * deflation_severity / 100.0 * qe_multiplier;

    // Cap at 5% of GDP per turn
    let max_qe = gdp * 0.05;
    qe_amount = qe_amount.min(max_qe);

    // Cap at available bank securities
    qe_amount = qe_amount.min(total_bank_securities);

    if qe_amount > 0.0 {
        self.omo_bond_holdings += qe_amount;
        self.omo_last_operation_turn = current_turn;
        self.omo_last_operation_amount = qe_amount;
        self.last_message = format!(
            "[QE] Deflation detected (CPI: {:.2}%). Purchased {:.0} in sovereign bonds from secondary market. M0 expanded.",
            cpi_inflation, qe_amount
        );
    }

    qe_amount
}
```

**Step 2: Call `execute_deflation_qe` in `process_banking_turn`, AFTER the existing OMO.**
```rust
// state/src/state/banking.rs, after line 1963 (after the existing OMO execution):

// Phase 35: Deflation-fighting QE â€” independent of XIBOR targeting.
// Fires when CPI < 0%, printing fresh M0 by purchasing bonds from banks.
let cpi_inflation = country.macro_indicators.inflation_indices.cpi_inflation;
let gdp = country.budget.gdp;
let qe_amount = country.central_bank.execute_deflation_qe(
    cpi_inflation,
    total_bank_securities,
    gdp,
    current_turn,
);
result.qe_amount = qe_amount;  // Add to BankingTurnResult

// Physically execute QE: adjust each DSPW bank's reserves and securities proportionally.
if qe_amount > 0.0 {
    // Only purchase from DSPW banks (primary dealers hold the most securities)
    let dspw_bank_ids = &country.debt_market.dspw_dealers.clone();
    let proportion = if total_bank_securities > 0.0 {
        qe_amount / total_bank_securities
    } else {
        0.0
    };
    for bank in companies.iter_mut() {
        if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
            // Prioritize DSPW banks for QE purchases
            let is_dspw = dspw_bank_ids.contains(&bank.id);
            if !is_dspw { continue; }
            let bank_share = bs.securities * proportion;
            let amount = bank_share.min(bs.securities);
            bs.securities -= amount;
            bs.reserves_at_central_bank += amount;  // Fresh M0 printed into reserves
        }
    }
}
```

**Step 3: Add `qe_amount` to `BankingTurnResult`.**
```rust
// state/src/state/banking.rs, BankingTurnResult struct:
/// Phase 35: Total QE bond purchases (fresh M0 printed for deflation fighting).
#[serde(default)]
pub qe_amount: f64,
```

**Step 4: Verify M0 expansion.**
The `calculate_m0` method (central_bank.rs line 201) computes `M0 = cash_in_circulation + bank_reserves`. Since QE increases `bs.reserves_at_central_bank`, M0 expands automatically. The money supply computation in `turn.rs` (line 3700+) will pick up the expanded reserves.

**Monetary Physics Summary:**
- **Before QE:** M0 is frozen (taxes + bonds only). Population grows â†’ money per capita shrinks â†’ deflation.
- **After QE:** When CPI < 0%, CB prints fresh M0 by buying bonds from DSPW banks. Bank reserves increase â†’ banks can lend more â†’ M0 expands â†’ money per capita stabilizes â†’ deflation reverses.
- **Inflation guard:** QE is capped at 5% of GDP per turn and only fires during deflation (CPI < 0%). Once CPI turns positive, QE stops automatically.

---

## PART 5: Megaregions & UI Purge

### 5.1 Megaregions Never Loaded into `country.megaregions`

**File:** `state/src/engine/generator/mod.rs`, line 239 â€” `country.megaregions: Vec::new()` (empty at generation).

**File:** `state/src/engine/turn.rs`, lines 4032â€“4049 (`load_regions_into_state`) â€” Loads regions from `regions.json` into `country.regions`, but there is **NO equivalent `load_megaregions_into_state`**. The `country.megaregions` vector is NEVER populated from `megaregions.json`.

**File:** `state/src/ui/snapshot.rs`, lines 582â€“587 â€” The snapshot searches `country.megaregions` (which is empty), so every region shows "Unassigned".

**Fix Plan:** Add a `load_megaregions_into_state` function (mirroring `load_regions_into_state`):
```rust
fn load_megaregions_into_state(data_dir: &Path, state: &mut GameState) -> Result<(), TurnError> {
    let path = data_dir.join("megaregions.json");
    if !path.exists() { return Ok(()); }
    let text = fs::read_to_string(&path)?;
    let all_megaregions: HashMap<String, Megaregion> = serde_json::from_str(&text)?;
    for country in state.countries.values_mut() {
        country.megaregions = all_megaregions.values()
            .filter(|m| m.owner_country == country.name)
            .cloned()
            .collect();
    }
    Ok(())
}
```
Call it right after `load_regions_into_state` (turn.rs line 235).

### 5.2 Ideology Enum Still in Polish

**File:** `state/src/politics/ideology.rs`, lines 33â€“91

The `Ideology` enum uses Polish serde renames:
```rust
#[serde(rename = "Socjalliberalizm")]
SocialLiberalism,
```
And `as_str()` returns Polish names (line 74â€“92). The UI displays these Polish strings.

**Fix Plan:** Change all serde renames to English and update `as_str()`:
```rust
#[serde(rename = "Social Liberalism")]
SocialLiberalism,
// ... etc.
```
**Save compatibility:** This will break loading existing saves that have Polish ideology names in `politics.json`. Add a migration in `load_country_data` (or a `from_name` fallback) that accepts BOTH Polish and English names:
```rust
pub fn from_name(name: &str) -> Option<Self> {
    // Try English first, then Polish fallback for old saves
    serde_json::from_str(&format!("\"{name}\"")).ok()
        .or_else(|| polish_to_english(name))
}
```
Create a `polish_to_english` lookup map for backward compatibility.

### 5.3 Region Name Generator

**File:** `state/src/society/geography.rs`, line 1343 â€” Regions are named `format!("{country}-Region{}", i + 1)` (e.g., "Nordia-Region1").

**Fix Plan:** Create a region name generator that produces realistic names based on the country's cultural group and geography:
```rust
fn generate_region_name(country: &str, cultural_group: &str, index: usize, is_capital: bool, rng: &mut impl Rng) -> String {
    if is_capital {
        return capital_name(country, cultural_group);  // e.g., "Nordia Capital District"
    }
    // Pick from a cultural name pool + geographic suffix
    let prefix = cultural_name_pool(cultural_group).choose(rng).unwrap();
    let suffix = geographic_suffix(rng).choose(rng).unwrap();  // "Valley", "Coast", "Highlands"
    format!("{} {}", prefix, suffix)
}
```
Store the generated name in `Region.display_name` (or `Region.name`) and use it in the snapshot instead of `r.id`.

### 5.4 Government Tab UI: "Ministry of Energy Minister ()"

**File:** `state/src/ui/snapshot.rs`, lines 686â€“708

Phase 34 added a `strip_prefix("Ministry of ")` fix (line 688) and a fallback for empty minister names (line 693â€“705). However:
1. The fallback produces `format!("Minister ({})", m.minister_party)` â€” when `minister_party` is empty (provisional government), this shows **"Minister ()"**.
2. The `strip_prefix` may not be applied if the ministry name doesn't start with exactly "Ministry of " (e.g., "Ministry of Heavy Industry" works, but any deviation doesn't).

**The root cause ties to Part 3:** The election deadlock means the provisional government's ministries are never refreshed with real leaders/party IDs, so `minister_party` stays empty.

**Fix Plan:**
1. **Improve the fallback:** When `minister_party` is empty, show "Vacant" instead of "Minister ()":
   ```rust
   if m.minister_name.is_empty() {
       if m.minister_party.is_empty() {
           "Vacant".to_string()
       } else {
           format!("Minister ({})", m.minister_party)
       }
   }
   ```
2. **Drop redundant words in the table header:** The header says "Ministry" and "Minister" (lines 430â€“431). Since the ministry name column already shows "Energy" (after strip_prefix), and the minister column shows the name, the headers are fine. But ensure the strip_prefix handles ALL ministry name formats (including "Ministry of Heavy Industry" â†’ "Heavy Industry", not just single-word suffixes).
3. **Display Full Names properly:** For the PM and Head of State rows, ensure the name fallback also uses "Vacant" instead of empty strings.

### 5.5 Files to Modify
- `state/src/engine/turn.rs` â€” Add `load_megaregions_into_state` and call it after `load_regions_into_state`.
- `state/src/politics/ideology.rs` â€” Change serde renames to English; update `as_str()`; add `polish_to_english` fallback.
- `state/src/society/geography.rs` â€” Add `generate_region_name` function; use it in `generate_regional_topology`.
- `state/src/ui/snapshot.rs` â€” Fix minister name fallback to "Vacant" when party is empty; use `display_name` for regions.
- `state/src/ui/tui/render.rs` â€” (Minor) verify government tab column widths and headers.

---

## PART 6: New `[9] Finance` Tab & Scrollable UI

### 6.1 New `[9] Finance` Tab

**File:** `state/src/ui/tui/tabs.rs` â€” Add `Tab::Finance` variant:
```rust
pub enum Tab {
    MacroFinance,        // [1]
    MarketLogistics,     // [2]
    ConstructionGeology, // [3]
    SocietyJustice,      // [4]
    Sectors,             // [5]
    Government,          // [6]
    Parliament,          // [7]
    Regions,             // [8]
    Finance,             // [9] NEW
}
```
Update `ALL`, `title()` ("Finance"), `hotkey()` ('9').

### 6.2 Finance Tab Data

**File:** `state/src/ui/snapshot.rs` â€” Add a `FinanceSnapshot` struct:
```rust
pub struct FinanceSnapshot {
    // Budget Revenues (Tax breakdown)
    pub pit_revenue: f64,
    pub cit_revenue: f64,
    pub vat_revenue: f64,
    pub wealth_tax_revenue: f64,
    pub capital_gains_tax_revenue: f64,
    pub total_tax_revenue: f64,
    pub taxes_evaded: f64,
    // Budget Expenditures
    pub ministry_allocations: f64,   // sum of ministry allocated_cash
    pub ministry_spent: f64,         // sum of ministry spent_cash
    pub debt_service_costs: f64,     // from debt_market
    pub total_expenditures: f64,
    // Sovereign Debt
    pub sovereign_debt: f64,         // debt_market.total_outstanding_debt
    pub sovereign_debt_to_gdp: f64,  // debt / gdp * 100
    pub weighted_avg_interest_rate: f64,
    // Shadow Economy
    pub shadow_gdp: f64,
    pub estimated_tax_loss: f64,     // shadow_gdp * effective_tax_rate
}
```

**Data sources:**
- **Tax breakdown:** `TaxCollectionResult` (tax.rs line 1173) â€” currently a local variable in `process_tax_collection_turn` (turn.rs line 2650), NOT persisted on `Country`. **Fix:** Add `pub last_tax_result: TaxCollectionResult` to `Country` and store it.
- **Debt data:** `country.debt_market.total_outstanding_debt` and `weighted_avg_interest_rate` (debt_market.rs lines 300â€“302).
- **Ministry spending:** Sum from `country.politics.ministry_config.ministries`.
- **Shadow GDP:** `country.macro_indicators.gdp_breakdown.shadow_gdp`.
- **Estimated tax loss:** `shadow_gdp * effective_tax_rate` where `effective_tax_rate = (pit + cit + vat) / official_gdp`.

### 6.3 Finance Tab Renderer

**File:** `state/src/ui/tui/render.rs` â€” Add `render_finance(snap)`:
- Section 1: Budget Revenues (PIT, CIT, VAT, Wealth, Capital Gains, Total, Taxes Evaded)
- Section 2: Budget Expenditures (Ministry Allocations, Ministry Spent, Debt Service, Total)
- Section 3: Sovereign Debt (Outstanding, Debt/GDP %, Avg Interest Rate)
- Section 4: Shadow Economy (Shadow GDP, Estimated Tax Loss)

### 6.4 Horizontal Scrolling Tab Header (Approved)

**File:** `state/src/ui/tui/app.rs`, lines 1428â€“1447

The current tab bar uses `Tabs::new(tab_titles)` from ratatui, which renders all tabs in a single row. With 9 tabs at ~10 chars each (" Finance [9] "), that's ~90 chars + dividers, overflowing 80-column terminals.

**Fix Plan (Horizontal Scrolling â€” preserves vertical space):**

Replace the `Tabs` widget with a custom scrolling header that shows only the tabs that fit the terminal width, with `â€ą`/`â€ş` scroll indicators:

**Step 1: Add scroll state to `App`.**
```rust
// state/src/ui/tui/app.rs, App struct:
pub tab_scroll_offset: usize,  // Index of the first visible tab
```

**Step 2: Compute visible tabs based on terminal width.**
```rust
let available_width = chunks[0].width as usize;
let tab_width = 12;  // " Finance [9] " â‰ 12 chars
let visible_count = (available_width.saturating_sub(4) / tab_width).max(1);  // -4 for borders/indicators

// Auto-scroll so the active tab is always visible
let active_idx = Tab::ALL.iter().position(|&t| t == app.active_tab).unwrap_or(0);
if active_idx < app.tab_scroll_offset {
    app.tab_scroll_offset = active_idx;
} else if active_idx >= app.tab_scroll_offset + visible_count {
    app.tab_scroll_offset = active_idx - visible_count + 1;
}

let end = (app.tab_scroll_offset + visible_count).min(Tab::ALL.len());
let visible_tabs = &Tab::ALL[app.tab_scroll_offset..end];
```

**Step 3: Render with scroll indicators.**
```rust
let mut spans: Vec<Span> = Vec::new();
if app.tab_scroll_offset > 0 {
    spans.push(Span::styled("â€ą ", Style::default().fg(Color::Yellow)));
}
for (i, t) in visible_tabs.iter().enumerate() {
    let actual_idx = app.tab_scroll_offset + i;
    let title = format!(" {} [{}] ", t.title(), t.hotkey());
    let style = if *t == app.active_tab {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    spans.push(Span::styled(title, style));
    if i < visible_tabs.len() - 1 {
        spans.push(Span::raw("|"));
    }
}
if end < Tab::ALL.len() {
    spans.push(Span::styled(" â€ş", Style::default().fg(Color::Yellow)));
}

let header = Paragraph::new(Line::from(spans))
    .block(Block::default().borders(Borders::ALL).title(country_label));
f.render_widget(header, chunks[0]);
```

**Step 4: Add scroll keybindings.**
In the key event handler, add:
- `Shift+Left` / `H`: scroll tabs left (`tab_scroll_offset = tab_scroll_offset.saturating_sub(1)`)
- `Shift+Right` / `L`: scroll tabs right (`tab_scroll_offset = (tab_scroll_offset + 1).min(Tab::ALL.len() - 1)`)
- Number keys `1`â€“`9`: jump directly to a tab (auto-scrolls to make it visible)

The header height stays at `Constraint::Length(3)` â€” no vertical space lost.

### 6.5 Files to Modify
- `state/src/ui/tui/tabs.rs` â€” Add `Tab::Finance` variant; update `ALL`, `title()`, `hotkey()`.
- `state/src/ui/snapshot.rs` â€” Add `FinanceSnapshot` struct; add `finance: FinanceSnapshot` to `CountrySnapshot`; build it in `build_country_snapshot`.
- `state/src/state/mod.rs` (or `Country` definition) â€” Add `pub last_tax_result: TaxCollectionResult` field.
- `state/src/engine/turn.rs` â€” Store `tax_result` into `country.last_tax_result`.
- `state/src/ui/tui/render.rs` â€” Add `render_finance`; update `render_tab_content` match.
- `state/src/ui/tui/app.rs` â€” Replace `Tabs` widget with wrapping `Paragraph`; increase header height.

---

## Implementation Steps (Ordered)

### Step 1: Fix Social Welfare Cash Leak (CRITICAL)
- **File:** `state/src/politics/ministries.rs` â€” Add `ministry_cash` field to `Ministry`; credit it in `allocate_cash_to_ministries`; change all spending branches to debit `ministry_cash` instead of `liquid_reserves`.
- **File:** `state/src/politics/social_programs.rs` â€” Cap `actual_payout` at `available`; debit `ministry_cash` instead of `liquid_reserves`; replace `DebtIssuance` with `Haircut`.
- **Tests:** Ministry `spent_cash` never exceeds `allocated_cash`; `ministry_cash` never goes negative; `liquid_reserves` is debited exactly once (at allocation); social programs with shortfall apply Haircut not DebtIssuance.

### Step 2: Fix Election Timing (CRITICAL)
- **File:** `state/src/engine/turn.rs` â€” Gate `process_political_year` to run only when `turn % 24 == 0`.
- **File:** `state/src/engine/turn.rs` â€” Scale payroll failure penalty to per-turn (20.0/24.0) or move to yearly block.
- **Tests:** `years_to_elections` decrements once per year; elections fire every `election_cycle` years; political_capital regenerates yearly and doesn't cascade to 0.

### Step 3: Fix Population & GDP Reconciliation (Full Per-Region Accounting)
- **File:** `state/src/economy/telemetry.rs` â€” Add `RegionalGdpAccumulator` struct; add `regional` map to `GdpAccumulator`.
- **File:** `state/src/economy/labor/labor.rs` â€” Distribute population delta to regions.
- **File:** `state/src/economy/labor/migration.rs` â€” Distribute migration to regional populations.
- **File:** `state/src/economy/trade/retail.rs` â€” Tag B2C consumption by `region_id`.
- **File:** `state/src/politics/ministries.rs` â€” Tag government spending by target region.
- **File:** `state/src/construction/orders.rs` â€” Tag construction investment by project `region_id`.
- **File:** `state/src/engine/turn.rs` â€” Reconcile `region.gdp` from regional accumulator; set `budget.gdp = sum(region.gdp)`.
- **Tests:** `sum(region.population) == budget.population`; `sum(region.gdp) == budget.gdp` (within 1.0 tolerance); regional GDP components sum to national components.

### Step 4: Fix Megaregion Loading
- **File:** `state/src/engine/turn.rs` â€” Add `load_megaregions_into_state`; call after `load_regions_into_state`.
- **Tests:** `country.megaregions` is non-empty after load; regions show correct megaregion in snapshot.

### Step 5: Translate Ideology Enum to English
- **File:** `state/src/politics/ideology.rs` â€” Change serde renames to English; update `as_str()`; add `polish_to_english` fallback.
- **Tests:** `from_name("Social Liberalism")` works; `from_name("Socjalliberalizm")` still works (backward compat); `as_str()` returns English.

### Step 6: Region Name Generator
- **File:** `state/src/society/geography.rs` â€” Add `generate_region_name`; use in `generate_regional_topology`.
- **Tests:** Region names are not "Region1"; capital region has a distinct name.

### Step 7: NGO/Religion Endowment Smoothing
- **File:** `state/src/entities/mod.rs` â€” Add `donation_history: VecDeque<f64>` to `Company`.
- **File:** `state/src/society/charities.rs` â€” Track donations.
- **File:** `state/src/economy/religion/religious_economy.rs` â€” Track donations.
- **File:** `state/src/corporate/manager.rs` â€” Use rolling average for NGO/Religion wages.
- **Tests:** NGO wage offer based on 6-turn average, not current cash; no mass hiring/firing loop.

### Step 8: Banking Operational Loop (Micro-Loans + DSPW + QE)
- **File:** `state/src/society/geography.rs` â€” Add `debt: f64` field to `ClassDemographics`.
- **File:** `state/src/state/banking.rs` â€” Add B2B micro-loan issuance; add B2C consumer loan issuance with `debt` tracking (savings += principal, debt += principal, repay principal+interest each turn); transfer loan interest to `brokerage_account.cash`; set `target_fte_demand` from loan portfolio; add `qe_amount` to `BankingTurnResult`.
- **File:** `state/src/state/central_bank.rs` â€” Add `execute_deflation_qe` method (CPI < 0% â†’ buy bonds from DSPW banks, print fresh M0).
- **File:** `state/src/economy/finance/debt_market.rs` â€” Enforce DSPW-only primary auction participation.
- **File:** `state/src/engine/generator/mod.rs` â€” Assign DSPW status to largest banks at generation.
- **Tests:** Bank `brokerage_account.cash` > 0 after turn; bank `target_fte_demand` > 0; bank employment > 0 after several turns; micro-loans tracked on balance sheet; `ClassDemographics.debt` tracks consumer loan principal; repayments reduce `debt` and `savings`; only DSPW banks participate in primary auctions; QE fires when CPI < 0%; QE expands M0; QE stops when CPI â‰Ą 0%.

### Step 9: Government Tab UI Cleanup
- **File:** `state/src/ui/snapshot.rs` â€” Fix minister name fallback to "Vacant"; use `display_name` for regions.
- **Tests:** Minister name shows "Vacant" not "Minister ()"; ministry name shows "Energy" not "Ministry of Energy".

### Step 10: New [9] Finance Tab
- **File:** `state/src/ui/tui/tabs.rs` â€” Add `Tab::Finance`.
- **File:** `state/src/ui/snapshot.rs` â€” Add `FinanceSnapshot`; build it.
- **File:** `state/src/state/mod.rs` â€” Add `last_tax_result` to `Country`.
- **File:** `state/src/engine/turn.rs` â€” Store `tax_result` on country.
- **File:** `state/src/ui/tui/render.rs` â€” Add `render_finance`.
- **Tests:** Finance tab renders; tax breakdown shows; debt/GDP shows; shadow tax loss shows.

### Step 11: Horizontal Scrolling Tab Header
- **File:** `state/src/ui/tui/app.rs` â€” Add `tab_scroll_offset` to `App`; replace `Tabs` widget with scrolling `Paragraph`; add scroll keybindings (Shift+Left/Right).
- **Tests:** All 9 tabs accessible on 80-column terminal; active tab always visible; scroll indicators show when tabs are hidden; number keys 1-9 jump to tabs.

### Step 12: Build & Test Verification
- `cargo build --lib`
- `cargo build`
- `cargo test --lib`
- Manual 48-turn (2-year) simulation checks:
  - Ministry `spent_cash` â‰¤ `allocated_cash` (no cash leak)
  - `ministry_cash` â‰Ą 0 (never negative)
  - `liquid_reserves` debited exactly once per allocation (no double-debit)
  - Elections fire every `election_cycle` years (not turns)
  - Political Capital > 0 (not frozen at 0.0)
  - `sum(region.population) == national population`
  - `sum(region.gdp) == official GDP` (within 1.0 tolerance)
  - Megaregions show in Regions tab (not "Unassigned")
  - Ideology names in English (not Polish)
  - Region names are realistic (not "Region1")
  - NGO/Religion employment stable (no mass hire/fire loop)
  - Banking employment > 0
  - Bank balance sheet shows outstanding micro-loans
  - `ClassDemographics.debt` tracks consumer loan principal (savings += principal, debt += principal)
  - Consumer loan repayments reduce `debt` and `savings`, interest flows to bank
  - Only DSPW banks participate in sovereign bond auctions
  - QE fires when CPI < 0% (CB buys bonds from DSPW banks, M0 expands)
  - QE stops when CPI â‰Ą 0% (no hyperinflation risk)
  - M0 expands during deflation, stabilizing money per capita
  - Finance tab shows tax breakdown, debt/GDP, shadow tax loss
  - All 9 tabs accessible on 80-column terminal (horizontal scrolling)
  - Tab scroll indicators (â€ą â€ş) appear when tabs are hidden
  - Number keys 1-9 jump to correct tabs

---

## Risks & Considerations

1. **Save compatibility (Ideology enum):** Changing serde renames will break existing saves. The `polish_to_english` fallback in `from_name` is essential. Old saves with Polish ideology names in `politics.json` must still load.

2. **Ministry Cash Account model (Part 1):** Adding `ministry_cash` to `Ministry` requires `#[serde(default)]` for backward compatibility. The pre-debit from `liquid_reserves` stays, but spending now debits `ministry_cash` â€” this eliminates the double-debit. All spending branches must be updated consistently. The Healthcare/Education branch must now debit `ministry_cash` (not skip the debit as it currently does).

3. **Election timing (Part 3):** Moving `process_political_year` to yearly cadence means interest group power, party regeneration, lobbying, and legislation all run once per year instead of every turn. This is the INTENDED behavior (these are yearly processes), but it may change simulation dynamics. Verify that no other code depends on `process_political_year` running every turn.

4. **Full per-region GDP accounting (Part 2):** Tagging every GDP component with its region of origin is a significant change touching retail clearing, ministry spending, and construction. The NX (net exports) component is hardest to regionalize â€” if per-region trade routing is too complex, distribute NX proportionally by region GDP share as a fallback. Debug assertions will catch any drift.

5. **Population reconciliation (Part 2):** Distributing the population delta proportionally assumes uniform birth/death rates across regions. In reality, some regions grow faster. A more accurate model would compute per-region birth/death rates, but the proportional approach is sufficient for Phase 35.

6. **Donation history (Part 4):** Adding `VecDeque<f64>` to `Company` increases save size slightly. The `#[serde(default)]` attribute ensures old saves without this field load correctly.

7. **Banking micro-loans, DSPW & QE (Part 4):** B2B and B2C micro-loans create new money (loan issuance creates deposits). Verify the money supply doesn't inflate uncontrollably. The DSPW obligation means DSPW banks MUST absorb sovereign debt issuances â€” if a DSPW bank is insolvent, the auction fails. Add a fallback: if all DSPW banks are insolvent, the CB directly monetizes the debt (with a penalty to credit rating). The QE mechanism prints fresh M0 during deflation â€” verify it doesn't overshoot into hyperinflation once CPI turns positive. The 5%-of-GDP cap per turn and the CPI < 0% trigger are the guards.

8. **Consumer debt tracking (Part 4):** The `debt` field on `ClassDemographics` must be `#[serde(default)]` for backward compatibility with old saves. If a class's `savings` drops below the repayment amount, the repayment should be partial (pay what's available, roll over the rest). Track defaulted consumer loans on the bank's balance sheet as non-performing loans (NPLs).

9. **Horizontal scrolling tab header (Part 6):** The scroll state (`tab_scroll_offset`) must be reset when switching countries or loading a new game. Number keys 1-9 must auto-scroll to make the target tab visible.

8. **Horizontal scrolling tab header (Part 6):** The scroll state (`tab_scroll_offset`) must be reset when switching countries or loading a new game. Number keys 1-9 must auto-scroll to make the target tab visible.

9. **`last_tax_result` persistence (Part 6):** Adding `TaxCollectionResult` to `Country` increases save size. Use `#[serde(default)]` for backward compatibility. The result is overwritten each turn, so it doesn't grow unboundedly.

10. **DebtIssuance removal (Part 1):** Replacing the DebtIssuance path with Haircut means social programs are strictly capped at the ministry's allocation. This may cause social unrest to rise faster (programs can't overspend). Monitor unrest levels in testing. If unrest spirals, consider allowing the Treasury (not the ministry) to issue supplementary debt for social programs via a separate budget bill.
