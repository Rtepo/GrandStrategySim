# Phase 28: Corporate/State Intelligence, Ghost Sectors & Shadow Economy Audit

**Date:** 2025-01-Phase 28
**Status:** Blueprint — Awaiting User Approval
**Prerequisite:** Phase 27 (Calendar fix, Sectors tab, Market filtering, Generator supply chain, Paid inventory)

---

## Executive Summary

Phase 27 succeeded in unfreezing Investment (I) and making the domestic supply chain functional. However, a deep audit of the Turn 24 telemetry and TUI reveals four critical systemic failures:

1. **Ghost Sectors**: Banking, NGO, and Religion sectors have 0 employment and 0 wages because they have no physical buildings and no cash to pay workers.
2. **G = 0.00**: Government Spending is zero because of a `* 100.0` arithmetic bug in `migrate_legacy_budget` that inflates ministry allocations by 100x, causing the treasury to be drained by `allocate_cash_to_ministries` before any actual procurement can happen. Additionally, State buildings (police, military, courts) don't participate in the labor market at all — civil servants and soldiers are never hired or paid.
3. **Corporate AI is a stub**: `CorporateAction::SwitchMethod` is evaluated but never applied (it's a no-op in `manager.rs`). The method-switching logic uses a "dummy_current" method and hardcoded synthetic alternatives instead of querying the real registry. No vertical integration or state intervention AI exists.
4. **Shadow Economy is dead**: `shadow_employment` is initialized to `None` for every company and is never set to `Some(...)` in production code (only in a test). Corruption index starts at 0.0 and can never increase because bribe acceptance probability equals the corruption index, creating a chicken-and-egg deadlock.

---

## PART 1: Ghost Sectors & Missing G (Government Spending)

### 1.1 Banking — No Physical Branches

**File:** `state/src/engine/generator/mod.rs` lines 856–911

**Finding:** `build_bank_companies` creates one bank `Company` per country with `EntitySector::Banking`, a balance sheet, and tier-1 capital. However:
- `building_ids` is never set (defaults to empty `Vec::new()`).
- No `Building` is created for the bank.
- `target_fte_demand` is not set (defaults to 0.0 in `Company::new`).
- `offered_wage_per_fte` is not set (defaults to 0.0).

**Consequence:** The bank company appears in the Sector tab with 0 employment and 0 wage. It cannot hire tellers, loan officers, or branch managers. The labor market filters by `offered_wage_per_fte > 0` and `available_cash > 0`, so the bank is invisible to the labor system.

**Fix Plan:**
- In `build_bank_companies`, after creating the company:
  - Set `target_fte_demand` to a reasonable value (e.g., 50–200 FTE depending on bank size).
  - Set `offered_wage_per_fte` to a competitive wage (e.g., `base_wage * 1.2`).
  - Set `available_cash` from `tier_1_capital * 0.1` (operating cash).
  - Create a `Building` of `Sector::Banking` (bank branch) with `worker_capacity` matching `target_fte_demand`.
  - Add the building ID to `company.building_ids`.
  - Save the building to the spatial registry.
- Alternatively, create a `generate_bank_buildings` function called from `generate_corporate_entities` that spawns one bank building per region (not just one per country).

### 1.2 NGO & Religion — No Buildings, No Cash, Zero Wage

**File:** `state/src/engine/generator/corporate.rs` lines 2508–2646; `state/src/infrastructure/cultural.rs` lines 185–205; `state/src/economy/religion/religious_economy.rs` lines 307–337; `state/src/registries/production_methods.rs` lines 890–997

**Finding:** `generate_charity_entities` creates NGO and Church companies with:
- `building_ids: Vec::new()` — no physical buildings.
- `available_cash: 0.0` — no operating funds.
- `offered_wage_per_fte: 0.0` — no wage offered.
- `liquid_capital: 0.0` — no capital.
- `target_fte_demand: worker_capacity as f64` — demands workers but can't pay them.

**Additionally:** `country.cultural_institutions` is initialized as `Vec::new()` in the generator (`mod.rs` line 268) and **never populated**. No `CulturalBuilding` (Temple, Monastery, CulturalHouse) is ever generated. This means:
- `collect_cultural_donations` (the tithe/donation system) runs every turn but iterates an empty list — it collects zero donations.
- `process_monastery_production` runs every turn but iterates an empty list — it produces zero goods.
- `process_church_fund` (state funding for church maintenance) runs but has no buildings to fund.
- The monastery production methods (`monastery_wine_production`, `monastery_scriptorium`, `monastery_herbal_garden`, `monastery_workshop`) exist in the registry but are never used because no monasteries are spawned.
- The `LatifundiumData` struct exists for monastery-owned agricultural estates, but no monastery ever gets one.

**Consequence:** NGOs, Churches, and Monasteries appear in the Sector tab with 0 employment and 0 wage. The entire religious/cultural economy — tithes, donations, monastery production, latifundium income, church fund — is dead code.

**Fix Plan (Strict Realistic Funding — NO Magical Seed Capital):**

> **ARCHITECTURAL RULE:** Churches, Monasteries, and NGOs do NOT receive magical startup cash. They must be funded organically through:
> 1. **Tithes/Donations** from citizens (via `collect_cultural_donations`).
> 2. **Production revenue** from monastery economic activity (wine, scriptorium, herbal garden, workshop).
> 3. **Latifundium income** from monastery-owned agricultural estates.
> 4. **Church fund** maintenance from the State treasury (via `process_church_fund`).
>
> No money is created from nowhere. All funding flows through existing double-entry mechanics.

**Step 1: Generate `CulturalBuilding` entities at world generation.**
- Create a `generate_cultural_institutions` function called from `generate_corporate_entities`.
- For each region, spawn:
  - **1 Temple/Church** per region (if the country has a dominant religion).
  - **1 Monastery** per region (if the country has a dominant religion, with probability scaling by region ruralness).
  - **1 CulturalHouse** per region (secular cultural center).
- Set `CulturalBuilding.region_id`, `building_type`, `capacity`, `condition: 1.0`.
- For Monasteries, randomly assign a `production_method` from the existing registry methods:
  - `monastery_wine_production` (rural regions with agriculture)
  - `monastery_scriptorium` (urban/civilized regions)
  - `monastery_herbal_garden` (any region)
  - `monastery_workshop` (any region)
- For Monasteries in rural regions, optionally assign a `LatifundiumData` with serf households and hectares (the monastery owns farmland).
- Link each `CulturalBuilding` to its owning Church/Religion company via `owner_company_id`.
- Store in `country.cultural_institutions`.

**Step 2: Wire donation income to Church/NGO companies.**
- `collect_cultural_donations` already debits donor savings and credits `building.available_cash`.
- Add a step after donation collection that transfers `building.available_cash` to the owning company's `available_cash` (so the company can pay wages).
- This is the organic funding mechanism: citizens donate → building collects → company pays workers.
- No magical seed capital. If donations are insufficient, the church/NGO hires fewer workers (labor market naturally clamps by `available_cash`).

**Step 3: Wire monastery production to real commodity output.**
- Replace the flat `100.0 * scale` in `process_monastery_production` with real production method lookup:
  - Query the registry for the building's `production_method` key.
  - Compute inputs consumed and outputs produced (like normal B2B production).
  - Place outputs in a building inventory for B2B sell orders.
  - Credit revenue to the owning company via `credit_company_by_id`.
- Monasteries that produce wine, scriptorium texts, or herbal medicines sell these on the B2B market and earn revenue.
- This revenue funds the monastery's workers — no magic money.

**Step 4: Wire Church Fund (State subsidy for church maintenance).**
- `process_church_fund` already exists and debits `country.budget.liquid_reserves` to fund church maintenance.
- This is a legitimate State expenditure (double-entry: treasury debited, church company credited).
- It should also accumulate to `task.gdp_acc.government_spending` since it's government spending on religious services.

**Step 5: Set Church/NGO company wages based on available funds.**
- After donation collection and production revenue, set `offered_wage_per_fte` based on `available_cash / target_fte_demand`.
- If `available_cash = 0`, the church offers 0 wage and hires nobody (realistic — a church with no congregation donations can't hire staff).
- If donations flow, the church can hire workers at a subsistence wage.
- The labor market naturally clamps hiring to what the church can afford.

**Step 6: Create physical buildings for Church/NGO companies.**
- Each Church/Religion company gets a `Building` (church building, monastery building) with `Sector::Religion`.
- Each NGO company gets a `Building` (office) with `Sector::NGO`.
- Add building IDs to `company.building_ids`.
- Save buildings to the spatial registry.
- These buildings have `worker_capacity` matching the company's `target_fte_demand`.

### 1.3 State Buildings — Not in Labor Market

**File:** `state/src/engine/turn.rs` lines 1594–1603; `state/src/economy/labor/labor_market.rs` lines 161–220

**Finding:** State buildings (Military Bases, Police Stations, Courts, Landfills) are generated with `owner_id = "State"` and have `current_employment` and `worker_capacity` fields. However:
- `resolve_regional_labor_market` only receives `&mut task.companies` — it never sees buildings.
- State buildings are not companies and don't submit `LaborBid`s.
- Civil servants, police officers, and soldiers are never hired through the labor market.
- Their `current_employment` is set at generation time and never updated.
- They don't pay wages, so they don't contribute to `G` (Government Spending) in GDP.

**Consequence:** The State's physical infrastructure exists but is economically inert. No public-sector wages flow into the economy, no civil servants consume goods, and `G` stays at 0.

**Fix Plan:**
- Create a "State Employer" pseudo-company for each country that represents the government as an employer.
  - Set its `sector` to `Sector::PublicServices`.
  - Set `target_fte_demand` to the sum of all state buildings' `worker_capacity`.
  - Set `offered_wage_per_fte` to a civil-service wage (e.g., `base_wage * 0.8`).
  - Fund it from `country.budget.liquid_reserves` each turn (treasury-funded payroll).
- Alternatively, modify `resolve_regional_labor_market` to also accept state buildings and create synthetic `LaborBid`s for them.
- **Wages paid to state employees should accumulate to `task.gdp_acc.government_spending`.**
- This is the primary mechanism for `G > 0`.

### 1.4 The G-Accumulator Bug — `* 100.0` in `migrate_legacy_budget`

**File:** `state/src/politics/ministries.rs` line 1147

**Finding:**
```rust
ministry.allocated_cash = (nominal * legacy_share * 100.0).round() / 100.0;
```

`legacy_share` is already a fraction (e.g., 0.05 for 5% industry allocation). The `* 100.0` inflates it by 100x, turning 5% into 500%. With `nominal = 23B` and `legacy_share = 0.05`, the ministry gets `23B * 0.05 * 100 = 115B` — far more than the treasury's ~12B.

**Chain of failure:**
1. `migrate_legacy_budget` sets `allocated_cash = 115B` per ministry.
2. `allocate_cash_to_ministries` computes `ratio = liquid_reserves / promised = 12B / 115B ≈ 0.1`.
3. Each ministry gets `115B * 0.1 = 11.5B`, totaling ~12B across all ministries.
4. `country.budget.liquid_reserves -= 12B` → treasury is now 0.
5. `execute_competency_spending_with_parties` checks `country.budget.liquid_reserves >= encumbrance` (line 663) — but reserves are 0, so **no procurement happens**.
6. `task.gdp_acc.government_spending += 0.0`.

**Fix Plan:**
- Remove the `* 100.0` from line 1147:
  ```rust
  ministry.allocated_cash = (nominal * legacy_share).round() / 100.0;
  ```
- This gives each ministry `23B * 0.05 = 1.15B`, which is reasonable relative to the ~12B treasury.
- `allocate_cash_to_ministries` will then allocate proportionally without draining the treasury.
- Ministries will actually be able to procure goods, and `G` will be non-zero.

### 1.5 Ministry Procurement — Missing Asks for Some Commodities

**File:** `state/src/engine/turn.rs` lines 834–887

**Finding:** The ministry procurement loop (Phase 26 fix) populates the local order book with sell orders (asks) from companies that have inventory of the commodities ministries want to buy. However:
- The `limit_price` for ministry buy bids is hardcoded at `120.0` (line 658).
- If the reference price for a commodity is above 120.0, no trade will match.
- The ask price is `ref_price * 1.1` (10% markup), which may exceed 120.0 for expensive commodities like IndustrialMachinery.

**Fix Plan:**
- Replace the hardcoded `120.0` with a dynamic limit price based on `get_reference_price` or `market_history`.
- Use `ref_price * 1.2` (20% above reference) as the ministry's willingness-to-pay.

---

## PART 2: Corporate Agility & State Intervention (AI Audit)

### 2.1 Dynamic Production Methods — STUB (No-Op)

**File:** `state/src/corporate/strategy.rs` lines 294–341; `state/src/corporate/manager.rs` lines 772–775

**Finding:** The corporate AI has a `evaluate_method_switch` function that:
- Creates a **dummy** `ActiveProductionMethod` with empty inputs/outputs (line 302–313).
- Calls `find_alternative_methods` which returns **hardcoded synthetic alternatives** (line 420–432) — not real registry methods.
- Returns `CorporateAction::SwitchMethod { method: alt_method }` if the alternative has positive gross margin.
- But in `manager.rs` line 772: `CorporateAction::SwitchMethod { .. } | ... => {}` — **the action is a no-op!**

**Consequence:** Companies never switch production methods. If a company's method requires ElectronicComponents and none are available, it simply produces nothing. There is no intelligent downgrade to a simpler method.

**Fix Plan:**
- **Implement real method switching:**
  1. In `evaluate_method_switch`, use the building's actual `active_method` (not a dummy).
  2. Query the `Registries` for all methods in the same sector that:
     - Have `year <= current_year`.
     - Have `required_tech` satisfied or `None`.
     - Produce at least one of the same output commodities.
     - Have inputs that are actually available on the market (check `market_history` for reference prices).
  3. Rank alternatives by gross margin (output revenue - input costs - wages).
  4. Switch to the best alternative if the current method's gross margin is negative or near-zero.
- **Apply the action in `manager.rs`:**
  - Replace `CorporateAction::SwitchMethod { .. } => {}` with code that:
    - Finds the building(s) owned by the company.
    - Replaces `building.active_method` with the new method.
    - Logs the switch for telemetry.

### 2.2 Vertical Integration — Not Implemented

**File:** `state/src/corporate/` (all files)

**Finding:** No code exists for vertical integration, subsidiaries, acquisitions, or mergers. The word "subsidiary" does not appear anywhere in the codebase.

**Assessment:** Vertical integration is a complex feature that requires:
- Company ownership graphs.
- Subsidiary creation with capital transfers.
- Input-shortage detection across the supply chain.
- Multi-building management.

**Recommendation:** Defer to Phase 29+. The method-switching fix (2.1) is a simpler and more impactful solution for the immediate supply-chain deadlock problem. Vertical integration can be simulated more simply through the State Intervention mechanism (2.3).

### 2.3 State Intervention — Not Implemented

**File:** `state/src/state/special_economic_zones.rs`; `state/src/politics/ministries.rs`

**Finding:**
- `SpecialEconomicZone` structs exist with tax multipliers, investment subventions, and clawback mechanics.
- But `country.special_economic_zones` is always initialized as `Vec::new()` and never populated.
- No AI exists to detect critical market shortages and create SOEs or SEZs.
- The ministry procurement system (Part 1.4) is the only state spending mechanism, and it's broken.

**Fix Plan:**
- **Phase 28 scope (minimal):**
  1. Fix the `* 100.0` bug so ministries can actually spend (Part 1.4).
  2. Add a "State Enterprise Spawner" that detects critical shortages:
     - After B2B matching, check for commodities with `net_surplus < 0` and no producers.
     - If a critical input (e.g., Iron, Copper, ElectronicComponents) has zero producers, spawn a state-owned company with a simple production method.
     - Fund it from `country.budget.liquid_reserves`.
  3. This is a safety valve — it only triggers when the domestic market fails to produce a critical good.
- **Phase 29+ scope (full):**
  - SEZ creation with tax incentives.
  - Subsidy targeting for strategic sectors.
  - Nationalization of bankrupt strategic companies.

---

## PART 3: The 0.00% Corruption & Shadow Economy Mystery

### 3.1 Shadow Employment — Never Activated + Broken Double-Entry

**File:** `state/src/economy/justice/legal_status.rs` lines 126–184; `state/src/engine/generator/corporate.rs` (all company constructors); `state/src/economy/trade/transfer_settler.rs` lines 328–348

**Finding (Activation):**
- Every company constructor sets `shadow_employment: None`.
- The only place `shadow_employment = Some(ShadowEmployment { ... })` is set is in a **test** (line 442).
- `process_shadow_economy_turn` (line 143) processes companies that already have `shadow_employment = Some(...)`, but since no company ever gets it set, the function is a no-op.
- There is no trigger that creates shadow employment — no event, no AI decision, no random chance.

**Finding (Double-Entry Violation):**
- The existing `process_shadow_economy_turn` at lines 169–172 debits company cash but **never credits citizen savings**:
  ```rust
  // Debit company cash for shadow wages (paid in cash, outside banking)
  if let Some(ref mut ba) = company.brokerage_account {
      ba.cash = (ba.cash - shadow_wages).max(0.0);
  }
  ```
- The comment explicitly says "paid in cash, outside banking" — this is an "under the table" game-ism that violates the engine's strict double-entry rules.
- The money vanishes from the economy: company cash decreases, but no worker's savings increase. This destroys money.
- Legal wages, by contrast, route through `settle_wage_payment` in `transfer_settler.rs` (line 328), which debits the company and credits `CitizenSavings` via `TransferRecipient::CitizenSavings`, syncing bank balance sheets.

**Consequence:** Hidden FTE = 0, PIT Evaded = 0, Shadow GDP = 0. The entire shadow economy subsystem is dead code. Even if activated, the existing wage payment logic would destroy money.

**Fix Plan:**

> **ARCHITECTURAL RULE (Shadow Wages):** There is no "under the table" in our memory structures. Every transaction must route through `TransferSettler` to ensure commercial bank reserves are kept in sync. When paying shadow wages, strictly deduct from `company.available_cash` (or brokerage) and credit `ClassDemographics.savings` using the `TransferSettler`. The ONLY difference from a legal wage is that you bypass the PIT (Income Tax) deduction function.

**Step 1: Fix the double-entry violation in `process_shadow_economy_turn`.**
- Replace the direct `ba.cash -= shadow_wages` debit with a call to `settle_wage_payment` (or `settle_transfer` with `TransferRecipient::CitizenSavings`).
- This requires changing the function signature to accept `&mut Country` (already has `country: &Country` — needs to be `&mut`) and the region/class context for the worker.
- The shadow wage payment flow becomes:
  1. Debit company cash via `TransferSettler` (same as legal wages).
  2. Credit worker's `ClassDemographics.savings` via `TransferSettler` (same as legal wages).
  3. **Bypass PIT**: do NOT call the PIT withholding step. The evaded PIT is tracked in `shadow.pit_evaded` for telemetry/inspectorate purposes, but no money is routed to the Treasury.
- This preserves double-entry: company cash decreases, worker savings increase, bank reserves sync. The only "loss" is the Treasury not receiving the PIT — which is exactly what tax evasion means.

**Step 2: Add a shadow employment trigger.**
- When a company cannot afford to pay the legal market wage (or has unfilled FTE after labor clearing), it may hire workers off-the-books at a lower wage.
- The probability of entering the shadow economy increases with:
  - High PIT rate (tax evasion incentive).
  - Low `available_cash` (can't afford legal wages).
  - High `target_fte_demand` (production pressure).
  - Low inspectorate capacity (low detection risk).
- When triggered, set `company.shadow_employment = Some(ShadowEmployment { ... })` with:
  - `hidden_fte`: a fraction of `target_fte_demand` (e.g., 10–30%).
  - `shadow_wage_per_fte`: below the legal wage (e.g., `offered_wage_per_fte * 0.5`).
- **Where to add the trigger:**
  - In `resolve_regional_labor_market` after labor clearing: if a company's `fulfilled_fte < target_fte_demand * 0.5` and it has `available_cash > 0`, it may resort to shadow hiring.
  - Or in `process_companies` in `manager.rs` as a corporate decision.

### 3.2 Corruption Index — Chicken-and-Egg Deadlock

**File:** `state/src/economy/justice/bribery.rs` lines 57–89, 126–138

**Finding:**
- `corruption_index` is initialized to `0.0` in `inspectorates.rs` line 381.
- Bribe acceptance probability = `corruption_index` (line 89: `rng.gen::<f64>() < corruption_index`).
- Since `corruption_index = 0.0`, `rng.gen::<f64>() < 0.0` is **always false**.
- No bribe is ever accepted, so `bribes_accepted_this_turn = 0`.
- `update_corruption_index` adds `0 * 0.01 = 0` entrenchment, so corruption stays at 0.
- **The system is in a permanent zero-corruption equilibrium.**

**Fix Plan:**
- **Seed corruption at generation time:**
  - Set initial `corruption_index` to a small non-zero value (e.g., `0.05–0.15` depending on country development level).
  - Less developed countries start with higher corruption.
- **Add passive corruption drift:**
  - Even without bribes, corruption should slowly increase if oversight is low.
  - In `update_corruption_index`, add a small passive entrenchment term:
    ```rust
    let passive_drift = 0.001; // Small per-turn drift
    let entrenchment = (bribes_accepted_this_turn as f64 * 0.01) + passive_drift;
    ```
  - This ensures corruption is never permanently zero unless oversight is very high.

### 3.3 Construction Fraud — Exists but Disconnected

**File:** `state/src/construction/fraud.rs`

**Finding:** Construction fraud logic exists with fraud detection, fines, and reputation penalties. However, it only triggers during construction tender execution, which requires active `ConstructionTenders` — these are not being generated in the current simulation because the construction sector is not fully active.

**Fix Plan:** Ensure that construction tenders are generated for infrastructure projects (this ties into the State Intervention fix in Part 2.3). Once tenders flow, fraud triggers will activate naturally.

---

## PART 4: Sector Tab UI Enhancements

### 4.1 Add PMI Column

**File:** `state/src/ui/snapshot.rs` lines 40–49, 482–518; `state/src/ui/tui/render.rs` lines 302–339

**Finding:**
- PMI is calculated in `update_gdp_shares_from_employment` (indicators.rs line 94–99) as `100 * (employment / capacity)` and stored in `share.extra["pmi"]`.
- The `SectorRow` struct does not include a PMI field.
- `aggregate_sectors` computes rows from company data only — it doesn't access `country.budget.sectors` where PMI is stored.
- `build_country_snapshot` has access to `country` and could pass PMI data to `aggregate_sectors`.

**Fix Plan:**
- Add `pmi: f64` field to `SectorRow`.
- Modify `aggregate_sectors` to accept `&country.budget.sectors` (or a pre-extracted PMI map).
- For each sector, look up PMI from `share.extra["pmi"]`.
- Add a "PMI" column to the table in `render_sectors`.
- Column width: ~8 characters (e.g., "55.2").
- Color-code: green if PMI > 50, red if PMI < 50, yellow if PMI ≈ 50.

### 4.2 Add ToT % Change for Employment and Avg Wage

**File:** `state/src/ui/snapshot.rs` lines 40–49; `state/src/ui/tui/render.rs` lines 302–339

**Finding:**
- The Macro tab already has ToT/YoY deltas for macro-level fields (GDP, CPI, etc.) via `TelemetryDeltas` and the `history` buffer.
- The Sector tab has no per-sector history tracking.
- `SectorRow` has no ToT fields.

**Fix Plan:**
- Add `employment_tot: Option<f64>` and `wage_tot: Option<f64>` fields to `SectorRow`.
- Create a per-sector history buffer (or extend the existing telemetry history to store per-sector snapshots).
- In `build_country_snapshot`, compare current sector employment/wage to the previous turn's values.
- Format as `▲+2.3%` or `▼-1.5%` (matching the Macro tab's delta formatting).
- Add these as additional columns or as suffix annotations to the existing Employment and Avg Wage columns.
- **Column layout adjustment:**
  - Sector: 25 chars
  - Companies: 10 chars
  - GDP Share: 12 chars
  - Employment: 12 chars + ToT: 8 chars = 20 chars
  - Avg Wage: 12 chars + ToT: 8 chars = 20 chars
  - PMI: 8 chars
  - Total: ~95 chars (fits 100-column terminal)

---

## Implementation Priority & Sequencing

### Tier 1 — Critical Economy Fixes (must do first)
1. **Fix `* 100.0` bug** in `migrate_legacy_budget` (1 line change, massive impact).
2. **State Employer pseudo-company** — make state buildings participate in labor market and pay wages → `G > 0`.
3. **Seed corruption index** at generation time (1 line change, wakes up shadow economy).
4. **Add passive corruption drift** in `update_corruption_index` (1 line addition).

### Tier 2 — Ghost Sector Fixes
5. **Bank buildings** — create physical bank branches with employment capacity, FTE demand, and wages funded from bank capital.
6. **Cultural institutions generation** — spawn `CulturalBuilding` entities (Temples, Monasteries, CulturalHouses) at world generation. Wire monastery production methods, latifundium income, and donation/tithe flow to Church/NGO companies. **NO magical seed capital** — churches/NGOs funded organically through tithes, production revenue, and church fund subsidies.

### Tier 3 — Corporate AI
7. **Real method switching** — query registry for available alternatives, apply the action in `manager.rs`.
8. **Shadow employment trigger** — activate when companies can't afford legal wages.

### Tier 4 — UI Enhancements
9. **PMI column** in Sectors tab.
10. **ToT indicators** for Employment and Avg Wage in Sectors tab.

### Tier 5 — State Intervention (DEFERRED to future political phase)
11. **DEFERRED: State Enterprise Spawner.** Buildings cannot magically pop into existence mid-game. The engine has a robust Construction & Tender market (Phase 22). If the State wants an SOE to operate a new Iron Mine, it must create the Company entity and publish a `ConstructionTender`. The SOE can only begin producing ONCE the construction sector physically builds the mine using steel, timber, and cement. Wiring the SOE AI to use the Phase 22 `ConstructionTenders` system is too complex for this phase and is deferred to a future political phase. Under no circumstances are we to bypass the construction market to spawn functional buildings mid-simulation.

### Deferred to Phase 29+ (or future political phase)
- **State Enterprise Spawner** — SOE creation via ConstructionTenders (not magical building spawns).
- Vertical integration / subsidiaries.
- Full SEZ creation AI.
- Nationalization of bankrupt companies.
- Construction fraud activation (depends on tender flow).

---

## Files to Modify

| File | Changes |
|------|---------|
| `state/src/politics/ministries.rs` | Remove `* 100.0` bug; fix ministry limit_price |
| `state/src/engine/generator/mod.rs` | Add bank buildings; set bank FTE/wage |
| `state/src/engine/generator/corporate.rs` | Add NGO/Church buildings; generate cultural institutions; wire donations to companies |
| `state/src/infrastructure/cultural.rs` | Verify donation/tithe collection works with generated buildings |
| `state/src/economy/religion/religious_economy.rs` | Replace flat monastery production with real registry method lookup; wire church fund to G accumulator |
| `state/src/engine/turn.rs` | Add State Employer to labor market; accumulate state wages to G |
| `state/src/economy/justice/bribery.rs` | Add passive corruption drift |
| `state/src/economy/justice/inspectorates.rs` | Seed corruption_index at init |
| `state/src/economy/justice/legal_status.rs` | Add shadow employment trigger; **fix double-entry: route shadow wages through `TransferSettler` instead of direct cash debit** |
| `state/src/economy/labor/labor_market.rs` | Accept state building bids (or pseudo-company) |
| `state/src/corporate/strategy.rs` | Real method switching from registry |
| `state/src/corporate/manager.rs` | Apply SwitchMethod action; add shadow employment trigger |
| `state/src/ui/snapshot.rs` | Add PMI and ToT fields to SectorRow |
| `state/src/ui/tui/render.rs` | Add PMI and ToT columns to Sectors tab |

---

## Verification Plan

1. `cargo check` after each tier.
2. `cargo test --lib` — all 542+ tests must pass.
3. 24-turn golden audit:
   - `G > 0` for all countries.
   - Banking, NGO, Religion sectors have non-zero employment.
   - `corruption_index > 0` for at least some countries.
   - `total_hidden_fte > 0` for at least some countries.
   - Sectors tab shows PMI column and ToT indicators.
4. No magical goods or money injection.
5. No international trade implementation added.
6. Double-entry accounting preserved for all state spending.
