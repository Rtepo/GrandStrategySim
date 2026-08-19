# Phase 16: Financial Audit & Banking System Blueprint

> **Status:** DRAFT — Awaiting USER Approval  
> **Date:** 2025-01-XX  
> **Rule:** NO IMPLEMENTATION until explicit approval. This document is a plan only.

---

## Table of Contents

1. [Part 1: Dependency Audit — Financial Black Holes](#part-1-dependency-audit)
2. [Part 2: Monetary Engine Blueprint](#part-2-monetary-engine-blueprint)
3. [Appendix: File Reference Index](#appendix-file-reference-index)

---

## Part 1: Dependency Audit — Financial Black Holes

This section catalogs every location in the codebase where fiat money may disappear from or appear in the economy without proper double-entry accounting. Each finding is classified by severity:

- **CRITICAL** — Money is created or destroyed with no offsetting entry. The money supply is directly corrupted.
- **HIGH** — Money moves to/from a ledger that is disconnected from the main economy. The money is stranded or phantom.
- **MEDIUM** — Money flow uses a placeholder or simplified formula that may not conserve money mass under edge conditions.
- **LOW** — Money flow is technically correct but uses a proxy field (`available_cash`) that may not reflect actual liquid capital, creating timing-of-sync risks.

---

### 1.1 B2C Retail Market Clearing — No Cash Settlement [CRITICAL]

**File:** `economy/retail.rs:183-272` (`clear_b2c_markets`)

**Problem:** The function allocates goods to consumers and tracks `units_sold` per store, but **never transfers money**. Citizens receive goods without paying. Stores track sales but never receive revenue. The `building.reserve` field is never updated.

**Flow:**
- Consumer demand is built from `consumption_registry` × population.
- Store offers are generated from inventory with markup pricing.
- Market clearing allocates demand to offers by utility (price + inertia).
- `units_sold` is updated per store.
- **MISSING:** `citizen_savings -= total_cost`, `building.reserve += total_cost`.

**Impact:** Every turn, goods flow from stores to consumers but no money flows back. This is a one-way valve: producers spend money on inputs and wages, goods reach consumers for free. Money drains from companies and never returns via consumer spending. The economy collapses into deflation as money pools in citizen savings with no return path to companies.

**Fix Required:** After allocating `units_sold` to each store, compute `revenue = units_sold × price_per_unit`, debit citizen savings (per class, pro-rata by population), and credit `building.reserve`. If citizens cannot afford goods, demand must be clamped to affordable quantity.

---

### 1.2 B2C Services (Education/Health) — Disconnected Citizen Savings [CRITICAL]

**File:** `economy/b2c_services.rs:109-266` (`clear_education_slots_b2c`, `clear_health_capacity_b2c`)

**Problem:** These functions accept `citizen_savings: &mut BTreeMap<String, f64>` and `local_governments: &mut BTreeMap<String, f64>` — **ephemeral maps** that are constructed in the turn loop and discarded. They are NOT connected to:
- `region.class_demographics.rural_classes[*].savings` / `urban_classes[*].savings`
- `country.budget.citizen_savings`
- Any persistent treasury ledger

**Flow:**
- Citizen pays for education/health: `*citizen_savings -= total_cost`, `building.reserve += total_cost`.
- Government subsidizes: `*gov_cash -= subsidy_amount`, `building.reserve += subsidy_amount`.
- **BUT:** The `citizen_savings` map is a temporary BTreeMap created in the turn loop. Changes are lost after the turn.

**Impact:** Money debited from citizens vanishes. Money credited to `building.reserve` persists, but the debit side is lost. This creates money from nothing (building.reserve increases without a corresponding citizen savings decrease that persists).

**Fix Required:** The `citizen_savings` parameter must be connected to actual `ClassDemographics.savings` fields. Either pass `&mut region.class_demographics` directly, or aggregate and write back after clearing.

---

### 1.3 Warehouse Storage Fees — No Creditor [CRITICAL]

**File:** `economy/b2b_orders.rs:678-683, 709-713` (`execute_production_cycle`)

**Problem:** Storage fees are deducted from `company.brokerage_account.cash` but **never credited to the warehouse owner**. The money simply disappears.

**Flow:**
- Company has warehouse inventory exceeding capacity.
- Storage fee = `overflow_quantity × storage_rate`.
- `brokerage_account.cash -= storage_fee`.
- **MISSING:** `warehouse_owner.brokerage_account.cash += storage_fee`.

**Impact:** Every turn, companies with warehouse inventory lose money to storage fees with no recipient. This is pure money destruction.

**Fix Required:** Identify the warehouse owner (logistics company) from the building registry and credit the storage fee to their brokerage account. If the warehouse is state-owned, credit to `country.budget.liquid_reserves`.

---

### 1.4 Maintenance Spending — Money Zeroed, Not Transferred [CRITICAL]

**File:** `economy/maintenance.rs:106-112` (`process_maintenance_spending`)

**Problem:** When a company pays for maintenance, the cost is deducted from `company.available_cash` but **never credited to anyone**. In the full-cost case, `company.available_cash -= cost`. In the partial case, `company.available_cash = 0.0`. No contractor, supplier, or worker receives the payment.

**Flow:**
- Building condition degrades over time.
- Company pays for maintenance restoration.
- `company.available_cash -= cost` (or zeroed).
- `building.condition += restoration`.
- **MISSING:** Credit to maintenance contractor / materials supplier.

**Impact:** Maintenance spending is pure money destruction. Companies bleed cash to maintain buildings with no economic counterparty.

**Fix Required:** Maintenance costs should be split into labor (credited to worker savings) and materials (credited to supplier company via B2B market). Alternatively, credit to a state maintenance fund if state-performed.

---

### 1.5 Waste Collection Operating Costs — No Creditor [CRITICAL]

**File:** `utilities/waste_collection.rs:130-134`

**Problem:** `company.available_cash = (company.available_cash - op_cost).max(0.0)` — operating cost is deducted but never credited to anyone.

**Impact:** Same as maintenance — pure money destruction.

**Fix Required:** Credit operating costs to the waste processing company's revenue or to workers.

---

### 1.6 Maritime Maintenance — No Creditor [CRITICAL]

**File:** `infrastructure/maritime.rs:497-500`

**Problem:** `maritime.available_cash = (maritime.available_cash - total_maintenance).max(0.0)` — same pattern. Maintenance cost disappears.

**Impact:** Pure money destruction.

**Fix Required:** Credit to shipyard/maintenance contractor.

---

### 1.7 Public Administration Wages — No Creditor [HIGH]

**File:** `engine/turn.rs:648-652`

**Problem:** Treasury deducts maintenance and wages for state buildings: `liquid_reserves -= maintenance + wages`. But the wages are **never credited to citizen savings**. The maintenance portion also has no creditor.

**Flow:**
- State buildings (schools, hospitals, courts, etc.) require maintenance and wages.
- Treasury pays: `liquid_reserves -= maintenance + wages`.
- **MISSING:** Citizen savings += wages (workers at state buildings).
- **MISSING:** Contractor/supplier += maintenance.

**Impact:** Government spending leaks money. Fiscal stimulus is deflationary instead of inflationary because the money vanishes instead of reaching citizens.

**Fix Required:** Split the deduction: wages credited to regional class demographics (public workers), maintenance credited to suppliers.

---

### 1.8 Interbank Market — Proportional Distribution Imprecision [MEDIUM]

**File:** `state/banking.rs:455-481` (`InterbankMarket::clear_market`)

**Problem:** The interbank clearing uses proportional allocation but the comments admit "simplified proportional distribution." Each surplus bank lends `position × (transfer_amount / total_surplus)` to each deficit bank in equal shares (`lend_amount / deficit_banks.len()`). This can create imbalances:

1. The sum of `per_borrower_amount` across all borrowers may not equal `lend_amount` due to floating-point rounding.
2. Each deficit bank receives from ALL surplus banks, but the total received may not equal its deficit.
3. `interbank_loans_given` and `interbank_loans_taken` HashMaps track aggregate amounts, not bilateral relationships. The sum of `interbank_loans_given` across all banks may not equal the sum of `interbank_loans_taken`.

**Impact:** Small money creation/destruction each turn due to rounding. Over many turns, this compounds.

**Fix Required:** Use exact bilateral matching with integer cents or fixed-point arithmetic. Ensure `Σ(interbank_loans_given) == Σ(interbank_loans_taken)` after clearing.

---

### 1.9 BFG Premium Collection — Double-Entry Violation [HIGH]

**File:** `state/banking.rs:1037-1043` (`BfgFund::collect_premiums`)

**Problem:** Premiums debit BOTH `reserves_at_central_bank` (Asset) AND `tier_1_capital` (Equity). This is incorrect double-entry.

**Correct flow:** Premium payment is an expense. The bank pays from its reserves (Asset decreases). The BFG receives reserves (Asset increases at BFG). The bank's Equity should NOT change — the expense reduces retained earnings, which is already captured by the reserve decrease. By also debiting `tier_1_capital`, the bank's balance sheet will show:
- Assets: decreased by `premium`
- Liabilities + Equity: decreased by `premium` (via tier_1_capital)
- But the BFG received `premium` — so the total system has `premium` less than before.

**Impact:** Money is destroyed equal to the premium amount. The bank's balance sheet stays balanced in isolation, but the system-wide money mass decreases.

**Fix Required:** Only debit `reserves_at_central_bank`. Do NOT debit `tier_1_capital`. The premium is a transfer of reserves, not a capital loss.

---

### 1.10 BFG/SOBK Emergency Liquidity — Untracked M0 Expansion [HIGH]

**File:** `state/banking.rs:1060-1070, 1250-1258`

**Problem:** `receive_cb_liquidity_line` adds to `reserves` (or `pool`) and `cb_emergency_loan`, but the comment says "In full implementation: CB creates new reserves." The Central Bank's `liquidity_injected` field is **NOT updated**. M0 expands but is untracked.

**Flow:**
- BFG calls `receive_cb_liquidity_line(&mut central_bank, amount)`.
- `self.cb_emergency_loan += amount`, `self.reserves += amount`.
- **MISSING:** `central_bank.liquidity_injected += amount`.
- **MISSING:** No actual reserve creation at the CB.

**Impact:** Money is created from nothing without CB tracking. M0 is undercounted.

**Fix Required:** Update `central_bank.liquidity_injected += amount` and create corresponding reserves in the CB's balance sheet.

---

### 1.11 Bank Resolution — BFG Can Go Negative [HIGH]

**File:** `state/banking.rs:1462-1467`

**Problem:** BFG pays out for uninsured deposits, interbank loans, and Lombard debt by debiting `bfg_fund.reserves`. There is **no guard** for `reserves < 0`. If BFG reserves are insufficient, the fund goes negative — money is created from nothing.

**Flow:**
- `bfg_fund.reserves -= toxic_uninsured` (line 1462)
- `bfg_fund.reserves -= (toxic_interbank_total + toxic_lombard)` (line 1466)
- No check: `if bfg_fund.reserves < 0 { /* trigger CB emergency loan or treasury subsidy */ }`

**Impact:** Bank failures can create unlimited money if BFG reserves are insufficient.

**Fix Required:** Before paying out, check if BFG has sufficient reserves. If not, trigger `receive_cb_liquidity_line` or `receive_state_subsidy` to replenish. Only then pay creditors.

---

### 1.12 Bank Resolution — CB Lombard Repayment Not Credited [MEDIUM]

**File:** `state/banking.rs:1456`

**Problem:** `central_bank.liquidity_injected -= toxic_lombard` reduces the CB's tracking, but the Lombard repayment money is not credited anywhere. Where does the money go?

**Flow:**
- Failed bank had `cb_lombard_loans = X`.
- `central_bank.liquidity_injected -= X`.
- **MISSING:** No credit to CB balance sheet. The money repaid vanishes.

**Impact:** M0 contracts by the Lombard amount but the money isn't received by the CB. It's destroyed.

**Fix Required:** Credit the repayment to the CB's reserve account or a dedicated CB income account.

---

### 1.13 Bankruptcy Liquidation — Arbitrary Tax Payment [MEDIUM]

**File:** `corporate/bankruptcy.rs:319-325`

**Problem:** Tax payment is hardcoded as 10% of seized cash: `let tax_payment = total_seized_cash * 0.10`. This is a placeholder — it does not reflect actual unpaid taxes.

**Impact:** Over/under-payment of taxes. If the company owed more than 10%, the treasury is shortchanged. If less, the treasury receives windfall money (from the company's perspective, but the money was already the company's).

**Fix Required:** Track actual unpaid tax liabilities per company. Use real tax debt in the waterfall.

---

### 1.14 Bankruptcy Liquidation — Bank Repayment is Simplified [MEDIUM]

**File:** `corporate/bankruptcy.rs:328-348`

**Problem:** Bank exposure is `bank.issued_loans / num_banks` — equal distribution regardless of actual exposure. No per-company loan tracking.

**Impact:** Banks that never lent to the bankrupt company receive payments. Banks that did lend may not be repaid. Money is misallocated.

**Fix Required:** Track per-company loan exposure in the `Loan` struct's `borrower_id` field. Use actual outstanding balances per bank.

---

### 1.15 Bankruptcy Liquidation — Shareholder Residual Stranded [LOW]

**File:** `corporate/bankruptcy.rs:351-356`

**Problem:** Residual cash after all creditors are paid is tracked in `creditor_distributions["shareholders"]` but never actually transferred to any shareholder entity.

**Impact:** Small amounts of money are stranded in the distribution map. Not a money creation/destruction issue, but a money trapping issue.

**Fix Required:** Identify shareholders from `company.owners` and credit their accounts.

---

### 1.16 Debt Market — Sovereign Default Capitalization [MEDIUM]

**File:** `economy/debt_market.rs:698-715`

**Problem:** When the treasury defaults, unpaid interest is capitalized into bondholder principals: `holder.quantity += unpaid_holder`. This means the bondholder's principal increases — new money is created (the unpaid interest becomes new debt). While this is standard sovereign default practice, the capitalized amount is not tracked as new money supply.

**Impact:** M3 increases via capitalized interest without explicit CB authorization. This is realistic but should be tracked.

**Fix Required:** Track capitalized arrears separately in money supply calculations.

---

### 1.17 Charity Available Cash Sync Risk [LOW]

**File:** `society/charities.rs:147, 263`

**Problem:** Donations are credited to `company.available_cash` and distributions debit from `company.available_cash`. This field is synced from `brokerage_account.cash` at the start of B2B order submission (`b2b_orders.rs:179`). If a charity company is not processed through B2B (it's an NGO, not a trading company), the `available_cash` may be overwritten to 0 on the next sync.

**Impact:** Charity money may be lost when `available_cash` is resynced from `brokerage_account.cash` (which was never updated).

**Fix Required:** Either: (a) credit/debit `brokerage_account.cash` directly for charities, or (b) ensure charities are exempt from the `available_cash` resync.

---

### 1.18 Inspectorate Fines & State Forest Remittance — Available Cash Proxy [LOW]

**Files:** `economy/inspectorates.rs`, `economy/state_forests.rs`

**Problem:** Fines and remittances use `company.available_cash` instead of `brokerage_account.cash`. The `available_cash` field is a snapshot synced at B2B order submission time. If fines/remittances are processed after B2B sync but before the next sync, the `available_cash` may not reflect actual liquid capital.

**Impact:** Companies may be fined/remitted for more than they actually have in their brokerage account, or less. The `available_cash` field can go negative (it's just an f64) while `brokerage_account.cash` remains positive.

**Fix Required:** Use `brokerage_account.cash` for all financial deductions. Reserve `available_cash` for read-only display purposes.

---

### 1.19 Defense Procurement — Encumbrance Without Settlement [MEDIUM]

**File:** `engine/turn.rs:1438-1442`

**Problem:** Treasury encumbers cash for defense bids: `liquid_reserves -= total_encumbered`. Bids are stored in `pending_defense_orders`. But the actual settlement (paying defense contractors for delivered goods) is not visible in the audited code. If settlement never happens, the encumbered money is destroyed.

**Impact:** If defense orders are never settled, treasury money vanishes.

**Fix Required:** Implement defense order settlement: when defense goods are delivered, credit the defense contractor's account and clear the pending order.

---

### 1.20 Production Cycle — Zeroed Revenue/Costs [HIGH]

**File:** `economy/b2b_orders.rs:768-775` (`execute_production_cycle`)

**Problem:** `input_costs` and `output_revenue` are explicitly set to 0.0 with comments "Costs already settled via B2B trades" and "Revenue already settled via B2B trades." But B2B trades only handle inter-company wholesale transactions. The final consumer sale (B2C) is handled by `retail.rs:clear_b2c_markets` which **does not transfer money** (see 1.1). Therefore, companies never receive revenue for goods sold to consumers.

**Impact:** Companies spend money on inputs (via B2B) and wages (via labor market), produce goods, sell them to consumers (via B2C), but never receive payment. The company's only revenue source is B2B sales to other companies. This creates a systemic money drain: money flows from companies → workers → citizens, but never returns from citizens → companies.

**Fix Required:** Fix B2C clearing (1.1) to transfer consumer payments to store/building owners. Then update `output_revenue` in the production cycle to reflect B2C revenue.

---

### 1.21 Overflow Inventory Destruction [MEDIUM]

**File:** `economy/b2b_orders.rs:747-759`

**Problem:** When warehouse capacity is exceeded and no warehouse is available, excess inventory is destroyed. The goods are destroyed but the money spent to produce them (inputs purchased via B2B, wages paid) has already left the producer's account. This is a real-world equivalent of burning cash.

**Impact:** Goods worth money are destroyed. The money spent to produce them is already in the economy (paid to suppliers and workers), so this doesn't destroy money directly. But it destroys economic value, which should be recorded as a write-down (loss) on the company's books.

**Fix Required:** Record inventory destruction as a financial loss event. Consider fire-sale pricing instead of destruction (route to bankruptcy auction pool at discount).

---

### 1.22 Wage Payment — No PIT Withholding [MEDIUM]

**File:** `economy/labor_market.rs:288-296`

**Problem:** Wages are debited from company `brokerage_account.cash` and credited to class `savings`, but there is no Personal Income Tax (PIT) withholding at source. The `tax_rates.pit` configuration exists but is never applied during wage payment.

**Impact:** PIT is never collected from wages. The treasury loses a major revenue source. If PIT is collected elsewhere (e.g., at year-end), the timing mismatch means money sits in citizen savings instead of the treasury for many turns.

**Fix Required:** Apply PIT withholding during wage payment: `wage_gross → wage_net = wage_gross × (1 - pit_rate)`, credit `wage_net` to class savings, credit `wage_gross × pit_rate` to `country.budget.liquid_reserves`.

---

### 1.23 Summary of Black Holes

| # | Severity | Location | Description |
|---|----------|----------|-------------|
| 1.1 | CRITICAL | retail.rs:183-272 | B2C clearing: no cash transfer from citizens to stores |
| 1.2 | CRITICAL | b2c_services.rs:109-266 | B2C services: citizen savings map is ephemeral, changes lost |
| 1.3 | CRITICAL | b2b_orders.rs:678-713 | Warehouse storage fees debited, never credited |
| 1.4 | CRITICAL | maintenance.rs:106-112 | Maintenance cost debited, never credited |
| 1.5 | CRITICAL | waste_collection.rs:130-134 | Waste op-cost debited, never credited |
| 1.6 | CRITICAL | maritime.rs:497-500 | Maritime maintenance debited, never credited |
| 1.7 | HIGH | turn.rs:648-652 | Public wages deducted from treasury, never credited to citizens |
| 1.8 | MEDIUM | banking.rs:455-481 | Interbank proportional distribution rounding |
| 1.9 | HIGH | banking.rs:1037-1043 | BFG premiums: double-debit (asset + equity) destroys money |
| 1.10 | HIGH | banking.rs:1060-1070 | CB emergency liquidity: M0 expansion untracked |
| 1.11 | HIGH | banking.rs:1462-1467 | BFG can go negative (money creation from nothing) |
| 1.12 | MEDIUM | banking.rs:1456 | CB Lombard repayment not credited |
| 1.13 | MEDIUM | bankruptcy.rs:319-325 | Arbitrary 10% tax payment in liquidation |
| 1.14 | MEDIUM | bankruptcy.rs:328-348 | Bank repayment simplified (equal distribution) |
| 1.15 | LOW | bankruptcy.rs:351-356 | Shareholder residual stranded in distribution map |
| 1.16 | MEDIUM | debt_market.rs:698-715 | Sovereign default: capitalized interest untracked in M3 |
| 1.17 | LOW | charities.rs:147,263 | Charity available_cash may be overwritten by B2B sync |
| 1.18 | LOW | inspectorates.rs, state_forests.rs | Fines/remittances use available_cash proxy |
| 1.19 | MEDIUM | turn.rs:1438-1442 | Defense encumbrance without settlement |
| 1.20 | HIGH | b2b_orders.rs:768-775 | Production revenue zeroed, B2C revenue never credited |
| 1.21 | MEDIUM | b2b_orders.rs:747-759 | Overflow inventory destroyed without write-down |
| 1.22 | MEDIUM | labor_market.rs:288-296 | No PIT withholding at wage payment source |

**Total CRITICAL: 6** | **HIGH: 5** | **MEDIUM: 8** | **LOW: 3**

---

## Part 2: Monetary Engine Blueprint

This section designs the complete banking system architecture, covering:
- Central Bank (M0/M1) base money creation and destruction
- Commercial bank (M3) fractional reserve credit creation
- Interest rate policy and transmission mechanism
- Integration with the existing turn loop

---

### 2.1 Current State Assessment

**Implemented but NOT wired into turn loop:**
- `CentralBank` struct with interest rates, reserve requirements, FX/gold reserves
- `BankBalanceSheet` with double-entry validation (`is_balanced()`)
- `InterbankMarket` with XIBOR clearing
- `BfgFund` (deposit insurance) with premium collection
- `SobkScheme` (voluntary liquidity pool)
- `BankResolution` (Good Bank / Bad Bank split)
- `issue_loan()` function with fractional reserve credit creation
- `calculate_credit_score()` with LTV, cashflow, and consolidation logic

**Existing but legacy (not balance-sheet based):**
- `Bank` struct (Python-era): `total_deposits`, `issued_loans`, `liquid_reserves`, `reserve_requirement_ratio`
- `Bank::max_new_credit()` — deterministic reserve-limit formula
- `Bank::required_reserves()` — `total_deposits × reserve_requirement_ratio`

**Key gap:** The `Company` struct has an optional `bank_type: Option<BankType>` and `balance_sheet: Option<BankBalanceSheet>` field, meaning banks are Companies with banking data. But the turn loop does not process banking operations (loan issuance, deposit acceptance, interest collection, reserve checking).

---

### 2.2 Monetary Base (M0) — Central Bank

#### 2.2.1 M0 Definition

```
M0 = Cash in Circulation + Bank Reserves at Central Bank
```

- **Cash in Circulation:** Physical fiat held by citizens and companies. In the simulation, this is the sum of all `brokerage_account.cash` across all companies + all `class_demographics.*.savings` across all regions.
- **Bank Reserves:** Sum of `balance_sheet.reserves_at_central_bank` across all commercial/universal/cooperative banks.

#### 2.2.2 Base Money Creation Mechanisms

The Central Bank creates M0 through exactly three channels:

1. **Lombard Loans (Emergency Lending):**
   - CB lends reserves to a bank that cannot meet reserve requirements via interbank market.
   - **Double-entry:** CB asset `liquidity_injected += amount`, bank asset `reserves_at_central_bank += amount`, bank liability `cb_lombard_loans += amount`.
   - Rate: Lombard Rate (penalty rate, reference + 150 bps).
   - Repayment: bank `reserves_at_central_bank -= repayment`, `cb_lombard_loans -= repayment`, CB `liquidity_injected -= repayment`.

2. **Open Market Operations (OMO) — Bond Purchases:**
   - CB buys government bonds from banks, crediting their reserve accounts.
   - **Double-entry:** CB asset `securities += bond_value`, CB liability `bank_reserves += bond_value`, bank asset `reserves_at_central_bank += bond_value`, bank asset `securities -= bond_value`.
   - This injects permanent reserves (until CB sells bonds back).

3. **FX/Gold Purchases:**
   - CB buys gold or foreign currency, paying in domestic reserves.
   - Already partially implemented via `buy_gold()` / `sell_gold()`.
   - **Double-entry:** CB asset `physical_gold_reserves += gold`, CB liability `reserves_at_central_bank (system) += domestic_paid`.

#### 2.2.3 Base Money Destruction Mechanisms

1. **Lombard Loan Repayment:** (reverse of creation #1)
2. **OMO Bond Sales:** CB sells bonds back to banks, debiting reserves.
3. **FX/Gold Sales:** (reverse of creation #3, already implemented via `sell_gold()`)
4. **Reserve Requirement Breach Penalty:** If a bank cannot meet reserves and has no collateral for Lombard, the CB can seize assets and destroy the corresponding reserve liability.

#### 2.2.4 Central Bank Balance Sheet

```
ASSETS                          | LIABILITIES
================================|=================================
physical_gold_reserves          | cash_in_circulation (M0 component)
fx_reserves (foreign currency)  | bank_reserves_at_cb (M0 component)
liquidity_injected (Lombard)    | 
securities (gov bonds from OMO) | equity (CB own capital)
```

**Invariant:** `Total Assets == Total Liabilities + Equity` must hold at all times.

#### 2.2.5 Reserve Requirement Enforcement

```
Required Reserves = Total Deposits × Reserve Requirement Ratio
```

- Checked at end of each turn after all deposit changes.
- If `reserves_at_central_bank < required_reserves`:
  1. Bank first tries interbank market (`InterbankMarket::clear_market`).
  2. If still short, bank borrows from CB Lombard facility at Lombard Rate.
  3. If bank has no collateral (securities) for Lombard, trigger `BankResolution`.

---

### 2.3 Broad Money (M3) — Commercial Banks

#### 2.3.1 M3 Definition

```
M3 = M0 + Demand Deposits + Time Deposits + Other Liquid Assets
```

In the simulation:
- **Demand Deposits:** Sum of `balance_sheet.deposits` (the liability created when banks issue loans).
- **Time Deposits:** Not yet modeled (future: certificates of deposit).
- **Other Liquid Assets:** Money market fund shares (not yet modeled).

#### 2.3.2 Credit Creation (Money Creation) — Loan Issuance

Already partially implemented in `issue_loan()` (`banking.rs:744-831`). The double-entry is:

```
BANK ASSETS                    | BANK LIABILITIES
===============================|===============================
loans_issued += principal      | deposits += principal  ← NEW MONEY
                               |
```

**Key rules:**
1. **Credit scoring** must approve the loan (LTV, cashflow, collateral).
2. **Reserve check:** After expansion, `reserves_at_central_bank >= (deposits + principal) × reserve_ratio`. If not, bank must borrow reserves first.
3. **Reserves do NOT change during loan creation.** Reserves only move when the borrower withdraws the deposit and wires it to another bank (clearing).
4. **Caller must add `principal_amount` to borrower's liquid capital externally.** This is the money entering the real economy.

#### 2.3.3 Credit Destruction (Money Destruction) — Loan Repayment

When a borrower repays a loan:

```
BANK ASSETS                    | BANK LIABILITIES
===============================|===============================
loans_issued -= repayment      | deposits -= repayment  ← MONEY DESTROYED
                               |
```

**Double-entry:**
1. Borrower's `brokerage_account.cash -= repayment` (principal + interest).
2. Bank's `loans_issued[outstanding_balance] -= principal_portion`.
3. Bank's `deposits -= principal_portion` (money destruction).
4. Bank's `tier_1_capital += interest_portion` (interest income → equity).

#### 2.3.4 Deposit Acceptance

When a citizen or company deposits cash into a bank:

```
BANK ASSETS                    | BANK LIABILITIES
===============================|===============================
reserves_at_central_bank += X  | deposits += X
                               |
```

**Double-entry:**
1. Depositor's cash (M0 component) decreases by X.
2. Bank's reserves (M0 component) increase by X.
3. Bank's deposits (M3 component) increase by X.
4. **Net effect on M0:** Zero (cash → reserves, both M0).
5. **Net effect on M3:** +X (new deposit created).

#### 2.3.5 Deposit Withdrawal

Reverse of deposit acceptance:
1. Bank's `reserves_at_central_bank -= X`.
2. Bank's `deposits -= X`.
3. Depositor's cash increases by X.

**Reserve check:** If withdrawal causes `reserves < required`, bank must borrow via interbank or Lombard.

#### 2.3.6 Money Multiplier

```
Money Multiplier = M3 / M0 = 1 / reserve_requirement_ratio (theoretical maximum)
```

In practice, the multiplier is lower due to:
- Excess reserves held by banks.
- Cash held outside the banking system (citizen savings not deposited).
- Non-performing loans (frozen deposits).

---

### 2.4 Interest Rate Policy

#### 2.4.1 Rate Hierarchy (Already Implemented)

```
Lombard Rate     = Reference Rate + 150 bps  (ceiling, cap 25%)
Rediscount Rate  = Reference Rate + 50 bps   (cap 25%)
Reference Rate   = Set by RPP mandate         (floor 0%, cap 20%)
Discount Rate    = Reference Rate - 75 bps    (floor 0%)
Deposit Rate     = Reference Rate - 150 bps   (floor 0%)
```

#### 2.4.2 Rate Transmission Mechanism

```
CB Reference Rate
    ↓
XIBOR (Interbank Rate, bounded by Deposit Rate ≤ XIBOR ≤ Lombard Rate)
    ↓
Commercial Loan Rate = XIBOR + Bank Margin + Risk Premium
    ↓
Deposit Rate offered to customers = CB Deposit Rate - Bank Spread
```

**Transmission steps per turn:**
1. CB `update_reference_rate(inflation, target, gdp_growth, turn)` — adjusts Reference Rate per mandate.
2. CB `update_rate_hierarchy()` — recalculates Lombard, Rediscount, Discount, Deposit rates.
3. `InterbankMarket::clear_market()` — XIBOR clears between Deposit Rate (floor) and Lombard Rate (ceiling).
4. Each bank sets `deposit_interest_rate = CB Deposit Rate - bank_spread` (e.g., 50 bps below CB Deposit Rate).
5. Each bank sets `interest_rate = XIBOR + bank_margin + risk_premium` for new loans.
6. Variable-rate loans reset: `interest_rate = current_XIBOR + bank_margin + original_risk_premium`.

#### 2.4.3 RPP Meeting Schedule

- RPP meets every 12 turns (quarterly).
- Between meetings, rates are stable.
- Emergency meetings can be called if inflation > 2× target or GDP growth < -2%.

#### 2.4.4 Interest Collection Per Turn

For each outstanding loan:
1. `interest_due = outstanding_balance × interest_rate / 4` (quarterly rate).
2. Borrower pays: `brokerage_account.cash -= interest_due`.
3. Bank receives: `tier_1_capital += interest_due` (interest income → equity).
4. If borrower cannot pay: loan status → `Overdue`. After 3 overdue turns → `Default`.

For deposits:
1. `interest_owed = deposits × deposit_interest_rate / 4`.
2. Bank pays: `tier_1_capital -= interest_owed`.
3. Depositor receives: credited to their deposit balance (or cash).

---

### 2.5 Banking Turn Loop Integration

The following phases must be added to `engine/turn.rs`:

#### Phase 16A: Pre-Production Banking (Before B2B Orders)

```
1. CB rate update (if RPP meeting turn):
   - update_reference_rate(inflation, target, gdp_growth, turn)
   - update_rate_hierarchy()

2. Interbank market clearing:
   - Calculate each bank's reserve position
   - Clear interbank market → set XIBOR
   - Banks with residual deficit borrow from CB Lombard

3. Deposit interest accrual:
   - For each bank: calculate deposit interest owed
   - Debit bank tier_1_capital, credit depositor balances

4. Loan interest collection:
   - For each outstanding loan: collect interest from borrower
   - Credit bank tier_1_capital
   - Update loan status (Current → Overdue → Default)

5. Loan repayment processing:
   - Process scheduled principal repayments
   - Destroy deposits (money destruction)
   - Update loan outstanding_balance

6. New loan issuance:
   - Companies/citizens request loans (based on liquidity needs)
   - Bank runs credit scoring + reserve check
   - Issue loan: create deposit (money creation)
   - Credit borrower's brokerage_account.cash
```

#### Phase 16B: Post-Production Banking (After B2C Clearing)

```
7. Deposit acceptance/withdrawal:
   - Citizens deposit savings into banks (based on deposit rate attractiveness)
   - Companies deposit excess cash into banks
   - Process withdrawals (with reserve check)

8. Reserve requirement enforcement:
   - Calculate required reserves for each bank
   - Check compliance
   - Non-compliant banks: Lombard borrowing → BankResolution

9. BFG premium collection:
   - Collect premiums from Commercial/Universal banks
   - Debit reserves_at_central_bank ONLY (not tier_1_capital)

10. Bank failure detection:
    - Check if any bank has tier_1_capital < 6% of risk-weighted assets
    - Check if any bank has reserves < required (after Lombard)
    - Trigger BankResolution for failed banks

11. Money supply calculation:
    - M0 = cash_in_circulation + Σ(bank reserves)
    - M3 = M0 + Σ(demand deposits) + Σ(time deposits)
    - Money multiplier = M3 / M0
    - Store in macro_indicators for policy decisions
```

---

### 2.6 Bank as Company — Integration Architecture

Banks are `Company` entities with `bank_type: Some(BankType::Commercial)` and `balance_sheet: Some(BankBalanceSheet)`. The integration approach:

```
Company
├── id: "BANK_ILIRIA_001"
├── sector: Sector::FinancialServices
├── legal_form: LegalForm::Corporation
├── company_capital: X (shareholder equity = tier_1_capital)
├── brokerage_account: BrokerageAccount (for B2B participation)
├── bank_type: Some(BankType::Commercial)
├── balance_sheet: Some(BankBalanceSheet)
│   ├── reserves_at_central_bank: f64
│   ├── loans_issued: Vec<Loan>
│   ├── deposits: f64
│   ├── cb_lombard_loans: f64
│   ├── interbank_loans_given: HashMap
│   ├── interbank_loans_taken: HashMap
│   ├── securities: f64
│   ├── tier_1_capital: f64
│   └── ...
└── ...
```

**Key principle:** The `brokerage_account.cash` field is the bank's operating cash for B2B transactions. The `balance_sheet.reserves_at_central_bank` is the bank's reserve account at the CB. These are separate accounts:
- `brokerage_account.cash` = till cash (for daily operations, B2B payments).
- `reserves_at_central_bank` = reserve account at CB (for reserve requirements).

When a bank issues a loan, the principal is credited to the borrower's `brokerage_account.cash`. The bank's `deposits` liability increases. The bank's `reserves_at_central_bank` does NOT change (reserves only move during interbank clearing).

---

### 2.7 Loan Request Triggers

Companies request loans when:
1. `brokerage_account.cash < planned_b2b_spending` (working capital shortage).
2. `company_capital < desired_expansion_cost` (investment loan for new buildings).
3. Existing loan in default and company needs consolidation.

Citizens (demographic classes) request loans when:
1. Mortgage for housing (future: housing market).
2. Consumer credit for B2C purchases (if citizen savings < desired consumption).

The loan request is evaluated by the bank's `calculate_credit_score()` function. If approved, `issue_loan()` creates the loan and deposit. The principal is added to the borrower's `brokerage_account.cash`.

---

### 2.8 Default and Resolution Pipeline

```
Loan Status Flow:
Current → Overdue (missed payment) → Default (3 overdue turns)

Default Processing:
1. Bank attempts restructuring (RestructuringPlan):
   - Haircut on outstanding balance
   - Extended repayment period
   - Equity swap for consolidation loans

2. If restructuring fails → Bankruptcy:
   - Syndic executes liquidation
   - Assets → BankruptcyAuctionPool
   - Cash waterfall: Taxes → Banks → Shareholders
   - Bank's loans_issued reduced by outstanding_balance
   - Bank's deposits reduced by outstanding_balance (MONEY DESTRUCTION)

3. If bank itself fails (tier_1 < 6% RWA or reserves exhausted):
   - BankResolution::execute_bank_resolution
   - Good Bank / Bad Bank split
   - BFG absorbs toxic liabilities
   - Bridge bank operated until privatization
```

---

### 2.9 Money Supply Conservation Invariant

At the end of every turn, the following invariant MUST hold:

```
M0 = Σ(company.brokerage_account.cash) 
   + Σ(class_demographics.*.savings across all regions)
   + Σ(bank.balance_sheet.reserves_at_central_bank)

M3 = M0 
   + Σ(bank.balance_sheet.deposits)
   + Σ(bank.balance_sheet.issued_bonds)  [if held by non-bank entities]

Conservation: ΔM0 = CB_net_injections (Lombard + OMO + FX)
              ΔM3 = ΔM0 + Δ(credit_created - credit_destroyed)
```

If `ΔM0 ≠ CB_net_injections`, there is a black hole. If `ΔM3 ≠ ΔM0 + ΔCredit`, there is a credit creation leak.

---

### 2.10 Implementation Priority Order

1. **Fix CRITICAL black holes first** (1.1–1.6) — these corrupt the money supply every turn.
2. **Wire banking turn loop** (Phase 16A/16B) — without this, the banking system is dead code.
3. **Fix HIGH black holes** (1.7, 1.9, 1.10, 1.11, 1.20) — these corrupt specific flows.
4. **Fix MEDIUM black holes** (1.8, 1.12–1.16, 1.19, 1.21, 1.22) — these cause edge-case corruption.
5. **Fix LOW black holes** (1.15, 1.17, 1.18) — these are timing/sync risks.

---

### 2.11 New Structs/Fields Required

1. **`Country.central_bank_balance_sheet: Option<CentralBankBalanceSheet>`** — Track CB assets and liabilities for M0 calculation.

2. **`CentralBankBalanceSheet`** struct:
   ```
   assets:
     - gold_reserves: f64
     - fx_reserves: HashMap<String, f64>
     - liquidity_injected: f64 (Lombard loans outstanding)
     - securities_held: f64 (bonds from OMO)
   liabilities:
     - cash_in_circulation: f64 (computed)
     - bank_reserves: f64 (computed from bank balance sheets)
   equity:
     - cb_capital: f64
   ```

3. **`Company.loan_requests: Vec<LoanRequest>`** — Queue of pending loan requests for bank processing.

4. **`LoanRequest`** struct:
   ```
   - borrower_id: String
   - requested_principal: f64
   - loan_type: LoanType
   - term_turns: u32
   - purpose: String
   ```

5. **`Country.banking_turn_result: Option<BankingTurnResult>`** — Summary of banking operations per turn for auditing.

6. **`BankingTurnResult`** struct:
   ```
   - m0: f64
   - m3: f64
   - money_multiplier: f64
   - total_loans_issued: f64
   - total_loans_repaid: f64
   - total_deposits_created: f64
   - total_deposits_destroyed: f64
   - cb_net_injection: f64
   - interbank_volume: f64
   - xibor: f64
   - bank_failures: u32
   ```

---

### 2.12 Testing Strategy

1. **Money Supply Conservation Test:** After each turn, verify `ΔM0 == CB_net_injections` and `ΔM3 == ΔM0 + ΔCredit`.

2. **Balance Sheet Integrity Test:** After each banking operation, verify every bank's `is_balanced()` returns true.

3. **Black Hole Regression Tests:** For each fixed black hole (1.1–1.22), write a test that verifies money mass is conserved for that specific flow.

4. **Reserve Requirement Test:** Verify that no bank can issue loans that would breach reserve requirements without first borrowing reserves.

5. **Interest Rate Transmission Test:** Verify that CB rate changes propagate to XIBOR → commercial loan rates → deposit rates within the expected number of turns.

6. **Bank Resolution Test:** Verify that bank failure triggers resolution, BFG payout, and bridge bank creation without money creation/destruction.

---

## Appendix: File Reference Index

| File | Role |
|------|------|
| `state/central_bank.rs` | CB struct, interest rates, M0/M3 calculation, gold ops |
| `state/banking.rs` | BankBalanceSheet, InterbankMarket, BFG, SOBK, BankResolution, issue_loan |
| `state/treasury.rs` | Treasury struct (liquid_reserves, citizen_savings, tax_history) |
| `state/tax.rs` | TaxRates, PIT, CIT, VAT, ExciseTax, PublicDebt |
| `economy/clearing.rs` | Market clearing with VAT, warehouse extraction, price discovery |
| `economy/production.rs` | Production cycle, wage calculation, input/output processing |
| `economy/retail.rs` | B2C market clearing, consumer demand, store offers |
| `economy/b2b_orders.rs` | B2B order submission, trade settlement, production execution |
| `economy/b2c_services.rs` | Education/health B2C clearing with subsidies |
| `economy/labor.rs` | Demographics, labor supply, wage calculation, citizen_savings aggregation |
| `economy/labor_market.rs` | Regional labor market clearing, wage payment, class savings credit |
| `economy/wholesale.rs` | Wholesale distribution, procurement, transport costs |
| `economy/debt_market.rs` | Sovereign debt, bond auctions, interest payments, default |
| `economy/maintenance.rs` | Building condition degradation, maintenance spending |
| `economy/inspectorates.rs` | Inspectorate fines (company cash → treasury) |
| `economy/state_forests.rs` | State forest harvest, treasury remittance |
| `economy/royalties.rs` | Technology royalty payments between companies |
| `society/charities.rs` | Charity fundraising and distribution |
| `corporate/bankruptcy.rs` | Bankruptcy auction pool, syndic, liquidation waterfall |
| `utilities/waste_collection.rs` | Waste collection operating costs |
| `infrastructure/maritime.rs` | Maritime infrastructure maintenance |
| `engine/turn.rs` | Global turn orchestrator, all phase ordering |
| `construction/orders.rs` | Construction order cash encumbrance |

---

> **Awaiting USER approval before any implementation begins.**
