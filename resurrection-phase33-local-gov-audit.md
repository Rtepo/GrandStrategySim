# Phase 33 — Corporate Panic, Ministry Effectiveness, Local Governments & UI Audit

**Status:** Read-only audit and implementation blueprint.
**Date:** Phase 33 planning.
**Rule:** No magical money printing. All flows must be double-entry consistent.

---

## Executive Summary

A 24+ turn simulation after Phase 32 revealed four classes of defects:

1. **UI/Naming:** Ministers and Speakers display as `(unnamed)` because party leaders are never assigned names. Ministry names are hardcoded Polish strings. The commodity ToT % column is hardcoded to `0.0` (a leftover TODO).
2. **Permanent Emergency Loop:** `check_emergency_conditions` overwrites `country.emergency_powers` every turn based on `liquid_reserves / gdp`. When the treasury is chronically depleted (which it is, because Ministry subsidies and B2B procurement drain it), the country is stuck in `MartialLaw` forever, which then escalates to a political State of Emergency.
3. **Ministry→GDP Disconnect:** Ministry B2B procurement *does* place bids in the order book and *does* accumulate into `gdp_acc.government_spending` — but only for matched trades. Subsidies, infrastructure funding, public-service wages, and direct transfers do **not** flow into `G`. Meanwhile `I` (investment) is zero because companies are bankrupting before they can invest, and `NX` is zero because the global trade layer (Phase 30) collapsed domestic production.
4. **Corporate Panic & Ghost Sectors:** Companies in the first 3 turns have no `brokerage_account` (except banks), so the labor market clamps their `max_affordable_fte` to 0, they fulfill 0 FTE, produce nothing, take losses, and bankrupt. NGO/Religion companies start with `available_cash: 0.0` and `brokerage_account: None`, so they can never hire even after donations arrive (the donation→brokerage fix at `engine/turn.rs:476` only runs for cultural buildings with `owner_company_id`). Banks have `operating_cash` but no `brokerage_account`, so they also hire 0 workers.

The Local Government architecture (`politics/local_government.rs`) already exists with `RegionalGovernance`, `RegionalBudget`, `RegionalDebt`, `MegaregionGovernance`, and `LocalCouncil` — but `region.governance` is **never initialized** (always `None`), so `process_regional_taxes` and `process_fiscal_transfers` silently no-op.

### Architectural Corrections Applied (from user review)

The following four strict rules override the initial draft and are enforced throughout this blueprint:

1. **NO Seed Grants for NGO/Religion** (Phase 28 rule): NGO and Religion companies start with `0.0` cash. They rely organically on `collect_cultural_donations`. No Treasury seed grants.
2. **State Employer Unification**: Ministry `PublicServiceWages` must route through the existing State Employer pseudo-company, not a parallel abstracted wage→G path.
3. **Investment Requires Real Construction**: `I` only increases when `ConstructionTender`s execute or fixed assets are purchased. No abstract cash→I addition.
4. **Local Governments Do Not Fragment the Labor Market**: `PublicServices` wages remain handled by the State Employer. Local governments collect taxes, process transfers, and publish Regional `ConstructionTenders` / B2C subsidies. No direct wage payments.

---

## PART 1: UI, Naming & Polish Strings Purge

### 1.1 VIP Names — Root Cause

**Finding:** Party leaders are never assigned names.

In `politics/turn.rs:432`, `regenerate_parties()` creates new parties with:
```rust
let mut party = Party {
    ideology: ideo.as_str().to_string(),
    ...
    ..Party::default()   // ← leader = Leader::default() → name = ""
};
```

`Leader::default()` (system.rs:79) has `name: String::default()` = `""`.

In `politics/ministries.rs:382`, minister names are pulled from party leaders:
```rust
let minister_name = active_parties
    .get(&pm_party)
    .map(|p| p.leader.name.clone())
    .unwrap_or_default();   // ← "" when leader.name is empty
```

The snapshot (snapshot.rs:619) then shows `(unnamed)` when `minister_name.is_empty()`.

`parliament.rs:420` has a fallback to `generate_full_vip()` when the party leader name is empty, so Speakers *should* get names — but only if `party` is `Some`. If the ruling party has an empty leader name, the Speaker gets a generated name, but the PM and Ministers still show `(unnamed)`.

**Fix Plan:**
1. In `regenerate_parties()` (turn.rs), when creating a **new** party, assign a generated leader:
   ```rust
   let leader = crate::politics::names::generate_full_vip(cultural_group, &mut rng);
   // Convert VipName → Leader with sensible defaults
   ```
   Build a helper `fn vip_to_leader(vip: VipName, country: &Country) -> Leader` that populates `name`, `gender`, `age`, `religion`, `nationality`, and ideology-appropriate `views`/`traits`.
2. When **preserving** an existing party (turn.rs:416 `updated = party.clone()`), keep the existing leader — do NOT regenerate.
3. In `ministries.rs`, add a fallback: if `minister_name` is empty after the `active_parties.get(...)` lookup, call `generate_full_vip()` and store the result in the party's `leader` field so it persists.

### 1.2 Polish Strings Purge

**Finding:** Polish strings are scattered across the politics module.

| File | Lines | Polish Strings |
|------|-------|----------------|
| `ministries.rs:490-508` | 19 | `competency_display_name()` — all ministry names |
| `turn.rs:682-693` | 6 | Head of state titles: `Król`, `Królowa`, `Prezydent`, `Przywódca`, `Dwór Królewski`, `Kancelaria Prezydencka`, `Rada Władzy` |
| `turn.rs:458` | 1 | `"Tymczasowy Rząd Technokratyczny"` (fallback party name) |
| `elections.rs:192` | 1 | `"Tymczasowy Rząd Technokratyczny"` (fallback party name) |
| `turn.rs:706-712` | 5 | Leader fields: `health: "Dobra"`, `views: "Republikańskie"/"Konserwatywne"`, `traits: ["Charyzmatyczny", "Dyplomatyczny"]`, `main_trait: "Praworządność"` |

**Fix Plan:**
1. `competency_display_name()` → English names: `"Ministry of Energy"`, `"Ministry of Defense"`, `"Ministry of Treasury"`, etc.
2. `random_head_of_state()` → English titles: `"King"`, `"Queen"`, `"President"`, `"Leader"`, `"Royal Court"`, `"Presidential Chancellery"`, `"Council of Authority"`.
3. Fallback party name → `"Provisional Technocratic Government"`.
4. Leader fields → English: `health: "Good"`, `views: "Republican"/"Conservative"`, `traits: ["Charismatic", "Diplomatic"]`, `main_trait: "Lawfulness"`.
5. Audit `politics/` for any remaining Polish display strings via `grep -rn "Ministerstwo\|Prezydent\|Król\|Przywódca\|Tymczasowy\|Dobra\|Republikańskie\|Konserwatywne\|Charyzmatyczny\|Dyplomatyczny\|Praworządność"` and replace.

**Note:** Internal serde field rename attributes (e.g., `#[serde(rename = "lider")]`) must remain unchanged — they are for save-file compatibility, not display.

### 1.3 ToT % Calculation — Root Cause

**Finding:** The commodity-level ToT % is hardcoded to `0.0`.

In `ui/snapshot.rs:358`:
```rust
let tot_balance_change = 0.0; // TODO: track historical net_surplus
```

This is a leftover TODO from Phase 27. The `CommodityRow.tot_balance_change` field exists but is never populated with real data.

The **macro-level** ToT deltas (GDP, CPI, PPI, M3, unemployment, shadow GDP, corruption, population, wage) DO work correctly via `compute_deltas()` (snapshot.rs:844) which reads from `TelemetryHistory`. The telemetry history is properly pushed at `engine/turn.rs:3729` and saved/loaded via serde.

The **sector-level** ToT (employment, wage per sector) also works via `_prev_employment`/`_prev_avg_wage` extra fields stored at `engine/turn.rs:3208`.

**Fix Plan:**
1. Add a `prev_net_surplus: HashMap<Commodity, f64>` field to `TelemetryHistory` (or a separate `CommodityHistory` struct) that stores the previous turn's net surplus per commodity.
2. In `engine/turn.rs`, after market clearing, store the current turn's net surplus per commodity into this history.
3. In `ui/snapshot.rs:358`, read the previous turn's net surplus and compute the real ToT % delta:
   ```rust
   let tot_balance_change = prev_surplus
       .get(&commodity)
       .map(|prev| if *prev != 0.0 { (net_surplus - prev) / prev.abs() * 100.0 } else { 0.0 })
       .unwrap_or(0.0);
   ```
4. Alternatively (simpler): store `_prev_net_surplus` in `country.budget.extra` or `country.macro_indicators.extra` as a JSON map, similar to how `_prev_employment` is stored per-sector.

---

## PART 2: Permanent Emergency & Ministry Disconnect

### 2.1 Permanent Emergency Loop — Root Cause

**Finding:** `check_emergency_conditions` (treasury.rs:27) unconditionally overwrites `country.emergency_powers` every turn:

```rust
country.emergency_powers = new_powers;
```

The threshold for `MartialLaw` is `liquid_reserves / gdp < -0.8` or `> 7 critical shortages`. When the treasury is chronically negative (which it is, because Ministries drain it via subsidies and procurement), this condition is permanently true.

Then in `crisis_management.rs:130`:
```rust
if country.emergency_powers == EmergencyPowers::MartialLaw {
    return Some(("Fiscal martial law escalated to political State of Emergency", true));
}
```

This creates a political State of Emergency every turn, which suspends Parliament, which prevents legislation, which prevents fiscal reform, which keeps the treasury negative — a death loop.

**Fix Plan:**
1. **Hysteresis:** Add a `turns_in_emergency: u32` counter to `EmergencyPowers` state (or track on `Country`). Only *escalate* (Normal → Excise → Rationing → MartialLaw) when the condition has persisted for ≥ 2 turns. Only *de-escalate* when the condition has cleared for ≥ 3 turns. This prevents flickering.
2. **SoE cooldown:** After a political State of Emergency expires (`turns_remaining` hits 0), impose a `cooldown_turns: u32` (e.g., 12 turns = 6 months) during which a new SoE cannot be activated unless severity is *catastrophic* (severity > 0.9). This prevents the immediate re-activation seen in Phase 31 tests.
3. **Fiscal SoE ≠ Political SoE:** The fiscal `EmergencyPowers` (treasury.rs) and the political `StateOfEmergency` (politics/parliament.rs) must be decoupled. Fiscal MartialLaw should NOT automatically escalate to a political State of Emergency. Instead, fiscal MartialLaw should enable rationing/excise taxes, and only a *separate* parliamentary vote (or a very high severity threshold > 0.85) should trigger a political SoE.
4. **Threshold review:** The current threshold `liquid_reserves / gdp < -0.8` means reserves must be at -80% of GDP. This is extremely deep. Consider whether this is realistic. A more standard threshold would be `liquid_reserves < 0` (insolvency) for MartialLaw, with earlier stages at `liquid_reserves < 0.5 * monthly_revenue`.

### 2.2 Ministry Spending → GDP Disconnect

**Finding:** Ministry spending partially flows into GDP, but most of it is a black hole.

| Spending Action | Flows into `G`? | Where it goes |
|----------------|-----------------|---------------|
| `B2BProcurementOrder` | ✅ Yes (turn.rs:1040) | Bid → order book → matched trade → `gdp_acc.government_spending += qty * price` |
| `Subsidy` | ❌ No | Debits treasury, credits company `available_cash`. Not counted in `G`. |
| `InfrastructureFunding` | ❌ No | Debits treasury, credits building reserve. Not counted in `G`. |
| `PublicServiceWages` | ❌ No | Debits treasury, pays building workers. Not counted in `G` (unless the state-employer pseudo-company path at turn.rs:1856 catches it, which it doesn't for ministry-paid wages). |
| `DirectTransfer` | ❌ No | Debits treasury, credits class savings. Not counted in `G`. |
| `TransferToLocalGov` | ❌ No | Debits treasury, credits regional budget. Not counted in `G`. |
| `RAndDGrant` | ❌ No | Debits treasury, credits company. Not counted in `G`. |

**The core problem:** In national accounting (SNA 2008), `G` (government consumption expenditure) includes:
- Government purchases of goods and services (B2B procurement ✅)
- Government employee compensation (public-service wages ❌ not counted)
- Government fixed capital formation (infrastructure funding ❌ not counted)

Subsidies are **not** part of `G` — they are transfer payments (treated as negative taxes). Direct transfers to households are also not `G`. R&D grants are intermediate consumption if to government labs, or subsidies if to private firms.

**Fix Plan:**
1. **Subsidies → NOT in GDP:** Subsidies are transfer payments, not final consumption. They should NOT be added to `G`. They flow into the economy when the recipient company spends them on wages/goods (which then shows up in `C` or `I`).
2. **DirectTransfer → NOT in GDP:** Same as subsidies — transfer payments are not final consumption.
3. **TransferToLocalGov → NOT in GDP:** This is an internal government transfer. It becomes `G` when the local government spends it.
4. **RAndDGrant → NOT in GDP:** Treated as subsidy (not in `G`).

> **ARCHITECTURAL CORRECTION #2 (from user review):**
> **State Employer Unification — No Parallel Wage Payments.**
> The `State Employer` pseudo-company (engine/turn.rs:1856) was built in Phase 28
> specifically to handle public-sector wages on the labor market and route them to `G`.
> Ministry `PublicServiceWages` must NOT be abstractly added to `gdp_acc.government_spending`.
> Instead, Ministry funding for public services must be **routed through or synced with**
> the existing `State Employer` pseudo-company mechanic. The Ministry allocates cash
> to the State Employer's payroll budget; the State Employer hires workers on the labor
> market; the State Employer's wage payments are already accumulated into `G` at turn.rs:1864.
> Do not create parallel, abstracted wage payment paths.

> **ARCHITECTURAL CORRECTION #3 (from user review):**
> **Investment (I) Requires Real Construction — No Abstract Cash→I.**
> Spending cash is not investment until physical materials are used.
> `I` (Investment) must ONLY increase when:
> - A `ConstructionTender` is successfully executed (materials consumed, fixed asset created), or
> - A fixed asset is purchased (B2B procurement of capital goods).
> Do NOT abstractly add `InfrastructureFunding.amount` to `gdp_acc.investment`.
> Ministry `InfrastructureFunding` should instead **launch or fund `ConstructionTender`s**
> that flow through the real construction system. The `I` accumulator captures investment
> only when those tenders complete and physical assets are built.

**Revised Implementation:**
- `PublicServiceWages`: Route ministry budget to the State Employer pseudo-company's payroll. The State Employer already debits treasury and accumulates `G` (turn.rs:1862-1864). No new GDP accounting path needed.
- `InfrastructureFunding`: Route ministry budget to `ConstructionTender` creation. `I` is accumulated when the tender executes (existing construction settlement logic). No abstract `I` addition.
- `B2BProcurementOrder`: Already correctly flows into `G` (turn.rs:1040). No change.
- `Subsidy`, `DirectTransfer`, `TransferToLocalGov`, `RAndDGrant`: Not in GDP. No change.

### 2.3 Ministry Competencies & Physical Buildings

**Finding:** Ministries do NOT diagnose infrastructure gaps or launch construction tenders.

The `InfrastructureFunding` action (ministries.rs:725) credits a building's reserve fund, but it does NOT:
- Identify missing transport links
- Create `ConstructionTenders`
- Fund actual construction projects

The `GovernmentCompetency::Infrastructure` ministry just picks existing buildings and gives them cash. There is no link to the construction system (`construction/tenders.rs`).

**Fix Plan:**
1. Add a `MinistryInfrastructurePlanner` function that, for the Infrastructure/Transport ministry, scans `country.transport_networks.links` for missing or degraded links and creates `ConstructionTender` entries funded by the ministry budget.
2. Add a `MinistryDefensePlanner` that procures military goods (Commodity::Weapons, Commodity::Ammunition) via B2B orders from defense-sector companies.
3. Add a `MinistryEducationPlanner` that funds school buildings (identified by `BuildingKind::School`) via `PublicServiceWages`.
4. These planners should be called during the ministry spending phase, before B2B order placement.

**Scope note:** This is a large feature. For Phase 33, implement the GDP accounting fix (2.2) first, and add one planner (Infrastructure) as a proof-of-concept. The remaining planners can be Phase 34.

---

## PART 3: Corporate Panic & The "Syndic" (Bankruptcy) Flow

### 3.1 Corporate Panic — Root Cause

**Finding:** Companies cannot hire workers in the first turn because they lack `brokerage_account`.

The labor market (labor_market.rs:197-204) computes:
```rust
let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
    company.brokerage_account
        .as_ref()
        .map(|ba| ba.cash / company.offered_wage_per_fte)
        .unwrap_or(0.0)   // ← 0 if no brokerage account
} else {
    0.0
};
let clamped_demand = company.target_fte_demand.min(max_affordable_fte);
```

If `brokerage_account` is `None`, `max_affordable_fte = 0`, so `clamped_demand = 0`, so the company bids for 0 workers, fulfills 0 FTE, produces nothing, and takes a loss.

**Who has a brokerage account at startup?**
- Banks: NO (generator/mod.rs:916 creates Company without brokerage_account)
- NGO/Religion: NO (generator/corporate.rs:2702, `brokerage_account: None`)
- Regular companies: Only if loaded from save with `rachunek_maklerski` field (entities/mod.rs:663)

**The result:** In turn 1, almost no company can hire. Production is zero. Companies take losses (overhead, depreciation). By turn 3, `consecutive_losses` triggers bankruptcy (lifecycle.rs:96). Mass bankruptcies follow. The Syndic (bankruptcy.rs:210) liquidates assets, fires all workers, and the economy collapses.

**Fix Plan:**
1. **Initialize brokerage accounts at company creation:** In `engine/generator/mod.rs` and `engine/generator/corporate.rs`, when creating any company, initialize `brokerage_account` with the company's initial cash:
   ```rust
   brokerage_account: Some(BrokerageAccount {
       cash: initial_operating_cash,
       ..Default::default()
   })
   ```
2. **For banks:** Set `brokerage_account.cash = operating_cash` (10% of tier_1_capital).
3. **For NGO/Religion:** Set `brokerage_account.cash = 0.0` (they start with no cash, but the account exists so donations can flow in). The donation transfer at turn.rs:476 already creates the account if missing — but only for cultural buildings with `owner_company_id`. Ensure ALL charity companies get a brokerage account at generation time.
4. **For regular companies:** Set `brokerage_account.cash = available_cash` (their initial working capital).
5. **Fallback in labor market:** As a safety net, if `brokerage_account` is `None`, fall back to `company.available_cash` instead of `0.0`:
   ```rust
   let available = company.brokerage_account
       .as_ref()
       .map(|ba| ba.cash)
       .unwrap_or(company.available_cash);
   let max_affordable_fte = if company.offered_wage_per_fte > 0.0 {
       available / company.offered_wage_per_fte
   } else { 0.0 };
   ```

### 3.2 Syndic / Bankruptcy Flow

**Finding:** The Syndic (bankruptcy.rs:196) works correctly for individual liquidations but is overwhelmed by mass bankruptcy cascades.

The Syndic:
1. Converts foreign currency balances to domestic (line 261)
2. Reclaims frozen cash from escrow (line 300)
3. Pays taxes (line 330)
4. Pays bank loans (line 348)
5. Routes residual to treasury as shareholder equity (line 360)

This is double-entry correct. The problem is not the Syndic itself — it's that too many companies reach bankruptcy simultaneously because they can't hire workers (see 3.1).

**Fix Plan:** Fix the root cause (3.1) and the Syndic will handle the remaining legitimate bankruptcies. No changes needed to the Syndic itself.

**Additional safeguard:** Add a "grace period" for new companies — companies younger than 2 turns (no `financial_history` entries) cannot be liquidated for sustained losses. They can only be liquidated for negative equity. This prevents first-turn mass bankruptcy.

### 3.3 Ghost Sectors (NGO, Religion, Banking) — 0 Employment

**Finding:** Ghost sectors have 0 employment because:
1. No `brokerage_account` → labor market clamps to 0 FTE (see 3.1).
2. NGO/Religion start with `available_cash: 0.0` → even with a brokerage account, they can't hire until donations arrive.
3. Donations arrive via `collect_cultural_donations` (turn.rs:449) → transferred to company (turn.rs:467) → brokerage account created/credited (turn.rs:473-480). But this only happens for cultural buildings with `owner_company_id`. If a charity company has no associated cultural building, it never receives donations.
4. Banks have `operating_cash` (10% of tier_1) but no brokerage account, so they can't hire tellers/loan officers.

**Fix Plan:**
1. Initialize `brokerage_account` for ALL companies at generation time (see 3.1).
2. For banks: `brokerage_account.cash = operating_cash`.
3. For NGO/Religion: `brokerage_account.cash = 0.0` (account exists, will be funded organically by donations).
4. Ensure the donation collection loop covers ALL charity companies, not just those with cultural buildings. Add a fallback: if a charity company has no cultural building, it can still receive direct donations from class savings (a simplified charity collection pass).

> **ARCHITECTURAL CORRECTION #1 (from user review):**
> **NO Seed Grants for NGO/Religion.** This enforces the Phase 28 rule.
> NGO and Religion companies MUST start with `0.0` cash and `brokerage_account.cash = 0.0`.
> They rely **organically** on `collect_cultural_donations` for all funding.
> Do NOT give them Treasury seed grants, initial cash, or any form of magical startup capital.
> They will hire workers *only* after donations naturally flow into their brokerage account.
> The empty `brokerage_account` exists solely so the labor market does not clamp them
> to zero — once donations arrive, they can bid for workers on equal footing.

---

## PART 4: Local Governments (Samorządy)

### 4.1 Current State

**Finding:** The Local Government architecture exists but is completely dormant.

`politics/local_government.rs` defines:
- `RegionalGovernance` — per-region government with head, council, budget, debt
- `RegionalBudget` — liquid_reserves, tax_revenue, property_tax, local_fees, central_grants, transfers, expenditures, debt_service
- `RegionalDebt` — total_debt, municipal_bonds, debt_to_revenue_ratio, credit_rating
- `MegaregionGovernance` — optional intermediate layer
- `LocalCouncil` (in `local_council.rs`) — curial factions, seat allocation

`society/geography.rs:548` has:
```rust
pub governance: Option<crate::politics::local_government::RegionalGovernance>,
```

But `region.governance` is **never set to `Some(...)`** anywhere in the codebase. It is always `None`.

As a result:
- `process_regional_taxes()` (fiscal_transfers.rs:37) — the `if let Some(governance)` branch never executes
- `process_fiscal_transfers()` (fiscal_transfers.rs:138) — the `let Some(governance)` guard returns early
- `process_municipal_debt_service()` (fiscal_transfers.rs:210) — same
- `process_local_elections()` — same

The entire local government system is dead code.

### 4.2 Architecture Plan

**Fix Plan — Initialize and activate Local Governments:**

1. **Initialization:** In `engine/generator/`, when generating a country, initialize `region.governance = Some(RegionalGovernance { ... })` for each region:
   - `id`: region ID
   - `head_type`: `Mayor` (democratic) or `Wójt` (authoritarian) based on `government_form`
   - `head`: Generate a local leader via `generate_full_vip()` + `vip_to_leader()`
   - `council`: Initialize `LocalCouncil` with seat count based on region population
   - `budget`: Initialize with a small seed grant from the central treasury (debit central, credit regional — double-entry)
   - `debt`: Empty
   - `admin_status`: `Normal`
   - `last_election_year`: start_year
   - `years_to_next_election`: 4

2. **Tax Collection:** `process_regional_taxes()` already calculates property tax and local fees. Once `governance` is `Some`, these will flow into `governance.budget.tax_revenue` and `governance.budget.liquid_reserves`. The tax deduction from class savings (`deduct_taxes_from_classes`) already works.

3. **Fiscal Transfers:** `process_fiscal_transfers()` splits regional revenue into local share, megaregion share, and central share based on `FiscalTransferConfig`. Once `governance` is `Some`, this will execute. The central share flows back to `country.budget.liquid_reserves`.

4. **Local Services (REVISED — see correction #4 below):** Regional governments do NOT directly pay wages for `PublicServices` buildings. `PublicServices` are managed by the `State Employer` pseudo-company on the labor market. Instead, regional governments use their retained budget to:
   - Publish Regional `ConstructionTenders` (e.g., local infrastructure — roads, bridges, local civic buildings)
   - Provide local B2C subsidies (e.g., subsidizing local service fees for residents)
   - Fund maintenance of regional infrastructure (transport links, civic buildings) via `InfrastructureFunding` to building reserve funds
   - These expenditures flow into GDP organically through the construction system (`I`) or B2C market (`C`), not through abstract wage payments.

> **ARCHITECTURAL CORRECTION #4 (from user review):**
> **Local Governments Must Not Fragment the Labor Market.**
> `PublicServices` buildings are managed by the `State Employer` pseudo-company on the
> labor market. Do NOT have Regional Governments directly pay building wages.
> For Phase 33, Local Governments are activated to:
> - Collect property taxes and local fees (DEBIT class savings → CREDIT regional budget)
> - Process fiscal transfers to the central Treasury (DEBIT regional → CREDIT central)
> - Use retained budget to publish Regional `ConstructionTenders` (local infrastructure)
> - Provide local B2C subsidies (subsidizing service fees for residents)
> Direct wage payment for PublicServices buildings is explicitly deferred.

5. **Local Elections:** `process_local_elections()` already exists. Once `governance` is initialized, local elections will run on their own cycle (every 4 years). The local council composition shifts based on regional demographics.

6. **Commissary Administration:** `check_commissary_administration()` already exists — if a region's debt-to-revenue ratio exceeds the threshold, the central government can dissolve the local council and appoint a commissary. This will now actually trigger.

7. **Municipal Bonds:** `process_municipal_debt_service()` already exists. Regional governments can issue municipal bonds (debt) to fund infrastructure. Bondholders are paid from `governance.budget.liquid_reserves`. If the region defaults, `admin_status` changes to `CommissaryAdministration`.

8. **GDP Impact (REVISED):** Local government spending flows into GDP organically:
   - Regional `ConstructionTenders` → `I` when tenders execute (via existing construction settlement)
   - Local B2C subsidies → `C` when subsidized services are consumed (via existing B2C clearing)
   - Do NOT abstractly add `governance.budget.local_expenditures` to `gdp_acc.government_spending`.
   - The only direct `G` contribution from local government is via the State Employer pseudo-company, which remains unified.

**Double-Entry Verification:**
- Tax collection: DEBIT class savings → CREDIT `governance.budget.liquid_reserves`
- Central grant: DEBIT `country.budget.liquid_reserves` → CREDIT `governance.budget.liquid_reserves`
- Central transfer: DEBIT `governance.budget.liquid_reserves` → CREDIT `country.budget.liquid_reserves`
- Regional ConstructionTender: DEBIT `governance.budget.liquid_reserves` → CREDIT construction company (via tender settlement) → `I` accumulator
- Local B2C subsidy: DEBIT `governance.budget.liquid_reserves` → CREDIT service provider (via B2C clearing) → `C` accumulator
- Municipal bond issue: DEBIT bondholder cash → CREDIT `governance.budget.liquid_reserves` + `governance.debt.total_debt`
- Debt service: DEBIT `governance.budget.liquid_reserves` → CREDIT bondholder cash
- **NOT included:** Direct wage payment to PublicServices workers (deferred — State Employer handles this)

---

## Implementation Steps (Ordered)

### Step 1: Fix Corporate Panic (Part 3) — HIGHEST PRIORITY
This is the root cause of the 300% shadow economy and GDP collapse.

1. **`engine/generator/mod.rs`**: Initialize `brokerage_account` for banks with `operating_cash`.
2. **`engine/generator/corporate.rs`**: Initialize `brokerage_account` for all charity companies with `cash = 0.0` (NO seed grants — Phase 28 rule).
3. **`entities/mod.rs`**: When loading companies from save, if `brokerage_account` is `None`, create one from `available_cash`.
4. **`economy/labor/labor_market.rs`**: Add fallback — if `brokerage_account` is `None`, use `company.available_cash` for `max_affordable_fte`.
5. **`corporate/lifecycle.rs`**: Add 2-turn grace period for new companies (no liquidation for sustained losses if `financial_history.len() < 2`).

### Step 2: Fix Permanent Emergency Loop (Part 2.1)

1. **`government/treasury.rs`**: Add hysteresis — track `turns_in_emergency` and require 2+ turns before escalating, 3+ turns of recovery before de-escalating.
2. **`politics/crisis_management.rs`**: Decouple fiscal MartialLaw from political SoE. Fiscal MartialLaw enables rationing/excise but does NOT auto-escalate to political SoE. Only severity > 0.85 triggers political SoE.
3. **`politics/parliament.rs`**: Add `cooldown_turns` to `StateOfEmergency`. After expiry, impose 12-turn cooldown before reactivation (unless severity > 0.9).

### Step 3: Fix Ministry → GDP Flow (Part 2.2) — REVISED per corrections #2 and #3

1. **`politics/ministries.rs`**: Route `PublicServiceWages` budget to the State Employer pseudo-company's payroll (correction #2 — no parallel wage payments). Route `InfrastructureFunding` budget to `ConstructionTender` creation (correction #3 — no abstract `I` addition).
2. **`engine/turn.rs`**: Ensure the State Employer pseudo-company receives ministry payroll funding before labor market clearing. Ensure ministry infrastructure tenders flow through the existing construction settlement system.
3. **No new GDP accumulator paths.** `G` is already accumulated by the State Employer (turn.rs:1864). `I` is already accumulated by construction settlement. The fix is routing, not accounting.

### Step 4: Fix VIP Names (Part 1.1)

1. **`politics/turn.rs`**: In `regenerate_parties()`, assign generated leaders to new parties via `generate_full_vip()` + `vip_to_leader()` helper.
2. **`politics/names.rs`**: Add `pub fn vip_to_leader(vip: VipName, country: &Country, ideology: &str) -> Leader` helper.
3. **`politics/ministries.rs`**: Add fallback — if `minister_name` is empty, generate one and persist it to the party's `leader` field.

### Step 5: Purge Polish Strings (Part 1.2)

1. **`politics/ministries.rs`**: `competency_display_name()` → English.
2. **`politics/turn.rs`**: Head of state titles → English. Leader fields → English. Fallback party name → English.
3. **`politics/elections.rs`**: Fallback party name → English.
4. Grep and replace any remaining Polish display strings in `politics/`.

### Step 6: Fix ToT % for Commodities (Part 1.3)

1. **`state/macro_data.rs`**: Add `prev_commodity_surplus: HashMap<String, f64>` to `MacroData` (or `TelemetryHistory`).
2. **`engine/turn.rs`**: After market clearing, store current net surplus per commodity.
3. **`ui/snapshot.rs`**: Read previous surplus and compute real ToT %.

### Step 7: Activate Local Governments (Part 4) — REVISED per correction #4

1. **`engine/generator/`**: Initialize `region.governance = Some(RegionalGovernance { ... })` for each region.
2. **`politics/fiscal_transfers.rs`**: Verify `process_regional_taxes`, `process_fiscal_transfers`, `process_municipal_debt_service` work with initialized governance.
3. **`engine/turn.rs`**: Add regional `ConstructionTender` publication from `governance.budget.liquid_reserves` (flows into `I` via construction settlement). Add local B2C subsidy mechanism (flows into `C` via B2C clearing). Do NOT add direct wage payments (correction #4).
4. **`politics/turn.rs`**: Wire local elections and council updates.

### Step 8: Tests

1. Test that companies can hire in turn 1 (brokerage account initialization).
2. Test that NGO/Religion companies start with `brokerage_account.cash = 0.0` (NO seed grant — Phase 28 rule).
3. Test that NGO/Religion companies hire only after donations arrive.
4. Test that banks hire workers.
5. Test that emergency powers have hysteresis (no flickering).
6. Test that political SoE has cooldown.
7. Test that ministry public-service wages flow into `G` via the State Employer pseudo-company (not a parallel path).
8. Test that ministry infrastructure funding creates `ConstructionTender`s (not abstract `I` addition).
9. Test that party leaders have non-empty names.
10. Test that ministers have non-empty names.
11. Test that ministry display names are in English.
12. Test that commodity ToT % is non-zero after turn 2.
13. Test that `region.governance` is initialized.
14. Test that `process_regional_taxes` credits `governance.budget.liquid_reserves`.
15. Test that regional ConstructionTenders debit `governance.budget.liquid_reserves` and flow into `I` via construction settlement.
16. Test that local governments do NOT directly pay PublicServices wages (correction #4).
17. Test double-entry consistency: local tax collection debits class savings, credits regional budget.

---

## Files to Modify

| File | Changes |
|------|---------|
| `engine/generator/mod.rs` | Initialize brokerage_account for banks |
| `engine/generator/corporate.rs` | Initialize brokerage_account for charities (cash=0.0, NO seed grants) |
| `entities/mod.rs` | Fallback brokerage_account creation on load |
| `economy/labor/labor_market.rs` | Fallback to available_cash when no brokerage_account |
| `corporate/lifecycle.rs` | 2-turn grace period for new companies |
| `government/treasury.rs` | Hysteresis for emergency powers |
| `politics/crisis_management.rs` | Decouple fiscal MartialLaw from political SoE |
| `politics/parliament.rs` | SoE cooldown_turns field |
| `politics/ministries.rs` | English display names; minister name fallback; route wages→State Employer, infrastructure→ConstructionTenders |
| `engine/turn.rs` | Sync ministry payroll to State Employer; ministry infrastructure→ConstructionTenders; regional ConstructionTenders; commodity surplus history |
| `politics/turn.rs` | Generate party leaders; English strings; local elections wiring |
| `politics/names.rs` | vip_to_leader() helper |
| `politics/elections.rs` | English fallback party name |
| `state/macro_data.rs` | prev_commodity_surplus field |
| `ui/snapshot.rs` | Real commodity ToT % calculation |
| `politics/fiscal_transfers.rs` | Verify/fix local gov activation |
| `politics/local_government.rs` | Initialization helpers |

---

## Risks & Considerations

1. **Save compatibility:** Adding `brokerage_account` initialization on load must not break existing saves. Use `#[serde(default)]` and fallback logic.
2. **Double-entry audit:** Every new cash flow must be verified as double-entry consistent. No magical money. Specifically: NO NGO seed grants (Phase 28 rule), NO abstract wage→G paths (use State Employer), NO abstract cash→I paths (use ConstructionTenders), NO local gov direct wage payments (use State Employer for PublicServices).
3. **Performance:** Local government initialization adds per-region overhead. With ~10-20 regions per country, this is negligible.
4. **Hysteresis complexity:** The emergency powers hysteresis adds state. Ensure it is saved/loaded correctly.
5. **Ministry GDP fix scope:** Route wages through State Employer and infrastructure through ConstructionTenders. The full infrastructure planner (2.3 — diagnosing missing transport links) is deferred to Phase 34.
6. **Polish string purge scope:** Only purge DISPLAY strings. Serde rename attributes for save compatibility must remain unchanged.
7. **Local government activation risk:** Activating dormant code may reveal latent bugs in `fiscal_transfers.rs`. Test thoroughly.
8. **State Employer synchronization:** Ministry payroll must be routed to the State Employer BEFORE labor market clearing. If the State Employer is created after ministry spending, the funding won't reach the labor market in the same turn. Verify turn ordering.

---

## Verification

- [ ] `cargo build --lib` succeeds
- [ ] `cargo build` (binary) succeeds
- [ ] `cargo test --lib` — all existing tests pass
- [ ] New tests for brokerage account initialization
- [ ] New tests for emergency hysteresis
- [ ] New tests for ministry GDP flow (via State Employer and ConstructionTenders)
- [ ] New tests for party leader names
- [ ] New tests for local government initialization
- [ ] New test: NGO/Religion start with 0.0 cash (no seed grants)
- [ ] New test: Local governments do NOT directly pay PublicServices wages
- [ ] Manual 24-turn simulation: Shadow GDP < 50% of official GDP
- [ ] Manual 24-turn simulation: No permanent State of Emergency
- [ ] Manual 24-turn simulation: `G` > 0 and reflects State Employer payroll (funded by ministries)
- [ ] Manual 24-turn simulation: `I` > 0 (via executed ConstructionTenders, not abstract cash)
- [ ] Manual 24-turn simulation: NGO/Religion/Banking sectors have > 0 employment
- [ ] Manual 24-turn simulation: Ministers and Speakers have real names
- [ ] Manual 24-turn simulation: Ministry names in English
- [ ] Manual 24-turn simulation: Commodity ToT % non-zero
