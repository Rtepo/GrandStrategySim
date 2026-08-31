---
agent: devin-local
session: lowly-keyboard
created: 2026-08-28T08:02:49Z
---
# Phase 90: The Great Genesis & Telemetry Audit

Technical Remediation Plan for 5 Pillars: Working Capital persistence, accrual financial history, cooperative bank generation, geological vein diversity, and telemetry ToT extensions.

# Phase 90: The Great Genesis & Telemetry Audit

**Date:** 2026-08-31  
**Status:** Pending Approval  
**Predecessor:** Phase 89 (v0.8.0 — Industrial & Fiscal Stabilization)

## Summary

Phase 89 introduced Working Capital Loans, auto-discovery of base industrial veins, tax pipeline fixes, and peasant demographic UI. Turn 0/Turn 1 screenshots reveal five deeper anomalies: (1) the Working Capital Loan is never persisted to disk, leaving agriculture companies at 0.00 cash; (2) financial history shows 0.00 for all fields because wage arrears are not accrued; (3) bank generation produces too many Investment banks and zero Cooperative banks; (4) vein generation places too few base industrial veins globally, resulting in Limestone monoculture; (5) ToT deltas are missing for Peasant Population, Furloughed, and GDP/Capita.

---

## Pillar 1: The Working Capital Black Hole & Furlough Panic

### Root Cause Analysis

**Three compounding bugs:**

1. **Loan not persisted to disk (CRITICAL):** In `generate_corporate_entities` (`corporate.rs:799-805`), `issue_working_capital_loans` modifies `all_companies` in memory (adding `principal` to `available_cash` and `liabilities`), but companies were already saved to disk at line 789. The loan function only saves **banks** back to disk (line 972). When the game loads from disk, companies have their pre-loan cash — which is near zero after seed inventory deduction.

2. **Seed companies start at 0.00 cash:** In `create_seed_company` (`corporate.rs:2978-2982`) and `create_seed_company_with_explicit_method` (`corporate.rs:2510-2514`), loan-eligible sectors skip the payroll grant but `available_cash` is initialized to `0.0` (not `company_liquid` as in `generate_region_companies:1312`). Then seed inventory cost is deducted, driving `available_cash` negative. The loan is supposed to fix this but isn't persisted.

3. **Loan principal doesn't cover seed inventory:** The principal is `initial_fte * initial_wage * 6.0` (6 turns of payroll). But seed inventory cost (up to 50% of `liquid_capital`) was already deducted from cash. The loan should cover both.

### Remediation Steps

#### Step 1.1: Re-save companies after loan issuance
**File:** `state/src/engine/generator/corporate.rs`  
**Location:** After `issue_working_capital_loans` call (line ~805)  
**Action:** Add a re-save of `all_companies` grouped by sector, identical to the pattern at lines 783-790. This ensures the loan-modified `available_cash` and `liabilities` are persisted.

```rust
// Phase 90: Re-save companies AFTER Working Capital Loans so the
// loan-modified available_cash and liabilities persist to disk.
let mut by_sector_post_loan: HashMap<String, Vec<Company>> = HashMap::new();
for c in &all_companies {
    let sname = sector_json_name(c.sector);
    by_sector_post_loan.entry(sname).or_default().push(c.clone());
}
for (sector_name, companies) in by_sector_post_loan {
    let _ = company_store.save_sector(&country.name, &sector_name, None, &companies);
}
```

#### Step 1.2: Set initial cash for seed companies
**File:** `state/src/engine/generator/corporate.rs`  
**Locations:** 
- `create_seed_company_with_explicit_method` (line ~2510)
- `create_seed_company` (line ~2978)

**Action:** For loan-eligible sectors, set `company.available_cash = company_liquid` (matching `generate_region_companies:1312`). This ensures seed companies have working capital before the loan is issued.

```rust
if is_working_capital_loan_eligible(sector) {
    company.available_cash = company_liquid;
} else {
    let payroll_grant = initial_fte * initial_wage * 3.0;
    company.available_cash += payroll_grant;
}
```

#### Step 1.3: Increase loan principal to cover seed inventory
**File:** `state/src/engine/generator/corporate.rs`  
**Location:** `issue_working_capital_loans` (line ~908-911)  
**Action:** Add the seed inventory cost (stored in `company.extra["seed_inventory_cost"]`) to the principal calculation.

```rust
let seed_cost = company.extra.get("seed_inventory_cost")
    .and_then(|v| v.as_f64())
    .unwrap_or(0.0);
let principal = initial_fte * initial_wage * 6.0 + seed_cost;
```

This ensures the loan covers both the seed inventory purchase AND 6 turns of payroll, leaving companies with healthy `available_cash` after the loan is applied.

---

## Pillar 2: Empty Financial History (Accrual Accounting Failure)

### Root Cause Analysis

In `corporate/manager.rs:789-801`, the financial history record is built from `total_profit` (building-level profit = sales revenue - input costs), `overhead` (5% of gross profit), `interest`, and `tax`. **Wage costs are NOT included** — wages are paid separately in the labor market (`labor_market.rs:451-466`), deducted from `company.available_cash`. When a company cannot afford wages, the unpaid portion accrues as `company.wage_arrears` but is never reflected in the financial history.

This means a company with massive wage arrears shows `revenue = 0.00`, `operating_costs = 0.00`, `net_profit = 0.00` — completely hiding the financial distress from the UI.

### Remediation Steps

#### Step 2.1: Add wage expense to financial history
**File:** `state/src/corporate/manager.rs`  
**Location:** `process_company` function (line ~788-801)  
**Action:** Add `wage_expense` field to the financial history record. This includes both paid wages and accrued arrears for the current turn.

The wage expense can be computed as: `fulfilled_fte * offered_wage_per_fte` (the full payroll obligation). The portion that was actually paid is deducted from `available_cash` in the labor market; the unpaid portion is `wage_arrears` (incremented this turn). For accrual accounting, the full obligation should be recorded as an expense.

```rust
// Phase 90: Accrual accounting — record full wage obligation as expense.
let wage_expense = (company.fulfilled_fte as f64) * company.offered_wage_per_fte;

let record = Value::Object(
    [
        ("year".to_string(), Value::from(year)),
        ("revenue".to_string(), Value::from(total_profit + overhead)),
        ("operating_costs".to_string(), Value::from(overhead + wage_expense)),
        ("wage_expense".to_string(), Value::from(wage_expense)),
        ("wage_arrears".to_string(), Value::from(company.wage_arrears)),
        ("interest".to_string(), Value::from(interest)),
        ("taxes".to_string(), Value::from(tax)),
        ("net_profit".to_string(), Value::from(net_profit - wage_expense)),
    ]
    .into_iter()
    .collect(),
);
```

**Note:** `net_profit` must be reduced by `wage_expense` to reflect the true bottom line. The current `net_profit = total_profit - overhead - interest - tax` does not include wages because wages are paid at the labor market level, not the building level. Adding wage expense here corrects the accrual accounting.

---

## Pillar 3: Banking Bottleneck & Cooperative Banks

### Root Cause Analysis

In `build_bank_companies` (`generator/mod.rs:1574-1584`), the bank type distribution is:
- First bank: Universal (if GDP > 100M) or Commercial
- Remaining banks: 50% Commercial, 50% Investment
- **Zero Cooperative banks**

Investment banks cannot issue working capital loans (they don't take retail deposits). With 50% of non-first banks being Investment banks, there are too few eligible lenders for the Working Capital Loan system.

### Remediation Steps

#### Step 3.1: Add Cooperative banks to the generation mix
**File:** `state/src/engine/generator/mod.rs`  
**Location:** `build_bank_companies` (line ~1574-1584)  
**Action:** Replace the 50/50 Commercial/Investment split with a weighted distribution that includes Cooperative banks.

```rust
let bank_type = if is_first {
    if treasury.gdp > 100_000_000.0 {
        BankingBankType::Universal
    } else {
        BankingBankType::Commercial
    }
} else {
    // Phase 90: Weighted distribution — 40% Commercial, 30% Cooperative,
    // 20% Investment, 10% Universal. Cooperative banks handle
    // agricultural and small-business working capital loans.
    let roll = rng.gen::<f64>();
    if roll < 0.40 {
        BankingBankType::Commercial
    } else if roll < 0.70 {
        BankingBankType::Cooperative
    } else if roll < 0.90 {
        BankingBankType::Investment
    } else {
        BankingBankType::Universal
    }
};
```

#### Step 3.2: Make Cooperative banks eligible for working capital loans
**File:** `state/src/engine/generator/corporate.rs`  
**Location:** `issue_working_capital_loans` (line ~868-875)  
**Action:** Add `BankType::Cooperative` to the eligible bank filter.

```rust
let eligible_bank_indices: Vec<usize> = bank_companies
    .iter()
    .enumerate()
    .filter(|(_, b)| {
        b.bank_type == Some(BankType::Commercial) 
            || b.bank_type == Some(BankType::Universal)
            || b.bank_type == Some(BankType::Cooperative)
    })
    .map(|(i, _)| i)
    .collect();
```

#### Step 3.3: Configure Cooperative bank balance sheet
**File:** `state/src/engine/generator/mod.rs`  
**Location:** `build_bank_companies` (line ~1602-1607)  
**Action:** Add Cooperative to the deposit-taking bank types.

```rust
let total_deposits = match bank_type {
    BankingBankType::Commercial | BankingBankType::Universal | BankingBankType::Cooperative => {
        treasury.citizen_savings * 0.5 * size_factor / num_banks as f64
    }
    _ => 0.0,
};
```

#### Step 3.4: Name Cooperative banks appropriately
**File:** `state/src/engine/generator/mod.rs`  
**Location:** `build_bank_companies` (line ~1587-1598)  
**Action:** Add Cooperative bank naming pattern.

```rust
let bank_name = if is_first {
    format!("State Bank of {name}")
} else if bank_type == BankingBankType::Cooperative {
    format!("Cooperative Bank of {name} {}", bank_idx)
} else {
    // existing surname-based naming
    ...
};
```

---

## Pillar 4: The Limestone Monoculture & Ghost Commodities

### Root Cause Analysis

**Two compounding issues:**

1. **Global vein count too low for regional coverage:** Veins are placed at random lat/lon (-80 to 80, -170 to 170). With only 12-25 AbundantIndustrial veins and 20-40 Ubiquitous veins globally, and a 10-degree overlap window, most populated regions receive zero or one vein. Ubiquitous veins (Limestone, Peat, Gravel) are the most numerous (20-40), so Limestone is the most likely to appear.

2. **No per-region minimum guarantee:** There is no mechanism to ensure each populated region receives a diverse set of base industrial veins (Iron, Coal, Copper, Stone, Sand).

### Remediation Steps

#### Step 4.1: Guarantee base industrial veins per populated region
**File:** `state/src/society/planet.rs`  
**Location:** After `generate_veins` and before `discover_base_industrial_veins`  
**Action:** Add a new method `ensure_base_industrial_veins_per_region` that guarantees each populated region receives at least one vein for each AbundantIndustrial commodity (Iron, HardCoal, BrownCoal, Stone, Sand) and one Ubiquitous commodity (Limestone, Peat, Gravel). This is NOT magic spawning — it represents the geological reality that any settled region has surface-visible deposits of common industrial minerals.

```rust
/// Phase 90: Ensure each populated region has at least one vein for each
/// AbundantIndustrial and Ubiquitous commodity. This fixes the Limestone
/// monoculture by guaranteeing diverse base industrial deposits.
pub fn ensure_base_industrial_veins_per_region(
    &mut self,
    populated_regions: &[(String, f64, f64)], // (region_id, lat, lon)
    rng: &mut impl Rng,
) {
    let base_commodities: &[(Commodity, RarityTier)] = &[
        (Commodity::Iron, RarityTier::AbundantIndustrial),
        (Commodity::HardCoal, RarityTier::AbundantIndustrial),
        (Commodity::BrownCoal, RarityTier::AbundantIndustrial),
        (Commodity::Stone, RarityTier::AbundantIndustrial),
        (Commodity::Sand, RarityTier::AbundantIndustrial),
        (Commodity::Limestone, RarityTier::Ubiquitous),
        (Commodity::Peat, RarityTier::Ubiquitous),
        (Commodity::Gravel, RarityTier::Ubiquitous),
    ];

    for (region_id, lat, lon) in populated_regions {
        for &(commodity, tier) in base_commodities {
            // Check if this region already has a vein for this commodity.
            let already_has = self.veins.iter().any(|v| {
                v.commodity == commodity 
                    && v.overlapping_regions.iter().any(|r| r == region_id)
            });
            if already_has {
                continue;
            }

            // Generate a vein centered on this region.
            let (min_reserves, max_reserves) = tier.reserve_range();
            let total_reserves = rng.gen_range(min_reserves..max_reserves);
            let quality = rng.gen_range(0.3..1.0);
            let depth = rng.gen_range(50.0..2000.0);
            let extraction_cost = 1.0 + (depth / 1000.0) + (1.0 - quality) * 0.5;

            let vein_id = format!("VEIN-{:04}", self.veins.len() + 1);
            let vein_name = generate_vein_name(commodity, self.veins.len() + 1);

            self.veins.push(GeologicalVein {
                id: vein_id,
                composite_id: None,
                name: vein_name,
                commodity,
                rarity_tier: tier,
                total_reserves,
                current_reserves: total_reserves,
                cells: vec![(*lat, *lon)],
                overlapping_regions: vec![region_id.clone()],
                extraction_cost,
                quality,
                depth,
                discovered: false, // Will be set by discover_base_industrial_veins
            });
        }
    }
}
```

#### Step 4.2: Call the new method during world generation
**File:** `state/src/engine/generator/mod.rs`  
**Location:** After `generate_planet` (line 199) and before `discover_base_industrial_veins` (line 210)  
**Action:** Call `ensure_base_industrial_veins_per_region` with populated region data.

```rust
// Phase 90: Ensure each populated region has diverse base industrial veins.
let populated_region_coords: Vec<(String, f64, f64)> = regions
    .values()
    .filter(|r| r.population > 0)
    .map(|r| (r.id.clone(), r.coord_y, r.coord_x))
    .collect();
state.planet.ensure_base_industrial_veins_per_region(&populated_region_coords, &mut rng);
```

#### Step 4.3: Increase the mining company cap per region
**File:** `state/src/engine/generator/corporate.rs`  
**Location:** `seed_geology_based_mines` (line ~2150)  
**Action:** Increase `max_mines` from 5 to 8 to accommodate the additional base industrial veins.

```rust
let max_mines = 8; // Phase 90: Increased from 5 to cover diverse base industrial veins.
```

---

## Pillar 5: Telemetry UI Extensions

### Root Cause Analysis

`TelemetryDeltas` (`snapshot.rs:221-258`) currently has ToT/YoY deltas for GDP, CPI, PPI, M3, Unemployment, Shadow GDP, Corruption, Population, and Wage. Missing:
- `peasant_population_tot` / `peasant_population_yoy`
- `furloughed_tot` / `furloughed_yoy`
- `gdp_per_capita_tot` / `gdp_per_capita_yoy`

`TelemetrySample` (`macro_data.rs:441-496`) does not store `peasant_population`, `furloughed_total`, or `gdp_per_capita`. These must be added to the sample struct and populated during the telemetry recording phase.

### Remediation Steps

#### Step 5.1: Add fields to TelemetrySample
**File:** `state/src/state/macro_data.rs`  
**Location:** `TelemetrySample` struct (line ~495)  
**Action:** Add three new fields with `#[serde(default)]`.

```rust
/// Phase 90: Peasant population (FreePeasant + Serf) for ToT/YoY delta.
pub peasant_population: f64,
/// Phase 90: Total furloughed workers for ToT/YoY delta.
pub furloughed_total: f64,
/// Phase 90: GDP per capita for ToT/YoY delta.
pub gdp_per_capita: f64,
```

**Rule 10 note:** No `#[serde(default)]` is added. This is a clean save-breaking change — old saves will fail to deserialize and must be regenerated. This is acceptable in alpha phase per Rule 10.

#### Step 5.2: Add fields to TelemetryDeltas
**File:** `state/src/ui/snapshot.rs`  
**Location:** `TelemetryDeltas` struct (line ~257)  
**Action:** Add six new Option<f64> fields.

```rust
/// Phase 90: Peasant population ToT delta (percent).
pub peasant_population_tot: Option<f64>,
/// Phase 90: Peasant population YoY delta (percent).
pub peasant_population_yoy: Option<f64>,
/// Phase 90: Furloughed workers ToT delta (percent).
pub furloughed_tot: Option<f64>,
/// Phase 90: Furloughed workers YoY delta (percent).
pub furloughed_yoy: Option<f64>,
/// Phase 90: GDP per capita ToT delta (percent).
pub gdp_per_capita_tot: Option<f64>,
/// Phase 90: GDP per capita YoY delta (percent).
pub gdp_per_capita_yoy: Option<f64>,
```

#### Step 5.3: Populate new TelemetrySample fields
**File:** `state/src/engine/turn.rs`  
**Location:** Telemetry sample creation (line ~5171-5193)  
**Action:** Compute and populate the three new fields. Peasant population and furloughed total must be computed from the country state (same logic as `build_country_snapshot`).

```rust
// Phase 90: Compute peasant population and furloughed for telemetry.
let mut peasant_pop: f64 = 0.0;
for region in &country.regions {
    if let Some(fp) = region.class_demographics.rural_classes.get("FreePeasant") {
        peasant_pop += fp.population as f64;
    }
    if let Some(serf) = region.class_demographics.rural_classes.get("Serf") {
        peasant_pop += serf.population as f64;
    }
}
let furloughed = country.macro_indicators.labor_market.furloughed_total;
let pop = country.budget.population as f64;
let gdp_pc = if pop > 0.0 { md.gdp_breakdown.official_gdp / pop } else { 0.0 };

let sample = TelemetrySample {
    // ... existing fields ...
    peasant_population: peasant_pop,
    furloughed_total: furloughed,
    gdp_per_capita: gdp_pc,
};
```

#### Step 5.4: Compute new deltas
**File:** `state/src/ui/snapshot.rs`  
**Location:** `compute_deltas` function (line ~4425-4446)  
**Action:** Add the six new delta computations.

```rust
// Phase 90: Peasant population, furloughed, and GDP/capita deltas.
peasant_population_tot: history.tot_pct(peasant_population, |s| s.peasant_population),
peasant_population_yoy: history.yoy_pct(peasant_population, |s| s.peasant_population),
furloughed_tot: history.tot_pct(furloughed_total, |s| s.furloughed_total),
furloughed_yoy: history.yoy_pct(furloughed_total, |s| s.furloughed_total),
gdp_per_capita_tot: history.tot_pct(gdp_per_capita, |s| s.gdp_per_capita),
gdp_per_capita_yoy: history.yoy_pct(gdp_per_capita, |s| s.gdp_per_capita),
```

**Note:** `compute_deltas` must receive `peasant_population`, `furloughed_total`, and `gdp_per_capita` as parameters. Update the function signature and call site at `snapshot.rs:2904`. The variables `peasant_population` (computed at line 2835) and `furloughed_total` (via `macro_data.labor_market.furloughed_total`) are already available in the calling context. `gdp_per_capita` can be computed as `macro_data.gdp_breakdown.official_gdp / population_f64` (guarded against zero).

**Explicit initializer audit (Phase 89 lesson):**
- `TelemetrySample {` — one explicit initializer at `turn.rs:5171` (covered by Step 5.3). Test helper at `macro_data.rs:617` uses `..Default::default()` (still compiles via Rust's `Default` derive).
- `TelemetryDeltas {` — one explicit initializer at `snapshot.rs:4425` (covered by Step 5.4). Struct derives `Default`, so `..Default::default()` usages still compile.
- `MacroIndicatorsResponse {` — one explicit initializer at `macro_queries.rs:50`. Uses `deltas: md.deltas.clone()`, so new `TelemetryDeltas` fields automatically pass through. No changes needed.

#### Step 5.5: Update MacroPage.tsx StatCards
**File:** `src/pages/MacroPage.tsx`  
**Location:** StatCard grid (line ~33-42)  
**Action:** Add delta props to the three cards.

```tsx
<StatCard label="GDP/Capita" value={fmt(macro.gdp_per_capita)} delta={macro.deltas.gdp_per_capita_tot} />
<StatCard label="Furloughed" value={num(Math.round(macro.furloughed_total))} delta={macro.deltas.furloughed_tot} />
<StatCard label="Peasants" value={`${num(Math.round(macro.peasant_population))} (${macro.peasant_pct.toFixed(1)}%)`} delta={macro.deltas.peasant_population_tot} />
```

---

## Implementation Order

1. **Pillar 1** — Working Capital persistence (highest impact, fixes instant furlough)
2. **Pillar 4** — Geological vein diversity (fixes Limestone monoculture)
3. **Pillar 3** — Cooperative bank generation (fixes banking bottleneck)
4. **Pillar 2** — Accrual financial history (fixes empty financial UI)
5. **Pillar 5** — Telemetry ToT extensions (UI enhancement)
6. **Iron CI/CD** — cargo build, cargo test, cargo clippy, npm build

## Files to Modify

- `state/src/engine/generator/corporate.rs` — Pillar 1 (loan persistence, seed cash, principal), Pillar 3 (Cooperative eligibility), Pillar 4 (mine cap)
- `state/src/engine/generator/mod.rs` — Pillar 3 (bank weights, Cooperative naming), Pillar 4 (vein guarantee call)
- `state/src/society/planet.rs` — Pillar 4 (ensure_base_industrial_veins_per_region)
- `state/src/corporate/manager.rs` — Pillar 2 (accrual wage expense in financial history)
- `state/src/state/macro_data.rs` — Pillar 5 (TelemetrySample fields)
- `state/src/ui/snapshot.rs` — Pillar 5 (TelemetryDeltas fields, compute_deltas)
- `state/src/engine/turn.rs` — Pillar 5 (telemetry sample population)
- `src-tauri/src/commands/macro_queries.rs` — Pillar 5 (pass new deltas through)
- `src/pages/MacroPage.tsx` — Pillar 5 (StatCard delta display)

## Verification

- [ ] `cargo build --workspace` — zero errors
- [ ] `cargo test --workspace --all-targets` — all tests pass
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `npm run build` — TypeScript + Vite build passes
- [ ] Manual: Generate new world, verify Turn 0 companies have positive `available_cash`
- [ ] Manual: Verify Resources tab shows diverse veins (Iron, Coal, Stone, Sand, Limestone) per region
- [ ] Manual: Verify Finance tab shows non-zero wage expenses for companies with arrears
- [ ] Manual: Verify Macro page shows ToT deltas for GDP/Capita, Furloughed, and Peasants
- [ ] Manual: Verify Cooperative banks appear in the banking sector

## Risks/Considerations

- **Save compatibility (Rule 10):** Adding fields to `TelemetrySample` WITHOUT `#[serde(default)]` is a clean save-breaking change. Old saves will fail to deserialize and must be regenerated. This is the correct alpha-phase behavior per Rule 10 — no serde shims or legacy migrations.
- **Vein count increase:** Guaranteeing 8 base industrial commodities per populated region will increase total vein count significantly. With ~30 populated regions, this adds up to 240 veins. The `veins_for_region` lookup is O(N) but N is small enough that performance is not a concern.
- **Loan principal increase:** Adding seed inventory cost to the principal increases bank loan assets. This is correct double-entry: the company receives more cash (asset) and records more liability, the bank records more loan asset and more deposit liability.
- **Financial history semantics:** Adding `wage_expense` changes the meaning of `net_profit` in the financial history. The frontend may need to be updated to display the new `wage_expense` field. This is a semantic improvement, not a regression — the old `net_profit` was incorrect because it ignored wages.
- **Cooperative bank balance sheets:** Cooperative banks should have smaller tier_1_capital and deposits than Commercial banks, reflecting their member-owned nature. The `size_factor` scaling already handles this for non-first banks.

---

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | Pillar 4 adds geological veins with explicit `total_reserves` drawn from the existing `RarityTier::reserve_range()`. No material is created from nothing — veins represent pre-existing geological deposits. Physical quantities (reserves in tons) remain separate from fiat values (extraction_cost). Mining companies extract from these reserves via existing production methods. No mass teleportation. |
| Double-Entry Bookkeeping | PASS | Pillar 1: Working Capital Loan already follows double-entry (company cash += principal, company liabilities += principal; bank loans_issued.push(loan), bank deposits += principal). The fix is persistence, not new accounting. Adding seed_cost to principal is correct — the company received inventory (asset) and pays for it via the loan (liability), with the treasury credited as provider (existing line 616). Pillar 2: Wage expense is accrued — paid wages reduce cash (asset), unpaid wages increase wage_arrears (liability). The financial history records the full obligation as expense, matching accrual accounting. No counterparty-less flows. Pillar 3: Cooperative bank loans follow the same double-entry pattern as Commercial banks. |
| No Teleportation | PASS | No new physical movements are introduced. Veins are placed at region coordinates (lat/lon) and bound to regions via `overlapping_regions`. Mining companies are spawned in the same region as their veins. No goods are moved without logistics. |
| Clamping | PASS | Pillar 5: GDP per capita is guarded against zero population (`if pop > 0.0 { gdp / pop } else { 0.0 }`). Telemetry deltas use `tot_pct`/`yoy_pct` which return `None` when no history exists and `0.0` when previous value is zero (avoiding div-by-zero). Pillar 1: `available_cash` is set to `company_liquid` (positive by construction). Loan principal is positive (guarded by `if principal <= 0.0 { continue }`). Pillar 4: Vein reserves are drawn from `reserve_range()` (positive bounds). New `TelemetrySample` fields are NOT given `#[serde(default)]` — old saves break cleanly per Rule 10, and new saves always populate the fields with computed values (no zero-default risk). |
| No Magic Numbers | PASS with caveat | Pillar 1: Loan principal uses `initial_fte * initial_wage * 6.0` — the 6.0 is a turn count (6 turns = 1.5 months of payroll runway), not a nominal currency constant. `initial_wage` is derived from `company.offered_wage_per_fte` (dynamic). Seed cost is read from company metadata (dynamic). Pillar 3: Bank weights (0.40, 0.30, 0.20, 0.10) are market-share distribution probabilities, not economic thresholds — these are structural design choices, not magic costs. Pillar 4: `max_mines = 8` is an entity-count cap, not an economic threshold. Caveat: The 50% cap on seed inventory deduction (`seed_cost.min(company.liquid_capital * 0.5)`) is an existing constant, not introduced by this plan. |
| Technological Matrices | PASS | No new production methods or building types are introduced. Mining companies use existing registry methods (`mining_method_name_for_commodity`). Processing plants use existing HeavyIndustry methods. The plan extends existing systems without creating parallel registries. |
| Architectural Parsimony | PASS | All five pillars extend existing systems: (1) re-uses existing `save_sector` pattern; (2) extends existing `financial_history` record; (3) adds `Cooperative` to existing `BankType` enum (already defined); (4) extends existing `Planet` vein system; (5) extends existing `TelemetrySample`/`TelemetryDeltas`. No parallel systems created. |
| Temporal Causality | PASS | Pillar 1: Loan issuance occurs during world generation (before Turn 0), re-save occurs immediately after. Pillar 2: Financial history is recorded in `process_company` (post-production phase), after wages have been paid/arrears accrued in the labor market phase. Pillar 4: Vein guarantee runs during world generation, before `discover_base_industrial_veins` and before mining company generation. Pillar 5: Telemetry sample is recorded at end of turn (after all macro updates), matching existing pattern at `turn.rs:5167-5194`. No temporal paradoxes. |
| Asymmetric Information | PASS | No new hidden data is sent to the frontend. Pillar 5 adds public macro indicators (peasant population, furloughed, GDP/capita) to telemetry — these are aggregate national statistics, not Fog-of-War-gated military data. Existing FoW enforcement is unchanged. |
| Full-Stack Accountability | PASS | Pillar 5 explicitly plans the full stack: `TelemetrySample` (storage) → `TelemetryDeltas` (computation) → `MacroIndicatorsResponse` (DTO) → `MacroPage.tsx` (UI). Pillar 2 adds `wage_expense` to `financial_history`, which is already surfaced via existing Finance page DTOs. Pillar 3 bank types are visible via existing banking queries. Pillar 4 veins are visible via existing Resources tab. |
| Complete Entity Lifecycle | PASS | Pillar 3: Cooperative banks are `Company` entities with `BankType::Cooperative`. They inherit the full company lifecycle — birth (generation), operation (turn processing), insolvency (bankruptcy.rs), and liquidation. No immortal structures. Pillar 4: Veins have `current_reserves` that deplete as mines extract. When reserves hit zero, the vein is exhausted (existing behavior). Mining companies bound to exhausted veins face reduced output and eventual bankruptcy. |
| Market Forces | PASS with caveat | Pillar 3: Bank type distribution uses weighted random selection (40/30/20/10), not a hardcoded market split. This represents structural market composition, not command-economy allocation. Loans are still assigned randomly among eligible banks (existing `rng.gen_range` at line 918). Caveat: The 40/30/20/10 weights are design parameters, not market-clearing outcomes. This is acceptable for world generation (initial conditions) — the market then operates competitively during simulation. |
| Rational Actors | PASS | No charity or debt forgiveness is introduced. Working Capital Loans are legitimate debt with interest (`xibor + bank_margin + risk_premium`). Companies must repay (existing loan repayment logic). Wage arrears are liabilities that companies must eventually pay (existing 30%/turn repayment logic). Cooperative banks are rational profit-seeking (or member-benefit-seeking) entities. No irrational behavior introduced. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None

All 13 directives pass. The plan is ready for user approval.
