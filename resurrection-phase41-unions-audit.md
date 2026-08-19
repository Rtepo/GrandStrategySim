# Phase 41 — The Labor Union Update: Target Wages, Tax UI Ghosts & Dashboard Overhaul

**Audit & Implementation Blueprint — Read-Only, No Code Changes**

---

## Executive Summary

Phase 40 unclogged the basic pipelines (budgets, tenders, NIRP, bank payroll), but a deep 7-turn simulation exposes five systemic and UI regressions: (1) wages and employment still swing wildly because `offered_wage_per_fte` is computed directly from cash-on-hand each turn; (2) the Finance tab shows `0.00` for all tax revenue because `last_tax_result` is `#[serde(skip)]` and lost on every save/reload cycle; (3) VIPs are cloned across ministries because `generate_full_vip` has no deduplication and `CompanyDef` drops `is_dspw` on reload; (4) geological deposits show `active_miners=0` because the snapshot looks up `formation.name/deposit_id` while buildings store `formation.id/deposit_id`; (5) banks pay miserable wages because their wage is pegged to a collapsing market average.

This document traces each bug to its root cause and proposes a concrete implementation plan.

---

## PART 1: "Target Wage" & Trade Unions (Labor Market 2.0)

### 1.1 Root Cause Analysis: Wild Wage Swings

**File:** `state/src/corporate/manager.rs` — `set_wage_offers()` (line 879)

The current wage computation is:
```rust
let computed_wage = (effective_cash * wage_budget_fraction) / effective_fte;
```

This means the wage offer is **directly proportional to cash on hand**. If a company sells a large batch of goods and its brokerage cash jumps from 50K to 500K, its wage offer jumps 10x (capped at 5% by Phase 40, but the **target** still moves wildly). Conversely, if a company spends cash on B2B orders, its wage offer crashes.

The Phase 40 sticky-wage caps (3% down, 5% up) dampen the **offered_wage** but do not solve the fundamental problem: the **underlying computed wage** is volatile, and the cap forces the company to offer a wage it cannot afford (leading to FTE layoffs) or a wage below market (leading to worker flight).

### 1.2 Proposed Fix: `target_wage` Field

**Add to `Company` struct** (`state/src/entities/mod.rs`):
```rust
/// Phase 41: Target wage — the company's long-run wage goal.
/// Adjusts slowly (max 2% per turn) toward market average or profitability-based target.
/// The offered_wage is then clamped to [target_wage * 0.95, target_wage * 1.05].
#[serde(default)]
pub target_wage: f64,
```

**Modify `set_wage_offers()`** (`state/src/corporate/manager.rs`):
1. If `target_wage == 0.0` (first turn), initialize it to `market_average_wage`.
2. Compute a `desired_wage` based on profitability: if the company is profitable, target slightly above market average; if losing money, target slightly below.
3. Move `target_wage` toward `desired_wage` by max 2% per turn:
   ```rust
   let adjustment = (desired_wage - company.target_wage).clamp(
       -company.target_wage * 0.02,
       company.target_wage * 0.02,
   );
   company.target_wage = (company.target_wage + adjustment).max(1.0);
   ```
4. Set `offered_wage_per_fte = target_wage` (with the existing 3% down / 5% up sticky caps applied as a secondary smoothing).
5. The labor market then computes `max_affordable_fte = cash / target_wage` — if the company can't afford its target wage, it hires fewer workers, but the **wage rate itself is stable**.

**Banks:** The `set_wage_offers` function currently skips banks (line 889). Banks set their wage in `process_banking_turn` as `(avg_wage * 1.2).max(1.0)`. This should also use a `target_wage` approach: initialize bank `target_wage` to `avg_wage * 1.2` and adjust slowly.

**STRICT RULE — Turn 1 Bank Wage Fallback:** When initializing `target_wage` for Banks, use the same hard fallback as other companies: `max(50.0)`. If `market_average_wage` is 0.0 on Turn 1 (e.g., before any wages have been paid), the bank's `target_wage` must be initialized to `50.0`, not `0.0`. The initialization formula is:
```rust
let bank_target = (avg_wage * 1.2).max(50.0);
company.target_wage = if company.target_wage == 0.0 {
    bank_target
} else {
    // Slow adjustment toward target, max 2% per turn
    let adjustment = (bank_target - company.target_wage).clamp(
        -company.target_wage * 0.02,
        company.target_wage * 0.02,
    );
    (company.target_wage + adjustment).max(50.0)
};
```

### 1.3 Trade Unions: From Dormant to Active

**Current state:** `state/src/corporate/unions.rs` — `process_unions()` runs every turn but only:
- Updates militancy based on unemployment/wages (line 56)
- Randomly reduces `worker_capacity` by 50% if militancy > 0.8 (line 117)
- Collects dues and recruits members

**Problems:**
- Unions never trigger strikes based on **layoffs** — only on random militancy checks.
- There is no `is_striking` flag on `Company` — strikes just reduce `worker_capacity`, which is a permanent structural change, not a temporary strike.
- The `on_strike` field on `Union` is never set to `true` by `process_unions`.

**Proposed implementation:**

1. **Add `is_striking: bool` to `Company`** (`state/src/entities/mod.rs`):
   ```rust
   /// Phase 41: Whether this company's workforce is currently on strike.
   /// Striking companies have 0.0 productivity for that turn.
   #[serde(default)]
   pub is_striking: bool,
   ```

2. **Modify `process_unions()`** (`state/src/corporate/unions.rs`):
   - At the start of each turn, reset `company.is_striking = false` for all companies.
   - After labor market clearing, check each company's layoff ratio:
     ```rust
     let layoff_ratio = if company.prev_fulfilled_fte > 0.0 {
         (company.prev_fulfilled_fte - company.fulfilled_fte) / company.prev_fulfilled_fte
     } else { 0.0 };
     ```
   - If `layoff_ratio > 0.10` (more than 10% laid off) AND the company has a union (`company.union_id` is set), find the union and check:
     - Union `militancy > 0.5` (moderate threshold)
     - Union `strike_fund > 500.0` (can afford a strike)
   - If both conditions met, set `company.is_striking = true` and `union.on_strike = true`.
   - The strike lasts 1–3 turns (random). During a strike, the company's production is 0.0.

3. **Apply strike penalty in production** (`state/src/engine/turn.rs`):
   - In the Wave 3 production cycle, add striking companies' buildings to the `merged_penalties` map with a 1.0 penalty (100% output reduction):
     ```rust
     if company.is_striking {
         for building in &task.ctx.buildings {
             if building.owner_id == company.id {
                 merged_penalties.insert(building.id.clone(), 1.0);
             }
         }
     }
     ```

4. **Strike payroll physics (STRICT DOUBLE-ENTRY):**
   - Striking workers **DO NOT get paid by the company**. When `company.is_striking == true`:
     - The company's wage payment for striking FTE is **temporarily zeroed out** — the company saves cash during the strike.
     - The company **must still pay building overhead/maintenance costs** (utilities, depreciation, fixed costs) — these are NOT zeroed.
     - In the labor market wage payment loop (`labor_market.rs`), skip the payroll debit for striking companies:
       ```rust
       if company.is_striking {
           // No payroll debit — workers are on strike
           continue;
       }
       ```
   - The **Union's `strike_fund`** is debited to pay the striking workers' **strike pay** directly into `ClassDemographics.savings` (double-entry: union fund ↓, class savings ↑).
   - **STRIKE PAY FORMULA (strict):** The Union pays each striking worker exactly **50% of `country.macro_indicators.average_wage`** (or 50.0, whichever is higher) per FTE:
     ```rust
     // In process_unions, for each striking company:
     let strike_pay_per_fte = (country.macro_indicators.average_wage * 0.5).max(50.0);
     let required_strike_pay = striking_fte * strike_pay_per_fte;
     if union.strike_fund >= required_strike_pay {
         union.strike_fund -= required_strike_pay;
         // Credit to the region's class demographics savings
         class_demographics.savings += required_strike_pay;
     } else {
         // Union fund exhausted — fund is zeroed, remaining workers get nothing,
         // and the strike IMMEDIATELY ends.
         class_demographics.savings += union.strike_fund; // pay out what's left
         union.strike_fund = 0.0;
         company.is_striking = false;
         union.on_strike = false;
     }
     ```
   - **If `union.strike_fund < required_strike_pay`, the fund is zeroed out, the remaining workers get nothing, and the strike immediately ends** (setting `is_striking = false`). This is the natural brake on strike duration — a union with a small fund cannot sustain a long strike.

5. **Strike resolution:** At the end of each turn, decrement strike duration. When a strike ends, the company must rehire (the FTE retention floor helps here). The union's `on_strike` flag is reset to `false`.

---

## PART 2: The Tax Ghosting Bug & Finance Dashboard

### 2.1 Root Cause Analysis: Tax Revenue Shows 0.00

**File:** `state/src/state/mod.rs` (line 507-509)
```rust
#[serde(skip)]
#[serde(default)]
pub last_tax_result: Option<tax::TaxCollectionResult>,
```

**The bug:** `last_tax_result` has `#[serde(skip)]`, meaning it is **never serialized to disk**. The TUI flow is:

1. `load_game_state()` → `last_tax_result = None` (deserialized from disk)
2. `run_turn()` → `last_tax_result = Some(tax_result)` (set in memory at `turn.rs:2733`)
3. `save_game_state()` → `last_tax_result` is **skipped** (not written to disk)
4. `rebuild_snapshot(&new_state)` → uses `new_state` which **has** `last_tax_result` in memory

**Analysis:** The snapshot IS built from the in-memory state, so `last_tax_result` should be available after a turn runs. However, the bug manifests in two scenarios:
- **Initial load:** Before any turn is run, `last_tax_result = None` → all taxes show 0.00.
- **Save/reload cycle:** If the user saves, exits, and reloads, `last_tax_result = None` → all taxes show 0.00 until the next turn runs.

The user's 7-turn simulation should have tax data after each turn (since the snapshot uses the in-memory state). But if the user reloads a save, the data is lost. The "ghosting" is that the tax collection **happened** (treasury cash increased, debt shrank) but the **display field** is ephemeral.

**Secondary possibility:** The tax collection might return zeros if:
- `building.current_employment = 0` (PIT base is zero)
- `building.last_profit = 0` (CIT base is zero)
- GDP is very low (VAT base is small)

But VAT is computed as `gdp * 0.6 * weighted_vat_rate * 0.8`, and GDP is set at world generation, so VAT should be non-zero from turn 1.

### 2.2 Proposed Fix: Persist `last_tax_result`

**Option A (Simple):** Remove `#[serde(skip)]` from `last_tax_result` and add `#[derive(Serialize, Deserialize)]` to `TaxCollectionResult`. This persists the last tax result to disk.

**Option B (Better):** Keep `last_tax_result` ephemeral, but also store a **persistent** `last_tax_summary` on `Country` with just the display fields:
```rust
/// Phase 41: Persistent tax summary for Finance tab display.
/// Updated every turn, serialized to disk.
#[serde(default)]
pub last_tax_summary: TaxSummary,
```

Where `TaxSummary` is a small serializable struct with `pit`, `cit`, `vat`, `wealth`, `capital_gains`, `customs`, `state_property` fields.

**Recommendation:** Option A is simpler. Add `Serialize, Deserialize` to `TaxCollectionResult` and remove `#[serde(skip)]`. The struct is small and won't bloat saves significantly.

### 2.3 Abolish Macro-VAT: Transactional B2C VAT (STRICT DOUBLE-ENTRY)

**The Flaw:** The current VAT calculation in `state/src/state/tax.rs` (lines 1338-1359) is a **top-down macro abstraction**:
```rust
let vat_owed = gdp * 0.6 * weighted_vat_rate * 0.8;
```
This is "magical math" — it conjures a VAT amount from GDP without any actual transaction. It violates our transactional double-entry principles. The `consumption_share` field in `VatBracket` is a static weight, not a real consumption measurement.

**The Strict Rule:** Delete the macro-level VAT calculation from `tax.rs`. VAT must become a **true transactional consumption tax** levied at the B2C retail clearing phase.

**Implementation:**

1. **Add a `Commodity::vat_category()` method** (`state/src/registries/enums.rs`):
   - Maps each `Commodity` to one of three VAT categories: `"services"`, `"industry"`, `"agriculture"`.
   - Agricultural commodities (Meat, Fruit, Cereal, Vegetables, etc.) → `"agriculture"`
   - Industrial commodities (Steel, Bricks, Cement, Machinery, Fuel, etc.) → `"industry"`
   - Service commodities (Software, MaintenanceServices, etc.) → `"services"`
   - Default fallback → `"industry"` (highest rate, safest for treasury)

2. **Modify `settle_b2c_clearing`** (`state/src/economy/trade/retail.rs`, line 547):
   - For each store's revenue, split it into **base amount** and **VAT amount**.
   - **STRICT RULE — Dynamic VAT Rate Lookup (No Hardcoding):** The VAT rate MUST be looked up dynamically from the country's actual tax laws (`country.tax_rates.vat`) for the commodity's VAT category. Ideologies and governments can change these rates, so the B2C clearing must respect the active legal rate at the moment of transaction:
     ```rust
     // DYNAMIC lookup — never hardcode a rate
     let vat_rate = country.tax_rates.vat
         .get(&commodity.vat_category())
         .map(|b| b.rate)
         .unwrap_or(0.0); // No VAT if category not in tax law
     let base_amount = class_revenue / (1.0 + vat_rate);
     let vat_amount = class_revenue - base_amount;
     ```
   - **Debit `ClassDemographics.savings`** by the TOTAL amount (base + VAT) — this already happens in `settle_b2c_purchase`.
   - **Credit `Company.cash`** (or brokerage) by the BASE amount only — modify `settle_b2c_purchase` to accept a `base_amount` parameter.
   - **Credit `country.budget.liquid_reserves`** by the VAT amount — this is the SINGLE treasury credit for VAT.
   - **Accumulate** the VAT amount into `country.accumulated_vat` for reporting purposes only.

3. **Modify `settle_b2c_purchase`** (`state/src/economy/trade/transfer_settler.rs`, line 241):
   - Add a `vat_amount: f64` parameter.
   - Debit citizen savings by `amount + vat_amount` (total).
   - Credit company by `amount` (base only).
   - Credit treasury by `vat_amount` (via `country.budget.liquid_reserves += vat_amount`).
   - Sync bank balance sheets for the base amount only (VAT goes to treasury, not bank deposits).

4. **Delete the macro-VAT block** from `process_tax_collection_turn` (`state/src/state/tax.rs`, lines 1338-1359):
   - Remove the `gdp * 0.6 * weighted_vat_rate * 0.8` calculation.
   - Remove the `vat_from_savings` deduction from `citizen_savings`.
   - Instead, read the accumulated transactional VAT from a new field on `Country`:
     ```rust
     /// Phase 41: Accumulated transactional VAT from B2C clearing.
     /// Reset to 0.0 at the start of each turn, accumulated during B2C clearing,
     /// and read by process_tax_collection_turn for REPORTING ONLY.
     #[serde(default)]
     pub accumulated_vat: f64,
     ```
   - **STRICT RULE — No Double-Crediting:** The treasury was ALREADY credited during the B2C clearing phase (step 2/3 above). In `process_tax_collection_turn`, `accumulated_vat` is used **strictly for REPORTING** — it populates `TaxCollectionResult.vat_collected` for the Finance tab. It must **absolutely NOT** add `accumulated_vat` to `country.budget.liquid_reserves` a second time:
     ```rust
     // CORRECT: reporting only, no second treasury credit
     result.vat_collected = country.accumulated_vat;
     // DO NOT: country.budget.liquid_reserves += country.accumulated_vat;
     // DO NOT: country.budget.citizen_savings -= country.accumulated_vat;
     ```

5. **Accumulation flow:**
   - At the start of each turn (in `turn.rs`, before B2C clearing): `task.ctx.country.accumulated_vat = 0.0;`
   - During B2C clearing (in `settle_b2c_clearing`): `country.accumulated_vat += vat_amount;` (reporting accumulator only — the actual treasury credit happens in `settle_b2c_purchase`)
   - At tax collection time: `result.vat_collected = country.accumulated_vat;` (reporting only — NO second treasury credit)

**Performance note:** VAT is treated purely as a final B2C sales tax. No intermediate-stage VAT crediting. This keeps the B2C clearing loop fast — one extra multiplication and one treasury credit per store-class pair.

**Double-entry audit trail for VAT:**
- B2C clearing: `ClassDemographics.savings ↓ (base+VAT)` → `Company.cash ↑ (base)` + `Treasury.liquid_reserves ↑ (VAT)` ✓ balanced
- Tax turn: `result.vat_collected = accumulated_vat` (read-only, no movement) ✓ no double-count

### 2.4 Finance Dashboard Layout Overhaul

**File:** `state/src/ui/tui/render.rs` — `render_finance()` (line ~900)

**Current layout:** Three columns (Label | Value | Detail), with sections stacked vertically:
- TAX REVENUE
- PUBLIC DEBT
- CENTRAL BANK
- BANKING
- SHADOW ECONOMY

**Problems:**
- The Detail column is mostly empty for tax rows.
- CB and Banking sections are long and push Shadow Economy off-screen.
- The layout doesn't use the Detail column effectively.

**Proposed redesign:**
1. **Tax Revenue section:** Move tax rates to the Detail column (already done in Phase 40 for CB, extend to taxes).
2. **Central Bank section:** Move ALL CB parameters to the Detail column. The Value column shows the main number; Detail shows the rate hierarchy.
3. **Banking section:** Move aggregate details to the Detail column.
4. **CB FX Reserves:** Display as a basket, not a single sum. Add a sub-table showing top 3 FX holdings by currency code.
5. **Gold Reserves:** Already added in Phase 40, ensure it displays in the Detail column.

---

## PART 3: VIP Cloning & Tab 6 / Tab 7 Routing

### 3.1 Root Cause Analysis: VIP Cloning

**File:** `state/src/politics/ministries.rs` — `form_government()` (line 356)

The `form_government` function generates minister names using:
```rust
let minister_name = if leader_used.contains(party_id) {
    crate::politics::names::generate_full_vip(cultural_group, &mut rng).full_name
} else {
    leader_used.insert(party_id.clone());
    resolve_minister_name(active_parties, party_id)
};
```

**The bug:** `generate_full_vip` (`state/src/politics/names.rs:307`) randomly picks from a **small name pool** (e.g., ~20 first names, ~20 surnames per culture). With 10+ ministries, the probability of collision is high:
- P(collision) ≈ 1 - (20*20)! / ((20*20)^n * (20*20-n)!) for n ministries
- With 400 possible names and 10 ministries: P(collision) ≈ 1 - 0.88 = 12%
- With 15 ministries: P(collision) ≈ 1 - 0.75 = 25%

There is **no deduplication** — `generate_full_vip` doesn't check against already-generated names.

**Also:** `build_vips()` in `parliament.rs` (line 499) generates VIPs independently from `form_government`, so the same person can appear in both the cabinet and the VIP list with different roles.

### 3.2 Proposed Fix: Deduplication + Pool Expansion (No Numbered Clones)

**STRICT RULE:** No numeric suffixes or "Jr." fallbacks — these break immersion. Instead, significantly expand the name pools so collisions are mathematically negligible.

**Step 1: Expand name pools** (`state/src/politics/names.rs`):
- Expand each culture's `first_names_male`, `first_names_female`, and `surnames` to **at least 50+ entries each**.
- With 50 male + 50 female + 50 surnames = 5,000 possible names per culture.
- For 15 ministries: P(collision after 10 redraws) ≈ (15/5000)^10 ≈ 0 — negligible.
- Use culturally authentic names (e.g., Slavic: "Borysław", "Witosław", "Dobromiła"; Germanic: "Albrecht", "Friedhelm", "Adelheid"; Latin: "Aurelio", "Costanza", "Gervasio").

**Step 2: Add `generate_unique_vip` with HashSet deduplication:**
```rust
pub fn generate_unique_vip(
    cultural_group: &str,
    rng: &mut impl Rng,
    used_names: &mut HashSet<String>,
) -> VipName {
    for _ in 0..20 {
        let vip = generate_full_vip(cultural_group, rng);
        if !used_names.contains(&vip.full_name) {
            used_names.insert(vip.full_name.clone());
            return vip;
        }
    }
    // With 50+ names per pool, this branch is mathematically unreachable.
    // If it somehow fires, just return the last drawn name (no numeric suffix).
    let vip = generate_full_vip(cultural_group, rng);
    used_names.insert(vip.full_name.clone());
    vip
}
```

**Step 3:** Update `form_government` to pass a shared `used_names` set across all ministries.

**Step 4:** Update `build_vips` in `parliament.rs` to also use the same `used_names` set (or at least deduplicate within itself).

### 3.3 Tab 6 (Government): Add VIP List

**File:** `state/src/ui/snapshot.rs` — `GovernmentSnapshot` (line 332)

Add a `vips: Vec<VipRow>` field to `GovernmentSnapshot`:
```rust
pub struct GovernmentSnapshot {
    pub head_of_state_name: String,
    pub head_of_state_role: String,
    pub pm_name: String,
    pub pm_party: String,
    pub pm_ideology: String,
    pub cabinet: Vec<MinisterRow>,
    pub vips: Vec<VipRow>,  // Phase 41: Move VIPs here from Parliament
    pub state_of_emergency: Option<EmergencySnapshot>,
    pub political_capital: f64,
}
```

**Populate from `parliament.vips`** in `build_government_snapshot` (the parliament struct is on `country.politics.parliament_struct`).

**Update `render_government`** (`state/src/ui/tui/render.rs`) to render the VIP list below the cabinet table.

### 3.4 Tab 7 (Parliament): Show Legislative Data

**File:** `state/src/ui/tui/render.rs` — `render_parliament()` (line 586)

**Remove** the VIP section from `render_parliament` (lines 694-716).

**Add/Enhance:**
1. **Legislative Queue:** Already rendered (lines 671-692) but may be empty. Ensure bills are queued by the political turn.
2. **Committees:** Add a `Committee` struct to `Parliament` and render a committees section. Committees can be derived from competencies (e.g., "Budget Committee", "Defense Committee").
3. **Recent Votes:** Already rendered (lines 652-668). Ensure votes are recorded.
4. **Deputy Speakers:** Already in `ChamberPresidium` but not rendered. Add a "Presidium" section showing Speaker and Deputy Speakers.

**Proposed Tab 7 layout:**
```
CHAMBERS
  Sejm (230 seats) — Speaker: X (Club: Y)
  Seat distribution: ...
PRESIDIUM
  Speaker: X
  Deputy Speakers: A, B
CLUBS
  Club | Seats | Ideology | Discipline
LEGISLATIVE QUEUE
  Bill Title | Stage | Initiator
RECENT VOTES
  Bill [PASSED/REJECTED] | For: X | Against: Y | Turn: N
COMMITTEES
  Committee Name | Chair | Members
```

---

## PART 4: Geological Disconnect & Naming Generators

### 4.1 Root Cause Analysis: `active_miners = 0`

**File:** `state/src/ui/snapshot.rs` (lines 533-562)

The snapshot counts miners by matching `building.deposit_id`:
```rust
// Line 537-538: Building stores deposit_id
if let Some(ref did) = b.deposit_id {
    *counts.entry(did.clone()).or_insert(0) += 1;
}
```

The lookup uses `full_id`:
```rust
// Line 550: Snapshot looks up
let full_id = format!("{}/{}", formation_name, dep_id);
// Line 562:
active_miners: counts.get(&full_id).copied().unwrap_or(0),
```

Where `formation_name = f.name.clone()` (line 547).

**But buildings store:** `format!("{}/{}", formation.id, key)` (see `corporate.rs:1242` and `corporate.rs:1322`).

**The bug:** `formation.id` ≠ `formation.name`. For example:
- `formation.id` = `"FORM-ILI-001"`
- `formation.name` = `"Ilirian Highlands"`

The building stores `"FORM-ILI-001/HardCoal"` but the snapshot looks up `"Ilirian Highlands/HardCoal"`. **They never match**, so `active_miners` is always 0.

### 4.2 Proposed Fix

**File:** `state/src/ui/snapshot.rs` (line 550)

Change:
```rust
let full_id = format!("{}/{}", formation_name, dep_id);
```
To:
```rust
let full_id = format!("{}/{}", f.id, dep_id);
```

This makes the snapshot lookup key match the building's stored deposit_id format.

**Also verify:** The production cycle (`state/src/economy/production/geology.rs:find_deposit_index`) uses `formation.id` (line 70), which is correct. So depletion works — only the **display** is broken.

### 4.3 Party Name Generator: Add Country Name

**File:** `state/src/politics/generator.rs` — `generate_party_name()` (line 124)

**Current issue:** The generator never uses `country_name` (except in the fallback). Names are like "Narodowy Partia Pracy" — generic cultural patterns without country identity.

**Proposed fix:**
1. With 30% probability, prepend the country adjective (derived from country name):
   ```rust
   if rng.gen::<f64>() < 0.3 {
       let adjective = country_adjective(country_name); // "Ilirian" from "Iliria"
       components.insert(0, adjective);
   }
   ```
2. Add a `country_adjective` helper that strips common suffixes (-ia, -a) and adds -n or -ian.
3. Expand the word lists to 8-10 items each to reduce repetition.

### 4.4 Tender Name Generator: Add Uniqueness

**File:** `state/src/construction/tender_market.rs` — `generate_tender_name()` (line 86)

**Current issue:** The sequence is derived from `current_turn`, so multiple tenders published on the same turn with the same project type get **identical names**.

**Proposed fix:**
1. Add a `tender_counter: u32` to `Country` (or pass a counter from the tender publisher).
2. Use the counter in the name: `format!("Housing Estate {}", counter)`.
3. Reset the counter each year (or let it grow monotonically).
4. Alternatively, use a static `AtomicU32` counter in the tender module.

---

## PART 5: The Banking Coma (Post-Mortem)

### 5.1 Why Banks Pay Miserable Wages

**File:** `state/src/state/banking.rs` (line 2676)
```rust
let bank_wage = (avg_wage * 1.2).max(1.0);
bank.offered_wage_per_fte = bank_wage;
```

**Root cause:** `avg_wage = country.macro_indicators.average_wage`. If the economy is depressed and most companies pay low wages, the average is low, and banks follow at 1.2x. With `average_wage = 10`, bank wage = 12.

**The `target_wage` fix from Part 1 solves this:** Banks would have their own `target_wage` initialized to a reasonable absolute floor (e.g., `max(avg_wage * 1.2, 5000.0)`) that adjusts slowly. This prevents bank wages from collapsing with the market average.

**Additional fix:** Remove the `set_wage_offers` skip for banks (line 889 in `manager.rs`). Instead, let `set_wage_offers` handle banks too, using the same `target_wage` logic. The banking turn can still set `target_fte_demand` based on portfolio size, but the **wage** should be set by the same code path as all other companies.

### 5.2 Why DSPW Primary Dealers Shows 0

**File:** `state/src/entities/mod.rs` — `CompanyDef` (line 214) and `From<CompanyDef>` (line 847)

**Root cause:** `CompanyDef` does **NOT** have an `is_dspw` field. When companies are deserialized from disk, the `From<CompanyDef>` implementation hardcodes:
```rust
is_dspw: false,  // Line 847
```

So even though the JSON file has `"is_dspw": true` (captured by `#[serde(flatten)]` into `extra`), the conversion to `Company` **always sets `is_dspw = false`**.

This means:
- Banks are generated with `is_dspw = true` (generator `mod.rs:1008`)
- Banks are saved to disk with `is_dspw: true` (Company serializes it with `#[serde(default)]`)
- Banks are loaded from disk with `is_dspw = false` (CompanyDef doesn't have the field, From hardcodes false)
- The snapshot counts `dspw_bank_count = 0` (no banks have `is_dspw = true` after reload)
- DSPW auction settlement fails to find primary dealers (line 2754: `c.is_dspw && primary_dealer_ids.contains(&c.id)`)

**Proposed fix:**

**Option A (Minimal):** Add `is_dspw` to `CompanyDef` and pass it through in `From<CompanyDef>`.

**Option B (Full fix — RECOMMENDED):** Add ALL dropped fields to `CompanyDef`. Known victims:
- `is_dspw: false` (line 847) — DSPW dealer status lost on reload
- `wage_arrears: 0.0` (line 830) — Phase 40 field, always reset on reload
- `productivity_penalty: 0.0` (line 831) — Phase 40 field, always reset on reload
- `consumer_loans: Vec::new()` (line 848) — Phase 35 field, always empty on reload

**STRICT RULE — Backward Compatibility:** Every single new field in `CompanyDef` MUST be wrapped with `#[serde(default)]`. Old saves completely lack these keys in their JSON. Without `#[serde(default)]`, deserialization will **panic** on old game states. Example:

```rust
// In CompanyDef — ALL new fields must have #[serde(default)]:
#[serde(default)]
pub is_dspw: bool,
#[serde(default)]
pub wage_arrears: f64,
#[serde(default)]
pub productivity_penalty: f64,
#[serde(default)]
pub consumer_loans: Vec<crate::state::banking::ConsumerLoan>,

// In From<CompanyDef>:
is_dspw: def.is_dspw,
wage_arrears: def.wage_arrears,
productivity_penalty: def.productivity_penalty,
consumer_loans: def.consumer_loans,
```

**Recommendation:** Option B. Add all four fields with `#[serde(default)]` and pass through in `From<CompanyDef>`. This is critical for data integrity — Phase 40's wage arrears system is completely non-functional across save/reload cycles without this fix.

---

## Implementation Order

1. **Fix `CompanyDef` deserialization** (Part 5.2) — add `is_dspw`, `wage_arrears`, `productivity_penalty`, `consumer_loans` to `CompanyDef` and pass through in `From`. This is a one-file fix with high impact.

2. **Fix geological deposit ID mismatch** (Part 4.1) — change `formation_name` to `f.id` in snapshot.rs line 550. One-line fix.

3. **Persist `last_tax_result`** (Part 2.1) — add `Serialize, Deserialize` to `TaxCollectionResult`, remove `#[serde(skip)]`.

4. **Add `target_wage` field** (Part 1.2) — add to `Company`, modify `set_wage_offers`, update all Company initializers. **Bank target_wage must use `max(50.0)` fallback on Turn 1.**

5. **Add `is_striking` field and trade union logic** (Part 1.3) — add to `Company`, modify `process_unions`, apply in production cycle. **Strike pay = 50% of average_wage (or 50.0 min) per FTE, from union.strike_fund to ClassDemographics.savings. Fund exhaustion ends strike immediately.**

6. **Abolish macro-VAT** (Part 2.3) — delete `gdp * 0.6 * weighted_vat_rate * 0.8` from `tax.rs`. Add `Commodity::vat_category()` method. Modify `settle_b2c_clearing` and `settle_b2c_purchase` to split base/VAT and credit treasury transactionally. Add `accumulated_vat` field to `Country`.

7. **Fix VIP cloning** (Part 3.1) — expand name pools to 50+ per gender/culture, add `generate_unique_vip` with HashSet deduplication (20 redraws, NO numeric suffixes), update `form_government` and `build_vips`.

8. **Move VIPs to Tab 6, redesign Tab 7** (Part 3.3-3.4) — update `GovernmentSnapshot`, `render_government`, `render_parliament`.

9. **Finance dashboard layout** (Part 2.4) — overhaul `render_finance` with Detail column usage.

10. **Party name generator** (Part 4.3) — add country adjective, expand word lists.

11. **Tender name generator** (Part 4.4) — add uniqueness counter.

12. **Bank wage fix** (Part 5.1) — remove `set_wage_offers` skip for banks, use `target_wage` with `max(50.0)` floor.

13. **Build, test, verify.**

---

## Files to Modify

| File | Change |
|------|--------|
| `state/src/entities/mod.rs` | Add `target_wage`, `is_striking` to `Company`; add `is_dspw`, `wage_arrears`, `productivity_penalty`, `consumer_loans` to `CompanyDef` (all `#[serde(default)]`); fix `From<CompanyDef>` |
| `state/src/corporate/manager.rs` | Rewrite `set_wage_offers` to use `target_wage`; remove bank skip |
| `state/src/corporate/unions.rs` | Add strike logic based on layoffs; set `is_striking`; pay strike benefits from `strike_fund` |
| `state/src/engine/turn.rs` | Apply strike penalty in production; reset `is_striking` each turn; reset `accumulated_vat` before B2C clearing |
| `state/src/state/mod.rs` | Remove `#[serde(skip)]` from `last_tax_result`; add `accumulated_vat` field with `#[serde(default)]` |
| `state/src/state/tax.rs` | Add `Serialize, Deserialize` to `TaxCollectionResult`; **delete macro-VAT block**; read `accumulated_vat` instead |
| `state/src/state/banking.rs` | Use `target_wage` for bank wages with `max(50.0)` fallback; remove hardcoded 1.2x average |
| `state/src/economy/trade/retail.rs` | Modify `settle_b2c_clearing` to compute VAT per transaction; pass VAT to `settle_b2c_purchase` |
| `state/src/economy/trade/transfer_settler.rs` | Modify `settle_b2c_purchase` to accept `vat_amount`; credit treasury with VAT |
| `state/src/registries/enums.rs` | Add `Commodity::vat_category()` method returning `"services"`/`"industry"`/`"agriculture"` |
| `state/src/politics/names.rs` | Expand name pools to 50+ per gender/culture; add `generate_unique_vip` with HashSet deduplication |
| `state/src/politics/ministries.rs` | Use `generate_unique_vip` in `form_government` |
| `state/src/politics/parliament.rs` | Use `generate_unique_vip` in `build_vips`; add committees |
| `state/src/politics/generator.rs` | Add country adjective to party names; expand word lists |
| `state/src/construction/tender_market.rs` | Add uniqueness counter to `generate_tender_name` |
| `state/src/ui/snapshot.rs` | Fix deposit ID mismatch; add `vips` to `GovernmentSnapshot` |
| `state/src/ui/tui/render.rs` | Move VIPs to Tab 6; redesign Tab 7; overhaul Finance layout |
| `state/src/engine/generator/corporate.rs` | Initialize `target_wage` for new companies |
| `state/src/engine/generator/mod.rs` | Initialize `target_wage` for banks |

---

## Verification Checklist

- [ ] `cargo build` succeeds with no errors
- [ ] `cargo test --lib` passes (698+ tests)
- [ ] 7-turn simulation:
  - [ ] Wages change by max 2% per turn (target_wage stability)
  - [ ] Companies with >10% layoffs trigger union strikes (is_striking = true)
  - [ ] Striking companies have 0 production for that turn
  - [ ] Striking workers receive 50% of average_wage (min 50.0) from union.strike_fund
  - [ ] Strike ends immediately when union.strike_fund is exhausted
  - [ ] Company payroll is zeroed for striking FTE (company saves cash)
  - [ ] Company still pays building overhead/maintenance during strike
  - [ ] VAT is collected transactionally at B2C clearing (no macro-VAT)
  - [ ] VAT revenue matches sum of per-transaction VAT credits to treasury
  - [ ] Tax revenue shows non-zero values in Finance tab after reload
  - [ ] DSPW Primary Dealers count is non-zero after reload
  - [ ] No VIP name appears more than once in Government tab
  - [ ] Tab 6 shows VIP list; Tab 7 shows legislative data (no VIPs)
  - [ ] Geological deposits show non-zero active_miners
  - [ ] Party names include country adjective (e.g., "Ilirian Conservative League")
  - [ ] Tender names are unique within a turn
  - [ ] Bank wages are stable and non-miserable (target_wage floor max(50.0))
  - [ ] `wage_arrears` and `productivity_penalty` persist across save/reload
  - [ ] Old saves (pre-Phase 41) load without deserialization panics

---

## Risks/Considerations

- **`target_wage` changes labor market dynamics significantly.** Companies with stable wages may hire more consistently, but the initial transition from cash-based wages to target-based wages may cause a one-turn employment shock. Initialize `target_wage` to the current `offered_wage_per_fte` for existing companies.

- **Trade union strikes could cascade.** If many companies lay off workers simultaneously (e.g., during a recession), many strikes could fire at once, further reducing production and deepening the recession. Cap the number of simultaneous strikes per union to 1, and cap total striking companies per country to 10% of the corporate sector.

- **Strike payroll physics (STRICT DOUBLE-ENTRY):** Striking workers do NOT get paid by the company — the company's payroll is zeroed for striking FTE, saving the company cash. The Union's `strike_fund` is debited to pay workers **exactly 50% of `country.macro_indicators.average_wage` (or 50.0, whichever is higher) per FTE** directly into `ClassDemographics.savings`. If `union.strike_fund < required_strike_pay`, the fund is zeroed, remaining workers get nothing, and the strike immediately ends. The company must still pay building overhead/maintenance costs during the strike.

- **Transactional VAT replaces macro-VAT.** The `gdp * 0.6 * weighted_vat_rate * 0.8` formula is deleted entirely. VAT is now collected at the B2C retail clearing phase as a true sales tax. This may cause VAT revenue to fluctuate more (based on actual consumption) vs the smooth macro approximation. The `accumulated_vat` field on `Country` bridges the B2C phase and the tax collection phase. **Performance:** one extra multiplication and one treasury credit per store-class pair — negligible overhead.

- **STRICT RULE — No Double-Crediting VAT:** The Treasury (`country.budget.liquid_reserves`) is credited ONCE during the B2C clearing phase when the transaction occurs. The `accumulated_vat` field is used **strictly for REPORTING** in `process_tax_collection_turn` — it populates `TaxCollectionResult.vat_collected` for the Finance tab. It must **absolutely NOT** add `accumulated_vat` to `liquid_reserves` a second time during the tax turn. Double-entry audit: `savings ↓ (base+VAT)` → `company ↑ (base)` + `treasury ↑ (VAT)` — balanced at B2C time; tax turn reads only.

- **STRICT RULE — Dynamic VAT Rate Lookup:** The B2C transaction logic MUST dynamically look up the current, active VAT rate from `country.tax_rates.vat` for the commodity's VAT category. No hardcoded rates. Ideologies and governments can change VAT rates through legislation, so the B2C clearing must respect the active legal rate at the moment of transaction. If a category is missing from the tax law, the rate defaults to `0.0` (no VAT).

- **Persisting `last_tax_result`** adds a small amount of save bloat (~200 bytes per country per save). This is negligible.

- **`CompanyDef` fixes** are critical for data integrity. The current `From<CompanyDef>` silently drops `is_dspw`, `wage_arrears`, `productivity_penalty`, and `consumer_loans` on every reload. This means Phase 40's wage arrears system is **completely non-functional** across save/reload cycles — arrears are reset to 0 every time the game is loaded. This is a critical bug that must be fixed. **All new `CompanyDef` fields MUST use `#[serde(default)]` to avoid deserialization panics on old saves.**

- **VIP deduplication** requires expanding the name pools to 50+ entries per gender/culture. No numeric suffixes or "Jr." fallbacks — these break immersion. With 5,000+ possible names per culture and 20 redraws, collision probability is mathematically negligible.

- **Geological deposit fix** is a one-line change but affects the display only. The production/depletion logic already works correctly because it uses `formation.id`.

- **Bank `target_wage`** must have an absolute floor of `max(50.0)` to prevent wages from collapsing to near-zero on Turn 1 when `market_average_wage` is 0.0. The same fallback applies to all companies.
