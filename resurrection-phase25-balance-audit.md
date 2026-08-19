# Phase 25 — The "Golden Year" Empirical Balance & Semantic Audit

**Date:** 2026-08-13
**Scope:** Pre-Global-Trade balancing audit of the domestic economy after Phase 24F
**Method:** Fresh 24-turn (one full year) simulation + static codebase audit
**Verdict:** **The economy does NOT survive on its own.** Multiple critical failures identified.

---

## Executive Summary

A fresh 6-country world was generated (`StartYear::Y1975`) and run for **24 turns** with full persistence, exercising the Phase 24F CSV telemetry exporter and a new extended treasury snapshot harness (`tests/golden_year_24_turns.rs`). The artefacts live in `state/test_simulation_data_phase25/`.

The headline result is unambiguous and severe:

| Metric (Iliria, representative) | Turn 0 | Turn 1 | Turn 12 | Turn 24 | Verdict |
|---|---|---|---|---|---|
| Official GDP | 0.0 | 0.0 | 0.0 | 0.0 | **Never produced** |
| Shadow GDP | 0.0 | 0.0 | 0.0 | 0.0 | **Never produced** |
| CPI / PPI | 0.0 / 0.0 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 | **Frozen — no price discovery** |
| Unemployment | 8.70% | 70.37% | 99.9999% | 100.00% | **Total labor collapse** |
| True Labor Utilization | 91.30% | 29.63% | 0.0001% | 0.00% | **Workforce extinct** |
| Average Wage | 15,233 | 8,948 | 95.78 | 95.78 | **Collapsed to a hard floor** |
| Citizen Savings | 18.1 B | **0.0** | 0.0 | 0.0 | **Wiped out in 1 turn** |
| Liquid Reserves | 11.2 B | 274 M | 260 M | 476 M | **Treasury survives but starved** |
| M3 | 0.0 | 86.7 B | 199.2 B | 364.8 B | **Exploding from zero** |
| Population | 8.71 M | 8.82 M | 10.53 M | 12.61 M | **Growing despite 100% unemployment** |

**The economy is a corpse that keeps breathing through a money-creation ventilator.** GDP is structurally zero, the entire workforce is unemployed, citizen savings are wiped out on turn 1, yet M3 grows ~30% per turn and the population keeps expanding. This is the textbook signature of (a) a GDP accumulator that never fires, (b) a wage floor propping up consumption with phantom income, and (c) bank credit creation expanding M3 with no production backing.

The supply chain itself is **clean** (no orphans, no hard circular deadlocks, Turn-1 freeze risk is low). The tech tree has **severe late-game scaling gaps** in state, retail, religious, and specialty buildings. The financial-velocity audit found **four confirmed black holes** introduced or exposed by Phase 23/24. The `src/` tree has **three critical duplications** (treasury, banking, infrastructure) that should be resolved before Global Trade piles more cross-folder coupling on top.

**No codebase changes have been made.** This document is the blueprint awaiting explicit approval.

---

## PART 1 — The Empirical 24-Turn Test

### 1.1 Methodology

- **Test:** `tests/golden_year_24_turns.rs` (newly added for this audit).
- **World:** `generate_world` with `country_count: 6`, `StartYear::Y1975`, `Registries::native_only()`.
- **Run:** 24 consecutive `run_turn` calls with `persist: true`, so the Phase 24F exporter appends to `data/telemetry/<country>_macro.csv`.
- **Extended snapshot:** Per-country per-turn row written to `phase25_treasury_audit.csv` with treasury, banking, and labor-utilization columns the auto-CSV lacks.
- **Market snapshots:** `market_surplus_turn0.csv` and `market_surplus_turn24.csv`.
- **Run time:** ~1050 s (17 min) for 24 turns × 6 countries on debug build.
- **Errors during run:** 0 (no `run_turn` returned `Err`).

The six countries are Oksytania, Persja, Dacja, Wenedia, Galia, Iliria. All six exhibit the same pathology, so Iliria is used as the running example; the per-country table at §1.7 confirms the pattern is global.

### 1.2 GDP and Shadow GDP

**Finding: Official GDP and Shadow GDP are exactly 0.0 for every country, every turn, including turn 0.**

Source: `test_simulation_data_phase25/telemetry/Iliria_macro.csv` (and the other 5 country CSVs), cross-checked against `phase25_treasury_audit.csv` columns `GDP` and `ShadowGDP`.

The Phase 24F exporter reads `country.macro_indicators.gdp_breakdown.official_gdp` (`src/io/telemetry_export.rs:117-118`). That field is populated by `crate::economy::telemetry::compute_gdp(&task.gdp_acc, prev_gdp)` at `src/engine/turn.rs:2974`. The `GdpAccumulator` (`src/economy/telemetry.rs:54-65`) is fed by expenditure-side hooks — notably `task.gdp_acc.consumption += b2c_revenue` at `src/engine/turn.rs:1636`, which runs **after** `settle_b2c_clearing()`.

The fact that GDP is 0.0 on turn 0 is expected (no turn has run). The fact that it stays 0.0 through turn 24 means **`GdpAccumulator` is never credited**. The two most likely root causes, both consistent with the labor data in §1.5:

1. **No B2C clearing revenue is being generated.** With 100% unemployment and citizen savings wiped to 0.0 on turn 1, consumers have no purchasing power, so `settle_b2c_clearing()` (`src/economy/trade/retail.rs:491-561`) settles nothing and the consumption hook never fires.
2. **No B2B investment/government-spending GDP hooks fire either**, otherwise we'd see at least the treasury's nominal budget (~49 B for Iliria) show up in GDP.

This is the single most important bug in the engine right now: **the expenditure-side GDP identity is broken because at least one of {consumption, investment, government spending, net exports} is never accumulated, and with citizens broke the consumption term is structurally zero.**

### 1.3 CPI / PPI — Frozen Indices

**Finding: CPI and PPI jump from 0.0 (turn 0, pre-turn) to exactly 100.0 (turn 1) and remain at 100.000000 through turn 24. No inflation, no deflation, no price discovery whatsoever.**

Source: `Iliria_macro.csv` columns `CPI_Index`, `PPI_Index`; identical in all 6 countries.

`compute_inflation` (`src/economy/telemetry.rs`, called at `src/engine/turn.rs:2977`) updates the indices from market price signals. The market surplus snapshot at turn 24 (`market_surplus_turn24.csv`) shows real imbalances (e.g. `Fibers -217900`, `Fuels -7158`, `Steel +267305`, `Clothing +522961`), so the market **is** producing and ordering — but the price index never moves. This points to either:

- The CPI/PPI reference basket is empty or not wired to the market clearing prices, or
- `compute_inflation` short-circuits when the previous index is the sentinel 100.0 and no consumption occurred (tie-in with §1.2: no B2C ⇒ no consumer price signal).

A frozen index at exactly 100.0 across 24 turns with wildly imbalanced markets is itself a bug, independent of the GDP issue.

### 1.4 Treasury (`liquid_reserves`) — Did it bankrupt?

**Finding: The Treasury does NOT bankrupt, but it is structurally starved and propped up by a money-creation ventilator.**

Iliria reserves: 11.2 B (turn 0) → 274 M (turn 1) → 260 M (turn 12) → 476 M (turn 24). The turn-1 cliff is the ministry allocation sweep (`src/politics/ministries.rs:527-548`) draining cash to ministries that then pay B2B/wages. After that, reserves hover in the 250–500 M band and even **grow** slightly by turn 24.

This is suspicious, not healthy:

- Nominal budget for Iliria is **49.4 B** (`NominalBudget` column, constant across all turns). The treasury is running on ~0.5% of its notional budget.
- Ministries enforce hard cash caps (`src/politics/ministries.rs:619-629`): if `liquid_reserves < amount` the spend is skipped. So the treasury cannot go negative — but it also cannot fund anything meaningful.
- Yet M3 grows from 0 → 364.8 B for Iliria alone, and bank deposits grow from 9.1 B → 139.6 B. **The money keeping the system alive is coming from bank credit creation** (`src/state/banking.rs:821-826`, `issue_loan` creates deposits without moving reserves), not from production or taxation.

**Verdict on the user's question "Did the Treasury bankrupt?":** No, but only because (a) ministry spending is hard-capped at whatever cash exists, and (b) the banking system is injecting M3 that partially recirculates into treasury via taxation. This is not solvency — it is a ventilator.

### 1.5 Unemployment & OHS Casualties

**Finding: Unemployment explodes from ~9% to 100% within 7 turns. True labor utilization hits 0.00%. The average wage collapses to a hard floor of 95.7824 and sticks there.**

Iliria unemployment trajectory: 8.70% → 70.37% → 90.51% → 96.97% → 99.03% → 99.90% → 99.97% → 99.99% → 100.00% (turn 7 onward, never recovers).

The wage floor at exactly **95.7824** (identical across all 6 countries, all turns from turn 7 on) is a smoking gun. A real labor market would show wage dispersion. A constant pinned to 4 significant figures across 6 independent countries means there is a **hardcoded minimum wage / subsistence floor** that fires when the market wage would otherwise go to zero. Combined with 100% unemployment, this means:

- Citizens are not being paid wages (no employment).
- Yet population keeps growing (8.71 M → 12.61 M for Iliria).
- And M3 keeps growing (bank credit).

The most likely mechanism: a subsistence/minimum-wage transfer credits citizens with ~95.78 per capita from a non-wage source (treasury transfer or money creation), which is then immediately taxed or spent, keeping the population alive but producing nothing. This is the "ventilator" from §1.4 made visible at the household level.

**OHS casualties:** The `Corruption_Index` and `Structural_Defects_Mean` columns are 0.0 for all countries all turns (the latter is a known placeholder per `src/io/telemetry_export.rs:105`). The financial-velocity audit (§4.3) found that `COMPENSATION_PER_CASUALTY = 50_000.0` is defined in `src/construction/fraud.rs:64` but **never actually paid**, so OHS is not a cash drain in this run — but it is also not a functioning system. Commuting costs were not the primary driver of unemployment here; the driver is the broader production/GDP collapse.

### 1.6 Money Supply M0 / M3

**Finding: M0 and M3 are 0.0 on turn 0, then explode. M3 for Iliria goes 0 → 86.7 B → 199.2 B → 364.8 B. Aggregate M3 across 6 countries reaches 2.25 trillion by turn 24.**

The turn-0 zero is itself a bug: `compute_money_supply` (`src/economy/telemetry.rs`, called at `src/engine/turn.rs:2983`) returns 0 before the first turn runs, even though `phase25_treasury_audit.csv` shows Iliria already has 11.2 B liquid reserves, 229.4 B private capital, and 18.1 B citizen savings at turn 0. **M0/M3 are not initialized from the treasury's actual cash positions.** They are computed fresh on turn 1 and apparently include bank deposits (which jump from 9.1 B to 14.9 B for Iliria on turn 1) plus created credit.

The 30%-per-turn compound growth in M3 with zero GDP growth is the classic signature of **unbacked credit expansion** — the banking system (`src/state/banking.rs:821-826`) is issuing loans that create deposits, but the real economy is not absorbing them as production, so they pile up as inert M3. This is not hyperinflation (CPI is frozen at 100) because the money never reaches consumers as wages — it is trapped in the bank/corporate loop.

### 1.7 Per-Country Final State (Turn 24)

All six countries exhibit identical pathology — this is a systemic bug, not a country-specific edge case.

| Country | GDP | CPI | PPI | Unemp % | Wage | Reserves | M3 | Pop (M) | Banks |
|---|---|---|---|---|---|---|---|---|---|
| Oksytania | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 122 M | 101.1 B | 8.45 | 1 |
| Persja | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 781 M | 660.2 B | 25.54 | 1 |
| Dacja | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 550 M | 358.2 B | 17.57 | 1 |
| Wenedia | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 279 M | 186.1 B | 10.43 | 1 |
| Galia | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 354 M | 579.4 B | 39.15 | 1 |
| Iliria | 0.0 | 100.0 | 100.0 | 100.0 | 95.78 | 476 M | 364.8 B | 12.61 | 1 |

**Critical observation:** Every country has **exactly 1 bank** at turn 24. The world generator creates banks (the 100-turn golden master asserts `initial_bank_count > 0`), but no new banks are being chartered and none are dying. A 1-bank monopoly with 2.25 trillion in M3 is not a functioning banking system — it is a single firehose of credit.

### 1.8 Market Imbalances at Turn 24

From `market_surplus_turn24.csv` (positive = surplus/glut, negative = deficit/starvation):

**Gluts (top):** Clothing +522,961 · Energy +435,914 · Steel +267,305 · ConstructionServices +158,439 · Cereal +113,601 · Heat +108,425

**Starvation (bottom):** Fibers −217,900 · ElectronicComponents −57,488 · NaturalGas −40,659 · Water −34,796 · Cement −31,687 · Seeds −19,474 · Fertilizers −16,228 · ConstructionMachinery −15,843 · Fuels −7,158

Interpretation:
- **Clothing, Steel, Energy, Cereal are massively overproduced** relative to demand — consistent with citizens having 0 savings: factories produce but nobody buys, so surplus piles up.
- **Fibers is critically starved** (−217,900). Fibers are produced by `Textile Mill` and `Synthetic Fiber Production` (light industry) and consumed by every textile method. A fiber shortage cascades into Clothing production — yet Clothing is in glut. This suggests Clothing is being produced from inventory or from a source that bypasses the fiber market, OR the surplus/deficit accounting is decoupled from actual production.
- **Seeds, Fertilizers, Cement, ConstructionMachinery are starved** — these are the inputs to agriculture and construction. With 100% unemployment, nobody is working the farms or building sites, so input demand should be low — yet these show deficits, indicating the B2B order pipeline is still firing orders that cannot be filled.
- **Water is starved** (−34,796) despite being a "free resource" — the water utility (`Water Utility` energy method) is not keeping up with demand.

The market is alive but deeply distorted: it is producing finished goods into a vacuum while starving for inputs. This is the supply-side mirror of the demand-side collapse (no wages ⇒ no consumption ⇒ gluts ⇒ no input orders ⇒ input starvation ⇒ no production ⇒ no wages).

---

## PART 2 — Supply Chain & Orphaned Commodities

### 2.1 Commodity Enum

**140 total variants** in `src/registries/enums.rs`. **19 are deprecated** (Phase 20, filtered by `is_active()`):
`MobileArtillery, AntiAircraftArtillery, InfantryFightingVehicles, MilitaryTrucks, Frigates, Cruisers, AircraftCarriers, Destroyers, Battleships, NavalVessels, Pistols, Gunpowder, Airplanes, PassengerShips, CargoShips, RollingStock, InsuranceServices, MineralResources, MarketResearch`.

**121 active commodities.** The deprecation discipline is good — `tests/supply_chain_integrity_test.rs:323-333` confirms `is_active()` filters them.

### 2.2 Orphaned Commodities

**Orphaned-produced (produced but never consumed): NONE.**
**Orphaned-consumed (required as input but never produced): NONE.**

This is confirmed by the existing passing tests `no_orphan_inputs` (`tests/supply_chain_integrity_test.rs:52-68`) and `no_orphan_b2c_demand` (`:73-93`). The supply chain is **closed** — every produced good has a B2B or B2C consumer, every consumed good has a producer. Water is the only "free resource" (extracted without a production method).

### 2.3 Circular Deadlocks

Five potential loops were identified; **all are mitigated by early-game bootstrap methods** that do not require the advanced input:

| Loop | A requires B | B requires A | Bootstrap breaker |
|---|---|---|---|
| Machinery ↔ MechanicalComponents | IndustrialMachinery ← MechanicalComponents | MechanicalComponents ← IndustrialMachinery | MaintenanceWorkshops produce MaintenanceServices from generic raw materials; early MC methods don't need IndustrialMachinery |
| Energy ↔ ElectronicComponents | Late energy methods need EC | EC methods need Energy | Early energy (Coal-Fired Boilers, Hydroelectric) needs no EC |
| Steel ↔ IndustrialMachinery | Late steel needs IndustrialMachinery | IndustrialMachinery needs Steel | Early steel (Bessemer, Open-Hearth) needs no IndustrialMachinery |
| Chemicals ↔ Catalysts | Catalysts need Chemicals | Late chemicals need Catalysts | Basic Chemical Production needs no Catalysts |
| SodaAsh ↔ Ammonia | SodaAsh needs Ammonia (Solvay) | Ammonia needs Catalysts (Haber-Bosch) → Chemicals | Linear chain, not circular; bootstraps via Basic Chemical Production |

**Verdict: No hard circular deadlocks.** The tech tree is correctly tiered so that 1880-era methods bootstrap without 20th-century inputs.

### 2.4 Turn-1 Freeze Risk

**LOW.** The 1880-era bootstrap methods have minimal inputs:
- Mining `Manual Mining`: Fuels + Food
- Agriculture `Manual Farming`: Seeds + Food
- Heavy Industry `Bessemer Converters`: Iron + Fuels
- Energy `Coal-Fired Boilers`: HardCoal + Water
- Light Industry `Handloom Weaving`: Fibers + Food
- Construction `Manual Construction`: Food + Timber

**Critical bootstrap commodities that MUST be in initial inventories:** Food, Fuels, Seeds, Iron, HardCoal, Fibers, Timber. If any of these is missing from the world generator's initial inventory seed, the corresponding chain freezes on Turn 1. This should be asserted in a test (currently it is not — see §2.5).

### 2.5 Supply-Chain Recommendations (for the blueprint)

1. **Add a Turn-1 bootstrap inventory test** asserting that every country starts with non-zero Food, Fuels, Seeds, Iron, HardCoal, Fibers, Timber. The 24-turn run's market surplus at turn 0 was **empty** (`market_surplus_turn0.csv` has only the header), which means the market had no orders to clear on turn 0 — this is suspicious and may indicate the market is not seeded with initial supply/demand, only with prices.
2. **Investigate the Clothing-glut / Fibers-starvation paradox** (§1.8). Either Clothing is being produced without consuming Fibers (a recipe bug), or the surplus accounting is double-counting inventory liquidation.
3. The 19 deprecated commodities should eventually be removed from the enum (they are filtered but still occupy variant slots and require `#[allow(deprecated)]` at use sites).

---

## PART 3 — Tech Tree & Production Method Slots

The 3-slot system (Organization / Production / Automation) is defined in `src/registries/production_methods.rs:52-90`. Methods are spread across `production_methods.rs` (state, retail, university, healthcare, education, industrial-specialty, OSP buildings) and `production_methods_data.rs` (the 13 economic sectors).

### 3.1 Economic Sectors — Healthy

**All 13 economic sectors have complete 3-slot coverage.** This is the good news.

| Sector | Total | Prod | Auto | Org | Status |
|---|---|---|---|---|---|
| Heavy Industry | 52 | 41 | 6 | 6 | Excellent |
| Mining | 30 | 24 | 6 | 5 | Excellent |
| Agriculture | 22 | 16 | 6 | 4 | Good |
| Light Industry | 17 | 11 | 5 | 5 | Good |
| Armaments | 16 | 12 | 4 | 4 | Good |
| Energy | 16 | 11 | 5 | 3 | Good |
| Transport & Logistics | 16 | 11 | 5 | 4 | Good |
| Public Services | 16 | 8 | 4 | 5 | Good |
| Maintenance Workshops | 15 | 4 | 6 | 5 | Good |
| Media & Entertainment | 15 | 6 | 5 | 4 | Good |
| Medical Services | 15 | 6 | 5 | 4 | Good |
| Educational Services | 14 | 5 | 5 | 3 | Good |
| Construction | 14 | 6 | 5 | 4 | Good |

### 3.2 State Buildings — CRITICAL

**All 12 state buildings have ZERO Automation methods and ZERO Organization methods.** They can only upgrade via the Production slot. This is the most severe tech-tree gap.

| Building | Total | Prod | Auto | Org |
|---|---|---|---|---|
| Baza Wojskowa (Military Base) | 3 | 3 | 0 | 0 |
| Komisariat (Police) | 4 | 4 | 0 | 0 |
| Sąd (Courthouse) | 3 | 3 | 0 | 0 |
| Siedziba Służb (Intel HQ) | 3 | 3 | 0 | 0 |
| Więzienie (Prison) | 4 | 4 | 0 | 0 |
| Straż Pożarna (Fire) | 3 | 3 | 0 | 0 |
| Schron Przeciwpowodziowy (Flood Shelter) | 2 | 2 | 0 | 0 |
| Straż Graniczna (Border Guard) | 3 | 3 | 0 | 0 |
| Urząd Celny (Customs) | 3 | 3 | 0 | 0 |
| Sanepid (Sanitary) | 2 | 2 | 0 | 0 |
| Inspektorat Nadzoru Budowlanego | 2 | 2 | 0 | 0 |
| **Inspektorat Ochrony Środowiska** | **1** | **1** | **0** | **0** |

`Inspektorat Ochrony Środowiska` has **1 method total — it cannot upgrade at all.**

**Late-game impact:** State security/justice capacity scales ~2× (via Production-slot efficiency only), while private heavy industry scales ~5.5× (CNC Mining) and agriculture ~7× (Precision Farming). The state will be structurally outclassed by the private sector in the late game — police cannot adopt digital surveillance, courts cannot adopt digital case management, military bases cannot automate logistics. This is a gameplay balance cliff.

### 3.3 Retail — CRITICAL

**5 of 6 retail buildings have exactly 1 method (cannot upgrade). Only Hurtownia (Wholesaler) has an Automation method. NO retail building has Organization methods.**

| Building | Total | Prod | Auto | Org |
|---|---|---|---|---|
| Targ (Marketplace) | 1 | 1 | 0 | 0 |
| Hurtownia (Wholesaler) | 2 | 1 | 1 | 0 |
| Sklep Detaliczny (Retail Store) | 1 | 1 | 0 | 0 |
| Supermarket | 1 | 1 | 0 | 0 |
| Dom Towarowy (Department Store) | 1 | 1 | 0 | 0 |
| Centrum Handlowe (Shopping Center) | 1 | 1 | 0 | 0 |

**Late-game impact:** A Shopping Center (late-game building) has the **same** 1.0× efficiency as an 1850 Marketplace. There is no self-checkout, no inventory automation, no e-commerce integration. B2C market capacity cannot scale with population growth. Combined with the §1.2 finding that B2C consumption is the broken GDP link, this is doubly important — even if citizens had money, the retail sector could not absorb their demand at scale.

### 3.4 Industrial Specialty & Religious — CRITICAL

**7 of 8 specialty buildings have 1 method. ALL 8 lack Automation and Organization.** Religious buildings (monasteries, temples) are completely static — no upgrade path whatsoever.

| Building | Total | Auto | Org |
|---|---|---|---|
| Zakład Solvaya (Soda Ash) | 1 | 0 | 0 |
| Młyn Nasienny (Seed Mill) | 1 | 0 | 0 |
| Nadleśnictwo (Forest District) | 2 | 0 | 0 |
| monastery_wine_production | 1 | 0 | 0 |
| monastery_scriptorium | 1 | 0 | 0 |
| monastery_workshop | 1 | 0 | 0 |
| temple_artisan_workshop | 1 | 0 | 0 |
| monastery_herbal_garden | 1 | 0 | 0 |

### 3.5 Education & Healthcare — LIMITED

**No education or healthcare building has Organization methods.** Some have Automation (University, Polytechnic, Hospital, High School). Clinic, Research Hospital, and Primary School have only 1 method.

### 3.6 Summary of Late-Game Scaling Gaps

- **29 buildings** completely lack Automation methods.
- **35 buildings** completely lack Organization methods.
- **16 buildings** have only 1 method total (cannot upgrade at all).
- **5 buildings** have only 2 methods.

The private industrial core (mining, agriculture, heavy industry, light industry, armaments, energy, transport, construction, maintenance) is well-designed and scales beautifully. **Everything outside that core — the state apparatus, retail, religion, specialty industry, and most of education/healthcare — is frozen in the 19th century.** This will make the late game deeply unbalanced: a 2050 economy with 1880 police, 1880 shops, and 1880 monasteries.

### 3.7 Tech-Tree Recommendations (for the blueprint)

1. **State buildings (12):** Add Automation methods (Digital Surveillance, AI Analytics, Automated Logistics, Drone Reconnaissance, Digital Case Management, E-Government) and Organization methods (Civil Service Reform, Lean Management, Digital Bureaucracy). Target 3-5× scaling.
2. **Retail (6):** Add Automation (Self-Checkout, Inventory Automation, E-Commerce) and Organization (Supply-Chain Management, Just-in-Time). Target 2-3× scaling. **This is the highest-priority fix for the GDP bug** — retail scaling is the B2C consumption bottleneck.
3. **Religious & specialty (8):** Add at least one Automation and one Organization method each, even if modest (Digital Archives, Modern Administration).
4. **Education/Healthcare:** Add Organization methods (Curriculum Reform, Hospital Administration, Integrated Care).
5. **Inspektorat Ochrony Środowiska:** Add at least 2 more methods — it is the only building in the game with a single method.

---

## PART 4 — Financial Velocity & Black Holes (Final Sweep)

### 4.1 Velocity of Money — Citizen Savings

**Finding: Citizens DO have spending mechanisms (B2C retail, services, taxation), so savings are not structurally hoarded. BUT in the 24-turn run, citizen savings are wiped to 0.0 on turn 1 and never recover, so the velocity question is moot — there is no money to hoard.**

The recirculation plumbing exists and is correct:
- **Spending:** `settle_b2c_clearing()` (`src/economy/trade/retail.rs:491-561`) routes citizen savings to retail companies; `settle_b2c_purchase()` (`src/economy/trade/transfer_settler.rs:241-297`) debits savings and credits company brokerage with bank balance-sheet sync; B2C services (`src/economy/trade/b2c_services.rs:303,477,484`) debit savings for education/health/transport.
- **Taxation:** PIT and VAT deducted from savings (`src/state/tax.rs:1283,1355`).
- **Accumulation:** Wages credited to savings (`src/economy/labor/labor_market.rs:381,384,412,415`; commuter wages at `src/engine/turn.rs:1432,1436`).

The problem is not hoarding — it is that **the wage credit pipeline is broken** (100% unemployment ⇒ no wages ⇒ savings go to 0 on turn 1 and stay there). The recirculation loop is intact but empty. Fixing §1.2/§1.5 (production and employment) will automatically restore velocity.

**One residual hoarding risk to monitor after the fix:** if the wage floor of 95.78 (§1.5) is a treasury transfer that credits savings but is then immediately consumed by subsistence, citizens will live hand-to-mouth with no buffer for discretionary B2C (luxury, furniture, cars). The consumption basket will collapse to subsistence-only. This should be re-audited once employment is fixed.

### 4.2 Ministry B2B/Wage Payments — Conservative

**Finding: Ministries CANNOT print money. All spending is hard-capped by `liquid_reserves`.** This is correct and was verified in `src/politics/ministries.rs`:
- `allocate_cash_to_ministries()` (`:527-548`) pro-rates allocations if treasury insufficient (`ratio = (available / promised).min(1.0)`).
- Every spending action (`:663, 700, 715, 727, 751`) checks `liquid_reserves >= amount` before debiting.
- If insufficient, spending is pro-rated or skipped (`:622-629`, `return; // Treasury empty`).

**Verdict: No black hole here.** Money is conserved — transactions are skipped, not created from nothing. This is exactly the behavior we want from the Phase 24 strict-treasury work.

### 4.3 Confirmed Black Holes

Four black holes were identified. Two are in Phase 23/24 systems (freight, network maintenance), one is an unimplemented OHS payment, and one is a leakage fallback in building maintenance.

#### Black Hole #1 — Network Infrastructure Maintenance (Phase 23B) **CRITICAL**

**Location:** `src/economy/logistics/transport_networks.rs:203-230`, `process_network_maintenance()`.

**Mechanism:** Treasury is debited `*treasury_cash -= repair_cost;` (lines 217, 224) for road/rail/network upkeep, but **no company is credited**. The money is destroyed. The physical link condition is restored, but the financial side is a pure sink.

**Severity:** Critical. This is a Phase 23 introduction. Every turn, every country pays network maintenance that vanishes. With 6 countries and growing networks, this is a steady drain on `liquid_reserves` that explains part of the treasury starvation in §1.4.

**Fix direction:** Credit a Construction-sector or Maintenance-sector company for the repair work (double-entry mirror of `process_maintenance_spending` in `src/economy/production/maintenance.rs:90-140`, which does it correctly for buildings).

#### Black Hole #2 — Cold-Start Freight Fallback (Phase 23A) **HIGH**

**Location:** `src/economy/logistics/logistics.rs:526-560`.

**Mechanism:** When no `FreightCapacity` is available (cold-start or undersupplied transport sector), the buyer pays a "self-transport penalty" to `TransferRecipient::Treasury` (line 548). The comment at line 541 says "the cost dissipates into the economy — fuel, wear-and-tear, etc." — but it does not dissipate, it is **destroyed**. The Treasury receives it but no transport company is credited, so the freight payment is a sink.

**Severity:** High. In early game (before transport companies scale) or in regions without transport capacity, every B2B shipment that uses the fallback destroys money. Given the §1.8 finding that FreightCapacity is barely produced (+1,589 surplus), this fallback may be firing frequently.

**Fix direction:** Route the self-transport penalty to a national logistics-services notional account or to the nearest transport company, OR explicitly model it as a fuel purchase (debit buyer, credit Energy-sector fuel supplier).

#### Black Hole #3 — OHS Casualty Compensation (Phase 22B) **MEDIUM (unimplemented)**

**Location:** `src/construction/fraud.rs:64` defines `COMPENSATION_PER_CASUALTY = 50_000.0`, but **no code ever pays it.**

**Mechanism:** `check_workplace_accident()` (`:206-220`) determines casualties; `apply_casualties_to_labor()` (`src/economy/telemetry.rs:370-442`) reduces population/FTE. The engine processes the human-capital destruction but never transfers the 50,000 compensation from any fund to the victim's household. The constant is dead code.

**Severity:** Medium. This is not currently destroying money (nothing is paid), but it is a **missing liability**: workers die and are disabled with no financial compensation, which means (a) the OHS system has no financial teeth for employers, and (b) once compensation is implemented, it must be funded from a real source (employer payroll, treasury, or insurance) or it becomes a new money-creation site.

**Fix direction:** Implement the payment: debit the employer company (or a workers'-comp insurance fund), credit the victim's household savings. Add an invariant test that total compensation paid equals `casualties × 50,000`.

#### Black Hole #4 — Building Maintenance Leakage Fallback **LOW**

**Location:** `src/economy/production/maintenance.rs:88-89` (comment) and `:90-140` (`process_maintenance_spending`).

**Mechanism:** Building maintenance correctly debits the owner (`debit_company_by_id`, `:134`) and credits a Construction-sector company (`credit_company_by_id`, `:137`). **But** the comment at `:88-89` warns: "If no Construction-sector company exists in the region, the debited funds are still removed from the owner but not credited (leakage fallback)." So in regions without a construction company, maintenance is a sink.

**Severity:** Low in the 24-turn run (construction companies exist), but will bite in under-developed regions and during the early game.

**Fix direction:** When no local construction company exists, credit a national construction/maintenance fund or the nearest inter-regional construction company. Do not destroy the debited cash.

### 4.4 Money Creation / Destruction Sites

**Legitimate (banking):**
- **Creation:** `src/state/banking.rs:821-826`, `issue_loan()` — creates deposits (liability) without moving reserves. This is standard fractional-reserve money creation and is correct **provided** the loans are backed by real economic activity. In the 24-turn run (§1.6), this is the ventilator inflating M3 with no GDP backing — the mechanism is correct, but the **loan issuance criteria are too loose** (issuing credit into an economy with 100% unemployment and 0 GDP).
- **Destruction:** `src/state/banking.rs:2080, 2087`, loan repayment extinguishes deposits and transfers reserves out. Correct.

**Other:** `src/corporate/lifecycle.rs:261` destroys `private_capital` on company creation (capital is converted into the company's assets — this is a transfer, not a true destruction, but worth verifying the matching credit lands in the company's balance sheet).

**No unauthorized money printing was found outside the banking system.** The M3 explosion is "legitimate" credit creation that is mis-allocated because the real economy is broken.

### 4.5 Phantom GDP Risk

**LOW.** `compute_gdp` (`src/economy/telemetry.rs:80-94`) sums expenditure components from `GdpAccumulator`, which is populated **after** cash settlement (e.g. `src/engine/turn.rs:1635-1636` credits consumption only after `settle_b2c_clearing()` runs). So GDP tracks actual cash flows, not phantom transactions. The problem is the opposite: GDP is **understated** (stuck at 0) because the accumulator hooks never fire, not overstated.

### 4.6 Financial-Velocity Recommendations (for the blueprint)

1. **Fix Black Hole #1 (network maintenance)** — credit construction companies. Highest financial priority.
2. **Fix Black Hole #2 (freight fallback)** — route to a real recipient or model as fuel purchase.
3. **Implement Black Hole #3 (OHS compensation)** — debit employer, credit household. Add invariant test.
4. **Fix Black Hole #4 (maintenance leakage)** — credit national fund when no local construction company.
5. **Tighten bank loan issuance criteria** — do not issue credit into economies/regions with 0 production or 100% unemployment. Add a creditworthiness check.
6. **Re-audit citizen savings velocity** after §1.2/§1.5 are fixed — confirm the 95.78 wage floor is not forcing hand-to-mouth subsistence-only consumption.

---

## PART 5 — Global Structural Refactoring Blueprint

### 5.1 Current `src/` Tree Issues

The Phase 24B cleanup reorganized `economy/` into 11 thematic subfolders. The rest of `src/` was not touched and has accumulated **3 critical duplications**, several moderate duplications, and a handful of misplacements. Full inventory in the appendix; the conflicts are summarized here.

### 5.2 Critical Duplications

#### Duplication A — Treasury (CRITICAL)
- `government/treasury.rs` — "Treasury cycle: tax collection and government OPEX"
- `state/treasury.rs` — "Financial and structural country state" (the `Treasury` struct itself, with `liquid_reserves`, `private_capital`, `citizen_savings`, etc.)

**Conflict:** Two files named `treasury.rs` in two different modules, both dealing with treasury. The `state/treasury.rs` file owns the data structure; `government/treasury.rs` owns the tax/OPEX cycle. This is a split-brain: the struct and its primary mutator live in different modules, forcing every consumer to import from both.

**Fix:** Merge `government/treasury.rs` into `state/treasury.rs` (or a new `state/treasury/cycle.rs` submodule). Remove `government/treasury.rs`. The `Treasury` struct and its turn cycle should live together.

#### Duplication B — Banking (CRITICAL)
- `economy/banking.rs` — "Commercial banking" (14 KB)
- `state/banking.rs` — "Commercial banking state" (the large file with `process_banking_turn`, `issue_loan`, etc.)

**Conflict:** Two banking files. `state/banking.rs` is the real engine (2000+ lines, loan issuance, interbank, resolution); `economy/banking.rs` is a smaller legacy file. This is the same split-brain as treasury: the data and the logic are separated across module boundaries.

**Fix:** Merge `economy/banking.rs` into `state/banking.rs` (or `state/banking/` subfolder if it needs to split). Remove `economy/banking.rs`. Banking is state-level financial infrastructure, not a production-side economy concern.

#### Duplication C — Infrastructure (CRITICAL)
- `economy/state_sector/infrastructure.rs` + `economy/state_sector/infrastructure_config.rs` — "Infrastructure funding and production logic"
- `infrastructure/` folder (10 files) — capacity-based infrastructure model with templates (building_condition, care, cultural, education, effects, healthcare, heritage, maritime, pricing)

**Conflict:** Infrastructure logic is split across `economy/state_sector/` (funding/production) and `infrastructure/` (templates/capacity). A developer working on infrastructure must look in two places.

**Fix:** Move `economy/state_sector/infrastructure.rs` → `infrastructure/funding.rs`; merge `economy/state_sector/infrastructure_config.rs` → `infrastructure/config.rs`. Remove the infrastructure files from `economy/state_sector/`.

### 5.3 Moderate Duplications & Misplacements

| # | Issue | Files | Fix |
|---|---|---|---|
| D | Corporate R&D split | `economy/corporate_rd.rs` vs `corporate/strategy.rs` | Move `economy/corporate_rd.rs` → `corporate/rd.rs` |
| E | Religion scattered | `economy/religion/` (4 files) + `society/religious_authority.rs` + `society/culture_registry.rs` + `infrastructure/cultural.rs` | Consolidate into `society/religion/` subfolder |
| F | Society split | `economy/society/ethnic_violence.rs` vs `society/` | Move → `society/conflict.rs`; delete `economy/society/` |
| G | Justice in economy | `economy/justice/` (9 files) — justice is a political concern | Move → `politics/justice/` |
| H | Transport networks misplaced | `economy/logistics/transport_networks.rs` is physical infrastructure | Move → `infrastructure/transport.rs` |
| I | State sector scattered | `economy/state_sector/` (8 files) vs `government/` vs `state/` | Move → `state/public_sector/` |
| J | Agriculture at root | `src/agriculture.rs` is a lone root file | Move → `economy/production/agriculture.rs` |
| K | Maritime misplaced | `infrastructure/maritime.rs` (shipyards, ports, ships) | Move → `military/maritime.rs` |
| L | Securities separate from finance | `securities/` (11 files) vs `economy/finance/` (3 files) | Move → `economy/finance/securities/` |
| M | Data vs registries | `data/` (3 registry files) vs `registries/` (10 registry files) | Move → `registries/data/` |

### 5.4 Proposed Final Directory Tree

```
src/
├── corporate/          (+ rd.rs from economy/corporate_rd.rs)
├── construction/       (unchanged — process domain, distinct from infrastructure)
├── economy/
│   ├── indicators.rs
│   ├── real_estate.rs
│   ├── telemetry.rs
│   ├── config/         (unchanged)
│   ├── finance/
│   │   ├── debt_market.rs
│   │   ├── payment_in_kind.rs
│   │   └── securities/ (MOVED from securities/)
│   ├── labor/          (unchanged)
│   ├── logistics/      (transport_networks.rs MOVED OUT to infrastructure/)
│   ├── market/         (unchanged)
│   ├── production/
│   │   ├── agriculture.rs  (MOVED from src/agriculture.rs)
│   │   └── (existing files)
│   └── trade/          (unchanged)
├── engine/             (unchanged)
├── entities/           (unchanged)
├── government/
│   └── kio.rs          (treasury.rs REMOVED — merged into state/treasury.rs)
├── i18n/               (unchanged)
├── infrastructure/
│   ├── funding.rs      (MOVED from economy/state_sector/infrastructure.rs)
│   ├── config.rs       (MERGED from economy/state_sector/infrastructure_config.rs)
│   ├── transport.rs    (MOVED from economy/logistics/transport_networks.rs)
│   └── (existing files, except cultural.rs and maritime.rs MOVED OUT)
├── international/      (unchanged)
├── io/                 (unchanged)
├── math/               (unchanged)
├── military/
│   ├── maritime.rs     (MOVED from infrastructure/maritime.rs)
│   └── (existing files)
├── politics/
│   ├── justice/        (MOVED from economy/justice/)
│   └── (existing files)
├── registries/
│   ├── data/           (MOVED from data/)
│   └── (existing files)
├── society/
│   ├── religion/       (NEW — from economy/religion/ + society/religious_authority.rs + infrastructure/cultural.rs)
│   ├── conflict.rs     (MOVED from economy/society/ethnic_violence.rs)
│   └── (existing files, except religious_authority.rs MOVED to religion/)
├── state/
│   ├── banking.rs      (MERGED with economy/banking.rs)
│   ├── treasury.rs     (MERGED with government/treasury.rs)
│   ├── public_sector/  (MOVED from economy/state_sector/, minus infrastructure files)
│   └── (existing files)
├── ui/                 (unchanged)
└── utilities/          (unchanged)
```

### 5.5 Folders Removed / Created

**Removed:** `data/` (→ `registries/data/`), `economy/banking.rs` (merged into `state/banking.rs`), `economy/corporate_rd.rs` (→ `corporate/rd.rs`), `economy/justice/` (→ `politics/justice/`), `economy/religion/` (→ `society/religion/`), `economy/society/` (→ `society/conflict.rs`), `economy/state_sector/` (→ `state/public_sector/` + `infrastructure/funding.rs`), `government/treasury.rs` (merged into `state/treasury.rs`), `securities/` (→ `economy/finance/securities/`), `src/agriculture.rs` (→ `economy/production/agriculture.rs`).

**Created:** `economy/finance/securities/`, `politics/justice/`, `registries/data/`, `society/religion/`, `state/public_sector/`.

### 5.6 Refactoring Principles Applied

1. **Single source of truth** — one treasury, one banking, one infrastructure.
2. **Thematic cohesion** — justice with politics, religion with society, securities with finance, agriculture with production.
3. **No lone root files** — `agriculture.rs` moves into `economy/production/`.
4. **Data vs logic** — registries (immutable data) consolidated; state (mutable logic) consolidated.
5. **Minimize cross-folder coupling** — the duplications in §5.2 currently force every turn-cycle consumer to import from 2-3 modules for one concern.

### 5.7 Refactoring Risks & Mitigations

- **Risk:** Large `use` path rewrites across `engine/turn.rs` (the 180 KB orchestrator imports from nearly every module).
  - **Mitigation:** Do the moves with `sed`-style find-replace on `use sim_engine::` paths, then `cargo check` iteratively. Each move is a mechanical rename.
- **Risk:** `state/banking.rs` and `economy/banking.rs` may have symbol collisions when merged.
  - **Mitigation:** Audit symbol names first; the merge is likely a superset (state/banking.rs is the larger file).
- **Risk:** Test imports break.
  - **Mitigation:** Tests use `use sim_engine::...` paths that will update with the module moves. Run `cargo test --no-run` after each move.
- **Sequencing:** Do the refactoring **after** the GDP/employment fix (§1) and the black-hole fixes (§4.3). Refactoring first would make the bug fixes harder to locate; bug fixes first give a stable baseline to refactor from.

---

## Consolidated Blueprint — Fix Priority Order

The following is the recommended execution order, grouped by dependency. **Nothing here is implemented yet — all await explicit approval.**

### Tier 0 — Stop the Bleeding (GDP & Employment)
These are the root causes of the §1 collapse. Everything else is secondary until the economy produces GDP and employs people.

| # | Fix | File(s) | Rationale |
|---|---|---|---|
| 0.1 | **Fix GDP accumulator** — ensure `GdpAccumulator.consumption` is credited when B2C clearing settles, even for small transactions | `src/economy/telemetry.rs`, `src/engine/turn.rs:1633-1636` | GDP is 0.0 for 24 turns. This is the #1 bug. |
| 0.2 | **Fix employment collapse** — investigate why unemployment goes to 100% by turn 7. Likely the labor-market clearing is not matching workers to jobs, or jobs are not being created because companies cannot afford wages | `src/economy/labor/labor_market.rs`, `src/corporate/manager.rs` | 100% unemployment with growing M3 means the labor market is disconnected from production. |
| 0.3 | **Fix the 95.78 wage floor** — identify the hardcoded minimum and decide whether it is a treasury transfer (money creation) or a real subsidy. If it is keeping the population alive via phantom income, it must be funded from a real source | Search for `95.78` or minimum-wage constants | The identical wage across 6 countries is a hardcoded floor, not a market outcome. |
| 0.4 | **Fix M0/M3 initialization** — turn-0 M0/M3 should reflect actual treasury cash, not 0.0 | `src/economy/telemetry.rs` `compute_money_supply` | M3 = 0 at turn 0 despite 11 B reserves is a computation bug. |
| 0.5 | **Fix CPI/PPI freeze** — indices stuck at 100.0 with imbalanced markets mean price discovery is broken | `src/economy/telemetry.rs` `compute_inflation` | Frozen indices make inflation targeting impossible. |

### Tier 1 — Close the Black Holes (§4.3)
| # | Fix | File(s) |
|---|---|---|
| 1.1 | Network maintenance: credit construction company | `src/economy/logistics/transport_networks.rs:203-230` |
| 1.2 | Freight fallback: route to real recipient | `src/economy/logistics/logistics.rs:526-560` |
| 1.3 | OHS compensation: implement the 50,000 payment | `src/construction/fraud.rs:64`, `src/engine/turn.rs:1077-1091` |
| 1.4 | Building maintenance leakage: credit national fund | `src/economy/production/maintenance.rs:88-140` |
| 1.5 | Tighten bank loan issuance (creditworthiness check) | `src/state/banking.rs:821-826` |

### Tier 2 — Supply Chain & Market (§2.5, §1.8)
| # | Fix | File(s) |
|---|---|---|
| 2.1 | Add Turn-1 bootstrap inventory test | `tests/` (new) |
| 2.2 | Investigate Clothing-glut / Fibers-starvation paradox | `src/registries/production_methods_data.rs` (textile recipes) |
| 2.3 | Seed market with initial supply/demand at turn 0 (market_surplus_turn0.csv was empty) | `src/engine/generator/` |

### Tier 3 — Tech Tree Scaling Gaps (§3)
| # | Fix | File(s) |
|---|---|---|
| 3.1 | Add Automation + Organization methods to 12 state buildings | `src/registries/production_methods.rs:137-799` |
| 3.2 | Add Automation + Organization methods to 6 retail buildings (highest priority for B2C/GDP) | `src/registries/production_methods.rs:1002-1124` |
| 3.3 | Add upgrade paths to 8 religious/specialty buildings | `src/registries/production_methods.rs:810-1000` |
| 3.4 | Add Organization methods to education/healthcare | `src/registries/production_methods.rs:1126-1422` |
| 3.5 | Add 2+ methods to Inspektorat Ochrony Środowiska (only 1 method) | `src/registries/production_methods.rs:775-796` |

### Tier 4 — Structural Refactoring (§5)
Execute **after** Tiers 0-1 are stable and the 24-turn audit re-runs clean. Mechanical moves, low logic risk, high import-rewrite volume.

| # | Fix | Scope |
|---|---|---|
| 4.1 | Merge `government/treasury.rs` → `state/treasury.rs` | Duplication A |
| 4.2 | Merge `economy/banking.rs` → `state/banking.rs` | Duplication B |
| 4.3 | Move `economy/state_sector/infrastructure*.rs` → `infrastructure/` | Duplication C |
| 4.4 | Move `economy/corporate_rd.rs` → `corporate/rd.rs` | D |
| 4.5 | Consolidate religion into `society/religion/` | E |
| 4.6 | Move `economy/society/` → `society/conflict.rs` | F |
| 4.7 | Move `economy/justice/` → `politics/justice/` | G |
| 4.8 | Move `economy/logistics/transport_networks.rs` → `infrastructure/transport.rs` | H |
| 4.9 | Move `economy/state_sector/` → `state/public_sector/` | I |
| 4.10 | Move `src/agriculture.rs` → `economy/production/agriculture.rs` | J |
| 4.11 | Move `infrastructure/maritime.rs` → `military/maritime.rs` | K |
| 4.12 | Move `securities/` → `economy/finance/securities/` | L |
| 4.13 | Move `data/` → `registries/data/` | M |

---

## Appendix A — Audit Artefacts

All artefacts in `state/test_simulation_data_phase25/`:

| File | Contents |
|---|---|
| `phase25_treasury_audit.csv` | Per-country per-turn extended telemetry (25 turns × 6 countries = 150 rows, 21 columns) |
| `telemetry/<country>_macro.csv` | Phase 24F auto-generated macro CSV (6 files) |
| `market_surplus_turn0.csv` | Market net surplus at turn 0 (empty — only header) |
| `market_surplus_turn24.csv` | Market net surplus at turn 24 (31 commodity rows) |
| `budgets.json`, `macro.json`, `market.json`, etc. | Final persisted game state |

Test source: `state/tests/golden_year_24_turns.rs` (newly added).

## Appendix B — Sub-Audit Sources

- **Supply chain & orphaned commodities:** Full producer/consumer map for all 140 commodities, circular-deadlock analysis, Turn-1 freeze risk assessment. (Sub-agent audit, cross-checked against `tests/supply_chain_integrity_test.rs`.)
- **Tech tree & production method slots:** Per-building method counts for all 13 economic sectors + 12 state + 6 retail + 8 specialty + 8 education/healthcare buildings. (Sub-agent audit of `src/registries/production_methods.rs` and `production_methods_data.rs`.)
- **Financial velocity & black holes:** Citizen savings flow, ministry budget enforcement, freight/OHS/commuting/maintenance money trails, money creation/destruction sites, phantom GDP risk. (Sub-agent audit with file:line citations.)
- **Structural refactoring:** Full `src/` tree inventory, per-folder file descriptions, 3 critical + 10 moderate duplications/misplacements, proposed final tree. (Sub-agent audit.)

---

## Status

**This audit is complete. No codebase changes have been made.**

The test `tests/golden_year_24_turns.rs` has been added (it is a diagnostic, not a fix) and the 24-turn simulation has been run once to produce the empirical data. The artefacts in `test_simulation_data_phase25/` are the evidence base for §1.

**Awaiting explicit approval to proceed with any tier of the Consolidated Blueprint.** Recommended starting point: **Tier 0 (GDP & Employment)**, because no other fix can be validated until the economy produces GDP and employs people.
