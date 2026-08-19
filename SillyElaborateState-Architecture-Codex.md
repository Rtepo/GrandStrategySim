# SillyElaborateState — Architecture Codex

**Status:** Phase 45 complete (705 tests passing). Compiled for handoff to a fresh AI session.
**Date:** 2026-08-17
**Crate root:** `state/`
**Repository root:** `C:\Users\netse\Downloads\SillyElaborateState`

> This document is a frozen architectural reference combined with a deep static audit.
> It is intended to give a new session complete context without re-reading 82 files of history.
> **No Rust code was modified to produce this document.** All findings are read-only.

---

## Table of Contents

1. [Engine Architecture & The Turn Pipeline](#section-1-engine-architecture--the-turn-pipeline)
2. [Strict Double-Entry Accounting](#section-2-strict-double-entry-accounting)
3. [World Genesis & Demographics](#section-3-world-genesis--demographics)
4. [The Macroeconomy](#section-4-the-macroeconomy)
5. [Statecraft & Institutions](#section-5-statecraft--institutions)
6. [The Deep Static Audit (Defects & Bottlenecks)](#section-6-the-deep-static-audit-defects--bottlenecks)
7. [Glossary & Key File Index](#section-7-glossary--key-file-index)

---

## SECTION 1: Engine Architecture & The Turn Pipeline

### 1.1 Top-Level Entry Point

The engine entry point lives in `state/src/engine/turn.rs`. A single invocation of the
top-level turn function advances the entire world by one turn. **One year = 24 turns**
(see the turn/year counter increment at the very end of the pipeline).

The pipeline is organized as a sequence of **parallel blocks** (Rayon `par_iter_mut`
over countries) interleaved with **sequential bottlenecks** (sequential `for` loops)
that require global coordination across countries.

### 1.2 The 177-Phase Turn Loop (Chronological)

The turn loop executes approximately **177 distinct phases/waves**. The list below
preserves the exact chronological order. Phases marked **[PAR]** run inside a
`par_iter_mut` block over countries; phases marked **[SEQ]** run sequentially.

#### Initialization (Sequential)

| # | Phase | Purpose |
|---|-------|---------|
| 1 | Load Phase 25 | Load regions from disk into `state.countries` |
| 2 | Load Phase 35 | Load megaregions from disk |
| 3 | Load Phase 25 | Load market history (VWAP, retail VWAP, base prices) |
| 4 | Load Phase 34 | Capture `prev_net_surplus` at START of turn |
| 5 | Load Phase 26 | Sync calendar from loaded turn/year |
| 6 | Load Phase 11 | Build ephemeral `company_id → country_name` lookup for embargo/tariff enforcement |
| 7 | Load Phase 43 | Build `country_name → currency_code` lookup for FX reserves |
| 8 | Load Phase 6.5 | Use unified calendar from `GameState` |

#### Parallel Block 1 — Pre-Market Setup [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 9 | Phase 14 | Prison labor preprocessing (reduce private camp demand, zero isolation camp labor) |
| 10 | Demographics & Labor | Process demographics and labor market data |
| 11 | Phase 13 | Religion initialization (per-class religion from dominant religion) |
| 12 | Banking Turn | Per-country banking operations |
| 13 | GDP Share Update | Update GDP shares from employment data |
| 14 | Resurrection Phase 10 | Emergency conditions check (uses global market surplus snapshot) |
| 15 | Phase 4 | Disruption reset (transient disruption modifiers) |
| 16 | Union Processing | Union activity and wage negotiations |
| 17 | Building Cycle with Geology | Building production cycles with geology constraints |
| 18 | Market Price Resolution | Resolve market prices from orders |
| 19 | Market Signal Building | Build market signal from prices, orders, PMI, capital market |

#### Sequential B2B Order Collection [SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 20 | Phase 6.4 | Initialize global order book |
| 21 | Resurrection Phase 3 | Drain pending defense orders from last turn into global order book |
| 22 | Phase 23A | Manage deferred trades (increment counters, expire old trades) |
| 23 | Phase 6.3 | Production planning — submit company B2B orders |
| 24 | Phase 19B | Submit maintenance service bids |
| 25 | Phase 19C | Submit fixed-asset purchase bids (cash-bottlenecked machinery/vehicles) |
| 26 | Phase 11 | Embargo-aware matching |
| 27 | Phase 24A.1 | Redistribute unfilled bids back to per-country order books |

#### Parallel Block 2 — Infrastructure & Construction [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 28 | Resurrection Phase 1 | Infrastructure pre-clearing: cultural donations, church/NGO transfers, Apostolic See remittance, cash relief, relief B2B buy orders, shipyard construction B2B, mobilization advance, construction B2B |
| 29 | Phase 22A | Construction tender market: developer tenders, state-funded tenders, contractor bids, awards |
| 30 | 3.9 | Submit agricultural harvest asks |
| 31 | 3.7b | Add fleet commodity demand |

#### Sequential Trade Settlement [SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 32 | Phase 23A-3 | Freight procurement gate (cross-region trades) |
| 33 | Phase 6.4a | Cash settlement (double-entry via `TransferSettler`) |
| 34 | Phase 13 | Storage transaction settlement |
| 35 | Black Hole 1.19 | Defense trade settlement (credit sellers via `TransferSettler`) |
| 36 | Phase 19B | Settle maintenance service trades |
| 37 | Phase 24D | Accumulate fixed-asset purchases as GDP investment (I) |

#### Parallel Block 3 — State Operations & Military [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 38 | Phase 31 | Crisis management AI (executive decrees: tax, bonds, subsidies) |
| 39 | Phase 32 | Parliament building payroll (MP and staff wages from Treasury) |
| 40 | Phase 7 | Ministry procurement (domestic order book, no tariffs) |
| 41 | Resurrection Phase 1 | Infrastructure post-clearing: refund unfilled bids, advance shipyard projects, deliver relief, heritage effects, port utilization, shipyard maintenance, fleet upkeep |
| 42 | Resurrection Phase 3 | Military turn (MIL-1 to MIL-6): upkeep, supply delivery, combat, casualties, peasant devastation, war exhaustion decay |
| 43 | Phase 6.4b-PRE | Construction progress (consume delivered materials) |
| 44 | Phase 22A | Tranche payments to contractors |
| 45 | Phase 22B | Construction fraud & OHS (material substitution, corner-cutting) |
| 46 | Phase 23B | Transport network degradation & maintenance |
| 47 | Phase 15A | Weather, condition degradation, OSP volunteer allocation (degrade fixed-asset cohorts) |

#### Phase 8 — Wave-Based Production [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 48 | Wave 1 | Energy production (Commodity::Energy/Heat from fuel) |
| 49 | Phase 8.1 | Grid distribution (convert Energy/Heat to ElectricitySupply capacity) |
| 50 | Phase 8.2 | Utility consumption (deficits, penalties, billing) — penalty collection is sequential |
| 51 | Phase 44 | Residential rent collection (double-entry rent transfer) |
| 52 | Wave 3 | General production (all non-energy sectors with blackout penalties) |
| 53 | Phase 8.3 | Waste collection & processing |
| 54 | Phase 25 | Restock retail stores (transfer output to retail shelves) |
| 55 | Phase 6.5 | Agricultural sub-sequence (state transitions, FTE demand) |
| 56 | Phase 25 | Set wage offers (after agricultural FTE demand computed) |

#### Phase 23C — Commuting & Passenger Transport [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 57 | Phase 23C | Build commute map and clear PassengerTransport B2C for commuters |

#### W1 — Wage Payment (Labor Market Resolution) [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 58 | Phase 28 | State Employer injection (civil service pseudo-company) |
| 59 | Phase 18B | Compute garnishment rates from justice state |
| 60 | Phase 25 | Clear labor market for ALL regions with commuter inflow |
| 61 | Phase 25 | Feed back actual FTE/wages to macro indicators |
| 62 | Phase 28 | State Employer removal (accumulate wages as GDP G) |
| 63 | Phase 37/38 | Save prev FTE/wage for next turn's hiring frictions |
| 64 | Phase 41 | Reset strike flag |
| 65 | Phase 25 | Sync building employment from `company.fulfilled_fte` |
| 66 | Phase 23C | Remit commuter wages back to home regions |
| 67 | Fix 1.22 | Credit withheld PIT and garnishments to Treasury |
| 68 | Phase 18A | Route TemporaryWorker remittances to ForeignEntity |
| 69 | Phase 18A | Shadow economy processing (tax evasion) |

#### D.5/D.6 — Payment in Kind & Harvest [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 70 | D.5 | Payment in kind (deduct harvest for subsistence, imputed GDP) |
| 71 | D.6 | Deposit harvest to warehouses (yield and rot) |
| 72 | Phase 10 | Storage fees (accumulate fees, apply perishability) |

#### Phase 6.5 — B2C Market Phases R1–R7 [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 73 | R4 | Reset procurement commitments for wholesalers |
| 74 | R5 | Apply clearance discounts for stale inventory |
| 75 | R6 | Clear B2C markets for ALL regions with rationing |
| 76 | R7 | Accrue retail rents and update leases for shopping centers |

#### Sequential B2C Aggregation [SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 77 | Phase 25 | Update retail VWAP from B2C clearing |
| 78 | Phase 44 | Aggregate B2C demand into `market.demand_volume` |

#### Parallel Block 4 — Company Lifecycle & Securities [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 79 | Resurrection Phase 10 | Rationing consequences (mortality and unrest penalties) |
| 80 | Resurrection Phase 9 | Tourism industry (parallel credit, sequential debit) |
| 81 | Phase 24C.7 | Update company information quality tier |
| 82 | Company Processing | Process companies with market signal |
| 83 | Company Lifecycle | Birth, death, restructuring |

#### Resurrection Phase 2 — Securities Market Sequence [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 84 | SEC-1 | Collect fund capital (NAV-based unit issuance) |
| 85 | SEC-2 | Submit fund orders (deterministic valuation score) |
| 86 | SEC-3 | Securitize loans into MBS (banks submit Ask orders) |
| 87 | SEC-4 | Create covered bonds from eligible mortgage pools |
| 88 | SEC-5 | Match securities orders on exchange |
| 89 | SEC-6 | Process MBS coupon payments (debit bank, credit owners) |
| 90 | SEC-6b | Process covered bond coupon payments |
| 91 | SEC-7 | Process derivatives (CDS premiums + futures mark-to-market) |
| 92 | SEC-7b | CCP margin management |
| 93 | SEC-8 | Charge fund fees (management and performance) |
| 94 | SEC-8b | KNF compliance audits |
| 95 | SEC-8c | Process trade finance (bills of lading delivery) |

#### Phase 7 — Tax Collection [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 96 | Budget Migration | Ensure `ministry_config` exists |
| 97 | Phase 40 | Calculate budget needs (GDP and ideology based) |
| 98 | Legislative Budget Cycle | Draft and process budget bill (once per year) |
| 99 | Tax Collection | Collect progressive taxes (PIT, CIT, VAT, wealth, capital gains) |
| 100 | Phase 42 | Debit companies for CIT and wealth tax |
| 101 | Route Tax | Route collected amounts to Treasury |
| 102 | Phase 38 | Store tax result for Finance tab |
| 103 | Regional Taxes | Process regional taxes |
| 104 | Fiscal Transfers | Process fiscal transfers |
| 105 | Commissary Admin | Check commissary administration |
| 106 | Phase 15B | Customs evasion recovery |
| 107 | Phase 29 | Corruption tax leakage |

#### Phase 8 — State Spending & Allocation [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 108 | Debt Service | National and local debt service |
| 109 | Phase 10 | State reserve maintenance |
| 110 | Phase 10 | Black ops funding (strict double-entry) |
| 111 | Arrears Check | Prioritize arrears repayment if in default |
| 112 | Wholesale Debt | Issue treasury securities before ministry allocation |
| 113 | Phase 38 | DSPW auction settlement (primary dealer banks) |
| 114 | Phase 29 | Anti-corruption budget reallocation |
| 115 | Ministry Cash | Hard-capped by physical reserves |
| 116 | Ministry Phase A | Strategies + B2B orders |
| 117 | Phase 29 | State construction (inspectorate tenders) |
| 118 | Ministry Phase B | Post-clearing reconciliation |

#### Phase 13 — Social Programs + Charity [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 119 | 13a | Charity fundraising (donations from wealthy/co-religionists) |
| 120 | 13b | Social welfare distribution (active SocialPrograms) |
| 121 | 13c | Charity distribution (relief to poorest classes) |

#### Debt Markets [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 122 | Retail Savings Bonds | Clear savings bonds B2C window |
| 123 | Secondary Debt Market | Clear secondary debt market |

#### Political Processing [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 124 | Phase 39 | Apply ruling ideology tax policy (every turn) |
| 125 | Phase 39 | Check snap election (every turn) |
| 126 | Phase 39 | Run election if due (every turn) |
| 127 | Phase 35 | Political year (once per year boundary) |

#### Military & Strategic Reserve [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 128 | Resurrection Phase 3 | Submit defense B2B orders for next turn (stored in `pending_defense_orders`) |
| 129 | Phase 10 | Strategic Reserve Agency buy/sell orders for price stabilization |
| 130 | Add Military Demand | Add military commodity demand to market |

#### Real Economy Phase 9 [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 131 | 9.1 | Corporate R&D (patent expiration, budget, method research, licensing) |
| 132 | 9.2 | Fishing turn (deterministic fishing harvest) |
| 133 | 9.3 | Infrastructure funding (owner allocation) |

#### Justice, Maintenance, Oversight [PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 134 | Phase 9.1 | B2C service clearing (education, health, information) |
| 135 | Phase 14 | Justice system coverage (crime demand, penalties, corruption OPEX) |
| 136 | Phase 18B | Vigilante justice + ombudsman |
| 137 | Phase 15A | Maintenance spending + disaster checks |
| 138 | Phase 22D | Private oversight + civil lawsuits + KIO appeals |
| 139 | Phase 15B | Smuggling + customs evasion recovery |
| 140 | Phase 9.2 | Innovation trading + royalty payments |

#### Sequential Post-Parallel Crediting [SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 141 | Phase 19A | Cross-border blueprint royalties (credit foreign licensors in home country) |
| 142 | Phase 28/36 | Store sector employment data for ToT computation |
| 143 | Phase 42 | Safety clamp (FX reserves never negative) + political capital regen |
| 144 | Persistence | Save companies, buildings, commercial buildings, housing, unions |

#### Migration & Society [SEQ → PAR]

| # | Phase | Purpose |
|---|-------|---------|
| 145 | Phase 15B Pass 1 | Collect migration flows (sequential) |
| 146 | Phase 15B Pass 2 | Apply migration flows (sequential) |
| 147 | Phase 15B Pass 3 | Deportations (parallel) |
| 148 | Phase 15C | Inspectorates + state forests (parallel) |
| 149 | Phase 22C | Construction inspections + bribery (parallel) |
| 150 | Phase 17C | Monastery production + church fund (parallel) |
| 151 | Phase 18A | Amnesty & legalization (parallel) |
| 152 | Phase 17B | Religious conversion + institutional assimilation (parallel) |
| 153 | Phase 17C | Pogroms (parallel) |
| 154 | Phase 18C | Terrorism (parallel) |
| 155 | Phase 17C | Apostolic See reinvestment (sequential: global charity + FDI) |

#### Sequential Global Aggregation [SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 156 | Merge Orders | Merge per-country orders into `global_orders` |
| 157 | Update Global Base Prices | Exponential smoothing from local cleared prices |
| 158 | Update Net Surplus | Recompute global net surplus |

#### Final Telemetry & Persistence [PAR → SEQ]

| # | Phase | Purpose |
|---|-------|---------|
| 159 | Phase 29 | PMI diffusion index (parallel) |
| 160 | Phase 24D | Macroeconomic telemetry: GDP, CPI/PPI, M0/M3 (parallel) |
| 161 | Phase 29 | Dynamic tariff adjustment (sequential) |
| 162 | Balance Global Trade | Balance global trade with diplomacy (sequential) |
| 163 | Phase 10 | Settle trade deficits via Forex/Gold reserves (sequential) |
| 164 | Phase 24D | Update net exports (sequential) |
| 165 | Phase 35 | Reconcile regional GDP (distribute national GDP by population) |
| 166 | Phase 24F | Record telemetry history (ToT/YoY samples) |
| 167 | Phase 24F | Aggregate OHS casualties |
| 168 | Phase 39 | Drain deferred diplomatic action queue |
| 169 | Phase 24F | CSV export (append telemetry rows) |
| 170 | Phase 24C.3 | Auto re-entry (decrement sovereign default lockout) |
| 171 | Phase 11 | Update diplomatic relations |
| 172 | Phase 11 | Persist diplomacy matrix |
| 173 | Phase 25 | Persist market history (CPI/PPI) |
| 174 | Phase 44 | Update market volumes (Market UI) |
| 175 | Save Market | Save `market.json` and `market_volumes.json` |
| 176 | Update Counters | Increment turn and year (1 year = 24 turns) |
| 177 | Update Storage | Update `storage.json` |

### 1.3 Rayon Parallelism & Sequential Bottlenecks

**Rayon `par_iter_mut()`** is used for all per-country operations that do not require
cross-country coordination. Approximately **140+ phases** run in parallel.

**Sequential bottlenecks** (17 total) exist where global coordination is required:

1. Defense order draining (must merge into global order book before matching)
2. Deferred trade management (increment counters)
3. B2B order submission (submit company orders)
4. Unfilled bid redistribution (after matching)
5. Freight procurement gate (cross-region trade settlement)
6. Trade settlement (sequential cash settlement via `TransferSettler`)
7. B2C demand aggregation (into global market)
8. Cross-border royalty crediting (credit foreign licensors in home country)
9. Sector employment data storage (for ToT computation)
10. FX reserve safety clamp (no negative reserves)
11. Persistence (save entities to disk)
12. Migration flows (two-pass: collect then apply)
13. Apostolic See reinvestment (global charity distribution)
14. Global order merging (merge per-country orders)
15. Price sample collection (for global price averaging)
16. Dynamic tariff adjustment (before global trade)
17. Diplomatic relations update (after all processing)

**Key architectural note:** The sequential B2B order submission (#3 above) is a known
hot path. All companies across all countries submit orders in a single sequential loop
to preserve deterministic order-book state. This is the single largest sequential
bottleneck in the turn loop.

---

## SECTION 2: Strict Double-Entry Accounting

### 2.1 Core Principle

**NO money is ever created or destroyed except via explicit Central Bank OMO,
Central Bank Lombard lending, QE, or FX Forced Conversion.** Every other transfer
is a strict double-entry movement between existing balances.

### 2.2 The Four Cash Reservoirs

| Reservoir | Location | Purpose |
|-----------|----------|---------|
| `brokerage_account.cash` | `securities/brokerage.rs` | Company operating cash (domestic fiat) |
| `available_cash` | `entities/mod.rs` | Legacy fallback if no brokerage account |
| `liquid_reserves` | `state/treasury.rs` | Treasury's cash balance |
| `reserves_at_central_bank` | `state/banking.rs` | Bank reserves at the Central Bank |

### 2.3 BrokerageAccount Structure

```rust
pub struct BrokerageAccount {
    pub cash: f64,                          // Domestic fiat currency
    pub fx_balances: HashMap<String, f64>,  // Foreign currency balances
    pub portfolio: BTreeMap<String, u64>,   // Share ownership
    pub pending_orders: BTreeMap<String, Order>,
    pub frozen_cash: f64,                   // Reserved for open orders
    pub is_frozen: bool,                    // KNF freeze status
    pub margin_account: Option<MarginAccount>,
}
```

**Invariants:**
- `cash >= 0.0` and `frozen_cash >= 0.0` always hold
- On company creation, `liquid_capital` is transferred to `brokerage_account.cash`
  and the `liquid_capital` field is zeroed to prevent cloning

### 2.4 TransferSettler — The Double-Entry Engine

Location: `state/src/economy/trade/transfer_settler.rs`

**When a company pays money out:**
1. `company.brokerage_account.cash -= amount` (or `available_cash` if no brokerage)
2. `bank.balance_sheet.deposits -= amount` (bank liability decreases — deposit extinguished)
3. `bank.balance_sheet.reserves_at_central_bank -= amount` (bank asset decreases)
4. Recipient receives `amount` (Treasury, citizen savings, or another company)

**When a company receives money:**
1. `company.brokerage_account.cash += amount`
2. `bank.balance_sheet.deposits += amount` (bank liability increases — new deposit)
3. `bank.balance_sheet.reserves_at_central_bank += amount` (bank asset increases)

**Intra-bank transfers** skip bank balance sheet updates (internal ledger move only).
This is intentional but should be documented at the call site.

### 2.5 TransferRecipient Variants

```rust
pub enum TransferRecipient {
    Treasury,                              // → liquid_reserves
    CitizenSavings { region_idx, is_rural, class_key },  // → class savings
    OtherCompany { recipient_idx },        // → brokerage_account.cash + bank sync
    ForeignEntity,                         // Money leaves system (FX outflow)
    CentralBank,                           // Money extinguished (deposit destroyed)
}
```

### 2.6 Treasury Operations

- **Tax collection:** flows from companies/citizens → `liquid_reserves`
- **Government spending:** ministries spend from `liquid_reserves` via B2B procurement
- **Ministry cash pocket (Phase 35 fix):** `allocate_cash_to_ministries` credits
  `ministry_cash` (the "pocket"). All spending debits from `ministry_cash`, NOT
  directly from `liquid_reserves`. This eliminates the double-debit bug.
- **Hard cap:** Treasury cannot spend more than `liquid_reserves`

### 2.7 Bank Balance Sheet

```rust
pub struct BankBalanceSheet {
    // ASSETS
    pub reserves_at_central_bank: f64,
    pub loans_issued: Vec<Loan>,
    pub interbank_loans_given: HashMap<String, f64>,
    pub securities: f64,
    pub mbs_holdings: Vec<MortgageBackedSecurity>,
    pub real_estate: f64,
    // LIABILITIES
    pub deposits: f64,
    pub cb_lombard_loans: f64,
    pub cb_deposit_facility_balance: f64,
    pub interbank_loans_taken: HashMap<String, f64>,
    pub issued_bonds: f64,
    // EQUITY
    pub tier_1_capital: f64,
}
```

**Balance invariant:** `Assets = Liabilities + Equity` (within 1e-6 tolerance)

### 2.8 The Five Explicit Money-Creation Channels

| Channel | Mechanism | Double-Entry | Creator |
|---------|-----------|--------------|---------|
| **Bank Loans** | `loans_issued.push()` + `deposits += principal` | Asset (loan) ↑, Liability (deposit) ↑ | Commercial Banks |
| **OMO Purchase** | CB buys bonds, credits bank reserves | CB: bonds ↑, reserves ↑; Bank: securities ↓, reserves ↑ | Central Bank |
| **OMO Sale** | CB sells bonds, debits bank reserves | CB: bonds ↓, reserves ↓; Bank: securities ↑, reserves ↓ | Central Bank |
| **QE** | CB buys bonds during deflation (capped 5% GDP/turn) | Same as OMO purchase | Central Bank |
| **Lombard Loans** | CB lends reserves to banks | CB: reserves ↓, loans ↑; Bank: reserves ↑, lombard_loans ↑ | Central Bank |
| **FX Conversion** | Currency swap via AMM | Debit input currency, credit output currency | No net creation |

**Critical:** Reserves do NOT change during loan creation (fractional reserve principle).
Reserves only change during clearing when the borrower wires money to another bank.

### 2.9 B2C Purchase Flow (VAT Split)

1. Citizens pay company: debit citizen savings, credit company
2. **VAT split:** company gets base amount, Treasury gets VAT portion
3. Bank sync: deposits and reserves increase by base amount only
4. VAT routed to Treasury via `settle_b2c_purchase`

### 2.10 Known Accounting Risks

1. **Reserve clamping at zero** (`transfer_settler.rs:94-98`): If reserves go negative
   due to floating-point error, they're clamped to zero without compensating the
   liability side. Could create balance sheet imbalance.
   **Recommendation:** Reject the transfer if it would cause negative reserves.

2. **ForeignEntity transfer** (`transfer_settler.rs:218-220`): Money is debited but
   not credited. Correct for FX outflows, but no corresponding `fx_reserves` adjustment
   on the Central Bank.
   **Recommendation:** Add CB FX reserve tracking for foreign transfers.

3. **Loan creation without inline reserve check** (`state/banking.rs:773-845`): The
   reserve requirement check happens in the caller, not inside `issue_loan()`.
   **Recommendation:** Move reserve check inside `issue_loan()` as a hard invariant.

---

## SECTION 3: World Genesis & Demographics

### 3.1 StartYear Eras

| Era | GDP-per-Capita Multiplier | Theme |
|-----|--------------------------|-------|
| 1900 | 0.8x | Age of Steam and Coal |
| 1925 | 1.2x | Factories and Electricity |
| 1950 | 2.5x | Golden Age of Industry |
| 1975 | 4.5x | Dawn of the Silicon Age |

Location: `state/src/engine/generator/mod.rs:56-63`

### 3.2 Era-Aware Sector Shares

| Era | Agriculture Range | Industry Multiplier | Services Multiplier |
|-----|-------------------|---------------------|---------------------|
| 1900 | 0.20–0.50 | 0.7x | 0.6x |
| 1925 | 0.10–0.30 | 0.9x | 0.8x |
| 1950 | 0.05–0.15 | 1.1x | 1.2x |
| 1975 | 0.02–0.08 | 1.3x | 1.5x |

Location: `state/src/engine/generator/mod.rs:684-692`

### 3.3 Era-Aware Class Demographics

**Rural/Urban Split:**

| Era | Rural | Urban |
|-----|-------|-------|
| 1900 | 80% | 20% |
| 1925 | 65% | 35% |
| 1950 | 50% | 50% |
| 1975 | 40% | 60% |

**Rural Class Distribution:**

| Era | Serf | FreePeasant | LandlessLaborer | Aristocracy |
|-----|------|-------------|-----------------|-------------|
| 1900 | 20% | 40% | 35% | 5% |
| 1925 | 10% | 50% | 35% | 5% |
| 1950 | 0% | 55% | 40% | 5% |
| 1975 | 0% | 60% | 35% | 5% |

**Serfs disappear by 1950** (post-WWII era).

**Urban Class Distribution (all eras):** Workers 70%, Bourgeoisie 30%

Location: `state/src/society/geography.rs:1462-1553`

### 3.4 Labor Participation & Initial Savings

| Class | Participation Rate | Initial Savings (per capita) |
|-------|-------------------|------------------------------|
| Serf | 0.65 | 0.0 |
| FreePeasant | 0.55 | 100.0 |
| LandlessLaborer | 0.60 | 50.0 |
| Aristocracy | 0.30 | 5,000.0 |
| Worker | 0.60 | 200.0 |
| Bourgeoisie | 0.55 | 1,000.0 |

### 3.5 Mega-Estate Housing Generation

Location: `state/src/engine/generator/corporate.rs:2974-3135`

**Building count consolidation:**
- Population > 5,000,000: **20 buildings**
- Population > 1,000,000: **15 buildings**
- Otherwise: **10 buildings**

**Capacity:** 10,000–50,000+ slots per building; total targets ~100% of population
(households ≈ population/4).

**Initial occupancy:** 80–90% (randomized).

**Era-aware housing types:**

| Era | Rural | Urban |
|-----|-------|-------|
| 1900/1925 | Huts (FreePeasant), FolwarkHousing (Serf), Palace (Aristocracy) | Tenements (workers), CityPalace (Bourgeoisie) |
| 1950 | Familok (workers), Palace (Aristocracy) | Tenements + SocialHousing |
| 1975 | SocialHousing, Familok | SocialHousing + Beamciok |

**Ownership assignment:**
- Palace/FolwarkHousing/SocialHousing → `"STATE:<country_id>"`
- Hut → `"CLASS:Aristocracy:<region_id>"`
- Tenement/CityPalace/Familok/Beamciok → `"CLASS:Bourgeoisie:<region_id>"`

### 3.6 Arable Land Constraints

Location: `state/src/society/geography.rs:1240-1244`, `state/src/engine/generator/corporate.rs:470-482`

**Arable land calculation:**
```rust
let arable_max = (population as f64 * rng.gen_range(0.15..0.45) * arable_mult) as i64;
```

**Climate multipliers:**
- Fertile: 1.2–1.8x
- Balanced: 0.8–1.2x
- Mountainous: 0.5–0.9x
- Desert: 0.3–0.6x

**Strict zero-agriculture rule:** If `region.arable_land_max <= 0`, NO agricultural
companies are generated.

**Company count scaling:**
```rust
let arable_scale = (region.arable_land_max as f64 / 10_000.0).max(1.0);
let company_count = (region_emp / 1500.0 * arable_scale).round().max(3.0).min(20.0) as usize;
```

### 3.7 Subsistence Economy & Imputed GDP

**Subsistence peasants:**
- Low GDP countries (gdp_pc < 1.5): 5–40% of population are subsistence peasants
- High GDP countries: 1%

**Serf in-kind payment** (`state/src/economy/finance/payment_in_kind.rs`):
- Serfs receive full in-kind payment (no cash)
- Harvest commodities deducted before B2B market sale
- Subsistence needs from `consumption_registry()` and `NeedTier::Subsistence`
- Deficits tracked in `NutritionalDeficit` with quality penalties

**Imputed GDP:**
- In-kind consumption valued at market prices (VWAP from market history)
- Fallback base price: 100.0 per unit
- Tracked in `RegionalGdpAccumulator.imputed_consumption` and
  `NationalGdpAccumulator.imputed_consumption` via `add_imputed_consumption()`

### 3.8 Processing Plant Generation

Location: `state/src/engine/generator/corporate.rs:1468-1567`

- Spawned after mining entities if region has geological formations with deposits
- Mined commodities map to HeavyIndustry processing methods (Iron→Smelting, Oil→Refining, etc.)
- Era-filtered: method year must be ≤ start_year
- Capped at 3 processing plants per region

### 3.9 Fixed Asset Seeding (Phase 45)

Location: `state/src/engine/generator/corporate.rs:2160-2170`

- Pre-tractor agriculture: `DraftAnimals` (not `AgriculturalMachinery`)
- Pre-truck logistics: `DraftAnimals`; rail era: `Trains`; truck era: `Trucks`
- HeavyIndustry: `IndustrialMachinery`
- Construction: `ConstructionMachinery`

### 3.10 Genesis Gaps Identified

1. **Imputed GDP not tracked during generation** — only calculated at runtime
2. **Housing-demographics ordering dependency** — relies on `generate_class_demographics()` being called before housing generation
3. **Processing plants require geological formations** — regions without formations get no processing even if they have mining
4. **Era-aware fixed assets incomplete** — some sectors may spawn anachronistic machinery
5. **Arable land zero edge case** — very low arable land (1-100 hectares) may still spawn non-viable farms

---

## SECTION 4: The Macroeconomy

### 4.1 Labor Market

Location: `state/src/economy/labor/labor_market.rs`, `state/src/corporate/manager.rs`, `state/src/corporate/unions.rs`

#### Target Wage System (Phase 41)

**Key constants** (`corporate/manager.rs:882-888`):
```rust
const STICKY_WAGE_MAX_DROP: f64 = 0.03;       // 3% max drop per turn
const STICKY_WAGE_MAX_RISE: f64 = 0.05;       // 5% max rise per turn
const TARGET_WAGE_MAX_ADJUSTMENT: f64 = 0.02; // 2% max adjustment per turn
const TARGET_WAGE_FALLBACK: f64 = 50.0;       // Hard floor
```

**Target wage calculation:**
1. If `target_wage == 0.0`, initialize to current `offered_wage_per_fte` or market average (50.0 fallback)
2. If `cash_per_fte > market_average * 2.0`: target 10% above market
3. If `cash_per_fte < market_average * 0.5`: target 10% below market
4. Otherwise: target market average
5. Move `target_wage` toward `desired_wage` by max 2% per turn
6. Sanity cap: 3× market average with 5000.0 floor

**Sticky bounds:**
- Sticky floor = `prev_offered_wage_per_fte * 0.97` (3% max drop)
- Companies with no labor demand: wage set to sticky floor

#### Wage Arrears (Phase 40)

**FTE retention floor:** 90% (10% max layoff per turn)
- Companies can retain up to 90% of `prev_fulfilled_fte` even with zero cash
- Unpaid wages accrue as `wage_arrears` (liability, NOT magical money)
- Applies to ALL companies including banks

**Arrears repayment:** 30% of available cash per turn
**Productivity penalty:** `wage_arrears / 10_000.0` capped at 50%

#### Strike Mechanic (Phase 41)

**Union militancy formula:**
```
target_militancy = (unemployment_factor + wage_pressure - social_relief).clamp(0.0, 100.0)
union.militancy = union.militancy * 0.7 + target_militancy * 0.3
```

**Strike triggers:**
- Union militancy > 0.7
- Company FTE dropped >10% (mass layoff)
- Union strike fund ≥ `(avg_wage * 0.5).max(50.0)`
- Cap: max 10% of corporate sector can strike simultaneously

**Strike effects:**
- `is_striking = true` → 0 production for that turn
- Workers NOT paid by company
- Union pays strike benefits: 50% of avg wage (min 50.0) per FTE
- Benefits credited to class savings (strict double-entry)
- Strike flag reset at end of turn

**Union dues:** 1% of company capital, replenishes strike fund

### 4.2 B2B Trading & Dynamic Pricing

Location: `state/src/economy/trade/b2b_orders.rs`, `state/src/economy/config/b2b_config.rs`, `state/src/economy/production/fixed_assets.rs`

#### B2B Configuration Defaults

```rust
max_cash_encumbrance_ratio: 0.8       // 80% of cash for input purchases
min_markup_ratio: 0.0                 // Fire sale pricing (sell at cost)
max_markup_ratio: 2.0                 // Scarcity premium (3x cost)
fire_sale_threshold: 0.8              // 80% inventory utilization
scarcity_threshold: 0.2               // 20% inventory utilization
max_building_inventory: 10000.0       // Tons per building
warehouse_storage_fee_per_ton: 1.0
buy_premium_ratio: 0.05               // 5% above reference price
freight_cost_reserve_ratio: 0.15      // 15% extra for freight
```

#### Dynamic Markup (Seller Side)

```
if utilization >= fire_sale_threshold (0.8):
    markup = min_markup_ratio (0.0)    // fire sale
else if utilization <= scarcity_threshold (0.2):
    markup = max_markup_ratio (2.0)    // scarcity premium
else:
    t = (utilization - scarcity) / (fire_sale - scarcity)
    markup = max_markup * (1.0 - t) + min_markup * t
```

#### Phase 45: Unfilled Bid Feedback (Buyer Side)

```rust
let last_unfilled = company.unfilled_bid_prices.get(&commodity).copied().unwrap_or(0.0);
let limit_price = if last_unfilled > 0.0 {
    (last_unfilled * 1.10).max(base_price * (1.0 + config.buy_premium_ratio))
} else {
    base_price * (1.0 + config.buy_premium_ratio)
};
```

**Critical Phase 45 rule:** Buyer price increases are capped **only** by available
cash/encumbrance. **No profitability ceiling** is applied. This was an explicit user
requirement.

**Liquidity clamping:** Bids scaled to fit within `max_encumber = liquid * 0.8`

#### Sell Ask Fallback Chain

1. `unit_cost * (1.0 + markup)` — cost-based
2. `reference_price * (1.0 + markup)` — market-based
3. `global_base_prices[commodity] * (1.0 + min_markup)` — floor price

#### Fixed Asset Depreciation & Replacement

**Depreciation:**
```rust
cohort.condition -= (1.0 / cohort.durability) * stress_factor
stress_factor = 1.0 + degradation_stress_weight * (1.0 - building_condition)
```
- Cohorts reaching `condition ≤ 0` are scrapped

**Obsolescence:**
```rust
obsolescence_factor = clamp(1.0 - k * (frontier_year - base_tech_year) / frontier_year, 0.0, 1.0)
```

**Replacement demand:**
```rust
replacement_demand = count * (1.0 - condition)
```
- Quality premium: `ref_price * asset_quality_wtp_multiplier`

**Maintenance services:**
```rust
maintenance_needed = Σ_cohort count * (1.0 - condition) * maintenance_per_condition_point
```

### 4.3 B2C & Retail

Location: `state/src/economy/trade/retail.rs`, `state/src/data/consumption_registry.rs`

#### Consumption Baskets

Per-demographic-class baskets organized by need tier:
- **Subsistence:** Cereal, Vegetable, Protein, HealthCapacity
- **Standard:** adds Clothing, Furniture, Radio
- **Luxury:** adds Luxury goods, Cars, Televisions

#### Phase 45: Era-Aware Consumption Multiplier

| Commodity | Before | Transition | After |
|-----------|--------|------------|-------|
| Radio | 0.0 (<1920) | 0.3 (1920–1930) | 1.0 (>1930) |
| Televisions | 0.0 (<1936) | 0.2 (1936–1950) | 1.0 (>1950) |
| AGD | 0.0 (<1930) | 0.3 (1930–1950) | 1.0 (>1950) |
| Cars | 0.0 (<1910) | 0.1 (1910–1950) | 0.5 (>1950) |
| Luxury | 0.5 (<1880) | — | 1.0 (>1880) |
| Subsistence | 1.0 (always) | — | — |

#### Phase 45: Wealth-Tier Multiplier

```
Subsistence: always 1.0 (people must eat)
Standard: (0.5 + (savings_per_capita / 1000.0).min(1.0)).min(1.5).max(0.1)
Luxury: (savings_per_capita / 500.0).min(2.0).max(0.0)
```

#### Dynamic Retail Pricing

```
scarcity_factor:
  if last_unmet > 0 && last_sold > 0: (last_unmet / (last_sold + last_unmet)).min(0.5) * 0.4
  if last_unmet > 0 (severe): 0.2
  if surplus (total > last_sold * 2): -0.15
  else: 0.0

aging_discount = (age * 0.05).min(0.30)

dynamic_markup = base_markup * (1.0 + scarcity_factor - aging_discount)
price_per_unit = avg_acquisition_cost * dynamic_markup.max(0.5)
```

#### B2C Market Clearing

**Utility calculation:**
```
base_utility = 1.0 / price_per_unit
quality_premium = quality_weight * quality / price_per_unit
inertia_bonus = previous_share * inertia_weight
utility = base_utility + quality_premium + inertia_bonus
```

**Allocation:** Sort by utility (descending), allocate using largest-remainder method.

**Unmet demand tracking (Phase 45):** If `remaining_demand > 0`, record on stores that
sold this commodity. If 0, clear tracking.

#### Transactional VAT (Phase 41)

**VAT categories:**
- Agriculture: 5% rate, 20% consumption share
- Industry: 23% rate, 35% consumption share
- Services: 15% rate, 45% consumption share

**Blended rate:**
```rust
blended_vat_rate = Σ(rate[category] * demand[commodity]) / Σ(demand[commodity])
```

**Settlement:**
- Citizens debited FULL amount (base + VAT)
- Company credited only base revenue
- VAT routed to Treasury via `settle_b2c_purchase`

---

## SECTION 5: Statecraft & Institutions

### 5.1 Politics & Ministries

Location: `state/src/politics/turn.rs`, `state/src/politics/ministries.rs`, `state/src/politics/parliament.rs`, `state/src/politics/names.rs`

#### Political Year Processing

`process_political_year` runs once per year boundary and:
1. Creates global VIP `HashSet` (Phase 45), pre-populated with party leader names
2. Interest group power migration (fallback mapping to prevent zero power)
3. Election escape hatch (if provisional government >4 years, force-generate real parties)
4. Election safety net (if democratic with <3 active parties, inject additional parties)
5. Party brokerage account initialization + dues/donations collection
6. Regime safety check
7. Snap election trigger (if provisional or <2 real parties)
8. Election execution (seats, coalition, ruling ideology)
9. Coalition stability check
10. Upper house recomputation
11. SOE dividend collection (30% of SOE profits)
12. Patent fee collection

#### Snap Election System

- **Cooldown:** 4 turns (~2 months)
- **Triggers:** Provisional Technocratic Government in power OR <2 real parties with nonzero support
- Sets `years_to_elections = 0` and records `last_snap_election_turn`

#### Ministry Formation

- Number of ministries: `min(15, max(3, (gdp / 1e9) as usize + 3))`
- PM always keeps Treasury and Defense (or InternalSecurity in authoritarian regimes)
- Coalition partners receive portfolios proportional to seat count
- **Phase 45:** Uses global `used_names` set for VIP deduplication

#### Budget Calculation (`calculate_budget_needs`)

- Base budget = 15% of GDP
- Each ministry's share based on ideology weights
- Minimum floor of 10,000 per ministry
- Does NOT debit treasury (allocation happens separately)

#### Cash Allocation (`allocate_cash_to_ministries`)

- Ministries hard-capped by actual `treasury.liquid_reserves`
- Proportional reduction if treasury cannot fully fund
- **Phase 35 fix:** Credits `ministry_cash` field (the "pocket")
- All spending debits from `ministry_cash`, NOT from `liquid_reserves`

#### Minister AI Spending

| Competency | Spending Type |
|------------|---------------|
| HeavyIndustry, LightIndustry, Defense, InternalSecurity | B2B procurement orders |
| Agriculture | Direct subsidies to agricultural companies |
| Infrastructure, Transport | Infrastructure funding |
| Healthcare, Education | Routes through `ministry_public_service_pool` to State Employer |
| SocialWelfare | Dynamic SocialProgram system |
| Treasury, ForeignAffairs, Justice, Science, Energy, Culture, Labor, Housing, StateAssets | Direct transfers |

#### Parliament System

- 0, 1, or 2 chambers based on GovernmentForm
- Parliamentary clubs track anonymized MP seat pools
- **VIP Generation (Phase 45):** `build_vips` pre-populates `used_names` with minister
  names, party leader names, and Head of State. Deputy speakers use global `used_names`
  set, generating unique names instead of cloning party leaders.

#### VIP Name Generation

- Cultural name pools: Slavic, Germanic, Latin, Middle Eastern, Balkan
- `generate_unique_vip` with HashSet deduplication (Phase 41): tries up to 20 times
  to generate unique name. No numeric suffixes or "Jr." fallbacks.

### 5.2 Military System

Location: `state/src/military/units.rs`, `state/src/military/upkeep.rs`, `state/src/military/turn.rs`

#### Unit Types

Infantry, Tanks, Artillery, AirForce, Naval, PeasantBattalion

#### Table of Equipment (ToE) — Phase 45

Era-gated equipment per unit type:
- **Infantry:** Rifles, Clothing, Ammunition (TowedArtillery 1880+, SupportEquipment 1965+)
- **Tanks:** Clothing, Ammunition (LightTanks 1916+, MediumTanks 1935+, HeavyTanks 1942+)
- **AirForce:** Clothing, Ammunition (Fighters/Bombers 1940+, Helicopters 1960+)
- **Naval:** Clothing, Ammunition (Submarines 1935+)
- **PeasantBattalion:** Empty (no equipment, zero upkeep, zero wages)

#### EquipmentReserve

```rust
pub struct EquipmentReserve {
    pub commodity: Commodity,
    pub toe_quantity: f64,        // Target
    pub current_quantity: f64,    // Current
    pub condition: f64,           // 0.0-1.0
    pub depreciation_rate: f64,
}
```

- `replacement_demand() = (toe - current) + current * (1 - condition)`
- `degrade()`: Reduces condition by depreciation_rate per turn; if condition → 0, quantity → 0
- `install()`: Blends new equipment (condition 1.0) with existing

#### MoD Procurement Flow

1. **Degrade equipment** (start of turn, BEFORE procurement)
2. **Calculate demand:** `submit_defense_b2b_orders` aggregates ToE replacement + upkeep
3. **Submit B2B bids:** limit_price = market_price × 1.2 (20% premium), cash-capped
4. **Market clearing:** Bids matched against asks
5. **Delivery:** `deliver_military_supplies_and_equipment` routes equipment → unit reserves
   (proportional by manpower), upkeep commodities → military_stockpile
6. **Resupply:** Units draw from military_stockpile

#### Military Turn Processing

1. Degrade equipment reserves (Phase 45)
2. Process unit upkeep (burn stockpiles, pay wages)
3. Supply delivery from B2B trades (includes equipment)
4. Resupply units from depot
5. Resolve battles on active fronts
6. Decay war exhaustion
7. Disband broken units (return survivors to demographics)
8. Process peasant battalion devastation

**Battle resolution:** Units cloned for battle (to burn supplies without affecting
original until after). Casualties routed back to home regions by rural class.

### 5.3 Central Bank

Location: `state/src/state/central_bank.rs`, `state/src/state/banking.rs`

#### Independence Models

- **Federal:** Strictly independent, governor elected by regional branch presidents
- **CentralIndependent:** Governor appointed by Head of State/Parliament for fixed term
- **Dependent:** Governor acts like minister, can be dismissed, forced to print/lower rates

#### Monetary Mandates

- **Inflationary:** Price stability supreme (Taylor weights: 2.0, 0.5)
- **Market:** Growth/stock market prioritized (Taylor weights: 0.5, 1.5)
- **Mixed:** Balances both (Taylor weights: 1.5, 0.5)

#### Interest Rates (RPP)

5 distinct rates with hierarchy: Lombard > Reference > Rediscount > Discount > Deposit

**Taylor Rule (Phase 36):**
```
reference_rate = neutral_rate + inflation_weight * (inflation - target_inflation)
               + growth_weight * (gdp_growth - potential_growth)
```
- Smoothing: 70% new Taylor rate, 30% previous
- **Phase 40 NIRP:** Floor at -2% (reference), -2.5% (discount), -3% (deposit); cap 20%

**Rate hierarchy:**
- Lombard: reference + 150 bps (cap 25%)
- Rediscount: reference + 50 bps (cap 25%)
- Discount: reference - 75 bps (floor -2.5%)
- Deposit: reference - 150 bps (floor -3%)

#### Open Market Operations (OMO)

- Compares current XIBOR to target rate
- XIBOR > target: CB buys bonds from banks (injects reserves)
- XIBOR < target: CB sells bonds to banks (absorbs reserves)
- Operation size: 10% of total reserves per 100 bps gap, capped at 5x intensity
- Limited by bond availability

#### Reserve Requirements

- `reserve_requirement_ratio`: CB-set (default 0.0, typically 0.10)
- Banks must hold `reserves >= deposits × cb_reserve_ratio`
- Failing banks may face resolution

#### FX Reserves

- `fx_reserves: HashMap<String, f64>` tracking foreign currencies
- Used for trade settlement, gold trading, currency interventions
- **Gold trading:** CB buys/sells via GlobalGoldExchange, debits `fx_reserves`,
  credits `physical_gold_reserves`
- **Gold coverage:** `gold_value_in_currency / M0`

#### QE (Phase 35)

- Triggered when CPI inflation < 0% (deflation)
- CB buys bonds: bank gives up securities, receives fresh reserves
- Capped at 5% of GDP per turn
- Records on `central_bank.omo_bond_holdings` and `liquidity_injected`

---

## SECTION 6: THE DEEP STATIC AUDIT (Defects & Bottlenecks)

### 6.1 Dead Code / Uninitialized Systems

#### 6.1.1 Completely Unused Modules

| File | Status | Evidence |
|------|--------|----------|
| `src/agriculture.rs` | **DEAD CODE** | ~600 lines. Declared in `lib.rs:60` but NO imports found anywhere (`grep use crate::agriculture` = 0 matches). Contains `calculate_agricultural_fte_demand`, `calculate_harvest_yield_and_rot`, `process_agricultural_despawn`. |
| `src/infrastructure/pricing.rs` | **DEAD CODE** | `calculate_capacity_prices()` never called. Comment at line 51: "Helper functions (stubs for now)". |

#### 6.1.2 Partially Implemented / Stub Systems

| File | Lines | Issue |
|------|-------|-------|
| `src/infrastructure/effects.rs` | 332, 365, 444 | "placeholder effect", "placeholder implementation", "stubs for now". Function IS called in `economy/indicators.rs:290` but with placeholder logic. |
| `src/corporate/development.rs` | 206, 395, 404, 484-495 | PropertyDeveloper uses hardcoded `equipment_cost = 10000.0`, `downtime_cost = 2000.0`, `has_leased_office_space()` returns `false`. |
| `src/economy/wholesale.rs` | 111-112, 131 | Uses `"placeholder_region"` and `"placeholder_wholesaler"` as string keys. |
| `src/society/housing.rs` | 856 | "This is a placeholder - actual pollution tracking would be in a dedicated field" |

#### 6.1.3 TODO/FIXME/HACK Comments (62 occurrences)

**Critical TODOs:**
- `corporate/manager.rs:223` — "TODO: Route to citizen savings for individual shareholders."
- `politics/turn.rs:631, 933` — "TODO: Get from country cultural metadata." (hardcoded to "Slavic")
- `state/treasury.rs:297` — "Phase 6.3.5: Placeholder logistics revenue from transport fees"
- `state/tax.rs:1436` — "Placeholder for future wiring."
- `ui/snapshot.rs:895` — "Placeholder — exchange rates loaded from currencies.json at runtime"
- `state/banking.rs:2353` — "XIBOR volatility placeholder"
- `economy/corporate_rd.rs:269` — "40.0 // Placeholder"

#### 6.1.4 Unimplemented Features

- **No `src/laws/` or `src/foreign_policy/` directories** — Laws handled in `politics/laws.rs`
  but many law types are stubs
- **Stub party logic** — `politics/turn.rs:191, 768, 826` references "stub" party
- **Phase migration markers** indicating incomplete migrations: Phase 20 (dead commodity
  extraction), Phase 23A (freight-producing methods), Phase 40 (state-funded tenders),
  Phase 44 (genesis housing), Phase 45 (regression tests)

### 6.2 Conflicting Logic / Duplicate Files

#### 6.2.1 Treasury Naming Conflict

| File | Purpose |
|------|---------|
| `src/government/treasury.rs` | Treasury cycle operations (`settle_rot_fees`, `process_storage_transactions`) |
| `src/state/treasury.rs` | `Treasury` struct definition (`liquid_reserves`, `gdp`, etc.) |

**Recommendation:** Rename to `treasury_operations.rs` vs `treasury_state.rs`

#### 6.2.2 Scattered Config Systems

**37 different `*Config` structs** scattered across modules:
- `MinistryConfig`, `LaborConfig`, `B2bOrderConfig`, `RetailConfig`, `SubsistenceConfig`, etc.
- Some loaded from JSON, others hardcoded defaults
- **Recommendation:** Centralize config loading or document which are data-driven vs hardcoded

#### 6.2.3 No Python/Rust Duplication

No `.py` files found in `state/src/`. Python files exist in `state/government/treasury/`
and `state/economy/indicators/` but are separate analysis tools, not duplicate logic.

### 6.3 Performance Bottlenecks

#### 6.3.1 Heavy Clone Operations

| File | Clone Count | Impact |
|------|-------------|--------|
| `src/engine/generator/corporate.rs` | **87 `.clone()` calls** | World generation likely slow |
| `src/io/save_manager.rs` | 13 | Acceptable (I/O path) |

**Recommendation:** Use references or move semantics in generator. This is genesis-only
so not a turn-loop hot path, but slows world creation.

#### 6.3.2 Nested Loops (O(N²) Patterns)

| File | Lines | Pattern |
|------|-------|---------|
| `src/engine/generator/corporate.rs` | 1413-1418, 1491-1495, 1617-1620, 3424-3432, 3441-3447 | Nested formation/deposit loops (O(N×M)) |
| `src/economy/labor/labor_market.rs` | 326 | `while fte_to_distribute > 0.001` iterative distribution |
| `src/economy/labor/labor_market.rs` | 249-254 | Sort bids by wage (O(N log N)) |
| `src/economy/market/order_book.rs` | 127, 215 | While loops for order matching (standard, acceptable) |

#### 6.3.3 Heavy Allocations in Loops

| File | Line | Issue |
|------|------|-------|
| `src/economy/market/clearing.rs` | 291 | `Vec::new()` + `filter().collect()` on every warehouse extraction call |
| `src/io/save_manager.rs` | 115-192 | 49 `Vec::new()` in default struct (not hot path, acceptable) |

#### 6.3.4 Sorting Every Turn

| File | Line | Sort |
|------|------|------|
| `src/engine/turn.rs` | 5075 | `goods.sort()` |
| `src/economy/logistics/air_cargo.rs` | 166 | `countries.sort()` |
| `src/economy/logistics/logistics.rs` | 860 | `owners.sort()` |
| `src/international/trade.rs` | 130 | `sorted_names.sort()` |
| `src/international/diplomacy.rs` | 70 | `sorted_names.sort()` |

**Recommendation:** Cache sorted lists if they don't change often.

#### 6.3.5 The Big Sequential Bottleneck

The **B2B order submission** (Phase 6.3, sequential) is the single largest sequential
bottleneck. All companies across all countries submit orders in a single loop to
preserve deterministic order-book state.

### 6.4 Half-Measures / Magic Numbers

#### 6.4.1 Critical Magic Numbers

| File | Line(s) | Value | Issue |
|------|---------|-------|-------|
| `src/agriculture.rs` | 17 | `PLACEHOLDER_TRANSPORT_FEE_PER_TON = 5.0` | Hardcoded transport fee |
| `src/agriculture.rs` | 22 | `100.0` | Placeholder base price |
| `src/corporate/strategy.rs` | 395 | `equipment_cost = 10000.0` | Placeholder |
| `src/corporate/strategy.rs` | 404 | `downtime_cost = 2000.0` | Placeholder |
| `src/economy/market/clearing.rs` | 32-33 | `PRICE_FLOOR = 0.2`, `PRICE_CAP = 5.0` | Hardcoded price bounds |
| `src/economy/labor/labor_market.rs` | 215 | `FTE_RETENTION_FLOOR = 0.90` | Hardcoded |
| `src/economy/labor/labor_market.rs` | 226-227 | `MAX_HIRING_GROWTH_RATE = 0.15`, `SMALL_COMPANY_FTE_THRESHOLD = 10.0` | Hardcoded |
| `src/engine/turn.rs` | 3232 | `liquid_reserves * 0.3` | MoD 30% reserve |
| `src/infrastructure/pricing.rs` | 36-40 | HospitalBeds=500, ClinicVisits=50, etc. | Hardcoded capacity prices |
| `src/economy/finance/payment_in_kind.rs` | 246, 256 | `100.0` | Base price fallback |

#### 6.4.2 Placeholder String Keys

| File | Line(s) | Placeholder |
|------|---------|-------------|
| `src/economy/wholesale.rs` | 111-112 | `"placeholder_region"`, `"placeholder_wholesaler"` |
| `src/economy/market/clearing.rs` | 271, 404 | `"logistics_placeholder"` |

#### 6.4.3 Irony: "No Magic Numbers" Comments

66 matches for "magic" or "hardcoded" in comments, many claiming to avoid magic numbers
while still having hardcoded defaults. Examples:
- `economy/market/market_history.rs:3`: "deterministic price fallback chain to avoid magic numbers"
- `corporate/manager.rs:862`: "using a market wage signal as a reference — NOT a hardcoded floor"

### 6.5 Audit Summary & Priority Recommendations

#### High Priority
1. **Delete or integrate `src/agriculture.rs`** — completely unused, 600 lines of dead code
2. **Complete `src/infrastructure/effects.rs`** — called but with placeholder logic
3. **Resolve treasury naming conflict** between `government/treasury.rs` and `state/treasury.rs`
4. **Fix ForeignEntity transfer** — add CB `fx_reserves` debit for foreign transfers
5. **Move reserve check inside `issue_loan()`** — make it a hard invariant

#### Medium Priority
6. **Reduce cloning in `engine/generator/corporate.rs`** — 87 clones in genesis
7. **Centralize configuration** — 37 scattered `*Config` structs
8. **Replace hardcoded capacity prices** in `infrastructure/pricing.rs`
9. **Profile labor market clearing** — iterative distribution loop
10. **Replace placeholder strings** in `economy/wholesale.rs` and `economy/market/clearing.rs`

#### Low Priority
11. **Cache sorted lists** if they don't change often
12. **Resolve TODO comments** — especially cultural metadata hardcoding ("Slavic")
13. **Document which configs are data-driven vs hardcoded**
14. **Add invariant tests** for balance sheet equality and money supply conservation

---

## SECTION 7: Glossary & Key File Index

### Key Terms

| Term | Definition |
|------|------------|
| **Turn** | One simulation step. 24 turns = 1 year. |
| **Wave** | Production execution grouping (Wave 1: Energy, Wave 3: General) |
| **R-Phase** | B2C market phase (R4: Reset, R5: Clearance, R6: Clear, R7: Rents) |
| **SEC-Phase** | Securities market phase (SEC-1 through SEC-8c) |
| **Resurrection Phase** | Major system group (1: Infrastructure, 2: Securities, 3: Military, 9: Real Economy, 10: Emergency) |
| **FTE** | Full-Time Equivalent (labor unit) |
| **ToE** | Table of Equipment (military equipment requirements) |
| **VWAP** | Volume-Weighted Average Price |
| **XIBOR** | Interbank Offered Rate |
| **OMO** | Open Market Operations |
| **QE** | Quantitative Easing |
| **NIRP** | Negative Interest Rate Policy |
| **DSPW** | Domestic Sovereign Bond auction system |
| **MBS** | Mortgage-Backed Security |
| **CDS** | Credit Default Swap |
| **CCP** | Central Counterparty |
| **KNF** | Financial regulator (Polish: Komisja Nadzoru Finansowego) |
| **KIO** | Anti-corruption bureau |
| **PIP** | Construction inspectorate |
| **OSPE** | OSP (Volunteer Fire Department) |
| **SOE** | State-Owned Enterprise |

### Key File Index

| File | Purpose |
|------|---------|
| `state/src/engine/turn.rs` | Main turn loop (177 phases) |
| `state/src/engine/generator/mod.rs` | World generator entry point |
| `state/src/engine/generator/corporate.rs` | Company/building/housing generation |
| `state/src/economy/trade/transfer_settler.rs` | Double-entry transfer engine |
| `state/src/economy/trade/b2b_orders.rs` | B2B order submission and pricing |
| `state/src/economy/trade/retail.rs` | B2C retail clearing |
| `state/src/economy/labor/labor_market.rs` | Labor market clearing |
| `state/src/economy/production/fixed_assets.rs` | Fixed asset depreciation |
| `state/src/economy/finance/payment_in_kind.rs` | Serf in-kind payment + imputed GDP |
| `state/src/state/treasury.rs` | Treasury struct |
| `state/src/state/banking.rs` | Bank balance sheets + banking turn |
| `state/src/state/central_bank.rs` | Central bank + monetary policy |
| `state/src/politics/turn.rs` | Political turn processing |
| `state/src/politics/ministries.rs` | Government formation + budget |
| `state/src/politics/parliament.rs` | Parliament + VIP generation |
| `state/src/politics/names.rs` | VIP name pools |
| `state/src/military/units.rs` | MilitaryUnit + EquipmentReserve + ToE |
| `state/src/military/upkeep.rs` | Defense B2B orders + degradation |
| `state/src/military/turn.rs` | Military turn processing |
| `state/src/construction/bom.rs` | Sector-based construction BOMs (Phase 45) |
| `state/src/construction/tender_market.rs` | Construction tender market |
| `state/src/registries/enums.rs` | Sector, Commodity enums |
| `state/src/registries/production_methods_data.rs` | Production method definitions |
| `state/src/data/consumption_registry.rs` | Consumption baskets |
| `state/src/society/geography.rs` | Region + class demographics |
| `state/src/society/housing.rs` | Housing system |
| `state/src/securities/brokerage.rs` | BrokerageAccount |
| `state/src/io/save_manager.rs` | Save/load + serialization |

### Phase 45 Key Changes (for reference)

1. **Global VIP HashSet** — prevents VIP name duplication across all political generation
2. **Physical Military Units** — `EquipmentReserve`, ToE, degradation, genesis spawn
3. **Fixed Assets** — `Trains` and `DraftAnimals` as fixed assets with replacement demand
4. **Sector-Enum Construction BOMs** — `get_construction_bom(Sector, u32)`, no string matching
5. **Production Method Inputs** — DraftAnimals, Trains, Bricks, Planks, Coke, etc.
6. **B2C Consumption Matrix** — era-gating + wealth-tier multipliers
7. **Dynamic Pricing** — unfilled bid feedback, VWAP de-seeding, no profitability ceiling
8. **Orphaned Commodities** — all 22 previously orphaned commodities now have supply/demand paths
9. **Build/Test/Verify** — 705 tests passing, 0 orphaned supply commodities

---

*End of Architecture Codex. This document is a frozen snapshot for handoff to a fresh AI session.*
