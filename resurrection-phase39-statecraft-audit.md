# Phase 39 — The "Statecraft" Expansion: Ministry Logic, Wage Leaks & Tax Overhaul

**Audit Date:** Current session  
**Status:** Read-only audit — NO code changes applied. Awaiting user approval.

---

## Summary

Phase 38's 3% sticky-wage cap is bypassed by two zero-wage early-exit paths in `set_wage_offers` and by `set_wage_offers` overriding bank wages after `process_banking_turn` already set them. Tax collection shows 0.00 in early turns because wealth/capital-gains brackets default empty and `last_tax_result` is only updated once per year boundary. Elections still miss their cue because the snap-election trigger lives inside `process_political_year`, which is gated to year boundaries only. Eight ministries currently have no physical or economic logic — they burn cash as `DirectTransfer` with no effect.

---

## PART 1: The Sticky Wage Leak & Banking Freeze

### 1.1 Wage Leak — Root Causes (3 distinct bugs)

#### Bug A: Zero-wage early exits bypass the 3% cap

**File:** `state/src/corporate/manager.rs` lines 876–878 and 917–919

```rust
// Line 876-878: Skip companies with no labor demand.
if company.target_fte_demand <= 0.0 {
    company.offered_wage_per_fte = 0.0;   // ← BYPASSES sticky floor
    continue;
}
// Line 917-919: No cash ⇒ no wages
if effective_cash <= 0.0 {
    company.offered_wage_per_fte = 0.0;   // ← BYPASSES sticky floor
    continue;
}
```

Both paths set `offered_wage_per_fte = 0.0` **before** the sticky-wage check at line 979. A company that had a wage of 50,000 last turn and drops to 0 cash this turn shows wage = 0, a 100% cut. When the sector average is computed, these zero-wage companies drag the average down by 30–37%.

**Fix:** Both early-exit paths must respect the sticky floor. If `prev_offered_wage_per_fte > 0.0`, set `offered_wage_per_fte = prev_offered_wage_per_fte * 0.97` (the floor) instead of 0.0. The labor market will then compute `max_affordable_fte = cash / floor_wage`, which will be 0 if cash is 0 — so no hiring happens, but the wage rate doesn't crash the sector average.

#### Bug B: Newly spawned companies dilute the sector average

**File:** `state/src/engine/generator/corporate.rs` — company generation

New companies are generated with `prev_offered_wage_per_fte = 0.0` (default). When `set_wage_offers` runs for the first time, the `else` branch at line 982–983 applies (no sticky floor), and the computed wage may be very low (e.g., 1.0 if the company has minimal cash). This dilutes the sector average.

**Fix (with Turn 1 hard fallback — user correction):** Initialize `prev_offered_wage_per_fte` to `(market_average_wage * 0.8).max(50.0)` for new companies in the generator. The `.max(50.0)` hard fallback is critical: in Turn 1, `market_average_wage` is often `0.0` (no employment history yet), so `0.0 * 0.8 = 0.0` would recreate the very bug we're fixing. The baseline constant of 50.0 ensures no company ever spawns with a 0.0 wage offer, even on the first turn of a fresh game. The sticky floor will then keep their wage reasonable in subsequent turns.

#### Bug C: Bankrupt companies set wage to 0 before death

**File:** `state/src/corporate/manager.rs` — bankruptcy path

When a company goes bankrupt, its buildings are transferred to the auction pool and `current_employment = 0`. But the company entity may persist for one more turn with `offered_wage_per_fte = 0.0` (from Bug A), dragging the sector average down before it's removed.

**Fix:** Exclude companies with `fulfilled_fte == 0 && prev_fulfilled_fte == 0` from the sector average computation, or exclude companies in bankruptcy/receivership state. Alternatively, the Bug A fix (sticky floor on early exits) already prevents the wage from dropping to 0, which solves this.

### 1.2 Banking Freeze — Root Cause

**File:** `state/src/engine/turn.rs` lines 390 and 1718

`process_banking_turn` (line 390) sets `bank.offered_wage_per_fte = (avg_wage * 1.2).max(1.0)` and `bank.target_fte_demand = max(2.0)`. But `set_wage_offers` (line 1718) runs **later** and overrides the bank's wage:

```rust
// set_wage_offers line 917-919:
if effective_cash <= 0.0 {
    company.offered_wage_per_fte = 0.0;  // ← Overrides bank's 1.2×avg_wage
    continue;
}
```

If the bank has no brokerage cash (common for new banks or banks that lent everything), `set_wage_offers` sets wage = 0. The labor market then computes `max_affordable_fte = 0 / 0` → 0, and the bank hires 0 workers despite `target_fte_demand = 2.0`.

**Fix:** Skip companies with `bank_type.is_some()` in `set_wage_offers` — their wages are already set by `process_banking_turn`. This preserves the bank's wage offer and lets the labor market compute `max_affordable_fte = bank_cash / bank_wage`, which may be low but won't be 0 if the bank has any cash.

**Secondary banking issue:** Banks earn nothing from their loan portfolio in `process_banking_turn` — interest income is not credited to `brokerage_account.cash`. The bank's cash only comes from new deposits and loan repayments. If no new loans are issued and deposits are static, the bank has no cash for payroll.

**Fix:** In `process_banking_turn`, credit each bank's `brokerage_account.cash` with the interest earned on outstanding loans (turn fraction of annual rate × outstanding balance). This is double-entry: debit loan interest receivable, credit bank cash. This gives banks a steady income stream to sustain teller employment.

---

## PART 2: Tax Blackout, Wealth Taxes & Customs

### 2.1 Collection Timing — The Year-Boundary Problem

**File:** `state/src/engine/turn.rs` lines 2643–2683 and 2870–2875

Tax collection runs **every turn** (line 2678, inside the main turn loop, not gated by `is_year_boundary`). So `last_tax_result` IS updated every turn. The 0.00 display in early turns is NOT a timing issue — it's a **rate configuration issue** (see 2.2).

However, there IS a persistence issue: `last_tax_result` is `#[serde(skip)]`, so it's lost on save/load. If the user saves and reloads mid-year, the Finance tab shows 0.00 until the next tax collection turn.

**Fix:** Keep `#[serde(skip)]` (it's ephemeral diagnostics), but ensure `process_tax_collection_turn` runs on the first turn after load. This already happens since tax collection runs every turn. The real fix is just ensuring the result is stored (already done in Phase 38).

### 2.2 Ideology Defaults — Wealth & Capital Gains Taxes Globally 0%

**File:** `state/src/engine/generator/mod.rs` lines 800–801

```rust
wealth_tax: crate::state::tax::WealthTax::default(),        // brackets: Vec::new() → 0%
capital_gains_tax: crate::state::tax::CapitalGainsTax::default(), // brackets: Vec::new() → 0%
```

`WealthTax::default()` and `CapitalGainsTax::default()` both have empty bracket vectors. The tax calculation functions check `brackets.is_empty()` and return 0.0. No ideology configures these brackets.

**File:** `state/src/politics/turn.rs` line 695–718 — `apply_ruling_ideology_policies`

This function sets trade doctrine, labor law, healthcare, education, etc. from ideology preferences, but does **NOT** configure wealth tax or capital gains tax brackets.

**Fix:** Add a new function `apply_ideology_tax_policy(country: &mut Country)` called from `apply_ruling_ideology_policies`. This function sets wealth tax and capital gains tax brackets based on the ruling ideology's economic school:

| Ideology School | Wealth Tax | Capital Gains Tax |
|---|---|---|
| Socialist/Marxist | 2% on assets > 1M, 5% on assets > 10M | 30% (all gains) |
| Social Democratic | 1% on assets > 2M, 3% on assets > 10M | 19% (Belka tax) |
| Keynesian | 0.5% on assets > 5M | 19% |
| Centrist/Liberal | 0% (no wealth tax) | 19% |
| Monetarist/Classical | 0% | 15% |
| Neoliberal | 0% | 10% |
| Anarcho-Capitalist | 0% | 0% |

Also update `build_tax_rates` in the generator to set baseline brackets (Belka tax: 19% capital gains, 1% wealth tax on assets > 5M) so that turn-0 tax collection yields nonzero revenue even before the first election.

### 2.3 New Revenue Streams — Customs & State Property

**File:** `state/src/state/tax.rs` lines 1171–1192

`TaxCollectionResult` currently has no fields for customs revenue or state property revenue. Customs (tariffs) are collected physically in `settle_trades_with_tariffs` (b2b_orders.rs line 594) and credited directly to treasury, but the amount is not tracked in `TaxCollectionResult`. State property revenue (SOE dividends, patent licensing, state forest remittances) is also credited directly to treasury but not tracked.

**Fix:** Add two fields to `TaxCollectionResult`:

```rust
pub customs_revenue: f64,        // Tariffs collected on cross-border trades
pub state_property_revenue: f64, // SOE dividends, patents, state forest remittances
```

**Customs revenue tracking (STRICT DOUBLE-ENTRY — user correction):** Customs (tariffs) must **physically deduct cash** from the importing/exporting companies during `b2b_orders.rs` clearing. The existing `settle_trades_with_tariffs` (b2b_orders.rs:594) already does this — it debits the buyer's encumbered cash and credits the buyer's country treasury via `TransferSettler`. The tariff amount is accumulated in `CustomsState.tariff_revenue_collected` (laws.rs:519). Wire this into `TaxCollectionResult.customs_revenue` during `process_tax_collection_turn` by reading `country.politics.customs_state.tariff_revenue_collected`. **If the payer has no cash, the tariff is recorded as evaded** (added to `taxes_evaded`), NOT magically created. No money printing.

**Wealth Tax double-entry (STRICT — user correction):** Wealth tax must **physically deduct LIQUID CASH** from `ClassDemographics.savings` (citizen liquid savings). The existing `calculate_wealth_tax` function in `tax.rs` must be audited to ensure it:
1. Debits `ClassDemographics.savings` (liquid savings only — NOT illiquid assets like real estate or equities).
2. Credits `country.budget.liquid_reserves` with the same amount.
3. If the citizen class has insufficient liquid savings, the unpaid portion is recorded as `taxes_evaded` — NOT collected from thin air.

**Capital Gains Tax double-entry (STRICT — user correction):** Capital gains tax must **physically deduct LIQUID CASH** from the entity's cash holdings. The existing `calculate_capital_gains_tax` function in `tax.rs` must be audited to ensure it:
1. Debits ONLY from `company.available_cash`, `company.brokerage_account.cash`, or `ClassDemographics.savings` (for citizens). **NEVER from `PrivateCapital`** — `PrivateCapital` is an abstract aggregate of equity and physical assets, not spendable currency. You cannot debit abstract equity or physical assets to pay a tax bill.
2. Credits `country.budget.liquid_reserves`.
3. If the entity has no liquid cash, the unpaid portion is recorded as `taxes_evaded`.

**State property revenue tracking:** Accumulate `state_forest_state.treasury_remittance` (already tracked at `state_forests.rs:189`, already double-entry via `TransferSettler`) plus SOE dividends (annual, see Part 4.4) and patent licensing fees into `TaxCollectionResult.state_property_revenue` during `process_tax_collection_turn`. Patent licensing fees must be physically deducted from companies paying for state patents (debit company cash, credit treasury).

**Finance tab display:** Add `customs_revenue` and `state_property_revenue` to `FinanceSnapshot`, and add rows in the Finance tab rendering:

```
Customs Revenue:     12,345
State Property:       8,901
```

---

## PART 3: Election Trigger Misalignment & VIP Exhaustion

### 3.1 Election Trigger — The Year-Boundary Gate

**File:** `state/src/engine/turn.rs` lines 2870–2875

```rust
let is_year_boundary = turn > 0 && (turn + 1) % 24 == 0;
if is_year_boundary {
    tasks.par_iter_mut().for_each(|task| {
        process_political_year(task.ctx.country, &mut task.companies, &mut task.unions, task.ctx.year);
    });
}
```

The snap-election trigger (added in Phase 38) lives inside `process_political_year` at `politics/turn.rs:287-296`. Since `process_political_year` only runs at year boundaries (turn 23, 47, 71...), the snap-election trigger only fires once per year. If a country falls into provisional government at turn 5, it waits until turn 23 for the snap election — an 18-turn deadlock.

**Fix:** Extract the snap-election trigger into a lightweight `check_snap_election(country: &mut Country) -> Vec<String>` function that runs **every turn**, outside the year-boundary gate. This function only checks:
1. Is the country democratic?
2. Is the ruling party "Provisional Technocratic Government"?
3. Are there fewer than 2 real parties with nonzero support?

If yes, set `years_to_elections = 0` and push a message. The actual election still fires inside `process_political_year` at the year boundary (since `years_to_elections == 0` triggers `election_due`). But if we want the election to fire immediately (not wait for year boundary), we need to call a minimal election function every turn when `years_to_elections == 0`.

**Better fix:** Move the election-checking block (lines 298-339 of `politics/turn.rs`) into a separate `run_election_if_due(country: &mut Country) -> Vec<String>` function. Call this every turn after the snap-election check. This way:
1. Every turn: `check_snap_election` forces `years_to_elections = 0` if provisional
2. Every turn: `run_election_if_due` fires an election if `years_to_elections == 0`
3. Year boundary: Full `process_political_year` runs (party regeneration, interest groups, coalition stability, etc.)

This breaks the deadlock immediately without waiting for the year boundary.

**Infinite loop guard:** The snap-election trigger must NOT fire if `years_to_elections` was already 0 in the previous turn (meaning an election already failed). Add a `last_snap_election_turn` field to `Politics` and only trigger if `current_turn - last_snap_election_turn >= 4` (one month cooldown).

### 3.2 VIP Generation — "Minister ()" Bug

**File:** `state/src/politics/ministries.rs` lines 506–518

```rust
fn resolve_minister_name(active_parties: &HashMap<String, Party>, party_id: &str) -> String {
    let name = active_parties
        .get(party_id)
        .map(|p| p.leader.name.clone())
        .unwrap_or_default();
    if name.is_empty() {
        format!("Minister ({})", party_id)  // ← "Minister ()" if party_id is ""
    } else {
        name
    }
}
```

When `coalition.first().cloned().unwrap_or_default()` returns `""` (empty coalition), `active_parties.get("")` returns `None`, `unwrap_or_default()` returns `""`, and the fallback produces `"Minister ()"`.

**Fix:** In `resolve_minister_name`, if `party_id` is empty, generate a random VIP name instead of using the fallback format:

```rust
fn resolve_minister_name(active_parties: &HashMap<String, Party>, party_id: &str) -> String {
    if party_id.is_empty() {
        // No party — generate a technocrat name
        let mut rng = rand::thread_rng();
        return crate::politics::names::generate_full_vip("Slavic", &mut rng).full_name;
    }
    let name = active_parties
        .get(party_id)
        .map(|p| p.leader.name.clone())
        .unwrap_or_default();
    if name.is_empty() {
        let mut rng = rand::thread_rng();
        crate::politics::names::generate_full_vip("Slavic", &mut rng).full_name
    } else {
        name
    }
}
```

Also ensure `generate_full_vip` always returns a non-empty name (it does — fallback is "Jan Kowalski" at `names.rs:280-286`).

**Secondary VIP issue:** When a party holds multiple ministries, the generator at line 422-427 generates new VIP names for subsequent ministries. This is correct. But the `cultural_group` used is `&country.macro_indicators.cultural_group` which may be empty for some countries. Fix: fall back to `"Slavic"` if empty.

---

## PART 4: The "Statecraft" Ministry Expansion

### 4.0 Architectural Constraints

**All new buildings must be built via `ConstructionTenders`** — no magical spawning.  
**All public wages must be routed through the `State Employer`** pseudo-company at `turn.rs:1766-1816`.  
**All cash flows must be double-entry** — debit ministry_cash, credit the target (building, company, or treasury).  
**No money printing** — spending is capped by `ministry_cash`.

### 4.1 Current Ministry Logic Status

| Competency | Current Logic | Status |
|---|---|---|
| HeavyIndustry | B2B procurement (Steel, Machinery) | Active |
| LightIndustry | B2B procurement (Clothing, LuxuryClothing) | Active |
| Agriculture | Subsidies to agriculture companies | Active |
| Infrastructure | `InfrastructureFunding` (no physical effect) | Stub |
| InternalSecurity | B2B procurement (Clothing, Machinery) | Active |
| ForeignAffairs | `DirectTransfer` (no effect) | **Dead** |
| Defense | B2B procurement (Steel, Machinery) | Active |
| Education | `PublicServiceWages` → State Employer | Active |
| Healthcare | `PublicServiceWages` → State Employer | Active |
| SocialWelfare | Handled by SocialProgram system | Active |
| Justice | `DirectTransfer` (no effect) | **Dead** |
| Treasury | `DirectTransfer` (no effect) | **Dead** |
| Science | `RAndDGrant` to companies | Partial |
| Energy | `DirectTransfer` (no effect) | **Dead** |
| Transport | `InfrastructureFunding` (no physical effect) | Stub |
| Housing | `DirectTransfer` (no effect) | **Dead** |
| Culture | `DirectTransfer` (no effect) | **Dead** |
| Labor | `DirectTransfer` (no effect) | **Dead** |
| Environment | `DirectTransfer` (no effect) | **Dead** |

### 4.2 New Building Types

All new buildings are `Sector::PublicServices` or `Sector::PublicAdministration` with `owner_id = "State"`, generated via `ConstructionTender` with `TenderInvestorType::State` and `investor_id = "STATE:<ministry_id>"`.

| Building Type | Sector | Ministry | Effect |
|---|---|---|---|
| `Court` | PublicServices | Justice | Already exists in generator. Produces `JusticeCapacity` output. Increases case throughput, reduces backlog. |
| `CustomsOffice` | PublicAdministration | Treasury | Produces `CustomsCapacity` output. Increases tariff collection efficiency and smuggling interception. |
| `Embassy` | PublicAdministration | ForeignAffairs | Built in **foreign** regions (see 4.3). Improves diplomatic relations, aids citizens abroad. |
| `ResearchInstitute` | PublicServices | Science | Produces `ResearchOutput` (new BuildingOutput). Generates state patents → `state_property_revenue`. |
| `LaborInspectorate` | PublicAdministration | Labor | Produces `LaborInspectionCapacity` (already exists in enums.rs:552). Increases shadow economy detection. |
| `PublicWorksSite` | PublicServices | Labor | Temporary employment during high unemployment. Hires unemployed citizens for infrastructure maintenance. |
| `SocialHousing` | PublicServices | Housing | Already exists as `ConstructionProjectType::SocialHousing`. Provides affordable housing, reduces homelessness. |

**Note:** `Sanepid Station`, `Building Inspectorate`, `Environmental Inspectorate` already exist and are published via `anti_corruption::maybe_publish_inspectorate_tender`. The State Assets competency should manage these.

### 4.3 Embassy Mapping to Foreign Regions

Embassies are unique — they're built in **foreign** regions, not domestic ones.

**Decision (user-approved):** Embassies will be **physical buildings** stored on the host country's building list. This is more realistic and integrates with the existing construction tender and State Employer systems.

**CRITICAL ARCHITECTURAL CORRECTION (user correction — borrow-checker safety):**
The engine uses Rayon (`tasks.par_iter_mut()`) for concurrent country processing. Mutating Country B's `phase22_tenders` or `buildings` from Country A's thread is a **catastrophic violation of Rust's mutable borrowing rules** — it will not compile. The solution is a **deferred event queue** on `GameState`:

1. **Deferred Diplomatic Queue:** Add `pub pending_diplomatic_actions: Vec<DiplomaticAction>` to `GameState` (NOT on `Country` — it must be shared/sequential).
2. **Enum definition:**
   ```rust
   pub enum DiplomaticAction {
       EmbassyConstructionRequest {
           home_country: String,
           host_country: String,
           host_region_id: String,
           funding_amount: f64,  // debited from home country's ministry_cash
       },
       EmbassyFundingTransfer {
           home_country: String,
           host_country: String,
           amount: f64,  // ongoing staff funding
       },
   }
   ```
3. **During `par_iter_mut` (STRICT — user correction, NO Mutex):** Do NOT use `Mutex`, `RwLock`, or any interior mutability to collect diplomatic actions. Instead, use Rayon's **functional pattern** — the per-country turn closure (or function) **returns** a `Vec<DiplomaticAction>`. The engine uses `.map(...).flatten().collect::<Vec<_>>()` to safely gather all diplomatic actions across all threads into a single vector without any locking. Implementation:
   ```rust
   // Instead of: tasks.par_iter_mut().for_each(|task| { ... })
   // Use:
   let diplomatic_actions: Vec<DiplomaticAction> = tasks
       .par_iter_mut()
       .map(|task| {
           // ... process country turn ...
           // Return diplomatic actions generated this turn
           std::mem::take(&mut task.pending_diplomatic_actions)
       })
       .flatten()
       .collect();
   // Now drain sequentially:
   for action in diplomatic_actions {
       execute_diplomatic_action(action, &mut state);
   }
   ```
   This requires changing the turn processing closure from `for_each` (returns `()`) to `map` (returns a value) + `flatten` + `collect`. The `Task` struct holds a `pending_diplomatic_actions: Vec<DiplomaticAction>` field that each country's turn populates locally, and `std::mem::take` extracts it for return.
4. **After `par_iter_mut` (sequential):** The engine drains the collected `diplomatic_actions` vector:
   - For `EmbassyConstructionRequest`: debit home country's treasury (or ministry_cash), credit host country's treasury, inject a `ConstructionTender` into host country's `phase22_tenders`.
   - For `EmbassyFundingTransfer`: debit home country's treasury, credit host country's treasury.
5. **Building ownership:** On tender completion, the embassy building is added to the host country's `buildings` list with `owner_id = "STATE:<home_country>"` and `sector = PublicAdministration`.
6. **Diplomatic relations:** Each embassy generates a `DiplomaticRelation` boost between home and host countries (stored on `Politics.diplomatic_relations`).
7. **Embassy staff:** Hired from the host country's labor market. The State Employer pseudo-company in the host country includes embassy buildings (filtered by `owner_id.starts_with("STATE:")`) in its capacity calculation.

**Cross-country transfer mechanism (double-entry):** The home country's treasury is debited, and the host country's `budget.liquid_reserves` is credited (diplomatic spending flows to the host economy). Both legs happen sequentially after the parallel turn loop, ensuring no borrow-checker violations.

### 4.4 Ministry Logic Implementation Plan

#### Justice
- **Buildings:** `Court` (already generated). New: `ProsecutorOffice` (produces `ProsecutionCapacity`).
- **Logic:** Each turn, compute `justice_capacity = sum(Court.worker_capacity * current_employment)`. This capacity is already used by `sentencing.rs` and `bribery.rs`. The Justice ministry's spending funds Court maintenance and new Court construction tenders.
- **Spending:** `InfrastructureFunding` for Court maintenance, `ConstructionTender` for new Courts.

#### Treasury / Finance
- **Buildings:** `CustomsOffice` (Służba Celno-Skarbowa). Produces `CustomsCapacity` output.
- **Logic:** Each turn, `customs_capacity = sum(CustomsOffice.worker_capacity * current_employment)`. This capacity is already used by `smuggling.rs::process_customs_evasion_recovery`. The Treasury ministry's spending funds CustomsOffice maintenance and construction.
- **Tax Collection Efficiency:** Customs capacity increases the fraction of tariffs actually collected (reduces smuggling leakage). Formula: `collection_efficiency = (customs_capacity / import_volume).min(0.95)`.
- **Spending:** `InfrastructureFunding` for CustomsOffice maintenance, `ConstructionTender` for new CustomsOffices.

#### Foreign Affairs
- **Buildings:** `Embassy` (see 4.3 for mapping).
- **Logic:** Each embassy generates a diplomatic relation boost. The ministry funds embassy staff via `PublicServiceWages` and new embassy construction via `ConstructionTender`.
- **Spending:** `PublicServiceWages` for embassy staff, `ConstructionTender` for new embassies.

#### Science
- **Buildings:** `ResearchInstitute`. Produces `ResearchOutput` (new BuildingOutput).
- **Logic:** Research output accumulates as `state_patents` (new field on `Country`). Each patent yields `state_property_revenue` (licensing fees from companies using the technology). The ministry also funds `RAndDGrant` to private companies (already exists).
- **Patent Revenue (STRICT — user correction, NO MAGIC MATH):** Do NOT use `patent_count * licensing_fee_per_patent` — that creates magical fiat money. Instead, **physically iterate** over the actual active companies in the economy and attempt to deduct the licensing fee from each company's liquid cash. Implementation:
  1. Compute `licensing_fee_per_company = state_patents * per_patent_fee / active_company_count` (spread across the economy).
  2. For each active company: attempt to debit `min(licensing_fee, company.available_cash + brokerage_account.cash)` from the company's liquid cash.
  3. Credit only the **successfully collected** amount to `country.budget.liquid_reserves`.
  4. Record the collected amount in `TaxCollectionResult.state_property_revenue`.
  5. If a company is broke (no liquid cash), the State gets nothing for that company — no phantom revenue.
  6. Track uncollected fees as `patent_fees_evaded` (diagnostic only, not collected later).
- **Spending:** `InfrastructureFunding` for ResearchInstitute maintenance, `ConstructionTender` for new institutes, `RAndDGrant` for private sector grants.

#### Energy
- **Buildings:** No new buildings. Energy ministry manages SOE energy companies (companies with `sector == Energy && state_share >= 1.0`).
- **Logic:** The ministry subsidizes state energy companies during shortages (price-cap subsidies). It also invests in grid infrastructure via `InfrastructureFunding`.
- **Price-Cap Subsidy:** When energy prices exceed `energy_price_cap` (set by ideology), the ministry pays the difference between market price and cap price for each unit consumed. This is a `Subsidy` to energy companies.
- **Spending:** `Subsidy` to state energy companies, `InfrastructureFunding` for grid.

#### Labor
- **Buildings:** `LaborInspectorate` (produces `LaborInspectionCapacity`, already in enums.rs:552). New: `PublicWorksSite` (temporary employment).
- **Logic:** Labor inspection capacity increases shadow economy detection (already used by `legal_status.rs`). Public works sites hire unemployed citizens during high unemployment (>10%). The ministry funds these via `PublicServiceWages`.
- **Public Works:** When `unemployment_rate > 0.10`, the ministry publishes `ConstructionTender` for `PublicWorksSite` buildings. These buildings have `worker_capacity = 100` and hire from the unemployed pool. Workers are paid via the State Employer.
- **FUNDING TRANSFER (STRICT — user correction):** The Labor Ministry MUST transfer funds from its `ministry_cash` to `country.ministry_public_service_pool` to cover PublicWorksSite wages. If the ministry relies on the State Employer without providing funding, the State Employer will drain the central `liquid_reserves` directly, bypassing the Ministry's budget cap and creating a massive double-entry cash leak. Implementation:
  1. Before the State Employer processes payroll, the Labor Ministry computes `public_works_wage_cost = sum(PublicWorksSite.current_employment) * state_employer_wage`.
  2. The ministry transfers `min(public_works_wage_cost, ministry.ministry_cash)` from `ministry.ministry_cash` to `country.ministry_public_service_pool` (double-entry: debit ministry_cash, credit pool).
  3. The State Employer then pays wages from the pool, capped by the pool's balance. If the pool is insufficient, wages are pro-rated — no deficit spending from `liquid_reserves`.
- **Spending:** `InfrastructureFunding` for LaborInspectorate maintenance, `ConstructionTender` for new inspectorates and public works sites, `PublicServiceWages` (with explicit ministry→pool transfer) for public works employees.

#### Housing
- **Buildings:** `SocialHousing` (already exists as `ConstructionProjectType::SocialHousing`).
- **Logic:** The ministry publishes `ConstructionTender` for `SocialHousing` buildings. These provide affordable housing, reducing homelessness and increasing `housing_satisfaction`. The ministry also imposes rent controls (caps rent at `rent_cap` set by ideology).
- **Rent Control:** When `average_rent > rent_cap`, the ministry pays the difference to landlords as a `Subsidy`. This prevents landlord bankruptcy while keeping rent affordable.
- **Spending:** `ConstructionTender` for social housing, `Subsidy` for rent control.

#### Culture
- **Buildings:** `NationalTheater`, `NationalLibrary` (new physical building types).
- **Logic:** The ministry funds arts and culture by subsidizing local government libraries and cultural institutions. It publishes `ConstructionTender` for `NationalTheater` and `NationalLibrary` buildings in major regional capitals (megaregion centers). These buildings produce `CulturalOutput` (new `BuildingOutput` variant) which increases `cultural_prestige` (new field on `Country`) and citizen satisfaction.
- **Library Subsidies:** The ministry sends `Subsidy` transfers to local government (regional) budgets for operating public libraries. The subsidy amount is proportional to regional population.
- **Cultural Prestige:** Each `NationalTheater` and `NationalLibrary` contributes to `cultural_prestige`, which boosts tourism revenue and international soft power.
- **Spending:** `ConstructionTender` for NationalTheater/NationalLibrary, `Subsidy` for library operations, `PublicServiceWages` for cultural institution staff.

#### Transport
- **Buildings:** No new buildings. Transport ministry subsidizes private transport companies and creates state-owned public transport entities.
- **Logic:** The ministry identifies critical logistics gaps (regions with `transport_coverage < 0.5` or `commute_coverage < 0.5`). In these gaps, it creates state-owned `PublicTransportCompany` entities (companies with `sector = TransportLogistics` and `state_share = 1.0`) to provide minimum service. These companies are funded via `Subsidy` from the ministry.
- **Private Subsidies:** The ministry subsidizes private transport companies (`sector = TransportLogistics`) that operate in under-served regions. The subsidy is proportional to the company's service coverage in gap regions.
- **Public Transport Entity Creation:** When a critical gap is identified, the ministry publishes a `ConstructionTender` for a `TransportDepot` building (new type) with `owner_id = "State"`. On completion, a state-owned transport company is created to operate from that depot.
- **Spending:** `Subsidy` to private transport companies, `ConstructionTender` for TransportDepot, `PublicServiceWages` for state transport employees.

#### State Assets (New Competency)
- **Decision (user-approved):** New **separate** `GovernmentCompetency::StateAssets` competency. `Environment` remains its own competency.
- **Buildings:** Existing `Sanepid Station`, `Building Inspectorate`, `Environmental Inspectorate` (already published via `anti_corruption`). Existing `StateForest` buildings (currently named `Nadleśnictwo` — see Polish String Purge below).
- **Logic:** The ministry manages state forests (already handled by `state_forests.rs`), inspectorates (already handled by `inspectorates.rs`), and collects SOE dividends.
- **SOE Dividends (ANNUAL ONLY — user correction):** Dividends are an annual financial event, NOT a per-turn extraction. Siphoning cash from a company every 2 weeks (every turn) would destroy its operational liquidity. SOE Dividends are calculated and extracted **ONCE per year** during `process_political_year` (year-boundary clearing) based on the **accumulated annual net profit**. Implementation:
  1. Add `annual_profit_accumulator: f64` to `Company` (accumulates `last_profit` each turn).
  2. Each turn, after production: `company.annual_profit_accumulator += company.last_profit`.
  3. At year boundary (`process_political_year`): For each company with `state_share >= 1.0` and `annual_profit_accumulator > 0`, remit 30% of the accumulated profit to treasury. Double-entry: debit `company.available_cash` (or `brokerage_account.cash`), credit `country.budget.liquid_reserves`. Record in `TaxCollectionResult.state_property_revenue`.
  4. Reset `company.annual_profit_accumulator = 0.0` after dividend extraction.
  5. If the company doesn't have enough cash to pay the dividend, pay what's available and carry the rest as `dividend_arrears` (paid next year). No money printing.
- **Spending:** `InfrastructureFunding` for inspectorate maintenance, `ConstructionTender` for new inspectorates.

### 4.5 New GovernmentCompetency Variant

Add `GovernmentCompetency::StateAssets` to the enum. Update `default_competency_bundles` to include it. Update `BudgetPriorities::weight_for` with a weight for StateAssets (e.g., `self.infrastructure * 0.3`).

### 4.6 New ConstructionProjectType Variants

Add to `ConstructionProjectType`:
- `Court` (for new court construction)
- `CustomsOffice`
- `Embassy`
- `ResearchInstitute`
- `LaborInspectorate`
- `PublicWorksSite`
- `NationalTheater` (Culture ministry)
- `NationalLibrary` (Culture ministry)
- `TransportDepot` (Transport ministry — state-owned public transport)

Each has a BOM (bill of materials) in `construction/bom.rs` requiring Steel, IndustrialMachinery, and OfficeMachinery.

### 4.7 New BuildingOutput Variants

Add to `BuildingOutput` enum (if not already present):
- `ResearchOutput` — produced by ResearchInstitute
- `ProsecutionCapacity` — produced by ProsecutorOffice
- `CulturalOutput` — produced by NationalTheater and NationalLibrary

`CustomsCapacity`, `LaborInspectionCapacity`, `SanitaryInspectionCapacity`, `BuildingInspectionCapacity`, `EnvironmentalInspectionCapacity` already exist.

### 4.8 Ministry Spending Action Updates

Update `MinistrySpendingAction` enum with:
- `ConstructionTenderPublished { tender_id: String, building_type: String, estimated_cost: f64 }`
- `PatentRevenueCollected { patent_count: u32, total_revenue: f64 }`
- `SoEDividendCollected { company_id: String, amount: f64 }` (annual only)
- `RentControlSubsidy { landlord_id: String, amount: f64 }`
- `PriceCapSubsidy { company_id: String, amount: f64 }`
- `LibrarySubsidy { region_id: String, amount: f64 }` (Culture)
- `CulturalInstitutionWages { building_ids: Vec<String>, total_amount: f64 }` (Culture)
- `TransportSubsidy { company_id: String, amount: f64 }` (Transport)
- `PublicTransportEntityCreated { company_id: String, depot_building_id: String }` (Transport)

### 4.9 Polish String Purge — State Forests (user correction)

**The Rule:** No Polish words in source code. All internal code references, struct names, building names, and comments regarding state forests must be refactored to English (`StateForest` or `StateForestry`).

**Files affected (12 occurrences across 3 files):**

1. **`state/src/registries/mod.rs:56`** — Building name registry mapping:
   - `("Nadleśnictwo", "forest_district")` → `("StateForest", "forest_district")`

2. **`state/src/registries/production_methods.rs:849-886`** — Production method registry:
   - Comment `// -- Nadleśnictwo (Forest District ...) --` → `// -- StateForest (Forest District ...) --`
   - Variable name `nadrlesnictwo` → `state_forest_methods`
   - Registry key `"Nadleśnictwo"` → `"StateForest"`
   - Production method `"Gospodarka Leśna"` → `"Forestry Management"` (also Polish)

3. **`state/src/economy/state_sector/state_forests.rs`** — 9 occurrences:
   - Doc comment `Implements the \`Lasy Państwowe\` (State Forests) mechanic` → `Implements the State Forests mechanic`
   - Comment `Harvested timber enters the Nadleśnictwo building's inventory` → `Harvested timber enters the StateForest building's inventory`
   - Comment `to find Nadleśnictwo buildings and inject timber` → `to find StateForest buildings and inject timber`
   - Comment `Timber enters Nadleśnictwo building inventory` → `Timber enters StateForest building inventory`
   - Comment `Inject harvested timber into Nadleśnictwo buildings` → `Inject harvested timber into StateForest buildings`
   - Variable `nadrlesnictwo_buildings` → `state_forest_buildings`
   - Filter `b.name == "Nadleśnictwo"` → `b.name == "StateForest"`
   - Test building name `"Nadleśnictwo"` → `"StateForest"`
   - Test assertion comments referencing `Nadleśnictwo` → `StateForest`

**Important:** This is a breaking change for existing saves — buildings named `"Nadleśnictwo"` in save files will no longer match the new `"StateForest"` name. The save loader must include a migration step that renames existing `"Nadleśnictwo"` buildings to `"StateForest"` on load. Add this to `save_manager.rs` as a post-load normalization step.

---

## Implementation Steps

### Step 1: Fix Sticky Wage Leak (Part 1)
1. `manager.rs:876-878` — Apply sticky floor on zero-demand early exit
2. `manager.rs:917-919` — Apply sticky floor on zero-cash early exit
3. `generator/corporate.rs` — Initialize `prev_offered_wage_per_fte = (market_average_wage * 0.8).max(50.0)` for new companies (hard fallback for Turn 1)
4. Update wage tests to verify zero-cash companies don't crash the sector average

### Step 2: Fix Banking Freeze (Part 1)
1. `manager.rs:874` — Skip companies with `bank_type.is_some()` in `set_wage_offers`
2. `banking.rs` — Credit interest income to `brokerage_account.cash` each turn
3. Update bank employment tests

### Step 3: Fix Tax Blackout & Add New Revenues (Part 2)
1. `tax.rs:1173` — Add `customs_revenue` and `state_property_revenue` to `TaxCollectionResult`
2. `generator/mod.rs:800-801` — Set baseline wealth tax and capital gains brackets in `build_tax_rates`
3. `politics/turn.rs:695` — Add `apply_ideology_tax_policy` to `apply_ruling_ideology_policies` (runs every turn)
4. `tax.rs` — Audit `calculate_wealth_tax` to ensure it physically debits `ClassDemographics.savings` (LIQUID only) and credits treasury. If insufficient savings → record as `taxes_evaded`. No money printing.
5. `tax.rs` — Audit `calculate_capital_gains_tax` to ensure it physically debits entity LIQUID cash (`available_cash`, `brokerage_account.cash`, or `ClassDemographics.savings`) — NEVER `PrivateCapital` (illiquid). If insufficient cash → record as `taxes_evaded`.
6. `b2b_orders.rs` — Verify `settle_trades_with_tariffs` physically debits buyer cash and credits treasury. If buyer has no cash → record as evaded.
7. `tax.rs` — Read `CustomsState.tariff_revenue_collected` and `state_forest_state.treasury_remittance` into `TaxCollectionResult`
8. `snapshot.rs` — Add `customs_revenue` and `state_property_revenue` to `FinanceSnapshot`
9. `render.rs` — Add Customs and State Property rows to Finance tab

### Step 4: Fix Election Trigger (Part 3)
1. `politics/turn.rs` — Extract `check_snap_election` and `run_election_if_due` from `process_political_year`
2. `engine/turn.rs` — Call `check_snap_election` and `run_election_if_due` every turn, outside year-boundary gate
3. `politics/system.rs` — Add `last_snap_election_turn: u32` to `Politics` for cooldown
4. Update election tests

### Step 5: Fix VIP Generation (Part 3)
1. `ministries.rs:508` — Generate random VIP name when `party_id` is empty or leader name is empty
2. `ministries.rs:446` — Fall back to `"Slavic"` if `cultural_group` is empty
3. Update minister name tests

### Step 6: Statecraft Ministry Expansion (Part 4)
1. `ministries.rs` — Add `GovernmentCompetency::StateAssets` variant
2. `construction/projects.rs` — Add new `ConstructionProjectType` variants (Court, CustomsOffice, Embassy, ResearchInstitute, LaborInspectorate, PublicWorksSite, NationalTheater, NationalLibrary, TransportDepot)
3. `registries/enums.rs` — Add new `BuildingOutput` variants (ResearchOutput, ProsecutionCapacity, CulturalOutput)
4. `generator/corporate.rs` — Add recipes for new building types
5. `construction/bom.rs` — Add BOMs for new building types
6. `entities/mod.rs` — Add `annual_profit_accumulator: f64` to `Company` for annual SOE dividend calculation
7. `ministries.rs` — Implement spending logic for Justice, Treasury, ForeignAffairs (physical embassies via deferred queue), Science (physical patent fee collection from companies), Energy, Labor (with explicit ministry→pool wage transfer), Housing, Culture, Transport, StateAssets
8. `state/mod.rs` — Add `DiplomaticAction` enum and `pending_diplomatic_actions: Vec<DiplomaticAction>` to `GameState`
9. `engine/turn.rs` — Accumulate `company.annual_profit_accumulator += last_profit` each turn; wire SOE dividend collection (ANNUAL, during `process_political_year`) and patent revenue (physical collection from companies) into `TaxCollectionResult.state_property_revenue`
10. `engine/turn.rs` — Change the Foreign Affairs ministry turn closure from `for_each` to `map(...).flatten().collect()` to gather `Vec<DiplomaticAction>` returns from each task. After the parallel block: drain the collected vector sequentially (debit home treasury, credit host treasury, inject tender into host country). NO Mutex/RwLock.
11. `snapshot.rs` — Add ministry building counts to `GovernmentSnapshot`
12. Tests for each ministry's spending logic

### Step 7: Polish String Purge (Part 4.9)
1. `registries/mod.rs:56` — Rename `"Nadleśnictwo"` → `"StateForest"` in building name registry
2. `registries/production_methods.rs:849-886` — Rename variable `nadrlesnictwo` → `state_forest_methods`, registry key `"Nadleśnictwo"` → `"StateForest"`, production method `"Gospodarka Leśna"` → `"Forestry Management"`, update comments
3. `economy/state_sector/state_forests.rs` — Replace all 9 occurrences of `Nadleśnictwo`/`Lasy Państwowe` with `StateForest`/`State Forests` in comments, variable names (`nadrlesnictwo_buildings` → `state_forest_buildings`), and filter strings (`b.name == "Nadleśnictwo"` → `b.name == "StateForest"`)
4. `io/save_manager.rs` — Add post-load migration: rename existing `"Nadleśnictwo"` buildings to `"StateForest"` on load
5. Update state forest tests to use `"StateForest"` building name

---

## Files to Modify

| File | Changes |
|---|---|
| `state/src/corporate/manager.rs` | Fix zero-wage early exits, skip banks in `set_wage_offers` |
| `state/src/engine/generator/corporate.rs` | Initialize `prev_offered_wage_per_fte` for new companies |
| `state/src/engine/generator/mod.rs` | Set baseline wealth/capital gains brackets in `build_tax_rates` |
| `state/src/state/banking.rs` | Credit interest income to bank cash each turn |
| `state/src/state/tax.rs` | Add `customs_revenue`, `state_property_revenue`; audit wealth/capital-gains double-entry |
| `state/src/economy/trade/b2b_orders.rs` | Verify tariff double-entry (debit buyer, credit treasury, evaded if no cash) |
| `state/src/politics/turn.rs` | Extract snap election + election check; add `apply_ideology_tax_policy`; annual SOE dividends |
| `state/src/politics/ideology.rs` | Add tax policy preferences to `IdeologyPreferences` |
| `state/src/politics/ministries.rs` | Fix `resolve_minister_name`; add `StateAssets`; implement all 10 ministry logics |
| `state/src/politics/system.rs` | Add `last_snap_election_turn` to `Politics` |
| `state/src/construction/projects.rs` | Add 9 new `ConstructionProjectType` variants |
| `state/src/registries/enums.rs` | Add new `BuildingOutput` variants (ResearchOutput, ProsecutionCapacity, CulturalOutput) |
| `state/src/construction/bom.rs` | Add BOMs for 9 new building types |
| `state/src/entities/mod.rs` | Add `annual_profit_accumulator: f64` to `Company` for annual SOE dividends |
| `state/src/state/mod.rs` | Add `pending_diplomatic_actions: Vec<DiplomaticAction>` to `GameState` (deferred queue) |
| `state/src/engine/turn.rs` | Call snap election every turn; wire SOE dividends (annual); drain diplomatic queue after par_iter_mut |
| `state/src/registries/mod.rs` | Rename `"Nadleśnictwo"` → `"StateForest"` in building name registry |
| `state/src/registries/production_methods.rs` | Rename `nadrlesnictwo` → `state_forest_methods`, `"Nadleśnictwo"` → `"StateForest"`, `"Gospodarka Leśna"` → `"Forestry Management"` |
| `state/src/economy/state_sector/state_forests.rs` | Purge all 9 Polish string references (`Nadleśnictwo`, `Lasy Państwowe`) → English (`StateForest`, `State Forests`) |
| `state/src/io/save_manager.rs` | Add post-load migration: rename `"Nadleśnictwo"` buildings → `"StateForest"` |
| `state/src/ui/snapshot.rs` | Add customs/state property revenue to `FinanceSnapshot` |
| `state/src/ui/tui/render.rs` | Add Customs and State Property rows to Finance tab |

---

## Verification

- `cargo build` — must compile with 0 errors
- `cargo test --lib -- --test-threads=1 --nocapture` — all tests must pass
- New tests:
  - Zero-cash company wage doesn't drop below sticky floor
  - New company wage doesn't dilute sector average
  - Bank wage not overridden by `set_wage_offers`
  - Bank interest income credited to cash
  - Wealth tax brackets configured by ideology
  - Capital gains tax brackets configured by ideology
  - Customs revenue tracked in `TaxCollectionResult`
  - State property revenue tracked in `TaxCollectionResult`
  - Snap election fires outside year boundary
  - Snap election cooldown prevents infinite loop
  - `resolve_minister_name` generates valid name for empty party_id
  - Each new ministry competency produces concrete effects

---

## Risks & Considerations

1. **Borrow checker:** The snap election extraction must avoid borrowing `country.politics` mutably while also reading `country.politics.active_parties`. Clone the parties vector before mutation.

2. **Performance:** Running election checks every turn adds overhead. The `check_snap_election` function is lightweight (2 field checks), but `run_election_if_due` involves seat calculation and coalition building. Only run `run_election_if_due` when `years_to_elections == 0` (rare).

3. **Save compatibility:** New `GovernmentCompetency::StateAssets` variant must have a serde default. New `ConstructionProjectType` variants must be handled in all match arms. New `BuildingOutput` variants must have defaults.

4. **Double-entry integrity (STRICT — user correction):** ALL tax and revenue flows must physically debit the payer's **LIQUID CASH** and credit the treasury. No magical counters:
   - SOE dividends: debit company cash, credit treasury (ANNUAL only, not per-turn).
   - Patent revenue: **physically iterate** over active companies, debit each company's liquid cash, credit only successfully collected amounts to treasury. Broke companies → State gets nothing. No `patent_count * fee` magic math.
   - Customs/tariffs: debit buyer cash, credit treasury (already done in `settle_trades_with_tariffs`).
   - Wealth tax: debit `ClassDemographics.savings` (LIQUID only), credit treasury. If insufficient → `taxes_evaded`.
   - Capital gains tax: debit entity LIQUID cash (`available_cash`, `brokerage_account.cash`, or `ClassDemographics.savings`) — **NEVER `PrivateCapital`** (illiquid abstract aggregate). If insufficient → `taxes_evaded`.
   - If any payer has no liquid cash, the tax is recorded as evaded — NOT collected from thin air.

5. **SOE Dividend timing (STRICT — user correction):** Dividends are extracted ONCE per year during `process_political_year`, based on accumulated annual profit (`annual_profit_accumulator`). Per-turn extraction would destroy company liquidity. If the company can't pay the full dividend, the unpaid portion carries as `dividend_arrears`.

6. **Ministry budget sufficiency:** New ministry logic must check `ministry_cash >= cost` before spending. No deficit spending — if a ministry can't afford a tender, it waits.

7. **Embassy cross-country safety (STRICT — user correction):** The engine uses Rayon `tasks.par_iter_mut()` — mutating Country B from Country A's thread is a borrow-checker violation. ALL cross-country embassy operations MUST go through a **deferred event queue**. Do NOT use `Mutex`, `RwLock`, or any interior mutability — use Rayon's **functional pattern**: the turn closure returns `Vec<DiplomaticAction>`, and the engine uses `.map(...).flatten().collect::<Vec<_>>()` to gather actions across all threads without locking. The collected vector is then drained sequentially after the parallel block.

8. **Public Works wage leak (STRICT — user correction):** The Labor Ministry MUST transfer funds from `ministry_cash` to `country.ministry_public_service_pool` before the State Employer processes PublicWorksSite payroll. Without this transfer, the State Employer drains `liquid_reserves` directly, bypassing the Ministry's budget cap. If the pool is insufficient, wages are pro-rated — no deficit spending from reserves.

9. **Patent revenue — no magic math (STRICT — user correction):** Patent revenue must NOT be computed as `patent_count * licensing_fee`. The engine must physically iterate over active companies, attempt to debit each company's liquid cash, and credit only successfully collected amounts to treasury. Broke companies → State gets nothing. No phantom revenue.

10. **Turn 1 wage initialization (STRICT — user correction):** `(market_average_wage * 0.8).max(50.0)` — the `.max(50.0)` hard fallback is mandatory because `market_average_wage` is `0.0` on Turn 1, which would recreate the zero-wage bug without the fallback.

11. **Ideology tax policy (user-approved):** Tax brackets are applied **every turn** from `apply_ideology_tax_policy`, overriding manual adjustments. This ensures ideological consistency — a Socialist government always enforces wealth taxes, a Neoliberal government never does. The player's agency is expressed through elections, not manual tax rate sliders.

12. **Culture & Transport (user correction):** These ministries were initially omitted. Culture funds `NationalTheater`/`NationalLibrary` construction and library subsidies. Transport subsidizes private transport companies and creates state-owned public transport entities in logistics gap regions. Both must be implemented with the same double-entry rigor as other ministries.

13. **Polish string purge (STRICT — user correction):** No Polish words in source code. All `Nadleśnictwo`/`Lasy Państwowe`/`Gospodarka Leśna` references must be renamed to English (`StateForest`/`State Forests`/`Forestry Management`). This is a breaking change for existing saves — the save loader must include a migration step to rename old building names on load.
