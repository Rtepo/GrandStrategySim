# Phase 44 — The Genesis Overhaul: Demand-Driven Supply Chains, Era Awareness & Consumption Audit

**Status: BLUEPRINT — awaiting user approval. No code changes made.**

**Date: 2026-08-16**

---

## Executive Summary

A read-only audit of the world generator and B2C consumption pipeline has identified **ten root causes** that explain why the Market UI shows zero Supply/Demand for nearly all commodities and why the initial world economy is non-functional:

1. **Monoculture sector generation** — `best_registry_method` picks the single highest-year method for an entire sector; ALL companies in that sector produce the SAME commodity. Agriculture produces only Cereal; LightIndustry produces only Clothing; HeavyIndustry produces only Steel/Machinery/Components.
2. **Missing processing plants** — Oil Refineries, Aluminum Smelters, Chemical Plants, Glass Works, Cement Plants, and Food Processing factories are never spawned, even though their production methods exist in the registry.
3. **Era-blind commodity selection** — While `best_registry_method` filters by year and tech prerequisites, it does not gate commodities by era. A 1900 scenario can spawn methods whose outputs feed advanced supply chains that don't exist yet.
4. **Unpopulated Market UI fields** — `GlobalMarket.supply_volume` and `GlobalMarket.demand_volume` are always empty `HashMap::new()`. They are read by the snapshot and rendered by the TUI, but no code ever writes to them. This is a Phase 43 regression: the fields were added to the struct but never wired to the order pipeline.
5. **B2C demand invisible in Market UI** — Consumer demand from `build_consumer_demand` is computed per-region and consumed by B2C clearing, but is never aggregated into `GlobalMarket.demand_volume`. The Market UI only reflects B2B order flow.
6. **Retail store inventory mismatch** — Stores are seeded with `Food, Cereal, Clothing, Meat`, but the consumption registry demands `Cereal, Vegetable, Protein, Meat, Fruit, HealthCapacity, EducationSlots, Clothing, Furniture, Radio, Luxury`. Vegetable, Protein, and Fruit have demand but zero store supply.
7. **Era-blind demographics** — `generate_class_demographics` uses a hardcoded 60% rural / 40% urban split for ALL scenarios. A 1900 scenario should be ~80% rural with Serfs; a 1975 scenario should be ~60% urban with no Serfs. The existing Serf consumption basket is never activated because Serfs are never generated.
8. **Zero genesis housing** — There is NO `generate_housing` function in the codebase. The generator creates companies, buildings, retail stores, and tourism entities, but zero residential buildings. On Turn 1, housing shortage is 100%, utility demand from housing is zero, and winter mortality has no housing data to work with.
9. **Subsistence economy results discarded** — `apply_payment_in_kind` (line 56 of `payment_in_kind.rs`) correctly computes in-kind deductions for Serfs (food, heat, clothing received as barter instead of cash wages), but at line 2204 of `turn.rs` the results are thrown away: `let (_in_kind_ledger, _nutritional_deficit) = apply_payment_in_kind(...)`. The entire subsistence economy is calculated but never recorded or valued.
10. **No imputed GDP for subsistence consumption** — `GdpAccumulator.consumption` (line 81 of `telemetry.rs`) only tracks B2C retail cash revenue. There is no path to value in-kind / subsistence consumption at market prices and add it to GDP. A 1900 scenario with 80% rural Serfs would show artificially low GDP because the majority of real economic activity (subsistence farming) is unmonetized and uncounted — exactly the flaw the user identified by referencing Victoria 3's Subsistence Farms.
11. **Polish field names in Region struct** — The `Region` and `LandRegistry` structs use Polish field names (`ziemia_orna_max`, `ziemia_orna_wykorzystana`, `klimat`, `profil_gleb`, `limity_wydobycia`, `limity_wykorzystane`, `zasoby`, `ziemia_zabudowana`, `hektary_skarb_panstwa`, `hektary_obywateli`, `hektary_korporacji`, `rezerwy_strategiczne`). The codebase policy is English-only; these must be renamed with `#[serde(alias = "...")]` for backward compatibility with existing saves.

---

## Part 1: Top-Down Demand-Driven Generation

### Current Architecture

The corporate generator (`state/src/engine/generator/corporate.rs`) creates companies in two passes:

1. **Seed pass** (`seed_minimum_viable_supply_chain`, line 1145): Creates one building per critical sector per region. For HeavyIndustry, it distributes across three product types (Steel, IndustrialMachinery, MechanicalComponents) via a random roll. For all other sectors, it calls `best_registry_method`.

2. **Budget-share pass** (`generate_region_companies`, line 452): Creates a power-law distribution of companies per sector per region, scaled by the sector's employment share. ALL companies in this pass call `best_registry_method(sector, start_year, registries)` at line 630.

### Root Cause: `best_registry_method` (line 1068)

```rust
fn best_registry_method(sector, start_year, registries) -> (String, ActiveProductionMethod) {
    let best = building_methods.production.values()
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| tech_requirements_met)
        .max_by_key(|pm| pm.year)   // ← Picks the SINGLE highest-year method
        .or_else(|| earliest_method);
}
```

This function selects the method with the highest year ≤ `start_year` whose tech requirements are met. It returns ONE method for the ENTIRE sector. Every company in that sector — both seed and budget-share — receives the same `ActiveProductionMethod`, producing the same commodity.

### Concrete Impact by Sector

**Agriculture** (`agriculture_methods()`, line 217):
- 1900 → "Steam Tractors" (1895) → outputs: **Cereal only**
- 1925 → "Hybrid Seeds" (1960 — wait, 1960 > 1925, so filtered) → "Mechanized Harvesting" (1950 — also > 1925) → "Steam Tractors" (1895) → **Cereal only**
- 1950 → "Mechanized Harvesting" (1950) → **Cereal only**
- 1975 → "Hybrid Seeds" (1960) → **Cereal only**

Available but NEVER selected: Vegetable Farming, Protein Farming, Orchard Cultivation, Livestock Ranching, Industrial Fiber Farming, Luxury Crop Plantation, Seed Production, Fodder Production, Timber Plantation.

**LightIndustry** (`light_industry_methods()`, line 593):
- 1900 → "Power Looms" (1885, requires steam_001 tech) → **Clothing only**
- 1975 → "Automated Textile Mills" (1965, requires auto3_003 tech) → **Clothing only**

Available but NEVER selected: Sawmill, Furniture Workshop, Paper Mill, Food Processing, Textile Mill, Luxury Clothing Atelier, Medical Equipment Workshop, Appliance Assembly.

**HeavyIndustry** (`heavy_industry_methods()`, line 320):
- The seed pass distributes across 3 categories (33% each): Steel, IndustrialMachinery, MechanicalComponents.
- The budget-share pass uses `best_registry_method` → picks the highest-year Steel method.
- Available but NEVER selected: Coke Production, Cement Production, Brick Making, Glass Making, Aluminum Smelting, Silicon Purification, Basic Chemical Production, Solvay Process, Haber-Bosch Process, Fertilizer Production, **Oil Refining**, Plastics Production, Asphalt Production, Catalyst Production, Semiconductor Fabrication, Advanced Electronics, Software Development, Battery Production, Pharmaceutical Production, Automobile Assembly.

### The Blueprint: Demand-Driven Generation

Redesign `seed_minimum_viable_supply_chain` and `generate_region_companies` to use a **top-down demand-driven** approach:

**Step 1: Calculate total population needs.**
- Sum `build_consumer_demand` across all regions to get total per-commodity consumer demand.
- Key demand commodities: Cereal, Vegetable, Protein, Meat, Fruit, Clothing, Furniture, HealthCapacity, EducationSlots.

**Step 2: Spawn Agriculture to meet food demand — BOUND BY ARABLE LAND.**

> **STRICT RULE (user correction):** Unlike factories, Agriculture is strictly bound by geography. Before spawning agricultural companies in a region, the generator MUST check `region.ziemia_orna_max` (arable land max, in hectares). If a region has negligible arable land, do not spawn massive agricultural operations there, regardless of national demand. Distribute agricultural companies proportionally to the `ziemia_orna_max` of the regions.

- Calculate each region's arable land share: `region_arable_share = region.ziemia_orna_max as f64 / total_country_arable`.
- Allocate agricultural company count per region proportional to `region_arable_share`, NOT just `region.population`.
- **STRICT RULE: Regions with `arable_land_max <= 0` get EXACTLY ZERO agricultural companies. NO FALLBACK FARMS.** The region must rely 100% on imported food via the B2B/B2C logistics market. Do not violate geography to create an artificial safety net.
- Within each region's allocation, distribute across multiple food-producing methods:
  - 40% Cereal producers (Manual Farming / Steam Tractors / Hybrid Seeds by era)
  - 20% Vegetable producers (Vegetable Farming)
  - 15% Protein producers (Protein Farming)
  - 15% Meat/Livestock producers (Livestock Ranching)
  - 10% Fruit/Other (Orchard Cultivation, Industrial Fiber Farming, etc.)
- Scale the total number of companies to meet ~80-100% of base food demand, but cap by available arable land.

**Step 3: Spawn LightIndustry to meet consumer goods demand.**
- Distribute across:
  - 30% Clothing producers (Handloom/Power Loom/Electric Loom by era)
  - 25% Food Processing (transforms Cereal+Vegetable+Protein → Food)
  - 15% Furniture (Sawmill + Furniture Workshop)
  - 10% Paper Mill
  - 10% Textile Mill (produces Fibers for Clothing)
  - 10% other era-appropriate goods

**Step 4: Calculate intermediate goods demand.**
- From the Agriculture + LightIndustry companies spawned, sum their input requirements.
- Key intermediate demands: Steel, Fuels, Energy, MechanicalComponents, IndustrialMachinery, Chemicals, Fibers, Timber, Planks, Glass, Cement.

**Step 5: Spawn HeavyIndustry to meet industrial demand.**
- Extend the existing 3-way split (Steel/Machinery/Components) to a broader distribution:
  - 20% Steel producers
  - 15% IndustrialMachinery producers
  - 15% MechanicalComponents producers
  - 10% Oil Refining (if Oil deposits exist) → produces Fuels, Bitumen
  - 10% Chemical Production → produces Chemicals
  - 5% Cement Production (if Limestone deposits exist)
  - 5% Glass Making (if Sand deposits exist)
  - 5% Aluminum Smelting (if Bauxite deposits exist, era ≥ 1900)
  - 5% Coke Production (if HardCoal deposits exist)
  - 5% Fertilizer Production (if Ammonia/Chemicals available)
  - 5% era-appropriate advanced (Pharmaceuticals, Semiconductors, etc.)

**Step 6: Spawn Mining strictly matching geological deposits.**
- Already fixed in Phase 43 (one company per deposit, capped at 5 per region).
- No changes needed to the mining generation itself, but the deposit-to-refinery linkage must be added (see Part 3).

### Files to Modify

- `state/src/engine/generator/corporate.rs` — `seed_minimum_viable_supply_chain`, `create_seed_company`, `generate_region_companies`, new diversification helper functions.
- Potentially `state/src/engine/generator/mod.rs` — if demand calculation needs to happen before corporate generation.

### Implementation Approach

Create a new function `diversified_method_for_sector(sector, start_year, registries, rng, commodity_priorities)` that returns a method based on weighted random selection from era-appropriate methods, rather than always picking the highest-year method. The weights should be derived from the demand-driven calculation.

For the budget-share pass, modify `generate_region_companies` to call the diversified selector instead of `best_registry_method`.

---

## Part 2: Era Awareness (1900 vs 1975 Scenarios)

### Current State

The `StartYear` enum (line 43 of `generator/mod.rs`) defines four scenarios:
- Y1900 — Age of Steam and Coal
- Y1925 — Factories and Electricity
- Y1950 — Golden Age of Industry
- Y1975 — Dawn of the Silicon Age

`best_registry_method` already filters by `pm.year <= start_year` and checks `required_tech` prerequisites. This prevents, for example, "Semiconductor Fabrication" (1970, requires `semi_003`) from appearing in 1900.

### What's Missing

The year filter prevents tech-inappropriate methods, but it does NOT prevent **commodity-inappropriate** methods. The issue is that the highest-year method for a sector may produce commodities that belong to a different era's supply chain:

- In 1975, LightIndustry picks "Automated Textile Mills" (1965) which requires `ElectronicComponents` as input. But no Electronics industry exists at world start, so these factories can never produce.
- In 1900, if we diversify LightIndustry, we must NOT select "Appliance Assembly" (1935) or "Synthetic Fiber Production" (1935) — these require Plastics/ElectronicComponents that don't exist yet.
- In 1900, HeavyIndustry must NOT select "Aluminum Smelting" (1900, requires `metall_006` tech) unless that tech is actually discovered.

### The Blueprint: Era-Aware Commodity Gating

Add an `era_filter` layer on top of the existing year/tech filter:

1. **Define era-appropriate commodity sets:**
   - **1900:** Coal, Steam, Iron, Steel, Cereal, Vegetable, Protein, Meat, Clothing, Fibers, Timber, Planks, Furniture, Paper, Glass, Cement, Bricks, Coke, Basic Chemicals, Fuels (from Oil Refining), MechanicalComponents, IndustrialMachinery, Energy (Coal-Fired)
   - **1925:** Add: Electricity-based methods, Aluminum, Synthetic Chemicals, Radio, Automobiles
   - **1950:** Add: Plastics, ElectronicComponents, Fertilizers, Pharmaceuticals, AGD (appliances), Modern Steel processes
   - **1975:** Add: Semiconductors, Software, Computers, Advanced Electronics, Batteries, EVs, Precision Farming

2. **In the diversified method selector, filter methods by whether their outputs are in the era-appropriate set.**

3. **For inputs, prefer methods whose inputs are also era-appropriate** (to avoid factories that require non-existent inputs). This is similar to the existing `best_simple_machinery_method` pattern that excludes `ElectronicComponents` and `Semiconductors`.

4. **Scale sector shares by era:**
   - 1900: Agriculture 40-50%, HeavyIndustry 20-25%, LightIndustry 15-20%, Services 10-15%
   - 1975: Agriculture 5-10%, HeavyIndustry 25-30%, LightIndustry 20-25%, Services 30-40%
   - The current `build_treasury` (line 511) already randomizes sector shares but does not adjust them by era.

### Files to Modify

- `state/src/engine/generator/corporate.rs` — Add era filtering to method selection.
- `state/src/engine/generator/mod.rs` — Adjust `build_treasury` sector shares by `start_year`.
- `state/src/registries/production_methods_data.rs` — No changes needed; the data is already there.

---

## Part 2.5: Era-Appropriate Demographics & Genesis Housing

> **STRICT RULE (user correction):** The initial Rural/Urban population split must scale based on `start_year`. 1900 should be heavily Rural/Peasant; 1975 should be heavily Urban/Worker. Additionally, the Genesis generation MUST spawn enough Residential Buildings (Housing) to shelter the region's population and prevent an immediate Turn 1 homelessness/satisfaction crisis.

### Issue A: Era-Blind Demographics

**Current state:** `generate_class_demographics` (line 1446 of `geography.rs`) uses a hardcoded 60% rural / 40% urban split for ALL scenarios:

```rust
let rural_pop = (region_pop as f64 * 0.6) as i64;
let urban_pop = region_pop - rural_pop;
```

This is annotated as "1975 developing economy" but is applied to every year. A 1900 scenario should be far more rural (75-85%), while a 1975 scenario should be more urbanized (40-50% rural).

**The class distribution is also era-blind:**
- Rural: 50% FreePeasant, 45% LandlessLaborer, 5% Aristocracy
- Urban: 70% Worker, 30% Bourgeoisie

In 1900, Serfs should still exist (tied to latifundia). By 1975, Serfs should be emancipated and replaced by FreePeasants and LandlessLaborers. The Aristocracy share should shrink over time.

**The Blueprint:**

1. **Pass `start_year` to `generate_class_demographics`.** Currently it only takes `region_pop`. The function signature should become `generate_class_demographics(region_pop: i64, start_year: StartYear)`.

2. **Era-based rural/urban split:**
   - 1900: 80% rural, 20% urban
   - 1925: 70% rural, 30% urban
   - 1950: 55% rural, 45% urban
   - 1975: 40% rural, 60% urban

3. **Era-based class distribution:**
   - 1900 rural: 30% FreePeasant, 20% Serf, 45% LandlessLaborer, 5% Aristocracy
   - 1925 rural: 45% FreePeasant, 10% Serf, 40% LandlessLaborer, 5% Aristocracy
   - 1950 rural: 55% FreePeasant, 0% Serf, 38% LandlessLaborer, 7% Aristocracy
   - 1975 rural: 60% FreePeasant, 0% Serf, 35% LandlessLaborer, 5% Aristocracy
   - Urban classes scale similarly: 1900 has more Bourgeoisie (entrepreneurs), 1975 has more Workers.

4. **Add Serf class to consumption registry.** The consumption registry already has a "Serf" basket (line 96 of `consumption_registry.rs`), but `generate_class_demographics` never creates Serfs. With era-aware demographics, 1900 scenarios will finally populate this class.

### Issue B: Zero Genesis Housing (CRITICAL)

**Current state:** There is NO `generate_housing` function anywhere in the codebase. The generator creates companies, buildings, retail stores, tourism entities, NGOs, and churches — but **zero residential buildings**. `load_housing_buildings` (line 4640 of `turn.rs`) loads from `entities/<country>/housing/`, but that directory is never populated during world generation.

**Impact on Turn 1:**
- `process_utility_consumption` (line 43 of `consumption.rs`) iterates `housing_buildings` for utility demand — with zero housing, residential utility demand is zero.
- `calculate_housing_shortage` (line 174 of `development.rs`) compares total housing capacity vs population — with zero housing, shortage is 100%, triggering a flood of construction tenders.
- Winter mortality calculation has no housing data to work with.
- The property developer AI detects 100% shortage and tries to build, but construction takes multiple turns, leaving the population homeless for the first several turns.

**The Blueprint:**

1. **Create a `generate_housing` function** in `corporate.rs` (or a new `housing.rs` in the generator module), called during `generate_corporate_entities` after retail stores.

2. **Spawn housing proportional to population, per region — using MEGA-ESTATE CONSOLIDATION:**
   - Calculate total housing capacity needed: `region.population` households.
   - **STRICT CAP: Maximum 10-20 `HousingBuilding` entities per region.** Each entity represents a "Mega-Estate" with `total_capacity` of 10,000 to 50,000+ slots. This avoids Rayon threading / CPU bottlenecks during the turn loop. A region with 5 million population gets ~10-20 buildings with ~250,000-500,000 slots each, NOT 50,000 buildings with 100 slots each.
   - Distribute housing types based on era and class demographics:
     - 1900 rural: Mostly `Hut` (peasant huts) + a few `Palace` (aristocracy) + `FolwarkHousing` (for serfs/landless)
     - 1900 urban: `Tenement` (kamienica) for workers + `CityPalace` for bourgeoisie
     - 1975 rural: `Familok` (workers' housing) + `Hut` (modernized)
     - 1975 urban: `Tenement` + `SocialHousing` + `Beamciok` (specialist housing)

3. **Set `occupied_slots` to ~80-90% of `total_capacity`** to simulate pre-existing occupancy. The world doesn't start empty — people already live somewhere.

4. **Set `living_standard` based on housing type and era:**
   - Hut: 0.3-0.5
   - Slum: 0.2-0.4
   - Familok: 0.4-0.6
   - Tenement: 0.5-0.7
   - SocialHousing: 0.6-0.8
   - CityPalace/Palace: 0.8-1.0

5. **Assign `owner` based on housing type and class — DOUBLE-ENTRY RENT INTEGRITY:**

   > **STRICT RULE (user correction):** Every `HousingBuilding` MUST have a valid `owner` ID. If housing is owned by a generic or missing ID, citizen rent payments will vanish, creating a massive deflationary black hole.

   | Housing Type | Era | Owner ID | Rationale |
   |---|---|---|---|
   | `Palace` | 1900 rural | `"STATE:<country_id>"` | Aristocratic estates are state-granted; rent flows to treasury |
   | `FolwarkHousing` | 1900 rural | `"STATE:<country_id>"` | Latifundia estate housing; state collects rent on behalf of aristocracy |
   | `Hut` | 1900 rural | `"CLASS:Aristocracy:<region_id>"` | Peasant huts on aristocratic land; rent credited to aristocracy class savings |
   | `Tenement` | 1900/1925 urban | `"CLASS:Bourgeoisie:<region_id>"` | Urban tenements owned by bourgeoisie landlords; rent credited to bourgeoisie savings |
   | `CityPalace` | All urban | `"CLASS:Bourgeoisie:<region_id>"` | Luxury urban housing owned by wealthy bourgeoisie |
   | `Familok` | 1925/1950 | `"CLASS:Bourgeoisie:<region_id>"` | Industrial workers' housing owned by factory owners |
   | `Beamciok` | 1950/1975 | `"CLASS:Bourgeoisie:<region_id>"` | Specialist housing owned by private landlords |
   | `SocialHousing` | 1975 urban | `"STATE:<country_id>"` | State-funded housing; rent flows to treasury |

   **Rent crediting model:** The utility billing system (line 154-161 of `consumption.rs`) already credits energy companies by distributing `region_billing` proportionally. Residential rent collection must follow the same pattern:
   - For `"STATE:<country_id>"` owners: rent is credited to `country.budget.liquid_reserves`.
   - For `"CLASS:<class>:<region_id>"` owners: rent is credited to the corresponding class savings in `region.class_demographics.rural_classes` or `.urban_classes`.

   **Current state:** There is NO residential rent collection system in the turn loop. `rent_per_slot` is defined on `HousingSlots` (line 59 of `housing.rs`) but never read. The blueprint must add a rent collection step that:
   - Iterates `housing_buildings` per region.
   - Computes `rent = occupied_slots * rent_per_slot`.
   - Debits rent from the occupying class's savings.
   - Credits rent to the owner entity (State treasury or class savings).
   - This is a double-entry transfer: citizen savings decrease, owner savings/treasury increases by the same amount. No money is created or destroyed.

6. **Save housing buildings** to `entities/<country>/housing/` using `DiskEntityStore::<HousingBuilding>`.

7. **Set `utility_connections` with baseline values** so housing has minimal water/electricity/heating connections from the start (simulating existing infrastructure).

### Files to Modify

- `state/src/society/geography.rs` — `generate_class_demographics` (add `start_year` parameter, era-based splits).
- `state/src/engine/generator/corporate.rs` — New `generate_housing` function, called from `generate_corporate_entities`.
- `state/src/engine/generator/mod.rs` — Pass `start_year` through to `generate_regional_topology` → `generate_class_demographics`.
- `state/src/data/consumption_registry.rs` — No changes needed (Serf basket already exists).
- `state/src/engine/turn.rs` — New residential rent collection step (debit class savings, credit owner).
- `state/src/utilities/consumption.rs` — Reference for rent crediting pattern (utility billing model).

### Implementation Notes

- `generate_regional_topology` (line 1356 of `geography.rs`) is called from `generate_country` (line 339 of `mod.rs`) with `population` and `gdp` but NOT `start_year`. The `start_year` must be threaded through.
- The `HousingBuilding` struct (line 88 of `housing.rs`) requires: `id`, `housing_type`, `micro_region_id`, `owner`, `primary_slots` (with `total_capacity`, `occupied_slots`, `target_class`, `rent_per_slot`), `sublet_slots`, `living_standard`, `construction_cost`, `maintenance_cost`, `condition`, `utility_connections`.
- The `RuralClass` enum (line 652 of `geography.rs`) has: `Aristocracy`, `FreePeasant`, `Serf`, `LandlessLaborer`. The `target_class` field on `HousingSlots` takes `Option<RuralClass>`. Urban classes (Worker, Bourgeoisie) are NOT in `RuralClass` — they are string-keyed in `urban_classes`. Housing for urban classes should use `target_class: None` and rely on the building type to imply the class.
- **Mega-Estate capacity values:** `total_capacity` is `u32`, so max is 4,294,967,295. Values of 10,000-500,000 are well within range. The `occupied_slots` field is also `u32`.
- **Owner ID format:** The `owner` field is a `String`. Use prefixed IDs (`"STATE:<country_id>"`, `"CLASS:Aristocracy:<region_id>"`) to distinguish housing owners from company IDs. The rent collection system must parse these prefixes to route payments correctly.

---

## Part 2.6: The Subsistence Economy & Imputed GDP

> **STRICT RULE (user correction):** Serfs operate largely outside the cash economy, similar to Victoria 3's Subsistence Farms. They receive basic needs (food, heat, simple clothing) as in-kind payments from estate harvests. They do NOT participate in standard B2C cash clearing for these basic needs. However, the *market value* of goods produced and consumed by Serfs MUST be calculated and added to GDP as "imputed consumption". This ensures 1900 GDP accurately reflects the size of the real economy, even if it's unmonetized.

### Current State: Payment-in-Kind Exists But Is Discarded

The codebase already has a complete payment-in-kind system in `state/src/economy/finance/payment_in_kind.rs`:

- `apply_payment_in_kind` (line 56) correctly handles Serfs: it deducts subsistence-tier commodities (Cereal, Vegetable, Protein, Heat, Clothing) from the agricultural harvest bundle and assigns them to Serfs as in-kind payment instead of cash wages.
- It also handles FreePeasant/LandlessLaborer as VWAP wage offsets (partial in-kind, partial cash).
- It tracks nutritional deficits and quality penalties from substitution.
- It produces an `InKindLedger` (deductions per company, cash offsets) and `NutritionalDeficit` (unmet needs per class).

**The critical bug:** At line 2204 of `turn.rs`, the results are thrown away:

```rust
let (_in_kind_ledger, _nutritional_deficit) = apply_payment_in_kind(
    region,
    &labor_allocation,
    &mut harvest_bundle,
    turn,
);
```

Both values are prefixed with `_`, meaning they are never used again. The subsistence economy is computed but never recorded, never valued, and never contributes to GDP.

### Current State: GDP Has No Imputed Consumption Path

`GdpAccumulator` (line 79 of `telemetry.rs`) tracks:
- `consumption`: B2C retail cash revenue only (via `add_consumption`)
- `government_spending`, `investment`, `net_exports`, `shadow_gdp`

There is no field for imputed/subsistence consumption. The `compute_gdp` function (line 133) calculates `official_gdp = consumption + government_spending + investment + net_exports`. With Serfs comprising 20% of a 1900 population and receiving all their needs in-kind, that entire economic activity is invisible to GDP.

Additionally, `calculate_in_kind_value` (line 228 of `payment_in_kind.rs`) uses a placeholder VWAP of 1.0 currency unit per commodity unit:

```rust
// Placeholder VWAP: 1.0 currency unit per commodity unit
// In full implementation, this would use actual market VWAP
total_units * fte * 1.0
```

This must be replaced with actual market prices (VWAP from `MarketHistory` or `base_prices` from `GlobalMarket`).

### The Blueprint: Subsistence Economy Loop

**Step 1: Capture the in-kind ledger instead of discarding it.**

At line 2204 of `turn.rs`, remove the underscore prefixes and store the results on `CountryTask`:

```rust
let (in_kind_ledger, nutritional_deficit) = apply_payment_in_kind(...);
// Store on task for GDP accounting and B2C demand netting
task.in_kind_ledger = in_kind_ledger;
task.nutritional_deficit = nutritional_deficit;
```

Add `in_kind_ledger: InKindLedger` and `nutritional_deficit: NutritionalDeficit` fields to `CountryTask`.

**Step 2: Net out subsistence demand from B2C consumer demand.**

Serfs (and partially FreePeasants/LandlessLaborers) have their subsistence needs met in-kind. Their B2C demand for those commodities must be reduced accordingly, otherwise B2C clearing will try to sell them goods they already received from the harvest.

In `build_consumer_demand` (or in a new post-processing step), subtract the in-kind deductions from the demand basket:

```rust
for ((region_id, demography_type, class_id), deductions) in &in_kind_ledger.deductions_by_class {
    for (commodity, amount) in deductions {
        // Reduce B2C demand for this class/commodity by the in-kind amount
        let key = (region_id.clone(), *demography_type, class_id.clone());
        if let Some(class_demand) = consumer_demand.demand.get_mut(&key) {
            if let Some(existing) = class_demand.get_mut(commodity) {
                *existing = (*existing - amount).max(0.0);
            }
        }
    }
}
```

**Step 3: Calculate imputed consumption value and add to GDP.**

After payment-in-kind is applied, value the in-kind deductions at market prices and add to `GdpAccumulator`:

```rust
// For each in-kind deduction, value at VWAP or base_price
let mut imputed_consumption = 0.0;
for (company_id, deductions) in &in_kind_ledger.deductions {
    for (commodity, quantity) in deductions {
        let price = market_history.vwap(commodity)
            .unwrap_or_else(|| global_market.base_prices.get(commodity).copied().unwrap_or(100.0));
        imputed_consumption += quantity * price;
    }
}
task.gdp_acc.add_imputed_consumption(&region.id, imputed_consumption);
```

Add a new field `imputed_consumption: f64` to `GdpAccumulator` and `RegionalGdpAccumulator`, with an `add_imputed_consumption` method. Update `compute_gdp` to include it:

```rust
let official_gdp = acc.consumption
    + acc.imputed_consumption  // ← NEW
    + acc.government_spending
    + acc.investment
    + acc.net_exports;
```

**Step 4: Replace placeholder VWAP with actual market prices.**

Update `calculate_in_kind_value` (line 228 of `payment_in_kind.rs`) to accept a price map parameter instead of using `1.0`:

```rust
fn calculate_in_kind_value(
    class_id: &str,
    fte: f64,
    consumption: &BTreeMap<String, ConsumptionBasket>,
    prices: &BTreeMap<Commodity, f64>,  // ← NEW parameter
) -> f64 {
    // ... use prices.get(commodity).copied().unwrap_or(100.0) instead of 1.0
}
```

**Step 5: Track imputed consumption in `GdpBreakdown` for UI display.**

Add `imputed_consumption: f64` to the `GdpBreakdown` struct in `state/macro_data.rs` so the UI can display it separately from cash consumption. This gives the user visibility into how much of GDP is monetized vs. subsistence.

### Files to Modify

- `state/src/engine/turn.rs` — Line 2204: capture in-kind results instead of discarding; add imputed consumption to GDP accumulator after payment-in-kind.
- `state/src/economy/finance/payment_in_kind.rs` — `calculate_in_kind_value`: replace placeholder VWAP with actual prices; add per-class deduction tracking to `InKindLedger`.
- `state/src/economy/telemetry.rs` — `GdpAccumulator` and `RegionalGdpAccumulator`: add `imputed_consumption` field and `add_imputed_consumption` method; update `compute_gdp` to include imputed consumption.
- `state/src/state/macro_data.rs` — `GdpBreakdown`: add `imputed_consumption` field.
- `state/src/economy/trade/retail.rs` — `build_consumer_demand` or post-processing: net out in-kind deductions from B2C demand.
- `state/src/economy/finance/payment_in_kind.rs` — `InKindLedger`: add `deductions_by_class: BTreeMap<(String, DemographyType, String), BTreeMap<Commodity, f64>>` for per-class tracking (currently only tracks per-company).

### Accounting Invariants

- **No money creation:** Imputed consumption is a valuation entry only. No cash is debited or credited. It does not affect bank balances, treasury, or company cash.
- **No double-counting:** A commodity unit deducted in-kind is removed from the harvest bundle (already implemented at line 154 of `payment_in_kind.rs`). It cannot also be sold B2C. The B2C demand netting in Step 2 ensures the same unit is not counted as both imputed and cash consumption.
- **Strict GDP = C_cash + C_imputed + G + I + NX:** The imputed component is separately tracked so it can be excluded for monetary analysis if needed, but the official GDP includes it.
- **Era-dependent magnitude:** In 1900 with 20% Serfs, imputed consumption may be 15-25% of GDP. By 1975 with 0% Serfs, it should be near zero. This naturally reflects economic monetization over time.

---

## Part 2.7: Polish Field Name Purge

> **STRICT RULE (user correction):** The codebase maintains a strict English-only policy. The `Region` and `LandRegistry` structs must be refactored to use English field names. `#[serde(alias = "...")]` must be used to maintain backward compatibility with existing save JSONs.

### Current Polish Fields in Region Struct (line 521 of `geography.rs`)

| Polish Name | English Name | Type |
|---|---|---|
| `klimat` | `climate` | `Climate` |
| `profil_gleb` | `soil_profile` | `BTreeMap<String, f64>` |
| `ziemia_orna_max` | `arable_land_max` | `i64` |
| `ziemia_orna_wykorzystana` | `arable_land_used` | `i64` |
| `limity_wydobycia` | `extraction_limits` | `BTreeMap<String, i64>` |
| `limity_wykorzystane` | `extraction_used` | `BTreeMap<String, i64>` |
| `zasoby` | `resources` | `Map<String, Value>` |
| `ziemia_zabudowana` | `built_land` | `i64` |

### Current Polish Fields in LandRegistry Struct (line 492 of `geography.rs`)

| Polish Name | English Name | Type |
|---|---|---|
| `klimat` | `climate` | `Climate` |
| `profil_gleb` | `soil_profile` | `BTreeMap<String, f64>` |
| `ziemia_orna_max` | `arable_land_max` | `i64` |
| `ziemia_orna_wykorzystana` | `arable_land_used` | `i64` |
| `limity_wydobycia` | `extraction_limits` | `BTreeMap<String, i64>` |
| `limity_wykorzystane` | `extraction_used` | `BTreeMap<String, i64>` |
| `zasoby` | `resources` | `Map<String, Value>` |
| `hektary_skarb_panstwa` | `state_hectares` | `BTreeMap<String, i64>` |
| `hektary_obywateli` | `citizen_hectares` | `BTreeMap<String, i64>` |
| `hektary_korporacji` | `corporate_hectares` | `BTreeMap<String, i64>` |
| `rezerwy_strategiczne` | `strategic_reserves` | `Map<String, Value>` |
| `ziemia_zabudowana` | `built_land` | `i64` |

### The Blueprint: Refactoring Approach

For each Polish field, apply the following transformation:

```rust
// Before:
pub ziemia_orna_max: i64,

// After:
#[serde(alias = "ziemia_orna_max")]
pub arable_land_max: i64,
```

The `#[serde(alias = "...")]` attribute ensures that:
- **Deserialization:** Old save files with `"ziemia_orna_max"` in JSON will still load correctly.
- **Serialization:** New saves will use the Rust field name `arable_land_max` as the JSON key (unless `#[serde(rename = "...")]` is also present, which it should NOT be for these fields).
- **Rust code:** All references in `.rs` files must use the English name `arable_land_max`.

### Scope of Changes

This is a mechanical but wide-reaching refactor. Every reference to the Polish field names across the codebase must be updated:

- `state/src/society/geography.rs` — Struct definitions, `generate_land_registry`, `generate_regional_topology`, `migrate_soil_profile_to_land_inventory`, maritime/ocean node construction.
- `state/src/engine/generator/corporate.rs` — Agriculture generation (references `ziemia_orna_max` for arable land checks).
- `state/src/engine/turn.rs` — Any turn logic that reads/writes these fields.
- `state/src/agriculture/` — Harvest yield calculations that use arable land.
- All test files that construct `Region` or `LandRegistry` directly.

### Files to Modify

- `state/src/society/geography.rs` — Rename all Polish fields in `Region` and `LandRegistry`; add `#[serde(alias = "...")]` for backward compatibility.
- All files referencing the old field names — mechanical find-and-replace.
- `state/src/data/regions.json` — No changes needed (serde alias handles old keys).
- `state/src/data/macro.json` — No changes needed (serde alias handles old keys).

---

## Part 3: The Geology-Mining-Refining Pipeline

### Current State

**Mining generation** (`seed_geology_based_mines`, line 1234):
- Phase 43 fix works correctly: collects ALL deposits per region, creates one mining company per deposit, capped at 5 per region.
- Each mine gets `building.deposit_id = Some(deposit_id)` linking it to the specific deposit.
- Fallback coal mine spawned if no deposits exist in a region.

**Deposit generation** (`generate_geological_formations`, line 1722 of `geography.rs`):
- Creates 2-10 geological formations, each overlapping 2-5 regions.
- Each formation generates 1-3 resource deposits from a formation-type-specific commodity pool.
- Deposit commodities: HardCoal, Iron, Copper, Zinc, Gold, Silver, Oil, NaturalGas, BrownCoal, Peat, Uranium, Sulfur, Tin, Lead, Sand, Gravel.

### The Problem: Missing Processing Plants

Mining companies extract raw materials (Oil, Bauxite, Iron, etc.), but the processing plants that transform these into intermediate goods are never spawned:

| Raw Material | Processing Method | Sector | Output | Status |
|---|---|---|---|---|
| Oil | Oil Refining (1880) | HeavyIndustry | Fuels, Bitumen | **NEVER SPAWNED** |
| Oil | Plastics Production (1935) | HeavyIndustry | Plastics | **NEVER SPAWNED** |
| Bauxite | Aluminum Smelting (1900) | HeavyIndustry | Aluminum | **NEVER SPAWNED** |
| Iron | Bessemer Converters (1880) | HeavyIndustry | Steel | Spawned (33% of HI) |
| HardCoal | Coke Production (1880) | HeavyIndustry | Coke | **NEVER SPAWNED** |
| Limestone | Cement Production (1880) | HeavyIndustry | Cement | **NEVER SPAWNED** |
| Sand | Glass Making (1880) | HeavyIndustry | Glass | **NEVER SPAWNED** |
| Sulfur+Salt | Basic Chemical Production (1880) | HeavyIndustry | Chemicals | **NEVER SPAWNED** |
| NaturalGas | Haber-Bosch Process (1910) | HeavyIndustry | Ammonia | **NEVER SPAWNED** |
| Ammonia | Fertilizer Production (1880) | HeavyIndustry | Fertilizers | **NEVER SPAWNED** |

The HeavyIndustry seed pass only distributes across Steel/Machinery/Components. The budget-share pass uses `best_registry_method` which picks the highest-year Steel method. Neither pass ever selects Oil Refining, Chemical Production, or any other processing method.

### The Blueprint: Deposit-Driven Processing Plants

After mining generation, scan the spawned mining companies' deposit commodities and spawn corresponding processing plants:

1. **Build a `mined_commodities: HashSet<Commodity>`** from all spawned mining companies' deposit links.

2. **For each mined commodity, check if a processing method exists** in the HeavyIndustry registry that consumes it as an input.

3. **Spawn one processing plant per mined commodity type** (per region that has the corresponding mine), using `create_seed_company_with_method_name`.

4. **Era-filter the processing methods** — don't spawn Plastics Production in 1900.

5. **Ensure the supply chain is complete:**
   - Oil mined → Oil Refining spawned → Fuels/Bitumen available
   - Bauxite mined → Aluminum Smelting spawned (if era ≥ 1900) → Aluminum available
   - Iron mined → Steel production already spawned → Steel available
   - HardCoal mined → Coke Production spawned → Coke available
   - Limestone mined → Cement Production spawned → Cement available
   - Sand mined → Glass Making spawned → Glass available
   - Sulfur + Salt mined → Chemical Production spawned → Chemicals available
   - NaturalGas mined → Haber-Bosch spawned (if era ≥ 1910) → Ammonia → Fertilizer Production spawned → Fertilizers available

6. **Strict deposit linkage invariant:** A mining company CANNOT spawn unless it claims an unassigned `deposit_id`. This is already enforced by Phase 43. The new requirement is that processing plants MUST be spawned for mined commodities.

### Files to Modify

- `state/src/engine/generator/corporate.rs` — New function `spawn_processing_plants_for_mined_commodities` called after `seed_geology_based_mines`.
- New helper: `processing_method_for_commodity(commodity, start_year, registries) -> Option<&'static str>` mapping raw materials to processing method names.

---

## Part 4: The B2C Consumption Black Hole

### Issue A: Market UI Supply/Demand Always Zero (Phase 43 Regression)

**Root cause:** `GlobalMarket.supply_volume` and `GlobalMarket.demand_volume` are never populated.

**Evidence:**

`load_market` (line 4199 of `turn.rs`):
```rust
Ok(GlobalMarket {
    base_prices,
    net_surplus,
    offshore_capital: 0.0,
    apostolic_see_ledger: ...,
    supply_volume: HashMap::new(),  // ← ALWAYS EMPTY
    demand_volume: HashMap::new(),  // ← ALWAYS EMPTY
})
```

`save_market` (line 4214) saves `orders.orders` (buy/sell per commodity) to `market.json`, but does NOT save `supply_volume` or `demand_volume`.

`default_market` (line 4259) also initializes them as empty.

No code anywhere in the engine writes to `market.supply_volume` or `market.demand_volume` during the turn. The snapshot reads them (line 502 of `snapshot.rs`), and the TUI renders them (line 167 of `render.rs`), but the data is always zero.

**Fix:** In `load_market`, populate these fields from the loaded orders:
```rust
let mut supply_volume = HashMap::new();
let mut demand_volume = HashMap::new();
for (good, order) in parsed.orders {
    supply_volume.insert(good, order.sell);
    demand_volume.insert(good, order.buy);
}
```

This will make the Market UI show B2B order volumes for the previous turn.

### Issue B: B2C Consumer Demand Not Reflected in Market UI

The Market UI's Demand column shows B2B buy orders only. Consumer demand from `build_consumer_demand` is computed per-region in the R1/R6 phases (lines 2298, 2332 of `turn.rs`) and consumed by B2C clearing, but is never aggregated into `GlobalMarket.demand_volume`.

**Fix:** After B2C clearing, aggregate the consumer demand per commodity and add it to `GlobalMarket.demand_volume`:
```rust
// After B2C clearing for all regions:
for (commodity, demand) in &total_b2c_demand {
    *market.demand_volume.entry(*commodity).or_insert(0.0) += demand;
}
```

This will make the Market UI show total demand (B2B + B2C) for each commodity.

### Issue C: Retail Store Inventory Mismatch

Retail stores are seeded with (`generate_retail_stores`, line 2328):
```rust
let seed_goods = [
    (Commodity::Food, 50.0 * production_scale),
    (Commodity::Cereal, 30.0 * production_scale),
    (Commodity::Clothing, 10.0 * production_scale),
    (Commodity::Meat, 5.0 * production_scale),
];
```

But the consumption registry demands:
- **Cereal** ✓ (in store inventory)
- **Vegetable** ✗ (NOT in store inventory)
- **Protein** ✗ (NOT in store inventory)
- **Meat** ✓ (in store inventory, but only Worker/Bourgeoisie/Aristocracy demand it)
- **Fruit** ✗ (NOT in store inventory)
- **Clothing** ✓ (in store inventory)
- **Food** ✗ (NOT in consumption registry — it's an intermediate good)
- **HealthCapacity** ✗ (service, not a retail good)
- **EducationSlots** ✗ (service, not a retail good)
- **Furniture** ✗ (NOT in store inventory)
- **Radio** ✗ (NOT in store inventory)

**Result:** B2C clearing can only sell Cereal, Meat, and Clothing. Vegetable, Protein, Fruit, and Furniture demand is permanently unmet.

**Fix:** Update the retail store seed inventory to match the consumption registry:
```rust
let seed_goods = [
    (Commodity::Cereal, 30.0 * production_scale),
    (Commodity::Vegetable, 20.0 * production_scale),
    (Commodity::Protein, 15.0 * production_scale),
    (Commodity::Meat, 10.0 * production_scale),
    (Commodity::Fruit, 8.0 * production_scale),
    (Commodity::Clothing, 10.0 * production_scale),
    (Commodity::Furniture, 5.0 * production_scale),
    (Commodity::Food, 20.0 * production_scale),  // Keep for production input demand
];
```

### Issue D: Wasted B2C Demand Computation

At line 2298 of `turn.rs`:
```rust
let _consumer_demand = build_consumer_demand(region, turn);
```

The demand is built but immediately discarded (underscore prefix). This is wasted computation. The demand is rebuilt at line 2332 for the actual clearing. The first call at line 2298 should be removed.

### Issue E: B2C Demand Requires Savings

`settle_b2c_clearing` (line 547 of `retail.rs`) debits citizen class savings to pay for purchases. If class savings are zero, B2C revenue settles as zero even if clearing matched demand with supply.

The `generate_class_demographics` function (line 1446 of `geography.rs`) does seed savings:
- FreePeasant: 100.0 per capita
- LandlessLaborer: 50.0 per capita
- Aristocracy: 5000.0 per capita
- Worker: 200.0 per capita
- Bourgeoisie: 1000.0 per capita

These are positive, so savings should not be the blocker for initial B2C. However, the per-capita amounts are small relative to commodity prices (acquisition_cost_per_unit = 100.0 in seed inventory), so citizens may run out of savings quickly if B2C wages are not flowing.

### Files to Modify

- `state/src/engine/turn.rs` — `load_market` (populate supply/demand volumes), B2C demand aggregation, remove wasted computation at line 2298.
- `state/src/engine/generator/corporate.rs` — `generate_retail_stores` (fix seed inventory).
- `state/src/economy/trade/retail.rs` — No changes needed to clearing logic itself.

---

## Implementation Steps (Ordered)

### Step 1: Fix Market UI Supply/Demand (Issue A)
- **File:** `state/src/engine/turn.rs` — `load_market` function (~line 4199)
- **Change:** Populate `supply_volume` and `demand_volume` from parsed orders.
- **Risk:** Low. Pure data wiring fix.

### Step 2: Fix Retail Store Seed Inventory (Issue C)
- **File:** `state/src/engine/generator/corporate.rs` — `generate_retail_stores` (~line 2328)
- **Change:** Add Vegetable, Protein, Fruit, Furniture to seed goods; keep Food for production input demand.
- **Risk:** Low. Only affects initial store inventory.

### Step 3: Remove Wasted B2C Demand Computation (Issue D)
- **File:** `state/src/engine/turn.rs` (~line 2298)
- **Change:** Remove the `let _consumer_demand = build_consumer_demand(region, turn);` line and its enclosing loop.
- **Risk:** Low. Dead code removal.

### Step 4: Add B2C Demand to Market UI (Issue B)
- **File:** `state/src/engine/turn.rs` — After B2C clearing phase (~line 2358)
- **Change:** Aggregate per-commodity B2C demand across all regions and add to `market.demand_volume`.
- **Risk:** Low. Additive change.

### Step 5: Purge Polish Field Names (Part 2.7)
- **File:** `state/src/society/geography.rs` — `Region` and `LandRegistry` structs
- **Change:** Rename all Polish fields to English (`ziemia_orna_max` → `arable_land_max`, etc.). Add `#[serde(alias = "...")]` for backward compatibility with existing saves. Update all references across the codebase.
- **Risk:** Low-Medium. Mechanical refactor; serde aliases ensure save compatibility. Must find all references via grep.

### Step 6: Diversify Agriculture Generation — Arable-Land-Bound (Part 1)
- **File:** `state/src/engine/generator/corporate.rs`
- **Change:** Replace `best_registry_method` for Agriculture with a diversified selector that distributes across Cereal, Vegetable, Protein, Meat, Fruit, and other food methods. **Agricultural company count per region must be proportional to `region.arable_land_max`** (renamed from `ziemia_orna_max` in Step 5), not just population. **Regions with `arable_land_max <= 0` get EXACTLY ZERO agricultural companies — NO FALLBACK FARMS.** The region relies 100% on imported food via B2B/B2C logistics.
- **Risk:** Medium. Changes initial world composition.

### Step 7: Diversify LightIndustry Generation (Part 1)
- **File:** `state/src/engine/generator/corporate.rs`
- **Change:** Replace `best_registry_method` for LightIndustry with a diversified selector that distributes across Clothing, Food Processing, Furniture, Paper, Textile, and other consumer goods methods.
- **Risk:** Medium. Changes initial world composition.

### Step 8: Diversify HeavyIndustry Generation (Part 1)
- **File:** `state/src/engine/generator/corporate.rs`
- **Change:** Extend the existing 3-way split to include Oil Refining, Chemical Production, Cement, Glass, Aluminum Smelting, Coke, Fertilizer, and other processing methods.
- **Risk:** Medium. Changes initial world composition.

### Step 9: Add Era-Aware Commodity Gating (Part 2)
- **File:** `state/src/engine/generator/corporate.rs`
- **Change:** Add era filter that excludes era-inappropriate commodities from method selection. Define era-appropriate commodity sets for 1900, 1925, 1950, 1975.
- **Risk:** Medium. Must be careful not to over-filter and create supply gaps.

### Step 10: Adjust Sector Shares by Era (Part 2)
- **File:** `state/src/engine/generator/mod.rs` — `build_treasury` (~line 511)
- **Change:** Adjust random sector share ranges based on `start_year`. 1900 should have higher Agriculture, lower Services. 1975 should have lower Agriculture, higher Industry/Services.
- **Risk:** Low-Medium. Only affects initial proportions.

### Step 11: Era-Aware Demographics (Part 2.5)
- **File:** `state/src/society/geography.rs` — `generate_class_demographics` (~line 1446)
- **Change:** Add `start_year: StartYear` parameter. Scale rural/urban split by era (1900: 80/20, 1975: 40/60). Adjust class distribution (1900 includes Serfs, 1975 has no Serfs). Thread `start_year` through `generate_regional_topology` and `generate_country`.
- **Risk:** Medium. Changes population composition; may affect labor market and consumption.

### Step 12: Genesis Housing Generation (Part 2.5)
- **File:** `state/src/engine/generator/corporate.rs` — New `generate_housing` function
- **Change:** Spawn `HousingBuilding` entities proportional to region population, with era-appropriate housing types (Huts/Folwark for 1900 rural, Tenements for urban, SocialHousing for 1975). Set `occupied_slots` to ~80-90% of capacity. Save to `entities/<country>/housing/`.
- **Risk:** Medium. New entity type in generation; must integrate with utility demand and winter mortality systems.

### Step 13: Subsistence Economy — Capture In-Kind Ledger (Part 2.6)
- **File:** `state/src/engine/turn.rs` (~line 2204), `state/src/economy/finance/payment_in_kind.rs`
- **Change:** Remove underscore prefixes from `in_kind_ledger` and `nutritional_deficit`. Store on `CountryTask`. Add per-class deduction tracking to `InKindLedger`. Replace placeholder VWAP (1.0) in `calculate_in_kind_value` with actual market prices.
- **Risk:** Medium. Must ensure in-kind deductions are correctly attributed per class for B2C demand netting.

### Step 14: Subsistence Economy — Imputed GDP (Part 2.6)
- **File:** `state/src/economy/telemetry.rs`, `state/src/state/macro_data.rs`, `state/src/engine/turn.rs`
- **Change:** Add `imputed_consumption` field to `GdpAccumulator`, `RegionalGdpAccumulator`, and `GdpBreakdown`. Add `add_imputed_consumption` method. Value in-kind deductions at VWAP/base_price and add to GDP. Update `compute_gdp` to include imputed consumption in official GDP.
- **Risk:** Medium. Must ensure no double-counting with B2C cash consumption.

### Step 15: Subsistence Economy — Net B2C Demand (Part 2.6)
- **File:** `state/src/economy/trade/retail.rs` — `build_consumer_demand` or post-processing
- **Change:** After in-kind payment is applied, subtract in-kind deductions from B2C consumer demand so Serfs (and partially FreePeasants) don't double-demand goods they already received.
- **Risk:** Medium. Must correctly match in-kind deductions to demand basket entries by class and commodity.

### Step 16: Spawn Processing Plants for Mined Commodities (Part 3)
- **File:** `state/src/engine/generator/corporate.rs`
- **Change:** After mining generation, scan mined commodities and spawn corresponding processing plants (Oil Refining, Aluminum Smelting, Chemical Production, etc.).
- **Risk:** Medium. Must ensure era-appropriate filtering and avoid over-spawning.

### Step 17: Build, Test, Verify
- `cargo build`
- `cargo test --lib -- --test-threads=1`
- Expected baseline: 698+ tests passing, 0 failed.
- Run a 10-turn simulation and verify Market UI shows non-zero Supply/Demand for Cereal, Vegetable, Protein, Meat, Clothing, Fuels, Steel, etc.
- Verify housing capacity ≥ 90% of population in all regions.
- Verify 1900 scenario has >70% rural population; 1975 has >50% urban.
- Verify 1900 GDP includes imputed consumption from Serf subsistence (check GdpBreakdown.imputed_consumption > 0).
- Verify old save files load correctly with serde aliases (no data loss).

---

## Risks and Considerations

1. **Test breakage:** Diversifying sector generation will change the initial world composition, which may break golden master tests that assert specific company counts or sector distributions. These tests may need updating.

2. **Performance:** Adding more companies (processing plants, diversified agriculture) and housing buildings increases entity count. The current cap of 20 companies per sector per region should be respected. Housing buildings should be capped per region to avoid entity explosion.

3. **Supply chain bootstrapping:** Even with diversified generation, some supply chains have circular dependencies (e.g., Machinery production requires Steel, Steel production requires Machinery). The existing `seed_inventory` function (line 1919) handles this by seeding one cycle of inputs, which is sufficient for bootstrapping.

4. **B2C demand vs. B2B demand in Market UI:** Adding B2C demand to `demand_volume` will make the Market UI show total demand, which may be confusing if the user expects to see only B2B order flow. Consider adding a separate "B2C Demand" column or a toggle.

5. **Era filtering granularity:** Over-filtering era-appropriate commodities could create supply gaps. The era sets must be carefully designed to ensure every demand commodity has at least one era-appropriate production method.

6. **Deposit-processing linkage:** The processing plant spawning must handle cases where a mined commodity has no processing method (e.g., Gold, Silver, Peat). These should be skipped gracefully.

7. **Arable land zero-edge case — STRICT ZERO, NO FALLBACK:** Some regions may have `arable_land_max = 0` (e.g., mountainous or sea regions). The agriculture generator must spawn EXACTLY ZERO agricultural companies in such regions. NO FALLBACK FARMS. The region relies 100% on imported food via the B2B/B2C logistics market. This is a strict geographic constraint — do not violate it to create an artificial safety net.

8. **Demographics thread-through:** `start_year` must be threaded through `generate_country` → `generate_regional_topology` → `generate_class_demographics`. This changes function signatures and may require updates to all callers, including tests that construct regions directly.

9. **Housing entity count — MEGA-ESTATE CONSOLIDATION (STRICT CAP):** A region with 5 million population needs ~5 million housing slots. If each `HousingBuilding` has ~100 slots (tenement), that's 50,000 buildings per region — far too many. **STRICT RULE: Consolidate Genesis Housing into "Mega-Estates".** Each `HousingBuilding` entity must have `total_capacity` of 10,000 to 50,000+ slots. Cap the total number of residential building entities spawned per region to a MAXIMUM of 10-20. This completely avoids Rayon threading / CPU bottlenecks during the turn loop. A region with 5M population gets ~10-20 buildings with ~250,000-500,000 slots each.

10. **Serf subsistence economy — historically accurate, not a bug:** Serfs operate outside the cash economy by design (similar to Victoria 3's Subsistence Farms). They receive basic needs as in-kind payments from estate harvests. This is NOT a labor market bug — it is the intended economic model for pre-industrial scenarios. The implementation must:
    - Correctly route subsistence needs through `apply_payment_in_kind` (already built, just needs results captured).
    - Value in-kind consumption at market prices for imputed GDP (new — `GdpAccumulator.imputed_consumption`).
    - Net out in-kind-satisfied demand from B2C so Serfs don't double-demand goods.
    - As the economy monetizes (1925 → 1950 → 1975), Serfs transition to FreePeasant/LandlessLaborer, imputed consumption shrinks, and cash B2C consumption grows. This naturally reflects economic development.

11. **Polish field rename — save compatibility:** The `#[serde(alias = "...")]` approach ensures old saves deserialize correctly, but new saves will use English keys. This means saves are forward-compatible (old → new) but NOT backward-compatible (new → old). If a user downgrades the engine after saving with English keys, the old engine won't read the new save. This is acceptable per the project's forward-only save policy.

12. **Imputed GDP double-counting risk:** A commodity unit deducted in-kind is removed from the harvest bundle (line 154 of `payment_in_kind.rs`), so it cannot also be sold B2C. However, the B2C demand netting (Step 15) must be carefully implemented to ensure the same unit is not counted as both imputed consumption AND cash consumption in GDP. The netting must happen BEFORE B2C clearing.

13. **Housing ownership & rent double-entry — DEFLATIONARY BLACK HOLE RISK:** Every `HousingBuilding` MUST have a valid `owner` ID. If housing is owned by a generic or missing ID, citizen rent payments will vanish, creating a massive deflationary black hole where money is debited from citizen savings but never credited to any entity. The owner mapping (Part 2.5, Step 5 of the Blueprint) must be strictly followed:
    - `Palace`, `FolwarkHousing`, `SocialHousing` → `"STATE:<country_id>"` (rent to treasury)
    - `Hut` → `"CLASS:Aristocracy:<region_id>"` (rent to aristocracy savings)
    - `Tenement`, `CityPalace`, `Familok`, `Beamciok` → `"CLASS:Bourgeoisie:<region_id>"` (rent to bourgeoisie savings)
    The residential rent collection step (new, added to `turn.rs`) must debit the occupying class's savings and credit the owner entity by the exact same amount. No money creation or destruction.

---

## Verification Checklist

- [ ] `cargo build` succeeds with no errors
- [ ] `cargo test --lib -- --test-threads=1` — all tests pass
- [ ] Market UI shows non-zero Supply and Demand for Cereal, Vegetable, Protein, Meat, Clothing
- [ ] Market UI shows non-zero Supply for Fuels, Steel, Chemicals, Glass, Cement
- [ ] 1900 scenario: no Electronics, Plastics, Semiconductors, Software production
- [ ] 1975 scenario: Electronics, Plastics, and advanced methods are available
- [ ] Every mined commodity has a corresponding processing plant
- [ ] B2C clearing produces non-zero revenue for Cereal, Vegetable, Protein, Meat, Clothing
- [ ] No mining company exists without a `deposit_id` (except fallback coal mines)
- [ ] No negative bank reserves or FX reserves (Phase 43 invariant preserved)
- [ ] Agricultural companies are proportional to `arable_land_max`, not just population
- [ ] Regions with zero arable land have EXACTLY zero agricultural companies (no fallback farms)
- [ ] 1900 scenario: >70% rural population, Serfs present
- [ ] 1975 scenario: >50% urban population, no Serfs
- [ ] Housing capacity ≥ 90% of population in all regions at world start
- [ ] Housing `occupied_slots` > 0 for all spawned housing buildings
- [ ] No immediate Turn 1 homelessness crisis (housing shortage < 20%)
- [ ] Housing buildings per region ≤ 20 (Mega-Estate consolidation)
- [ ] Each `HousingBuilding.total_capacity` ≥ 10,000 (Mega-Estate scale)
- [ ] Every `HousingBuilding.owner` is a valid non-empty string (no deflationary black hole)
- [ ] `Palace`/`FolwarkHousing`/`SocialHousing` owners = `"STATE:<country_id>"`
- [ ] `Hut` owners = `"CLASS:Aristocracy:<region_id>"`
- [ ] `Tenement`/`CityPalace`/`Familok`/`Beamciok` owners = `"CLASS:Bourgeoisie:<region_id>"`
- [ ] Residential rent debited from occupying class savings and credited to owner entity (double-entry)
- [ ] Rent collection: total debited == total credited (no money creation/destruction)
- [ ] 1900 scenario: `GdpBreakdown.imputed_consumption > 0` (Serf subsistence valued)
- [ ] 1975 scenario: `GdpBreakdown.imputed_consumption ≈ 0` (no Serfs, fully monetized)
- [ ] `official_gdp = consumption + imputed_consumption + government_spending + investment + net_exports`
- [ ] No double-counting: in-kind deductions reduce B2C demand before clearing
- [ ] Old save files with Polish JSON keys load correctly via `#[serde(alias)]`
- [ ] New save files use English JSON keys
- [ ] All Rust code references use English field names (no `ziemia_orna_max` in `.rs` files)
