# Phase 36 — Central Bank Awakening, Demographic Reconciliation & Financial Market Audit

## Summary

A read-only audit of the codebase revealing five root-cause defects: (1) the Central Bank reference rate is frozen at 0% due to a hardcoded GDP growth placeholder and fixed-step adjustments instead of a Taylor Rule; (2) national population diverges from regional sums because top-down demographic models and migration modify `budget.population` independently of regions; (3) elections are permanently deadlocked because all ideology bids return 0 when interest groups have no power, triggering a "Provisional Technocratic Government" fallback with hardcoded Polish strings; (4) banking employment is zero because Phase 35's FTE demand step doesn't set `offered_wage_per_fte` and only one bank is generated per country; (5) Sector ToT is always 0% because `_prev_employment` is overwritten at end-of-turn with current-turn data, making the snapshot compare current-to-current.

---

## Approved Architectural Constraints

The following three strict constraints are mandated by the user and MUST be obeyed throughout implementation:

### Constraint 1: Configurable Central Bank Targets

`target_inflation` (e.g., 0.02) and `potential_growth` (e.g., 0.02) MUST be stored as serialized fields within the `CentralBank` struct (or its policy configuration). They MUST NOT be hardcoded as magic numbers inside the Taylor Rule function math. The Central Bank must have a defined, serialized target it aims for, visible in the UI and adjustable by the simulation.

### Constraint 2: Strict Double-Entry for Investment Funds

When Investment Funds purchase sovereign bonds on the DSPW primary market or secondary market, they MUST strictly use their collected capital reserves. The purchase cost MUST be deducted from the Fund's cash balance (brokerage_account.cash or available_cash) and credited to the Treasury (primary auction) or the selling bank (secondary market). If the Fund does not have sufficient liquidity, the purchase MUST fail. No magical bond absorption — no bond appears on the fund's balance sheet without an equal and opposite cash debit.

### Constraint 3: State Construction Tender Priority

State and Regional `ConstructionTenders` already have guaranteed Treasury cash encumbered via `estimated_cost` and tranche payments. The root cause of VWAP 0.00 for construction materials is NOT merely a lack of corporate loans — it is that `submit_construction_b2b_orders` (construction/orders.rs line 84-86) uses the contractor's `computed_liquid_capital()` as the encumbrance source, NOT the tranche payments the contractor has received or the guaranteed Treasury escrow. ConstructionProjects backed by State Tenders MUST correctly utilize their encumbered/escrowed cash to bid competitively on the B2B market, breaking the VWAP 0.00 deadlock. The contractor's B2B bidding capacity must reflect tranche payments received plus remaining guaranteed tranches, not just their own thin liquid capital.

---

## PART 1: Central Bank & Interest Rate Absurdity

### Root Cause Analysis

**1.1 Reference Rate Frozen at 0.00%**

File: `state/src/state/central_bank.rs`, lines 251-311

The `update_reference_rate` function uses fixed-step adjustments (+50bps, -25bps) rather than a Taylor Rule. Worse, the call site in `process_banking_turn` (banking.rs line 1952) hardcodes GDP growth:

```rust
let gdp_growth = 0.02; // Placeholder: 2% default growth
```

With the default `Mixed` mandate:
- `inflation_gap.abs() > 0.02` is rarely true (inflation is often near 0)
- `gdp_growth < 0.015` is never true (hardcoded 0.02)
- Result: `rate_adjustment = 0.0` every turn

Since `RppInterestRates` derives `Default` (reference_rate = 0.0), the rate starts at 0% and never moves.

**1.2 Sovereign Bond Yields Disconnected from CB Rate**

File: `state/src/economy/finance/debt_market.rs`, lines 288-336

The `DebtMarket.weighted_avg_interest_rate` is calculated from actual `coupon_rate` fields on outstanding `TreasurySecurity` instances. Legacy bonds (ministries.rs line 1200) use `public_debt.interest_rate` from the static JSON save (e.g., 0.077 = 7.7%). New debt issuance does not reference the CB rate at all.

**1.3 KNF Uses Default Central Bank**

File: `state/src/engine/turn.rs`, lines 2597-2604

```rust
let mut central_bank = crate::state::central_bank::CentralBank::default();
central_bank.id = "BC_ILIRIA".to_string();
crate::securities::knf::process_knf_compliance(
    &mut task.ctx.country.knf,
    &mut task.companies,
    &mut task.ctx.country.budget,
    &mut central_bank,  // <-- DEFAULT CB, not the country's actual CB
```

This means KNF audits run against a fresh, empty CentralBank instead of the country's actual CB with its real interest rates and reserves.

**1.4 Investment Funds: Active but Disconnected from Debt Market**

Files: `state/src/securities/funds.rs`, `state/src/engine/turn.rs` lines 2495-2593

Investment Funds ARE called from the turn engine (`collect_fund_capital`, `submit_fund_orders`, `charge_fund_fees`). However:
- They only participate in the **equity** market (stock exchange), not the sovereign debt market
- No fund invests in treasury bonds or secondary-market debt
- Hedge funds are not separately implemented — they use the same `fund_type` field

### Implementation Plan

**Step 1: Taylor Rule Reference Rate with Configurable Targets**

File: `state/src/state/central_bank.rs`

**Constraint 1 compliance**: Add two new serialized fields to `CentralBank`:

```rust
/// Target inflation rate (e.g., 0.02 for 2%). Serialized, adjustable.
#[serde(default = "default_target_inflation")]
pub target_inflation: f64,

/// Potential/long-run GDP growth rate (e.g., 0.02 for 2%). Serialized, adjustable.
#[serde(default = "default_potential_growth")]
pub potential_growth: f64,

/// Neutral real interest rate (e.g., 0.02 for 2%). Serialized, adjustable.
#[serde(default = "default_neutral_rate")]
pub neutral_rate: f64,
```

With default functions:
```rust
fn default_target_inflation() -> f64 { 0.02 }
fn default_potential_growth() -> f64 { 0.02 }
fn default_neutral_rate() -> f64 { 0.02 }
```

Replace the fixed-step logic in `update_reference_rate` with a Taylor Rule:

```
reference_rate = neutral_rate + 1.5 * (inflation - target_inflation) + 0.5 * (gdp_growth - potential_growth)
```

- Uses `self.target_inflation` and `self.potential_growth` (serialized fields, NOT hardcoded)
- Uses `self.neutral_rate` as the real neutral rate
- Floor at 0%, cap at 20%
- Mandate modifiers: Inflationary mandate weighs inflation gap 2.0×; Market mandate weighs growth gap 1.5×
- Set initial `reference_rate` to 0.02 (2%) in `build_central_bank` (generator/mod.rs)
- The `update_reference_rate` signature changes to drop the `target_inflation` parameter (now read from `self.target_inflation`)

**Step 2: Real GDP Growth Calculation**

File: `state/src/state/banking.rs`, `process_banking_turn` line 1952

Replace `let gdp_growth = 0.02;` with actual GDP growth computed from telemetry history:

```rust
let gdp_growth = if macro_data.telemetry_history.len() >= 2 {
    let prev_gdp = macro_data.telemetry_history[macro_data.telemetry_history.len() - 2].gdp;
    let cur_gdp = country.budget.gdp;
    if prev_gdp > 0.0 { (cur_gdp - prev_gdp) / prev_gdp } else { 0.02 }
} else {
    0.02
};
```

Also update the `update_reference_rate` call to drop the `target_inflation` parameter (now read from `self.target_inflation`):

```rust
country.central_bank.update_reference_rate(
    inflation,
    gdp_growth,
    current_turn,
);
```

**Step 3: Sovereign Bond Yields as CB Spread**

File: `state/src/economy/finance/debt_market.rs` or `state/src/politics/budget_lifecycle.rs`

When issuing new treasury securities, set `coupon_rate = cb_reference_rate + credit_spread` where:
- `credit_spread` depends on `credit_rating` (AAA = +0.5%, AA = +1.0%, A = +1.5%, BBB = +2.5%, etc.)
- For existing saves, the legacy `public_debt.interest_rate` field is only used for the initial legacy bond; all new issuance uses the CB-linked rate

**Step 4: Fix KNF to Use Country's Actual CB**

File: `state/src/engine/turn.rs`, lines 2597-2604

Replace the default CentralBank with the country's actual CB:

```rust
crate::securities::knf::process_knf_compliance(
    &mut task.ctx.country.knf,
    &mut task.companies,
    &mut task.ctx.country.budget,
    &mut task.ctx.country.central_bank,  // Use actual CB
    ...
);
```

Note: This requires checking whether `process_knf_compliance` takes `&mut CentralBank` or `&CentralBank`. If `&mut`, we need to ensure no double-borrow with the banking step.

**Step 5: Investment Fund Debt Market Participation (Strict Double-Entry)**

File: `state/src/securities/funds.rs`

**Constraint 2 compliance**: Add a `submit_fund_bond_orders` function that allows fixed-income funds to participate in sovereign debt markets with STRICT double-entry accounting:

- **Primary market (DSPW auctions)**: When the Treasury issues new securities, funds with `fund_type = FixedIncome` or `Balanced` may bid. The purchase cost is DEBITED from the fund's `brokerage_account.cash` (or `available_cash` fallback) and CREDITED to the Treasury's `liquid_reserves`. If the fund's cash is insufficient, the bid is REJECTED — no magical bond absorption.
- **Secondary market**: Funds may buy sovereign bonds from DSPW banks. The purchase cost is DEBITED from the fund's cash and CREDITED to the selling bank's `brokerage_account.cash` or `reserves_at_central_bank`. The bond's `face_value` is transferred from the bank's `securities` to a new `fund_securities_holdings` tracker on the fund.
- **Coupon income**: Each turn, funds holding sovereign bonds receive coupon payments from the Treasury. The coupon amount is DEBITED from `Treasury.liquid_reserves` and CREDITED to the fund's cash.
- **No liquidity = no purchase**: Every bond purchase checks `fund_cash >= purchase_cost`. If false, the order fails silently (logged as a diagnostic message).

Add a `FundBondHolding` struct to track fund-owned sovereign bonds:
```rust
pub struct FundBondHolding {
    pub security_id: String,
    pub face_value: f64,
    pub purchase_price: f64,
    pub coupon_rate: f64,
    pub last_coupon_turn: u32,
}
```

Call `submit_fund_bond_orders` from the turn engine after debt issuance (after `issue_treasury_securities`).

---

## PART 2: Demographic & Aggregation Lie

### Root Cause Analysis

**2.1 Top-Down Population Model**

File: `state/src/economy/labor/labor.rs`, line 399

```rust
budget.population = new_population;
```

`new_population` comes from a demographic model (births, deaths, etc.) that operates on the national level. The Phase 35 fix (lines 401-436) distributes the *delta* proportionally across regions, but:

1. **Rounding errors accumulate**: Each turn, `pop_delta * share` is rounded, and the rounding residue fix only adjusts the last region's `population`, not its class demographics
2. **Class demographics diverge from region population**: The delta is distributed to `region.population` AND to `class_demographics.population` independently, so `sum(class.population) != region.population` after a few turns
3. **Migration bypasses regions entirely**: `migration.rs` lines 380, 395, 511 modify `budget.population` directly without touching any region or class demographics

**2.2 Initial Load Mismatch**

`Treasury.population` (u64) is loaded from JSON ("populacja"). Region populations are loaded from separate JSON fields. There is no guarantee that `treasury.population == sum(region.population)` at load time. The simulation starts with a mismatch that only grows.

### Implementation Plan

**Step 1: Strict Bottom-Up Population Aggregation**

File: `state/src/economy/labor/labor.rs`, end of `process_labor_turn`

After all population modifications (labor model, migration, casualties), add a strict reconciliation:

```rust
// Phase 36: STRICT bottom-up aggregation.
// region.population = sum(rural_classes.population) + sum(urban_classes.population)
// budget.population = sum(region.population)
for region in &mut country.regions {
    let rural_sum: i64 = region.class_demographics.rural_classes.values().map(|d| d.population).sum();
    let urban_sum: i64 = region.class_demographics.urban_classes.values().map(|d| d.population).sum();
    region.population = rural_sum + urban_sum;
}
let total_pop: u64 = country.regions.iter().map(|r| r.population).filter(|p| *p > 0).sum::<i64>() as u64;
country.budget.population = total_pop;
country.macro_indicators.demographics.population_size = total_pop as f64;
```

This runs AFTER all population changes, ensuring the national total always equals the exact sum of all class demographics across all regions.

**Step 2: Migration Must Update Regions**

File: `state/src/economy/labor/migration.rs`, lines 376-397

When applying migration outflows and inflows, distribute the population change to the country's regions proportionally (same pattern as the labor model). Then the Step 1 reconciliation will enforce consistency.

Alternatively, simply remove the direct `budget.population` modifications in migration.rs and let the Step 1 reconciliation handle the national total after regional demographics are updated. The migration cohort system already creates immigrant cohorts in demographics — the reconciliation will pick those up.

**Step 3: Post-Load Reconciliation**

File: `state/src/engine/turn.rs` or `state/src/io/save_manager.rs`

After loading a save, run the same strict bottom-up aggregation to fix any pre-existing mismatch. This ensures old saves start in a consistent state.

---

## PART 3: Election Deadlock & The "Socjalliberalizm" Ghost

### Root Cause Analysis

**3.1 All Ideology Bids Return 0**

File: `state/src/politics/turn.rs`, `regenerate_parties` function, line 521

```rust
let total_support: f64 = new_parties.values().map(|p| p.support).sum();
if total_support == 0.0 {
    // Create "Provisional Technocratic Government" with "Socjalliberalizm"
}
```

All bids are 0 because `base_bid(ig_power)` (ideology.rs line 339) sums `interest_group.total_political_weight * weight` for each group. If interest groups have no power (all `total_political_weight = 0`), all bids are 0.

Interest group power is calculated by `calculate_interest_groups_power` (interest_groups.rs line 344), which uses `calculate_nominal_power` (line 125). This function maps class demographics to interest groups via `class_group_mapping`. If the mapping is empty or misconfigured, all population goes to the `default_group`, which doesn't match any ideology's base weights.

**3.2 Polish Strings in Provisional Government**

File: `state/src/politics/turn.rs`, lines 527, 532

```rust
let leader = super::names::vip_to_leader(vip, "Socjalliberalizm");
// ...
ideology: "Socjalliberalizm".to_string(),
profile: "Centrum".to_string(),
economic_school: "Monetarystyczna".to_string(),
base: vec!["Biurokraci".to_string(), "Specjaliści".to_string()],
```

These Polish strings are baked into the `Party` struct and displayed verbatim by the UI. The Phase 35 ideology translation only affected the `Ideology` enum's serde renames and `as_str()` — it did not touch these hardcoded fallback strings.

**3.3 Election Cycle Never Advances**

The `years_to_elections` countdown (line 155-157) decrements by 1 each year. But if the provisional government is the only party, it wins with 100% support every election. The `election_due` check (line 159) does trigger, elections are held (line 163-195), but the result is always the same: the provisional party wins, `years_to_elections` is reset to `election_cycle()` (4 years), and the cycle repeats.

The real issue is that the provisional government is supposed to be a *fallback* — it should only exist when no parties can form. But because interest groups have no power, no real parties are ever generated, and the fallback is permanent.

### Implementation Plan

**Step 1: Fix Provisional Government Strings**

File: `state/src/politics/turn.rs`, lines 527-540

Replace Polish strings with English:

```rust
let leader = super::names::vip_to_leader(vip, "Social Liberalism");
// ...
ideology: "Social Liberalism".to_string(),
profile: "Centrist".to_string(),
economic_school: "Monetarist".to_string(),
base: vec!["Bureaucrats".to_string(), "Specialists".to_string()],
```

**Step 2: Party Ideology Migration for Existing Saves**

File: `state/src/politics/turn.rs`, in `process_political_year` or a migration function

At the start of `process_political_year`, iterate all `active_parties` and translate any Polish ideology strings to English using `Ideology::from_name()`:

```rust
for party in country.politics.active_parties.values_mut() {
    if let Some(ideo) = Ideology::from_name(&party.ideology) {
        party.ideology = ideo.as_str().to_string();
    }
    // Also translate profile and economic_school if needed
}
```

**Step 3: Ensure Interest Groups Have Power**

File: `state/src/politics/interest_groups.rs` or `state/src/politics/turn.rs`

The `class_group_mapping` must be populated. If it's empty (default), inject a default mapping that covers all common class keys. The mapping maps rural/urban class keys (e.g., "Chłopi", "Proletariat", "Burżuazja") to interest group names (e.g., "Agrykolanie", "Związki Zawodowe", "Kapitaliści").

If the mapping is already populated but class keys don't match, add a fallback: for any unmapped class key, distribute population to the most thematically appropriate group based on the class name.

**Step 4: Provisional Government Escape Hatch**

File: `state/src/politics/turn.rs`, `regenerate_parties`

If the provisional government has been in power for more than 4 years (configurable), force-generate at least 3 real parties with non-zero support from a default ideology distribution. This breaks the permanent deadlock even if interest group power is temporarily zero.

---

## PART 4: Ghost Banking, Sector ToT & Missing VWAP

### Root Cause Analysis

**4.1 Banking Employment at Zero**

Files: `state/src/engine/generator/mod.rs` line 871, `state/src/state/banking.rs` line 2575

Only ONE bank is generated per country (`vec![company]` at line 935). The generation code DOES set `target_fte_demand`, `physical_fte_demand`, and `offered_wage_per_fte` (lines 931-933).

However, the Phase 35 banking step (banking.rs line 2575-2587) OVERWRITES `target_fte_demand` and `physical_fte_demand` based on loan portfolio, but does NOT update `offered_wage_per_fte`. If the bank has no loans (portfolio = 0), `fte_demand = max(2.0, 0) = 2.0`. But the labor market also checks `max_affordable_fte = cash / offered_wage_per_fte`. If the bank's `brokerage_account.cash` or `available_cash` is 0 (consumed by operations or never properly initialized), the bank can't hire.

**4.2 Sector ToT Always 0%**

File: `state/src/engine/turn.rs`, lines 3255-3273

At the END of each turn, `_prev_employment` and `_prev_avg_wage` are stored:

```rust
share.extra.insert("_prev_employment".to_string(), serde_json::Value::from(*fte));
```

The snapshot (snapshot.rs line 1024) reads `_prev_employment` and compares it to the current sector data. But since the snapshot is built AFTER the turn completes, `_prev_employment` already contains the CURRENT turn's data. The comparison is current-to-current, yielding 0% ToT.

**4.3 VWAP 0.00 for Construction Materials — State Tender Cash Not Reaching B2B**

File: `state/src/construction/orders.rs`, lines 84-86

The root cause is NOT merely a lack of corporate loans. `submit_construction_b2b_orders` uses the contractor's `computed_liquid_capital()` as the encumbrance source:

```rust
let liquid = company.computed_liquid_capital();
company.available_cash = liquid;
let max_encumber = liquid * config.max_cash_encumbrance_ratio;
```

However, State and Regional `ConstructionTenders` have guaranteed Treasury cash via `estimated_cost` and tranche payments (`release_construction_tranches`, orders.rs line 287). The tranche system releases milestone payments from the Treasury to the contractor as progress is made. But at progress = 0 (project just started), NO tranches have been released yet, so the contractor has only their own thin liquid capital to bid for materials.

The result: the contractor can't afford to buy materials → no B2B trades → VWAP stays at 0 → no progress → no tranches released → deadlock.

### Implementation Plan

**Step 1: Fix Bank Wage Setting in Banking Step**

File: `state/src/state/banking.rs`, Step 15 (line 2575)

In addition to setting `target_fte_demand`, also set `offered_wage_per_fte` based on the bank's available cash and the national average wage:

```rust
let avg_wage = country.macro_indicators.average_wage.max(1.0);
let bank_cash = bank.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(bank.available_cash);
let payroll_budget = bank_cash * 0.3; // 30% of cash for payroll
let wage = (avg_wage * 1.2).max(1.0); // Banks pay 20% above average
bank.offered_wage_per_fte = wage;
// Ensure FTE demand doesn't exceed what the bank can afford
let max_affordable = payroll_budget / wage;
bank.target_fte_demand = fte_demand.min(max_affordable).max(2.0);
bank.physical_fte_demand = bank.target_fte_demand;
```

**Step 2: Generate Multiple Banks Based on Region Count**

File: `state/src/engine/generator/mod.rs`, `build_bank_companies`

Change from `vec![company]` to generating `max(1, regions.len() / 3)` banks, with at least one DSPW primary dealer. Each bank is assigned to a different region. This ensures banking services are distributed across the country.

**Step 3: Fix Sector ToT by Moving Storage to Turn Start**

File: `state/src/engine/turn.rs`

Move the `_prev_employment` storage from the END of the turn (line 3255) to the BEGINNING of the turn (before any company processing). This way, `_prev_employment` captures the previous turn's end state, and the snapshot (built after the turn) compares current to previous.

Alternatively, use a two-slot approach: store current as `_cur_employment` at end of turn, and in the snapshot, compare `_cur_employment` to `_prev_employment`. At the start of the next turn, move `_cur_employment` to `_prev_employment`.

**Step 4: State Construction Tender Priority — B2B Bidding with Guaranteed Escrow**

File: `state/src/construction/orders.rs`, `submit_construction_b2b_orders`

**Constraint 3 compliance**: The contractor's B2B bidding capacity must reflect the guaranteed Treasury escrow from State Tenders, not just their own liquid capital.

The fix has two parts:

**Part A: Advance tranche payment at project start**

When a State-backed ConstructionProject is created (investor_id starts with `"STATE:"`), immediately release the first tranche (mobilization payment, typically 20-30% of contract_price) from the Treasury to the contractor. This gives the contractor cash to start buying materials. This mirrors real-world construction where mobilization advances are paid at contract signing.

File: `state/src/construction/tender_market.rs`, `award_tender` function

After creating the project, if `is_state_investor`, call `settle_treasury_to_company` for the first tranche amount and mark it as released.

**Part B: B2B encumbrance includes pending tranche payments**

File: `state/src/construction/orders.rs`, `submit_construction_b2b_orders`

For each company with active construction projects, compute the B2B encumbrance ceiling as:

```rust
let liquid = company.computed_liquid_capital();
let pending_tranche_value: f64 = buildings.iter()
    .filter(|b| b.active_project.as_ref().map_or(false, |p|
        p.main_contractor_id == company.id &&
        p.tranches.iter().any(|t| !t.released)
    ))
    .flat_map(|b| b.active_project.as_ref().unwrap().tranches.iter()
        .filter(|t| !t.released)
        .map(|t| t.amount))
    .sum();
let max_encumber = (liquid + pending_tranche_value * 0.5) * config.max_cash_encumbrance_ratio;
```

This allows the contractor to bid for materials using 50% of their pending tranche value as additional bidding capacity, backed by the guaranteed Treasury escrow. The 50% haircut prevents over-commitment.

**Part C: Ensure material producers exist**

If construction material producers (cement, steel, bricks) have zero production capacity, no sell orders will exist. The corporate generator should ensure at least one producer per critical material per region. This is already partially handled by the Phase 20A "minimum viable supply chain" seeding, but may need verification.

---

## PART 5: Megaregion Naming

### Root Cause Analysis

File: `state/src/society/geography.rs`, line 2255

```rust
pub fn generate_megaregions(country: &str, region_ids: &[String]) -> Megaregion {
    Megaregion {
        id: format!("MEG-{country}"),
        name: format!("Megaregion {country}"),
        ...
    }
}
```

The megaregion name is a generic `"Megaregion {country}"` string. No culturally appropriate name generation is applied, unlike the Phase 35 region name generator.

### Implementation Plan

**Step 1: Megaregion Name Generator**

File: `state/src/society/geography.rs`

Add a `generate_megaregion_name` function that produces culturally appropriate names based on:
- The country name
- Geographic descriptors (e.g., "Central", "Northern", "Southern", "Coastal", "Highland")
- Cultural suffixes (e.g., "Voivodeship", "Governorate", "Province", "Prefecture")

Example output: "Central Iliria Voivodeship", "Nordia Coastal Province"

Update `generate_megaregions` to call this function.

**Step 2: Add `display_name` to Megaregion (Optional)**

If the `Megaregion.name` field is used in serialization, add a separate `display_name` field (with `serde(default)`) to avoid breaking saves, similar to the Phase 35 `Region.display_name` approach.

---

## Implementation Order

1. **Step 1 (Part 2)**: Strict bottom-up population aggregation — fixes the most visible bug
2. **Step 1 (Part 1)**: Taylor Rule with configurable CB targets + real GDP growth — awakens the CB
3. **Step 2 (Part 1)**: Sovereign bond yields as CB spread — connects debt to CB
4. **Step 3 (Part 1)**: Fix KNF to use actual CB — correct regulation
5. **Step 4 (Part 3)**: Fix provisional government strings + party migration — kills the ghost
6. **Step 5 (Part 3)**: Ensure interest groups have power + election escape hatch — breaks deadlock
7. **Step 6 (Part 4)**: Fix bank wage setting + multiple banks — banking employment
8. **Step 7 (Part 4)**: Fix Sector ToT storage timing — correct ToT display
9. **Step 8 (Part 4)**: State construction tender priority — mobilization advance + B2B escrow bidding
10. **Step 9 (Part 5)**: Megaregion name generator — polish
11. **Step 10 (Part 1)**: Investment fund debt participation (strict double-entry) — market depth
12. **Step 11**: Build, test, and verify

## Verification

- `cargo build --lib` and `cargo build` — 0 errors
- `cargo test --lib` — all tests pass
- Long-run simulation verification:
  - CB reference rate > 0% and varies with inflation (Taylor Rule active)
  - `CentralBank.target_inflation` and `potential_growth` are serialized and visible
  - Sovereign bond yields > CB reference rate (credit spread applied)
  - KNF audits run against the country's actual CentralBank
  - `budget.population == sum(region.population) == sum(class.population)` (strict bottom-up)
  - Elections produce multiple parties, not provisional government
  - No "Socjalliberalizm" or other Polish strings in UI
  - Banking sector has > 0 employees
  - Sector ToT shows non-zero deltas
  - VWAP > 0 for construction materials (State tender mobilization + B2B escrow)
  - Investment fund bond purchases debit fund cash and credit Treasury/seller
  - Megaregions have proper culturally appropriate names

## Risks/Considerations

- **Backward compatibility**: New CB fields use `serde(default = "...")` with sensible defaults. Migration functions translate old Polish strings. Fund bond holdings use `serde(default)`.
- **Performance**: Bottom-up population aggregation is O(regions × classes) per turn — negligible.
- **Double-borrow**: KNF fix requires careful borrow management if `process_knf_compliance` takes `&mut CentralBank`. May need to reorder turn steps to avoid conflicting with banking step.
- **Interest group mapping**: The `class_group_mapping` configuration is data-driven. If the config file is missing, a hardcoded fallback must be provided.
- **Construction mobilization advance**: Releasing the first tranche at project start means the Treasury pays before any work is done. This is realistic (mobilization advances are standard in public procurement) but must be capped (e.g., 20% of contract_price) to prevent abuse.
- **Fund double-entry**: Every bond purchase must atomically check-and-debit fund cash. If the fund's cash is modified between the check and the debit (race condition in parallel processing), the double-entry could break. Funds must be processed sequentially or the check-and-debit must be atomic.
