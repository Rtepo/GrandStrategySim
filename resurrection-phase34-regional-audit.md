# Phase 34 — Regional Audit: 1M Wage Bug, Election Lock, UI Purge & Investment Failure

A read-only audit of the simulation codebase revealing five root-cause defects and a plan for their remediation.

---

## PART 1: The 1,000,000 Wage Bug & Ghost Sectors

### 1.1 Root Cause: Charity Wage Explosion

**File:** `state/src/corporate/manager.rs`, lines 873–943 (`set_wage_offers`)

The wage offer formula is:

```
offered_wage_per_fte = (brokerage_cash × wage_budget_fraction) / target_fte_demand
```

The only guard is:

```rust
let sane_max = 1_000_000.0; // absolute sanity cap
company.offered_wage_per_fte = computed_wage.min(sane_max);
```

**The bug:** When an NGO or Religion company receives a donation (e.g., 50,000 currency) but has a tiny `target_fte_demand` (e.g., 5–10 FTE from `create_charity_company`), the wage explodes:

- 50,000 × 0.6 / 5 = **6,000** (reasonable)
- 500,000 × 0.6 / 5 = **60,000** (high but not insane)
- 5,000,000 × 0.6 / 3 = **1,000,000** (hits the sanity cap)

The sanity cap of 1,000,000 is exactly the value showing in the UI. The cap is acting as the wage offer, not as a guard against overflow.

**Fix Plan:**

1. Replace the absolute `sane_max = 1_000_000.0` with a **relative cap** based on the market average wage:
   ```rust
   let sane_max = _market_average_wage * 3.0; // max 3× national average
   ```
   The `_market_average_wage` parameter is already passed to `set_wage_offers` but is currently unused (prefixed with `_`).

2. Add a **minimum FTE denominator** to prevent division by tiny numbers:
   ```rust
   let effective_fte = company.target_fte_demand.max(1.0);
   let computed_wage = wage_budget / effective_fte;
   ```

3. For charity sectors (NGO, Religion), use a **lower wage budget fraction** (0.4 instead of 0.6) since their cash comes from donations, not revenue.

### 1.2 Root Cause: Banking Sector Dead at 0.00

**File:** `state/src/engine/generator/mod.rs`, lines 919–936 (`build_bank_companies`)

Banks are generated with `target_fte_demand = bank_fte` (50–200 FTE) and `offered_wage_per_fte = bank_wage`, but **`region_id` is never set**. It defaults to `String::new()` (empty string).

**File:** `state/src/economy/labor/labor_market.rs`, lines 185–188

The labor market filters companies by region:
```rust
let region_companies: Vec<&mut Company> = companies
    .iter_mut()
    .filter(|c| c.region_id == region.id)
    .collect();
```

Since banks have `region_id = ""`, they match **no region** and are **excluded from labor clearing entirely**. They can never hire anyone, so their employment stays at 0 and their wages show as 0.00.

**Fix Plan:**

1. In `build_bank_companies`, assign each bank to the **capital region** (or the first region) of the country:
   ```rust
   company.region_id = country_regions.first().map(|r| r.id.clone()).unwrap_or_default();
   ```

2. Alternatively, assign banks to **multiple regions** by creating one bank branch per megaregion. This is more realistic but more complex. The simpler fix (capital region) is recommended for Phase 34.

3. Verify that `set_wage_offers` runs on banks (it does — banks are in `task.companies`), and that the labor market includes them once `region_id` is set.

---

## PART 2: The "Provisional Government" Lock & Elections

### 2.1 Root Cause: Provisional Government Persistence

**File:** `state/src/politics/turn.rs`, lines 372–479 (`regenerate_parties`)

When `total_support == 0.0` (no ideology bid crosses the threshold), a stub party is created:

```rust
new_parties.insert(
    "Provisional Technocratic Government".to_string(),
    Party {
        ideology: "Socjalliberalizm".to_string(),
        ...
        ..Party::default()  // leader.name is empty!
    },
);
```

The `..Party::default()` means `leader.name = String::new()` (empty).

**The loop:** In subsequent years, `regenerate_parties` preserves existing parties (line 412–422):
```rust
for (name, party) in old_parties {
    if let Some(ideo) = Ideology::from_name(&party.ideology) {
        if let Some(&bid) = bids.get(&ideo) {
            if bid > threshold || parliament.contains_key(name) {
                let mut updated = party.clone();  // Preserves empty leader.name!
                ...
```

The provisional government is preserved **with its empty leader name** if SocialLiberalism's bid crosses the threshold. If it doesn't, a new provisional government is created — again with an empty leader.

### 2.2 Root Cause: Elections Never Fire

**File:** `state/src/politics/turn.rs`, lines 87–140 (`process_political_year`)

Elections fire when:
```rust
let election_due = country.politics.years_to_elections == 0
    || country.politics.budget_crisis
    || (country.politics.minority_government && unrest > 40.0);

if form.is_democratic() && election_due {
```

The `years_to_elections` is set to `form.election_cycle()` after elections (line 121). But `bootstrap_politics` sets `years_to_elections = 0` (line 543), so elections should fire on the first `process_political_year` call.

**The problem:** `process_political_year` runs once per **year** (24 turns). If the first year's `regenerate_parties` produces `total_support == 0`, the provisional government is created. Then elections fire (line 108), but `calculate_seats` with a single party produces a trivial parliament. The provisional government wins by default.

In subsequent years, the provisional government is preserved, and elections produce the same result. The system is stable in a bad equilibrium.

**Fix Plan:**

1. **Name the provisional government's leader:** In `regenerate_parties`, when creating the provisional government, generate a named leader:
   ```rust
   let vip = super::names::generate_full_vip(cultural_group, &mut rng);
   let leader = super::names::vip_to_leader(vip, "Socjalliberalizm");
   ```

2. **Backfill empty leader names on preserved parties:** When preserving existing parties (line 416), check if `leader.name` is empty and generate one:
   ```rust
   let mut updated = party.clone();
   if updated.leader.name.is_empty() {
       let vip = super::names::generate_full_vip(cultural_group, &mut rng);
       updated.leader = super::names::vip_to_leader(vip, &updated.ideology);
   }
   ```

3. **Force initial election diversity:** In `bootstrap_politics`, after `process_political_year`, if only one party exists, inject 2–3 additional parties with generated leaders to ensure competitive elections.

4. **Election safety net:** If `active_parties.len() <= 1` and the form is democratic, force-regenerate parties with a lower threshold (e.g., 0.0) to ensure at least 3 parties contest the election.

### 2.3 Polish String Purge

**Status:** The strings "Tymczasowy Rząd" and "Przywódca" have already been purged from the Rust source. A grep for `Tymczasowy|Przywódca` returns no matches in `state/src/`.

**Remaining Polish strings to audit:**

- `state/src/politics/turn.rs` line 127–139: Election result messages still contain Polish (`"zdecydowaną większością"`, `"powołując chwiejny Rząd Mniejszościowy"`, `"[WYBORY] Misję tworzenia rządu otrzymał"`).
- `state/src/politics/turn.rs` line 94: `"[NAPRAWA USTROJU] Przywrócono mechanizmy demokratyczne."`
- `state/src/politics/turn.rs` line 326: `"[STATE OF EMERGENCY] Auto-expired — Parliament resumes."` (already English)
- `state/src/politics/parliament.rs` line 328: Chamber name `"Sejm"` and line 350: `"Senate"` — these are proper nouns and may be acceptable, but should be configurable.

**Fix Plan:** Replace all user-facing Polish strings in `politics/turn.rs` with English equivalents. Keep serde field names in Polish for save compatibility.

---

## PART 3: The UI / UX Disaster

### 3.1 Government Tab Layout

**File:** `state/src/ui/tui/render.rs`, lines 363–455 (`render_government`)

**Current issues:**
- The ministry name includes the full "Ministry of Energy" prefix, making rows like "Ministry of Energy Minister ()".
- The minister name column shows "(unnamed)" when `m.minister_name.is_empty()`.
- The table uses 6 columns with fixed widths (20, 20, 15, 20, 12, 12) which doesn't display cleanly.

**Fix Plan:**

1. **Strip the "Ministry of " prefix** from `ministry_name` in the snapshot, so column 1 shows just "Energy", "Defense", etc.
2. **Use the `resolve_minister_name` fallback** in the snapshot builder (line 622–628) instead of hardcoding "(unnamed)".
3. **Adjust column widths** to be more balanced: `Length(18), Length(22), Length(15), Length(18), Length(12), Length(12)`.
4. **Add a separator row** between the header section (Head of State, PM) and the cabinet section.

### 3.2 VIP Names Still (unnamed)

**File:** `state/src/ui/snapshot.rs`, lines 595–604

The PM name fallback is:
```rust
if !p.leader.name.is_empty() {
    p.leader.name.clone()
} else {
    "(unnamed)".to_string()
}
```

**File:** `state/src/politics/parliament.rs`, lines 498–563 (`build_vips`)

The `build_vips` function generates VIPs with names, and they're stored in `Parliament.vips`. However:

**File:** `state/src/ui/snapshot.rs`, lines 667–767 (`build_parliament_snapshot`)

The `ParliamentSnapshot` struct (line 275–281) **does not include a VIPs field**. The `build_parliament_snapshot` function maps chambers, clubs, votes, and the legislative queue — but **completely omits the VIPs list**. The VIPs are generated but never reach the UI.

**Fix Plan:**

1. Add `pub vips: Vec<VipRow>` to `ParliamentSnapshot`.
2. Define `VipRow { full_name, party, role, ideology, age }`.
3. In `build_parliament_snapshot`, map `parl.vips` to `VipRow` entries.
4. In `render_parliament`, add a VIP section showing Head of State, PM, Ministers, and Speakers with their names.
5. **Root fix:** Backfill empty leader names in `regenerate_parties` (see Part 2.2).

### 3.3 ToT% Still +0.00%

**File:** `state/src/engine/turn.rs`, line 3724

```rust
state.market_history.prev_net_surplus = market.net_surplus.clone();
```

This runs at the **end of the turn**, AFTER the market has been cleared. Then both `market_history.json` (with `prev_net_surplus`) and `market.json` (with `net_surplus`) are saved.

**File:** `state/src/ui/tui/app.rs`, lines 903–907 (`rebuild_snapshot`)

The snapshot loads both files:
```rust
let market_history = load_market_history(data_dir);  // has prev_net_surplus = CURRENT turn's surplus
let market = load_global_market(data_dir);            // has net_surplus = CURRENT turn's surplus
```

**The bug:** `prev_net_surplus` is set to the **current** turn's `net_surplus`, so when the snapshot compares them:
```rust
let tot_balance_change = if prev_surplus.abs() > 0.01 {
    ((net_surplus - prev_surplus) / prev_surplus.abs()) * 100.0
```
The result is always `(current - current) / current = 0.0`.

**Fix Plan:**

1. **Capture `prev_net_surplus` at the START of the turn**, before market clearing modifies `net_surplus`:
   ```rust
   // At the beginning of process_turn, before any market operations:
   state.market_history.prev_net_surplus = state.market_history.current_net_surplus.clone();
   state.market_history.current_net_surplus = market.net_surplus.clone();
   ```
   Or simpler: just move line 3724 to **before** the market clearing step (around line 370, before the B2B/B2C clearing begins).

2. **Alternative:** Store the previous surplus in a separate field that is only updated after the snapshot is built. But the simpler fix (moving the assignment) is preferred.

3. **Verify:** After the fix, `prev_net_surplus` should contain the **previous** turn's surplus, and `net_surplus` should contain the **current** turn's surplus, producing a real delta.

---

## PART 4: Investment (I) Failure & Corporate Panic

### 4.1 Root Cause: Investment (I) = 0

**File:** `state/src/engine/turn.rs`, lines 880–887

Investment (I) is ONLY accumulated from B2B trades of fixed-asset commodities:
```rust
let investment: f64 = secured_trades.iter()
    .filter(|t| t.commodity.is_fixed_asset())
    .map(|t| t.quantity * t.execution_price)
    .sum();
task.gdp_acc.investment += investment;
```

**File:** `state/src/registries/enums.rs`, lines 612–623

`is_fixed_asset()` returns true only for: `IndustrialMachinery`, `ConstructionMachinery`, `AgriculturalMachinery`, `OfficeMachinery`, `Trucks`, `Cars`, `DraftAnimals`.

**File:** `state/src/construction/orders.rs`, lines 216–256

When a construction project **completes**, `advance_construction_projects` increases `building.worker_capacity` and `company.fixed_capital`, but **never adds anything to `gdp_acc.investment`**. The capital formation from construction is completely invisible to GDP.

**This is the root cause:** Ministries publish construction tenders, contractors bid, projects are awarded, materials are consumed, buildings are completed — but none of this flows into `I`. The only thing that flows into `I` is machinery purchases, which are rare in the early game.

**Fix Plan (Correction Applied):**

> **STRICT RULE:** Tranche payments are merely cash transfers from Investor to Contractor — they are NOT investment. Investment (`I`) only occurs when physical materials (Cement, Steel, Wood) are consumed and work is performed. `I` must be accumulated exclusively from the delta in `cost_spent` (or derived value from `progress_delta`) inside `advance_construction_projects`. Do NOT touch `release_construction_tranches` for GDP accounting.

1. **Record materials consumed as investment:** In `advance_construction_projects`, the call `project.consume_delivered_materials(&mut building.inventory, unit_costs)` already tracks the value of materials consumed. The delta in `project.cost_spent` before and after consumption is the real investment for this turn.

2. **Implementation approach:** Modify `advance_construction_projects` to return `f64` — the total `cost_spent` delta across all projects this turn. In `turn.rs`, accumulate this into `task.gdp_acc.investment`:
   ```rust
   // In advance_construction_projects:
   let cost_before = project.cost_spent;
   let consumed = project.consume_delivered_materials(&mut building.inventory, unit_costs);
   let turn_investment = project.cost_spent - cost_before;
   total_investment += turn_investment;
   // ...
   // Return total_investment at the end.
   ```

3. **Do NOT record tranche payments as I.** Tranches in `release_construction_tranches` are cash-flow events (Investor → Contractor), not capital formation. Recording them would double-count once materials are consumed.

4. **Do NOT record completion value as I separately.** The `cost_spent` delta already captures all materials consumed during construction. Recording `capital_increase` on completion would double-count the materials that were consumed in prior turns.

### 4.2 Shadow Economy Explosion

**File:** `state/src/economy/justice/legal_status.rs`, lines 239–282 (`trigger_shadow_employment`)

Shadow employment triggers when:
```rust
let unmet_demand = (company.target_fte_demand - company.fulfilled_fte).max(0.0);
if unmet_demand < company.target_fte_demand * 0.5 {
    continue;
}
```

In the first 3 turns, companies have `fulfilled_fte = 0` (no labor clearing has happened yet, or labor clearing produced 0 hires because wages were 0 or cash was insufficient). So `unmet_demand = target_fte_demand`, which is > 50% of target. If the company has any cash, it can trigger shadow employment.

**The probability:**
```rust
let pit_incentive = pit_rate * 0.5;
let detection_risk = inspectorate_capacity * 0.01;
let shadow_probability = (0.10 + pit_incentive - detection_risk).clamp(0.0, 0.8);
```

With a typical PIT rate of 0.15–0.20 and low inspectorate capacity (0–10), the probability is:
- 0.10 + 0.075–0.10 - 0.0–0.1 = 0.075–0.20

So 7.5–20% of eligible companies enter the shadow economy **every turn**. Over 24 turns, this compounds.

**Fix Plan:**

1. **Add a startup grace period:** Companies with < 3 turns of financial history should not trigger shadow employment. They haven't had time to establish legal operations.
   ```rust
   if company.financial_history.len() < 3 {
       continue;
   }
   ```

2. **Lower the base probability:** Reduce the base chance from 10% to 5%:
   ```rust
   let shadow_probability = (0.05 + pit_incentive - detection_risk).clamp(0.0, 0.8);
   ```

3. **Increase the unmet demand threshold:** Only trigger shadow employment when unmet demand is > 80% (not 50%):
   ```rust
   if unmet_demand < company.target_fte_demand * 0.8 {
       continue;
   }
   ```

### 4.3 Syndic / Corporate Panic

**File:** `state/src/corporate/lifecycle.rs`, lines 62–110

The grace period (Phase 33) prevents liquidation for companies with < 2 financial history entries. However, companies can still go bankrupt from **negative equity** (`company_capital < 0`) regardless of the grace period.

**File:** `state/src/corporate/bankruptcy.rs`, lines 193–360 (`Syndic`)

The Syndic processes bankrupt companies: converts FX, reclaims frozen cash, pays taxes, pays banks, routes residual to treasury. This is functioning correctly.

**The issue:** If companies go bankrupt in the first few turns (from negative equity due to B2B losses), the Syndic liquidates them, their buildings are destroyed, and no new companies spawn fast enough to replace them. This creates a deflationary spiral.

**Fix Plan:**

1. **Extend the negative equity grace period:** For companies with < 3 financial history entries, allow negative equity to persist for up to 3 turns before liquidation. Add a `negative_equity_turns` counter to `Company`.

2. **Increase spawn rate:** In `spawn_new_companies` (line 184), increase the spawn rate for the first 5 years to ensure the economy has enough companies.

---

## PART 5: New [8] Regions Tab & Local Gov Accounting

### 5.1 New [8] Regions Tab

**File:** `state/src/ui/tui/tabs.rs`

Add a new `Tab::Regions` variant:
```rust
pub enum Tab {
    MacroFinance,      // [1]
    MarketLogistics,   // [2]
    ConstructionGeology, // [3]
    SocietyJustice,    // [4]
    Sectors,           // [5]
    Government,        // [6]
    Parliament,        // [7]
    Regions,           // [8] NEW
}
```

Update `ALL`, `title()`, and `hotkey()` accordingly.

### 5.2 Region Snapshot Data

**File:** `state/src/ui/snapshot.rs`

Add a new `RegionRow` struct and `regions: Vec<RegionRow>` to `CountrySnapshot`:

```rust
pub struct RegionRow {
    pub id: String,
    pub display_name: String,
    pub megaregion: String,
    pub population: i64,
    pub regional_gdp: f64,
    pub gdp_per_capita: f64,
    pub has_governance: bool,
    pub liquid_reserves: f64,
}
```

In `build_country_snapshot`, iterate `country.regions` and build rows. For `megaregion`, look up `country.megaregions` to find which megaregion contains the region. For `display_name`, use the region's `display_name` field (from geography.rs line 619) or fall back to `id`.

### 5.3 Region Tab Renderer

**File:** `state/src/ui/tui/render.rs`

Add `render_regions(snap)`:
- Columns: Region Name (25), Megaregion (20), Population (12), Regional GDP (15), GDP/capita (12), Reserves (12)
- Sort by regional GDP descending
- Show a summary row at the top with national totals

### 5.4 GDP per Capita on Macro Tab

**File:** `state/src/ui/tui/render.rs`, lines 97–125

Add a row after "Official GDP":
```rust
("  GDP per Capita", fmt_money(g.official_gdp / snap.population as f64)),
```

### 5.5 Local Government Expenditure — DEFERRED

> **STRICT RULE:** For Phase 34, do NOT touch local government spending logic. Leave `fiscal_transfers.rs` and `local_government.rs` entirely out of Phase 34. Phase 34 is solely focused on rendering the `[8] Regions` Tab and fixing the macro panic (wage bug, banking, elections, UI, investment, shadow economy). Local government expenditure accounting (construction tenders flowing into `I`, B2C subsidies flowing into `C` or `G`) is deferred to a future phase.

No changes to `fiscal_transfers.rs` or `local_government.rs` are planned for Phase 34. The `process_regional_taxes` and `process_fiscal_transfers` functions from Phase 33 remain as-is.

---

## Implementation Steps (Ordered)

### Step 1: Fix the 1M Wage Bug
- **File:** `state/src/corporate/manager.rs`
- Replace `sane_max = 1_000_000.0` with `sane_max = _market_average_wage * 3.0`
- Add `effective_fte = company.target_fte_demand.max(1.0)` denominator floor
- Add charity sector wage budget fraction (0.4 for NGO/Religion)
- **Tests:** Wage offer capped at 3× market average; charity with tiny FTE doesn't explode

### Step 2: Fix Banking Dead Sector
- **File:** `state/src/engine/generator/mod.rs`
- Set `company.region_id` to the capital region in `build_bank_companies`
- **Tests:** Bank has non-empty `region_id`; bank appears in labor market clearing

### Step 3: Fix Provisional Government & Elections
- **File:** `state/src/politics/turn.rs`
- Generate named leader for provisional government
- Backfill empty leader names on preserved parties
- Force initial election diversity (≥3 parties for democracies)
- Purge remaining Polish strings in election messages
- **Tests:** Provisional government has a named leader; preserved parties get backfilled names; democratic countries have ≥3 parties

### Step 4: Fix VIP Names in UI
- **File:** `state/src/ui/snapshot.rs`
- Add `vips: Vec<VipRow>` to `ParliamentSnapshot`
- Map `parl.vips` in `build_parliament_snapshot`
- **File:** `state/src/ui/tui/render.rs`
- Add VIP section to `render_parliament`
- Use `resolve_minister_name` fallback instead of "(unnamed)" in government snapshot
- **Tests:** Parliament snapshot includes VIPs; VIPs have non-empty names

### Step 5: Fix Government Tab Layout
- **File:** `state/src/ui/snapshot.rs`
- Strip "Ministry of " prefix from `ministry_name` in `MinisterRow`
- **File:** `state/src/ui/tui/render.rs`
- Adjust column widths and add separator row
- **Tests:** Ministry names show as "Energy" not "Ministry of Energy"

### Step 6: Fix ToT% Calculation
- **File:** `state/src/engine/turn.rs`
- Move `state.market_history.prev_net_surplus = market.net_surplus.clone()` from end-of-turn (line 3724) to start-of-turn (before market clearing)
- **Tests:** ToT% is non-zero when market surplus changes between turns

### Step 7: Fix Investment (I) = 0
- **File:** `state/src/construction/orders.rs`
- Modify `advance_construction_projects` to return `f64` — the total `cost_spent` delta (materials consumed) across all projects this turn
- **Do NOT touch `release_construction_tranches`** — tranche payments are cash transfers, not investment
- **File:** `state/src/engine/turn.rs`
- Accumulate the returned `cost_spent` delta into `task.gdp_acc.investment`
- **Tests:** Construction projects consuming materials increase I; tranche payments do NOT affect I

### Step 8: Reduce Shadow Economy Panic
- **File:** `state/src/economy/justice/legal_status.rs`
- Add 3-turn grace period before shadow employment can trigger
- Lower base probability from 0.10 to 0.05
- Raise unmet demand threshold from 50% to 80%
- **Tests:** New companies don't enter shadow economy; probability is lower

### Step 9: Add [8] Regions Tab
- **File:** `state/src/ui/tui/tabs.rs`
- Add `Tab::Regions` variant
- **File:** `state/src/ui/snapshot.rs`
- Add `RegionRow` struct and `regions` field to `CountrySnapshot`
- Build region rows in `build_country_snapshot`
- **File:** `state/src/ui/tui/render.rs`
- Add `render_regions` function
- Add GDP per capita to Macro tab
- **Tests:** Region tab renders; regions sorted by GDP; GDP per capita shows on Macro tab

### Step 10: Build & Test Verification
- `cargo build --lib`
- `cargo build`
- `cargo test --lib`
- Manual 24-turn simulation checks:
  - No sector shows 1M wages
  - Banking sector has employment > 0
  - No "Provisional Technocratic Government" after year 2
  - Elections produce multiple parties
  - VIPs have real names in the UI
  - Ministry names show as "Energy" not "Ministry of Energy"
  - ToT% is non-zero
  - Investment (I) > 0 from construction
  - Shadow GDP < 50% of official GDP
  - Regions tab shows all regions with GDP per capita

---

## Risks & Considerations

1. **Save compatibility:** The `prev_net_surplus` move is a logic change, not a schema change. No save migration needed.
2. **Wage cap regression:** Capping wages at 3× market average could suppress wages in economies where the market average is very low. The cap should be a floor of `max(market_average * 3.0, 5000.0)` to avoid unrealistic suppression.
3. **Bank region assignment:** Assigning all banks to the capital region concentrates banking employment there. This is realistic for small countries but may need refinement for large countries.
4. **Investment accounting:** `I` is recorded exclusively from `cost_spent` delta (materials consumed) in `advance_construction_projects`. Tranche payments and completion value are NOT recorded as I to avoid double-counting. This aligns with national accounts: I is capital formation, not cash flow.
5. **Shadow economy grace period:** A 3-turn grace may be too long if companies are genuinely unable to hire. Consider 2 turns instead.
6. **Election forcing:** Forcing ≥3 parties may create unrealistic parties in small countries. Consider making the minimum configurable or scaling with population.
7. **Local government scope:** `fiscal_transfers.rs` and `local_government.rs` are explicitly out of scope for Phase 34. No changes to local government spending accounting are planned.
