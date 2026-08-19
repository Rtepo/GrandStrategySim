# Phase 43 — Economic Defibrillation Audit

**Summary:** A read-only audit tracing 5 root causes: (1) bank reserves go negative because `adjust_bank_balance` doesn't clamp at zero and labor-market wage debits bypass bank sync, (2) PIT/CIT display as 0.00 because `aggregate_citizen_savings` only sums rural classes and the tax_result doesn't include labor-withheld PIT, (3) small sectors start at 0 FTE because banks and some generators lack the Genesis Labor Fix, (4) committees are invisible because `committee_system` is never initialized, and (5) FX reserves accumulate fake "IEU" because the Phase 42 code hardcoded it instead of looking up the foreign country's real currency code.

---

## PART 1: Negative Bank Reserves & 0.00 Tax Mystery

### 1.1 Bank Reserves Going Negative

**Symptom:** Finance tab shows Bank Reserves at `-9.2K`. This is a catastrophic double-entry violation.

**Root Cause A — `adjust_bank_balance` has no floor:**

**File:** `state/src/economy/trade/transfer_settler.rs:84-98`
```rust
fn adjust_bank_balance(
    companies: &mut [Company],
    bank_id: &str,
    deposit_delta: f64,
    reserve_delta: f64,
) -> bool {
    if let Some(bank) = companies.iter_mut().find(|c| c.id == bank_id) {
        if let Some(ref mut bs) = bank.balance_sheet {
            bs.deposits += deposit_delta;
            bs.reserves_at_central_bank += reserve_delta;
            return true;
        }
    }
    false
}
```

This function blindly applies `reserve_delta` (which is negative for outbound transfers) without checking if the result goes below zero. When a company's `brokerage_account.cash` is sufficient (checked at `transfer_settler.rs:153`) but the bank's `reserves_at_central_bank` is less than the transfer amount, the bank's reserves go negative.

**Root Cause B — Labor market debits bypass bank sync entirely:**

**File:** `state/src/economy/labor/labor_market.rs:392-434`

The labor market debits `company.brokerage_account.cash` or `company.available_cash` directly:
```rust
if let Some(ba) = &mut company.brokerage_account {
    ba.cash -= wage_payment;
} else {
    company.available_cash -= wage_payment;
}
```

This does NOT call `settle_transfer()` or `adjust_bank_balance()`. So when a company pays wages:
- Company's deposit decreases (brokerage_account.cash) ✓
- Bank's deposits liability decreases ✗ (NOT synced)
- Bank's reserves_at_central_bank decreases ✗ (NOT synced)

This creates a divergence: the company's deposit shrinks but the bank's balance sheet still shows the old deposit amount. Later, when `settle_transfer` IS called (e.g., for B2B trades), the bank's reserves are debited based on the transfer amount, but the bank's deposits haven't been reduced proportionally by the wage payments. Over time, reserves drift negative.

**Root Cause C — Multiple unclamped deductions in `banking.rs`:**

**File:** `state/src/state/banking.rs`

Several operations debit `reserves_at_central_bank` without clamping:
- Line 1117: `bs.reserves_at_central_bank -= premium;` (BFG premium)
- Line 1262: `bs.reserves_at_central_bank -= contribution;` (deposit insurance)
- Line 2070: `bs.reserves_at_central_bank -= interest;` (Lombard interest)

**Fix Plan:**
1. **Clamp `adjust_bank_balance`**: After applying `reserve_delta`, clamp `reserves_at_central_bank` at `0.0`. If the bank can't afford the debit, the transfer should fail or be partial.
2. **Sync bank balance sheets in labor market**: After debiting `company.brokerage_account.cash` for wages, also call `adjust_bank_balance` to reduce the bank's deposits and reserves by the same amount. This maintains double-entry consistency.
3. **Clamp all direct `reserves_at_central_bank -=` operations** in `banking.rs` to not go below zero. If a bank can't afford an operation, it should fail or accrue arrears.

### 1.2 Taxes Displaying as 0.00

**Symptom:** PIT, CIT, Wealth Tax all display as 0.00 in the Finance tab despite state debt dropping.

**Root Cause A — `aggregate_citizen_savings` only sums rural classes:**

**File:** `state/src/economy/labor/labor.rs:590-596`
```rust
fn aggregate_citizen_savings(country: &mut crate::state::Country) {
    let total: f64 = country.regions.iter()
        .flat_map(|r| r.class_demographics.rural_classes.values())
        .map(|d| d.savings)
        .sum();
    country.budget.citizen_savings = total;
}
```

This ONLY sums `rural_classes` savings, completely ignoring `urban_classes` savings. Since most of the population is urban and most wages go to urban classes, `country.budget.citizen_savings` is severely understated. PIT collection (capped by `citizen_savings` at `tax.rs:1307`) collects almost nothing.

**Root Cause B — PIT is withheld at source but not reported in tax_result:**

**File:** `state/src/engine/turn.rs:2105-2110`
```rust
if labor_alloc.pit_withheld > 0.0 {
    task.ctx.country.budget.liquid_reserves += labor_alloc.pit_withheld;
}
```

PIT is actually collected during labor market clearing (withheld at source) and credited directly to the treasury. But this amount is NOT reflected in `tax_result.pit_collected` or `tax_result.actual_pit_collected`. The `process_tax_collection_turn` at line 2761 calculates PIT independently from `total_wages * rate`, caps it at `citizen_savings` (which is near-zero due to Root Cause A), and stores THAT near-zero value in `tax_result`. The Finance tab displays `tax_result`, so it shows 0.00 even though PIT was actually collected.

**Root Cause C — CIT display mismatch:**

CIT IS being calculated and debited correctly (liabilities are recorded in `tax.rs:1360`, debited in `turn.rs:2778-2794`, and `total_cit_debited` is stored in `tax_result_stored.cit_collected` at `turn.rs:2822`). However, if companies have no profits (`building.last_profit <= 0`), CIT is 0. In early turns, many companies may not have profitable production cycles yet.

**Fix Plan:**
1. **Fix `aggregate_citizen_savings`**: Add `urban_classes` to the sum:
   ```rust
   let total: f64 = country.regions.iter()
       .flat_map(|r| r.class_demographics.rural_classes.values()
           .chain(r.class_demographics.urban_classes.values()))
       .map(|d| d.savings)
       .sum();
   ```
2. **Include withheld PIT in tax_result**: After labor market clearing, add `labor_alloc.pit_withheld` to the stored tax_result so the Finance tab displays the actual total PIT collected (withheld at source + any additional collected by `process_tax_collection_turn`).
3. **Avoid double-counting**: Since PIT is withheld at source AND `process_tax_collection_turn` tries to collect PIT from `citizen_savings`, we must ensure we don't double-collect. The simplest fix is to set `result.pit_collected = 0` and `result.actual_pit_collected = 0` in `process_tax_collection_turn` (since PIT is already withheld), and instead add the withheld amount to the stored tax_result.

---

## PART 2: Labor Genesis Floor & Mining Mismatch

### 2.1 Genesis Floor — Small Sectors Start at 0 Employees

**Symptom:** Turn 1 employment in Banking and LocalServices is exactly 0.

**Root Cause A — Banks have no Genesis Labor Fix:**

**File:** `state/src/engine/generator/mod.rs:993-1011`

Banks are created via `Company::new()` which sets `fulfilled_fte: 0.0`. Unlike the corporate generators in `corporate.rs`, the bank generator does NOT apply the Phase 42 Genesis Labor Fix. Banks start with:
- `fulfilled_fte: 0.0` (no workers)
- `operating_cash = tier_1_capital * 0.1` (very small payroll budget)
- `target_fte_demand: bank_fte` (100-300 FTE demand, but 0 fulfilled)

**Root Cause B — `initial_fte` rounds to 0 for very small capacities:**

**File:** `state/src/engine/generator/corporate.rs:623`
```rust
let initial_fte = (actual_capacity as f64 * 0.6).round();
```

If `actual_capacity` is 1, then `0.6 * 1 = 0.6`, which rounds to 1. But if `actual_capacity` is 0 (which can happen for edge cases), `initial_fte = 0`.

**Fix Plan:**
1. **Apply Genesis Labor Fix to banks**: After creating each bank company, set:
   ```rust
   let initial_fte = (bank_fte * 0.6).max(2.0);
   company.fulfilled_fte = initial_fte;
   company.prev_fulfilled_fte = initial_fte;
   let payroll_grant = initial_fte * bank_wage * 3.0;
   company.available_cash += payroll_grant;
   ```
2. **Enforce minimum floor of 2.0 FTE**: In all corporate generators, change:
   ```rust
   let initial_fte = (actual_capacity as f64 * 0.6).round();
   ```
   to:
   ```rust
   let initial_fte = (actual_capacity as f64 * 0.6).round().max(2.0);
   ```

### 2.2 Mining Mismatch — 190 Companies but 40 Active Miners

**Symptom:** ~190 mining companies exist but geology deposits show only ~40 active miners.

**Root Cause — One mining company per commodity per region, not per deposit:**

**File:** `state/src/engine/generator/corporate.rs:1245-1255`
```rust
let mut deposits_by_commodity: BTreeMap<Commodity, String> = BTreeMap::new();
for formation in &country.geological_formations {
    for (key, deposit) in &formation.resource_deposits {
        deposits_by_commodity
            .entry(deposit.commodity)
            .or_insert_with(|| format!("{}/{}", formation.id, key));
    }
}
```

The `deposits_by_commodity` map uses `Commodity` as the key with `or_insert_with`, so only the FIRST deposit for each commodity is recorded. If a formation has 5 HardCoal deposits, only 1 gets a mining company. The other 4 deposits will show 0 active miners in the snapshot.

Additionally, the fallback at line 1272 uses `find_any_deposit_for_commodity` which could return `None`, leaving the building with `deposit_id = None` (not counted as an active miner).

**Fix Plan:**
1. **Change `deposits_by_commodity` to `Vec<(Commodity, String)>`**: Instead of deduplicating by commodity, keep ALL deposits and create one mining company per deposit:
   ```rust
   let mut all_deposits: Vec<(Commodity, String)> = Vec::new();
   for formation in &country.geological_formations {
       for (key, deposit) in &formation.resource_deposits {
           all_deposits.push((deposit.commodity, format!("{}/{}", formation.id, key)));
       }
   }
   ```
2. **Enforce strict linkage**: Every mining company MUST have a `deposit_id`. If no deposit is found, skip creating the company entirely (don't create unlinked fallback mines).
3. **Limit company count**: To avoid entity explosion, cap at 1 mining company per deposit (not per commodity), and skip deposits that already have a mining company in a different region.

---

## PART 3: Market UI Overhaul (Supply & Demand)

**Symptom:** Market tab shows prices locked at 100.00 base value, and the `Last` column is useless.

**Current State:**

**File:** `state/src/ui/tui/render.rs:196-203`
```
Header: Commodity | VWAP | Last | Base | Balance | ToT %
```

The `Last` column shows `c.last_trade` which is often 0 or stale. The `Balance` column shows `net_surplus` (sell - buy) but doesn't show the raw supply and demand volumes.

**Data Availability:**

**File:** `state/src/ui/tui/app.rs:929-935`

The market.json file already contains `orders` with `buy` (demand) and `sell` (supply) volumes per commodity. This data is currently parsed and used to compute `net_surplus = sell - buy`, but the raw `buy` and `sell` values are NOT preserved in the `CommodityRow` struct.

**Fix Plan:**
1. **Add `supply_volume` and `demand_volume` fields to `CommodityRow`**:
   ```rust
   pub struct CommodityRow {
       pub name: String,
       pub vwap: f64,
       pub last_trade: f64,
       pub base_price: f64,
       pub net_surplus: f64,
       pub tot_balance_change: f64,
       pub active: bool,
       pub supply_volume: f64,   // NEW: total sell order volume
       pub demand_volume: f64,   // NEW: total buy order volume
   }
   ```

2. **Parse and store supply/demand in `load_global_market`**: In `app.rs:929-935`, store the raw `buy` and `sell` values alongside `net_surplus`. Pass these through to the snapshot builder.

3. **Replace `Last` column with `Supply` and `Demand`**: In `render_market_logistics`:
   ```
   Header: Commodity | VWAP | Supply | Demand | Base | Balance | ToT %
   ```
   Remove the `Last` column. Add `Supply` (green) and `Demand` (red) columns showing raw volumes. This lets the user see exactly why prices aren't moving (e.g., massive demand but 0 supply).

4. **Adjust column constraints**: Update the `Constraint` array to accommodate 7 columns instead of 6.

---

## PART 4: Cloned Speakers & Invisible Committees

### 4.1 Cloned Speakers

**Symptom:** "Natalia Mik" is the Speaker for BOTH the Sejm and Senate simultaneously.

**Root Cause:**

**File:** `state/src/politics/parliament.rs:322,346`

```rust
// Lower chamber:
let speaker = generate_speaker(politics, cultural_group, rng);

// Upper chamber:
let upper_speaker = generate_speaker(politics, cultural_group, rng);
```

Both calls to `generate_speaker` use the same `politics` struct. Inside `generate_speaker` (line 434-462):
```rust
let full_name = if let Some(p) = party {
    if !p.leader.name.is_empty() {
        p.leader.name.clone()
    } else {
        generate_full_vip(cultural_group, rng).full_name
    }
} else {
    generate_full_vip(cultural_group, rng).full_name
};
```

If the ruling party leader's name is non-empty, BOTH speakers get the SAME name — the ruling party leader's name. There is no `used_names` set shared between the two calls.

**Fix Plan:**
1. **Pass a shared `used_names: &mut HashSet<String>` to `generate_speaker`**: The lower chamber speaker is generated first, adding their name to `used_names`. The upper chamber speaker is generated second, skipping any name already in `used_names`.
2. **If the party leader's name is already used, generate a new unique VIP**: Instead of blindly cloning the party leader's name, check if it's in `used_names`. If so, generate a fresh name.

### 4.2 Invisible Committees

**Symptom:** Committees were supposed to be added to the UI in Phase 42, but they are invisible.

**Root Cause — `committee_system` is NEVER initialized:**

**File:** `state/src/politics/committees.rs:294`
```rust
pub fn initialize_committees(
    &mut self,
    parliament: &HashMap<String, u32>,
    ruling_coalition: &[String],
) {
```

The `initialize_committees` method exists but is NEVER called anywhere in the codebase. A grep for `committee_system = Some` or `.initialize_committees` returns zero results. So `country.politics.committee_system` remains `None` forever, and the snapshot at `snapshot.rs:1156-1173` gets an empty vec.

**Fix Plan:**
1. **Call `initialize_committees` during government formation**: In `politics/turn.rs` or `politics/ministries.rs`, after elections and government formation, create a `CommitteeSystem` and call `initialize_committees`:
   ```rust
   let mut cs = CommitteeSystem::default();
   cs.initialize_committees(&politics.parliament, &politics.coalition);
   country.politics.committee_system = Some(cs);
   ```
2. **Call it during world generation too**: Ensure `committee_system` is initialized when a new game is generated.

### 4.3 No Bills in Legislative Queue

**Symptom:** There are no bills. The legislative queue is always empty.

**Root Cause:**

Bills are only created as budget bills (once per year, `turn.rs:2730`). The `draft_budget_bill` function creates a `BudgetBill`, but this is processed through `process_budget_lifecycle`, NOT through the parliament's `legislative_queue`. The parliament's `queue_bill` method (line 272) is never called.

**Fix Plan:**
1. **Display "(No pending bills)" when the queue is empty**: In `render_parliament`, after the legislative queue section, if the queue is empty, add a row:
   ```
   ("(No pending bills)", "", "", "")
   ```
2. **Future enhancement**: Wire the budget bill (and potentially other bills) into the parliament's `legislative_queue` so they appear in the UI. This is a larger change and can be deferred.

---

## PART 5: Purge "IEU" & Fix Failing Tests

### 5.1 Purge Fake "IEU" Currency from FX Reserves

**Symptom:** FX Reserves accumulated a fake "IEU" currency instead of real foreign fiat currencies.

**Root Cause:**

**File:** `state/src/economy/trade/b2b_orders.rs:664,670`
```rust
let foreign_ccy = "IEU".to_string();  // Exports
let seller_ccy = "IEU".to_string();  // Imports
```

The Phase 42 FX conversion code hardcoded "IEU" as the foreign currency for all cross-border trades. "IEU" (International Exchange Unit) is an internal reference currency for cross-rate calculations, NOT a real fiat currency. FX reserves should accumulate the actual currency code of the foreign country (e.g., "SAR", "WEN", "HEL").

**Currency Mapping:**

**File:** `state/src/engine/generator/mod.rs:836`
```rust
let prefix = name[..3.min(name.len())].to_uppercase();
```

Each country's currency code is the first 3 letters of the country name, uppercased. The mapping from country name to currency code is stored in `GameState.currencies` (a `HashMap<String, Currency>` where each `Currency` has a `members: Vec<String>` listing country names).

**Fix Plan:**
1. **Build a country-to-currency mapping**: In `turn.rs`, before calling `settle_trades_with_tariffs`, build a `HashMap<String, String>` from country name to currency code using `state.currencies`:
   ```rust
   let mut country_to_currency: HashMap<String, String> = HashMap::new();
   for (ccy_code, currency) in &state.currencies {
       for member in &currency.members {
           country_to_currency.insert(member.clone(), ccy_code.clone());
       }
   }
   ```
2. **Pass this mapping to `settle_trades_with_tariffs`**: Add a `country_to_currency: &HashMap<String, String>` parameter.
3. **Look up the foreign country's currency**: Replace `"IEU".to_string()` with:
   ```rust
   let foreign_ccy = country_to_currency.get(&buyer_country)
       .cloned()
       .unwrap_or_else(|| "???".to_string());
   ```
4. **Clean existing IEU reserves**: On load, remove any "IEU" key from `central_bank.fx_reserves` and warn.

### 5.2 Fix 5 Failing save_manager Tests

**Symptom:** 5 tests in `io::save_manager::tests` fail with `missing field 'gdp'`.

**Root Cause:**

**File:** `state/data/budgets.json` (committed version)

The committed `data/budgets.json` uses Polish keys that don't match the English-keyed `Treasury` struct:
- `"PKB"` instead of `"gdp"`
- `"populacja"` instead of `"population"`
- `"budżet_nominalny"` instead of `"nominal_budget"`
- `"rezerwy_plynne"` instead of `"liquid_reserves"`
- `"oszczednosci_obywateli"` instead of `"citizen_savings"`
- `"kapitał_prywatny"` instead of `"private_capital"`
- `"poziom_infrastruktury"` instead of `"infrastructure_level"`
- `"infrastruktura_energetyczna"` instead of `"energy_infrastructure"`
- `"gielda"` instead of `"stock_market"`
- `"sektory"` instead of `"sectors"`
- `"nauka"` instead of `"science"`

The `Treasury` struct (`state/src/state/treasury.rs:220-281`) expects English field names without `#[serde(rename = "...")]` attributes (except for `outstanding_corporate_debts`, `liquidation_expenses`, and `logistics_revenue` which have Polish renames).

The 5 failing tests all load from `data/budgets.json`:
1. `loads_real_budget_map` — loads the file and checks GDP > 0
2. `loads_and_joins_one_country` — loads and joins country data
3. `missing_country_errors` — tests error handling
4. `real_budget_round_trip_preserves_keys` — tests key preservation
5. `real_game_state_struct_round_trip` — tests struct round-trip

**Note:** The working copy of `data/budgets.json` was previously overwritten by a simulation run (which saved with English keys), which is why the tests passed during Phase 42 development. But the committed version uses Polish keys, so the tests fail on a clean checkout.

**Fix Plan:**
1. **Write a migration script** that reads `data/budgets.json`, replaces all Polish top-level keys with English equivalents, and writes it back. The key mapping is:
   ```
   "PKB" → "gdp"
   "populacja" → "population"
   "budżet_nominalny" → "nominal_budget"
   "rezerwy_plynne" → "liquid_reserves"
   "oszczednosci_obywateli" → "citizen_savings"
   "kapitał_prywatny" → "private_capital"
   "poziom_infrastruktury" → "infrastructure_level"
   "infrastruktura_energetyczna" → "energy_infrastructure"
   "gielda" → "stock_market"
   "sektory" → "sectors"
   "nauka" → "science"
   ```
2. **Also check nested structs**: The `StockMarket` struct expects `index`, `confidence`, `last_change`, `sector_indices`. If the committed file uses Polish names for these, they need to be replaced too. The `BudgetAllocations` struct expects `industry`, `education_propaganda`, `healthcare`, etc. The `ScienceState` expects `innovation_points`, `researching`, `discovered`, `base_innovativeness`.
3. **Run the migration**: Apply the key replacements to `data/budgets.json` in-place. The `extra` flatten map will continue to preserve any remaining Polish keys losslessly.
4. **Verify all 5 tests pass**: Run `cargo test --lib io::save_manager::tests -- --test-threads=1`.

---

## Implementation Order

1. **Fix `aggregate_citizen_savings`** (1-line fix, highest impact on tax display)
2. **Include withheld PIT in tax_result** (small change in `turn.rs`)
3. **Clamp `adjust_bank_balance`** at zero (prevents negative reserves)
4. **Sync bank balance sheets in labor market** (maintains double-entry)
5. **Apply Genesis Labor Fix to banks** (fixes 0-employee banks)
6. **Enforce minimum 2.0 FTE floor** (fixes small sectors)
7. **Fix mining deposit linkage** (one company per deposit, strict linkage)
8. **Purge "IEU" from FX reserves** (use real currency codes)
9. **Fix cloned speakers** (shared `used_names` between chambers)
10. **Initialize `committee_system`** (call `initialize_committees` during government formation)
11. **Add Supply/Demand columns to Market UI** (replace Last column)
12. **Migrate `data/budgets.json` keys** (Polish → English)
13. **Build, test, verify** (all 698+ tests green)

---

## Files to Modify

- `state/src/economy/labor/labor.rs` — Fix `aggregate_citizen_savings` to include urban classes
- `state/src/engine/turn.rs` — Include withheld PIT in tax_result; build country-to-currency map; pass to `settle_trades_with_tariffs`; initialize `committee_system`
- `state/src/economy/trade/transfer_settler.rs` — Clamp `adjust_bank_balance` at zero
- `state/src/economy/labor/labor_market.rs` — Sync bank balance sheets after wage debits
- `state/src/state/banking.rs` — Clamp all direct `reserves_at_central_bank -=` operations
- `state/src/engine/generator/mod.rs` — Apply Genesis Labor Fix to banks
- `state/src/engine/generator/corporate.rs` — Enforce minimum 2.0 FTE floor; fix mining deposit linkage
- `state/src/economy/trade/b2b_orders.rs` — Replace hardcoded "IEU" with real currency codes
- `state/src/politics/parliament.rs` — Share `used_names` between lower and upper speaker generation
- `state/src/politics/turn.rs` — Call `initialize_committees` after government formation
- `state/src/ui/snapshot.rs` — Add `supply_volume` and `demand_volume` to `CommodityRow`
- `state/src/ui/tui/render.rs` — Replace `Last` column with `Supply` and `Demand`; display "(No pending bills)"
- `state/src/ui/tui/app.rs` — Parse and store raw buy/sell volumes from market.json
- `state/data/budgets.json` — Migrate Polish keys to English

## Verification

- [ ] `cargo build` succeeds with no new warnings
- [ ] `cargo test --lib -- --test-threads=1` — all 698+ tests pass (0 failures)
- [ ] Manual simulation: Bank Reserves never go negative
- [ ] Manual simulation: PIT/CIT/Wealth Tax show non-zero values in Finance tab
- [ ] Manual simulation: Banking and LocalServices sectors have >0 employees on Turn 1
- [ ] Manual simulation: Mining deposits show active miners matching company count
- [ ] Manual simulation: Market tab shows Supply and Demand columns
- [ ] Manual simulation: Parliament tab shows committees
- [ ] Manual simulation: No two chambers share the same Speaker name
- [ ] Manual simulation: FX reserves show real currency codes (SAR, WEN, etc.), not "IEU"

## Risks/Considerations

- **Labor market bank sync**: Adding `adjust_bank_balance` calls in the labor market is a significant change to a hot code path. Must ensure the bank lookup is efficient (O(n) scan of companies) and doesn't cause borrow checker issues with the current `region_companies` slice pattern.
- **PIT double-counting**: Must carefully ensure that PIT is not collected twice — once at source (labor market) and again in `process_tax_collection_turn`. The fix should either zero out the `process_tax_collection_turn` PIT or merge the two collection paths.
- **Mining company explosion**: Creating one company per deposit could significantly increase entity count. May need to cap total mining companies or merge small deposits.
- **Data migration**: The `data/budgets.json` file is 42K+ lines. A simple find-and-replace of top-level keys should work, but must verify that nested Polish keys (in `extra` maps) are not accidentally replaced.
- **Committee initialization timing**: Must initialize committees AFTER parliament seats are distributed and AFTER the ruling coalition is formed, otherwise committee chairs and membership will be wrong.
