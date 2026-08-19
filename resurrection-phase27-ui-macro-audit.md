# Phase 27: UI Overhaul, Calendar Fix & Supply/Trade Audit

**Date:** 2025-01-Phase 27
**Status:** Blueprint — revised with user corrections, awaiting approval before implementation.

## User-Mandated Corrections (Strict Realism Rules)

The following four corrections are binding on the implementation:

1. **NO Magical Goods Injection:** Every commodity must exist in a building's inventory and be paid for. No seeding `market.json` with free goods. Advanced factories/mines get their starting inputs in `building.inventory` with the cost deducted from `company.liquid_capital`. Double-entry must be preserved.
2. **Mining MUST Respect Geology (Phase 21):** Mining companies can only be spawned for a mineral if a corresponding `ResourceDeposit` exists in the region's `geological_formations`. No hardcoded "spawn Iron everywhere" loops.
3. **International Trade ON HOLD:** Do NOT implement cross-country B2B matching. The domestic economy must balance itself via proper generation. Countries lacking deposits will suffer shortages until Phase 28/29.
4. **Calendar Math Fix:** Use `if turn > 0 && turn % 24 == 0` (not bare `turn % 24 == 0`, which would fire on turn 0).

## Summary

Three critical issues are audited: (1) a calendar bug where the year increments every turn instead of every 24 turns, (2) UI/UX shortcomings including an ugly sector table, poor market tab layout, and unfiltered zero-commodity rows, and (3) a persistent macroeconomic blockage where GDP is 100% Consumption with I=0 and G=0, caused by a cascading supply-chain failure rooted in the generator's method-selection logic.

---

## PART 1: The Calendar Bug

### Root Cause

In `state/src/engine/turn.rs` at lines 3450–3452:

```rust
turn += 1;
year += 1;
update_storage(state, turn, year);
```

The year increments by 1 every single turn. In this engine, **1 Year = 24 Turns** (2 turns per month, 12 months). The year should only increment after 24 turns have passed.

### Evidence

- `storage.json` after 5 turns shows `"current_turn": 5, "year": 1905` — but starting year was 1900, so year should still be 1900 (or 1901 at most if 0-indexed).
- The TUI header shows "Turn 5 Year 1905" — 5 years have passed in 5 turns.
- The golden audit telemetry shows `Turn 0 Year 1975, Turn 1 Year 1976, ... Turn 23 Year 1998` — 24 years in 24 turns.

### Fix

**File:** `state/src/engine/turn.rs`

Replace lines 3450–3452 with:

```rust
turn += 1;
// 1 Year = 24 Turns (2 turns per month). Year only increments
// after a full year of 24 turns has passed.
// Guard with turn > 0 to avoid firing on turn 0 (game start).
if turn > 0 && turn % 24 == 0 {
    year += 1;
}
update_storage(state, turn, year);
```

Also update `state.calendar` to stay in sync:

```rust
state.calendar.global_turn = turn;
state.calendar.current_year = year;
if turn > 0 {
    state.calendar.current_month = ((turn - 1) % 24) / 2 + 1;
    state.calendar.half_month = (turn - 1) % 2 == 1;
}
```

### Verification

- Run 24 turns from 1975 start → year should be 1976 at turn 24, not 1999.
- Run 48 turns → year should be 1977.
- TUI header should show "Turn 5 Year 1975" not "Turn 5 Year 1980".

---

## PART 2: UI/UX Overhaul

### 2.1 Sector Overview — Move to Dedicated Tab 5

#### Current State

The sector overview is rendered as raw text appended to the bottom of the Macro & Finance tab (`render.rs` lines 127–139):

```rust
rows.push(("=== Sector Overview ===", String::new()));
for s in snap.sectors.iter().take(15) {
    rows.push((
        "  Sector",
        format!("{} | cos={} share={:.1}% emp={:.0} wage={:.1}", ...),
    ));
}
```

This produces an ugly wall of text with no column alignment.

#### Fix

**Step 1: Add `Sectors` variant to `Tab` enum**

**File:** `state/src/ui/tui/tabs.rs`

```rust
pub enum Tab {
    MacroFinance,
    MarketLogistics,
    ConstructionGeology,
    SocietyJustice,
    Sectors,  // NEW
}
```

Update `ALL`, `title()`, `hotkey()`, `next()`, `prev()`:
- `title()` → `"Sectors"`
- `hotkey()` → `'5'`
- `ALL` array includes `Tab::Sectors`

**Step 2: Add `render_sectors` function**

**File:** `state/src/ui/tui/render.rs`

Create a proper `ratatui::widgets::Table` with 5 columns:

| Column | Width | Content |
|--------|-------|---------|
| Sector | 25 | `sector_name` |
| Companies | 10 | `company_count` |
| GDP Share % | 12 | `pct_gdp_share` (formatted `{:.1}%`) |
| Employment | 12 | `total_employment` (formatted with `fmt_money`) |
| Avg Wage | 12 | `average_wage` (formatted with `fmt_money`) |

Use `Table::new()` with explicit `Constraint`s and a header row.

**Step 3: Update `render_tab_content`**

Add `Tab::Sectors => render_sectors(snap)` to the match.

**Step 4: Remove sector overview from Macro tab**

Remove lines 127–139 from `render_macro_finance()`.

**Step 5: Update tab navigation**

Update `app.rs` hotkey handling: `'5'` selects `Tab::Sectors`. The existing `'1'`–`'4'` logic should be extended to `'5'`.

### 2.2 Market Tab Layout & Zeroes

#### Current State

**Layout issues:**
- The market table uses fixed `Constraint::Length` for all columns (28+12+12+12+12 = 76 chars), which doesn't fill wider terminals.
- Only 20 commodities are shown at a time, but the table doesn't expand vertically.
- The "Surplus" column is labeled "Surplus" but should be "Balance".
- No ToT (turn-over-turn) % change column exists.

**Zero commodities:**
- All 140+ commodities are always shown (`Commodity::all()` at `snapshot.rs:246`).
- Commodities like `HeavyTanks`, `ElectronicComponents`, `Semiconductors` show 0.00 across all columns because they are not yet produced or traded.
- This is expected behavior for early-game/start-year scenarios, but it clutters the UI.

#### Fix

**File:** `state/src/ui/snapshot.rs`

1. **Add `tot_balance_change` field to `CommodityRow`:**

```rust
pub struct CommodityRow {
    pub name: String,
    pub vwap: f64,
    pub last_trade: f64,
    pub base_price: f64,
    pub net_surplus: f64,
    pub tot_balance_change: f64,  // NEW: % change of net_surplus from last turn
}
```

2. **Add `active` field to `CommodityRow`:**

```rust
pub active: bool,  // true if vwap > 0 OR last_trade > 0 OR net_surplus != 0
```

3. **Populate `tot_balance_change` from `market_history`:**

Compute the difference between current `net_surplus` and the previous turn's `net_surplus` (stored in market history or a new field).

4. **Filter inactive commodities:**

In the snapshot builder, mark commodities as `active = vwap > 0.0 || last_trade > 0.0 || net_surplus.abs() > 0.01`. The TUI can then either:
- **Option A (recommended):** Only show active commodities by default, with a toggle key `[f]` to show all.
- **Option B:** Show all but sort active first, inactive last.

**File:** `state/src/ui/tui/render.rs`

5. **Fix market table layout:**

Change column constraints to use `Constraint::Min` for the last column and `Constraint::Percentage` for wider terminals:

```rust
Table::new(table_rows, [
    Constraint::Length(28),       // Commodity
    Constraint::Length(12),       // VWAP
    Constraint::Length(12),       // Last
    Constraint::Length(12),       // Base
    Constraint::Length(12),       // Balance (renamed from Surplus)
    Constraint::Min(10),          // ToT % (new column)
])
```

6. **Rename "Surplus" to "Balance":**

Update the header row.

7. **Add ToT % change column:**

Show `tot_balance_change` formatted as `▲ +2.34%` or `▼ -1.20%` using the existing `fmt_delta` helper.

8. **Increase visible rows:**

Change `take(20)` to `take(40)` or compute dynamically based on terminal height.

### 2.3 Why So Many Zeroes?

#### Audit Finding

The zeroes are **expected behavior** — commodities like `HeavyTanks`, `ElectronicComponents`, `Semiconductors`, `Silicon`, `RareEarthElements` are not produced at game start because:

1. The production methods that output them require high-tech inputs that don't exist yet.
2. The generator only spawns one company per sector per region, and that company gets the highest-year method available — which may not produce the commodity in question.
3. No trades means no VWAP, no last_trade, and zero net_surplus.

#### Recommendation

**Filter inactive commodities** (Option A above). Add a `[f]` toggle key in the Market tab to show/hide inactive commodities. This keeps the UI clean while preserving the ability to inspect the full commodity list when needed.

---

## PART 3: Macroeconomic Blockage (I=0, G=0)

### 3.1 Generator Scaling Audit

#### Root Cause: Cascading Supply-Chain Failure

The generator's `best_registry_method()` function (`corporate.rs:796`) selects **one** production method per building — the one with the highest year ≤ `start_year`. This causes a cascading supply-chain failure:

**For a 1975 start year:**

| Sector | Selected Method | Outputs | Inputs | Problem |
|--------|----------------|---------|--------|---------|
| Mining | "CNC Mining" (1970) | HardCoal only | Energy, Fuels, **ElectronicComponents** | No Iron, Copper, Oil, NaturalGas produced. Can't even produce HardCoal because ElectronicComponents unavailable. |
| Energy | "Combined Cycle Plant" (1975) | Energy, Heat | **NaturalGas**, **ElectronicComponents** | No NaturalGas (no gas extraction method selected). No ElectronicComponents. Can't produce Energy. |
| HeavyIndustry (50%) | "CNC Manufacturing" (1970) | IndustrialMachinery | Energy, **Steel**, **ElectronicComponents** | No Steel (no Iron). No ElectronicComponents. Can't produce machinery. |
| HeavyIndustry (50%) | "Mini-Mill Production" (1975) | Steel | Energy, **ElectronicComponents** | No ElectronicComponents. Can't produce Steel. |

**The cascade:**
1. Mining picks "CNC Mining" → needs ElectronicComponents → none produced → **mining stops**
2. No Iron/Copper/Oil → HeavyIndustry can't make Steel or ElectronicComponents → **industry stops**
3. No NaturalGas → Energy can't run Combined Cycle → **energy stops**
4. No Energy → Everything stops → **GDP = consumption only (depleting seed inventory)**

#### Why ElectronicComponents Is Never Produced

ElectronicComponents are produced by HeavyIndustry's "Electronic Components Assembly" method (1920), which requires:
- **Copper** (mined by "Copper Ore Mining", 1880 — never selected because "CNC Mining" has higher year)
- **Tin** (no dedicated mining method exists in the registry)
- Energy (can't be produced — see above)
- IndustrialMachinery (can't be produced — see above)

#### Fix: Multi-Method Mining & Era-Appropriate Fallbacks

**File:** `state/src/engine/generator/corporate.rs`

**Step 1: Spawn multiple mining companies per region, respecting geology**

Instead of one Mining company per region, spawn **one mining company per available mineral deposit** in that region. The generator MUST query `country.geological_formations` and only spawn a mine if a matching `ResourceDeposit` exists in the region.

```rust
fn seed_mining_companies(
    country: &Country,
    region: &Region,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    // Query geological formations for this region's deposits.
    // Only spawn a mine if the deposit actually exists in the geology.
    let region_deposits: Vec<&ResourceDeposit> = country
        .geological_formations
        .iter()
        .filter(|f| f.overlapping_regions.contains(&region.id))
        .flat_map(|f| f.resource_deposits.values())
        .collect();

    let mut result = Vec::new();
    for deposit in &region_deposits {
        // Map deposit commodity → production method name
        let method_name = match deposit.commodity {
            Commodity::Iron => "Iron Ore Mining",
            Commodity::Copper => "Copper Ore Mining",
            Commodity::Oil => "Oil Drilling",
            Commodity::NaturalGas => "Natural Gas Extraction",
            Commodity::Tin => "Tin Ore Mining",
            Commodity::HardCoal => "Manual Mining", // or era-appropriate
            _ => continue,
        };
        if let Some(method) = find_method_by_name(registries, "mining", method_name, start_year) {
            let (mut company, mut building) = create_seed_company_with_method(
                Sector::Mining, region, /* small capacity */ 200,
                start_year, method, idgen, rng,
            );
            // Link the building to the deposit (Phase 21A pattern)
            building.deposit_id = Some(deposit.id.clone());
            result.push((company, building));
        }
    }
    result
}
```

**Critical:** No hardcoded "spawn Iron everywhere" loops. If a region has no Copper deposit, no Copper mine is spawned there. Countries without specific mineral deposits will suffer shortages — this is intended realism.

**Step 2: Ensure era-appropriate energy methods**

For 1975 start, ensure at least some energy plants use methods that don't require ElectronicComponents:
- "Steam Turbine Plant" (1900) — needs only HardCoal + Water
- "Internal Combustion Plant" (1910) — needs Fuels + MechanicalComponents

The generator should spawn **both** advanced and fallback energy plants.

**Step 3: Ensure MechanicalComponents production**

MechanicalComponents are produced by HeavyIndustry's "Precision Machining" method (1910), which needs Steel + Energy + IndustrialMachinery. This is another chicken-and-egg problem. The generator should seed some MechanicalComponents in the initial market inventory.

**Step 4: Seed initial inputs in building inventory (NOT market.json)**

Advanced factories/mines get their starting inputs seeded directly into `building.inventory`, with the cost deducted from `company.liquid_capital`. No goods are spawned into `market.json` — every commodity must be paid for and exist in a building.

```rust
fn seed_paid_inventory(
    method: &ActiveProductionMethod,
    building_capacity: u32,
    company: &mut Company,
    market_prices: &HashMap<Commodity, f64>,
) -> BTreeMap<Commodity, f64> {
    let production_scale = building_capacity as f64 / 1000.0;
    let mut inventory = BTreeMap::new();
    let mut total_cost = 0.0;

    for (&commodity, &qty_per_1k) in &method.inputs {
        if commodity.is_fixed_asset() {
            continue;
        }
        let seed_qty = qty_per_1k * production_scale;
        if seed_qty > 0.0 {
            let unit_cost = market_prices.get(&commodity).copied().unwrap_or(100.0);
            total_cost += seed_qty * unit_cost;
            inventory.insert(commodity, seed_qty);
        }
    }

    // Deduct the cost from the company's liquid capital (double-entry)
    company.liquid_capital -= total_cost;
    company.available_cash -= total_cost;

    inventory
}
```

This ensures every commodity in the economy is owned by a building and was paid for by a company. No magical goods injection.

**Step 5: Add Tin mining method**

The registry has no Tin mining method. Add one to `mining_methods()`:

```rust
m.insert(MethodSlot::Production, "Tin Ore Mining".into(),
    pm(1880, None, 0.05, 0.20, 0.75, 1.0,
       &[(Commodity::Fuels, 3.0), (Commodity::Food, 5.0)],
       &[(Commodity::Tin, 6.0)]));
```

### 3.2 International Trade Audit (ON HOLD)

#### Current State

**File:** `state/src/international/trade.rs`

The global trade system (`balance_global_trade()`) works as follows:

1. **Collect phase:** For each country, compute `export_weight = GDP × competitiveness` and `import_weight = GDP / competitiveness`.
2. **Allocate phase:** Distribute global supply/demand proportionally to weights.
3. **Apply phase:** Adjust `country.budget.liquid_reserves` by `trade_balance = exports - imports`.

#### Critical Limitation

The system operates on **aggregate monetary values**, NOT on specific commodities. It does NOT route Iron from country A to country B. It just adjusts a country's cash reserves based on its share of global trade.

This means:
- A country lacking Iron **cannot import Iron** through the global trade system.
- The system cannot act as a safety valve for raw material starvation.
- The B2B order book (`OrderBook` with bids/asks) is domestic-only — it matches buyers and sellers within the same country.
- There is no mechanism for cross-country B2B trade of specific commodities.

#### Decision: International Trade ON HOLD

Per user directive, cross-country B2B matching is **NOT** being implemented in Phase 27. The domestic economy must balance itself via proper generation first. Countries that lack specific mineral deposits will suffer shortages until Phase 28/29 introduces international trade.

#### Future Plan (Phase 28/29, not implemented now)

**Phase 1 (minimal):** After domestic B2B matching, collect unfilled bids per commodity. For each unfilled bid, check if any other country has unfilled asks for the same commodity. Match them with a freight cost premium and tariff applied. Settle the trades through the existing `settle_trades_with_tariffs` mechanism.

**Phase 2 (full):** Integrate with the maritime/port system. Cross-country trade requires port throughput capacity. Implement shipping contracts and trade finance.

---

## Implementation Steps

### Step 1: Fix Calendar Bug (small, critical)
- **File:** `state/src/engine/turn.rs`
- Change `year += 1` to `if turn > 0 && turn % 24 == 0 { year += 1 }`
- Sync `state.calendar` after the increment
- **Test:** 24-turn golden audit should show year 1976 at turn 24

### Step 2: Add Sectors Tab (medium, UI)
- **Files:** `state/src/ui/tui/tabs.rs`, `state/src/ui/tui/render.rs`, `state/src/ui/tui/app.rs`
- Add `Tab::Sectors` variant
- Add `render_sectors()` function with proper `Table` widget
- Remove sector text from Macro tab
- Update hotkey handling for `'5'`

### Step 3: Fix Market Tab Layout (medium, UI)
- **Files:** `state/src/ui/snapshot.rs`, `state/src/ui/tui/render.rs`
- Add `tot_balance_change` and `active` fields to `CommodityRow`
- Rename "Surplus" to "Balance"
- Add ToT % change column
- Fix column constraints to fill terminal width
- Add `[f]` toggle to filter inactive commodities
- Increase visible rows from 20 to 40

### Step 4: Fix Generator Supply Chain (large, critical)
- **Files:** `state/src/engine/generator/corporate.rs`, `state/src/registries/production_methods_data.rs`
- Spawn mining companies based on geological deposits (Phase 21 respect)
- Add Tin mining method to registry
- Ensure era-appropriate fallback energy methods (not just highest-year)
- Seed initial inputs in `building.inventory` with cost deducted from `company.liquid_capital` (no magical market seeding)

### Step 5: Verify & Test
- `cargo build`
- `cargo test --lib`
- Run 24-turn golden audit
- Verify: GDP has nonzero C, I, and G components
- Verify: CPI is dynamic (not frozen)
- Verify: Year increments correctly (24 turns = 1 year, not 1 year per turn)
- Verify: Sector tab renders properly
- Verify: Market tab fills screen and shows Balance + ToT columns
- Verify: No goods spawned into market.json — all commodities exist in building inventories

---

## Risks & Considerations

1. **Calendar fix may break existing tests** that assume year increments every turn. The golden test telemetry may need adjustment.
2. **Multi-mining companies** increase the total entity count, which may slow down generation and turn processing. Consider keeping the additional mining companies small (200 worker capacity vs 1000 for main mines).
3. **Geology-respecting mining** means some countries may lack critical minerals (e.g., no Tin deposits → no ElectronicComponents → no advanced manufacturing). This is intended realism — shortages will persist until Phase 28/29 introduces international trade.
4. **Paid inventory seeding** reduces company starting cash, which may slow initial production. The seed quantities should be modest (1-2 turns of inputs) to avoid bankrupting companies on Turn 1.
5. **The `best_registry_method` function** should not be removed — it's correct for most sectors. The fix is specific to Mining (which needs multiple methods per region based on geology) and Energy (which needs fallback methods that don't require advanced inputs).
6. **International trade is ON HOLD.** Countries suffering raw material shortages will have depressed GDP until Phase 28/29. This is the intended design — domestic balance first.
