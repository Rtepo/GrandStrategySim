# SillyElaborateState — Grand Architectural Codex

**Status:** Phase 93 complete — consolidated reference for next development phase.  
**Date:** 2026-08-31  
**Crate root:** `state/`  
**Repository root:** `C:\Users\netse\Downloads\SillyElaborateState`  

> This document is the successor to the outdated Phase 45 codex. It captures the exact mechanics, math, and data flows of the newly implemented macroeconomic, demographic, geographic, and political systems. It concludes with the Phase 93 deep-sweep audit findings. **No new feature code was written to produce this document.**

---

## Table of Contents

1. [Macro-Banking & Double-Entry Accounting](#1-macro-banking--double-entry-accounting)
2. [Corporate Lifecycle & Production](#2-corporate-lifecycle--production)
3. [Demography & Labor Markets](#3-demography--labor-markets)
4. [Geography & Resources](#4-geography--resources)
5. [Politics & Governance](#5-politics--governance)
6. [Audit Findings](#6-audit-findings)

---

## 1. Macro-Banking & Double-Entry Accounting

### 1.1 Balance-Sheet Architecture

The canonical bank balance sheet is defined in `state/src/state/banking.rs:183-257`.

| Side | Items | Definition |
|------|-------|------------|
| **Assets** | `reserves_at_central_bank` | Vault reserves at the CB. |
| | `cb_deposit_facility_balance` | Excess reserves parked at the CB. |
| | `loans_issued: Vec<Loan>` | Outstanding credit assets. |
| | `interbank_loans_given` | Overnight liquidity lent to other banks. |
| | `securities` | Bonds & liquid instruments. |
| | `mbs_holdings` | MBS / covered-bond holdings. |
| | `real_estate` | Owned physical premises. |
| **Liabilities** | `deposits` | Demand + term deposits. |
| | `cb_lombard_loans` | Emergency CB borrowing. |
| | `interbank_loans_taken` | Overnight borrowing from other banks. |
| | `issued_bonds` | Bank-issued debt. |
| **Equity** | `tier_1_capital` | Common equity + retained earnings. |

**Invariant:** `Assets = Liabilities + Equity` to within `1e-6` (`BankBalanceSheet::is_balanced`, `state/src/state/banking.rs:296-300`).

### 1.2 Fractional Reserve & Reserve Requirement

At generation, the default reserve requirement is `0.10` (`default_reserve_requirement_ratio`, `state/src/state/banking.rs:38-40`).

| Metric | Formula | Location |
|--------|---------|----------|
| Reserve position | `reserves_at_central_bank - (deposits * cb_reserve_ratio)` | `state/banking.rs:336-339` |
| Effective reserves for lending | `reserves_at_central_bank - cb_lombard_loans` | `state/banking.rs:802` |
| Meets reserve req. | `reserves_at_central_bank >= deposits * cb_reserve_ratio` | `state/banking.rs:324-327` |

The older `state/src/economy/banking.rs` still exists as a port of the legacy Python banking turn, but the active turn loop now calls `process_banking_turn` from `state/src/state/banking.rs`.

### 1.3 Tier 1 Capital Generation

At world generation (`state/src/engine/generator/mod.rs:1748-1774` and `state/src/engine/generator/corporate.rs:1140`):

```rust
const TARGET_TIER_1_RATIO: f64 = 0.12; // 1.5x the 8% KNF minimum
let estimated_total_assets = total_deposits + reserves + estimated_loan_exposure;
let tier_1_from_ratio = estimated_total_assets * TARGET_TIER_1_RATIO;
let tier_1_from_gdp = treasury.gdp * 0.05 * size_factor / num_banks as f64;
let tier_1_capital = tier_1_from_ratio.max(tier_1_from_gdp);
```

* `BankBalanceSheet.tier_1_capital` is the equity line.
* Regulatory intent: Tier 1 must be >= 6% of risk-weighted assets (`state/banking.rs:250`).

### 1.4 Working Capital Loans

#### Genesis seeding
`issue_working_capital_loans` (`state/src/engine/generator/corporate.rs:925-1188`) issues 24-turn loans to newly spawned companies.

```rust
let payroll_principal = initial_fte * initial_wage * 4.0;
let base_principal = payroll_principal + seed_cost;
let per_turn_debt_service = base_principal * (estimated_annual_rate / 24.0 + 1.0 / 24.0);
let debt_service_reserve = per_turn_debt_service * 3.0;
let overhead_reserve = payroll_principal * 0.05 * 3.0;
let principal = base_principal + debt_service_reserve + overhead_reserve;
```

* Rate: `xibor + estimated_bank_margin + sector_risk_premium`.
* Collateral: `company.fixed_capital * 0.8`.
* Per-bank exposure cap: `bank_total_lent < bank_tier1 * 10.0`.
* Double-entry at birth:
  * `company.available_cash += principal; company.liabilities += principal;`
  * `bank.balance_sheet.loans_issued.push(loan); bank.balance_sheet.deposits += principal;`
  * **Reserves do not change at creation** — they move only during clearing.

#### In-turn corporate working capital
`process_companies` (`state/src/corporate/manager.rs:121-197`) turns `process_company` liability increases into real bank loans. Banks are sorted by excess reserves and the first with capacity issues a loan via `issue_loan(..., LoanType::WorkingCapital, 12 turns, ...)`. If no bank can issue, the liability increase is reverted.

#### Loan issuance invariants
`issue_loan` (`state/src/state/banking.rs:763-855`) enforces:

1. Credit scoring (`calculate_credit_score`).
2. Rate: `xibor + bank_margin + risk_premium_bps / 10000.0`.
3. Reserve check: `effective_reserves >= required_reserves` after simulating `new_deposits`.
4. Creates `Loan` record and pushes it to `loans_issued` while increasing `deposits` by the principal.

### 1.5 Loan Servicing & KNF Logic

`process_banking_turn` Step 6 (`state/src/state/banking.rs:2328-2469`):

```rust
let interest = loan.outstanding_balance * annual_to_per_turn_rate(loan.interest_rate);
loan.outstanding_balance += interest; // compound
let principal_portion = loan.principal / loan.term_turns as f64;
let payment = principal_portion + interest;
let actual_payment = payment.min(loan.outstanding_balance);
```

Borrower cash is debited (`brokerage_account` then `available_cash`); the bank's `reserves_at_central_bank` is credited. Intra-bank deposits are reduced; inter-bank transfers move deposits/reserves between banks.

**KNF compliance** (`state/src/securities/knf.rs:289-386`):

* Tier-1 audit:
  ```rust
  let tier_1_ratio = balance_sheet.tier_1_capital / total_assets;
  if tier_1_ratio < knf.min_tier_1_ratio {
      let fine = severity * total_assets * config.knf_penalty_multiplier;
      balance_sheet.reserves_at_central_bank -= fine;
      balance_sheet.tier_1_capital -= fine;
      treasury.liquid_reserves += fine;
  }
  ```
* Leverage audit:
  ```rust
  if total_liabilities > tier_1_capital * 10.0 { ... fine ... }
  ```
* Freezes: `freeze_brokerage_account` sets `brokerage.is_frozen = true` for market manipulation, audit violation, or fraud.
* Circuit breakers: trading halted if market index drops > 10% in one turn.

**Bank resolution** (`state/src/state/banking.rs:1545-1694`) performs good-bank / bad-bank splits, wipes equity, protects insured deposits, and creates a 24-turn bridge bank owned by the BFG (Bank Guarantee Fund).

### 1.6 LDR Mechanics

There is **no explicit `loan-to-deposit` (LDR) cap or metric** in the current code. The only reference is the historical "8193% LDR anomaly" comment in `state/src/corporate/manager.rs:120`. An implicit LDR can be derived as:

```rust
let ldr = bs.loans_issued.iter().map(|l| l.outstanding_balance).sum::<f64>() / bs.deposits;
```

The old anomaly is guarded by `issue_loan` reserve checks, competitive allocation by excess reserves, and `bank_operational_capacity` labor limits.

### 1.7 Money-Creation Channels

| Channel | Mechanism | File / Lines |
|---------|-----------|--------------|
| **Commercial bank loans** | `issue_loan` creates `loans_issued` asset + `deposits` liability; reserves unchanged. | `state/banking.rs:837-844` |
| **CB OMO** | CB buys bonds: bank `securities` ↓, `reserves_at_central_bank` ↑. Sells: opposite. | `state/banking.rs:2225-2271`, `state/central_bank.rs:434-473` |
| **CB Lombard** | Bank deficit → `cb_lombard_loans` ↑ and `reserves_at_central_bank` ↑. | `state/banking.rs:2307-2326` |
| **CB QE for deflation** | If `inflation < 0%`, CB buys up to `gdp * 0.05` bonds from DSPW banks. | `state/banking.rs:2901-2936` |
| **Interbank clearing** | Surplus banks lend to deficit banks; reserves physically move. | `state/banking.rs:394-476` |
| **B2C consumer loans** | `class.savings += loan_amount; class.debt += loan_amount`. | `state/banking.rs:2743-2899` |

### 1.8 TransferSettler — Double-Entry Settlement

`settle_transfer` (`state/src/economy/trade/transfer_settler.rs:149-284`) is the canonical cash router between companies and the state.

**On a company payment:**
1. `company.brokerage_account.cash -= amount` (or `available_cash`).
2. If not intra-bank: `bank.balance_sheet.deposits -= amount` and `bank.balance_sheet.reserves_at_central_bank -= amount`.
3. Recipient is credited (`Treasury`, `CitizenSavings`, `OtherCompany`, `ForeignEntity`, or `CentralBank`).

**On a company receipt (e.g. B2C):** the inverse journal is applied.

**Key pre-checks:**
* `would_cause_negative_reserves` rejects the transfer if reserves would go negative (`state/transfer_settler.rs:113-127`).
* FX transfers require `total_fx >= amount` (`state/transfer_settler.rs:166-171`); domestic deposits are extinguished and CB FX reserves are drawn down proportionally.
* Intra-bank transfers skip reserve/deposit adjustments.

Convenience wrappers: `settle_transfer_to_treasury`, `settle_wage_payment`, `settle_company_to_company`, `settle_treasury_to_company`, `settle_b2c_purchase`.

---

## 2. Corporate Lifecycle & Production

### 2.1 Birth & Seed Capital Extraction

Company genesis (`state/src/engine/generator/corporate.rs:1445-1652`, `state/src/entities/mod.rs:683-787`):

```rust
let company_fixed = region_fixed * capital_share;
let company_liquid = region_liquid * capital_share;
let company_capital = company_fixed + company_liquid;
let initial_wage = (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0);
let initial_fte = (actual_capacity as f64 * 0.6).round().max(2.0);
```

An `ActiveProductionMethod` and a `Building` are created. The building is seeded with 2 turns of inputs (4 turns for agriculture) plus 1 turn of output inventory and a food buffer.

**Seed inventory cost extraction** (`state/src/engine/generator/corporate.rs:1649-1652`, `629-637`):

```rust
let deductible = seed_cost.min(company.liquid_capital * 0.5);
company.liquid_capital -= deductible;
company.available_cash -= deductible;
company.extra.insert("seed_inventory_cost", Value::from(deductible));
```

At the country level, the sum of `seed_inventory_cost` is credited to `country.budget.liquid_reserves`, so seed stock is not free money.

### 2.2 Working Capital Allocation

In addition to genesis loans, the in-turn pipeline is:

1. `process_company` may increase `liabilities` (e.g. `Expand` action financed by `FinanceSource::BankLoan`).
2. `process_companies` detects `liabilities_after > liabilities_before + 0.01`.
3. It collects eligible banks, sorts by `excess = (reserves - cb_lombard_loans) - (deposits * reserve_ratio)`, and calls `issue_loan`.
4. On success: `company.available_cash += principal`, `brokerage_account.cash += principal`, `outstanding_loan_bank_id = bank_id`.
5. On failure: liability increase is **reverted**.

**Operational cash** for payroll/furlough (`state/src/entities/mod.rs:807-810`):

```rust
pub fn operational_cash(&self) -> f64 {
    self.available_cash.max(0.0)
        + self.brokerage_account.as_ref().map(|b| b.cash.max(0.0)).unwrap_or(0.0)
}
```

### 2.3 Production Cycle

**Pre-clearing profit forecast** (`state/src/economy/production/production.rs:162-374`):

```rust
let base_employment = building.current_employment.min(building.worker_capacity) as f64;
let scale = building.scale_factor.max(1) as f64;
let effective_employment = base_employment * scale * (1.0 - disruption_factor.clamp(0.0, 1.0));
let production_scale = effective_employment / 1000.0;
let wages_paid = effective_employment * wage_multiplier * base_wage * opex_multiplier;
```

BOM inputs and outputs scale per 1000 workers:

```rust
input_amount = amount_per_1k * production_scale;
output_revenue += amount_per_1k * production_scale * price;
```

Gross profit:

```rust
building.last_profit = output_revenue - input_costs - wages_paid;
```

**Physical production execution** (`state/src/economy/trade/b2b_orders.rs:998-1219`) runs in Wave 1 (energy) and Wave 3 (all other sectors). Fulfillment is input-constrained:

```rust
let mut fulfillment_ratio = 1.0;
for (&commodity, &qty_per_1k) in &method.inputs {
    let required = qty_per_1k * production_scale;
    let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
    fulfillment_ratio = (available / required).min(fulfillment_ratio);
}
fulfillment_ratio = fulfillment_ratio.clamp(0.0, 1.0);
let produced = qty_per_1k * production_scale * fulfillment_ratio * efficiency * machinery_factor;
```

Wages are **not** paid during this step; they are settled in the labor-market phase.

### 2.4 Revenue & Corporate Accounting

`process_companies` (`state/src/corporate/manager.rs:39-258`) aggregates:

```rust
let total_profit: f64 = owned.iter().map(|j| buildings[*j].last_profit).sum();
let avg_fulfillment_ratio: f64 = ...; // average of building.last_fulfillment_ratio
```

`process_company` (`state/src/corporate/manager.rs:630-837`):

```rust
company.liquid_capital += total_profit;
if company.liquid_capital < 0.0 {
    company.liabilities += -company.liquid_capital;
    company.liquid_capital = 0.0;
}
company.company_capital = company.fixed_capital + company.liquid_capital - company.liabilities;

let overhead = (total_profit * 0.05).max(0.0);
let leverage = company.liabilities / company.fixed_capital.max(1.0);
let risk_margin = (leverage * 0.03).min(0.12);
let interest_cost = company.liabilities * (xibor + risk_margin);
let interest = interest_cost.min(total_profit * 0.5);
let taxable_income = total_profit - overhead - interest;
let tax = if taxable_income > 0.0 { taxable_income * corporate_tax_rate } else { 0.0 };
country.budget.liquid_reserves += tax;
let net_profit = total_profit - overhead - interest - tax;
```

The financial history record stores `revenue`, `operating_costs`, `wage_expense`, and `net_profit`.

### 2.5 Furloughs vs. Grace Periods

**Material-shortage grace period** (`state/src/corporate/strategy.rs:768-808`):

| Sector | Grace length | Expiration condition |
|--------|--------------|----------------------|
| Agriculture | 24 turns (1 year) or `current_turn - founded_turn >= 24` | First non-zero `revenue` in `financial_history` |
| Mining, HeavyIndustry, LightIndustry, Energy, Construction | 12 turns | First non-zero `revenue` in `financial_history` |
| Other | Turn 1 only | `financial_history` no longer empty |

**Furlough evaluation** (`state/src/corporate/strategy.rs:818-874`):

```rust
let wage_per_fte = ctx.company.offered_wage_per_fte.max(1.0);
let total_payroll = ctx.company.fulfilled_fte as f64 * wage_per_fte;
let cash_shortage = ctx.company.operational_cash() < total_payroll * 2.0;
let material_shortage = ctx.avg_fulfillment_ratio < 0.1
    && !is_within_material_shortage_grace(ctx.company, ctx.current_turn);

let furlough_count = if material_shortage {
    ((ctx.company.fulfilled_fte as f64) * (1.0 - ctx.avg_fulfillment_ratio)).ceil() as u32
} else {
    let affordable_fte = (ctx.company.operational_cash() / wage_per_fte).floor() as u32;
    ctx.company.fulfilled_fte.saturating_sub(affordable_fte)
};
let furlough_count = furlough_count.min(ctx.company.fulfilled_fte).max(1);
```

Furloughs move workers to `furloughed_workers_count`, remove them from active `fulfilled_fte`, and exclude them from the labor market. Reinstatement is free when conditions improve. Attrition applies each turn:

```rust
let quit_rate = (0.05 * (1.0 - wage_fraction) * (1.0 + furlough_turns * 0.10)).min(0.50);
let quit_count = (furloughed_workers_count * quit_rate).ceil() as u32;
```

Currently `wage_fraction = 0.0` for all furloughs (era-appropriate unpaid furlough).

### 2.6 Bankruptcy / Death Conditions & Liquidation

`CompanyLifecycle::liquidate_bankrupt_companies` (`state/src/corporate/lifecycle.rs:65-177`):

**Death triggers:**
1. Negative equity: `company_capital < 0.0`.
2. Sustained losses: 3+ consecutive years of `net_profit < 0.0` (requires at least 2 history records; the grace period prevents infant mass bankruptcy).

**Strategic energy exception:** Energy companies in distress are marked for receivership instead of immediate liquidation.

**Liquidation routing:**
1. Outstanding loans to the company are marked `Default` in every bank.
2. Buildings owned by the company are added to `country.bankruptcy_auction_pool`. Heritage buildings are protected (`owner_id` cleared, capacity preserved).
3. Frozen cash is removed from `justice_state.frozen_company_cash`.
4. Exchange listings (`EQUITY:{company_id}`) are removed.
5. `company.liquid_capital.max(0.0)` is credited to `bankruptcy_auction_pool.cash_collected`.
6. The company is removed from the companies vector and dead buildings are retained-out of the buildings vector.

The lifecycle is called in `state/src/engine/turn.rs:3332-3338`, after `process_companies`.

---

## 3. Demography & Labor Markets

### 3.1 Rural Class Distinction

The rural class structure is defined in `state/src/society/geography.rs:748-757`:

```rust
pub enum RuralClass { Aristocracy, FreePeasant, Serf, LandlessLaborer }
```

**World-generation rural shares by start year** (`state/src/society/geography.rs:1949-1968`):

| Era | Rural | Serf | FreePeasant | LandlessLaborer | Aristocracy |
|-----|-------|------|-------------|-----------------|-------------|
| 1900 | 80% | 20% | 40% | 35% | 5% |
| 1925 | 65% | 10% | 50% | 35% | 5% |
| 1950 | 50% | 0% | 55% | 40% | 5% |
| 1975+ | 40% | 0% | 60% | 35% | 5% |

**Class seed values** (`state/src/society/geography.rs:1975-2046`):

| Class | `labor_participation` | Savings seed |
|-------|----------------------|--------------|
| Serf | 0.65 | 0.0 |
| FreePeasant | 0.55 | `pop * 100.0 * dev_savings_mult` |
| LandlessLaborer | 0.60 | `pop * 50.0 * dev_savings_mult` |
| Aristocracy | 0.30 | `pop * 5000.0 * dev_savings_mult` |
| Urban Worker | 0.60 | `pop * 200.0 * dev_savings_mult` |
| Urban Bourgeoisie | 0.55 | `pop * 1000.0 * dev_savings_mult` |

### 3.2 Serf vs. Wage Labor

* **Serfs** are tied to `LatifundiumData` (`state/src/entities/legal_form.rs:429-458`). Their labor cost is `worker_capacity * serf_labor_cost_multiplier * market_wage` until the capacity exceeds the serf population, after which the surplus is paid at market wage. They have **no cash savings**; payment-in-kind deducts the full subsistence basket from the harvest instead of cash wages.
* **FreePeasant / LandlessLaborer** also receive subsistence in-kind, but the imputed value offsets the cash wage bill: `cash_offset += in_kind_value.min(class_wages)`.
* **Aristocracy** receive full cash wages, no in-kind deduction.

The in-kind imputation uses a fixed base price of `total_units * fte * 100.0` (`state/src/economy/finance/payment_in_kind.rs:257-258`).

### 3.3 Labor Intensity Ratios & Regional FTE Scaling

**Sector labor-intensity** (`state/src/engine/generator/mod.rs:1283-1362`) maps start year + sector to an employment-to-output ratio. For example at 1900:

```rust
Sector::Agriculture       => 2.5,
Sector::Mining            => 0.8,
Sector::HeavyIndustry     => 0.6,
Sector::LightIndustry     => 1.0,
```

**Regional available FTE** (`state/src/economy/labor/labor.rs:551-564`):

```rust
demo.available_fte = (pop * participation).min(pop * 1.5);
```

**Commuter inflow** (`state/src/engine/turn.rs:2518-2531`):

```rust
let commuter_inflow_fte =
    (coverage * 0.05 * adjacent_land as f64 * local_pool).min(local_pool * 0.5);
```

### 3.4 Natural Unemployment Baseline

At generation (`state/src/engine/generator/mod.rs:1377-1418`):

```rust
let unemployment_rate = rng.gen_range(3.0..15.0);
let workforce = (treasury.population as f64 * activity_rate / 100.0).max(1.0);
let employed_total = (workforce * (1.0 - unemployment_rate / 100.0)).max(0.0);
```

`activity_rate` is random `55.0..85.0` from culture (`state/src/society/cultures.rs:227`).

Per-turn frictional baseline (`state/src/economy/labor/labor.rs:303-318`):

```rust
let bezrobotni = (sila_robocza - labor_market.employed_total).max(0.0);
let frykcyjne_bazowe = if job_agency_active { 1.5 } else { 3.0 };
let stopa_bezrobocia = stopa_bezrobocia_surowa.max(frykcyjne_bazowe);
labor_market.unemployment_structure.friction = frykcyjne_bazowe / 100.0;
labor_market.unemployment_structure.cyclical = (pozostalo * 0.6) / 100.0;
labor_market.unemployment_structure.structural = (pozostalo * 0.4) / 100.0;
```

### 3.5 Wage Setting, Arrears & Strikes

**Target wage** (`state/src/corporate/manager.rs:1245-1395`):

```rust
const STICKY_WAGE_MAX_DROP: f64 = 0.03;
const STICKY_WAGE_MAX_RISE: f64 = 0.05;
const TARGET_WAGE_MAX_ADJUSTMENT: f64 = 0.02;
const TARGET_WAGE_FALLBACK: f64 = 50.0;

let desired_wage = if cash_per_fte > market_average_wage * 2.0 { market_average_wage * 1.1 }
    else if cash_per_fte < market_average_wage * 0.5 { market_average_wage * 0.9 }
    else { market_average_wage };

let adjustment = (desired_wage - company.target_wage).clamp(
    -company.target_wage * TARGET_WAGE_MAX_ADJUSTMENT,
     company.target_wage * TARGET_WAGE_MAX_ADJUSTMENT,
);
company.target_wage = (company.target_wage + adjustment).max(TARGET_WAGE_FALLBACK);
```

Final offered wage is clamped by `prev_offered_wage_per_fte * (1 ± sticky bounds)` and a sanity cap.

**Wage arrears** (`state/src/economy/labor/labor_market.rs:440-518`):

```rust
let wage_payment = company.fulfilled_fte as f64 * company.offered_wage_per_fte;
let actual_paid = if wage_payment <= available_cash { wage_payment } else { available_cash };
let arrears_this_turn = wage_payment - actual_paid;
company.wage_arrears += arrears_this_turn;
company.productivity_penalty = (company.wage_arrears / 10_000.0).min(0.50);
```

Arrears are repaid at `30%` of remaining cash per turn. FTE retention floor: `90%` of previous turn's `fulfilled_fte` even with zero cash, producing arrears instead of 100% layoffs.

**Strikes** (`state/src/corporate/unions.rs:104-259`):

```rust
let unemployment_factor = (unemployment_rate - 0.05).max(0.0) * 100.0;
let wage_pressure = if gdp_per_capita > 0.0 { (1.0 - (average_wage / gdp_per_capita)).max(0.0) * 50.0 } else { 0.0 };
let target_militancy = (unemployment_factor + wage_pressure - social_relief).clamp(0.0, 100.0);
union.militancy = union.militancy * 0.7 + target_militancy * 0.3;
```

* Strike trigger: militancy > 0.7 **or** layoff > 10%, with `union.strike_fund >= (avg_wage * 0.5).max(50.0)`.
* Simultaneous strike cap: `max(1, total_companies / 10)`.
* Strike pay: `strike_pay_per_fte = (avg_wage * 0.5).max(50.0)`. Striking workers are **not** paid by the company; the union pays benefits.
* Union dues: `1%` of company capital per turn.

---

## 4. Geography & Resources

### 4.1 Planet Vein Generation

The top-down planetary generation system lives in `state/src/society/planet.rs`.

**Rarity tiers** (`state/src/society/planet.rs:19-88`):

| Tier | Count range | Reserve range (tons) | Commodities |
|------|-------------|----------------------|-------------|
| UltraRare | 4-8 | 1M - 10M | Uranium, Gold |
| Rare | 8-15 | 5M - 50M | Silver, Tin |
| Uncommon | 10-20 | 20M - 200M | Copper, Zinc, Bauxite |
| AbundantIndustrial | 12-25 | 100M - 1B | Iron, HardCoal, BrownCoal, Stone, Sand |
| Ubiquitous | 20-40 | 500M - 5B | Limestone, Peat, Gravel |

`Planet::generate_veins` (`state/src/society/planet.rs:178-242`) places each vein at a random `lat/lon`, draws reserves/quality/depth, and computes `extraction_cost = 1.0 + (depth / 1000.0) + (1.0 - quality) * 0.5`. A vein overlaps a region if `|lat_diff| < 10.0` and `|lon_diff| < 10.0`.

`ensure_base_industrial_veins_per_region` (`state/src/society/planet.rs:318-449`) guarantees surface-visible base materials in populated regions using a deterministic hash of `(region_id, commodity)`:

* Ubiquitous materials in ~80% of regions.
* Industrial materials (Iron / HardCoal / BrownCoal) in ~35% of regions.
* Reserve/quality/depth are derived from hash bit slices, not a fresh RNG, ensuring determinism.

### 4.2 Regional Geographic Distribution

**Region count** (`state/src/society/geography.rs:1682-1686`):

```rust
fn region_count(population: i64, gdp_pc: f64) -> usize {
    let base = (population / 2_000_000).max(4).min(15) as f64;
    let multiplier = 1.0 + (gdp_pc / 100_000.0) * 0.3;
    ((base * multiplier) as i64).max(4).min(15) as usize
}
```

**Topology** (`state/src/society/geography.rs:2061-2109`):
* Circular land borders between consecutive regions (`distance = 100.0`).
* Capital connects to every other region (`distance = 150.0`).
* Maritime nodes are added with coastline/sea-lane edges (`50.0 / 500.0`).

**Regional coordinates** are initialized to `(0.0, 0.0)` in `generate_regional_topology` and later spread via a deterministic spring layout (`populate_region_coordinates`, `state/src/society/geography.rs:3155-3161`).

### 4.3 Resource Discovery

**Formation-level discovery** (`state/src/society/geography.rs:2335-2395`):

```rust
let discovered = depth < 200.0 && rng.gen::<f64>() < 0.7;
```

**Planet-level auto-discovery** (`state/src/society/planet.rs:289-303`):

```rust
if vein.rarity_tier == RarityTier::UltraRare ||
   vein.rarity_tier == RarityTier::Rare { continue; } // stay hidden
if overlaps_populated { vein.discovered = true; }
```

So `Uncommon`, `AbundantIndustrial`, and `Ubiquitous` veins overlapping populated regions auto-discover; `Rare` and `UltraRare` remain hidden until a geological survey is performed.

**Runtime deposit gating** (`state/src/economy/production/geology.rs:178-187`):

```rust
pub fn can_access_depth(method_year: u32, deposit_depth: f64) -> bool {
    deposit_depth <= max_depth_for_method_year(method_year)
}
// max depth table
y < 1885 => 200.0, y < 1890 => 400.0, y < 1895 => 600.0, y < 1900 => 800.0,
y < 1950 => 1000.0, y < 1970 => 1200.0, _ => 2000.0
```

### 4.4 Mines, Processing Plants & Fixed Asset Seeding

**Mines** (`state/src/engine/generator/corporate.rs:2427-2488`):
* Collect `Planet` veins for the region.
* Capped at 8 mines per region.
* Building `deposit_id` is bound to `vein.composite_id` or `vein.id`.
* Commodity → method mapping: HardCoal → "Manual Mining", Iron → "Iron Ore Mining", etc.

**Processing plants** (`state/src/engine/generator/corporate.rs:2501-2605`):
* Mined commodities are mapped to `HeavyIndustry` methods (e.g. Iron → "Iron Smelting").
* Filtered by `pm.year <= start_year` and required tech.
* Capped at 3 plants per region.

**Fixed asset seeding** (`state/src/engine/generator/corporate.rs:3377-3427`):

```rust
let machinery_commodity = match sector {
    Sector::HeavyIndustry => Commodity::IndustrialMachinery,
    Sector::Construction => Commodity::ConstructionMachinery,
    Sector::Agriculture => if start_year < 1920 { DraftAnimals } else { AgriculturalMachinery },
    Sector::PublicServices | Sector::PublicAdministration | Sector::Banking => OfficeMachinery,
    Sector::TransportLogistics | Sector::ExportServices => {
        if start_year < 1900 { DraftAnimals }
        else if start_year < 1930 { Trains }
        else { Trucks }
    }
    _ => IndustrialMachinery,
};
```

### 4.5 Arable Land & Climate Multipliers

Climate (`state/src/society/geography.rs:197-218`): `Fertile`, `Desert`, `Mountainous`, `Balanced`.

Arable multiplier at generation:

```rust
Climate::Fertile     => rng.gen_range(1.2..1.8)
Climate::Desert      => rng.gen_range(0.3..0.6)
Climate::Mountainous => rng.gen_range(0.5..0.9)
Climate::Balanced    => rng.gen_range(0.8..1.2)
```

Regional arable maximum:

```rust
let arable_max = (region_pop as f64 * rng.gen_range(0.15..0.45) * arable_mult) as i64;
```

---

## 5. Politics & Governance

### 5.1 VIP Generation, Gender & Portrait Seeds

**Name generation** (`state/src/politics/names.rs:270-419`):

```rust
pub fn generate_person_name(cultural_group: &str, gender: &str, rng: &mut impl Rng) -> VipName
```

* Cultural pools: `slavic`, `germanic`, `latin`, `middle_eastern`, `balkan`.
* `generate_full_vip` gives 70% male, 30% female politicians, age 35-75.
* `generate_unique_vip` retries up to 20 times against a `used_names` `HashSet`.
* `generate_key_vip` retries up to 50 times for key political figures.
* `generate_key_vip_with_gender` is used for royal consorts and heirs where gender is predetermined.

**`Vip` and portrait** (`state/src/politics/vip_registry.rs:311-545`):

```rust
pub struct Vip {
    pub id: String,
    pub full_name: String,
    pub gender: String,
    pub age: u32,
    pub health: f64,
    pub traits: Vec<String>,
    pub roles: Vec<String>,
    pub dynasty: String,
    pub portrait_seed: String,
    pub base_influence: f64,
    ...
}
```

* VIP ID: `format!("VIP-{:06}", next_id)`.
* `portrait_seed`: `format!("{}-{}-{}", nationality, gender, full_name)`.

### 5.2 Gender Constraints

* Politicians: 70% male / 30% female.
* Royal consort: opposite gender of the monarch.
* Royal children: 50% male / 50% female.
* Childbearing age: female 18-45, male 18-60 (ages in years; turns are months).

### 5.3 Dynamic Extended Royal Dynasties

`RoyalDynasty` (`state/src/politics/succession.rs:84-113`) and `RoyalFamilyMember` (`state/src/politics/succession.rs:17-59`) track members, relations, succession order, parents, spouse, children, birth/death turns.

`process_dynasty_turn` (`state/src/politics/succession.rs:229-516`):
1. Monarch marriage if unmarried and ≥ 18.
2. Birth roll: deterministic 20% chance from hash of `"birth_{}_{}_{}"`.
3. Succession recalculation: **primogeniture** — legitimate children sorted by age descending; eldest is `is_heir_apparent`.
4. Record deaths.
5. Regency selection if monarch dies: prefers Consort > Sibling > Cousin > Child.

At world generation (`state/src/politics/turn.rs:1395-1450`), a monarchy initializes `politics.royal_dynasty` with the monarch, a consort of opposite gender, and 1-2 royal heirs.

### 5.4 Fiscal Tax Pipeline — Accrual Accounting

`process_tax_collection_turn` (`state/src/state/tax.rs:1281-1538`) is a **read-only** calculator returning `TaxCollectionResult`.

| Tax | Source / Formula | Notes |
|-----|------------------|-------|
| **PIT** | `sum(building.current_employment * average_wage)` × progressive or flat rate | Already withheld at source; reported only. |
| **CIT** | `sum(last_profit.max(0.0))` per owner × `corporate_tax_rate` | SEZ adjustments, `(1 - evasion)` applied. |
| **VAT** | `country.accumulated_vat` | Physically credited during B2C clearing. |
| **Wealth tax** | Progressive brackets on `liquid_capital + fixed_capital`. | |
| **Capital gains** | `calculate_capital_gains_tax` with brackets and holding-period modifiers. | |
| **Customs** | `customs_state.tariff_revenue_collected` | |
| **State property** | `state_forest_state.treasury_remittance` | |
| **Evasion** | `enforcement = min(1.0, tax_office_workers / (total_companies * 0.1))` | Evaded amount remains in entity cash. |

**Tax routing** (`state/src/state/tax.rs:931-1035`):

```rust
let micro = tax_amount * routing_config.microregion_share;
let region = tax_amount * routing_config.region_share;
let central = tax_amount - micro - region; // remainder → central

country.budget.liquid_reserves += central_share;
region.treasury.liquid_reserves += region_share;
```

The caller in `state/src/engine/turn.rs:3556-3613` physically debits companies and routes the collected amounts to the treasury. PIT/CIT/wealth are routed 100% to the central treasury in the current wiring; VAT and customs were already credited earlier.

### 5.5 Ministries, Budget & Parliament Payroll

**Ministries** (`state/src/politics/ministries.rs:284-683`):

```rust
let num_ministries = min(15, max(3, (gdp / 1e9) as usize + 3));
```

* PM keeps Treasury + Defense (or Treasury + InternalSecurity in autocracy).
* Coalition partners receive portfolios by `competency_idx % coalition.len()`.
* `calculate_budget_needs`: `base_budget = gdp * 0.15`; share by ideology weights; floor `10_000.0`.
* `allocate_cash_to_ministries`: hard-capped by `liquid_reserves`; `ministry_cash` pocket is credited; all spending debits the pocket, not the treasury.

**Parliament payroll** (`state/src/engine/turn.rs:5886-6000`):

```rust
let average_wage = gdp / population * parliament_mp_salary_wage_ratio; // default 0.1
let mp_salary = average_wage * parliament_mp_salary_multiplier;       // default 3.0
let staff_salary = average_wage * parliament_staff_salary_ratio;      // default 0.8
let staff_count = total_seats * 2;
```

MPs are not paid if Parliament is suspended; staff are always paid. Shortfalls reduce `political_capital` and raise `factional_tension`. Credits are routed to `Bourgeoisie` (MPs) and `Worker` (staff) savings in the capital region.

### 5.6 Political Year, Elections & Snap Elections

`process_political_year` (`state/src/politics/turn.rs:39-549`) runs once per year boundary and handles VIP aging, interest groups, parties, elections, coalition stability, upper house, SOE dividends, and patent fees.

`process_political_turn` (`state/src/politics/turn.rs:688+`) is the monthly orchestrator.

**Snap election** (`state/src/politics/turn.rs:1020-1048`): 4-turn cooldown; triggers if provisional government or <2 real parties. Sets `years_to_elections = 0`.

**Elections** (`state/src/politics/elections.rs:48-158`):
* Threshold filter → D'Hondt, Sainte-Laguë, or Hare-Niemeyer on 100 seats.
* D'Hondt: `quotient = support / (seats + 1)`.
* `build_coalition`: majority = `total / 2 + 1`; adds partners by increasing ideological distance up to `max_distance = 1.4`.
* `check_coalition_stability`: collapse if `max_dist > 0.8` and `chance > 0.5`.
* `ideological_distance`: Euclidean on 3D compass `(economy, liberty, tradition)`.

**Ideology** (`state/src/politics/ideology.rs`): 15 English canonical ideologies with `IdeologyCompass` coordinates. Budget priorities are in `state/src/politics/ministries.rs:79-214`.

---

## 6. Audit Findings

The Phase 93 deep-sweep audit focused on the four requested categories. All findings are static; no `cargo check` or test run was performed.

### 6.1 Rule 12 Violations (English-Only Domain Language)

| Type | Example | File / Lines |
|------|---------|--------------|
| Variables | `analfabetyzm`, `srednie`, `wyzsze`, `podstawowe`, `wydobycie`, `roln`, `wegiel_mix`, `gaz_mix`, `oze_mix`, `rezerwy` | `state/src/engine/generator/mod.rs:1041-1616` |
| Variables | `sila_robocza`, `bezrobotni_aktualni`, `stopa_bezrobocia_aktualna`, `fundusz_sredni`, `wsk_min` | `state/src/economy/labor/labor.rs:53-377` |
| Variables | `straz`, `straz_gran`, `urzad_cel` | `state/src/registries/production_methods.rs:690-850` |
| JSON / map keys | `brak`, `podstawowe`, `srednie`, `wyzsze` | `state/src/politics/turn.rs:222-237` |
| JSON / map keys | `zanieczyszczenie`, `magazyny`, `zboze` | `state/src/society/housing.rs:1000-1001`, `state/src/state/treasury.rs:391-403` |
| Localized strings | `Powstanie w {}` | `state/src/politics/rebellions.rs:182` |
| Doc comments / enum keys | `obowiazkowa_sluzba`, `powszechny_pobor`, etc. | `state/src/registries/enums.rs:34-72` |
| Class key | `"Robotnicy"` for strike pay routing | `state/src/corporate/unions.rs` |

**Note:** `state/src/economy/labor/labor.rs` contains the largest concentration of Polish identifiers (`sila_robocza`, `bezrobotni`, `frykcyjne_bazowe`, `dzieci`, `dorosli`, `starsi`, etc.), which is a direct Rule 12 breach.

### 6.2 Dead & Uninitialized Code

| Item | File / Lines | Finding |
|------|--------------|---------|
| `fn duty_rates` | `state/src/international/trade.rs:437+` | `#[allow(dead_code)]`, never referenced. |
| `fn has_geological_resource_vein` | `state/src/engine/generator/corporate.rs:2945+` | `#[allow(dead_code)]`, never referenced. |
| `fn calculate_spot_price` | `state/src/energy/grid.rs:1012+` | `#[allow(dead_code)]`, deprecated. |
| `type HashMap = FxHashMap` | `state/src/corporate/lifecycle.rs:13` | `#[allow(dead_code)]`. |
| `struct CulturalGroup` | `state/src/society/cultures.rs:31` | `#[allow(dead_code)]`. |
| `enum HvConnectivityMode` | `state/src/energy/grid.rs:254` | `#[allow(dead_code)]`. |
| `process_succession` | `state/src/politics/succession.rs` | Implemented but not called from `state/src/politics/turn.rs:710-713` (explicit TODO). |
| `let _ = ...` discards | `state/src/engine/turn.rs` | ~30 sites discard subsystem results: construction tranches, B2B orders, material fraud, deferred trades, maintenance bids, military messages, utility distribution, retail rents, etc. |
| `labor_config.rs` | `state/src/economy/labor/labor_config.rs` | Full `LaborConfig` is stored on `Country` but `process_demographics_and_labor` uses hardcoded constants; `resolve_regional_labor_market` is passed `LaborConfig::default()`. |

### 6.3 Conflicting Logic & Half-Measures

| Item | File / Lines | Description |
|------|--------------|-------------|
| Production profit vs. forecast | `state/src/economy/trade/b2b_orders.rs:998-1219`, `state/src/economy/production/production.rs:162-374` | `process_building_cycle` sets `last_profit` from pre-clearing forecast; `execute_production_cycle` physically produces goods but does not recompute `last_profit`. Corporate accounting therefore uses the forecast, not actual post-trade production. |
| Building auction valuation | `state/src/corporate/lifecycle.rs:146-152` | Every building of a liquidated company is priced at the **company-level** `fixed_capital`, not a per-building value. |
| Arbitrary clamps | `state/src/economy/indicators.rs`, `state/src/engine/generator/mod.rs`, `state/src/engine/generator/corporate.rs`, `state/src/economy/labor/labor.rs` | Pervasive `.min(...)`, `.max(...)`, `.clamp(...)` with hard thresholds (e.g. `10_000.0` arrears scaling, `0.03/0.05` sticky wage caps, `50.0` floors). |
| Magic constants | `state/src/engine/generator/mod.rs:850-940`, `:1624-1625`; `state/src/economy/state_sector/osp.rs:15-19` | Army-size heuristics, `+/-150 bps` CB spread, flat `VOLUNTEER_RATE = 0.001`, `MAX_VOLUNTEER_FTE = 20`. |
| Lombard reserve clamp | `state/src/state/banking.rs` | Step 5 clamps `reserves_at_central_bank` to `0.0` after interest, which can silently break the accounting identity. |
| KNF fine shortfall | `state/src/securities/knf.rs:337-345` | When a fine exceeds reserves, it creates an `interbank_loans_taken[central_bank.id]` liability without symmetric CB reserve creation. |
| TODOs / band-aids | Multiple | `process_succession` not wired; dividends to individual shareholders not routed; R1 consumer demand build removed rather than fixed; guild FTE hardcoded to `0.0`; energy generation-mix proxy in UI; full access hardcoded in Tauri query commands. |

### 6.4 Missing Lifecycles

| Entity | File / Lines | Missing Mechanism |
|--------|--------------|-------------------|
| Rebel proto-states | `state/src/politics/rebellions.rs:176` | `spawn_rebel_proto_state` creates a full rebel `Country`; no despawn, annexation, or reintegration logic exists. |
| Mass movements | `state/src/politics/mass_movements.rs:153` | `check_mass_movement_spawn` creates `MassMovement` objects; no decay, dissolve, or remove path. |
| OSP volunteers | `state/src/economy/state_sector/osp.rs:57` | Volunteer FTE is allocated, but OSP companies have no bankruptcy/closure path. |
| State research institutes | `state/src/economy/state_sector/state_research.rs:25` | `execute_state_research` runs but the research institution has no decay/despawn. |
| Infrastructure funding | `state/src/economy/state_sector/infrastructure.rs:28-137` | Funding is guarded, but the infrastructure entity itself has no lifecycle/closure/decommission path. |

### 6.5 Most Critical Findings (Executive Summary)

1. **Rule 12 is breached in core modules.** The labor-market and generator modules still contain Polish variable and JSON keys. This must be cleaned before the next major phase.
2. **No real LDR metric.** The "8193% LDR anomaly" is guarded by `issue_loan` reserve checks, but the simulation does not compute or report loan-to-deposit.
3. **Forecast profit vs. actual production mismatch.** `process_company` uses pre-clearing `last_profit` while `execute_production_cycle` does not update it; the corporate P&L is based on an estimate, not realized output.
4. **Missing lifecycles for political and state-sector entities.** Rebels, mass movements, OSP, state research, and infrastructure lack death/closure logic.
5. **Lombard reserve clamp and KNF fine shortfall** are silent balance-sheet band-aids that can create unbacked liabilities or accounting mismatches.
6. **~30 `let _ = ...` discards** in `engine/turn.rs` indicate features that are wired but whose outputs are ignored, suggesting incomplete integration or dead phantoms.

---

*End of Phase 93 Grand Architectural Codex & Deep Sweep Audit.*
