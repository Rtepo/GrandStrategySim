---
agent: devin-local
session: lowly-keyboard
created: 2026-08-28T08:02:49Z
---
# Phase 92: The Hyper-Inflation & Demography Audit

Phase 92 fixes five critical genesis and turn-processing bugs that cause hyper-inflationary loan hallucination, labor market collapse, empty financial histories, geological monoculture, and gender-mismatched royal dynasties.

## Summary

Phase 92 fixes five critical genesis and turn-processing bugs: (1) working-capital loan principals that hallucinate ~40B against a ~26M GDP, (2) labor-market hyper-bidding that bankrupts companies in one turn, (3) accrual accounting that records zero wage expense despite millions in arrears, (4) a cloned vein monoculture that gives every region identical resources, and (5) gender-mismatched royal consort names and missing extended dynasty members.

## Root Cause Analysis

### Pillar 1: The 40-Billion Fiat Hallucination

**Root cause:** The loan principal formula in `corporate.rs:990` is:
```rust
let principal = initial_fte * initial_wage * 6.0 + seed_cost + debt_service + overhead;
```

Where:
- `initial_fte = (actual_capacity * 0.6).round().max(2.0)` — `actual_capacity` derives from `target_emp` (sector employment distributed from the national workforce). For a 10M-population country, a single sector can have ~600K target employment, distributed across ~5 companies per region × ~5 regions = 25 companies, each with ~24,000 capacity → `initial_fte ≈ 14,400`.
- `initial_wage = (company_liquid * 0.6 / actual_capacity).max(50.0)` — `company_liquid` derives from `sector_liquid = sector_fixed * 0.4 = (target_emp * base_wage * 2.0) * 0.4`. With `base_wage = gdp_pc * 800 ≈ 1440`, `sector_liquid ≈ 691M`, per company ≈ 27.6M → `initial_wage ≈ 1383`.
- Principal per company ≈ `14,400 * 1383 * 6.0 + ... ≈ 119M + reserves`.
- With ~500 companies nationally, total loans ≈ **60B**, against GDP ≈ **18B**.

The Tier 1 capital sizing in `mod.rs:1648` estimates `loan_exposure = gdp * 0.15 / num_banks`, but actual loans are ~3.3× GDP, not 15% of GDP. The estimate is off by a factor of ~22.

**Fix:** The fundamental problem is that `initial_fte` is calibrated to the NATIONAL workforce distribution, but the loan formula multiplies it by 6 turns of wages, producing a principal that exceeds annual GDP. The fix has two parts:

1. **Cap total working-capital lending per bank to a fraction of its Tier 1 capital** (e.g., 10× Tier 1, consistent with a 10% LDR cap). This is a prudential lending limit, not a magic number — it's the standard leverage cap.
2. **Scale `initial_fte` to the regional labor pool, not the national sector employment.** Currently `actual_capacity` is derived from `target_emp` which is the national sector employment divided among regions. But the regional labor pool is much smaller. The company should not demand more workers than the regional labor pool can supply.

### Pillar 2: Labor Market Hyper-Bidding & Furlough Contagion

**Root cause:** Total corporate FTE demand vastly exceeds the available regional labor pool. With ~500 companies each demanding ~14,000 FTE, total demand is ~7M workers. But the regional labor pool (population × labor_participation × 1.5) is much smaller — a region with 2M population might have ~1.5M available FTE across all classes.

The labor market (`labor_market.rs:155`) sorts bids by wage descending. Companies with massive loan-funded cash offer high wages, win workers, and pay them. Companies that lose bids get 0 FTE. The retention floor (`FTE_RETENTION_FLOOR = 0.90`) only guarantees the BID amount, not allocation.

On Turn 2, companies that won bids on Turn 1 have spent their cash on wages. Their `set_wage_offers` computes a lower wage (less cash per FTE). They lose bids to companies that still have cash. They get 0 FTE, accrue no new wages, and their financial history shows zeros.

**Fix:** The root cause is the same as Pillar 1: companies are generated with FTE capacity that exceeds the regional labor pool. The fix is to scale `actual_capacity` (and thus `target_fte_demand`) to a fraction of the regional available FTE, not the national sector employment. This ensures total corporate demand is bounded by regional labor supply.

Additionally, add a **wage-bidding cap relative to company revenue**. Currently `set_wage_offers` caps wages at `3× market_average`, but a company with zero revenue can still bid at this cap if it has loan cash. The cap should also consider the company's revenue-to-payroll ratio.

### Pillar 3: Accrual Accounting Bypass (Empty Financial History)

**Root cause:** In `manager.rs:794`:
```rust
let wage_expense = (company.fulfilled_fte as f64) * company.offered_wage_per_fte;
```

This is computed in `process_company`, which runs AFTER the labor market. If the company lost all labor bids (`fulfilled_fte = 0`), `wage_expense = 0`. If the company also has no building revenue (`total_profit = 0`), then:
- `revenue = 0 + 0 = 0`
- `operating_costs = 0 + 0 = 0`
- `wage_expense = 0`
- `net_profit = 0 - 0 = 0`

The `wage_arrears` field shows the cumulative balance but is not reflected in `wage_expense` or `net_profit`. The financial history correctly records zero flows for a company that didn't operate, but it's misleading because it doesn't surface the arrears as an economic cost.

**The deeper bug:** Even on Turn 1 (when the company DID have workers), the `wage_expense` might be zero if the company was furloughed before `process_company` runs. The furlough path sets `fulfilled_fte` to standby levels, but if the standby level is 0 (no active crops/seasonal work), `wage_expense = 0` despite the company having accrued arrears during the labor market phase.

**Fix:** Track the actual wage payment and arrears accrual from the labor market phase as transient fields on `Company`, and use these in `process_company` instead of recomputing `fulfilled_fte * offered_wage`:

1. Add `wages_paid_this_turn: f64` and `arrears_accrued_this_turn: f64` to `Company` (reset each turn).
2. In `labor_market.rs:438-467`, set these fields when processing payroll.
3. In `manager.rs:794`, compute `wage_expense = wages_paid_this_turn + arrears_accrued_this_turn`.
4. This ensures the financial record always reflects the actual wage obligation, even if `fulfilled_fte` was modified after the labor market.

### Pillar 4: The Cloned Vein Monoculture

**Root cause:** In `planet.rs:334-370`, `ensure_base_industrial_veins_per_region` iterates over ALL populated regions and creates ALL 8 base commodities for EACH region:

```rust
for (region_id, lat, lon) in populated_regions {
    for &(commodity, tier) in base_commodities {
        // ... create a vein with overlapping_regions = vec![region_id.clone()]
    }
}
```

Every region gets Iron, HardCoal, BrownCoal, Stone, Sand, Limestone, Peat, Gravel — the exact same set. The veins are spatially isolated (each only overlaps one region), but the COMMODITY PROFILE is identical everywhere. This is the "monoculture" — not that the veins are shared, but that every region has the same resource mix.

**Fix:** Rewrite `ensure_base_industrial_veins_per_region` to use geographic determinants:
1. **Ubiquitous commodities** (Stone, Sand, Gravel, Limestone, Peat): These are genuinely common. Keep them in most regions but with varying quality and reserves based on terrain.
2. **Industrial commodities** (Iron, HardCoal, BrownCoal): These should be geographically concentrated. Use a hash of `(region_id, commodity)` to deterministically decide presence (~40% chance for Iron, ~50% for coal types). Only regions with the right geological profile get them.
3. **Reserve scaling**: Scale reserves by region area or terrain, not a flat random range.
4. This creates diverse regional resource profiles — some regions are mining hubs, others have only construction materials.

### Pillar 5: Gendered Names & Extended Dynasties

**Root cause (gender mismatch):** In `turn.rs:1420-1421`:
```rust
let consort_gender = if monarch_gender == "M" { "F" } else { "M" };
let consort_vip_name = super::names::generate_key_vip(cultural_group, rng, &mut used_names);
```

`generate_key_vip` calls `generate_full_vip` which randomly picks gender (70% male). The `consort_gender` is computed but NEVER passed to the name generator. The VIP record uses `consort_gender` for the `gender` field, but the `full_name` comes from a name generated with a random gender. Result: a female consort named "Kazimierz Nowak" (male name).

The same bug exists in `succession.rs:260`:
```rust
let spouse_name = crate::politics::names::generate_full_vip(culture, &mut rng);
let spouse_gender = if registry.get(&monarch_id).map(|v| v.gender.as_str()).unwrap_or("M") == "M" { "F" } else { "M" };
```

**Root cause (limited dynasty):** The royal dynasty initialization in `turn.rs:1395-1512` only creates:
- 1 Monarch
- 1 Consort
- 1-2 Children (heirs)

No siblings, uncles, aunts, or cousins. The `RoyalRelation` enum already has `Sibling` and `Cousin` variants, but they're never used during genesis.

**Fix:**

1. **Add gender-parameterized VIP generation:**
   - Add `generate_person_name_with_gender(cultural_group, gender, rng, used_names)` — a gender-aware variant of `generate_key_vip` that calls `generate_person_name` with the specified gender.
   - Update all consort/spouse generation sites to use this function.
   - Keep `generate_key_vip` and `generate_full_vip` for cases where gender is random.

2. **Extend royal dynasty initialization:**
   - Generate 1-2 siblings of the monarch (princes/princesses, aged near the monarch).
   - Generate 1-2 uncles/aunts (older than monarch, siblings of the monarch's hypothetical parent).
   - Generate 0-2 cousins (children of uncles/aunts).
   - Assign appropriate `RoyalRelation` values (`Sibling`, `Cousin`).
   - Set `succession_order` for each member (monarch=0, children=1-2, siblings=3-4, cousins=5-6).
   - Link family relationships (sibling → shared parent IDs, cousin → uncle/aunt parent IDs).

## Implementation Steps

### Step 1: Pillar 1 — Fix Loan Principal Scale (corporate.rs + mod.rs)

**File:** `state/src/engine/generator/corporate.rs`
- In `generate_region_companies` (~line 1279), cap `actual_capacity` to a fraction of the regional available FTE. Compute `regional_labor_pool = sum of region.class_demographics.available_fte`. Set `actual_capacity = actual_capacity.min((regional_labor_pool * 0.4) as u32)` — no single company should demand more than 40% of the regional labor pool.
- In `issue_working_capital_loans` (~line 968), add a per-bank lending cap: `max_bank_lending = bank_tier1 * 10.0` (10× Tier 1, consistent with 10% leverage). Track `bank_total_lent` and skip companies when the cap is reached.
- Reduce the payroll runway from 6 turns to 4 turns: `payroll_principal = initial_fte * initial_wage * 4.0`. This aligns the loan with a realistic startup runway while the company builds revenue.

**File:** `state/src/engine/generator/mod.rs`
- In `build_bank_companies` (~line 1648), update `estimated_loan_exposure` to use the new per-company cap: `estimated_loan_exposure = num_companies_per_bank * avg_principal`. Compute `avg_principal` from the actual company data, not a flat 15% of GDP.

### Step 2: Pillar 2 — Labor Market FTE Scaling (corporate.rs + manager.rs)

**File:** `state/src/engine/generator/corporate.rs`
- In all seed company constructors (lines ~1411, ~2616, ~3063, ~4223), set `target_fte_demand` and `physical_fte_demand` to the CAPPED `actual_capacity` from Step 1. This ensures total corporate FTE demand is bounded by regional labor supply.

**File:** `state/src/corporate/manager.rs`
- In `set_wage_offers` (~line 1268), add a revenue-aware wage cap: if the company's `total_profit` from last turn was below its wage bill, cap `offered_wage_per_fte` at `market_average_wage * 0.9` (below-market wage for unprofitable companies). This prevents companies from bidding above their revenue capacity.

### Step 3: Pillar 3 — Accrual Accounting Fix (Company struct + labor_market.rs + manager.rs)

**File:** `state/src/entities.rs` (or wherever `Company` is defined)
- Add transient fields: `pub wages_paid_this_turn: f64` and `pub arrears_accrued_this_turn: f64`. Initialize to 0.0 in `Default`. Mark with `#[serde(skip)]` since they're transient.

**File:** `state/src/economy/labor/labor_market.rs`
- In the payroll section (~line 438-467), set `company.wages_paid_this_turn = actual_paid` and `company.arrears_accrued_this_turn = arrears_this_turn`.
- Reset these fields at the start of `resolve_regional_labor_market` for each company.

**File:** `state/src/corporate/manager.rs`
- In `process_company` (~line 794), replace:
  ```rust
  let wage_expense = (company.fulfilled_fte as f64) * company.offered_wage_per_fte;
  ```
  with:
  ```rust
  let wage_expense = company.wages_paid_this_turn + company.arrears_accrued_this_turn;
  ```
- This ensures the financial record always reflects the actual wage flow, including unpaid portions.

### Step 4: Pillar 4 — Geographic Vein Diversity (planet.rs)

**File:** `state/src/society/planet.rs`
- Rewrite `ensure_base_industrial_veins_per_region`:
  - Split `base_commodities` into two tiers:
    - `ubiquitous`: Stone, Sand, Gravel, Limestone, Peat — present in ~80% of regions.
    - `industrial`: Iron, HardCoal, BrownCoal — present in ~35% of regions each.
  - Use a deterministic hash of `(region_id, commodity)` to decide presence. This ensures reproducibility and geographic diversity.
  - Scale reserves by a region-specific factor (e.g., `hash(region_id) % 100 / 100.0 * 0.5 + 0.5` → 50-100% of the base range).
  - Vary quality and depth by region hash.
  - This creates distinct regional resource profiles: some regions are iron-rich, others are coal-rich, others have only construction materials.

### Step 5: Pillar 5a — Gendered Name Generation (names.rs + turn.rs + succession.rs)

**File:** `state/src/politics/names.rs`
- Add `generate_key_vip_with_gender(cultural_group, gender, rng, used_names)` — calls `generate_person_name(cultural_group, gender, rng)` with the specified gender, then checks uniqueness (50 retries, duplicate on exhaustion).
- Add `generate_full_vip_with_gender(cultural_group, gender, rng)` — calls `generate_person_name(cultural_group, gender, rng)` directly (no uniqueness check).

**File:** `state/src/politics/turn.rs`
- At line 1421, replace `generate_key_vip(cultural_group, rng, &mut used_names)` with `generate_key_vip_with_gender(cultural_group, consort_gender, rng, &mut used_names)`.
- At line 1465, replace `generate_key_vip(cultural_group, rng, &mut used_names)` with `generate_key_vip_with_gender(cultural_group, heir_gender, rng, &mut used_names)`.

**File:** `state/src/politics/succession.rs`
- At line 260, replace `generate_full_vip(culture, &mut rng)` with `generate_full_vip_with_gender(culture, spouse_gender, &mut rng)`.
- Audit all other `generate_full_vip` / `generate_key_vip` calls in `succession.rs` and pass the correct gender.

### Step 6: Pillar 5b — Extended Royal Dynasty (turn.rs)

**File:** `state/src/politics/turn.rs`
- After generating the monarch, consort, and children (current code ~line 1395-1512), add:
  1. **Siblings (1-2):** Generate 1-2 siblings of the monarch. Age: `monarch_age ± rng.gen_range(-5..5)`. Gender: random. Relation: `RoyalRelation::Sibling`. Succession order: after children. Link `father_vip_id` / `mother_vip_id` to a synthetic "royal parent" VIP (or leave as None for genesis members). Register as VIPs with `VipRoleExtended::RoyalHeir` (siblings are in the line of succession).
  2. **Uncles/Aunts (1-2):** Generate 1-2 uncles/aunts (siblings of the monarch's parent). Age: `monarch_age + 15 ± 5`. Gender: random. Relation: `RoyalRelation::Cousin` (use Cousin for extended family beyond siblings). Actually, add `RoyalRelation::Uncle` / `RoyalRelation::Aunt` — wait, the enum only has `Sibling` and `Cousin`. Use `Cousin` for uncles/aunts since the enum doesn't have an Uncle variant. Or better: extend the enum with `Uncle` and `Aunt` variants.
  3. **Cousins (0-2):** Generate 0-2 children of uncles/aunts. Age: `monarch_age - 10 ± 5`. Gender: random. Relation: `RoyalRelation::Cousin`. Link `father_vip_id` or `mother_vip_id` to the uncle/aunt VIP ID.
  4. Set `succession_order` for all members: Monarch=0, Children=1-2, Siblings=3-4, Cousins=5-6.
  5. Link spouse IDs for married siblings/uncles (optional — can be generated later by `process_dynasty_turn`).

**File:** `state/src/politics/succession.rs`
- Add `RoyalRelation::Uncle` and `RoyalRelation::Aunt` variants to the enum.
- Update `process_dynasty_turn` to handle these new relation types in marriage and succession logic.

## Files to Modify

- `state/src/engine/generator/corporate.rs` — Pillar 1 (loan principal cap, FTE scaling) + Pillar 2 (target_fte_demand cap)
- `state/src/engine/generator/mod.rs` — Pillar 1 (Tier 1 estimation, bank count)
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
  - Companies with wage arrears show non-zero wage_expense in financial history
  - Regions have diverse vein profiles (not all regions have Iron)
  - Royal consort name matches gender (female consort has female name)
  - Royal dynasty has >3 members (monarch, consort, children, siblings, cousins)

## Risks/Considerations

1. **FTE cap may reduce corporate employment below historical targets.** This is intentional — the current targets are unrealistic and cause the hyper-bidding crisis. The cap aligns corporate demand with actual labor supply.
2. **Loan principal reduction may cause some companies to furlough earlier.** This is preferable to the current situation where companies receive hallucinated billions and then collapse. The 4-turn runway + revenue should sustain viable companies.
3. **Transient wage fields on Company increase struct size.** Two `f64` fields = 16 bytes. Negligible vs the existing struct size.
4. **Vein diversity may leave some regions without industrial resources.** This is realistic — not every region has iron deposits. The market will transport resources via freight.
5. **Extended dynasty increases genesis VIP count by ~4-6 per monarchy.** This is within the name pool capacity (~2500 combinations for key figures).
6. **RoyalRelation enum extension breaks save compatibility.** Per Directive 10 (Domain Purity Over Backward Compatibility), this is acceptable in the alpha phase.

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | No physical transformations are introduced. Vein generation creates geological reserves with mass-conserving extraction (existing system). FTE cap reduces demand but doesn't create/destroy workers. Transient wage fields track flows, not mass. |
| Double-Entry Bookkeeping | PASS | Loan principal reduction preserves the existing double-entry: company receives cash (asset) + liability (loan), bank records loan asset + deposit liability. The per-bank lending cap (10× Tier 1) is a prudential limit, not a new transaction. Transient wage fields (`wages_paid_this_turn`, `arrears_accrued_this_turn`) track the existing payroll flow — wages paid debit company cash / credit worker income; arrears accrue as company liability. No new cash flows are created. |
| No Teleportation | PASS | No physical movement is introduced. Vein resources remain in their regions; existing freight/logistics system handles transport. |
| Clamping | PASS | FTE cap uses `.min()` to clamp `actual_capacity` to 40% of regional labor pool. Per-bank lending cap uses `.min()` to clamp total lending. Wage cap uses `.min()` to clamp offered wage. Vein presence/absence is binary (present or not). All fields have lower bound 0.0 via existing `.max(0.0)` patterns. Transient wage fields are reset to 0.0 each turn. |
| No Magic Numbers | PASS | FTE cap (40% of regional labor pool) is a dynamic, macroeconomic-derived ratio — it scales with the actual available workforce, not a flat number. Per-bank lending cap (10× Tier 1) is a standard prudential leverage ratio, not a nominal float. Wage cap (0.9× market_average for unprofitable companies) uses the dynamic market average. Loan runway (4 turns) is a count, not a nominal value. Vein presence probability (35-80%) is a geographic distribution parameter, not an economic threshold. |
| Technological Matrices | PASS | No new building types or production methods are introduced. The plan modifies existing company generation and labor market logic only. |
| Architectural Parsimony | PASS | All fixes extend existing systems: (1) loan cap extends `issue_working_capital_loans`, (2) FTE cap extends `generate_region_companies`, (3) wage tracking extends existing labor market payroll, (4) vein diversity rewrites `ensure_base_industrial_veins_per_region` (existing function), (5) gendered names add variants to existing `names.rs` functions. No parallel systems are created. |
| Temporal Causality | PASS | Transient wage fields are set during the labor market phase (step 2 in turn sequence) and read during `process_company` (step 4). This is a forward data flow — no temporal paradox. FTE cap is applied during genesis (before any turn). Wage cap is applied in `set_wage_offers` (before labor market). Vein diversity is applied during planet generation (before any turn). Dynasty extension is applied during genesis. |
| Asymmetric Information | PASS | No new DTOs or frontend data exposure is introduced. The transient wage fields are `#[serde(skip)]` and never serialized. The financial history already exposes `wage_expense` and `wage_arrears` via existing DTOs. No hidden data is sent to the frontend. |
| Full-Stack Accountability | PASS | The financial history fix (Pillar 3) ensures existing DTO fields (`wage_expense`, `wage_arrears`) are correctly populated. The `CompaniesPage.tsx` already displays these fields (updated in Phase 91). No new frontend components are needed — the fix makes existing UI show correct data. Vein diversity is visible through the existing geological/exploration UI. Dynasty members are visible through the existing VIP explorer. |
| Complete Entity Lifecycle | PASS | No new entities are created. Extended dynasty members are VIPs with the same lifecycle as existing VIPs (birth, aging, marriage, death). Veins have existing lifecycle (discovery, extraction, depletion). Transient wage fields are reset each turn (birth) and read once (life) — no accumulation or immortality. |
| Market Forces | PASS | FTE cap doesn't impose a hardcoded split — it bounds total demand to the available supply, and the labor market still clears competitively (bids sorted by wage). Wage cap for unprofitable companies is a market signal (below-average wage), not a command-economy price fix. Vein presence is determined by geographic determinants (hash of region+commodity), not a manual allocation. |
| Rational Actors | PASS | Companies with less cash offer lower wages (rational — they can't afford more). Unprofitable companies bid below market (rational — they're cutting costs). Banks cap lending at 10× Tier 1 (rational — prudential risk management). No charity or debt forgiveness is introduced. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None. The plan is architecturally sound and ready for implementation.
