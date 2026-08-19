# Final Commodity Audit & Test Fix Plan

**Status:** BLUEPRINT ONLY — awaiting user approval before implementing commodity fixes. The integration test fix (Part 1) is approved for immediate implementation.

**Date:** Phase 20 post-implementation audit.

---

## Part 1: Integration Test Fix

### Problem

`tests/simulation_100_turns.rs` fails to compile because it uses `crate::economy::market::ApostolicSeeLedger` instead of the correct external-crate path `sim_engine::economy::market::ApostolicSeeLedger`. Integration tests are compiled as separate crates and cannot use the `crate::` prefix to reference the library under test.

### Affected File

**File:** `state/tests/simulation_100_turns.rs`

### Exact Lines to Fix

| Line | Current | Fixed |
|------|---------|-------|
| 33 | `crate::economy::market::ApostolicSeeLedger::default()` | `sim_engine::economy::market::ApostolicSeeLedger::default()` |
| 65 | `crate::economy::market::ApostolicSeeLedger::default()` | `sim_engine::economy::market::ApostolicSeeLedger::default()` |

### Verification

After the fix, run:
```
cargo test --test simulation_100_turns --no-run
```
This should compile successfully. The test already imports `sim_engine::economy::market::GlobalMarket` on line 8, confirming that `sim_engine::` is the correct prefix.

### Notes

- `golden_master_test.rs` compiles successfully — no fix needed there.
- The `ApostolicSeeLedger` struct exists at `state/src/economy/market.rs:65`.

---

## Part 2: The Absolute Commodity Audit

### Methodology

Every one of the 136 `Commodity` enum variants was searched across the entire `state/src/` codebase using ripgrep. For each variant, the following was checked:

1. **Produced:** Appears in the `outputs` array of a `pm(...)` call in `production_methods_data.rs`, or in an `outputs: HashMap::from(...)` in `production_methods.rs`, or is dynamically produced by a specialized module (fishing.rs, media.rs, assimilation.rs).
2. **Consumed (B2B):** Appears in the `inputs` array of a `pm(...)` call or `production_methods.rs` method.
3. **Consumed (B2C):** Appears in `consumption_registry.rs` baskets.
4. **Consumed (BOM):** Appears in `blueprint_specs.rs` as an ideal material or substitute.
5. **Consumed (Infrastructure):** Appears in `building_condition.rs` renovation BOMs.
6. **Consumed (Utilities):** Appears in `utilities/grid.rs` for district heating.
7. **Consumed (Generator):** Appears in `corporate.rs` state building recipes or fixed-asset seeding.
8. **Referenced in logic:** Appears in any other `.rs` file for gameplay logic (e.g., justice system, security, fishing).

### Classification Summary

| Category | Count | Description |
|----------|-------|-------------|
| **Fully Integrated** | 96 | Produced AND consumed somewhere |
| **Orphan Inputs** | 3 | Consumed but never produced |
| **Orphan Outputs** | 0 | Produced but never consumed |
| **Dead Enums** | 30 | Only in enum definition + `try_from` |
| **Dynamic/Special** | 7 | Produced/consumed outside the registry by specialized modules |
| **Total** | 136 | |

### Category 1: Fully Integrated (96 commodities)

These commodities are produced by at least one production method and consumed by at least one production method, B2C basket, blueprint BOM, or infrastructure BOM.

```
AdministrativeServices, AgriculturalMachinery, Aluminum, Ammonia, Ammunition,
Asphalt, Bauxite, Batteries, Bitumen, Bricks, BrownCoal, Cars, Catalysts,
Cement, Cereal, Chemicals, Clay, Clothing, Coke, ConstructionMachinery,
ConstructionServices, Copper, EducationSlots, ElectronicComponents, Energy,
Fertilizers, Fibers, Fodder, Food, Fruit, Fuels, Furniture, Glass, Gold,
Gravel, HardCoal, HealthCapacity, Hydrogen, IndustrialFiber,
IndustrialMachinery, InnovationPoints, Iron, Lead, LightTanks, Limestone,
Lithium, Livestock, Luxury, LuxuryClothing, LuxuryFurniture,
MaintenanceServices, Meat, MechanicalComponents, MedicalEquipment,
NaturalGas, OfficeMachinery, Oil, Paper, PassengerTransport, Peat,
Pharmaceuticals, Planks, Plastics, Protein, Radio, RareEarthElements,
RefinedFuel, Rifles, Salt, Sand, Seeds, Semiconductors, Silicon, Silver,
SodaAsh, Software, Steel, Stone, Sulfur, SupportEquipment, Televisions,
Timber, Tin, TowedArtillery, Trucks, Vegetable, Water, Zinc,
Agd, Magnesium (BOM only — see note below)
```

**Note on `Magnesium`:** Referenced 2 times — only in enum + `try_from`. However, it appears in the `all()` array. On closer inspection, it is NOT referenced in any production method, BOM, or B2C basket. It should be reclassified as **Dead Enum** (see below). This reduces the fully integrated count to **95** and increases dead enums to **31**.

**Corrected count:**
| Category | Count |
|----------|-------|
| Fully Integrated | 95 |
| Orphan Inputs | 3 |
| Dead Enums | 31 |
| Dynamic/Special | 7 |
| **Total** | 136 |

### Category 2: Orphan Inputs (3 commodities)

These commodities are consumed by some system but have NO producer anywhere in the codebase.

| Commodity | Consumed By | Root Cause |
|-----------|-------------|------------|
| `Heat` | `utilities/grid.rs` — district heating consumes `Commodity::Heat` from building inventory | No production method outputs `Heat`. Energy methods only output `Energy`. |
| `RenovationServices` | `infrastructure/building_condition.rs` — renovation BOMs demand 5–1000 units | No production method or dynamic module produces `RenovationServices`. |
| `AssimilationCapacity` | `economy/assimilation.rs:117` — reads from `last_production` | No production method outputs `AssimilationCapacity`. Only test code inserts it into `last_production`. |

### Category 3: Orphan Outputs (0 commodities)

No commodities are produced without any consumer. All produced commodities have at least one downstream consumer.

### Category 4: Dead Enums (31 commodities)

These variants exist in the `Commodity` enum and `all()` array but are **never referenced** in any production method, B2C basket, BOM, generator recipe, or gameplay logic. Their only appearances are:
- The enum variant definition in `enums.rs`
- The `try_from` deserialization match arm in `enums.rs`

| # | Commodity | Serde Key | Notes |
|---|-----------|-----------|-------|
| 1 | `MobileArtillery` | `mobile_artillery` | Military vehicle — no production or consumption |
| 2 | `AntiAircraftArtillery` | `anti_aircraft_artillery` | Military vehicle — no production or consumption |
| 3 | `InfantryFightingVehicles` | `infantry_fighting_vehicles` | Military vehicle — no production or consumption |
| 4 | `Bombers` | `bombers` | Military aircraft — no production or consumption |
| 5 | `MilitaryTrucks` | `military_trucks` | Military vehicle — no production or consumption |
| 6 | `HeavyTanks` | `heavy_tanks` | Military vehicle — no production or consumption |
| 7 | `MediumTanks` | `medium_tanks` | Military vehicle — no production or consumption |
| 8 | `Frigates` | `frigates` | Naval vessel — no production or consumption |
| 9 | `Helicopters` | `helicopters` | Military aircraft — no production or consumption |
| 10 | `Cruisers` | `cruisers` | Naval vessel — no production or consumption |
| 11 | `AircraftCarriers` | `aircraft_carriers` | Naval vessel — no production or consumption |
| 12 | `Magnesium` | `magnesium` | Metal — no production or consumption |
| 13 | `Fighters` | `fighters` | Military aircraft — no production or consumption |
| 14 | `Destroyers` | `destroyers` | Naval vessel — no production or consumption |
| 15 | `Battleships` | `battleships` | Naval vessel — no production or consumption |
| 16 | `Pistols` | `pistols` | Small arm — no production or consumption |
| 17 | `Trains` | `trains` | Transport vehicle — no production or consumption |
| 18 | `Prefabricates` | `prefabricates` | Construction material — no production or consumption |
| 19 | `Gunpowder` | `gunpowder` | Explosive — no production or consumption |
| 20 | `Airplanes` | `airplanes` | Civil aircraft — no production or consumption |
| 21 | `PassengerShips` | `passenger_ships` | Civil vessel — no production or consumption |
| 22 | `CargoShips` | `cargo_ships` | Civil vessel — no production or consumption |
| 23 | `NavalVessels` | `naval_vessels` | Naval vessel — no production or consumption |
| 24 | `MineralResources` | `mineral_resources` | Generic resource — no production or consumption |
| 25 | `RollingStock` | `rolling_stock` | Rail equipment — no production or consumption |
| 26 | `BankingServices` | `banking_services` | Service — no production or consumption |
| 27 | `LocalServicesCommodity` | `local_services` | Service — no production or consumption |
| 28 | `InsuranceServices` | `insurance_services` | Service — no production or consumption |
| 29 | `Submarines` | `submarines` | Naval vessel — no production or consumption |
| 30 | `MarketResearch` | `market_research` | Service — no production or consumption |
| 31 | `Magnesium` | `magnesium` | Metal — no production or consumption |

### Category 5: Dynamic/Special (7 commodities)

These commodities are produced or consumed by specialized modules outside the production method registry. They are **functional but not registry-integrated**.

| Commodity | Producer | Consumer | Status |
|-----------|----------|----------|--------|
| `Fish` | `economy/fishing.rs` — dynamically generated sell asks | `retail_registry.rs` — Butcher compatibility | Functional but not in registry |
| `Information` | `economy/media.rs` — dynamically generated B2C supply | `economy/media.rs` — B2C clearing | Functional but not in registry |
| `JusticeCapacity` | `production_methods.rs` — courthouse methods | `economy/justice_system.rs`, `economy/ethnic_violence.rs` | Functional (in `production_methods.rs`, not `production_methods_data.rs`) |
| `SecurityCapacity` | `production_methods.rs` — police station methods | `economy/ethnic_violence.rs` | Functional (in `production_methods.rs`) |
| `IntelligenceCapacity` | `production_methods.rs` — intelligence HQ methods | Intelligence logic | Functional (in `production_methods.rs`) |
| `FireProtectionCapacity` | `production_methods.rs` — fire brigade methods | Fire protection logic | Functional (in `production_methods.rs`) |
| `ShelterCapacity` | `production_methods.rs` — levee methods | Flood shelter logic | Functional (in `production_methods.rs`) |
| `BorderEnforcementCapacity` | `production_methods.rs` — border guard methods | Border enforcement logic | Functional (in `production_methods.rs`) |
| `CustomsCapacity` | `production_methods.rs` — customs house methods | Customs logic | Functional (in `production_methods.rs`) |
| `SanitaryInspectionCapacity` | `production_methods.rs` — Sanepid methods | Inspection logic | Functional (in `production_methods.rs`) |
| `BuildingInspectionCapacity` | `production_methods.rs` — Building Inspectorate methods | Inspection logic | Functional (in `production_methods.rs`) |
| `EnvironmentalInspectionCapacity` | `production_methods.rs` — Environmental Inspectorate methods | Inspection logic | Functional (in `production_methods.rs`) |
| `ReligiousTexts` | `production_methods.rs` — monastery methods | Religious economy | Functional (in `production_methods.rs`) |
| `ReligiousArt` | `production_methods.rs` — temple methods | Religious economy | Functional (in `production_methods.rs`) |

**Note:** The government capacity commodities (JusticeCapacity through EnvironmentalInspectionCapacity) and religious commodities (ReligiousTexts, ReligiousArt) are produced in `production_methods.rs` (the older hardcoded registry), not `production_methods_data.rs` (the Phase 20 registry). They are functional but split across two registry files. This is a structural inconsistency, not a bug.

---

## Part 3: The Gap-Fill Blueprint

### 3A: Fix Orphan Inputs (3 commodities)

#### `Heat` — Add to energy production methods

**Problem:** `utilities/grid.rs` consumes `Commodity::Heat` from building inventory, but no production method produces it.

**Fix:** Add `Heat` as a co-output to existing energy methods that use combustion (coal, gas, oil plants). In real economies, cogeneration plants produce both electricity and heat.

**File:** `state/src/registries/production_methods_data.rs` — `energy_methods()`

**Changes:**
```rust
// Coal-Fired Boilers: add Heat co-output
outputs: &[(Commodity::Energy, 30.0), (Commodity::Heat, 10.0)]

// Turbo-Generator Plant: add Heat co-output
outputs: &[(Commodity::Energy, 50.0), (Commodity::Heat, 15.0)]

// Steam Turbine Plant: add Heat co-output
outputs: &[(Commodity::Energy, 80.0), (Commodity::Heat, 25.0)]

// Combined Cycle Plant: add Heat co-output
outputs: &[(Commodity::Energy, 250.0), (Commodity::Heat, 40.0)]
```

**Rationale:** Cogeneration is realistic. Not all energy methods produce heat (hydro, solar, wind, nuclear don't), which creates strategic variety.

#### `RenovationServices` — Add to construction or maintenance methods

**Problem:** `infrastructure/building_condition.rs` demands `RenovationServices` in renovation BOMs, but no method produces it.

**Fix:** Add `RenovationServices` as an output to construction methods (construction companies do renovations) or maintenance workshop methods.

**File:** `state/src/registries/production_methods_data.rs` — `construction_methods()` and/or `maintenance_workshops_methods()`

**Changes (option A — construction):**
```rust
// Manual Construction: add RenovationServices co-output
outputs: &[(Commodity::ConstructionServices, 10.0), (Commodity::RenovationServices, 5.0)]

// Reinforced Concrete: add RenovationServices co-output
outputs: &[(Commodity::ConstructionServices, 30.0), (Commodity::RenovationServices, 10.0)]
```

**Changes (option B — maintenance workshops):**
```rust
// Manual Repair Shop: add RenovationServices co-output
outputs: &[(Commodity::MaintenanceServices, 10.0), (Commodity::RenovationServices, 5.0)]

// Mechanized Workshop: add RenovationServices co-output
outputs: &[(Commodity::MaintenanceServices, 18.0), (Commodity::RenovationServices, 8.0)]
```

**Recommendation:** Option A (construction) is more realistic — construction companies handle major renovations. Maintenance workshops handle minor repairs.

#### `AssimilationCapacity` — Dedicated Integration Center production method

**Problem:** `economy/assimilation.rs` reads `AssimilationCapacity` from `last_production`, but no production method produces it. The code comments explicitly state this is produced by **Integration Centers** (Phase 17B), not schools. The assimilation system uses a dual-channel model: education coverage (from schools) + integration capacity (from Integration Centers). Adding `AssimilationCapacity` to regular schools would break this architectural separation.

**Fix:** Create a dedicated Integration Center production method in `public_services_methods()` that explicitly consumes `Paper`, `Software`, and `AdministrativeServices`, and outputs `AssimilationCapacity`.

**File:** `state/src/registries/production_methods_data.rs` — `public_services_methods()`

**Changes:**
```rust
// ── Phase 20: Integration Center (Phase 17B AssimilationCapacity producer) ──
m.insert(MethodSlot::Production, "Integration Center".into(),
    pm(1950, None, 0.25, 0.40, 0.35, 1.0,
       &[(Commodity::Paper, 8.0), (Commodity::AdministrativeServices, 5.0), (Commodity::Food, 3.0)],
       &[(Commodity::AssimilationCapacity, 20.0)]));
m.insert(MethodSlot::Production, "Language & Civic Integration".into(),
    pm(1980, Some("auto3_004"), 0.30, 0.40, 0.30, 2.0,
       &[(Commodity::Paper, 5.0), (Commodity::Software, 5.0), (Commodity::AdministrativeServices, 8.0), (Commodity::ElectronicComponents, 2.0)],
       &[(Commodity::AssimilationCapacity, 50.0)]));
m.insert(MethodSlot::Production, "Digital Integration Platform".into(),
    pm(2000, Some("advman_004"), 0.35, 0.40, 0.25, 3.5,
       &[(Commodity::Software, 10.0), (Commodity::AdministrativeServices, 10.0), (Commodity::ElectronicComponents, 5.0)],
       &[(Commodity::AssimilationCapacity, 100.0)]));
```

**Rationale:** Integration Centers are dedicated institutions for adult immigrant assimilation — distinct from schools (which handle children's education). The dual-channel assimilation architecture in `assimilation.rs` explicitly separates education coverage from integration capacity. The three methods span 1950–2000, showing historical progression from paper-based civic classes to digital platforms. The inputs (`Paper`, `Software`, `AdministrativeServices`) reflect the administrative and educational nature of integration work.

**Note:** The generator's `seed_minimum_viable_supply_chain()` already seeds `PublicServices` buildings, which will now have access to these Integration Center methods through `best_registry_method()`.

### 3B: Handle Dead Enums (31 commodities)

**Strategy:** The 31 dead enums fall into three groups:

#### Group 1: Military Vehicles/Vessels/Aircraft (19 variants)

```
MobileArtillery, AntiAircraftArtillery, InfantryFightingVehicles,
Bombers, MilitaryTrucks, HeavyTanks, MediumTanks,
Frigates, Helicopters, Cruisers, AircraftCarriers,
Fighters, Destroyers, Battleships, Submarines,
Pistols, Gunpowder, Airplanes, NavalVessels
```

**Options:**
- **Option A (Integrate):** Add production methods to `armaments_methods()` for each. This would make the armaments sector much richer but requires significant work — each vehicle needs inputs (steel, electronic components, fuels, mechanical components) and appropriate technology unlocks.
- **Option B (Deprecate):** Remove from the enum and `all()`, keeping `#[serde(alias = "...")]` on a legacy variant for save compatibility. This reduces the enum to 117 variants.
- **Option C (Partial integrate):** Integrate the most impactful ones (HeavyTanks, MediumTanks, Fighters, Bombers, Helicopters, Submarines) and deprecate the rest.

**Recommendation:** **Option C (partial integrate).** The armaments sector currently only produces `TowedArtillery`, `LightTanks`, `Rifles`, `Ammunition`, and `SupportEquipment`. Adding `HeavyTanks`, `MediumTanks`, `Fighters`, `Bombers`, `Helicopters`, and `Submariles` would create a complete military supply chain. The remaining 13 (naval vessels, anti-aircraft, IFVs, military trucks, pistols, gunpowder, airplanes) can be deprecated.

**Proposed armaments additions:**
```rust
// Heavy Tank Production (1942)
pm(1942, Some("arm_002"), 0.25, 0.40, 0.35, 2.5,
   inputs: &[(Steel, 40.0), (Fuels, 20.0), (MechanicalComponents, 15.0), (ElectronicComponents, 5.0)],
   outputs: &[(HeavyTanks, 2.0)])

// Medium Tank Production (1935)
pm(1935, Some("arm_002"), 0.22, 0.38, 0.40, 2.0,
   inputs: &[(Steel, 30.0), (Fuels, 15.0), (MechanicalComponents, 10.0)],
   outputs: &[(MediumTanks, 4.0)])

// Fighter Production (1940)
pm(1940, Some("arm_004"), 0.25, 0.40, 0.35, 3.0,
   inputs: &[(Steel, 20.0), (Aluminum, 15.0), (Fuels, 10.0), (ElectronicComponents, 5.0)],
   outputs: &[(Fighters, 5.0)])

// Bomber Production (1942)
pm(1942, Some("arm_004"), 0.28, 0.42, 0.30, 3.5,
   inputs: &[(Steel, 30.0), (Aluminum, 20.0), (Fuels, 15.0), (ElectronicComponents, 8.0)],
   outputs: &[(Bombers, 3.0)])

// Helicopter Production (1960)
pm(1960, Some("auto3_003"), 0.30, 0.40, 0.30, 4.0,
   inputs: &[(Steel, 15.0), (Aluminum, 10.0), (Fuels, 12.0), (ElectronicComponents, 8.0), (MechanicalComponents, 5.0)],
   outputs: &[(Helicopters, 4.0)])

// Submarine Production (1935)
pm(1935, Some("arm_002"), 0.25, 0.40, 0.35, 3.0,
   inputs: &[(Steel, 50.0), (Fuels, 10.0), (MechanicalComponents, 15.0), (ElectronicComponents, 5.0)],
   outputs: &[(Submarines, 1.0)])
```

**Proposed deprecations (13 variants):**
```
MobileArtillery, AntiAircraftArtillery, InfantryFightingVehicles,
MilitaryTrucks, Frigates, Cruisers, AircraftCarriers,
Destroyers, Battleships, NavalVessels,
Pistols, Gunpowder, Airplanes
```

#### Group 2: Transport & Civil Vessels (5 variants)

```
Trains, Prefabricates, PassengerShips, CargoShips, RollingStock
```

**Options:**
- **Option A (Integrate):** Add production methods. `Trains` and `RollingStock` would go in heavy industry; `PassengerShips` and `CargoShips` in heavy industry or transport; `Prefabricates` in construction.
- **Option B (Deprecate):** Remove with save-compatibility aliases.

**Recommendation:** **Integrate `Prefabricates` and `Trains`** (they have clear supply chain roles). **Deprecate `PassengerShips`, `CargoShips`, `RollingStock`** (niche/obsolete for the simulation's scope).

**Proposed additions:**
```rust
// Heavy industry: Prefabricates Production
pm(1900, None, 0.10, 0.30, 0.60, 1.5,
   inputs: &[(Cement, 10.0), (Steel, 5.0), (Energy, 5.0)],
   outputs: &[(Prefabricates, 20.0)])

// Heavy industry: Locomotive Production
pm(1890, Some("steam_002"), 0.15, 0.35, 0.50, 2.0,
   inputs: &[(Steel, 25.0), (MechanicalComponents, 10.0), (Energy, 5.0)],
   outputs: &[(Trains, 3.0)])
```

#### Group 3: Services & Misc (7 variants)

```
BankingServices, LocalServicesCommodity, InsuranceServices,
MineralResources, MarketResearch, Magnesium
```

**Options:**
- **Option A (Integrate):** Add production methods for banking, insurance, local services, and market research. Add magnesium to mining/heavy industry.
- **Option B (Deprecate):** Remove with save-compatibility aliases.

**Recommendation:** **Integrate `BankingServices`, `LocalServicesCommodity`, and `Magnesium`.** **Deprecate `InsuranceServices`, `MineralResources`, and `MarketResearch`.**

**Proposed additions:**
```rust
// Public services or new banking sector: Banking Services
pm(1880, None, 0.30, 0.40, 0.30, 1.0,
   inputs: &[(Paper, 5.0), (OfficeMachinery, 2.0), (Energy, 3.0)],
   outputs: &[(BankingServices, 15.0)])

// Local services: LocalServicesCommodity
pm(1880, None, 0.15, 0.35, 0.50, 1.0,
   inputs: &[(Fuels, 5.0), (Food, 4.0), (Clothing, 2.0)],
   outputs: &[(LocalServicesCommodity, 20.0)])

// Mining: Magnesium Production
pm(1900, None, 0.10, 0.30, 0.60, 1.5,
   inputs: &[(Energy, 10.0), (Water, 5.0), (Chemicals, 3.0)],
   outputs: &[(Magnesium, 15.0)])
```

### 3C: Summary of Proposed Changes

| Action | Count | Details |
|--------|-------|---------|
| **Fix orphan inputs** | 3 | `Heat` (energy co-output), `RenovationServices` (construction co-output), `AssimilationCapacity` (dedicated Integration Center method) |
| **Integrate dead enums** | 11 | `HeavyTanks`, `MediumTanks`, `Fighters`, `Bombers`, `Helicopters`, `Submarines`, `Prefabricates`, `Trains`, `BankingServices`, `LocalServicesCommodity`, `Magnesium` |
| **Deprecate dead enums** | 16 | `MobileArtillery`, `AntiAircraftArtillery`, `InfantryFightingVehicles`, `MilitaryTrucks`, `Frigates`, `Cruisers`, `AircraftCarriers`, `Destroyers`, `Battleships`, `NavalVessels`, `Pistols`, `Gunpowder`, `Airplanes`, `PassengerShips`, `CargoShips`, `RollingStock`, `InsuranceServices`, `MineralResources`, `MarketResearch` |
| **Add Automation/Organization methods** | 8 | 3 Medical Automation, 1 Medical Organization, 1 Education Automation, 1 Media Automation, 1 Public Services Organization, 1 Maintenance Automation |
| **Add Integration Center methods** | 3 | 3 Production methods in Public Services (1950, 1980, 2000) |
| **No action needed (dynamic/special)** | 14 | Government capacities + religious + Fish + Information |

**Post-fix commodity count:** 136 - 16 deprecated = **120 active variants**.

### 3D: Automation & Organization Method Progression

**Problem:** The audit revealed that while most sectors have 3-5 Automation and 3-5 Organization methods, several sectors have thin coverage with large temporal gaps. The user explicitly requested historically accurate, tech-gated progression for both `Automation` and `Organization` slots across all sectors, with methods that drastically alter labor ratios (skilled vs. basic) and increase efficiency.

**Current Coverage Assessment:**

| Sector | Production | Automation | Organization | Gap |
|--------|-----------|------------|--------------|-----|
| mining | 28 | 5 | 4 | Good |
| agriculture | 17 | 5 | 4 | Good |
| heavy_industry | 53 | 6 | 5 | Good |
| light_industry | 16 | 4 | 3 | Good |
| armaments | 7 | 4 | 3 | Good |
| construction | 6 | 5 | 3 | Good |
| energy | 12 | 4 | 3 | Good |
| transport | 7 | 4 | 3 | Good |
| media | 6 | 3 | 3 | Moderate — add 1-2 Automation |
| medical | 6 | **2** | 3 | **Thin — 110-year gap in Automation** |
| education | 5 | 3 | 3 | Moderate — add 1-2 Automation |
| public_services | 4 | 3 | 3 | Moderate — add Integration Center methods (3A above) |
| maintenance_workshops | 4 | 3 | 3 | Moderate |

**Sectors needing expansion:**

#### Medical Services — Automation (currently 2 methods, 1880→1990 gap)

**File:** `state/src/registries/production_methods_data.rs` — `medical_methods()`

**Add:**
```rust
// Punch Card Records (1930) — intermediate step between manual and electronic
m.insert(MethodSlot::Automation, "Punch Card Records".into(),
    pm(1930, Some("elecf_005"), 0.15, 0.30, 0.55, 1.5,
       &[(Commodity::Paper, 3.0), (Commodity::Energy, 2.0)], &[]));

// Mainframe Patient Database (1970) — early computerization
m.insert(MethodSlot::Automation, "Mainframe Patient Database".into(),
    pm(1970, Some("cs_005"), 0.20, 0.35, 0.45, 2.0,
       &[(Commodity::ElectronicComponents, 3.0), (Commodity::Energy, 5.0)], &[]));

// AI-Assisted Diagnostics (1998) — late-stage automation
m.insert(MethodSlot::Automation, "AI-Assisted Diagnostics".into(),
    pm(1998, Some("advman_006"), 0.35, 0.40, 0.25, 3.5,
       &[(Commodity::ElectronicComponents, 8.0), (Commodity::Software, 8.0)], &[]));
```

**Progression:** Manual Records (1880, eff 1.0) → Punch Card (1930, eff 1.5) → Mainframe (1970, eff 2.0) → Electronic Health Records (1990, eff 2.5) → AI Diagnostics (1998, eff 3.5). Basic labor ratio drops from 0.70 → 0.25.

#### Medical Services — Organization (add 1 method for 1940s gap)

```rust
// Socialized Medicine (1948) — post-war public health
m.insert(MethodSlot::Organization, "Socialized Medicine".into(),
    pm(1948, Some("bio_003"), 0.25, 0.40, 0.35, 1.8,
       &[(Commodity::Food, 5.0), (Commodity::Paper, 8.0)], &[]));
```

#### Education — Automation (add 1 method for 1950s gap)

**File:** `state/src/registries/production_methods_data.rs` — `education_methods()`

```rust
// Language Laboratory (1960) — intermediate between audiovisual and smart classrooms
m.insert(MethodSlot::Automation, "Language Laboratory".into(),
    pm(1960, Some("radio_004"), 0.25, 0.35, 0.40, 2.0,
       &[(Commodity::Energy, 8.0), (Commodity::ElectronicComponents, 3.0)], &[]));
```

**Progression:** Blackboard & Books (1880, eff 1.0) → Audiovisual Aids (1950, eff 1.5) → Language Laboratory (1960, eff 2.0) → Smart Classrooms (1990, eff 3.0).

#### Media — Automation (add 1 method for 1950s gap)

**File:** `state/src/registries/production_methods_data.rs` — `media_methods()`

```rust
// Magnetic Tape Editing (1955) — intermediate between linotype and digital
m.insert(MethodSlot::Automation, "Magnetic Tape Editing".into(),
    pm(1955, Some("radio_004"), 0.18, 0.32, 0.50, 2.0,
       &[(Commodity::Energy, 5.0), (Commodity::MechanicalComponents, 2.0)], &[]));
```

#### Public Services — Organization (add 1 method for 1960s gap)

**File:** `state/src/registries/production_methods_data.rs` — `public_services_methods()`

```rust
// Computerized Bureaucracy (1965) — intermediate between civil service and New Public Management
m.insert(MethodSlot::Organization, "Computerized Bureaucracy".into(),
    pm(1965, Some("cs_005"), 0.28, 0.40, 0.32, 2.0,
       &[(Commodity::Food, 5.0), (Commodity::ElectronicComponents, 3.0)], &[]));
```

#### Maintenance Workshops — Automation (add 1 method for 1970s gap)

**File:** `state/src/registries/production_methods_data.rs` — `maintenance_workshops_methods()`

```rust
// Computerized Diagnostics (1975) — intermediate between electrified and robotic
m.insert(MethodSlot::Automation, "Computerized Diagnostics".into(),
    pm(1975, Some("cs_005"), 0.25, 0.40, 0.35, 3.0,
       &[(Commodity::ElectronicComponents, 5.0), (Commodity::Software, 3.0)], &[]));
```

#### Summary of Automation/Organization Additions

| Sector | Slot | New Methods | Years |
|--------|------|------------|-------|
| Medical | Automation | 3 | 1930, 1970, 1998 |
| Medical | Organization | 1 | 1948 |
| Education | Automation | 1 | 1960 |
| Media | Automation | 1 | 1955 |
| Public Services | Organization | 1 | 1965 |
| Maintenance | Automation | 1 | 1975 |
| **Total** | | **8 new methods** | |

**Design Principles Applied:**
- Every new method is tech-gated with an appropriate `required_tech` reference.
- Labor ratios shift dramatically: basic labor decreases from ~0.70 (1880) to ~0.25 (1998), while expert labor increases from ~0.10 to ~0.35.
- Efficiency multipliers increase progressively from 1.0 to 3.5-5.5.
- Each method consumes era-appropriate inputs (Paper → ElectronicComponents → Software).
- No method produces outputs (Automation/Organization slots only modify production efficiency and labor composition).

### 3E: Deprecation Strategy

For deprecated variants:
1. Remove the variant from the `Commodity` enum.
2. Remove from `Commodity::all()`.
3. Remove from `try_from` match arm.
4. Add a catch-all legacy deserialization that maps the old serde key to a placeholder or returns an error.
5. Update `Commodity::all()` return type to the new count.
6. Update any tests that reference the deprecated variants.

**Alternative (safer):** Instead of removing variants, mark them with `#[deprecated]` and add a `Commodity::is_active()` method that returns `false` for deprecated variants. The generator and market code can then skip deprecated commodities. This avoids save-compatibility issues entirely.

**Recommended approach:** Use the `#[deprecated]` attribute + `is_active()` filter. This is safer and reversible.

---

## Verification Plan

After implementing the gap-fill:

1. **Run existing tests:**
   ```
   cargo test --lib
   cargo test --test supply_chain_integrity_test
   cargo test --test tech_tree_integrity_test
   ```

2. **Run integration tests (after Part 1 fix):**
   ```
   cargo test --test simulation_100_turns
   cargo test --test golden_master_test
   ```

3. **Add new test:** Update `supply_chain_integrity_test.rs` to verify:
   - `Heat` is produced by at least one energy method.
   - `RenovationServices` is produced by at least one construction/maintenance method.
   - `AssimilationCapacity` is produced by at least one public services method (Integration Center).
   - `AssimilationCapacity` is NOT produced by any education method (architectural separation).
   - No dead enums remain (or they are marked deprecated).
   - Every sector has at least 3 Automation methods with no temporal gap > 40 years.
   - Every sector has at least 3 Organization methods with no temporal gap > 40 years.
   - Automation methods show progressive labor ratio shift (basic ratio decreases over time).
   - Automation methods show progressive efficiency increase over time.

4. **Full build:**
   ```
   cargo check --lib
   cargo test
   ```

---

## Risks & Considerations

1. **Save compatibility:** Removing enum variants breaks deserialization of old saves. The `#[deprecated]` + `is_active()` approach avoids this entirely.

2. **Two registry files:** `production_methods.rs` and `production_methods_data.rs` both define production methods. The Phase 20 registry (`production_methods_data.rs`) is the authoritative source for the 13 main sectors. The older `production_methods.rs` contains government buildings (courthouses, police stations, etc.). Any new methods for `Heat`, `RenovationServices`, and `AssimilationCapacity` should go in `production_methods_data.rs` for consistency.

3. **Armaments expansion:** Adding 6 new military production methods increases the armaments sector's complexity. Each method should have appropriate technology prerequisites to avoid anachronistic production (e.g., no helicopters before 1960).

4. **Government capacity commodities:** The 12 government capacity commodities (JusticeCapacity, SecurityCapacity, etc.) are produced in `production_methods.rs` but not in `production_methods_data.rs`. A future phase should consolidate these into the Phase 20 registry for structural consistency. This is NOT a bug — just a technical debt item.

5. **`Fish` and `Information`:** These are produced by specialized modules (fishing.rs, media.rs) outside the registry. They are functional but not visible to the supply chain integrity test. A future enhancement could add them as dynamic producers in the registry.

6. **Automation/Organization progression:** The 8 new Automation/Organization methods fill temporal gaps in Medical, Education, Media, Public Services, and Maintenance sectors. Each method references an existing technology ID (verified against the tech tree). The labor ratio progression (basic 0.70→0.25, expert 0.10→0.35) reflects historical automation trends. These methods do NOT produce outputs — they only modify the efficiency and labor composition of the sector's Production methods.

7. **Integration Center architectural separation:** `AssimilationCapacity` is produced ONLY by Integration Center methods in `public_services_methods()`, NOT by education methods. This preserves the dual-channel assimilation architecture from Phase 17B: education coverage (schools) + integration capacity (Integration Centers). Adding it to schools would break this separation and cause the assimilation system to double-count school capacity.
