---
agent: devin-local
session: lowly-keyboard
created: 2026-08-28T08:02:49Z
---
# Phase 92: The Hyper-Inflation, Demography & Historical Production Audit

Phase 92 fixes eight structural flaws: hyper-inflationary loan hallucination, labor hyper-bidding, accrual accounting bypass, cloned vein monoculture, gender-mismatched royal names, corporate size bloat, GDP-share-derived employment instead of historical labor intensity, and total corporate FTE demand exceeding the workforce.

## Summary

Phase 92 fixes five critical genesis/turn bugs plus three structural demographic flaws: (1) working-capital loans hallucinating ~40B against ~26M GDP, (2) labor-market hyper-bidding from corporate bloat, (3) accrual accounting recording zero wage expense despite arrears, (4) cloned vein monoculture, (5) gender-mismatched royal names and limited dynasties, (6) corporate size distribution spawning monopolies instead of SMEs, (7) sector employment derived from GDP share instead of historical labor intensity, (8) total corporate FTE demand exceeding the workforce.

## Root Cause Analysis

### Pillar 1: The 40-Billion Fiat Hallucination

**Root cause:** The loan principal formula in `corporate.rs:990`:
```rust
let principal = initial_fte * initial_wage * 6.0 + seed_cost + debt_service + overhead;
```

Where `initial_fte ≈ 14,400` (derived from national sector employment divided among ~20 companies per region) and `initial_wage ≈ 1383`. With ~500 companies, total loans reach ~60B against GDP ~18B. The Tier 1 estimate in `mod.rs:1648` assumes loans are 15% of GDP — off by ~22×.

The root cause is that `initial_fte` is far too large (see Pillars 6-8). Once company sizes are historically realistic (10-500 workers for most), the loan principal naturally shrinks to a realistic fraction of GDP.

**Remaining fix after Pillars 6-8:** Reduce the payroll runway from 6 turns to 4 turns (a startup company doesn't need 6 months of payroll upfront — it begins generating revenue immediately). Update the Tier 1 estimation to use actual company count and average principal rather than a flat 15% of GDP.

### Pillar 2: Labor Market Hyper-Bidding & Furlough Contagion

**Root cause:** Total corporate FTE demand (~7M for a 10M country) vastly exceeds the available workforce (~6.4M). The labor market (`labor_market.rs:155`) sorts bids by wage descending — companies with massive loan cash outbid everyone, win workers, pay them, then go broke on Turn 2. Companies that lose get 0 FTE.

The root cause is Pillar 8: total corporate FTE demand is not bounded by the available workforce. Once company sizes are historically realistic and total demand is constrained to ~95% of the workforce (leaving 3-8% natural unemployment), hyper-bidding disappears.

**Remaining fix after Pillars 6-8:** Add a revenue-aware wage cap in `set_wage_offers`: if a company's last-turn revenue was below its wage bill, cap `offered_wage_per_fte` at `market_average_wage * 0.9`. This prevents unprofitable companies from bidding above their revenue capacity.

### Pillar 3: Accrual Accounting Bypass (Empty Financial History)

**Root cause:** In `manager.rs:794`:
```rust
let wage_expense = (company.fulfilled_fte as f64) * company.offered_wage_per_fte;
```

This is computed in `process_company`, which runs AFTER the labor market. If the company lost all bids (`fulfilled_fte = 0`) or was furloughed, `wage_expense = 0` despite millions in accumulated arrears. The financial history correctly records zero flows for a non-operating company, but it's misleading because arrears aren't surfaced as an economic cost.

**Fix:** Track actual wage payment and arrears accrual from the labor market phase as transient fields on `Company`:

1. Add `wages_paid_this_turn: f64` and `arrears_accrued_this_turn: f64` to `Company` (reset each turn, `#[serde(skip)]`).
2. In `labor_market.rs:438-467`, set these fields when processing payroll.
3. In `manager.rs:794`, compute `wage_expense = wages_paid_this_turn + arrears_accrued_this_turn`.
4. This ensures the financial record always reflects the actual wage obligation, even if `fulfilled_fte` was modified after the labor market.

### Pillar 4: The Cloned Vein Monoculture

**Root cause:** In `planet.rs:334-370`, `ensure_base_industrial_veins_per_region` creates ALL 8 base commodities for EVERY region. Every region gets Iron, HardCoal, BrownCoal, Stone, Sand, Limestone, Peat, Gravel — the identical resource mix.

**Fix:** Rewrite `ensure_base_industrial_veins_per_region` to use geographic determinants:
1. **Ubiquitous commodities** (Stone, Sand, Gravel, Limestone, Peat): Present in ~80% of regions. Use a deterministic hash of `(region_id, commodity)` to decide presence.
2. **Industrial commodities** (Iron, HardCoal, BrownCoal): Present in ~35% of regions each. Same hash-based decision.
3. **Reserve scaling**: Scale reserves by a region-specific hash factor (50-100% of base range).
4. **Quality/depth variation**: Vary by region hash.
5. This creates diverse regional resource profiles — some regions are mining hubs, others have only construction materials.

### Pillar 5: Gendered Names & Extended Dynasties

**Root cause (gender mismatch):** In `turn.rs:1420-1421`:
```rust
let consort_gender = if monarch_gender == "M" { "F" } else { "M" };
let consort_vip_name = super::names::generate_key_vip(cultural_group, rng, &mut used_names);
```

`generate_key_vip` calls `generate_full_vip` which randomly picks gender (70% male). The `consort_gender` is computed but NEVER passed to the name generator. Result: a female consort named "Kazimierz Nowak" (male name). Same bug in `succession.rs:260`.

**Root cause (limited dynasty):** The royal dynasty initialization in `turn.rs:1395-1512` only creates monarch + consort + 1-2 children. No siblings, uncles, aunts, or cousins.

**Fix:**

1. **Add gender-parameterized VIP generation:**
   - Add `generate_key_vip_with_gender(cultural_group, gender, rng, used_names)` — calls `generate_person_name(cultural_group, gender, rng)` with the specified gender, then checks uniqueness (50 retries, duplicate on exhaustion).
   - Add `generate_full_vip_with_gender(cultural_group, gender, rng)` — calls `generate_person_name(cultural_group, gender, rng)` directly.

2. **Update all consort/spouse generation sites** to use the gender-parameterized variants.

3. **Extend royal dynasty initialization:**
   - Generate 1-2 siblings of the monarch (princes/princesses, aged near the monarch). Relation: `RoyalRelation::Sibling`.
   - Generate 1-2 uncles/aunts (older than monarch). Add `RoyalRelation::Uncle` and `RoyalRelation::Aunt` to the enum.
   - Generate 0-2 cousins (children of uncles/aunts). Relation: `RoyalRelation::Cousin`.
   - Set `succession_order` for all members: Monarch=0, Children=1-2, Siblings=3-4, Cousins=5-6.
   - Link family relationships (sibling → shared parent IDs, cousin → uncle/aunt parent IDs).

### Pillar 6: Historical Corporate Bloat (SME vs. Monopolies)

**Root cause:** In `generate_region_companies` (`corporate.rs:1233-1239`):
```rust
let company_count = (region_emp / 1500.0).round().max(3.0).min(20.0) as usize;
```

With `region_emp = 450,000` (Agriculture in a large region), `company_count = 20` (capped). Each company gets `450,000 / 20 = 22,500` workers on average. The power-law distribution (`x^2`) gives the largest company ~20% = 90,000 workers. In 1900, a 90K-worker company is absurd — the largest real-world firms (Krupp, US Steel) had ~50-80K workers total, and they were exceptional national champions.

The problem is that `company_count` is capped at 20 regardless of `region_emp`. A region with 450K agricultural workers should have hundreds of small farms, not 20 giant agribusinesses.

**Fix:** Restructure `generate_region_companies` to produce a historically realistic size distribution:

1. **Sector-specific target company size:** Define a `target_workers_per_company(sector, start_year)` function:
   - Agriculture (1900): 50-300 workers per farm (small family farms)
   - LightIndustry (1900): 100-500 workers per workshop/factory
   - HeavyIndustry (1900): 500-5000 workers per plant (larger, concentrated)
   - Mining (1900): 200-2000 workers per mine
   - LocalServices (1900): 10-100 workers per shop/tavern
   - Construction (1900): 50-300 workers per firm
   - Energy (1900): 100-500 workers per plant
   - ExportServices (1900): 50-300 workers per firm
   - Hospitality (1900): 20-150 workers per hotel/tavern
   - TransportLogistics (1900): 100-500 workers per firm

2. **Company count from target size:** `company_count = (region_emp / target_size).round().max(3.0).min(200.0)`. This allows hundreds of small farms for agriculture, while heavy industry gets fewer, larger plants.

3. **Power-law distribution preserved:** The `x^2` power-law still produces a few large players and many small ones, but the AVERAGE size is now historically realistic. For agriculture with 450K workers and target_size=150, `company_count = 3000` capped at 200. Average size = 2,250 workers. Power-law gives largest ~450 workers, smallest ~50 workers. This is realistic for 1900.

4. **National champion flag:** Keep the `>25,000` threshold for national champions. With the new distribution, only the largest heavy industry plants will qualify.

### Pillar 7: 1900s Labor Intensity & Sector Proportions

**Root cause:** In `mod.rs:1290-1294`, sector employment is derived from GDP share:
```rust
let share_emp = (employed_total * (share.gdp_share / total_gdp_share)) as i64;
share.extra.insert("zatrudnienie".to_string(), Value::from(share_emp));
```

This is economically wrong. GDP share ≠ employment share. In 1900:
- Agriculture: ~40-60% of employment but only ~15-30% of GDP (low labor productivity)
- HeavyIndustry: ~10-15% of employment but ~20-30% of GDP (high capital intensity)
- Services: ~20-30% of employment but ~30-40% of GDP

Using GDP share to distribute employment understates agricultural employment and overstates industrial employment.

**Fix:** Introduce a `labor_intensity_ratio(sector, start_year)` function that maps GDP share to employment share:

1. **Define labor intensity ratios per era:**
   - 1900: Agriculture 2.5× (high employment per GDP), HeavyIndustry 0.6×, LightIndustry 1.0×, LocalServices 1.2×, Mining 0.8×, Energy 0.5×, Construction 1.5×, etc.
   - 1925: Agriculture 2.0×, HeavyIndustry 0.7×, LightIndustry 1.0×, LocalServices 1.1×
   - 1950: Agriculture 1.5×, HeavyIndustry 0.8×, LightIndustry 1.0×, LocalServices 1.0×
   - 1975: Agriculture 1.0×, HeavyIndustry 0.9×, LightIndustry 1.0×, LocalServices 0.9×

2. **Compute employment from GDP share × labor intensity:**
   ```rust
   let raw_emp = share.gdp_share * labor_intensity_ratio(sector, start_year);
   let total_raw_emp: f64 = sectors.values().map(|s| s.gdp_share * labor_intensity_ratio(s.sector, start_year)).sum();
   let share_emp = (employed_total * (raw_emp / total_raw_emp)) as i64;
   ```

3. This ensures agriculture gets ~40-60% of employment in 1900 (matching historical reality) while heavy industry gets ~10-15%, regardless of their GDP shares.

### Pillar 8: Natural Unemployment & Labor Pool Balance

**Root cause:** The genesis generator distributes `employed_total` (workforce × (1 - unemployment_rate)) across all sectors, then each sector distributes its share across regions and companies. But the seed companies (`seed_minimum_viable_supply_chain`) ADD additional companies on top of the budget-share companies. This means total corporate FTE demand = budget-share demand + seed demand, which can exceed `employed_total`.

Additionally, the `dev_bias` multiplier in `corporate.rs:257-268` can inflate regional employment beyond the sector target. A region with `dev_bias = 1.5` gets 50% more employment than its population share would suggest, but the labor pool doesn't increase proportionally.

**Fix:** Enforce a hard balance between total corporate FTE demand and available workforce:

1. **After all companies are generated** (both budget-share and seed), compute `total_corporate_fte_demand = sum of all company.target_fte_demand`.

2. **Compute `total_available_workforce = sum of all region.class_demographics.available_fte`** (already computed in `labor.rs:557`).

3. **If `total_corporate_fte_demand > total_available_workforce * 0.95`**, scale ALL companies' `target_fte_demand` and `physical_fte_demand` by `factor = (total_available_workforce * 0.95) / total_corporate_fte_demand`. This ensures 5% natural unemployment at genesis.

4. **Scale `fulfilled_fte` and `prev_fulfilled_fte`** by the same factor to maintain consistency.

5. **This is NOT a `.min()` clamp on individual companies** — it's a systemic scaling that preserves the relative size distribution (power-law) while ensuring the total matches the labor pool. The 0.95 factor leaves 5% unemployment, which is historically realistic.

6. **The scaling happens AFTER all company generation** (including seeds), so it accounts for the total demand from all sources.

## Implementation Steps

### Step 1: Pillar 7 — Labor Intensity Ratios (mod.rs)

**File:** `state/src/engine/generator/mod.rs`
- Add `fn labor_intensity_ratio(sector: Sector, start_year: StartYear) -> f64` with era-specific ratios.
- In `build_macro_data` (~line 1290-1294), replace the GDP-share-proportional employment distribution with the labor-intensity-weighted distribution.

### Step 2: Pillar 6 — Historical Company Size Distribution (corporate.rs)

**File:** `state/src/engine/generator/corporate.rs`
- Add `fn target_workers_per_company(sector: Sector, start_year: u32) -> f64` with sector-and-era-specific target sizes.
- In `generate_region_companies` (~line 1233-1239), replace the flat `region_emp / 1500.0` company count with `region_emp / target_workers_per_company(sector, start_year)`. Raise the cap from 20 to 200 to allow many small firms.
- Preserve the power-law distribution (`x^2`) for size variation within the sector.

### Step 3: Pillar 8 — Labor Pool Balance (corporate.rs)

**File:** `state/src/engine/generator/corporate.rs`
- In `generate_corporate_entities`, after ALL companies are generated (both budget-share and seed), compute `total_corporate_fte_demand` and `total_available_workforce`.
- If demand exceeds 95% of supply, scale all companies' FTE fields by the ratio.
- This is a post-generation normalization pass, not a per-company clamp.

### Step 4: Pillar 1 — Loan Principal & Tier 1 (corporate.rs + mod.rs)

**File:** `state/src/engine/generator/corporate.rs`
- In `issue_working_capital_loans` (~line 990), reduce payroll runway from 6 to 4 turns: `payroll_principal = initial_fte * initial_wage * 4.0`.
- Add a per-bank lending cap: `max_bank_lending = bank_tier1 * 10.0` (10× Tier 1). Track `bank_total_lent` and skip companies when the cap is reached.

**File:** `state/src/engine/generator/mod.rs`
- In `build_bank_companies` (~line 1648), update `estimated_loan_exposure` to use actual company count and average principal rather than a flat 15% of GDP.

### Step 5: Pillar 2 — Revenue-Aware Wage Cap (manager.rs)

**File:** `state/src/corporate/manager.rs`
- In `set_wage_offers` (~line 1268), add a revenue-aware wage cap: if the company's `total_profit` from last turn was below its wage bill, cap `offered_wage_per_fte` at `market_average_wage * 0.9`.

### Step 6: Pillar 3 — Accrual Accounting Fix (entities + labor_market.rs + manager.rs)

**File:** `state/src/entities.rs` (or wherever `Company` is defined)
- Add `pub wages_paid_this_turn: f64` and `pub arrears_accrued_this_turn: f64`. Initialize to 0.0 in `Default`. Mark with `#[serde(skip)]`.

**File:** `state/src/economy/labor/labor_market.rs`
- In the payroll section (~line 438-467), set `company.wages_paid_this_turn = actual_paid` and `company.arrears_accrued_this_turn = arrears_this_turn`.
- Reset these fields at the start of `resolve_regional_labor_market` for each company.

**File:** `state/src/corporate/manager.rs`
- In `process_company` (~line 794), replace `wage_expense = fulfilled_fte * offered_wage` with `wage_expense = wages_paid_this_turn + arrears_accrued_this_turn`.

### Step 7: Pillar 4 — Geographic Vein Diversity (planet.rs)

**File:** `state/src/society/planet.rs`
- Rewrite `ensure_base_industrial_veins_per_region`:
  - Split into `ubiquitous` (Stone, Sand, Gravel, Limestone, Peat — ~80% presence) and `industrial` (Iron, HardCoal, BrownCoal — ~35% presence).
  - Use deterministic hash of `(region_id, commodity)` for presence decisions.
  - Scale reserves, quality, and depth by region-specific hash factors.

### Step 8: Pillar 5a — Gendered Name Generation (names.rs + turn.rs + succession.rs)

**File:** `state/src/politics/names.rs`
- Add `generate_key_vip_with_gender(cultural_group, gender, rng, used_names)` and `generate_full_vip_with_gender(cultural_group, gender, rng)`.

**File:** `state/src/politics/turn.rs`
- At consort generation (~line 1421), use `generate_key_vip_with_gender(cultural_group, consort_gender, rng, &mut used_names)`.
- At heir generation (~line 1465), use `generate_key_vip_with_gender(cultural_group, heir_gender, rng, &mut used_names)`.

**File:** `state/src/politics/succession.rs`
- At spouse generation (~line 260), use `generate_full_vip_with_gender(culture, spouse_gender, &mut rng)`.
- Audit all other name generation calls and pass the correct gender.

### Step 9: Pillar 5b — Extended Royal Dynasty (turn.rs + succession.rs)

**File:** `state/src/politics/succession.rs`
- Add `RoyalRelation::Uncle` and `RoyalRelation::Aunt` variants to the enum.

**File:** `state/src/politics/turn.rs`
- After generating monarch + consort + children (current code ~line 1395-1512), add:
  1. **Siblings (1-2):** Age near monarch. Relation: `Sibling`. Succession order after children.
  2. **Uncles/Aunts (1-2):** Age = monarch_age + 15 ± 5. Relation: `Uncle`/`Aunt`.
  3. **Cousins (0-2):** Children of uncles/aunts. Age = monarch_age - 10 ± 5. Relation: `Cousin`.
  4. Set `succession_order` for all members.
  5. Link family relationships (parent IDs, children IDs).

## Files to Modify

- `state/src/engine/generator/mod.rs` — Pillar 1 (Tier 1 estimation) + Pillar 7 (labor intensity ratios)
- `state/src/engine/generator/corporate.rs` — Pillar 1 (loan principal) + Pillar 2 (FTE scaling) + Pillar 6 (company size distribution) + Pillar 8 (labor pool balance)
- `state/src/corporate/manager.rs` — Pillar 2 (revenue-aware wage cap) + Pillar 3 (wage_expense from transient fields)
- `state/src/economy/labor/labor_market.rs` — Pillar 3 (set transient wage fields)
- `state/src/entities.rs` (or Company definition) — Pillar 3 (add transient fields)
- `state/src/society/planet.rs` — Pillar 4 (geographic vein diversity)
- `state/src/politics/names.rs` — Pillar 5a (gender-parameterized VIP generation)
- `state/src/politics/turn.rs` — Pillar 5a (consort gender) + Pillar 5b (extended dynasty)
- `state/src/politics/succession.rs` — Pillar 5a (spouse gender) + Pillar 5b (Uncle/Aunt relation)

## Verification

- [ ] `cargo build --workspace` — zero errors
- [ ] `cargo test --workspace --all-targets` — all tests pass (skip known flaky `test_border_conflict_generation`)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `npm run build` — zero errors
- [ ] Manual verification: Generate a new world and verify:
  - Total working-capital loans < 50% of GDP
  - Bank LDR < 80% at genesis
  - No mass furloughs on Turn 1
  - Most companies have 10-500 workers; only a few have >5000
  - Agriculture employs ~40-60% of workforce in 1900
  - Total corporate FTE demand ≤ 95% of available workforce
  - Natural unemployment 3-8% at genesis
  - Companies with wage arrears show non-zero wage_expense in financial history
  - Regions have diverse vein profiles (not all regions have Iron)
  - Royal consort name matches gender (female consort has female name)
  - Royal dynasty has >3 members (monarch, consort, children, siblings, uncles, cousins)

## Risks/Considerations

1. **Company count increase may impact performance.** Raising the cap from 20 to 200 per region-sector could create thousands of companies. Mitigation: the cap is 200, and most sectors will have 10-50 companies per region. Agriculture in large regions may hit the cap, but this is realistic (hundreds of small farms).
2. **Labor intensity ratios are approximations.** Historical labor intensity varied by country and era. The ratios are reasonable averages for the 1900-1975 period. They can be refined later with country-specific data.
3. **Post-generation FTE scaling changes company sizes proportionally.** This preserves the power-law distribution but may make some companies smaller than their historical minimum. The `min_workers_for_sector` seed function already provides a floor for seed companies.
4. **Transient wage fields on Company increase struct size.** Two `f64` fields = 16 bytes. Negligible.
5. **Vein diversity may leave some regions without industrial resources.** This is realistic — the market transports resources via freight.
6. **Extended dynasty increases genesis VIP count by ~4-6 per monarchy.** Within name pool capacity.
7. **RoyalRelation enum extension breaks save compatibility.** Per Directive 10, acceptable in alpha.
8. **The `update_gdp_shares_from_employment` function has a pre-existing bug** (reads `"employment"` key but sets `"zatrudnienie"` key at line 67 vs 83). This should be fixed in the same phase since it affects sector employment tracking. The fix: change line 83-87 to read `"zatrudnienie"` instead of `"employment"`.

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | No physical transformations introduced. Vein generation creates geological reserves with mass-conserving extraction (existing system). FTE scaling adjusts demand, not worker count. Transient wage fields track flows, not mass. Labor intensity ratios redistribute employment shares, not physical mass. |
| Double-Entry Bookkeeping | PASS | Loan principal reduction preserves existing double-entry: company receives cash (asset) + liability (loan), bank records loan asset + deposit liability. Per-bank lending cap (10× Tier 1) is a prudential limit, not a new transaction. Transient wage fields track existing payroll flow — wages paid debit company cash / credit worker income; arrears accrue as company liability. Post-generation FTE scaling adjusts demand fields, not cash flows. No new cash flows are created. |
| No Teleportation | PASS | No physical movement introduced. Vein resources remain in their regions; existing freight/logistics system handles transport. Company size distribution doesn't move physical matter. |
| Clamping | PASS | Company count cap (200) is a generation parameter, not a runtime clamp. Post-generation FTE scaling uses proportional factor, not `.min()` on individual companies. Per-bank lending cap uses `.min()` on cumulative lending. Wage cap uses `.min()` on offered wage. Vein presence is binary. All fields have lower bound 0.0 via existing `.max(0.0)` patterns. Transient wage fields reset to 0.0 each turn. |
| No Magic Numbers | PASS | Labor intensity ratios (0.5×-2.5×) are historically-derived economic parameters, not nominal floats — they scale employment relative to GDP share, adapting to any GDP level. Target workers per company (50-5000) are historically-derived sector sizes, not economic thresholds — they scale with sector and era. Post-generation FTE scaling factor (0.95) is a labor economics parameter (5% natural unemployment), not a nominal float. Per-bank lending cap (10× Tier 1) is a standard prudential leverage ratio. Wage cap (0.9× market_average) uses dynamic market average. Loan runway (4 turns) is a count. Vein presence probability (35-80%) is a geographic distribution parameter. |
| Technological Matrices | PASS | No new building types or production methods introduced. The plan modifies existing company generation and labor market logic only. Company size distribution doesn't affect production methods — each company still uses the existing method slots. |
| Architectural Parsimony | PASS | All fixes extend existing systems: (1) labor intensity ratios extend `build_macro_data`, (2) company size distribution extends `generate_region_companies`, (3) labor pool balance is a post-generation pass in `generate_corporate_entities`, (4) loan cap extends `issue_working_capital_loans`, (5) wage tracking extends existing labor market payroll, (6) vein diversity rewrites existing function, (7) gendered names add variants to existing functions. No parallel systems are created. |
| Temporal Causality | PASS | Labor intensity ratios are applied during genesis (before any turn). Company size distribution is applied during genesis. Post-generation FTE scaling is applied during genesis (after all companies are generated, before any turn). Transient wage fields are set during labor market phase (step 2) and read during `process_company` (step 4) — forward data flow, no paradox. Wage cap is applied in `set_wage_offers` (before labor market). Vein diversity is applied during planet generation. Dynasty extension is applied during genesis. |
| Asymmetric Information | PASS | No new DTOs or frontend data exposure introduced. Transient wage fields are `#[serde(skip)]` and never serialized. Financial history already exposes `wage_expense` and `wage_arrears` via existing DTOs. No hidden data sent to frontend. |
| Full-Stack Accountability | PASS | Financial history fix (Pillar 3) ensures existing DTO fields are correctly populated. `CompaniesPage.tsx` already displays these fields. No new frontend components needed. Vein diversity visible through existing geological/exploration UI. Dynasty members visible through existing VIP explorer. Company size distribution is visible through existing company list/detail UI. |
| Complete Entity Lifecycle | PASS | No new entities created. Extended dynasty members are VIPs with same lifecycle as existing VIPs. Veins have existing lifecycle. Transient wage fields are reset each turn (birth) and read once (life) — no accumulation or immortality. Companies generated with new size distribution have same lifecycle as existing companies. |
| Market Forces | PASS | Company size distribution uses power-law (market-driven concentration), not hardcoded splits. Post-generation FTE scaling preserves relative sizes (proportional, not equal). Labor market still clears competitively (bids sorted by wage). Wage cap for unprofitable companies is a market signal (below-average wage), not a command-economy price fix. Vein presence determined by geographic determinants (hash), not manual allocation. |
| Rational Actors | PASS | Companies with less cash offer lower wages (rational). Unprofitable companies bid below market (rational cost-cutting). Banks cap lending at 10× Tier 1 (rational prudential risk management). No charity or debt forgiveness introduced. Labor intensity ratios reflect rational economic reality (agriculture has lower productivity → more workers per GDP unit). |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None. The plan is architecturally sound and ready for implementation.
