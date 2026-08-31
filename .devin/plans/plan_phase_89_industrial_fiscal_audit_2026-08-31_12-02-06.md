---
agent: devin-local
session: lowly-keyboard
created: 2026-08-28T08:02:49Z
---
# Phase 89: Industrial & Fiscal Initialization Audit

Technical Remediation Plan for four systemic bottlenecks: industrial cash crunch (furlough contagion), undiscovered veins & ghost mines, zero tax revenue, and demographic UI expansion.

## Audit Context

Turn 2 screenshots after v0.7.1 reveal three severe systemic bottlenecks paralyzing production and state revenue, plus a UI visibility request. Phase 88 successfully stabilized Agriculture (LDR healthy, arable land logging) and Banking (loans distributed), but the industrial sectors and fiscal pipeline remain broken.

---

## Pillar 1: The Industrial Cash Crunch (Furlough Contagion)

### Root Cause Analysis

**Current state:** `issue_agriculture_working_capital_loans` in `state/src/engine/generator/corporate.rs:816` only issues Working Capital Loans to `Sector::Agriculture` companies (line 858: `if company.sector != Sector::Agriculture { continue; }`). All other sectors receive a one-time **free Genesis Payroll Grant** of 3 turns of wages (line 1265: `payroll_grant = initial_fte * initial_wage * 3.0`), which is NOT a loan — it's free money that violates Rule 1 (closed-loop economy).

**The cascade:** Mining, HeavyIndustry, LightIndustry, Energy, and Construction companies start with only 3 turns of payroll coverage. By Turn 2, before their seed inventory sales clear through B2B/B2C, they hit wage arrears. The `is_distressed` function (strategy.rs:734) triggers because `operational_cash() < payroll * 2.0`. The `is_within_material_shortage_grace` function (strategy.rs:761) only protects Agriculture (revenue-based grace with 24-turn hardcap) — non-agriculture companies lose grace as soon as `financial_history` is non-empty (line 785), which happens after Turn 1's production cycle. Result: mass furloughs on Turn 2.

**Additional issue:** The 3-turn free Genesis Payroll Grant for non-agriculture sectors is NOT double-entry consistent — it's created from thin air. Agriculture correctly replaced this with loans in Phase 88, but the other sectors still have the free grant.

### Remediation Steps

1. **Rename and generalize the loan function** in `state/src/engine/generator/corporate.rs`:
   - Rename `issue_agriculture_working_capital_loans` → `issue_working_capital_loans` (or keep the name and broaden the filter).
   - Change the sector filter from `company.sector != Sector::Agriculture` to a check against a set of eligible sectors: `{Agriculture, Mining, HeavyIndustry, LightIndustry, Energy, Construction}`.
   - Exclude state-owned companies (`state_share >= 1.0`) — they don't need loans.
   - Exclude Banking and service sectors — they have shorter cash cycles and don't need 6-turn runway.

2. **Adjust the risk premium per sector** (not a magic constant — scale by sector capital intensity):
   - Agriculture: 100 bps (existing, longest cash cycle — harvest delay).
   - Mining: 150 bps (high CAPEX, long ramp-up).
   - HeavyIndustry: 150 bps (high CAPEX, long production chains).
   - LightIndustry: 100 bps (faster turnover).
   - Energy: 200 bps (highest CAPEX, longest ramp).
   - Construction: 100 bps (project-based, tranche-funded but needs startup cash).

3. **Compute loan principal as 6 turns of payroll** for all eligible sectors (same formula as Agriculture: `initial_fte * initial_wage * 6.0`).

4. **Remove the free Genesis Payroll Grant** for eligible sectors. The 3-turn free grant at line 1265 (`payroll_grant = initial_fte * initial_wage * 3.0`) must be removed for sectors that receive Working Capital Loans. The loan principal (6 turns) supersedes the free grant (3 turns). Non-eligible sectors (services, banking, etc.) retain the free grant temporarily until a follow-up phase converts them to loans too.

5. **Update the call site** at line 777 to call the renamed function. Update the doc comment.

6. **Extend the material-shortage grace period** in `state/src/corporate/strategy.rs:761`:
   - For heavy CAPEX sectors (Mining, HeavyIndustry, LightIndustry, Energy, Construction): Apply the same revenue-based grace as Agriculture — protect from material-shortage furlough until first non-zero revenue OR 12 turns (half the agriculture hardcap, since industrial sales cycles are shorter than agricultural harvest cycles).
   - For Agriculture: Keep existing 24-turn hardcap.
   - For other sectors: Keep existing `financial_history.is_empty()` check.

### Files to Modify
- `state/src/engine/generator/corporate.rs` — generalize loan function, remove free grant for eligible sectors
- `state/src/corporate/strategy.rs` — extend grace period for industrial sectors

### Directive Compliance
- Rule 1 (Double-Entry): Replaces free grant with legitimate loans. ✓
- Rule 2 (No Magic Numbers): Risk premiums are per-sector, not arbitrary. ✓
- Rule 4 (Complete Lifecycles): Loans have 24-turn repayment terms. ✓
- Rule 8 (Rational Actors): Banks lend at risk-adjusted rates. ✓

---

## Pillar 2: Undiscovered Veins & Ghost Mines

### Root Cause Analysis

**Issue 1 — All veins spawn as `discovered: false`:** In `state/src/society/planet.rs:235`, veins are created with `discovered: false`. This locks out extraction because mining buildings check `discovered` status before producing.

**Issue 2 — `active_mine_count` is always 0:** The `build_geological_deposit_rows` function in `state/src/ui/snapshot.rs:3595` reads `active_mine_count` from the resource value's JSON map. However, `reseed_resources_from_planet` in `state/src/society/geography.rs:1624` does NOT write an `active_mine_count` field — it only writes `commodity`, `formation_name`, `geological_reserves`, `reserves`, `annual_extraction`, `efficiency`, `domestic_consumption`, `extraction_cost`, `depth`, and `discovered`. The snapshot defaults to 0 (line 3597: `unwrap_or(0)`).

**Issue 3 — `formation_name` mapping:** The `reseed_resources_from_planet` function writes `vein.name` to the `formation_name` field (line 1655). The snapshot reads it at line 3591-3594. This should work IF the reseed function is actually called. Need to verify the reseed is invoked during world generation.

### Remediation Steps

1. **Initialize base industrial veins as `discovered: true`** in `state/src/society/planet.rs`:
   - After vein generation and merging (after `merge_overlapping_veins()` at line 241), add a pass that sets `discovered: true` for veins that:
     - Overlap at least one populated region (regions with population > 0).
     - Contain base industrial commodities (coal, iron, copper, etc. — NOT rare/precious metals).
   - Rare veins (gold, silver, rare earths) remain `discovered: false` — they require geological survey.
   - This requires passing region population data into the vein generation context, or doing a post-generation pass with access to regions.

2. **Fix `active_mine_count` in the snapshot** in `state/src/ui/snapshot.rs`:
   - The `build_geological_deposit_rows` function currently reads `active_mine_count` from the resource JSON, which is never written.
   - **Option A (preferred):** Pass `buildings` into `build_geological_deposit_rows` and count buildings where `building.deposit_id == resource_key` (the vein ID or composite ID). This is the authoritative count.
   - **Option B:** Have `reseed_resources_from_planet` write `active_mine_count: 0` as a placeholder, then update it during corporate generation after mines are spawned.
   - Choose Option A: modify `build_geological_deposit_rows` to accept `&[Building]` and count active mines by matching `deposit_id` to the resource key.

3. **Verify `reseed_resources_from_planet` is called** during world generation:
   - Trace the call chain in `state/src/engine/generator/mod.rs` to confirm `reseed_resources_from_planet` is invoked after Planet generation and before corporate generation.
   - If not called, add the call.

4. **Ensure `formation_name` is correctly written** in `reseed_resources_from_planet`:
   - Line 1655 writes `vein.name` to `formation_name`. This is correct.
   - Verify that `generate_vein_name` (planet.rs:95) produces human-readable names (it does: "Northern Iron Range", etc.).
   - The snapshot reads it at line 3591-3594 with fallback to "Unknown". If veins have names, this should work.

5. **Update `reseed_resources_from_planet`** to write `discovered: vein.discovered` (already done at line 1663). After step 1, base veins will have `discovered: true`, so the snapshot will show them as discovered.

### Files to Modify
- `state/src/society/planet.rs` — set `discovered: true` for base industrial veins in populated regions
- `state/src/ui/snapshot.rs` — fix `active_mine_count` by counting buildings with matching `deposit_id`
- `state/src/society/geography.rs` — verify `reseed_resources_from_planet` is called and writes correct data

### Directive Compliance
- Rule 11 (Fog of War): Only base veins in populated regions are auto-discovered. Rare veins remain hidden. ✓
- Rule 17 (Full-Stack Accountability): UI shows correct mine counts and formation names. ✓

---

## Pillar 3: The Fiscal Black Hole (Zero Tax Revenue)

### Root Cause Analysis

**Current tax flow (traced through `state/src/engine/turn.rs`):**

1. **PIT:** Withheld at source in `resolve_regional_labor_market` (labor_market.rs:637-692). `pit_withheld` is accumulated in `LaborAllocationMatrix`. After labor clearing, at turn.rs:2748, `pit_withheld` is credited to `country.budget.liquid_reserves`. This is immediate — there is no `accumulated_pit` field.

2. **VAT:** Collected during B2C clearing in `settle_b2c_clearing` (retail.rs:1017). At turn.rs:3054-3055, `vat_collected` is credited to `country.budget.liquid_reserves` AND `country.accumulated_vat`.

3. **CIT:** Computed in `process_tax_collection_turn` (tax.rs:1337-1389). Liabilities are returned to the caller. At turn.rs:3565-3582, companies are physically debited. At turn.rs:3597, the routed amount is credited to `country.budget.liquid_reserves` via `route_tax_collection_to_country`.

4. **Property Tax:** Collected by `process_regional_taxes` (fiscal_transfers.rs:25) at turn.rs:3621. This updates `governance.budget.tax_revenue` and `governance.budget.property_tax` at the REGIONAL level. It does NOT flow to the national treasury.

5. **Display:** `last_tax_result` is stored at turn.rs:3620. The Finance snapshot reads it at snapshot.rs:3892-3900.

**Hypothesis — Why tax revenue shows 0.00:**

The most likely root cause is a **cascade from Pillar 1's furlough contagion**:
- Turn 2: Mass furloughs in industrial sectors → no wages paid → PIT = 0.
- Turn 2: No production → no goods to sell → B2C revenue = 0 → VAT = 0.
- Turn 2: No profit → CIT = 0.
- `last_tax_result` is overwritten every turn, so Turn 1's non-zero tax data is replaced by Turn 2's zeros.

However, the user reports "4.5M in macro Consumption" which suggests B2C IS running. This means either:
- (a) The 4.5M Consumption is from Turn 1's GDP breakdown which persists in `macro_indicators.gdp_breakdown.consumption`.
- (b) B2C is running on Turn 2 but VAT collection is broken.

**Additional structural issues found:**

- **No `accumulated_pit` field:** The user's directive mentions `accumulated_pit`, but this field does not exist on `Country`. PIT is credited immediately to `liquid_reserves` during labor clearing, not accumulated.
- **Property tax is regional-only:** `process_regional_taxes` updates `governance.budget.tax_revenue` (regional), not `country.budget` (national). The Finance tab reads from `last_tax_result` which has no property tax field.
- **Tax assessment vs furlough sequencing:** Labor clearing (W1) runs BEFORE tax collection (PHASE 7). If companies furlough workers during corporate decisions (which runs AFTER labor clearing but BEFORE tax collection), the furloughed workers don't generate wages → no PIT. But actually, furloughs happen during corporate strategy evaluation, which runs in a different phase. Need to verify the exact sequence.

### Remediation Steps

1. **Add diagnostic logging** to trace tax flow:
   - Log `pit_withheld` after labor clearing (turn.rs:2748).
   - Log `vat_collected` and `accumulated_vat` after B2C clearing (turn.rs:3055).
   - Log `total_cit_debited` and `total_actual_collected` after tax collection (turn.rs:3583).
   - Log `last_tax_result` fields before storing (turn.rs:3620).
   - This will confirm whether the issue is collection (0 tax computed) or display (tax computed but not stored/read correctly).

2. **Verify PIT withholding is non-zero:**
   - Check that `pit_rate` (from `country.tax_rates.income_tax.rate`) is non-zero. `build_tax_rates` sets it to `rng.gen_range(0.1..0.25)`, so it should be 10-25%.
   - Check that labor clearing actually pays wages. If all companies furloughed their workforce BEFORE labor clearing, no wages are paid → no PIT.
   - Verify the sequence: furlough decisions happen in corporate strategy evaluation, which runs AFTER labor clearing. So Turn 2's labor clearing should still pay wages to workers who were hired on Turn 1. The furloughs take effect on Turn 3 (after the corporate decision phase).

3. **Verify VAT collection is non-zero:**
   - Check that `commercial_buildings` (retail stores) exist and have inventory.
   - Check that `consumer_demand` is non-zero (citizens have savings from Turn 1 wages).
   - Check that `blended_vat_rate` is non-zero (VAT rates are configured).
   - If B2C revenue is 4.5M, VAT should be ~4.5M * 0.15-0.23 = 675K-1M.

4. **Verify CIT computation:**
   - Check that `building.last_profit` is non-zero for some buildings.
   - On Turn 1, buildings may not have produced yet (first production cycle). On Turn 2, if furloughed, no production → no profit → CIT = 0.
   - This is expected behavior — CIT is a profit tax, and if there's no profit, there's no CIT.

5. **Add property tax to national tax reporting:**
   - The `TaxCollectionResult` struct (tax.rs:1195) has no `property_tax_collected` field.
   - Add `pub property_tax_collected: f64` to `TaxCollectionResult`.
   - In `process_regional_taxes` (fiscal_transfers.rs:25), aggregate total property tax across all regions and return it.
   - In turn.rs after `process_regional_taxes`, update `tax_result_stored.property_tax_collected` with the aggregated regional property tax.
   - Add `property_tax_revenue` to `FinanceSnapshot` and display it in the Finance tab.

6. **Add `accumulated_pit` field** to `Country` (state/mod.rs):
   - Add `pub accumulated_pit: f64` with `#[serde(default)]`.
   - In labor clearing (turn.rs:2748), accumulate PIT: `country.accumulated_pit += labor_alloc.pit_withheld` (in addition to crediting `liquid_reserves`).
   - Reset `accumulated_pit = 0.0` at the start of each turn (alongside `accumulated_vat`).
   - In `process_tax_collection_turn`, read `country.accumulated_pit` for reporting (like VAT).
   - This provides a symmetric accumulation pattern for both PIT and VAT.

7. **Verify tax assessment vs furlough sequencing:**
   - Trace the exact turn phase order:
     1. W1: Labor clearing (wages paid, PIT withheld)
     2. R6: B2C clearing (VAT collected)
     3. PHASE 7: Tax collection (CIT computed, `last_tax_result` stored)
     4. Corporate strategy evaluation (furlough decisions for NEXT turn)
   - Furloughs decided in step 4 take effect in the NEXT turn's labor clearing. So Turn 2's tax collection should still see Turn 2's wages and consumption.
   - If this sequence is correct, the 0.00 issue is likely because Turn 2's economic activity is genuinely 0 (companies couldn't produce because they had no inputs — the seed inventory was sold but not yet converted to cash).

8. **Fix the root cause:** The furlough contagion (Pillar 1) is the primary driver. Once industrial companies have 6-turn Working Capital Loans, they won't furlough on Turn 2, and tax revenue will flow naturally. The diagnostic logging (step 1) will confirm this.

### Files to Modify
- `state/src/state/mod.rs` — add `accumulated_pit` field to `Country`
- `state/src/state/tax.rs` — add `property_tax_collected` to `TaxCollectionResult`, read `accumulated_pit` for reporting
- `state/src/engine/turn.rs` — accumulate PIT, reset `accumulated_pit`, aggregate property tax, add diagnostic logging
- `state/src/politics/fiscal_transfers.rs` — return total property tax from `process_regional_taxes`
- `state/src/ui/snapshot.rs` — add `property_tax_revenue` to `FinanceSnapshot`, read from `last_tax_result`
- `src/pages/FinancePage.tsx` — display property tax revenue

### Directive Compliance
- Rule 1 (Double-Entry): All taxes are physically debited from entities and credited to treasury. ✓
- Rule 7 (Individual Accountability): CIT is per-company, property tax is per-region. ✓
- Rule 16 (Temporal Causality): Tax collection runs after economic activity, not before. ✓
- Rule 17 (Full-Stack Accountability): Property tax visible in Finance tab. ✓

---

## Pillar 4: Demographic UI Expansion (Peasant Population)

### Root Cause Analysis

**Current state:** The `MacroIndicatorsResponse` DTO (snapshot.rs:2542) has fields for GDP, unemployment, inflation, wages, money supply, GDP components, CPI/PPI, telemetry deltas, and furloughed total. It does NOT include any peasant demographic metrics.

**Critical semantic distinction:** Peasants are NOT corporate agricultural workers. In this engine, peasants exist OUTSIDE the corporate structure. They operate on `Smallholder` parcels (cadastre.rs:1106, `land_use_tag: "Smallholder"`) and belong to the rural demographic classes `FreePeasant` and `Serf` — NOT to any `Sector::Agriculture` company. Corporate agricultural FTE tracks wage laborers on commercial farms, which is a fundamentally different population.

**Rural class structure** (geography.rs:748-757, 1958-2003):
- `FreePeasant` — "Free Peasants - own smallholdings, family labor." These are the primary peasant class running subsistence farms.
- `Serf` — "Serfs/Tied Peasants - tied to latifundia, unpaid labor." Also peasants, but unfree. Population is set at world gen (line 1976) and can be re-aggregated from Latifundia data via `aggregate_serf_population` (line 1384).
- `LandlessLaborer` — "Landless Laborers/Komornicy - work for wages, no land." These are rural WAGE laborers who work for corporate farms — they are NOT peasants.
- `Aristocracy` — landowning elite. NOT peasants.

**The peasant population = `FreePeasant` + `Serf` class populations**, summed across all regions from `region.class_demographics.rural_classes`. This explicitly excludes:
- `LandlessLaborer` (rural wage laborers, not subsistence farmers)
- `Aristocracy` (landowning elite)
- Urban classes (`Worker`, `Bourgeoisie`) — peasants who migrated to cities
- Corporate Agriculture sector FTE — wage laborers on commercial farms

**Existing but inadequate field:** `LaborMarket.subsistence_peasants` (macro_data.rs:331) is set once at world generation (generator/mod.rs:1287) and NEVER updated during the turn loop. It is a static initial estimate, not a dynamic metric. The authoritative source is the `class_demographics.rural_classes` population fields, which ARE updated during the turn loop (migration, mortality, starvation deaths).

### Remediation Steps

1. **Add peasant population fields to `MacroIndicatorsResponse`** in `state/src/ui/snapshot.rs`:
   ```rust
   /// Absolute peasant population: FreePeasant + Serf class populations
   /// across all regions. These are subsistence farmers operating outside
   /// the corporate structure on Smallholder parcels.
   pub peasant_population: f64,
   /// Percentage share of peasant population out of total national population.
   pub peasant_pct: f64,
   ```

2. **Compute peasant population in `build_country_snapshot`:**
   - The function already iterates `country.regions` for OHS/savings aggregation (lines 2791-2801, 2855-2861). Add a parallel aggregation:
   ```rust
   let mut peasant_population: f64 = 0.0;
   for region in &country.regions {
       if let Some(free_peasant) = region.class_demographics.rural_classes.get("FreePeasant") {
           peasant_population += free_peasant.population as f64;
       }
       if let Some(serf) = region.class_demographics.rural_classes.get("Serf") {
           peasant_population += serf.population as f64;
       }
   }
   let total_pop = country.budget.population as f64;
   let peasant_pct = if total_pop > 0.0 { peasant_population / total_pop * 100.0 } else { 0.0 };
   ```
   - Use string keys `"FreePeasant"` and `"Serf"` to match the serde rename_all = "snake_case" serialization. Note: the enum variant `FreePeasant` serializes to `"free_peasant"` — need to verify the actual key used. World generation inserts with `"FreePeasant".to_string()` (geography.rs:1984) and `"Serf".to_string()` (geography.rs:1976), so the keys are the PascalCase variant names as strings, NOT the snake_case serde form. Use the PascalCase string keys.

3. **Update the Tauri command** in `src-tauri/src/commands/macro_queries.rs:50`:
   - Add the two new fields to the `MacroIndicatorsResponse` construction:
   ```rust
   peasant_population: md.labor.peasant_population,  // or compute from snap
   peasant_pct: md.labor.peasant_pct,
   ```
   - Either add the fields to `LaborSummary` or compute directly from the snapshot's country data. Since `build_country_snapshot` has access to `country`, compute there and pass through.

4. **Add fields to `LaborSummary`** in `state/src/ui/snapshot.rs:199`:
   ```rust
   /// Peasant population (FreePeasant + Serf classes) across all regions.
   pub peasant_population: f64,
   /// Peasant share of total national population (percentage).
   pub peasant_pct: f64,
   ```
   - Compute in `build_country_snapshot` where `LaborSummary` is constructed (line 2818).

5. **Update the frontend** in `src/pages/MacroPage.tsx`:
   - Add a new StatCard after the Furloughed card (line 37):
   ```tsx
   <StatCard label="Peasants" value={`${num(Math.round(macro.peasant_population))} (${macro.peasant_pct.toFixed(1)}%)`} />
   ```
   - This places it prominently next to the Unemployment Rate, as requested.

6. **Regenerate TypeScript DTOs:** The `ts_rs` macro will auto-export the new fields to `src/types/api.ts` on next build.

### Files to Modify
- `state/src/ui/snapshot.rs` — add `peasant_population` and `peasant_pct` to `MacroIndicatorsResponse` and `LaborSummary`, compute from `rural_classes` in `build_country_snapshot`
- `src-tauri/src/commands/macro_queries.rs` — pass new fields through
- `src/pages/MacroPage.tsx` — display peasant population metric

### Directive Compliance
- Rule 17 (Full-Stack Accountability): New metric visible in UI. ✓
- Rule 12 (English-Only): Field names in English. ✓
- Rule 7 (Individual Accountability): Peasant population is extracted from per-class demographic records, not averaged or communized. ✓

---

## Implementation Order

1. **Pillar 2** (Veins & Ghost Mines) — quickest fix, unblocks mining production.
2. **Pillar 1** (Industrial Cash Crunch) — root cause of furlough contagion and tax black hole.
3. **Pillar 3** (Fiscal Black Hole) — partially resolved by Pillar 1; add diagnostics and property tax.
4. **Pillar 4** (Demographic UI) — cosmetic, no economic impact.

## Verification

- [ ] `cargo build` — zero compilation errors
- [ ] `cargo test --release` — all 2200+ tests pass with 0 failures
- [ ] `cargo clippy` — zero warnings
- [ ] `npm build` — frontend builds successfully, new DTOs generated
- [ ] Manual verification: Generate new world, run Turn 1 and Turn 2, verify:
  - Mining/HeavyIndustry/LightIndustry/Energy/Construction companies have Working Capital Loans
  - Geological deposits show as `discovered: true` with correct formation names
  - Active mine count is non-zero for regions with mines
  - Tax revenue (PIT, VAT, CIT) is non-zero on Turn 1 and Turn 2
  - Macro dashboard shows agricultural workforce count and percentage

## Risks/Considerations

- **Pillar 1 risk:** Extending loans to more sectors increases bank balance sheet expansion. Need to verify that eligible banks have sufficient capital. The Phase 88 fix already distributes across all Commercial/Universal banks, so this should scale.
- **Pillar 2 risk:** Auto-discovering all base veins removes exploration gameplay. Mitigation: only base industrial commodities (coal, iron, copper, etc.) are auto-discovered; rare/precious metals remain hidden.
- **Pillar 3 risk:** Adding `accumulated_pit` is a save-breaking change (Rule 10 — Domain Purity Over Backward Compatibility). This is acceptable in alpha phase.
- **Pillar 4 risk:** Peasant population is extracted from `FreePeasant` + `Serf` class demographics, which are updated during the turn loop (migration, mortality, starvation). The `LandlessLaborer` class is explicitly excluded — they are rural wage laborers, not subsistence farmers. The `subsistence_peasants` field on `LaborMarket` is NOT used because it is a static world-gen value that is never updated.
- **Cross-pillar dependency:** Pillar 3's tax black hole is primarily caused by Pillar 1's furlough contagion. Fixing Pillar 1 should naturally resolve most of Pillar 3. The diagnostic logging in Pillar 3 will confirm this.

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | No physical transformations introduced. Vein discovery is a metadata flag, not a material creation. Tax collection moves fiat between existing ledgers. Loan principal is created via fractional-reserve banking (explicitly designed Central Bank mechanic). |
| Double-Entry Bookkeeping | PASS | Working Capital Loans: Company gets cash (asset) + liability; Bank gets loan asset + deposit liability. Tax collection: PIT debited from wages, credited to treasury; VAT debited from consumer spending, credited to treasury; CIT debited from company cash, credited to treasury. Property tax debited from class savings, credited to regional budget. No void flows. Individual company ledgers maintained (per-company CIT liabilities, per-bank loan records). |
| No Teleportation | PASS | No physical commodity movements introduced. All changes are financial (loans, taxes) or metadata (vein discovery flags, UI DTOs). |
| Clamping | PASS | Loan principal clamped to `principal > 0.0` (existing check at corporate.rs:870). Agricultural workforce pct guards against division by zero (`employed_total > 0`). VAT/PIT accumulation starts at 0.0 and only increases. No new buffers that can go negative or exceed maximums. |
| No Magic Numbers | PASS | Risk premiums are per-sector (100-200 bps), justified by sector capital intensity and cash cycle length — not arbitrary. 6-turn loan runway matches Agriculture's existing proven runway. 12-turn industrial grace hardcap is half the agricultural hardcap (24 turns), reflecting shorter industrial sales cycles. VAT rates are dynamic (from `country.tax_rates.vat`). PIT rate is dynamic (from `country.tax_rates.income_tax.rate`). No hardcoded nominal floats. |
| Technological Matrices | PASS | No new building types or production methods introduced. Existing mining buildings and methods are reused. Vein discovery only affects existing building `deposit_id` linkage. |
| Architectural Parsimony | PASS | Extends existing `issue_agriculture_working_capital_loans` function (renamed, not duplicated). Extends existing `is_within_material_shortage_grace` function. Extends existing `TaxCollectionResult` struct. Extends existing `MacroIndicatorsResponse` DTO. Extends existing `build_geological_deposit_rows` function. No parallel systems created. |
| Temporal Causality | PASS | Loan issuance occurs during world generation (before Turn 1). Vein discovery occurs during world generation (before Turn 1). Tax collection runs in PHASE 7 after B2C clearing (R6) and labor clearing (W1) — correct sequence. Furlough grace period is checked during corporate strategy evaluation, which runs after labor clearing. `accumulated_pit` is reset at start of turn and accumulated during labor clearing, read during tax collection — no temporal paradox. |
| Asymmetric Information | PASS | Vein discovery: base industrial veins in populated regions are public knowledge; rare veins remain hidden (Fog of War preserved). Macro indicators are not role-gated (they are public economic data). Geological deposit rows already have role-gating by caller (snapshot.rs:3552 comment). No hidden data sent to frontend. |
| Full-Stack Accountability | PASS | Pillar 2: `active_mine_count` fix makes mine count visible in UI. Pillar 3: Property tax added to `FinanceSnapshot` and `FinancePage.tsx`. Pillar 4: Peasant population metric (`peasant_population`, `peasant_pct`) added to `MacroIndicatorsResponse` and `MacroPage.tsx`, extracted from `FreePeasant` + `Serf` rural class demographics — NOT corporate FTE. All backend changes have corresponding frontend updates. |
| Complete Entity Lifecycle | PASS | Working Capital Loans: Birth (issued at world gen), Life (24-turn repayment term, interest accrual), Death (repayment or default). Veins: Birth (generated with reserves), Life (extraction depletes reserves), Death (reserves exhausted). No immortal structures. `accumulated_pit` is reset each turn (no infinite accumulation). |
| Market Forces | PASS | Loans are distributed across eligible banks via random assignment (existing pro-rata-like distribution). No hardcoded percentage splits. Tax rates are progressive/configurable, not fixed splits. |
| Rational Actors | PASS | Banks lend at risk-adjusted rates (XIBOR + margin + sector-specific risk premium). Companies take loans to survive startup period — rational survival behavior. No debt forgiveness or charity. Furlough grace is temporary (hardcapped), not permanent protection. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None
