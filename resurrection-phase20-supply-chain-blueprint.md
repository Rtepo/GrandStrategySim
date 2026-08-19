# Resurrection Phase 20 — Supply Chain & Generator Overhaul Blueprint

**Status:** BLUEPRINT ONLY — awaiting user approval before any implementation
**Date:** 2026-08-11
**Prerequisite:** `resurrection-supply-chain-audit.md` (approved)
**Goal:** Eliminate all orphan inputs/outputs, activate a modern 8-layer supply chain, fix the generator to guarantee a minimum viable economy at world birth, and ensure Phase 19 Generative Blueprints have real material flows to work with.

> **Architectural mandate (user-confirmed):**
> 1. Nothing is created out of thin air — every commodity must trace back to a primary extraction source.
> 2. All code keys, struct fields, and method names are in English.
> 3. The generator MUST pull from the registry, not invent parallel recipes.
> 4. Every building at world birth starts with a fixed-asset cohort and one turn of inventory.

---

## Table of Contents

1. [Phase 20.1 — Enums & Commodities Overhaul](#phase-201--enums--commodities-overhaul)
2. [Phase 20.2 — The Great Registry Rewrite](#phase-202--the-great-registry-rewrite)
3. [Phase 20.3 — B2C Consumption & Retail Integration](#phase-203--b2c-consumption--retail-integration)
4. [Phase 20.4 — Generator Bootstrapping Rewrite](#phase-204--generator-bootstrapping-rewrite)
5. [Phase 20.5 — Blueprint Spec Updates](#phase-205--blueprint-spec-updates)
6. [Phase 20.6 — Validation & Testing](#phase-206--validation--testing)
7. [Implementation Order & Dependencies](#implementation-order--dependencies)

---

## Phase 20.1 — Enums & Commodities Overhaul

**File:** `state/src/registries/enums.rs`

### 20.1.1 — Add 6 New Commodities

Insert these new variants into the `Commodity` enum, in alphabetical position, with `#[serde(rename_all = "snake_case")]` producing the JSON key shown:

| Variant | JSON key | Category | Purpose |
|---------|----------|----------|---------|
| `Batteries` | `batteries` | Intermediate | Energy storage; input for EVs, electronics, grid storage |
| `Lithium` | `lithium` | Raw Material | Battery feedstock; mined from brines/hard rock |
| `Plastics` | `plastics` | Intermediate | Oil-derived polymer; input for Agd, Cars, packaging |
| `RareEarthElements` | `rare_earth_elements` | Raw Material | Neodymium, dysprosium etc.; input for semiconductors, magnets |
| `RefinedFuel` | `refined_fuel` | Intermediate | High-grade distillate from crude oil; distinct from generic `Fuels` |
| `Semiconductors` | `semiconductors` | Intermediate | Silicon-based ICs; input for ElectronicComponents, solar, advanced machinery |

Each variant requires:
- A doc comment `/// "json_key" — description.`
- Addition to the `Commodity::all()` array (currently `[Commodity; 132]` → becomes `[Commodity; 136]`)

### 20.1.2 — Fix Naming Mismatches

The generator produces `Grains` and `Vegetables` (plural) but the consumption registry and agriculture registry methods use `Cereal` and `Vegetable` (singular). These are distinct enum variants that are invisible to each other in the market.

**Action:** Remove the `Grains` and `Vegetables` variants from the `Commodity` enum entirely. All code that previously produced or consumed them will be rewritten in Phase 20.2 and 20.4 to use `Cereal` and `Vegetable` instead.

| Remove | Replace With | Affected Code |
|--------|-------------|---------------|
| `Commodity::Grains` | `Commodity::Cereal` | Generator Agriculture recipe (Phase 20.4 rewrite) |
| `Commodity::Vegetables` | `Commodity::Vegetable` | Generator Agriculture recipe (Phase 20.4 rewrite) |

**Migration:** The `Commodity::all()` array shrinks by 2, then grows by 6 net (+4). Update the array size from `[Commodity; 132]` to `[Commodity; 136]`.

### 20.1.3 — Update `is_fixed_asset()` and `is_quality_durable()`

The current `is_fixed_asset()` returns true for `{IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks, Cars}`. No change needed here — the set is correct.

The current `is_quality_durable()` returns true for `{Cars, Agd, Televisions, Radio, Furniture, LuxuryFurniture, Clothing, LuxuryClothing}`. No change needed.

`is_blueprint_eligible()` calls both — no change needed.

### 20.1.4 — Commodity Count Update

After all changes:
- Remove 2: `Grains`, `Vegetables`
- Add 6: `Batteries`, `Lithium`, `Plastics`, `RareEarthElements`, `RefinedFuel`, `Semiconductors`
- Net: 132 − 2 + 6 = **136 variants**
- Update `Commodity::all()` return type to `[Commodity; 136]`
- Add legacy aliases `"grains"` → `Cereal` and `"vegetables"` → `Vegetable` in `TryFrom<&str>` for save migration

---

## Phase 20.2 — The Great Registry Rewrite

This is the largest section. It covers two files:
- `state/src/registries/production_methods_data.rs` — production methods for all 21 sectors
- `state/src/registries/tech_tree_data.rs` — technologies that unlock advanced methods

### 20.2.1 — Production Methods Rewrite Strategy

**Current state:** `default_production_methods()` covers only 13 sectors (mining, agriculture, heavy_industry, light_industry, armaments, construction, energy, transport, media, medical, education, public_services, maintenance_workshops). The generator covers 10 sectors with its own parallel recipes.

**Target state:** `default_production_methods()` covers ALL 21 sectors with multiple era-gated methods per sector. The generator will call `best_registry_method(sector, start_year)` to select the appropriate method.

**Design rules for every production method:**
1. Each sector gets at least 3 era-gated Production-slot methods (early/mid/late).
2. The earliest method in each sector has `required_tech: None` and a year ≤ 1880 so it is always available.
3. Every commodity that appears as an input must be produced by some other method in the registry (or be a free natural resource: Water, Air).
4. Every commodity that appears as an output must be consumed by some other method, construction BOM, or B2C demand.
5. Labor ratios (experts + skilled + basic) must sum to 1.0.
6. Per-1000-worker quantities are positive floats.
7. Fixed-asset commodities in inputs are NOT consumed per-turn (handled by Phase 19B fixed-asset system) — they represent the machinery needed to operate, not material consumed.

### 20.2.2 — Complete Sector-by-Sector Production Method Definitions

Below is the full specification for every sector. Each method is shown as:
```
"Method Name" (year, tech_id?, experts/skilled/basic, efficiency)
  inputs: [(Commodity, qty_per_1k_workers), ...]
  outputs: [(Commodity, qty_per_1k_workers), ...]
```

---

#### Sector: `mining` (Layer 0 — Primary Extraction)

**Existing methods to KEEP (already in registry):**
- "Manual Mining" (1880, None) → HardCoal 10.0 | inputs: Fuels 2.0, Food 5.0
- "Pneumatic Drilling" (1885, mining_002) → HardCoal 15.0 | inputs: Fuels 5.0, Food 5.0, MechanicalComponents 2.0
- "Electric Mine Pumps" (1890, mining_004) → HardCoal 18.0 | inputs: Energy 5.0, Fuels 3.0
- "Longwall Mining" (1895, mining_006) → HardCoal 25.0 | inputs: Energy 8.0, Fuels 4.0, MechanicalComponents 3.0
- "Froth Flotation" (1900, mining_007) → Copper 12.0 | inputs: Energy 10.0, Chemicals 5.0
- "Open-Pit Mining" (1905, mining_008) → HardCoal 40.0 | inputs: Fuels 15.0, Energy 10.0
- "Mechanized Longwall" (1950, auto3_001) → HardCoal 60.0 | inputs: Energy 20.0, Fuels 10.0, MechanicalComponents 8.0
- "CNC Mining" (1970, auto3_004) → HardCoal 80.0 | inputs: Energy 25.0, Fuels 8.0, ElectronicComponents 5.0

**NEW methods to ADD (activate dead commodities, add modern resources):**

```
"Iron Ore Mining" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Iron, 12.0)]

"Copper Ore Mining" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Copper, 8.0)]

"Oil Drilling" (1880, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Fuels, 5.0), (Food, 5.0), (MechanicalComponents, 2.0)]
  outputs: [(Oil, 30.0)]

"Natural Gas Extraction" (1900, None, 0.08/0.25/0.67, 1.2)
  inputs: [(Fuels, 3.0), (Energy, 3.0)]
  outputs: [(NaturalGas, 25.0)]

"Bauxite Mining" (1890, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Bauxite, 15.0)]

"Sand And Gravel Quarry" (1880, None, 0.03/0.15/0.82, 1.0)
  inputs: [(Fuels, 2.0), (Food, 3.0)]
  outputs: [(Sand, 20.0), (Gravel, 15.0)]

"Stone Quarrying" (1880, None, 0.03/0.15/0.82, 1.0)
  inputs: [(Fuels, 2.0), (Food, 3.0)]
  outputs: [(Stone, 25.0)]

"Clay Mining" (1880, None, 0.03/0.15/0.82, 1.0)
  inputs: [(Fuels, 2.0), (Food, 3.0)]
  outputs: [(Clay, 20.0)]

"Limestone Quarrying" (1880, None, 0.03/0.15/0.82, 1.0)
  inputs: [(Fuels, 2.0), (Food, 3.0)]
  outputs: [(Limestone, 22.0)]

"Sulfur Mining" (1890, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Energy, 2.0)]
  outputs: [(Sulfur, 10.0)]

"Salt Mining" (1880, None, 0.03/0.15/0.82, 1.0)
  inputs: [(Fuels, 2.0), (Food, 3.0)]
  outputs: [(Salt, 18.0)]

"Tin Ore Mining" (1890, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Tin, 8.0)]

"Zinc Ore Mining" (1890, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Zinc, 8.0)]

"Lead Ore Mining" (1890, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 3.0), (Food, 5.0)]
  outputs: [(Lead, 8.0)]

"Silver Mining" (1890, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Fuels, 5.0), (Energy, 3.0), (Chemicals, 2.0)]
  outputs: [(Silver, 3.0)]

"Gold Mining" (1890, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Fuels, 5.0), (Energy, 3.0), (Chemicals, 3.0)]
  outputs: [(Gold, 2.0)]

"Peat Cutting" (1880, None, 0.02/0.10/0.88, 0.8)
  inputs: [(Food, 3.0)]
  outputs: [(Peat, 15.0)]

"Brown Coal Mining" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Fuels, 2.0), (Food, 5.0)]
  outputs: [(BrownCoal, 18.0)]

"Rare Earth Element Mining" (1965, rare_001, 0.15/0.35/0.50, 2.0)
  inputs: [(Energy, 15.0), (Chemicals, 8.0), (Fuels, 5.0)]
  outputs: [(RareEarthElements, 5.0)]

"Lithium Extraction" (1970, lithium_001, 0.12/0.30/0.58, 1.5)
  inputs: [(Energy, 10.0), (Water, 15.0), (Fuels, 3.0)]
  outputs: [(Lithium, 8.0)]
```

**Automation and Organization slots:** Keep existing automation/organization methods. Add new ones for modern eras as needed (GPS-Guided Mining, Automated Drone Surveying, etc.)

**Key invariant:** Every mining output traces to a consumer. Iron → Steel. Copper → ElectronicComponents. Oil → Fuels/Plastics/RefinedFuel. Bauxite → Aluminum. Sand → Glass/Silicon. Stone → Construction. Clay → Bricks/Cement. Limestone → Cement. Etc.

---

#### Sector: `agriculture` (Layer 0 — Primary Production)

**Existing methods to KEEP:**
- "Manual Farming" (1880, None) → Cereal 15.0 | inputs: Seeds 5.0, Food 3.0
- All other existing Cereal/Vegetable methods through "Precision Farming" and "Hydroponics"

**NEW methods to ADD:**

```
"Vegetable Farming" (1880, None, 0.02/0.10/0.88, 1.0)
  inputs: [(Seeds, 5.0), (Water, 8.0), (Food, 2.0)]
  outputs: [(Vegetable, 18.0)]

"Protein Farming" (1880, None, 0.03/0.12/0.85, 1.0)
  inputs: [(Seeds, 6.0), (Water, 10.0), (Food, 2.0)]
  outputs: [(Protein, 12.0)]

"Orchard Cultivation" (1885, None, 0.03/0.12/0.85, 1.1)
  inputs: [(Seeds, 4.0), (Water, 8.0), (Food, 2.0)]
  outputs: [(Fruit, 15.0)]

"Livestock Ranching" (1880, None, 0.03/0.12/0.85, 1.0)
  inputs: [(Fodder, 15.0), (Water, 10.0), (Food, 3.0)]
  outputs: [(Meat, 10.0), (Livestock, 5.0)]

"Industrial Fiber Farming" (1880, None, 0.03/0.12/0.85, 1.0)
  inputs: [(Seeds, 5.0), (Water, 8.0)]
  outputs: [(IndustrialFiber, 12.0)]

" Luxury Crop Plantation" (1885, None, 0.05/0.15/0.80, 1.2)
  inputs: [(Seeds, 4.0), (Water, 10.0), (Food, 2.0)]
  outputs: [(Luxury, 8.0)]

"Seed Production" (1880, None, 0.05/0.20/0.75, 0.8)
  inputs: [(Cereal, 10.0), (Water, 5.0), (Food, 2.0)]
  outputs: [(Seeds, 12.0)]

"Fodder Production" (1880, None, 0.03/0.12/0.85, 1.0)
  inputs: [(Cereal, 8.0), (Water, 5.0)]
  outputs: [(Fodder, 15.0)]

"Timber Plantation" (1880, None, 0.02/0.10/0.88, 0.7)
  inputs: [(Seeds, 2.0), (Water, 5.0)]
  outputs: [(Timber, 10.0)]
```

**Key invariant:** Seeds are produced from Cereal (seed production method). Fodder is produced from Cereal. Livestock consumes Fodder. Vegetable/Protein/Fruit have direct B2C demand. IndustrialFiber → Fibers (light industry). Timber → Construction/Planks.

---

#### Sector: `heavy_industry` (Layers 1–3 — Smelting, Chemicals, Components, Machinery)

This is the most critical sector. It must produce:
- Layer 1: Steel, Cement, Bricks, Glass, Aluminum, Silicon, Coke
- Layer 2: Chemicals, SodaAsh, Ammonia, Fertilizers, Plastics, RefinedFuel, Fuels, Bitumen, Asphalt, Catalysts, Hydrogen
- Layer 3: MechanicalComponents, ElectronicComponents, Semiconductors, Software, Batteries
- Layer 5: IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks, Cars

**Existing methods to KEEP:**
- "Bessemer Converters" (1880, steel_001) → Steel 15.0 | inputs: Iron 20.0, Fuels 10.0
- "Open-Hearth Furnaces" (1885, steel_002) → Steel 22.0 | inputs: Iron 25.0, Fuels 12.0
- "Electric Arc Furnaces" (1905, steel_008) → Steel 30.0 | inputs: Iron 20.0, Energy 15.0
- "Basic Oxygen Process" (1955, auto3_002) → Steel 50.0 | inputs: Iron 30.0, Energy 10.0
- "Continuous Casting" (1965, auto3_005) → Steel 70.0 | inputs: Iron 35.0, Energy 15.0, ElectronicComponents 3.0
- "Mini-Mill Production" (1975, auto3_007) → Steel 90.0 | inputs: Energy 25.0, ElectronicComponents 5.0
- "Electrified Factories" (1910, elecf_001) → IndustrialMachinery 15.0 | inputs: Energy 20.0, Steel 10.0
- "CNC Manufacturing" (1970, auto3_004) → IndustrialMachinery 30.0 | inputs: Energy 20.0, Steel 15.0, ElectronicComponents 8.0
- All existing automation/organization methods

**NEW Layer 1 methods (Smelting & Basic Processing):**

```
"Coke Production" (1880, None, 0.08/0.25/0.67, 1.0)
  inputs: [(HardCoal, 20.0), (Energy, 5.0)]
  outputs: [(Coke, 15.0)]

"Cement Production" (1880, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Limestone, 25.0), (Clay, 8.0), (Energy, 10.0)]
  outputs: [(Cement, 30.0)]

"Brick Making" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Clay, 20.0), (Energy, 5.0)]
  outputs: [(Bricks, 25.0)]

"Glass Making" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Sand, 20.0), (SodaAsh, 5.0), (Energy, 12.0)]
  outputs: [(Glass, 18.0)]

"Aluminum Smelting" (1900, metall_006, 0.15/0.35/0.50, 1.5)
  inputs: [(Bauxite, 20.0), (Energy, 30.0), (Catalysts, 2.0)]
  outputs: [(Aluminum, 12.0)]

"Silicon Purification" (1950, semi_001, 0.20/0.40/0.40, 2.0)
  inputs: [(Sand, 15.0), (Energy, 20.0), (Chemicals, 5.0)]
  outputs: [(Silicon, 8.0)]
```

**NEW Layer 2 methods (Chemical & Petroleum Processing):**

```
"Basic Chemical Production" (1880, None, 0.10/0.30/0.60, 1.0)
  inputs: [(Sulfur, 8.0), (Salt, 5.0), (Water, 10.0), (Energy, 8.0)]
  outputs: [(Chemicals, 15.0)]

"Solvay Process" (1880, None, 0.12/0.30/0.58, 1.0)
  inputs: [(Salt, 10.0), (Limestone, 8.0), (Ammonia, 3.0), (Energy, 8.0)]
  outputs: [(SodaAsh, 12.0)]

"Haber-Bosch Process" (1910, chem_002, 0.15/0.35/0.50, 1.5)
  inputs: [(NaturalGas, 10.0), (Energy, 12.0), (Catalysts, 1.0)]
  outputs: [(Ammonia, 10.0)]

"Fertilizer Production" (1880, None, 0.10/0.30/0.60, 1.0)
  inputs: [(Ammonia, 8.0), (Chemicals, 5.0), (Energy, 5.0)]
  outputs: [(Fertilizers, 18.0)]

"Oil Refining" (1880, None, 0.10/0.30/0.60, 1.0)
  inputs: [(Oil, 25.0), (Energy, 5.0), (Catalysts, 1.0)]
  outputs: [(Fuels, 18.0), (Bitumen, 3.0)]

"Advanced Refining" (1920, petrol_001, 0.12/0.32/0.56, 1.8)
  inputs: [(Oil, 30.0), (Catalysts, 2.0), (Energy, 8.0)]
  outputs: [(Fuels, 22.0), (RefinedFuel, 8.0), (Bitumen, 4.0)]

"Plastics Production" (1935, petrol_003, 0.15/0.35/0.50, 2.0)
  inputs: [(Oil, 15.0), (Chemicals, 8.0), (Energy, 10.0)]
  outputs: [(Plastics, 20.0)]

"Asphalt Production" (1900, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Bitumen, 8.0), (Sand, 10.0), (Gravel, 8.0), (Energy, 3.0)]
  outputs: [(Asphalt, 20.0)]

"Catalyst Production" (1900, None, 0.12/0.30/0.58, 1.0)
  inputs: [(Chemicals, 8.0), (RareEarthElements, 1.0), (Energy, 5.0)]
  outputs: [(Catalysts, 6.0)]

"Hydrogen Production" (1970, hydro_001, 0.15/0.35/0.50, 1.5)
  inputs: [(NaturalGas, 8.0), (Energy, 15.0)]
  outputs: [(Hydrogen, 6.0)]
```

**NEW Layer 3 methods (Components & Parts):**

```
"Mechanical Components Workshop" (1880, None, 0.10/0.30/0.60, 1.0)
  inputs: [(Steel, 10.0), (Energy, 5.0), (IndustrialMachinery, 2.0)]
  outputs: [(MechanicalComponents, 15.0)]

"Precision Machining" (1910, mech_008, 0.15/0.35/0.50, 1.8)
  inputs: [(Steel, 12.0), (Energy, 8.0), (IndustrialMachinery, 3.0)]
  outputs: [(MechanicalComponents, 25.0)]

"Electronic Components Assembly" (1920, elecf_001, 0.15/0.35/0.50, 1.5)
  inputs: [(Copper, 8.0), (Tin, 3.0), (Energy, 8.0), (IndustrialMachinery, 2.0)]
  outputs: [(ElectronicComponents, 10.0)]

"Semiconductor Fabrication" (1970, semi_003, 0.25/0.45/0.30, 3.0)
  inputs: [(Silicon, 5.0), (RareEarthElements, 2.0), (Chemicals, 5.0), (Energy, 15.0)]
  outputs: [(Semiconductors, 8.0)]

"Advanced Electronics" (1980, semi_005, 0.25/0.45/0.30, 3.5)
  inputs: [(Semiconductors, 3.0), (Copper, 5.0), (Tin, 2.0), (Energy, 10.0)]
  outputs: [(ElectronicComponents, 20.0)]

"Software Development" (1980, cs_005, 0.35/0.45/0.20, 2.5)
  inputs: [(ElectronicComponents, 3.0), (Energy, 5.0), (Food, 5.0)]
  outputs: [(Software, 15.0)]

"Battery Production" (1990, batt_001, 0.20/0.40/0.40, 2.0)
  inputs: [(Lithium, 5.0), (Lead, 5.0), (Semiconductors, 2.0), (Energy, 10.0)]
  outputs: [(Batteries, 8.0)]
```

**NEW Layer 5 methods (Investment Goods — THE CRITICAL GAP):**

```
// ── IndustrialMachinery ──
"Machine Shop" (1880, None, 0.12/0.30/0.58, 1.0)
  inputs: [(Steel, 12.0), (MechanicalComponents, 5.0), (Energy, 8.0)]
  outputs: [(IndustrialMachinery, 10.0)]

// (existing "Electrified Factories" and "CNC Manufacturing" stay)

"Smart Manufacturing" (1995, advman_006, 0.30/0.45/0.25, 5.0)
  inputs: [(Steel, 15.0), (ElectronicComponents, 8.0), (Software, 5.0), (Semiconductors, 2.0), (Energy, 15.0)]
  outputs: [(IndustrialMachinery, 50.0)]

// ── ConstructionMachinery (ALL NEW) ──
"Blacksmith Workshop" (1880, None, 0.10/0.25/0.65, 1.0)
  inputs: [(Steel, 10.0), (Iron, 5.0), (Fuels, 5.0)]
  outputs: [(ConstructionMachinery, 8.0)]

"Machine Factory" (1910, mech_008, 0.15/0.35/0.50, 1.8)
  inputs: [(Steel, 15.0), (MechanicalComponents, 8.0), (Energy, 8.0)]
  outputs: [(ConstructionMachinery, 20.0)]

"Heavy Equipment Plant" (1950, auto3_001, 0.20/0.40/0.40, 3.0)
  inputs: [(Steel, 20.0), (MechanicalComponents, 10.0), (ElectronicComponents, 3.0), (Energy, 12.0)]
  outputs: [(ConstructionMachinery, 40.0)]

"Automated Equipment Plant" (1990, advman_004, 0.25/0.45/0.30, 5.0)
  inputs: [(Steel, 18.0), (MechanicalComponents, 8.0), (ElectronicComponents, 8.0), (Software, 3.0), (Energy, 15.0)]
  outputs: [(ConstructionMachinery, 70.0)]

// ── AgriculturalMachinery (ALL NEW) ──
"Implement Workshop" (1880, None, 0.10/0.25/0.65, 1.0)
  inputs: [(Steel, 10.0), (Iron, 5.0), (Fuels, 3.0)]
  outputs: [(AgriculturalMachinery, 8.0)]

"Implement Factory" (1910, mech_008, 0.15/0.35/0.50, 1.8)
  inputs: [(Steel, 15.0), (MechanicalComponents, 8.0), (Energy, 8.0)]
  outputs: [(AgriculturalMachinery, 20.0)]

"Tractor Plant" (1950, auto3_001, 0.20/0.40/0.40, 3.0)
  inputs: [(Steel, 20.0), (MechanicalComponents, 10.0), (ElectronicComponents, 3.0), (Energy, 12.0)]
  outputs: [(AgriculturalMachinery, 40.0)]

"Precision Ag Equipment" (1990, advman_004, 0.25/0.45/0.30, 5.0)
  inputs: [(Steel, 18.0), (MechanicalComponents, 8.0), (ElectronicComponents, 8.0), (Software, 3.0), (Energy, 15.0)]
  outputs: [(AgriculturalMachinery, 70.0)]

// ── OfficeMachinery (ALL NEW) ──
"Typewriter Workshop" (1890, mech_008, 0.15/0.35/0.50, 1.0)
  inputs: [(Steel, 8.0), (MechanicalComponents, 5.0), (Energy, 3.0)]
  outputs: [(OfficeMachinery, 10.0)]

"Office Equipment Factory" (1950, auto3_001, 0.20/0.40/0.40, 2.5)
  inputs: [(Steel, 10.0), (MechanicalComponents, 8.0), (ElectronicComponents, 3.0), (Energy, 8.0)]
  outputs: [(OfficeMachinery, 25.0)]

"Computer Factory" (1980, auto3_004, 0.25/0.45/0.30, 4.0)
  inputs: [(Steel, 5.0), (ElectronicComponents, 10.0), (Semiconductors, 3.0), (Software, 3.0), (Energy, 8.0)]
  outputs: [(OfficeMachinery, 50.0)]

// ── Trucks (ALL NEW) ──
"Wagon Workshop" (1880, None, 0.10/0.25/0.65, 1.0)
  inputs: [(Steel, 8.0), (Timber, 5.0), (MechanicalComponents, 3.0), (Fuels, 2.0)]
  outputs: [(Trucks, 5.0)]

"Truck Assembly" (1920, auto_001, 0.15/0.35/0.50, 2.0)
  inputs: [(Steel, 15.0), (MechanicalComponents, 8.0), (Fuels, 3.0), (Energy, 5.0)]
  outputs: [(Trucks, 15.0)]

"Modern Truck Plant" (1960, auto3_002, 0.20/0.40/0.40, 3.5)
  inputs: [(Steel, 18.0), (MechanicalComponents, 10.0), (ElectronicComponents, 5.0), (Fuels, 5.0), (Energy, 8.0)]
  outputs: [(Trucks, 35.0)]

"Electric Truck Plant" (2000, advman_006, 0.25/0.45/0.30, 5.0)
  inputs: [(Steel, 15.0), (Aluminum, 5.0), (ElectronicComponents, 8.0), (Batteries, 5.0), (Energy, 10.0)]
  outputs: [(Trucks, 60.0)]

// ── Cars (ALL NEW) ──
"Coachbuilder" (1900, mech_008, 0.12/0.30/0.58, 1.0)
  inputs: [(Steel, 10.0), (Timber, 5.0), (MechanicalComponents, 5.0), (Fuels, 2.0)]
  outputs: [(Cars, 5.0)]

"Assembly Line" (1913, auto_001, 0.10/0.30/0.60, 2.0)
  inputs: [(Steel, 15.0), (MechanicalComponents, 8.0), (Fuels, 3.0), (Energy, 5.0)]
  outputs: [(Cars, 20.0)]

"Modern Auto Plant" (1960, auto3_003, 0.20/0.40/0.40, 3.5)
  inputs: [(Steel, 18.0), (MechanicalComponents, 8.0), (ElectronicComponents, 5.0), (Plastics, 5.0), (Fuels, 3.0), (Energy, 8.0)]
  outputs: [(Cars, 50.0)]

"EV Factory" (2010, advman_006, 0.25/0.45/0.30, 5.0)
  inputs: [(Steel, 12.0), (Aluminum, 8.0), (ElectronicComponents, 10.0), (Semiconductors, 5.0), (Batteries, 8.0), (Energy, 10.0)]
  outputs: [(Cars, 80.0)]
```

---

#### Sector: `light_industry` (Layer 6 — Consumer Goods)

**Existing methods to KEEP:**
- All existing clothing methods (Handloom Weaving through Fast Fashion)
- All existing automation/organization methods

**NEW methods to ADD:**

```
"Sawmill" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Timber, 15.0), (Energy, 3.0)]
  outputs: [(Planks, 12.0)]

"Furniture Workshop" (1880, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Planks, 12.0), (Steel, 3.0), (Energy, 3.0)]
  outputs: [(Furniture, 10.0)]

"Luxury Furniture Workshop" (1880, None, 0.12/0.30/0.58, 1.2)
  inputs: [(Planks, 10.0), (Luxury, 3.0), (Gold, 1.0), (Energy, 5.0)]
  outputs: [(LuxuryFurniture, 5.0)]

"Paper Mill" (1880, None, 0.08/0.25/0.67, 1.0)
  inputs: [(Timber, 15.0), (Chemicals, 3.0), (Water, 10.0), (Energy, 8.0)]
  outputs: [(Paper, 18.0)]

"Appliance Assembly" (1935, elecf_005, 0.15/0.35/0.50, 2.0)
  inputs: [(Steel, 8.0), (ElectronicComponents, 5.0), (Plastics, 3.0), (Energy, 5.0)]
  outputs: [(Agd, 12.0)]

"Food Processing" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Cereal, 10.0), (Vegetable, 5.0), (Protein, 3.0), (Energy, 3.0)]
  outputs: [(Food, 18.0)]

"Textile Mill" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(IndustrialFiber, 12.0), (Energy, 3.0)]
  outputs: [(Fibers, 15.0)]

"Synthetic Fiber Production" (1935, synth_006, 0.15/0.35/0.50, 2.0)
  inputs: [(Plastics, 10.0), (Chemicals, 3.0), (Energy, 5.0)]
  outputs: [(Fibers, 20.0)]
```

---

#### Sector: `armaments_industry` (Layer 7 — Military Goods)

**Existing methods to KEEP:** All existing armaments methods.

**No new methods needed** — the armaments sector already has proper methods. The fix is in the generator (Phase 20.4) which must seed this sector with a proper recipe.

---

#### Sector: `construction` (Layer 8 — Services)

**Existing methods to KEEP:** All existing construction methods.

**No new methods needed** — construction methods are well-defined. The fix is that `ConstructionMachinery` now has producers (heavy_industry), so construction BOM inputs are no longer orphaned.

---

#### Sector: `energy` (Layer 8 — Services)

**Existing methods to KEEP:** All existing energy methods (Coal-Fired Boilers through Wind Turbine Farm).

**NEW methods to ADD:**

```
"Water Utility" (1880, None, 0.05/0.20/0.75, 1.0)
  inputs: [(Energy, 3.0), (Chemicals, 1.0)]
  outputs: [(Water, 50.0)]

"Geothermal Plant" (1980, advman_004, 0.25/0.40/0.35, 4.0)
  inputs: [(MechanicalComponents, 8.0), (ElectronicComponents, 3.0), (Water, 5.0)]
  outputs: [(Energy, 100.0)]

"Battery Storage Facility" (2000, batt_003, 0.20/0.40/0.40, 3.0)
  inputs: [(Batteries, 5.0), (ElectronicComponents, 3.0), (Energy, 10.0)]
  outputs: [(Energy, 80.0)]
```

---

#### Sector: `transport_logistics` (Layer 8 — Services)

**Existing methods to KEEP:** All existing transport methods.

**No new methods needed** — transport methods produce PassengerTransport which is consumed by the transport service system.

---

#### Sectors: `media_and_entertainment`, `medical_services`, `educational_services`, `public_services`, `maintenance_workshops`

**Existing methods to KEEP:** All existing methods in these sectors.

**No new methods needed** — these sectors already have proper production methods. The fix is in the generator (Phase 20.4) which must seed these sectors.

---

#### Sectors: `local_services`, `export_services`, `hospitality`, `banking`, `public_administration`, `ngo`, `religion`, `waste_management`

These sectors produce services (not physical commodities) and are handled by their respective subsystems. The generator must seed them with appropriate recipes (Phase 20.4), but no new registry methods are needed for most. Where service commodities are produced (LocalServicesCommodity, BankingServices, AdministrativeServices), the generator will use `best_registry_method()` to pull from the registry.

---

### 20.2.3 — Technology Tree Additions

**File:** `state/src/registries/tech_tree_data.rs`

Add new technology nodes to unlock the advanced production methods defined above. Each tech node uses the `tech()` helper function.

**New Era 3 (1945–1980) Commercial techs:**

```
tech("rare_001", "Rare Earth Extraction", 1965, 200,
    "Separation of rare earth elements from mineral ores.",
    Commercial, &["metall_007", "chem_005"], &[]);

tech("lithium_001", "Lithium Extraction", 1970, 180,
    "Brine and hard-rock lithium processing for batteries.",
    Commercial, &["chem_005"], &[]);

tech("semi_001", "Silicon Purification", 1950, 160,
    "Zone refining and Czochralski crystal growth for semiconductor-grade silicon.",
    Commercial, &["metall_007", "electr_006"], &[]);

tech("semi_003", "Semiconductor Fabrication", 1970, 200,
    "Photolithography and doping for integrated circuit manufacturing.",
    Commercial, &["semi_001", "solid_006"], &[]);

tech("semi_005", "VLSI Design", 1980, 220,
    "Very large scale integration for microprocessor manufacturing.",
    Commercial, &["semi_003", "cs_005"], &[]);

tech("batt_001", "Lithium Battery Production", 1990, 180,
    "Rechargeable lithium-ion battery manufacturing.",
    Commercial, &["lithium_001", "semi_003"], &[]);

tech("batt_003", "Grid Energy Storage", 2000, 200,
    "Utility-scale battery storage for grid stabilization.",
    Commercial, &["batt_001", "auto3_007"], &[]);

tech("petrol_001", "Catalytic Cracking", 1920, 120,
    "Catalytic cracking of crude oil for higher fuel yields.",
    Commercial, &["chem_002"], &[]);

tech("petrol_003", "Petrochemical Processing", 1935, 140,
    "Cracking and polymerization for plastics production.",
    Commercial, &["petrol_001", "chem_003"], &[]);

tech("hydro_001", "Hydrogen Production", 1970, 160,
    "Steam methane reforming and electrolytic hydrogen production.",
    Commercial, &["chem_005", "elecf_008"], &[]);
```

**Linkage:** Each tech's `unlocks_methods` field must reference the sector key and method name that it unlocks. For example, `semi_003` unlocks `("heavy_industry", &[("production", "Semiconductor Fabrication")])`.

**Important:** The `unlocks_methods` field in the tech tree uses English snake_case sector keys matching `Sector` serde rename. Method names must exactly match the names used in `production_methods_data.rs`.

---

## Phase 20.3 — B2C Consumption & Retail Integration

**File:** `state/src/data/consumption_registry.rs`

### 20.3.1 — Current State

The consumption registry currently has baskets for 5 classes:
- `"Serf"` — subsistence only (Cereal, Vegetable, Protein, HealthCapacity)
- `"FreePeasant"` — subsistence + standard (adds Clothing, Furniture, EducationSlots)
- `"LandlessLaborer"` — subsistence + standard (same as FreePeasant, lower quantities)
- `"Aristocracy"` — subsistence + standard + luxury (adds Luxury, OfficeMachinery)
- `"Worker"` — subsistence + standard

**Missing from all baskets:** Televisions, Radio, Agd, Cars, LuxuryClothing, LuxuryFurniture, Meat, Fruit, Information

### 20.3.2 — Wealth-Tier Demand Assignment

The `WealthBracket` enum has four variants: `VeryHigh`, `High`, `Medium`, `Low`. The `NeedTier` enum has: `Subsistence`, `Standard`, `Luxury`. The mapping principle:

| NeedTier | WealthBracket | Demand Pattern |
|----------|--------------|----------------|
| Subsistence | All (Low → VeryHigh) | Food, basic clothing, health, education |
| Standard | Medium → VeryHigh | Furniture, Agd, Radio, basic Cars, Televisions |
| Luxury | High → VeryHigh | LuxuryFurniture, LuxuryClothing, premium Cars, Meat, Fruit |

### 20.3.3 — Updated Consumption Baskets

**`"Serf"` (rural, lowest wealth):**
```rust
// Subsistence: unchanged
Cereal: 0.15, Vegetable: 0.10, Protein: 0.05, HealthCapacity: 0.02
// Standard: empty (no change)
// Luxury: empty (no change)
```

**`"FreePeasant"` (rural, low-medium wealth):**
```rust
// Subsistence:
Cereal: 0.18, Vegetable: 0.12, Protein: 0.08, HealthCapacity: 0.03, EducationSlots: 0.01
// Standard:
Clothing: 0.02, Furniture: 0.01, Radio: 0.003  // NEW: basic consumer electronics
// Luxury: empty
```

**`"LandlessLaborer"` (rural, low wealth):**
```rust
// Subsistence:
Cereal: 0.16, Vegetable: 0.11, Protein: 0.06, HealthCapacity: 0.025, EducationSlots: 0.008
// Standard:
Clothing: 0.015, Furniture: 0.008
// Luxury: empty
```

**`"Aristocracy"` (rural, very high wealth):**
```rust
// Subsistence:
Cereal: 0.25, Vegetable: 0.20, Protein: 0.15, Luxury: 0.05, HealthCapacity: 0.05, EducationSlots: 0.02, Meat: 0.08, Fruit: 0.05  // NEW: Meat, Fruit
// Standard:
Clothing: 0.05, Furniture: 0.03, OfficeMachinery: 0.01, Televisions: 0.005, Agd: 0.008  // NEW: Televisions, Agd
// Luxury:
Luxury: 0.10, LuxuryFurniture: 0.02, LuxuryClothing: 0.03, Cars: 0.005  // NEW: LuxuryFurniture, LuxuryClothing, Cars
```

**`"Worker"` (urban, medium wealth):**
```rust
// Subsistence:
Cereal: 0.20, Vegetable: 0.15, Protein: 0.10, HealthCapacity: 0.04, EducationSlots: 0.015, Meat: 0.03  // NEW: Meat
// Standard:
Clothing: 0.03, Furniture: 0.02, Radio: 0.005, Televisions: 0.003, Agd: 0.005  // NEW: Radio, Televisions, Agd
// Luxury: empty
```

**NEW class baskets to ADD:**

**`"Bourgeoisie"` (urban, high wealth):**
```rust
// Subsistence:
Cereal: 0.22, Vegetable: 0.18, Protein: 0.12, HealthCapacity: 0.04, EducationSlots: 0.02, Meat: 0.06, Fruit: 0.04
// Standard:
Clothing: 0.04, Furniture: 0.03, Radio: 0.005, Televisions: 0.008, Agd: 0.01, Cars: 0.003
// Luxury:
Luxury: 0.05, LuxuryFurniture: 0.01, LuxuryClothing: 0.015
// tier_budget_share: Subsistence 0.4, Standard 0.4, Luxury 0.2
```

**`"PettyBourgeoisie"` (urban, medium-high wealth):**
```rust
// Subsistence:
Cereal: 0.21, Vegetable: 0.16, Protein: 0.11, HealthCapacity: 0.04, EducationSlots: 0.018, Meat: 0.04, Fruit: 0.03
// Standard:
Clothing: 0.035, Furniture: 0.025, Radio: 0.004, Televisions: 0.005, Agd: 0.008, Cars: 0.002
// Luxury:
Luxury: 0.02, LuxuryClothing: 0.005
// tier_budget_share: Subsistence 0.5, Standard 0.4, Luxury 0.1
```

### 20.3.4 — Retail Registry Update

**File:** `state/src/economy/retail_registry.rs`

The `commodity_profile_map()` and `is_compatible()` functions must be updated to handle the new consumer goods:

```rust
StoreProfile::Electronics => matches!(
    commodity,
    Commodity::OfficeMachinery | Commodity::ElectronicComponents
    | Commodity::Radio | Commodity::Televisions | Commodity::Agd  // NEW: Radio, Televisions, Agd
),
StoreProfile::Luxury => matches!(
    commodity,
    Commodity::LuxuryFurniture | Commodity::Luxury | Commodity::LuxuryClothing  // NEW: LuxuryClothing
),
StoreProfile::CarDealer => matches!(  // NEW profile
    commodity,
    Commodity::Cars | Commodity::Trucks
),
StoreProfile::Butcher => matches!(
    commodity,
    Commodity::Meat | Commodity::Protein | Commodity::Fish
),
StoreProfile::Grecery => matches!(
    commodity,
    Commodity::Cereal | Commodity::Vegetable | Commodity::Fruit | Commodity::Food
    | Commodity::Meat  // NEW: Meat
),
```

### 20.3.5 — Demographic Class ID Consistency

The consumption registry is keyed by class ID strings (`"Serf"`, `"Worker"`, etc.). The `RuralClass` enum in `geography.rs` serializes to `"aristocracy"`, `"free_peasant"`, `"serf"`, `"landless_laborer"` (snake_case). The consumption registry uses `"Serf"`, `"FreePeasant"`, `"LandlessLaborer"`, `"Aristocracy"` (PascalCase).

**This mismatch is already handled** by the `get_class()` method which uses `serde_json::to_string(&class)` — but that produces `\"serf\"` (snake_case with quotes). The consumption registry lookup in `build_consumer_demand` uses `consumption.get(class_id)` where `class_id` comes from the BTreeMap key in `rural_classes`.

**Action:** Ensure the class IDs inserted into `rural_classes` / `urban_classes` match the consumption registry keys. The generator or the region initialization code must use PascalCase keys (`"Serf"`, `"FreePeasant"`, etc.) or the consumption registry must be re-keyed to snake_case. This will be verified and aligned in Phase 20.4 when the generator is updated.

---

## Phase 20.4 — Generator Bootstrapping Rewrite

**File:** `state/src/engine/generator/corporate.rs`

This is the second-largest change. The generator must be rewritten to:
1. Guarantee a minimum viable supply chain
2. Pull production methods from the registry instead of inventing parallel recipes
3. Seed fixed-asset cohorts and inventory at world birth

### 20.4.1 — Replace `sector_recipe()` with `best_registry_method()`

**Delete:** The entire `sector_recipe()` function (corporate.rs l.707–866) and its 10 hardcoded match arms.

**Add:** A new `best_registry_method()` function that pulls the most appropriate method from the registry:

```rust
/// Select the best available production method from the registry for a sector
/// at the given start year.
///
/// # Rules
/// * Looks up the sector's BuildingMethods from the registry.
/// * Iterates all Production-slot methods (not Automation/Organization).
/// * Returns the method with the highest year that is <= start_year
///   and whose required_tech is None or whose tech year <= start_year.
/// * Falls back to the earliest method if none match.
/// * Converts the registry's ProductionMethod to an ActiveProductionMethod.
fn best_registry_method(
    sector: Sector,
    start_year: u32,
    registries: &Registries,
) -> (String, ActiveProductionMethod) {
    let sector_key = sector_json_name(sector);
    let methods = registries.production_methods.get(&sector_key);

    let building_name = default_building_name(sector);

    match methods {
        Some(building_methods) => {
            // Find the best Production-slot method
            let best = building_methods.production.values()
                .filter(|pm| pm.year <= start_year)
                .filter(|pm| {
                    match &pm.required_tech {
                        None => true,
                        Some(tech_id) => {
                            // Check if tech is in the discovered list or
                            // tech year <= start_year (pre-seed assumption)
                            registries.tech_tree.get(tech_id)
                                .map(|node| node.year <= start_year)
                                .unwrap_or(false)
                        }
                    }
                })
                .max_by_key(|pm| pm.year)
                .or_else(|| building_methods.production.values().min_by_key(|pm| pm.year));

            match best {
                Some(pm) => {
                    let method = method_from_ratios(
                        pm.experts_ratio,
                        pm.skilled_ratio,
                        pm.basic_ratio,
                        pm.inputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.outputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.year,
                    );
                    (building_name, method)
                }
                None => (building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year)),
            }
        }
        None => (building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year)),
    }
}
```

**Add:** A `default_building_name()` function that returns a sensible building name per sector:

```rust
fn default_building_name(sector: Sector) -> String {
    match sector {
        Sector::Mining => "Mine".to_string(),
        Sector::Agriculture => "Farm".to_string(),
        Sector::HeavyIndustry => "Heavy Industry Plant".to_string(),
        Sector::LightIndustry => "Factory".to_string(),
        Sector::ArmamentsIndustry => "Armaments Factory".to_string(),
        Sector::Construction => "Construction Company".to_string(),
        Sector::Energy => "Power Plant".to_string(),
        Sector::TransportLogistics => "Transport Depot".to_string(),
        Sector::MediaAndEntertainment => "Media Studio".to_string(),
        Sector::MedicalServices => "Medical Facility".to_string(),
        Sector::EducationalServices => "School".to_string(),
        Sector::PublicServices => "Public Office".to_string(),
        Sector::MaintenanceWorkshops => "Maintenance Workshop".to_string(),
        Sector::LocalServices => "Local Services".to_string(),
        Sector::ExportServices => "Export Company".to_string(),
        Sector::Hospitality => "Hospitality Venue".to_string(),
        Sector::Banking => "Bank Branch".to_string(),
        Sector::PublicAdministration => "Administrative Office".to_string(),
        Sector::WasteManagement => "Waste Facility".to_string(),
        Sector::NGO => "NGO Office".to_string(),
        Sector::Religion => "Religious Institution".to_string(),
    }
}
```

### 20.4.2 — Phase A: Minimum Viable Supply Chain Seeding

Add a new function called BEFORE the existing budget-proportional loop:

```rust
/// Phase 20A: Seed minimum viable supply chain.
///
/// For each region, create at least one building for every critical sector,
/// regardless of budget employment share. This guarantees that every
/// fundamental commodity has at least one producer at world birth.
fn seed_minimum_viable_supply_chain(
    country: &Country,
    country_regions: &[&Region],
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    let mut result = Vec::new();

    // Critical sectors that MUST be seeded (priority order)
    let critical_sectors = [
        Sector::Mining,
        Sector::Energy,
        Sector::Agriculture,
        Sector::HeavyIndustry,
        Sector::LightIndustry,
        Sector::Construction,
        Sector::MaintenanceWorkshops,
        Sector::TransportLogistics,
        Sector::MedicalServices,
        Sector::EducationalServices,
        Sector::PublicServices,
        Sector::ArmamentsIndustry,
        Sector::MediaAndEntertainment,
        Sector::LocalServices,
        Sector::ExportServices,
        Sector::Hospitality,
    ];

    for region in country_regions {
        let region_pop = region.population.max(1000) as f64;

        for &sector in &critical_sectors {
            // Skip if this sector is already in the budget with significant employment
            // (it will be handled by the budget-proportional pass)
            // But always seed if employment is 0
            let budget_emp = country.budget.sectors.get(&sector)
                .and_then(|s| s.extra.get("zatrudnienie"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            // Minimum workers per sector per region
            let min_workers = min_workers_for_sector(sector, region_pop);

            // Create one company + one building for this sector in this region
            let (company, building) = create_seed_company(
                sector,
                region,
                min_workers,
                start_year,
                registries,
                idgen,
                rng,
            );
            result.push((company, building));
        }
    }

    result
}
```

```rust
/// Minimum workers for a seed building in a sector, scaled by region population.
fn min_workers_for_sector(sector: Sector, region_pop: f64) -> u32 {
    let base = match sector {
        Sector::Mining => 200,
        Sector::Energy => 150,
        Sector::Agriculture => 300,
        Sector::HeavyIndustry => 250,
        Sector::LightIndustry => 200,
        Sector::Construction => 100,
        Sector::MaintenanceWorkshops => 50,
        Sector::TransportLogistics => 80,
        Sector::MedicalServices => 60,
        Sector::EducationalServices => 50,
        Sector::PublicServices => 40,
        Sector::ArmamentsIndustry => 100,
        Sector::MediaAndEntertainment => 40,
        Sector::LocalServices => 80,
        Sector::ExportServices => 50,
        Sector::Hospitality => 50,
        _ => 50,
    };
    // Scale with region population, capped
    ((base as f64) * (region_pop / 100_000.0).max(0.5).min(5.0)) as u32
}
```

### 20.4.3 — Phase B: Registry-Aligned Company Generation

The existing `generate_region_companies()` function must be updated to call `best_registry_method()` instead of `sector_recipe()`:

**Current (line 554):**
```rust
let (building_name, method) = sector_recipe(sector, start_year, rng);
```

**New:**
```rust
let (building_name, method) = best_registry_method(sector, start_year, registries);
```

This requires passing `registries` through to `generate_region_companies()`. The `generate_corporate_entities()` function already receives `_registries: &Registries` — change the parameter name to `registries` (remove the underscore) and pass it through.

### 20.4.4 — Phase C: Fixed-Asset Cohort Seeding

Add a new function to seed initial fixed assets:

```rust
/// Seed an initial FixedAssetCohort for a building based on its sector.
///
/// This represents pre-existing capital stock — the machinery that was
/// already installed before the simulation begins. Without this, every
/// building starts at machinery_factor = 1.0 (manual mode) and the
/// Phase 19B system has nothing to maintain or depreciate.
fn seed_fixed_assets(
    sector: Sector,
    start_year: u32,
    rng: &mut impl Rng,
) -> Vec<FixedAssetCohort> {
    let (commodity, count) = match sector {
        Sector::Mining => (Commodity::IndustrialMachinery, 5.0),
        Sector::HeavyIndustry => (Commodity::IndustrialMachinery, 8.0),
        Sector::Construction => (Commodity::ConstructionMachinery, 5.0),
        Sector::Agriculture => (Commodity::AgriculturalMachinery, 3.0),
        Sector::TransportLogistics => (Commodity::Trucks, 5.0),
        Sector::PublicServices => (Commodity::OfficeMachinery, 2.0),
        Sector::EducationalServices => (Commodity::OfficeMachinery, 2.0),
        Sector::MaintenanceWorkshops => (Commodity::IndustrialMachinery, 3.0),
        Sector::Energy => (Commodity::IndustrialMachinery, 5.0),
        Sector::LightIndustry => (Commodity::IndustrialMachinery, 3.0),
        Sector::ArmamentsIndustry => (Commodity::IndustrialMachinery, 5.0),
        Sector::MediaAndEntertainment => (Commodity::OfficeMachinery, 2.0),
        _ => return Vec::new(), // No fixed assets for service-only sectors
    };

    vec![FixedAssetCohort {
        blueprint_id: "LEGACY_SEED".to_string(),
        commodity,
        count,
        condition: 0.7 + rng.gen::<f64>() * 0.3, // 0.7–1.0 (worn but functional)
        quality: 0.8, // Legacy quality (below blueprint-produced)
        durability: 200.0,
        base_tech: "seed".to_string(),
        base_tech_year: start_year.saturating_sub(20), // 20-year-old tech
        acquired_turn: 0,
    }]
}
```

**Integration:** In the `Building` creation code (both `create_seed_company` and `generate_region_companies`), replace `fixed_assets: Vec::new()` with:
```rust
fixed_assets: seed_fixed_assets(sector, start_year, rng),
```

### 20.4.5 — Inventory Seeding

Add a function to seed one turn of inputs:

```rust
/// Seed one production cycle of inputs into a building's inventory.
///
/// This prevents first-turn production starvation — buildings can produce
/// immediately without waiting for B2B market clearing.
fn seed_inventory(
    method: &ActiveProductionMethod,
    building_capacity: u32,
) -> BTreeMap<Commodity, f64> {
    let production_scale = building_capacity as f64 / 1000.0;
    let mut inventory = BTreeMap::new();

    for (&commodity, &qty_per_1k) in &method.inputs {
        // Skip fixed-asset commodities — they're handled by cohorts, not inventory
        if commodity.is_fixed_asset() {
            continue;
        }
        let seed_qty = qty_per_1k * production_scale;
        if seed_qty > 0.0 {
            inventory.insert(commodity, seed_qty);
        }
    }

    // Also seed a small amount of Food for worker subsistence
    inventory.entry(Commodity::Food).or_insert(production_scale * 5.0);

    inventory
}
```

**Integration:** In the `Building` creation code, replace `inventory: BTreeMap::new()` with:
```rust
inventory: seed_inventory(&method, base_capacity),
```

Also set `inventory_capacity` to a reasonable default based on the building's scale:
```rust
inventory_capacity: (base_capacity as f64 * 10.0).max(100.0),
```

### 20.4.6 — Updated `generate_corporate_entities()` Flow

The main generation function is restructured:

```rust
pub fn generate_corporate_entities(
    data_dir: &Path,
    country: &mut Country,
    regions: &HashMap<String, Region>,
    registries: &Registries,  // <-- remove underscore
    start_year: u32,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    // ... existing setup ...

    // PHASE A: Seed minimum viable supply chain (NEW)
    let seed_entities = seed_minimum_viable_supply_chain(
        country, &country_regions, start_year, registries, &mut idgen, rng
    );
    for (company, building) in &seed_entities {
        all_companies.push(company.clone());
        all_buildings.push(building.clone());
    }
    // Group seed companies by sector for saving
    // ...

    // PHASE B: Budget-proportional scaling (EXISTING, with best_registry_method)
    for (&sector, share) in &country.budget.sectors {
        // ... existing logic but using best_registry_method() ...
    }

    // ... existing state buildings, landfills, tourism, charities ...

    // PHASE D: Post-generation validation (NEW)
    validate_supply_chain(&all_buildings, registries);

    Ok(())
}
```

### 20.4.7 — Post-Generation Supply Chain Validation

```rust
/// Validate that the generated economy has no orphan commodities.
///
/// Logs warnings for any commodity that is consumed but has no producer,
/// or is produced but has no consumer.
fn validate_supply_chain(buildings: &[Building], registries: &Registries) {
    // Collect all produced and consumed commodities from building active_methods
    let mut produced: BTreeSet<Commodity> = BTreeSet::new();
    let mut consumed: BTreeSet<Commodity> = BTreeSet::new();

    for building in buildings {
        for &c in building.active_method.outputs.keys() {
            produced.insert(c);
        }
        for &c in building.active_method.inputs.keys() {
            if !c.is_fixed_asset() {
                consumed.insert(c);
            }
        }
    }

    // Orphan inputs: consumed but not produced (excluding free natural resources)
    let free_resources = [Commodity::Water]; // Water is a utility output
    let orphan_inputs: Vec<_> = consumed
        .iter()
        .filter(|c| !produced.contains(c) && !free_resources.contains(c))
        .collect();

    for c in orphan_inputs {
        eprintln!("WARNING: Orphan input — {} is consumed but no building produces it", c);
    }

    // Check B2C demand coverage
    let consumption = consumption_registry();
    let b2c_demand: BTreeSet<Commodity> = consumption.values()
        .flat_map(|basket| basket.tiers.values())
        .flat_map(|tier| tier.keys().copied())
        .collect();

    let orphan_demand: Vec<_> = b2c_demand
        .iter()
        .filter(|c| !produced.contains(c))
        .collect();

    for c in orphan_demand {
        eprintln!("WARNING: B2C demand orphan — {} is demanded by consumers but no building produces it", c);
    }
}
```

### 20.4.8 — Budget Sector Employment Fix

The current `build_treasury()` in `generator/mod.rs` only creates sectors for 10 of 21 sectors. It must be expanded to include all sectors that produce goods:

**Add to `build_treasury()`:**
```rust
// Existing sectors stay. Add missing critical sectors:
sectors.insert(Sector::ArmamentsIndustry, sector_share(arm_share / sum, 0.6, tech_limit));
sectors.insert(Sector::TransportLogistics, sector_share(transport_share / sum, 0.4, tech_limit));
sectors.insert(Sector::MediaAndEntertainment, sector_share(media_share / sum, 0.3, tech_limit));
sectors.insert(Sector::MedicalServices, sector_share(med_share / sum, 0.2, tech_limit));
sectors.insert(Sector::EducationalServices, sector_share(edu_share / sum, 0.2, tech_limit));
sectors.insert(Sector::MaintenanceWorkshops, sector_share(maint_share / sum, 0.5, tech_limit));
```

Where the new shares are generated alongside the existing ones:
```rust
let arm_share = rng.gen_range(0.02..0.08);
let transport_share = rng.gen_range(0.03..0.10);
let media_share = rng.gen_range(0.01..0.05);
let med_share = rng.gen_range(0.04..0.10);
let edu_share = rng.gen_range(0.03..0.08);
let maint_share = rng.gen_range(0.01..0.04);
```

And the sum must include these new shares.

---

## Phase 20.5 — Blueprint Spec Updates

**File:** `state/src/registries/blueprint_specs.rs`

Update the `BlueprintSpec` definitions to incorporate the new modern materials:

### 20.5.1 — Updated Specs

**IndustrialMachinery:** Add Semiconductors as a premium upgrade path:
```rust
MaterialRole {
    ideal: Commodity::ElectronicComponents,
    substitutes: vec![
        (Commodity::MechanicalComponents, 0.8, 0.9),  // existing
        (Commodity::Semiconductors, 1.2, 1.1),  // NEW: better than ideal
    ],
    share: 0.3,
},
```

**Cars:** Add Plastics, Batteries, Semiconductors:
```rust
roles: vec![
    MaterialRole { ideal: Commodity::Steel, substitutes: vec![(Commodity::Iron, 0.7, 0.6)], share: 0.3 },
    MaterialRole { ideal: Commodity::Aluminum, substitutes: vec![(Commodity::Iron, 0.6, 0.5)], share: 0.15 },
    MaterialRole { ideal: Commodity::ElectronicComponents, substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)], share: 0.15 },
    MaterialRole { ideal: Commodity::Plastics, substitutes: vec![(Commodity::Steel, 0.7, 0.8)], share: 0.2 },  // NEW
    MaterialRole { ideal: Commodity::Batteries, substitutes: vec![(Commodity::Fuels, 0.5, 0.7)], share: 0.2 },  // NEW (EV variant)
],
```

**Trucks:** Add Plastics, Batteries for modern variants:
```rust
roles: vec![
    MaterialRole { ideal: Commodity::Steel, substitutes: vec![(Commodity::Iron, 0.7, 0.6)], share: 0.5 },
    MaterialRole { ideal: Commodity::MechanicalComponents, substitutes: vec![(Commodity::Iron, 0.6, 0.7)], share: 0.25 },
    MaterialRole { ideal: Commodity::ElectronicComponents, substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)], share: 0.15 },
    MaterialRole { ideal: Commodity::Batteries, substitutes: vec![(Commodity::Fuels, 0.5, 0.7)], share: 0.1 },  // NEW
],
```

**AGD (Appliances):** Add Plastics:
```rust
roles: vec![
    MaterialRole { ideal: Commodity::Steel, substitutes: vec![(Commodity::Iron, 0.7, 0.6)], share: 0.3 },
    MaterialRole { ideal: Commodity::ElectronicComponents, substitutes: vec![(Commodity::MechanicalComponents, 0.8, 0.9)], share: 0.35 },
    MaterialRole { ideal: Commodity::Plastics, substitutes: vec![(Commodity::Steel, 0.6, 0.7)], share: 0.2 },  // NEW
    MaterialRole { ideal: Commodity::Chemicals, substitutes: vec![(Commodity::Iron, 0.5, 0.5)], share: 0.15 },
],
```

**Televisions:** Add Semiconductors, Plastics:
```rust
roles: vec![
    MaterialRole { ideal: Commodity::ElectronicComponents, substitutes: vec![(Commodity::MechanicalComponents, 0.6, 0.8)], share: 0.4 },
    MaterialRole { ideal: Commodity::Semiconductors, substitutes: vec![(Commodity::ElectronicComponents, 0.7, 0.8)], share: 0.2 },  // NEW
    MaterialRole { ideal: Commodity::Plastics, substitutes: vec![(Commodity::Steel, 0.5, 0.7)], share: 0.15 },  // NEW
    MaterialRole { ideal: Commodity::Glass, substitutes: vec![(Commodity::Chemicals, 0.7, 0.6)], share: 0.15 },
    MaterialRole { ideal: Commodity::Chemicals, substitutes: vec![(Commodity::Steel, 0.6, 0.7)], share: 0.1 },
],
```

---

## Phase 20.6 — Validation & Testing

### 20.6.1 — Supply Chain Integrity Test

Create a test in `state/tests/supply_chain_integrity_test.rs` that:

1. Loads the full `Registries::native_only()`
2. Builds a map of all produced commodities (from all production methods across all sectors)
3. Builds a map of all consumed commodities (from all production method inputs, construction BOMs, and consumption registry demand)
4. Asserts no orphan inputs exist (every consumed commodity has at least one producer)
5. Asserts no orphan B2C demand exists (every consumption registry commodity has at least one producer)
6. Asserts the production dependency graph is bootstrappable (every commodity's production chain traces back to primary extraction or free resources)

### 20.6.2 — Generator Bootstrap Test

Create a test that:

1. Calls `generate_world()` with a small country count
2. Loads all generated buildings
3. Verifies every critical sector has at least one building per region
4. Verifies every building has a non-empty `active_method.outputs`
5. Verifies every building has a non-empty `fixed_assets` (for sectors that should have machinery)
6. Verifies every building has non-empty `inventory`
7. Verifies no orphan inputs in the generated economy

### 20.6.3 — Golden Master Regression Test

Run the 100-Turn Golden Master test and verify:
1. Fixed Asset Cohorts are formed (non-zero count across all buildings)
2. MaintenanceServices are produced and consumed
3. No commodity has zero supply for 10+ consecutive turns
4. GDP grows or remains stable (no economic freeze)
5. Consumer demand is at least partially fulfilled for Cereal, Vegetable, Protein, Clothing

### 20.6.4 — Build & Lint Commands

```bash
cd state
cargo build --release
cargo clippy -- -D warnings
cargo test --release
cargo test --release -- --ignored golden_master
```

---

## Implementation Order & Dependencies

The phases must be implemented in strict order because each depends on the previous:

```
20.1 (Enums)          ← Must be first; everything depends on the enum
    ↓
20.2 (Registry)       ← Depends on new enum variants existing
    ↓
20.3 (B2C)            ← Depends on commodities being activated in the registry
    ↓
20.4 (Generator)      ← Depends on registry having methods for all sectors
    ↓
20.5 (Blueprint Specs) ← Depends on new commodities existing in the enum
    ↓
20.6 (Testing)        ← Depends on everything above
```

### Estimated Changes Per File

| File | Change Type | Scope |
|------|------------|-------|
| `registries/enums.rs` | Edit | Add 6 variants, remove 2, fix missing `Chemicals` in `all()`, update array size to 136, add legacy aliases |
| `registries/production_methods_data.rs` | Major rewrite | Add ~50 new production methods across mining, agriculture, heavy_industry, light_industry, energy |
| `registries/tech_tree_data.rs` | Edit | Add ~10 new tech nodes |
| `data/consumption_registry.rs` | Edit | Update 5 existing baskets, add 2 new class baskets |
| `economy/retail_registry.rs` | Edit | Update commodity-profile compatibility, add CarDealer profile |
| `engine/generator/corporate.rs` | Major rewrite | Replace `sector_recipe()`, add `best_registry_method()`, add seed functions, add validation |
| `engine/generator/mod.rs` | Edit | Expand `build_treasury()` sector list |
| `registries/blueprint_specs.rs` | Edit | Update 5 specs with new materials |
| `tests/supply_chain_integrity_test.rs` | New file | Supply chain validation test |
| `construction/bom.rs` | No change needed | ConstructionMachinery now has producers |

---

## Risks & Mitigations

### Risk 1: Breaking Save Compatibility
**Impact:** Existing saves reference commodity keys that are being removed (`grains`, `vegetables`).
**Mitigation:** Add a save-migration function in `io/save_manager.rs` that maps old commodity keys to new ones on load. `grains` → `cereal`, `vegetables` → `vegetable`. All other commodity keys remain stable.

### Risk 2: Performance Impact of Larger Registry
**Impact:** ~50 new production methods increase registry size and lookup time.
**Mitigation:** The registry is a `HashMap` — lookup is O(1). The 50 new methods add negligible memory. The generator's `best_registry_method()` iterates ~10-20 methods per sector — O(n) where n is small.

### Risk 3: Economic Imbalance from New Commodities
**Impact:** New production methods might produce too much or too little, causing price crashes or shortages.
**Mitigation:** The market clearing system (`economy/market.rs`) automatically adjusts prices based on supply and demand. Starting prices are seeded at 100.0 for all commodities in `generate_world()`. The Golden Master test will verify stability.

### Risk 4: Generator Producing Too Many Buildings
**Impact:** The minimum viable supply chain seeds 16 sectors × N regions buildings, plus the budget-proportional pass.
**Mitigation:** Each seed building has a small `worker_capacity` (50–300). The `split_capacity()` function keeps building-level simulation cheap via `scale_factor`. A 10-region country generates ~160 seed buildings + ~200 budget-proportional buildings = ~360 buildings — well within performance budget.

### Risk 5: Fixed-Asset Seeding Creating Unrealistic Economy
**Impact:** Buildings starting with machinery might over-produce on turn 1.
**Mitigation:** Seed cohorts have condition 0.7–1.0 (worn) and quality 0.8 (legacy, below blueprint-produced). The `machinery_factor` formula is `1.0 + Σ count × quality × condition × obsolescence × unit_capacity` — with quality 0.8 and condition ~0.85, the factor is modest (approximately 1.0 + 5 × 0.8 × 0.85 × 1.0 × 0.1 ≈ 1.34 for a mining building with 5 machines). This is a reasonable starting capacity, not an over-production issue.

---

## Conclusion

Phase 20 is the most impactful change since Phase 19. It transforms the economy from a frozen, broken system where 4 of 6 fixed assets have zero supply into a fully bootstrapped modern supply chain with:

- **136 commodities** (132 − 2 + 6) covering a complete 8-layer supply chain from mining to consumer goods
- **~50 new production methods** ensuring every commodity has at least one producer
- **10 new technologies** unlocking modern materials (Semiconductors, Plastics, Batteries, Rare Earth Elements, Lithium)
- **7 consumption baskets** (5 updated + 2 new) with wealth-tier-appropriate demand for consumer durables
- **A generator that pulls from the registry** instead of inventing parallel recipes
- **Guaranteed minimum viable supply chain** with fixed-asset and inventory seeding at world birth
- **Post-generation validation** that catches orphan commodities before the simulation starts

This blueprint is the technical foundation for making Phase 19 Generative Blueprints actually work — without it, there are no materials to design products from, no machinery to install as fixed assets, and no supply chain for blueprints to substitute materials within.

**No implementation has begun. Awaiting explicit user approval of this Phase 20 blueprint.**
