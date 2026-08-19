# Resurrection Salvage Audit

**Date:** 2026-08-12
**Status:** Read-only analysis. No code changes made. Awaiting user approval on recommendations.
**Precondition:** Phase 24A (Logic Pass) and Phase 24B (Folder Restructure) are complete. All 525 lib tests + 42 integration tests pass. M3 multiplier reduced from 1091x to 322x.

---

## Part 1: Unhooked Logic Analysis

Each module was analyzed for: (a) what it does, (b) whether it's called, (c) whether it duplicates live logic, (d) salvage plan or scrap verdict.

### 1.1 `corporate/funds.rs` (27.3 KB) — SCRAP

**What it does:** Implements FIO (open-end) and FIZ (closed-end) investment funds with standalone `OpenEndFundData` / `ClosedEndFundData` structs. Includes spawn triggers (savings thresholds), bankruptcy/exit triggers, portfolio valuation, and a sophisticated `execute_fire_sale` function with closed-loop buyer=receiver mechanics.

**Usage:** Never called. All public items are re-exported in `corporate/mod.rs` but no code anywhere uses them.

**Duplication:** This is a **parallel implementation** of `securities/funds.rs`, which is the LIVE system. The live system uses `Company` entities with `fund_type` and `fund_ledger`, integrates with brokerage accounts and the stock exchange, and is called from `engine/turn.rs` (lines 1691-1788). The dead module uses custom `Region`/`Country` structs incompatible with the live `state::Country`.

**Unique mechanics worth preserving:**
- `execute_fire_sale` — sophisticated fire-sale logic with forced IPO for 100% ownership cases, dynamic pricing, and proportional market impact on confidence/share prices.
- Spawn trigger thresholds (savings > average_wage * N).

**Salvage plan:** Extract `execute_fire_sale` logic into `securities/funds.rs` if fund liquidation is needed. Extract spawn trigger thresholds into the live fund spawn logic. Then delete the module.

**Verdict: SCRAP.** The architecture is fundamentally incompatible. The live `securities/funds.rs` already handles fund lifecycle. Only the fire-sale algorithm is worth porting.

---

### 1.2 `corporate/development.rs` (12 KB) — SALVAGE

**What it does:** Implements `PropertyDeveloper`, an AI agent for automated property development. Evaluates market opportunities (housing shortage, commercial vacancy, ROI), recommends project types (residential/commercial), estimates costs/materials/labor, and creates `ConstructionProject` instances.

**Usage:** Never called. `PropertyDeveloper` is never instantiated. `evaluate_market_opportunity` and `create_project` are never invoked.

**Duplication:** Material BOM estimation partially duplicates `construction/bom.rs`. Project creation follows a similar pattern to `construction/tender_market.rs`. However, the **market analysis logic is unique** — no other module calculates housing shortage, commercial vacancy, or construction ROI.

**Unique mechanics:**
- Housing shortage calculation (population vs capacity by type)
- Commercial vacancy rate analysis (currently placeholder: hardcoded 0.2)
- ROI calculation for construction projects
- Project type recommendation engine
- Risk-tolerance-based decision making

**Salvage plan:** Integrate with the existing tender market system (`construction/tender_market.rs`):
1. Make `PropertyDeveloper` an "investor AI" that publishes tenders instead of creating projects directly.
2. Add a turn-loop phase that lets developers evaluate opportunities and publish tenders.
3. Fix the placeholder `calculate_commercial_vacancy` to use actual `CommercialBuilding` occupancy data.
4. Add housing inventory access to the developer's evaluation context.

**Verdict: SALVAGE.** The market analysis and ROI calculation logic is unique and valuable. Integration effort: 2-3 hours via tender market system.

---

### 1.3 `corporate/bounded_rationality.rs` (4.7 KB) — SALVAGE

**What it does:** Implements a 5-tier "fog of war" information asymmetry system (`InformationQuality`): Blind, Local, National, Global, Predictive. Companies with more capital get better market data. Includes `try_upgrade_to_predictive` which consumes `MarketResearch` commodities.

**Usage:** Never called. Exported but unused.

**Duplication:** No duplication. The `capital_intensity.rs` module uses a similar multiplier pattern (10x, 100x, 1000x, 10000x, 100000x average_wage) but for a different purpose (capital requirements, not information access).

**Unique mechanics:**
- Information asymmetry / fog of war for corporate AI
- Market Research commodity consumption for tier upgrades
- Decision accuracy modulation (planned: ±30% error for Blind, exact for Predictive)

**Salvage plan:** This was designed for Phase 22 construction bidding accuracy. Hook it in:
1. Add `information_quality: InformationQuality` field to `Company` (with `#[serde(default)]`).
2. Call `determine_information_quality()` at the start of `process_company()`.
3. Use the tier to modulate bid cost estimation accuracy in construction tenders.
4. Use the tier to modulate expansion investment decisions (over/under-estimating demand).
5. Implement `MarketResearch` commodity consumption via `try_upgrade_to_predictive()`.

**Verdict: SALVAGE.** The information asymmetry mechanic is unique, well-tested (6 unit tests), and was explicitly designed for Phase 22. Integration effort: 4-5 hours.

---

### 1.4 `corporate/capital_intensity.rs` (3.5 KB) — SALVAGE

**What it does:** Defines `CapitalIntensity` enum (Micro/Low/Medium/High/Massive) and maps each `Sector` to a tier. `minimum_capital_for_sector()` calculates dynamic entry barriers scaled by average_wage.

**Usage:** **Partially live.** `sector_capital_intensity()` is called from `infrastructure/building_condition.rs` for renovation BOM calculations. However, `minimum_capital_for_sector()` is **never called** — the primary purpose of the module (entry barriers) is unused.

**Duplication:** The codebase uses hardcoded capital formulas elsewhere:
- `engine/generator/corporate.rs`: `target_emp * base_wage * 2.0` (no sector differentiation)
- `corporate/lifecycle.rs`: `capital_per_company * 0.5` (no sector minimums)
- `state/special_economic_zones.rs`: hardcoded `minimum_fixed_capital` per zone

**Unique mechanics:**
- Sector-aware capital requirements (Energy = 100,000x wage, NGO = 10x wage)
- Dynamic inflation scaling via average_wage index
- Entry barrier enforcement (currently absent from the simulation)

**Salvage plan:**
1. Add validation to `spawn_new_companies` in `corporate/lifecycle.rs` — skip spawning if capital < sector minimum.
2. Add validation to `engine/generator/corporate.rs` — ensure seed companies meet sector minimums.
3. Optionally add entry barrier enforcement in the turn loop for undercapitalized companies.
4. Replace hardcoded SEZ minimums with calculated values.

**Verdict: SALVAGE.** The module is already partially live. Hooking up `minimum_capital_for_sector()` would add realistic entry barriers. Integration effort: 1-2 hours for basic validation.

---

### 1.5 `corporate/unions.rs` (5.2 KB) — ALREADY LIVE (No action needed)

**What it does:** Implements union militancy, strikes, dues collection, and member recruitment.

**Usage:** **Fully integrated.** Called from `engine/turn.rs` line 352 (Phase 4). Unions are loaded/saved via `DiskEntityStore<Union>`.

**Verdict: KEEP AS-IS.** This module is not dead code. The audit incorrectly flagged it. It is production-ready and fully wired into the turn loop.

---

### 1.6 `economy/real_estate.rs` (5.1 KB) — SALVAGE (partial)

**What it does:** Implements shopping center mechanics: retail rent accrual, lease signing, diversity bonus calculation, anchor tenant updates.

**Usage:** **Partially live.** 3 of 4 functions are called in Phase R7 (`turn.rs` lines 1559-1573):
- `accrue_retail_rents` — called, but rent is not transferred to any financial account.
- `calculate_diversity_bonus` — called, but result is discarded (`let _diversity_bonus`).
- `update_anchor_tenant` — called, but uses hardcoded `tenant_sales = 1000.0`.
- `sign_retail_leases` — **NOT called**. Logic is duplicated inline in `turn.rs` lines 1575-1634.

**Unique mechanics:**
- Retail lease lifecycle (signing, expiration, rent accrual)
- Diversity bonus for shopping center attractiveness
- Anchor tenant identification

**Salvage plan:**
1. Replace inline lease-signing in `turn.rs` (lines 1575-1634) with a call to `sign_retail_leases`.
2. Route rent payments through `TransferSettler` — currently rent is "collected" but no money moves.
3. Apply diversity bonus to `RetailProfile.effective_attractiveness` in Phase R2.
4. Fix `update_anchor_tenant` to use actual sales data instead of hardcoded 1000.0.
5. Fix `calculate_diversity_bonus` to use actual store profiles instead of hardcoded Grocery/Clothing.

**Verdict: SALVAGE.** The module is partially integrated but has placeholder logic and missing financial wiring. Integration effort: 3-4 hours.

---

### 1.7 `economy/corporate_rd.rs` (10 KB) — SALVAGE (partially live)

**What it does:** Implements corporate R&D: budget allocation, method research (patent discovery), licensing opportunity evaluation, and patent expiration.

**Usage:** **Partially live.** 2 of 4 functions are called from `turn.rs` line 2051-2058:
- `allocate_corporate_rd_budget` — called (allocates cash to rd_budget)
- `check_patent_expiration` — called (removes expired patents)
- `execute_corporate_method_research` — **NOT called** (companies never discover techs)
- `evaluate_licensing_opportunities` — **NOT called** (companies never license methods)

**Bugs found:**
- Line 224: `licensed_turn: 0` should be `current_turn` — needs signature change.
- Lines 116-119: Prerequisite check is skipped (comment says "requires State's discovered techs" but it's not implemented).
- Line 269: `estimate_new_unit_cost` returns placeholder 40.0.
- Line 216: Royalty cost uses simplified VWAP estimate (100.0).

**Salvage plan:**
1. Add `execute_corporate_method_research()` call in `turn.rs` after line 2058.
2. Add `evaluate_licensing_opportunities()` call after research.
3. Fix `licensed_turn` bug by adding `current_turn: u32` parameter.
4. Implement prerequisite checking using `treasury.science.discovered`.
5. Improve placeholder cost estimates with actual market data.

**Verdict: SALVAGE.** The module is 50% integrated and provides unique corporate R&D mechanics. Integration effort: 2-3 hours for basic hooking, 4-6 hours with bug fixes.

---

## Part 2: One-Way Doors & Incomplete Mechanics

### 2.1 R&D Investment Without Royalty Collection

**File:** `economy/corporate_rd.rs`, lines 36-38, 122-124
**Issue:** Companies spend cash on R&D and receive patents, but `evaluate_licensing_opportunities` (the function that would license OUT patents and collect royalties) is never called. Money flows into R&D but never returns as royalty income.
**Fix:** Hook `evaluate_licensing_opportunities` into the turn loop. Ensure `process_all_royalty_payments` in `royalties.rs` actually credits patent holders.

### 2.2 Infrastructure Funding Without Revenue Return

**File:** `economy/state_sector/infrastructure.rs`, lines 42-44, 137-177
**Issue:** Money is deducted from treasuries/companies and added to building reserves, but infrastructure outputs (Innovation Points, Health Capacity, Education Slots) are consumed as free public goods with no revenue stream back to the owner.
**Fix:** Implement tolls, fees, or service charges for infrastructure usage. Alternatively, model infrastructure as a public good funded by taxes (current implicit model) and document it as intentional.

### 2.3 Sovereign Default Forex Lockout (No Auto Re-entry)

**File:** `state/forex.rs`, lines 527-530, 234-246
**Issue:** When a country defaults on trade deficits, it's locked out of Forex for 12 turns. `unlock_country()` exists but is never called automatically. Countries remain locked out indefinitely.
**Fix:** Add automatic unlock when `sovereign_default_turns_remaining` reaches 0. Check in the turn loop's forex processing phase.

### 2.4 Debt Market Binary Lockout

**File:** `economy/finance/debt_market.rs`, lines 679, 365-367, 808-826
**Issue:** Default locks a country out of primary debt issuance until ALL arrears are cleared. Even 99% repayment leaves the country fully locked out.
**Fix:** Implement partial re-entry based on arrears repayment percentage. Allow limited issuance at higher interest rates when arrears are partially cleared.

### 2.5 Credit Rating Asymmetric Recovery

**File:** `economy/finance/debt_market.rs`, lines 681-683, 721-724
**Issue:** Credit rating crashes 3 notches on default but recovers only 1 notch per turn. A single default requires 3 turns of perfect payment to restore standing.
**Fix:** Either reduce the crash to 2 notches, or increase recovery to 2 notches per turn, or make recovery proportional to payment ratio.

### 2.6 Inventory Overflow Destruction

**File:** `economy/trade/b2b_orders.rs`, lines 983-996, 1021-1023
**Issue:** When inventory exceeds capacity and no warehouse is available, overflow is destroyed. A write-down is recorded in `building.last_profit`, but the commodity value is lost from the system with no compensation.
**Fix:** Implement insurance mechanisms or salvage value for destroyed inventory. Alternatively, route overflow to a fire-sale auction pool (similar to bankruptcy auction pool).

### 2.7 Perishable Goods Decay (Double Loss)

**File:** `engine/turn.rs`, lines 1479-1489
**Issue:** Agricultural goods decay in warehouse inventory. The owner pays rot fees to the warehouse owner, AND the commodity value itself is destroyed. This is a double loss: commodity value + fee payment.
**Fix:** This may be intentional (realistic modeling of perishability). However, the rot fees should be the warehouse owner's revenue, not an additional penalty. Verify that rot fees are credited to the warehouse owner's account.

### 2.8 Utility Grid Consumption Without Payment

**File:** `utilities/grid.rs`, lines 70-81
**Issue:** Energy and Heat commodities are consumed from building inventory to power the grid, but there's no direct payment mechanism back to the energy producers.
**Fix:** Implement direct billing for energy/heat consumption. Route payments through `TransferSettler` from grid consumers to energy producers.

### 2.9 Arrears Capitalization (Creditors Underpaid)

**File:** `economy/finance/debt_market.rs`, lines 669-716, 699-709
**Issue:** When a country defaults, unpaid interest is capitalized as arrears (added to principal). Creditors receive increased principal balance instead of actual cash payment. This is not equivalent due to time value of money.
**Fix:** This is a standard sovereign debt restructuring mechanism. However, ensure that capitalized arrears eventually convert to cash payments when the country recovers. Add a "arrears repayment" phase that pays down capitalized arrears when treasury reserves are sufficient.

---

## Part 3: Remaining Black Holes

### Black Hole #4: `order_book.rs` submit_bid() — liquid_capital debit without credit

**File:** `economy/market/order_book.rs`, line 340
**Severity:** HIGH (if `submit_bid()` is used by any live code path)

**Issue:** `submit_bid()` debits `company.liquid_capital` to prevent double-spending, but there is no corresponding credit mechanism. When bids are filled or cancelled, `liquid_capital` is never restored. The refund functions credit `available_cash` or `brokerage_account.cash`, never `liquid_capital`.

**Impact:** Any entity using `submit_bid()` from `order_book.rs` (not the b2b_orders wrapper) permanently loses money.

**Fix:** Either:
- (a) Remove the `liquid_capital` debit from `submit_bid()` and rely on the b2b_orders wrapper for encumbrance, OR
- (b) Add a `liquid_capital` credit to the refund functions, OR
- (c) Verify that `submit_bid()` is never called directly (only via b2b_orders wrapper) and mark it as internal-only.

**Note:** The Phase 24A.1 fix routed B2B refunds through `b2b_orders::refund_unfilled_bids`, which correctly credits `debit_cash` and `available_cash`. If `submit_bid()` is only called via the b2b_orders wrapper, this black hole may be dormant. Verify call sites.

---

### Black Hole #5: Ministries — InfrastructureFunding & PublicServiceWages

**File:** `politics/ministries.rs`, lines 694-698, 701-705, 885-893, 953-957
**Severity:** MEDIUM

**Issue:** Ministries log spending actions by incrementing `ministry.spent_cash`, but no actual cash is debited from the treasury and no recipient is credited. The comment at line 887 admits: "For now, record as a spending action. The actual building reserve update happens when buildings are processed in Phase 7." — but no such update occurs in Phase 7.

**Impact:** Ministry budget accounting is broken. Cash appears "spent" in the ministry ledger but remains in the treasury. No building or citizen receives the funds.

**Fix:** Either:
- (a) Debit `country.budget.liquid_reserves` when `ministry.spent_cash` is incremented, and credit the appropriate recipient (building reserve, citizen savings, etc.), OR
- (b) If the spending is purely notional (planning, not execution), document it as such and don't count it against any budget.

---

### Black Hole #6: Ministries — B2B Procurement (Money Creation)

**File:** `politics/ministries.rs`, lines 648-666
**Severity:** HIGH

**Issue:** When ministries submit B2B bids, they increment `ministry.spent_cash` but don't debit any actual cash account. The `Ministry` struct has no `available_cash` or `liquid_capital` field. When trades execute, sellers are credited but no buyer is debited. **Money is created from nothing.**

**Impact:** Ministry B2B procurement creates money. Sellers receive cash that was never debited from any account.

**Fix:** Either:
- (a) Debit `country.budget.liquid_reserves` when ministry bids are submitted, and refund if unfilled, OR
- (b) Give ministries their own cash account funded from the treasury allocation, and debit it for bids.

---

### Documented Gap: Dividend Routing to Citizen Shareholders

**File:** `corporate/manager.rs`, line 237
**Issue:** `// TODO: Route to citizen savings for individual shareholders.`
**Impact:** Dividends to non-company, non-state shareholders (individual citizens, demographic classes) are currently lost. The dividend queue only credits companies and the treasury.
**Fix:** Add citizen shareholder routing via `ClassDemographics.savings` in regional demographics. This requires tracking per-class share ownership.

---

## Summary Table

| Module | Size | Status | Verdict | Effort |
|--------|------|--------|---------|--------|
| `corporate/funds.rs` | 27.3 KB | Dead | **SCRAP** (extract fire-sale logic) | 2h extraction |
| `corporate/development.rs` | 12 KB | Dead | **SALVAGE** (tender market integration) | 2-3h |
| `corporate/bounded_rationality.rs` | 4.7 KB | Dead | **SALVAGE** (fog of war for AI) | 4-5h |
| `corporate/capital_intensity.rs` | 3.5 KB | Partially live | **SALVAGE** (hook entry barriers) | 1-2h |
| `corporate/unions.rs` | 5.2 KB | Fully live | **KEEP** (no action needed) | 0h |
| `economy/real_estate.rs` | 5.1 KB | Partially live | **SALVAGE** (fix placeholders, wire rent) | 3-4h |
| `economy/corporate_rd.rs` | 10 KB | Partially live | **SALVAGE** (hook 2 unused functions) | 2-3h |

| Black Hole | Severity | Fix Effort |
|------------|----------|------------|
| #4: order_book submit_bid liquid_capital | HIGH (if live) | 1-2h |
| #5: Ministry infrastructure/wage spending | MEDIUM | 2-3h |
| #6: Ministry B2B procurement money creation | HIGH | 2-3h |
| Dividend citizen routing TODO | MEDIUM | 3-4h |

| One-Way Door | Severity | Fix Effort |
|--------------|----------|------------|
| R&D without royalty collection | HIGH | 2-3h (hook corporate_rd) |
| Infrastructure without revenue | LOW (may be intentional) | 4-6h |
| Forex lockout no auto re-entry | HIGH | 1h |
| Debt market binary lockout | MEDIUM | 2-3h |
| Credit rating asymmetric recovery | LOW | 1h |
| Inventory overflow destruction | MEDIUM | 2-3h |
| Perishable decay double loss | LOW (verify) | 1h |
| Utility consumption without payment | MEDIUM | 3-4h |
| Arrears capitalization | LOW (standard) | 2-3h |

---

## Recommended Action Priority

### Tier 1 — Critical (fix before any new features)
1. **Black Hole #6:** Ministry B2B procurement creates money from nothing.
2. **Black Hole #4:** Verify and fix `order_book.rs` `submit_bid()` liquid_capital debit.
3. **Forex lockout auto re-entry:** Countries permanently locked out of trade.
4. **R&D royalty collection:** Hook `execute_corporate_method_research` and `evaluate_licensing_opportunities`.

### Tier 2 — High Value (significant economic realism improvement)
5. **Salvage `corporate_rd.rs`:** Hook the 2 unused functions + fix bugs.
6. **Salvage `capital_intensity.rs`:** Add entry barrier validation to company spawning.
7. **Salvage `bounded_rationality.rs`:** Information asymmetry for construction bidding.
8. **Salvage `real_estate.rs`:** Fix placeholders, wire rent payments, apply diversity bonus.
9. **Black Hole #5:** Wire ministry spending to actual treasury debits.

### Tier 3 — Medium Value (polish and completeness)
10. **Salvage `development.rs`:** Integrate PropertyDeveloper with tender market.
11. **Debt market partial re-entry:** Allow limited issuance with partial arrears repayment.
12. **Inventory overflow fire-sale:** Route to auction pool instead of destruction.
13. **Utility grid payment:** Implement direct billing for energy/heat.
14. **Dividend citizen routing:** Route dividends to ClassDemographics.savings.

### Tier 4 — Low Priority (may be intentional or minor)
15. **Scrap `corporate/funds.rs`:** Extract fire-sale logic, delete the rest.
16. **Credit rating recovery balance:** Adjust recovery rate.
17. **Perishable decay verification:** Confirm rot fees credit to warehouse owner.
18. **Arrears repayment phase:** Add cash repayment of capitalized arrears.
19. **Infrastructure revenue model:** Decide if public goods model is intentional.

---

## Awaiting Approval

**No code changes will be made until explicit approval is received on the Salvage/Scrap recommendations and the priority tiers above.**
