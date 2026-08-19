# Resurrection Spring Cleaning — Implementation Plan

**Date:** 2026-08-12
**Companion to:** `resurrection-spring-cleaning-audit.md`
**Status:** Awaiting approval

---

## Acknowledged Corrections

1. **No short-term patches for interest (Black Hole #3).** Every unit of corporate interest MUST route via `TransferSettler` to the specific issuing commercial bank. Loans without a valid lender bank are invalid state — they will be assigned to a commercial bank during migration or wiped entirely. No "central bank residual" hack.
2. **Strict separation of Logic vs Folder Restructuring.**
   - **Phase 24A (The Logic Pass):** Fix all accounting black holes, eliminate the duplicate Polish-keyed registry, wire up dead code (liquidation/dividends/downsizing), and close one-way doors — all in the current file structure. Get all 525+ tests passing.
   - **Phase 24B (The Folder Restructure):** Only after Phase 24A tests pass, move files into the new subdirectory structure and update `use` statements. No logic changes.
3. **Land conservation on demolition (24A.9).** Buildings occupy physical land (hectares) in a region's `LandUseInventory` (Phase 13 physics). The `Building` struct does not currently track its land footprint. Demolishing a building without returning its hectares to the region's available land pool causes land evaporation — the region's total surface area silently shrinks. This MUST be fixed by adding a `land_hectares` field to `Building` and routing hectares back to `LandUseInventory` on demolition.
4. **State debt repayment (24A.2).** The State/Treasury (`Country.budget`) can also be a borrower. If `loan.borrower_id` resolves to the state/treasury rather than a company, the repayment MUST be deducted from `country.budget.liquid_reserves` via `settle_transfer_from_treasury` (or equivalent double-entry function), NOT scanned for in the `companies` vector and marked as Default. State debt must never become phantom debt.

---

## Phase 24A — The Logic Pass

### 24A.1 — Fix Black Hole #1: B2B Bid Refund Pipeline

**Root cause:** `task.order_book` is reset to empty at `engine/turn.rs:630` (after merging into `global_order_book`), but `refund_unfilled_bids` is called at `turn.rs:773` on the now-empty `task.order_book`. Additionally, the WRONG refund function is called: `order_book::refund_unfilled_bids` (credits `liquid_capital`) instead of `b2b_orders::refund_unfilled_bids` (properly releases `debit_cash` and restores `available_cash` + `brokerage_account.cash`).

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `engine/turn.rs` | `run_turn` (lines 623-632, 751-777) | After `match_orders_with_embargoes` (line 635), redistribute `global_order_book.bids` back to per-country `task.order_book` by matching `bid.buyer_id` against each task's `companies`. Then call `b2b_orders::refund_unfilled_bids` (the CORRECT function, exported as `refund_unfilled_b2b_bids`) instead of `order_book::refund_unfilled_bids`. |
| `engine/turn.rs` | import block (line 8-28) | Add `refund_unfilled_b2b_bids` to the `use crate::economy::{...}` import. |
| `economy/order_book.rs` | `refund_unfilled_bids` (line 415) | **Delete** this function (it credits the wrong field). Keep `refund_unfilled_bids_cultural` and `refund_unfilled_bids_maritime` (these credit `building.available_cash` which is correct for cultural/maritime buildings). |
| `economy/mod.rs` | re-exports (line 66) | No change needed — `refund_unfilled_b2b_bids` is already exported. |

**Redistribution logic (new helper in `engine/turn.rs`):**
```rust
// After match_orders_with_embargoes, give unfilled bids back to their countries.
for task in &mut tasks {
    let company_ids: HashSet<String> = task.companies.iter().map(|c| c.id.clone()).collect();
    for (commodity, bids) in &global_order_book.bids {
        let country_bids: Vec<Bid> = bids.iter()
            .filter(|b| company_ids.contains(&b.buyer_id))
            .cloned()
            .collect();
        if !country_bids.is_empty() {
            task.order_book.bids.entry(*commodity).or_default().extend(country_bids);
        }
    }
}
```

**Test:** Add `test_bid_refund_releases_debit_cash` — submit bids, partially fill, assert `sum(debit_cash) == sum(filled_trade_values)` and unfilled bidders have `debit_cash == 0`.

---

### 24A.2 — Fix Black Hole #2: Phantom Loan Repayments

**Root cause:** `state/banking.rs:1995-2032` iterates only banks, reducing `loan.outstanding_balance` and increasing `bank.reserves_at_central_bank`, but never debits the borrower. `Loan.borrower_id` already exists (line 141) but is unused in repayment.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `state/banking.rs` | `process_banking_turn` loan repayment loop (lines 1995-2032) | After computing `actual_payment` for each loan, find the borrower by `loan.borrower_id`. **Three cases:** (a) If borrower is a company in `companies`, debit its `available_cash` via `transfer_settler::settle_transfer` and reduce `company.liabilities` by the principal portion. (b) If `borrower_id` matches the state/treasury identifier (e.g., `"STATE"` or `country.budget.treasury_id`), debit `country.budget.liquid_reserves` via `transfer_settler::settle_transfer_from_treasury` — state debt must never become phantom debt. (c) If borrower is not found in either companies or the treasury, mark `loan.status = LoanStatus::Default`. |

**Revised repayment loop (pseudocode):**
```rust
for (bi, bank) in companies.iter_mut().enumerate() {
    if let (Some(_), Some(ref mut bs)) = (&bank.bank_type, &mut bank.balance_sheet) {
        for loan in &mut bs.loans_issued {
            if loan.status == LoanStatus::Default { continue; }
            let interest = loan.outstanding_balance * loan.interest_rate;
            loan.outstanding_balance += interest;
            if loan.interest_type == InterestType::Variable {
                loan.interest_rate = xibor + loan.bank_margin;
            }
            if loan.term_turns > 0 {
                let principal_portion = loan.principal / loan.term_turns as f64;
                let payment = principal_portion + interest;
                let actual_payment = payment.min(loan.outstanding_balance);
                loan.outstanding_balance -= actual_payment;
                repaid_total += actual_payment;
                loan.turns_remaining = loan.turns_remaining.saturating_sub(1);
                loan.last_payment_turn = current_turn;
                if loan.outstanding_balance <= 0.01 {
                    loan.outstanding_balance = 0.0;
                    loan.status = LoanStatus::Repaid;
                }
                // NEW: Debit the borrower (deferred to avoid double-borrow)
                pending_debits.push((loan.borrower_id.clone(), bi, actual_payment, principal_portion));
            }
        }
        bs.reserves_at_central_bank += repaid_total;
        result.total_loan_repayments += repaid_total;
    }
}
// NEW: Execute borrower debits after releasing bank borrows
// CORRECTION: Three-way borrower resolution (company / state / default)
const STATE_BORROWER_ID: &str = "STATE"; // or country.budget.treasury_id
for (borrower_id, bank_idx, amount, principal_portion) in pending_debits {
    if borrower_id == STATE_BORROWER_ID {
        // CASE B: State/Treasury borrower — debit liquid_reserves, never Default
        transfer_settler::settle_transfer_from_treasury(
            &mut country.budget, amount, bank_idx, companies,
        );
    } else if let Some(borrower_idx) = companies.iter().position(|c| c.id == borrower_id) {
        // CASE A: Company borrower — Use TransferSettler to debit borrower and credit bank atomically
        let _ = transfer_settler::settle_transfer(
            companies, borrower_idx, amount,
            TransferRecipient::OtherCompany { recipient_idx: bank_idx },
            country,
        );
        // Reduce borrower's liabilities by principal portion
        if let Some(borrower) = companies.get_mut(borrower_idx) {
            borrower.liabilities = (borrower.liabilities - principal_portion).max(0.0);
        }
    } else {
        // CASE C: Borrower vanished — mark loan as Default (will be cleaned in bankruptcy)
        if let Some(bank) = companies.get_mut(bank_idx) {
            if let Some(ref mut bs) = bank.balance_sheet {
                for loan in &mut bs.loans_issued {
                    if loan.borrower_id == borrower_id && loan.status != LoanStatus::Repaid {
                        loan.status = LoanStatus::Default;
                    }
                }
            }
        }
    }
}
```

**Note:** `transfer_settler::settle_transfer` already handles the bank balance sheet sync (deposits + reserves) for both payer and recipient. The `bs.reserves_at_central_bank += repaid_total` line above will be REMOVED — the TransferSettler handles it.

**Test:** Add `test_loan_repayment_debits_borrower` — issue a loan, advance turns, assert `bank.reserves_increase == borrower.cash_decrease` and `borrower.liabilities` decreases by principal portion.

**Test:** Add `test_state_loan_repayment_debits_treasury` — create a `Loan` with `borrower_id == "STATE"` in a bank's `loans_issued`, advance turns, assert `country.budget.liquid_reserves` decreases by the repayment amount and `bank.reserves_at_central_bank` increases by the same. The loan must NOT be marked `Default`.

**Related finding (not part of 24A.2 but documented for awareness):** `economy/debt_market.rs::process_debt_service` (line 576) computes a `payments: Vec<(String, f64)>` Vec of holder credits but NEVER iterates it — the treasury debits `liquid_reserves -= total_due` (line 718) but no central bank or retail holder is ever credited. This is a parallel money-destruction black hole in the sovereign debt system. It should be fixed in the same pass as 24A.2 but is tracked separately as it involves `TreasurySecurity` holders, not `Loan` records.

---

### 24A.3 — Fix Black Hole #3: Vanishing Corporate Interest

**Root cause:** `corporate/manager.rs:272-280` subtracts `interest` from `company.liquid_capital` but never credits any bank. The company's `liabilities` field is a lump sum with no link to a specific bank.

**Problem:** The corporate manager computes interest as `company.liabilities * (xibor + risk_margin)` — a synthetic calculation not tied to any actual `Loan` record. The `FinanceSource::BankLoan(loan)` path (manager.rs:407-409) does `company.liabilities += loan` without creating a `Loan` in any bank's `loans_issued`.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `entities/mod.rs` | `Company` struct | Add field `pub outstanding_loan_bank_id: Option<String>` — tracks which commercial bank issued the company's working-capital loan (set when a loan is taken via `FinanceSource::BankLoan`). |
| `corporate/manager.rs` | `apply_action` — `FinanceSource::BankLoan` branch (lines 407-410) | When a BankLoan is taken, find a suitable bank in `companies` (filter by `bank_type.is_some() && balance_sheet.is_some()`), call `state::banking::issue_loan` to create a proper `Loan` record in that bank's `loans_issued`, and set `company.outstanding_loan_bank_id = Some(bank_id)`. The `company.liabilities += loan` stays. |
| `corporate/manager.rs` | `process_company` — interest calculation (lines 272-280) | Replace the synthetic interest calc with: look up the actual `Loan` records for this company across all banks' `loans_issued` (filter by `loan.borrower_id == company.id && loan.status == Current`). Sum their accrued interest. Route each loan's interest to its issuing bank via `transfer_settler::settle_transfer(companies, company_idx, interest, TransferRecipient::OtherCompany { recipient_idx: bank_idx }, country)`. If `company.outstanding_loan_bank_id` is `None` but `company.liabilities > 0`, this is invalid state — wipe `liabilities` to 0 (per the "no short-term patches" rule: invalid loans are wiped, not routed to treasury). |

**Signature change:** `process_company` currently takes `company: &mut Company, total_profit, country, year, market_signal`. To access all companies (for bank lookup and TransferSettler), it needs `companies: &mut [Company]` and `company_idx: usize`. This is a signature change propagated to `process_companies` (the caller at manager.rs:50-67), which already has `companies: &mut [Company]`.

**Migration:** In `io/save_manager.rs` load path, for any company with `liabilities > 0` and no `outstanding_loan_bank_id`, either:
- Assign the liability to the first commercial bank in the country (create a `Loan` record), OR
- Wipe `liabilities` to 0 (if no banks exist).
This ensures no invalid state persists from old saves.

**Test:** Add `test_corporate_interest_routes_to_bank` — company takes a loan, next turn assert `bank.reserves_at_central_bank` increases by interest amount and `company.liquid_capital` decreases by the same.

---

### 24A.4 — Migrate `settle_trades` to TransferSettler

**Root cause:** `economy/b2b_orders.rs:443-550` manually does `buyer.debit_cash -= `, `seller.available_cash += `, `seller.brokerage_account.cash += ` — bypassing bank balance sheet sync.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `economy/b2b_orders.rs` | `settle_trades` (lines 443-550) | Replace the cash settlement block (lines 453-471) with: find `buyer_idx` and `seller_idx` in `companies`, call `transfer_settler::settle_transfer(companies, buyer_idx, trade_value, TransferRecipient::OtherCompany { recipient_idx: seller_idx }, country)`. This releases `buyer.debit_cash`, credits `seller.available_cash` + `brokerage_account.cash`, AND syncs both banks' balance sheets. The physical inventory routing (lines 473-546) stays unchanged. |
| `economy/b2b_orders.rs` | `settle_trades_with_tariffs` (lines 578-630) | The tariff leg (lines 622-627) currently does `buyer.debit_cash -= ` and `country.budget.liquid_reserves += `. Replace with `transfer_settler::settle_transfer_to_treasury(companies, buyer_idx, tariff_amount, country)`. |
| `economy/b2b_orders.rs` | imports | Add `use crate::economy::transfer_settler::{settle_transfer, settle_transfer_to_treasury, TransferRecipient};` |
| `economy/b2b_orders.rs` | `settle_trades` signature | Add `country: &mut crate::state::Country` parameter (needed by TransferSettler). Update the call in `settle_trades_with_tariffs` (line 587) and in `engine/turn.rs:668`. |

**Test:** Add `test_settle_trades_double_entry_invariant` — after settling N trades, assert `sum(cash_delta across all companies + treasury) == 0`.

---

### 24A.5 — Merge Duplicate Production-Method Registries

**Root cause:** `registries/production_methods.rs` uses Polish display-name keys; `registries/production_methods_data.rs` uses English snake_case sector keys. Both merge into one `HashMap` queried by two different code paths.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `registries/production_methods.rs` | `state_building_methods` (line 137), `industrial_production_methods` (line 810) | Re-key all 34 `registry.insert(...)` calls from Polish display names to English snake_case sector keys. Map state buildings to their sectors: military→`public_administration`, courts→`public_administration`, hospitals→`medical_services`, schools→`educational_services`, monasteries→`religion`, markets/retail→`local_services`, etc. For building-specific subtypes (monastery_wine vs. monastery_scriptorium), use `"{sector}/{subtype}"` keys. |
| `economy/production.rs` | `resolve_active_method` (line 60) | Change lookup from `registries.production_methods.get(&building.name)` to `registries.production_methods.get(&sector_json_name(building.sector))` (or the subtype key if `building.subtype` is set). |
| `registries/mod.rs` | `from_tech_tree_json`, `native_only` (lines 70-103) | No structural change — both still call `state_building_methods()` + `industrial_production_methods()` + `default_production_methods()`. The keys just no longer collide-by-convention. |
| `registries/production_methods_data.rs` | `default_production_methods` (line 37) | No change — already uses English keys. After re-keying `production_methods.rs`, consider merging the two files (delete `production_methods_data.rs`, move its functions into `production_methods.rs`). This is optional in 24A. |

**Test:** Add `test_every_sector_has_methods` — for each `Sector` variant, assert `registries.production_methods` contains the sector's snake_case key.

---

### 24A.6 — Wire Dividends to All Shareholders

**Root cause:** `corporate/manager.rs:441-453` only routes dividends to `cultural_institutions`. `SecuritiesExchange::route_dividends` (exchange.rs:648) is the proper router but is never called.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `corporate/manager.rs` | `apply_action` — `PayDividend` branch (lines 441-453) | Replace the monastery-only loop with a call to `country.stock_exchange.route_dividends(company_id, total, companies, brokerage_accounts, treasury, treasury_id)`. Build the `BTreeMap<String, &mut BrokerageAccount>` from companies' `brokerage_account` fields. For state-owned companies (state_share > 0), the treasury receives the state's share. Keep the monastery routing as a fallback for `cultural_institutions.owned_company_shares` that aren't in the exchange's `owners` map. |
| `corporate/manager.rs` | `process_companies` aggregate stats (line 146) | Fix `total_dividends` — change from `total_profit.max(0.0)` to accumulate: `companies[i].aggregated_stats.total_dividends += total_dividend_paid_this_turn`. |
| `securities/funds.rs` | NEW: `distribute_fund_income_to_unit_holders` | Add a function that distributes fund dividend/interest income back to `ClassDemographics.savings` proportional to each class's unit holdings. Call it from `engine/turn.rs` after SEC-6 (MBS/covered bond coupon processing). This closes the loop: dividends → funds → citizens. |
| `engine/turn.rs` | SEC sequence (after line 1760) | Call `securities::funds::distribute_fund_income_to_unit_holders(&mut task.companies, &mut task.ctx.country.regions, &config)`. |

**Test:** Add `test_dividends_reach_all_shareholders` — company with monastery + fund + treasury owners pays dividend; assert each receives their proportional share and `sum(received) == total_dividend`.

---

### 24A.7 — Fix Corporate IPO to Use Stock Exchange

**Root cause:** `corporate/manager.rs:455-491` directly mutates `shares_count` and `liquid_capital` without populating `owners` or listing on the exchange. Proceeds appear from nowhere.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `corporate/manager.rs` | `apply_action` — `Ipo` branch (lines 455-491) | Replace the direct mutation with a call to `country.stock_exchange.execute_ipo(company, shares_to_float, reserve_price, buyers, brokerage_accounts)`. Build `buyers` from investment funds (FIO/FIZ) and wealthy `ClassDemographics` that can afford shares. Build `brokerage_accounts` from companies' and demographics' brokerage accounts. The `execute_ipo` function (exchange.rs:583) already handles buyer debiting, owner dilution, and `shares_count` update. |

**Test:** Add `test_ipo_debits_buyers_and_populates_owners` — company IPOs, assert buyers' cash decreased, `company.owners` is populated, `company.shares_count` increased, and `sum(buyer_cash_decrease) == proceeds`.

---

### 24A.8 — Wire Bankruptcy Liquidation + Ghost Reference Cleanup

**Root cause:** `corporate/lifecycle.rs:62-128` is a crude stub. `Syndic::execute_liquidation` (bankruptcy.rs:236) is dead code using the legacy `Bank` struct.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `state/mod.rs` | `Country` struct | Add field `pub bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool`. Initialize in all constructors. |
| `corporate/bankruptcy.rs` | `Syndic::execute_liquidation` (line 236) | Migrate from legacy `Bank` struct to `Company` + `BankBalanceSheet`. Replace `banks: &mut Vec<Bank>` parameter with `companies: &mut [Company]`. Build creditor claims from each bank's `balance_sheet.loans_issued` filtered by `loan.borrower_id == company.id`. The waterfall: (1) tax owed → treasury, (2) bank loans proportional to outstanding balance → banks via `transfer_settler`, (3) residual → shareholders via `route_dividends`-style distribution. |
| `corporate/lifecycle.rs` | `liquidate_bankrupt_companies` (line 62) | Before removing a company, call `Syndic::execute_liquidation`. After removal, clean ghost references: (a) mark all `Loan` records with `borrower_id == company.id` as `LoanStatus::Default` across all banks (unless `borrower_id == "STATE"` — state loans are never defaulted, they persist); (b) clear `building.inventory` for orphaned buildings (route to `bankruptcy_auction_pool`); (c) cancel `building.active_project` (recover delivered materials to auction pool); (d) delist from `country.stock_exchange` (remove order book entries for `EQUITY:{company_id}`); (e) reclaim `country.politics.justice_state.frozen_company_cash[company.id]` into the liquidation pool; (f) **return each orphaned building's `land_hectares` to its region's `LandUseInventory`** under the building's `land_category` (same land conservation logic as 24A.9 Demolish). |
| `corporate/bankruptcy.rs` | `preserve_defects_through_bankruptcy` (line 388) | Already correct — keep as-is. Call it from `liquidate_bankrupt_companies` before building ownership transfer. |
| `engine/turn.rs` | `CompanyLifecycle::process_lifecycle` call (line 1648) | Pass `&mut task.ctx.country.bankruptcy_auction_pool` through to `liquidate_bankrupt_companies`. |

**Test:** Add `test_bankruptcy_cleans_ghost_references` — bankrupt a company with loans, buildings, stock listing, frozen cash; assert no bank has an active loan to it, no building has its `owner_id`, no stock exchange entry exists, no frozen cash entry remains.

---

### 24A.9 — Corporate Downsizing (Asset Sales + Production Halt + Land Conservation)

**Root cause:** `CorporateAction::Restructure` (manager.rs:427-439) only adjusts numbers on the company struct. No asset sales, no production halt, no worker return to labor pool. Additionally, the `Building` struct does not track its land footprint, so demolishing a building causes land evaporation — the region's total surface area silently shrinks.

**Land conservation requirement (Phase 13 physics):** Buildings occupy physical land (hectares) within a region's `LandUseInventory` (`society/geography.rs:370`). The `LandUseInventory` tracks hectares by `LandCategory` (Urbanized, Industrial, Agricultural, etc.). When a building is demolished, its hectares MUST be returned to the region's available land pool under the appropriate category, ensuring `sum(category.area_hectares)` remains constant.

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `entities/mod.rs` | `Building` struct (line 945) | Add field `pub land_hectares: f64` with `#[serde(default)]` for backward compatibility. This tracks how many hectares the building occupies in its region's `LandUseInventory`. |
| `entities/mod.rs` | `Building` struct | Add field `pub land_category: LandCategory` with `#[serde(default)]` — tracks which `LandCategory` the building's hectares were deducted from (Urbanized for retail/services, Industrial for factories/mines, Agricultural for farms). Needed to know which category to return hectares to on demolition. |
| `engine/generator/corporate.rs` | Building generation (lines 589, 998) | Set `land_hectares` during generation based on `scale_factor` * sector-specific constant (e.g., Industrial: 50 ha * scale_factor, Agricultural: 200 ha * scale_factor, Urbanized: 10 ha * scale_factor). Set `land_category` from the building's sector. |
| `corporate/strategy.rs` | `CorporateAction` enum (line 44) | Add variants: `Demolish { building_id: String }` and `HaltProduction { building_id: String }`. |
| `corporate/manager.rs` | `apply_action` — `Restructure` branch (line 427) | When layoffs occur, route laid-off workers back to the regional labor pool: find the company's `region_id`, increment `country.regions[region_idx].class_demographics` unemployment. When `capital_write_off > 0`, fire-sale excess inventory: submit asks to the order book at 80% of market price for each commodity in `building.inventory`. Cancel pending `building.active_project` with material recovery (delivered materials go to auction pool). |
| `corporate/manager.rs` | `apply_action` — NEW `Demolish` branch | `Demolish`: (1) find the building and its `region_id` + `land_hectares` + `land_category`; (2) return `land_hectares` to `region.land_use_inventory` under the building's `land_category` (if the category was Industrial/Urbanized, return to that category; if Agricultural, also restore soil class data); (3) remove building from `buildings` vec; (4) add its fixed_capital to `bankruptcy_auction_pool` at fire-sale price. **Land conservation invariant:** `region.land_use_inventory.total_area` must not change. |
| `corporate/manager.rs` | `apply_action` — NEW `HaltProduction` branch | `HaltProduction`: set `building.current_employment = 0` without changing `worker_capacity` (temporary shutdown). Route halted workers back to regional labor pool. |

**Land return logic (pseudocode for Demolish):**
```rust
// CORRECTION: Land conservation — return hectares to region's LandUseInventory
let region = country.regions.iter_mut().find(|r| r.id == building.region_id);
if let Some(region) = region {
    let category_key = serde_json::to_string(&building.land_category).unwrap_or_default();
    if let Some(cat) = region.land_use_inventory.categories.get_mut(&category_key) {
        cat.area_hectares += building.land_hectares;
    }
    // If agricultural, also restore soil class hectares
    if building.land_category == LandCategory::Agricultural {
        // Restore to community/municipal pool by default (demolished factory land
        // becomes available public land)
        if let Some(cat) = region.land_use_inventory.categories.get_mut(&category_key) {
            cat.ownership_distribution.community_hectares += building.land_hectares as i64;
        }
    }
}
// INVARIANT: region.land_use_inventory.total_area is unchanged
```

**Test:** Add `test_downsizing_returns_workers_to_labor_pool` — company restructures with layoffs, assert regional unemployment increases by the layoff count.

**Test:** Add `test_demolition_conserves_land` — demolish a building with `land_hectares = 50.0` in an Industrial category, assert `region.land_use_inventory` Industrial category hectares increase by 50.0 and `total_area` is unchanged.

---

### 24A.10 — Delete Dead Code

**Files to delete:**

| File | Size | Reason |
|------|------|--------|
| `economy/banking.rs` | 4.8 KB | Superseded by `state/banking.rs`. Remove from `economy/mod.rs` exports. |
| `corporate/development.rs` | 15.7 KB | `PropertyDeveloper` never instantiated. Remove from `corporate/mod.rs`. |
| `corporate/funds.rs` | 27.3 KB | `OpenEndFundData`/`ClosedEndFundData` never used (securities/funds.rs is live). Remove from `corporate/mod.rs`. |
| `corporate/bounded_rationality.rs` | 4.7 KB | Never called. Remove from `corporate/mod.rs`. |

**Functions to delete (in kept files):**

| Function | File | Reason |
|----------|------|--------|
| `run_economic_turn`, `compare_to_expected`, `ParityResult` | `economy/indicators.rs` | Target 2 stubs, never called from turn loop. |
| `CountryTurnCtx` | `economy/mod.rs` | Only used by deleted functions. |
| `order_book::refund_unfilled_bids` | `economy/order_book.rs` | Replaced by `b2b_orders::refund_unfilled_bids` in 24A.1. |
| Legacy `Bank` struct | `state/banking.rs` | Only used by pre-migration `execute_liquidation`; after 24A.8 migration, unused. |
| `execute_infrastructure_production`, `submit_infrastructure_procurement_orders` | `economy/infrastructure.rs` | Never called from turn loop. |
| `execute_state_research` | `economy/state_research.rs` | Never called. |
| `sign_retail_leases` | `economy/real_estate.rs` | Never called. |
| `apply_consolidation`, `enforce_procurement_cap` | `economy/wholesale.rs` | Never called. |

**Total: ~53 KB of dead code removed.**

---

### 24A.11 — Add Double-Entry Invariant Tests

**Files to modify:**

| File | Function | Change |
|------|----------|--------|
| `tests/banking_integration_test.rs` | NEW test | `test_money_mass_conservation_after_banking_turn` — snapshot total cash (all companies + treasury + central bank) before and after `process_banking_turn`; assert delta == 0 (modulo new credit creation, which must be explicitly tracked). |
| `tests/supply_chain_integrity_test.rs` | NEW test | `test_money_mass_conservation_after_trade_settlement` — snapshot total cash before and after `settle_trades_with_tariffs`; assert delta == 0. |
| `tests/golden_master_test.rs` | NEW test | `test_capital_drift_under_5_percent` — run 20 turns, assert total capital drift < 5% (replaces the current 109,154% drift). |

---

### 24A.12 — Verification Gate

Before proceeding to Phase 24B:
1. `cargo test` — all 525+ existing tests pass + new tests pass.
2. `cargo clippy` — no new warnings.
3. Run 100-turn simulation — assert capital drift < 5% (down from 109,154%).
4. Assert citizen savings grow > 0% (dividends now reach ClassDemographics).
5. Assert market imbalances shift over turns (no longer frozen).

---

## Phase 24B — The Folder Restructure

**Precondition:** Phase 24A is complete, all tests pass, capital drift < 5%.

**Approach:** Pure mechanical moves. No logic changes. Each subdirectory migration is a separate commit.

### 24B.1 — Create subdirectory structure

Create the following subdirectories under `src/economy/`:
`market/`, `trade/`, `finance/`, `production/`, `logistics/`, `labor/`, `justice/`, `state_sector/`, `religion/`, `society/`, `config/`

### 24B.2 — Move files (one subdirectory per commit)

| Subdirectory | Files moved |
|-------------|-------------|
| `market/` | `market.rs`, `order_book.rs`, `clearing.rs`, `market_history.rs` |
| `trade/` | `b2b_orders.rs`, `b2c_services.rs`, `retail.rs`, `retail_registry.rs`, `wholesale.rs`, `royalties.rs`, `blueprints.rs`, `innovation_trading.rs`, `transfer_settler.rs` |
| `finance/` | `debt_market.rs`, `payment_in_kind.rs` |
| `production/` | `production.rs`, `fixed_assets.rs`, `maintenance.rs`, `geology.rs`, `weather.rs`, `disasters.rs` |
| `logistics/` | `logistics.rs`, `transport_networks.rs`, `commuting.rs` |
| `labor/` | `labor.rs`, `labor_market.rs`, `migration.rs`, `assimilation.rs` |
| `justice/` | `justice_system.rs`, `prison_labor.rs`, `sentencing.rs`, `civil_lawsuits.rs`, `legal_status.rs`, `inspectorates.rs`, `inspectorate_fleet.rs`, `bribery.rs` |
| `state_sector/` | `state_forests.rs`, `state_research.rs`, `infrastructure.rs`, `infrastructure_config.rs`, `fishing.rs`, `fishing_config.rs`, `osp.rs`, `smuggling.rs` |
| `religion/` | `religious_economy.rs`, `media.rs`, `propaganda.rs` |
| `society/` | `ethnic_violence.rs` |
| `config/` | `b2b_config.rs`, `corporate_config.rs`, `generative_goods_config.rs`, `innovation_config.rs`, `service_config.rs` |

### 24B.3 — Update `use` statements

For each moved file, update:
1. The `pub mod` declarations in `economy/mod.rs`.
2. All `use crate::economy::{...}` statements across the codebase (primarily `engine/turn.rs`).
3. All `use crate::economy::<module>::<item>` paths.

### 24B.4 — Verification

1. `cargo test` — all tests pass (no logic changed, only paths).
2. `cargo clippy` — no warnings.
3. Git diff shows ONLY file moves + import path changes.

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| TransferSettler signature requires `companies: &mut [Company]` + `payer_idx` — borrow checker conflicts in `process_company` | Use the "deferred debits" pattern (collect pending operations in a Vec, execute after releasing borrows) — already used in `state/banking.rs`. |
| Re-keying production methods breaks golden-master parity | Golden master tests use `turn_0` snapshots — verify the re-keyed registry produces the same `ActiveProductionMethod` for each building. Add a migration test. |
| IPO buyer pool construction is complex (funds + demographics) | Start with funds-only as buyers (they already have brokerage accounts). Add demographics buyers in a follow-up. |
| `BankruptcyAuctionPool` on `Country` breaks save compatibility | Use `#[serde(default)]` — old saves load with an empty pool. |
| Dead code deletion removes tests | Move tests from deleted modules to the appropriate live module (e.g., `corporate/funds.rs` tests → `securities/funds.rs` if applicable, else delete). |
| `Building.land_hectares` / `land_category` fields break save compatibility | Use `#[serde(default)]` — old saves load with `0.0` hectares and `LandCategory::Industrial` default. For existing buildings with `0.0` hectares, demolition returns 0 hectares (no-op on land inventory). A migration pass can compute hectares from `scale_factor` for existing saves. |
| State borrower ID convention (`"STATE"`) may not match existing data | Check `country.budget.treasury_id` or `country.central_bank.id` for the actual identifier used. Make the state-borrower check configurable or check multiple known IDs. |
| `process_debt_service` payments Vec is never applied (related finding) | Fix in the same pass as 24A.2: iterate `payments` Vec and credit central bank reserves / retail holder savings. This is a parallel black hole in the sovereign debt system. |

---

## Execution Order (Phase 24A)

1. **24A.1** (bid refund) — smallest, highest impact, isolated to `turn.rs` + `order_book.rs`.
2. **24A.2** (loan repayment) — isolated to `state/banking.rs`.
3. **24A.3** (corporate interest) — requires `entities/mod.rs` field + `manager.rs` signature change.
4. **24A.4** (settle_trades migration) — depends on 24A.3's signature changes being stable.
5. **24A.5** (registry merge) — isolated to `registries/` + `production.rs`.
6. **24A.6** (dividends) — depends on 24A.4 (TransferSettler available in manager context).
7. **24A.7** (IPO) — depends on 24A.6 (exchange wiring established).
8. **24A.8** (bankruptcy) — depends on 24A.2 (loan cleanup) + 24A.6 (dividend routing for residual).
9. **24A.9** (downsizing) — isolated to `strategy.rs` + `manager.rs`.
10. **24A.10** (dead code deletion) — do LAST, after all wiring is confirmed.
11. **24A.11** (invariant tests) — add incrementally with each step.
12. **24A.12** (verification gate) — final check before 24B.

---

*This plan modifies only Rust source files and test files. No configuration, git, or CI changes are required for Phase 24A.*
