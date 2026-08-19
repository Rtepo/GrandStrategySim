# Global Supply Chain & Commodity Consistency Audit

**Date:** 2026-08-11
**Status:** ANALYSIS ONLY — awaiting user approval before any implementation
**Trigger:** 100-Turn Golden Master test revealed 0 Fixed Asset Cohorts formed; economy freezes because no companies produce or sell `IndustrialMachinery`, `ConstructionMachinery`, `AgriculturalMachinery`, or `OfficeMachinery`.

---

## Executive Summary

The audit uncovered **three catastrophic structural flaws** that explain the frozen economy:

1. **3 of 4 fixed-asset machinery commodities have ZERO production sources.** `ConstructionMachinery`, `AgriculturalMachinery`, and `OfficeMachinery` are consumed as inputs by multiple production methods and construction BOMs, but no production method in the registry OR the generator ever produces them as outputs. `IndustrialMachinery` is the sole exception (produced by the generator's HeavyIndustry recipe and 2 late-game registry methods), but even it is fragile.

2. **The generator's `sector_recipe` function is severely incomplete.** Only 10 of 21 sectors have explicit recipes. The remaining 11 sectors (including ArmamentsIndustry, TransportLogistics, MediaAndEntertainment, MedicalServices, EducationalServices, MaintenanceWorkshops, Banking, and others) fall through to a `_` catch-all arm that returns **empty inputs and empty outputs** — meaning their buildings produce nothing and consume nothing at world birth. The production methods registry (`production_methods_data.rs`) has proper methods for these sectors, but the generator never loads them into buildings.

3. **A commodity name mismatch breaks the food supply chain.** The generator's Agriculture recipe produces `Grains` and `Vegetables` (plural), but the consumption registry demands `Cereal` and `Vegetable` (singular). These are **different enum variants** that are invisible to each other in the market. Citizens starve despite farms producing food.

Beyond these three critical flaws, the audit identified **25+ orphan inputs** (commodities consumed but never produced), **10+ orphan outputs** (commodities produced but never consumed), and a complete absence of modern supply chain commodities (Plastics, Crude Oil refinement, Semiconductors, Batteries, Rare Earth Elements, Lithium) needed for Phase 19 Generative Blueprints.

---

## PART 1 — The Dead-End Analysis

### Methodology

Two independent production-system sources were audited:

| Source | File | Role |
|--------|------|------|
| **Registry** | `state/src/registries/production_methods_data.rs` | Defines tech-unlocked production methods per sector (used when companies research/switch methods) |
| **Generator** | `state/src/engine/generator/corporate.rs` → `sector_recipe()` | Defines the `ActiveProductionMethod` seeded onto each building at world birth (the method the building actually uses from turn 0) |

**Critical architectural note:** The generator creates its own `ActiveProductionMethod` via `sector_recipe()` and writes it directly to `Building.active_method`. The simulation engine (`b2b_orders.rs::execute_production_cycle`) reads `building.active_method` — NOT the registry. The registry methods are only loaded when a company researches a tech and switches methods. This means **the generator's recipes are the de facto turn-0 economy**, and any commodity not produced by `sector_recipe` does not exist at world start.

Additional consumption sources audited:
- `state/src/construction/bom.rs` — construction bill of materials (B2B consumption for building projects)
- `state/src/data/consumption_registry.rs` — B2C consumer demand baskets
- `state/src/economy/retail_registry.rs` — retail store commodity eligibility
- `state/src/economy/b2b_orders.rs` — B2B order submission (fixed-asset purchase bids, maintenance service bids)

### 1.1 Orphan Inputs — Commodities Consumed but NEVER Produced

These commodities appear as inputs in production methods (registry or generator) or in construction BOMs, but have **zero production methods** generating them as outputs in either the registry or the generator.

#### Tier 1: CRITICAL — Fixed-Asset Machinery with Zero Supply

These are the direct cause of the "0 Fixed Asset Cohorts" bug. They are `is_fixed_asset() == true` commodities that are consumed as B2B inputs but never produced.

| Commodity | Consumed By | Production Sources | Impact |
|-----------|------------|-------------------|--------|
| **ConstructionMachinery** | Construction PM (5.0–8.0/1k workers), Construction automation (3.0), Construction BOM (10–80 units/building), Chemical BOM (60 units) | **NONE** — zero in registry, zero in generator | No construction projects can complete; no fixed-asset cohorts for construction sector |
| **AgriculturalMachinery** | Agriculture PM (3.0/1k workers), Generator Agriculture recipe (2.0) | **NONE** — zero in registry, zero in generator | Agricultural modernization impossible; no fixed-asset cohorts for agriculture |
| **OfficeMachinery** | Public Services PM (3.0/1k workers), Generator PublicServices recipe (2.0) | **NONE** — zero in registry, zero in generator | Government offices cannot modernize; no fixed-asset cohorts for public services |
| **Trucks** | Generator ExportServices recipe (2.0) | **NONE** — zero in registry, zero in generator | Export services cannot procure trucks; no fixed-asset cohorts for transport |
| **IndustrialMachinery** | Generator Mining recipe (5.0) | Generator HeavyIndustry (20.0), Registry heavy_industry "Electrified Factories" (15.0, tech `elecf_001` year 1910), "CNC Manufacturing" (30.0, tech `auto3_004` year 1970) | **Partially supplied** — only if HeavyIndustry sector is seeded with non-zero employment AND world start year ≥ 1910 for registry methods. Generator recipe works at any year but only if HeavyIndustry companies exist. |

**Root cause of 0 cohorts:** `submit_fixed_asset_purchase_bids()` (b2b_orders.rs l.1139) correctly submits buy bids for fixed-asset commodities found in `building.active_method.inputs`. The production cycle (l.777) correctly produces fixed-asset outputs into inventory. Sell asks (l.303) correctly list them. But for 3 of 4 machinery types, **no building in the world has them in `active_method.outputs`**, so no sell asks are ever submitted. The buy bids go unfilled forever. For IndustrialMachinery, sell asks exist only if HeavyIndustry was seeded — and even then, the supply is a single sector with no competition.

#### Tier 2: CRITICAL — Fundamental Intermediates with Zero Supply

These commodities are consumed by multiple sectors but have no production source. They block entire production chains.

| Commodity | Consumed By (sectors) | Production Sources | Impact |
|-----------|----------------------|-------------------|--------|
| **MechanicalComponents** | Mining, Agriculture, HeavyIndustry, Armaments, Transport, Energy, MaintenanceWorkshops (8 sectors) | **NONE** | Blocks all mechanized production methods; maintenance workshops cannot function |
| **ElectronicComponents** | Mining, HeavyIndustry, LightIndustry, Armaments, Energy, Transport, Media, Medical, Education, PublicServices, MaintenanceWorkshops (11 sectors) | **NONE** | Blocks all electrified/automated methods; modern economy impossible |
| **Software** | Mining, Agriculture, HeavyIndustry, LightIndustry, Armaments, Transport, Media, Medical, Education, PublicServices, MaintenanceWorkshops (11 sectors) | **NONE** | Blocks all computerized methods; knowledge economy impossible |
| **Chemicals** | Mining, Agriculture, Armaments, Medical, Education (5 sectors) | **NONE** | Blocks froth flotation, GM crops, munitions, medicine, research |
| **Fibers** | LightIndustry (clothing production) | **NONE** | Blocks all clothing production — clothing is a B2C demand good |
| **Timber** | Construction, Energy (biomass) | **NONE** | Blocks basic construction; blocks biomass energy |
| **Planks** | LightIndustry, Construction | **NONE** | Blocks furniture, construction |
| **Stone** | Construction | **NONE** | Blocks construction |
| **Bricks** | Construction | **NONE** | Blocks construction |
| **Glass** | LightIndustry, Construction BOM | **NONE** | Blocks appliance/furniture production, construction |
| **Sand** | Construction BOM (cement plants) | **NONE** | Blocks cement plant construction; needed for Silicon production |
| **NaturalGas** | Energy (Combined Cycle Plant, generator recipe) | **NONE** | Blocks gas power generation |
| **BrownCoal** | Energy (generator recipe) | **NONE** | Blocks lignite power generation |

#### Tier 3: IMPORTANT — Sector-Specific Inputs with Zero Supply

| Commodity | Consumed By | Production Sources | Impact |
|-----------|------------|-------------------|--------|
| **Seeds** | Agriculture (all methods) | **NONE** | Blocks all agriculture — no crops without seeds |
| **Fertilizers** | Agriculture (modern methods) | **NONE** | Blocks modern farming methods |
| **Livestock** | Agriculture (Horse-Drawn Machinery) | **NONE** | Blocks early mixed farming |
| **Pharmaceuticals** | Medical Services (all methods) | **NONE** | Blocks all healthcare beyond basic practice |
| **MedicalEquipment** | Medical Services (modern methods) | **NONE** | Blocks modern surgery, diagnostics |
| **Silicon** | Energy (Solar Power Plant) | **NONE** | Blocks solar power; blocks semiconductor chain |
| **Water** | Energy, Agriculture (Hydroponics) | **NONE** | Assumed free natural resource, but no production method exists. Blocks hydroponics. |

#### Tier 4: Construction BOM Orphans

The construction BOM (`construction/bom.rs`) requires these commodities for building projects. Several are already listed above, but the BOM amplifies their criticality:

| Commodity | BOM Usage | Production Sources |
|-----------|-----------|-------------------|
| ConstructionMachinery | 10–80 units per building (ALL building types) | **NONE** |
| Stone | 600 units (cement plants) | **NONE** |
| Sand | 400 units (cement plants) | **NONE** |
| Glass | 20–200 units (all building types) | **NONE** |
| Bricks | 50–300 units (all building types) | **NONE** |
| Timber | 100–300 units (all building types) | **NONE** |

**Every single construction project in the game requires ConstructionMachinery, which has zero supply.** This means no new buildings can ever be built, locking the economy at its initial state forever.

### 1.2 Orphan Outputs — Commodities Produced but NEVER Consumed

These commodities appear as outputs in production methods but have no consumption path — not as production method inputs, not in construction BOMs, not in B2C demand, and not in fixed-asset/maintenance systems.

#### Critical: Commodity Name Mismatches

The generator produces old-era commodity names that do not match the consumption registry's new-era names. These are the most damaging orphan outputs because they represent **food that citizens cannot eat**.

| Produced Commodity | Produced By | Intended Consumer Demand | Mismatch? |
|-------------------|-------------|------------------------|-----------|
| **Grains** | Generator Agriculture (60.0) | Consumption registry demands **Cereal** | **YES — different enum variants. `Grains` ≠ `Cereal`** |
| **Vegetables** (plural) | Generator Agriculture (20.0) | Consumption registry demands **Vegetable** (singular) | **YES — different enum variants. `Vegetables` ≠ `Vegetable`** |

The registry's agriculture methods correctly produce `Cereal` and `Vegetable`, but the generator's recipe produces `Grains` and `Vegetables`. Since the generator's recipe is what buildings use at turn 0, the food supply chain is broken from the start.

#### Other Orphan Outputs

| Commodity | Produced By | Consumption Path | Verdict |
|-----------|------------|-----------------|---------|
| **Meat** | Generator Agriculture (30.0) | Not in consumption registry (registry demands `Protein`, not `Meat`). Retail registry allows Butcher to sell `Meat`, but no B2C demand exists for it. | Orphan — no demand |
| **Fruit** | Generator Agriculture (15.0) | Not in consumption registry. Retail registry allows Grocery to sell `Fruit`, but no B2C demand. | Orphan — no demand |
| **Gold** | Generator Mining (2.0) | Not consumed by any method. Not in B2C demand. May be intended for strategic reserve. | Orphan — no consumption path |
| **Copper** | Registry mining (Froth Flotation, 12.0) | Not consumed by any production method. Not in B2C demand. Used in blueprint specs as a potential material but not in any active BOM. | Orphan — no consumption path |
| **Aluminum** | Generator HeavyIndustry (15.0) | Not consumed by any production method. Used in blueprint specs (IndustrialMachinery ideal material) but blueprint BOM is separate from production method inputs. | Orphan in production-method system |
| **CargoShips** | Generator ExportServices (10.0) | Not consumed by any method. Not in B2C demand. | Orphan — no consumption path |
| **BankingServices** | Generator ExportServices (12.0) | Not consumed by any method. Not in B2C demand. May be consumed by banking system internally. | Likely orphan |
| **Prefabricates** | Generator Construction (25.0) | Not consumed by any method. Not in B2C demand. | Orphan — no consumption path |
| **PassengerTransport** | Generator ExportServices (15.0), Registry transport methods | Not in consumption registry. May be consumed by transport system. | Likely orphan in B2C |
| **Radio** | Registry media (Radio Broadcasting, 20.0) | Quality durable (`is_quality_durable`), but not in consumption registry. Retail-sellable but no B2C demand. | Orphan in demand system |
| **Televisions** | Registry media (Television Broadcasting, 15.0+) | Quality durable, but not in consumption registry. | Orphan in demand system |

#### Military Goods (Special Case — Not True Orphans)

| Commodity | Produced By | Consumption Path |
|-----------|------------|-----------------|
| TowedArtillery, LightTanks, Rifles, Ammunition, SupportEquipment | Registry armaments methods | Consumed by military procurement system (government defense purchasing), not by production methods. **Not orphan** — but the armaments sector has no generator recipe, so these are never produced at turn 0. |

### 1.3 Generator Gaps — `engine/generator/corporate.rs`

#### Gap 1: `sector_recipe()` Only Covers 10 of 21 Sectors

The `sector_recipe()` function (corporate.rs l.707) has explicit `match` arms for only 10 sectors. The remaining 11 fall through to a `_` catch-all that returns **empty inputs and empty outputs**:

```rust
_ => {
    let name = "Production Facility".to_string();
    (name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year))
}
```

| Sector | Has `sector_recipe`? | Has Registry Methods? | Turn-0 Output |
|--------|---------------------|----------------------|---------------|
| Agriculture | ✅ Yes | ✅ Yes | Grains, Meat, Vegetables, Fruit |
| Mining | ✅ Yes | ✅ Yes | Oil, Iron, Copper, Gold |
| HeavyIndustry | ✅ Yes | ✅ Yes | Steel, Cement, IndustrialMachinery, Fuels, Aluminum |
| LightIndustry | ✅ Yes | ✅ Yes | Clothing, Furniture, Paper, Agd, Food |
| LocalServices | ✅ Yes | ❌ No | LocalServicesCommodity |
| ExportServices | ✅ Yes | ❌ No | PassengerTransport, CargoShips, BankingServices |
| Construction | ✅ Yes | ✅ Yes | ConstructionServices, Prefabricates |
| Energy | ✅ Yes | ✅ Yes | Energy |
| PublicServices | ✅ Yes | ✅ Yes | AdministrativeServices |
| Hospitality | ✅ Yes | ❌ No | LocalServicesCommodity |
| **ArmamentsIndustry** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **TransportLogistics** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **MediaAndEntertainment** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **MedicalServices** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **EducationalServices** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **MaintenanceWorkshops** | ❌ No (catch-all) | ✅ Yes | **NOTHING** |
| **Banking** | ❌ No (catch-all) | ❌ No | **NOTHING** |
| **PublicAdministration** | ❌ No (catch-all) | ❌ No | **NOTHING** |
| **NGO** | ❌ No (catch-all) | ❌ No | **NOTHING** |
| **Religion** | ❌ No (catch-all) | ❌ No | **NOTHING** |
| **WasteManagement** | ❌ No (catch-all) | ❌ No | **NOTHING** (state-owned landfills generated separately) |

**Impact:** 11 sectors produce nothing at world birth. This means:
- No `HealthCapacity` is produced → citizens have no healthcare
- No `EducationSlots` is produced → citizens have no education
- No `MaintenanceServices` is produced → fixed assets cannot be repaired
- No military goods are produced → defense industry is stillborn
- No `PassengerTransport` is produced (from transport sector) → transport services absent
- No `Radio`/`Televisions` are produced (from media sector) → media goods absent

The registry has proper methods for 7 of these 11 sectors (armaments, transport, media, medical, education, public_services, maintenance_workshops), but the generator never loads them.

#### Gap 2: No Minimum-Viable-Supply-Chain Guarantee

The generator iterates `country.budget.sectors` and creates companies proportional to each sector's `zatrudnienie` (employment) allocation:

```rust
for (&sector, share) in &country.budget.sectors {
    let target_emp = share.extra.get("zatrudnienie")...unwrap_or(0).max(0);
    if target_emp == 0 { continue; }  // <-- skips sectors with 0 employment
    ...
}
```

**There is no guarantee that:**
- Every fundamental output commodity has at least one producer
- Every sector with registry methods gets at least one building
- The supply chain is acyclic and bootstrappable
- Critical sectors (Mining, Energy, Agriculture, HeavyIndustry) get minimum representation

If a sector has 0 employment in the budget, it is silently skipped. A world could be generated with no mining, no energy, and no heavy industry — and the generator would not complain.

#### Gap 3: Generator Recipes Diverge from Registry Methods

The generator's `sector_recipe` creates hardcoded `ActiveProductionMethod` structs that do not correspond to any method in `production_methods_data.rs`. This creates two parallel, inconsistent economies:

- **Generator economy (turn 0):** HeavyIndustry produces 5 outputs (Steel, Cement, IndustrialMachinery, Fuels, Aluminum) from 4 inputs. Mining produces 4 outputs from 3 inputs.
- **Registry economy (post-tech-research):** HeavyIndustry produces only Steel (6 methods) or IndustrialMachinery (2 methods). Mining produces only HardCoal (7 methods) or Copper (1 method).

When a company researches a tech and switches to a registry method, its output profile changes dramatically — often losing outputs that other sectors depend on. For example, a HeavyIndustry building that switches from the generator recipe to "Bessemer Converters" stops producing `Cement`, `IndustrialMachinery`, `Fuels`, and `Aluminum` — commodities that other sectors need.

#### Gap 4: No Fixed-Asset Seeding at World Birth

Buildings are generated with `fixed_assets: Vec::new()` (empty). The only way to acquire fixed assets is through the B2B market (`submit_fixed_asset_purchase_bids`), which requires:
1. A building with a fixed-asset commodity in `active_method.inputs` (the buyer)
2. A building with a fixed-asset commodity in `active_method.outputs` (the seller)
3. A successful market match between bid and ask

Since 3 of 4 machinery types have zero producers (Gap in 1.1), and the 4th (IndustrialMachinery) depends on HeavyIndustry being seeded, most buildings can never acquire fixed assets. The `machinery_factor` stays at 1.0 (manual mode baseline) forever, and the Phase 19B obsolescence/maintenance system has nothing to act on.

---

## PART 2 — Missing Real-World Resources Blueprint

### 2.1 Current Commodity Coverage Assessment

The `Commodity` enum has 128 variants. Many represent a 19th–20th century Polish economy (the original Python project's setting). For Phase 19 Generative Blueprints to model a modern supply chain, critical commodities are missing.

### 2.2 Existing Commodities That Are Dead (Enum Variants with No Production or Consumption)

These enum variants exist but are completely unused — no method produces them, no method consumes them, no BOM uses them, no B2C demand references them:

| Commodity | Enum Variant | Status |
|-----------|-------------|--------|
| Bauxite | `Bauxite` | Dead — never produced, never consumed. Should be Aluminum ore. |
| Catalysts | `Catalysts` | Dead — never produced, never consumed. Needed for petroleum refining. |
| Coke | `Coke` | Dead — never produced, never consumed. Should be Steel-making input from coal. |
| Ammonia | `Ammonia` | Dead — never produced, never consumed. Needed for fertilizers. |
| SodaAsh | `SodaAsh` | Dead — never produced, never consumed. Needed for glass/chemicals. |
| Sulfur | `Sulfur` | Dead — never produced, never consumed. Needed for chemicals. |
| Tin | `Tin` | Dead — never produced, never consumed. Alloy metal. |
| Zinc | `Zinc` | Dead — never produced, never consumed. Alloy metal. |
| Lead | `Lead` | Dead — never produced, never consumed. Needed for batteries. |
| Magnesium | `Magnesium` | Dead — never produced, never consumed. Light metal. |
| Silver | `Silver` | Dead — never produced, never consumed. Precious metal. |
| Peat | `Peat` | Dead — never produced, never consumed. Fuel source. |
| Clay | `Clay` | Dead — never produced, never consumed. Needed for bricks/cement. |
| Limestone | `Limestone` | Dead — never produced, never consumed. Needed for cement. |
| Gravel | `Gravel` | Dead — never produced, never consumed. Construction material. |
| Hydrogen | `Hydrogen` | Dead — never produced, never consumed. Future fuel. |
| Bitumen | `Bitumen` | Dead — never produced, never consumed. Road construction. |
| Asphalt | `Asphalt` | Dead — never produced, never consumed. Road construction. |
| MineralResources | `MineralResources` | Dead — generic catch-all, never used. |
| Gunpowder | `Gunpowder` | Dead — never produced, never consumed. Early munitions. |
| Salt | `Salt` | Dead — never produced, never consumed. Food/chemical input. |
| Heat | `Heat` | Dead — never produced, never consumed. District heating. |
| MarketResearch | `MarketResearch` | Dead — never produced, never consumed. |
| RenovationServices | `RenovationServices` | Dead — never produced, never consumed. |
| Information | `Information` | Dead — never produced, never consumed. Phase 18C media service. |
| ReligiousTexts | `ReligiousTexts` | Dead — never produced, never consumed. |
| ReligiousArt | `ReligiousArt` | Dead — never produced, never consumed. |
| InsuranceServices | `InsuranceServices` | Dead — never produced, never consumed. |
| Various capacity commodities | JusticeCapacity, SecurityCapacity, IntelligenceCapacity, FireProtectionCapacity, ShelterCapacity, BorderEnforcementCapacity, CustomsCapacity, SanitaryInspectionCapacity, BuildingInspectionCapacity, EnvironmentalInspectionCapacity, AssimilationCapacity | Produced by state buildings (not via production_methods_data.rs), consumed by government systems. Not orphan but outside this audit's scope. |

**That's 25+ dead enum variants** — commodities that exist in the type system but have no economic life. Many of these should be activated as part of the modern supply chain.

### 2.3 Proposed New Commodities to Add

The following modern commodities do not exist in the `Commodity` enum and must be added for a realistic supply chain:

| New Commodity | Proposed Enum Name | Category | Purpose |
|---------------|-------------------|----------|---------|
| **Rare Earth Elements** | `RareEarthElements` | Raw Material | Input for semiconductors, batteries, advanced electronics |
| **Lithium** | `Lithium` | Raw Material | Input for batteries |
| **Plastics** | `Plastics` | Intermediate | Produced from Crude Oil; input for Agd, Cars, Electronics, Packaging |
| **RefinedFuel** | `RefinedFuel` | Intermediate | Distinct from `Fuels` (which is generic); produced from Crude Oil via refinery |
| **Semiconductors** | `Semiconductors` | Intermediate | Produced from Silicon + Rare Earth Elements; input for ElectronicComponents, Solar, advanced machinery |
| **Batteries** | `Batteries` | Intermediate | Produced from Lithium + Semiconductors; input for EVs, electronics, energy storage |

**Note on `Oil` vs Crude Oil:** The enum already has `Commodity::Oil` (serde: `"oil"`, Polish: "Ropa Naftowa" = Crude Oil). This IS crude oil. The issue is that no registry method produces it (only the generator's Mining recipe does) and no method refines it into `Fuels` or `RefinedFuel`. The existing `Fuels` commodity should be treated as the refined product.

**Note on `Sand` vs Silica:** The enum has `Commodity::Sand` (serde: `"sand"`, Polish: "Piasek"). Silica is high-purity sand. We can either use `Sand` as the silicon ore input or add a separate `Silica` commodity. Recommendation: use `Sand` as the raw input for Silicon production (purification), avoiding a new enum variant.

**Note on `Bauxite`:** The enum already has `Commodity::Bauxite`. It should be the ore input for Aluminum production (Bayer process → alumina → smelting → aluminum). Currently dead — needs activation.

### 2.4 Proposed Modern Supply Chain Map

Below is the complete proposed supply chain showing how every commodity should be produced and consumed. **Existing but dead commodities are marked [DEAD], new commodities are marked [NEW], existing active commodities are unmarked.**

#### Layer 0: Primary Extraction (Mining Sector)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| HardCoal | Fuels/Energy, Food | Existing — keep |
| BrownCoal [DEAD→ACTIVATE] | Fuels/Energy, Food | Activate for lignite power |
| Iron | Fuels/Energy, Food | Existing — keep |
| Copper | Energy, Chemicals | Existing — keep |
| **Oil** (Crude Oil) | Fuels/Energy, Food | Activate in registry (currently only in generator) |
| **Bauxite** [DEAD→ACTIVATE] | Energy, Fuels | Activate as Aluminum ore |
| **Sand** [DEAD→ACTIVATE] | Energy, Fuels | Activate as Silicon feedstock + construction material |
| **Stone** [DEAD→ACTIVATE] | Energy, Fuels | Activate for construction |
| **Clay** [DEAD→ACTIVATE] | Energy, Fuels | Activate for bricks/cement |
| **Limestone** [DEAD→ACTIVATE] | Energy, Fuels | Activate for cement |
| **Gravel** [DEAD→ACTIVATE] | Energy, Fuels | Activate for construction |
| **Sulfur** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for chemical industry |
| **Salt** [DEAD→ACTIVATE] | Energy, Fuels | Activate for chemicals/food |
| **Tin** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for alloys/electronics |
| **Zinc** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for alloys/galvanizing |
| **Lead** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for batteries |
| **Magnesium** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for light alloys |
| **Silver** [DEAD→ACTIVATE] | Energy, Chemicals | Activate for electronics/jewelry |
| **Rare Earth Elements** [NEW] | Energy, Chemicals, MechanicalComponents | New — critical for modern electronics |
| **Lithium** [NEW] | Energy, Fuels, Water | New — critical for batteries |
| **NaturalGas** [DEAD→ACTIVATE] | Energy, Fuels | Activate as mining byproduct/extract |
| **Peat** [DEAD→ACTIVATE] | Fuels, Food | Activate as low-grade fuel |
| Gold | Energy, Fuels, Chemicals | Existing — keep (strategic reserve / monetary) |

#### Layer 1: Smelting & Basic Processing (Heavy Industry Sector)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| Steel | Iron, HardCoal/Coke, Energy | Existing — keep |
| **Coke** [DEAD→ACTIVATE] | HardCoal, Energy | Activate as steel-making input (coal → coke) |
| Cement | Limestone, Clay, Energy | Activate (currently only in generator, not registry) |
| Bricks | Clay, Energy | Activate |
| Glass | Sand, SodaAsh, Energy | Activate |
| **Aluminum** | Bauxite, Energy (huge electricity), Catalysts | Activate full chain (Bauxite → Aluminum) |
| **Silicon** | Sand, Energy, Chemicals | Activate (silica purification) |
| **Copper** (refined) | Copper ore, Energy | Existing — keep |
| **Semiconductors** [NEW] | Silicon, RareEarthElements, Chemicals, Energy | New — advanced intermediate |

#### Layer 2: Chemical & Petroleum Processing (Heavy Industry / Chemical sub-sector)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| **Chemicals** | Sulfur, Salt, Water, Energy | Activate full chain |
| **SodaAsh** [DEAD→ACTIVATE] | Salt, Limestone, Ammonia, Energy | Activate (Solvay process) |
| **Ammonia** [DEAD→ACTIVATE] | NaturalGas/Hydrogen, Energy | Activate (Haber-Bosch) |
| **Fertilizers** | Ammonia, Chemicals, Energy | Activate full chain |
| **Plastics** [NEW] | Oil, Chemicals, Energy | New — from petroleum |
| **RefinedFuel** [NEW] | Oil, Catalysts, Energy | New — from crude oil |
| **Fuels** (generic) | Oil, Energy | Activate refinery chain (Oil → Fuels) |
| **Bitumen** [DEAD→ACTIVATE] | Oil, Energy | Activate (road construction byproduct of refining) |
| **Asphalt** [DEAD→ACTIVATE] | Bitumen, Sand/Gravel, Energy | Activate (road construction) |
| **Catalysts** [DEAD→ACTIVATE] | Chemicals, RareEarthElements, Energy | Activate (for refining/chemical processes) |
| **Hydrogen** [DEAD→ACTIVATE] | NaturalGas, Energy (electrolysis) | Activate (future fuel/chemical input) |

#### Layer 3: Components & Parts Manufacturing (Heavy Industry / Light Industry)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| **MechanicalComponents** | Steel, Energy, IndustrialMachinery | **CRITICAL — must activate** (consumed by 8 sectors) |
| **ElectronicComponents** | Semiconductors, Copper, Tin, Energy | **CRITICAL — must activate** (consumed by 11 sectors) |
| **Software** | ElectronicComponents, Energy, Food | **CRITICAL — must activate** (consumed by 11 sectors; represents IT services) |
| **Batteries** [NEW] | Lithium, Lead, Semiconductors, Energy | New — for EVs, electronics, energy storage |
| Planks | Timber, Energy | Activate (sawmill) |
| Timber | (Forestry — see below) | Activate |
| Paper | Planks/Timber, Chemicals, Water, Energy | Activate |

#### Layer 4: Forestry & Agriculture Inputs

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| **Timber** | Energy, Fuels, MechanicalComponents | Activate (forestry/sawmill — should be a Mining or Agriculture sub-method) |
| **Seeds** | Cereal/Protein output, Energy | Activate (seed production from agriculture) |
| **Fibers** | IndustrialFiber, Energy | Activate (textile mill) or from Agriculture |
| **Livestock** | Fodder, Water, Food | Activate (ranching) |
| **Fodder** [DEAD→ACTIVATE] | Cereal, Water | Activate (animal feed from crops) |
| **IndustrialFiber** | Seeds, Water, Energy | Activate (cotton/flax/hemp farming) |
| **Water** | (Free natural resource or infrastructure output) | Activate as utility output |

#### Layer 5: Investment Goods (Final Machinery — THE CRITICAL GAP)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| **IndustrialMachinery** | Steel, MechanicalComponents, ElectronicComponents, Aluminum, Energy | **CRITICAL — must have early-game method (pre-1910)** |
| **ConstructionMachinery** | Steel, MechanicalComponents, Energy, Fuels | **CRITICAL — must add production method (currently ZERO)** |
| **AgriculturalMachinery** | Steel, MechanicalComponents, Energy, Fuels | **CRITICAL — must add production method (currently ZERO)** |
| **OfficeMachinery** | Steel, ElectronicComponents, MechanicalComponents, Energy | **CRITICAL — must add production method (currently ZERO)** |
| **Trucks** | Steel, MechanicalComponents, ElectronicComponents, Fuels, Energy | **CRITICAL — must add production method (currently ZERO)** |
| **Cars** | Steel, Aluminum, ElectronicComponents, Plastics, Rubber, Energy | **CRITICAL — must add production method (currently ZERO)** |

#### Layer 6: Consumer Goods (Light Industry)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| Clothing | Fibers, Energy | Existing — keep |
| Furniture | Planks, Steel, Glass, Energy | Activate |
| Agd (appliances) | Steel, ElectronicComponents, Plastics, Energy | Activate |
| Food | Cereal, Vegetable, Protein, Energy | Activate (food processing) |
| Televisions | ElectronicComponents, Glass, Plastics, Energy | Activate |
| Radio | ElectronicComponents, Energy | Activate |

#### Layer 7: Military Goods (Armaments Industry)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| Rifles, Ammunition, etc. | Steel, Chemicals, MechanicalComponents, Fuels | Existing — keep, but generator must seed this sector |

#### Layer 8: Services (Service Sectors)

| Output | Input Requirements | Notes |
|--------|-------------------|-------|
| Energy | HardCoal/BrownCoal/NaturalGas/Oil/Water/Silicon | Existing — keep |
| HealthCapacity | Pharmaceuticals, MedicalEquipment, Chemicals, Energy | Existing registry — generator must seed |
| EducationSlots | Paper, Chemicals, ElectronicComponents, Software | Existing registry — generator must seed |
| AdministrativeServices | Paper, OfficeMachinery, Software | Existing registry — generator must seed |
| MaintenanceServices | Steel, MechanicalComponents, ElectronicComponents, Energy | Existing registry — generator must seed |
| PassengerTransport | Fuels/Energy, Steel, MechanicalComponents | Existing registry — generator must seed |
| ConstructionServices | Steel, Cement, Bricks, Planks, Stone, ConstructionMachinery | Existing registry — generator must seed |

### 2.5 Proposed Production Method Additions for Fixed Assets

The following new production methods must be added to `production_methods_data.rs` for the heavy_industry sector (or a new machinery sub-sector) to produce the missing fixed assets:

#### IndustrialMachinery (needs early-game method)
- **"Machine Shop"** (1880, no tech): Steel + MechanicalComponents + Energy → IndustrialMachinery (10.0). Early-game baseline.
- **"Electrified Factories"** (1910, elecf_001): existing — keep
- **"CNC Manufacturing"** (1970, auto3_004): existing — keep
- **"Smart Manufacturing"** (1995, advman_006): Steel + ElectronicComponents + Software + Semiconductors + Energy → IndustrialMachinery (50.0)

#### ConstructionMachinery (needs ALL methods — currently zero)
- **"Blacksmith Shop"** (1880, no tech): Steel + Iron + Fuels → ConstructionMachinery (8.0). Early-game.
- **"Machine Factory"** (1910, mech_008): Steel + MechanicalComponents + Energy → ConstructionMachinery (20.0)
- **"Heavy Equipment Plant"** (1950, auto3_001): Steel + MechanicalComponents + ElectronicComponents + Energy → ConstructionMachinery (40.0)
- **"Automated Equipment Plant"** (1990, advman_004): Steel + MechanicalComponents + ElectronicComponents + Software + Energy → ConstructionMachinery (70.0)

#### AgriculturalMachinery (needs ALL methods — currently zero)
- **"Blacksmith Shop"** (1880, no tech): Steel + Iron + Fuels → AgriculturalMachinery (8.0)
- **"Implement Factory"** (1910, mech_008): Steel + MechanicalComponents + Energy → AgriculturalMachinery (20.0)
- **"Tractor Plant"** (1950, auto3_001): Steel + MechanicalComponents + ElectronicComponents + Energy → AgriculturalMachinery (40.0)
- **"Precision Ag Equipment"** (1990, advman_004): Steel + MechanicalComponents + ElectronicComponents + Software + Energy → AgriculturalMachinery (70.0)

#### OfficeMachinery (needs ALL methods — currently zero)
- **"Typewriter Workshop"** (1890, mech_008): Steel + MechanicalComponents + Energy → OfficeMachinery (10.0)
- **"Office Equipment Factory"** (1950, auto3_001): Steel + MechanicalComponents + ElectronicComponents + Energy → OfficeMachinery (25.0)
- **"Computer Factory"** (1980, auto3_004): Steel + ElectronicComponents + Semiconductors + Software + Energy → OfficeMachinery (50.0)

#### Trucks (needs ALL methods — currently zero)
- **"Wagon Workshop"** (1880, no tech): Steel + Timber + MechanicalComponents + Fuels → Trucks (5.0)
- **"Truck Assembly"** (1920, auto_001): Steel + MechanicalComponents + Fuels + Energy → Trucks (15.0)
- **"Modern Truck Plant"** (1960, auto3_002): Steel + MechanicalComponents + ElectronicComponents + Fuels + Energy → Trucks (35.0)
- **"Electric Truck Plant"** (2000, advman_006): Steel + Aluminum + ElectronicComponents + Batteries + Energy → Trucks (60.0)

#### Cars (needs ALL methods — currently zero)
- **"Coachbuilder"** (1900, mech_008): Steel + Timber + MechanicalComponents + Fuels → Cars (5.0)
- **"Assembly Line"** (1913, auto_001): Steel + MechanicalComponents + Fuels + Energy → Cars (20.0)
- **"Modern Auto Plant"** (1960, auto3_003): Steel + MechanicalComponents + ElectronicComponents + Plastics + Fuels → Cars (50.0)
- **"EV Factory"** (2010, advman_006): Steel + Aluminum + ElectronicComponents + Semiconductors + Batteries + Energy → Cars (80.0)

### 2.6 Blueprint Spec Updates for New Materials

The `blueprint_specs.rs` should be updated to incorporate the new modern materials:

- **IndustrialMachinery**: Add Semiconductors as a high-quality substitute for ElectronicComponents (quality > 1.0, durability > 1.0)
- **Cars**: Add Plastics (lighter, cheaper, lower durability), Aluminum (lighter, higher quality), Batteries (for EV variant)
- **Trucks**: Add Plastics, Batteries for modern variants
- **AGD**: Add Plastics as ideal for housing/shell role
- **Televisions**: Add Semiconductors and Plastics as ideal materials

---

## PART 3 — The Generator Bootstrapping Plan

### 3.1 Problem Statement

The current `generate_corporate_entities` function creates a "plausible" economy by distributing companies across sectors proportional to budget employment shares. It does NOT guarantee a minimum viable supply chain. The result is a world where critical commodities may have zero producers, causing the economy to freeze.

### 3.2 Design Principles for the New Bootstrapping Strategy

1. **Supply-chain completeness over realism:** Every commodity that is consumed by any production method, construction BOM, or B2C demand must have at least one producer at world birth.
2. **Acyclic bootstrapping:** The production graph must be acyclic at turn 0 — no commodity's production should depend on a commodity that itself has no producer. Primary extraction (mining, forestry) must have only labor + fuel/energy inputs (which are self-supplied).
3. **Minimum viable scale:** Each critical sector gets at least one building per region, regardless of budget employment share. The budget determines the *scale* of the sector, not its *existence*.
4. **Generator-registry alignment:** The generator should pull initial production methods FROM the registry (`production_methods_data.rs`) rather than inventing its own `sector_recipe` methods. This eliminates the parallel-economy problem.
5. **Fixed-asset seeding:** New worlds should seed buildings with a small initial `FixedAssetCohort` (condition 0.7–1.0, base tech year = start_year) so the economy doesn't start at zero machinery capacity. This represents pre-existing capital stock.

### 3.3 Proposed Strategy: Three-Phase Bootstrapping

#### Phase A: Minimum Viable Supply Chain Seeding (Pre-Budget)

Before the existing budget-proportional generation runs, execute a **minimum viable supply chain seeding pass** that guarantees at least one building per region for each critical sector:

```
For each region in country:
    For each sector in CRITICAL_SECTORS:
        Create 1 building with:
            - worker_capacity = max(region.population * sector_min_ratio, MIN_WORKERS)
            - active_method = best_registry_method(sector, start_year)  // FROM REGISTRY
            - fixed_assets = [seed_cohort(sector, start_year)]  // Initial machinery
            - inventory = seed_inventory(sector)  // One turn of inputs
```

**Critical sectors that MUST be seeded (minimum 1 building per region):**

| Priority | Sector | Reason |
|----------|--------|--------|
| P0 | Mining | Produces Iron, HardCoal, Oil, Bauxite, Sand, Stone, Clay, etc. — foundation of all industry |
| P0 | Energy | Produces Energy — required by every other sector |
| P0 | Agriculture | Produces Cereal, Vegetable, Protein — citizen survival |
| P0 | HeavyIndustry | Produces Steel, Cement, MechanicalComponents, IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery — the investment goods bottleneck |
| P1 | LightIndustry | Produces Clothing, Furniture, Paper, Food — B2C demand |
| P1 | Construction | Produces ConstructionServices — needed for expansion |
| P1 | MaintenanceWorkshops | Produces MaintenanceServices — needed for fixed-asset upkeep |
| P2 | TransportLogistics | Produces PassengerTransport — citizen mobility |
| P2 | MedicalServices | Produces HealthCapacity — citizen health (B2C demand) |
| P2 | EducationalServices | Produces EducationSlots — citizen education (B2C demand) |
| P2 | PublicServices | Produces AdministrativeServices — government function |
| P3 | ArmamentsIndustry | Produces military goods — defense |
| P3 | MediaAndEntertainment | Produces Radio, Televisions — B2C durables |
| P3 | ExportServices | Produces export capacity |
| P3 | Banking | Produces BankingServices — financial system |

#### Phase B: Budget-Proportional Scaling (Existing Logic, Refined)

After the minimum viable supply chain is seeded, run the existing budget-proportional generation to scale sectors up to their target employment. This adds additional companies and buildings on top of the minimum seeded ones.

**Refinement:** The existing `sector_recipe()` function should be **replaced** with a call to `best_registry_method(sector, start_year)` that pulls the most advanced available method from `production_methods_data.rs` for the given start year. This eliminates the parallel-economy problem and ensures generator-registry alignment.

```
fn best_registry_method(sector: Sector, start_year: u32) -> ActiveProductionMethod {
    let methods = default_production_methods().get(sector_json_name(sector));
    // Find the most advanced Production method (not Automation/Organization)
    // whose required_tech is None or whose tech year <= start_year
    // Fall back to the earliest method if none match
}
```

#### Phase C: Fixed-Asset Cohort Seeding

For each generated building, seed an initial `FixedAssetCohort` based on the sector's machinery requirements:

```
fn seed_cohort(sector: Sector, start_year: u32) -> Option<FixedAssetCohort> {
    let commodity = sector_machinery_commodity(sector);  // e.g. Mining → IndustrialMachinery
    let count = sector_machinery_count(sector);           // e.g. 5 machines
    Some(FixedAssetCohort {
        blueprint_id: "LEGACY_SEED".to_string(),
        commodity,
        count,
        condition: 0.7 + rng.gen::<f64>() * 0.3,  // 0.7–1.0 (worn but functional)
        quality: 0.8,  // Legacy quality
        durability: 200.0,
        base_tech: TechId::from("seed"),
        base_tech_year: start_year.saturating_sub(20),  // 20-year-old tech
        acquired_turn: 0,
    })
}
```

| Sector | Machinery Commodity | Seed Count |
|--------|---------------------|------------|
| Mining | IndustrialMachinery | 5 |
| HeavyIndustry | IndustrialMachinery | 8 |
| Construction | ConstructionMachinery | 5 |
| Agriculture | AgriculturalMachinery | 3 |
| TransportLogistics | Trucks | 5 |
| PublicServices | OfficeMachinery | 2 |
| MedicalServices | MedicalEquipment (not fixed asset) | 0 (use inventory seed) |
| EducationalServices | OfficeMachinery | 2 |
| MaintenanceWorkshops | IndustrialMachinery | 3 |
| Energy | IndustrialMachinery | 5 |
| LightIndustry | IndustrialMachinery | 3 |
| ArmamentsIndustry | IndustrialMachinery | 5 |

### 3.4 Proposed Inventory Seeding

To prevent first-turn starvation (where buildings can't produce because they have no inputs), seed each building with one turn's worth of inputs:

```
fn seed_inventory(sector: Sector) -> BTreeMap<Commodity, f64> {
    let method = best_registry_method(sector, start_year);
    method.inputs.iter().map(|(commodity, qty_per_1k)| {
        // Seed 1 turn of inputs at the building's capacity
        (*commodity, qty_per_1k * (building_capacity as f64 / 1000.0))
    }).collect()
}
```

This gives every building one production cycle of inputs at birth, allowing the economy to bootstrap before B2B markets clear.

### 3.5 Proposed Commodity Name Reconciliation

The generator's `sector_recipe` for Agriculture must be fixed to use the correct commodity names:

| Current (Broken) | Fixed |
|------------------|-------|
| `Commodity::Grains` | `Commodity::Cereal` |
| `Commodity::Vegetables` | `Commodity::Vegetable` |

If `sector_recipe` is replaced with `best_registry_method()` (Phase B), this is automatically fixed because the registry methods already use the correct names.

### 3.6 Proposed Validation Pass

After generation, run a **supply chain validation pass** that verifies:

1. **No orphan inputs:** For every commodity in any building's `active_method.inputs`, there exists at least one building in the world with that commodity in `active_method.outputs`.
2. **No orphan B2C demand:** For every commodity in the consumption registry, there exists at least one building producing it.
3. **No orphan construction BOM:** For every commodity in any construction BOM, there exists at least one building producing it.
4. **Acyclic check:** The production dependency graph (commodity → producer sector → input commodities) has no cycles that lack a primary-extraction entry point.

If validation fails, log a WARNING with the specific orphan commodities and their dependent sectors. This makes future regressions immediately visible.

### 3.7 Summary of Generator Changes

| Change | File | Impact |
|--------|------|--------|
| Replace `sector_recipe()` with `best_registry_method()` | `corporate.rs` | Eliminates parallel-economy problem; fixes 11 empty-recipe sectors |
| Add minimum viable supply chain seeding pass | `corporate.rs` | Guarantees every critical sector has at least 1 building per region |
| Add fixed-asset cohort seeding | `corporate.rs` | Buildings start with functional machinery (Phase 19B has something to act on) |
| Add inventory seeding | `corporate.rs` | Prevents first-turn production starvation |
| Fix commodity name mismatch (Grains→Cereal, Vegetables→Vegetable) | `corporate.rs` | Fixes food supply chain |
| Add post-generation supply chain validation | `corporate.rs` or new module | Catches future regressions |

---

## Appendix A: Complete Commodity Production/Consumption Matrix

Legend: ✅ = has production/consumption, ❌ = no production/consumption, ⚠️ = partial/conditional

| # | Commodity | Produced (Registry) | Produced (Generator) | Consumed (Methods) | Consumed (BOM) | Consumed (B2C) | Verdict |
|---|-----------|--------------------|--------------------|--------------------|----------------|----------------|---------|
| 1 | Agd | ❌ | ✅ LightIndustry | ❌ | ❌ | ❌ (retail only) | Orphan output (no demand) |
| 2 | Aluminum | ❌ | ✅ HeavyIndustry | ❌ (blueprint only) | ❌ | ❌ | Orphan output |
| 3 | Ammunition | ✅ Armaments | ❌ | ❌ | ❌ | ❌ | Military only (no generator seed) |
| 4 | TowedArtillery | ✅ Armaments | ❌ | ❌ | ❌ | ❌ | Military only |
| 5 | MobileArtillery | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 6 | AntiAircraftArtillery | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 7 | Asphalt | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 8 | Bitumen | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 9 | InfantryFightingVehicles | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 10 | Bauxite | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be Al ore) |
| 11 | Bombers | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 12 | Bricks | ❌ | ❌ | ✅ Construction | ✅ BOM | ❌ | **Orphan input** |
| 13 | Cement | ❌ | ✅ HeavyIndustry | ✅ Construction | ✅ BOM | ❌ | Partial (generator only) |
| 14 | Trucks | ❌ | ❌ | ✅ ExportServices | ❌ | ❌ | **Orphan input (fixed asset)** |
| 15 | MilitaryTrucks | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 16 | Tin | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 17 | Zinc | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 18 | HeavyTanks | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 19 | LightTanks | ✅ Armaments | ❌ | ❌ | ❌ | ❌ | Military only |
| 20 | MediumTanks | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 21 | ElectronicComponents | ❌ | ❌ | ✅ (11 sectors) | ❌ | ❌ | **Orphan input (critical)** |
| 22 | MechanicalComponents | ❌ | ❌ | ✅ (8 sectors) | ❌ | ❌ | **Orphan input (critical)** |
| 23 | Planks | ❌ | ❌ | ✅ LightIndustry, Construction | ❌ | ❌ | **Orphan input** |
| 24 | Timber | ❌ | ❌ | ✅ Construction, Energy | ✅ BOM | ❌ | **Orphan input** |
| 25 | Energy | ✅ Energy | ✅ Energy | ✅ (many) | ❌ | ❌ | OK |
| 26 | Frigates | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 27 | NaturalGas | ❌ | ❌ | ✅ Energy | ❌ | ❌ | **Orphan input** |
| 28 | Clay | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be brick/cement input) |
| 29 | Helicopters | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 30 | Stone | ❌ | ❌ | ✅ Construction | ✅ BOM | ❌ | **Orphan input** |
| 31 | Rifles | ✅ Armaments | ❌ | ❌ | ❌ | ❌ | Military only |
| 32 | Catalysts | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be refining input) |
| 33 | Coke | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be steel input) |
| 34 | Silicon | ❌ | ❌ | ✅ Energy (solar) | ❌ | ❌ | **Orphan input** |
| 35 | Cruisers | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 36 | AircraftCarriers | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 37 | Magnesium | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 38 | OfficeMachinery | ❌ | ❌ | ✅ PublicServices | ❌ | ❌ | **Orphan input (fixed asset)** |
| 39 | ConstructionMachinery | ❌ | ❌ | ✅ Construction | ✅ BOM | ❌ | **Orphan input (fixed asset, CRITICAL)** |
| 40 | IndustrialMachinery | ✅ HeavyIndustry | ✅ HeavyIndustry | ✅ Mining (generator) | ❌ | ❌ | Partial (conditional) |
| 41 | AgriculturalMachinery | ❌ | ❌ | ✅ Agriculture | ❌ | ❌ | **Orphan input (fixed asset, CRITICAL)** |
| 42 | Furniture | ❌ | ✅ LightIndustry | ❌ | ❌ | ✅ B2C | OK (B2C) |
| 43 | LuxuryFurniture | ❌ | ❌ | ❌ | ❌ | ❌ (retail only) | Dead |
| 44 | Copper | ✅ Mining | ✅ Mining | ❌ | ❌ | ❌ | Orphan output |
| 45 | Meat | ❌ | ✅ Agriculture | ❌ | ❌ | ❌ (retail only) | Orphan output |
| 46 | Fighters | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 47 | Fertilizers | ❌ | ❌ | ✅ Agriculture | ❌ | ❌ | **Orphan input** |
| 48 | Destroyers | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 49 | Software | ❌ | ❌ | ✅ (11 sectors) | ❌ | ❌ | **Orphan input (critical)** |
| 50 | Fruit | ❌ | ✅ Agriculture | ❌ | ❌ | ❌ (retail only) | Orphan output |
| 51 | Lead | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be battery input) |
| 52 | Fuels | ❌ | ✅ HeavyIndustry | ✅ (many) | ❌ | ❌ | Partial (generator only) |
| 53 | Battleships | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 54 | Paper | ❌ | ✅ LightIndustry | ✅ (many) | ❌ | ❌ | OK |
| 55 | Sand | ❌ | ❌ | ❌ | ✅ BOM (cement) | ❌ | **Orphan input (BOM)** |
| 56 | Pistols | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 57 | Trains | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 58 | Prefabricates | ❌ | ✅ Construction | ❌ | ❌ | ❌ | Orphan output |
| 59 | Gunpowder | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 60 | Food | ❌ | ✅ LightIndustry | ✅ (many, as labor input) | ❌ | ❌ | OK (labor subsistence) |
| 61 | Radio | ✅ Media | ❌ | ❌ | ❌ | ❌ (retail only) | Orphan output (no demand) |
| 62 | Oil | ❌ | ✅ Mining | ✅ HeavyIndustry (generator) | ❌ | ❌ | Partial (generator only) |
| 63 | Fish | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (fishing system separate) |
| 64 | Cars | ❌ | ❌ | ❌ | ❌ | ❌ (retail only) | **Orphan (fixed asset + durable, no production)** |
| 65 | Airplanes | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 66 | Sulfur | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be chemical input) |
| 67 | SupportEquipment | ✅ Armaments | ❌ | ❌ | ❌ | ❌ | Military only |
| 68 | Silver | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 69 | Steel | ✅ HeavyIndustry | ✅ HeavyIndustry | ✅ (many) | ✅ BOM | ❌ | OK |
| 70 | PassengerShips | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 71 | CargoShips | ❌ | ✅ ExportServices | ❌ | ❌ | ❌ | Orphan output |
| 72 | NavalVessels | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 73 | MineralResources | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 74 | Glass | ❌ | ❌ | ✅ LightIndustry | ✅ BOM | ❌ | **Orphan input** |
| 75 | Salt | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 76 | RollingStock | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 77 | Televisions | ✅ Media | ❌ | ❌ | ❌ | ❌ (retail only) | Orphan output (no demand) |
| 78 | Peat | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 79 | PassengerTransport | ✅ Transport | ✅ ExportServices | ❌ | ❌ | ❌ | Orphan output (no B2C demand) |
| 80 | Clothing | ✅ LightIndustry | ✅ LightIndustry | ❌ | ❌ | ✅ B2C | OK |
| 81 | LuxuryClothing | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 82 | AdministrativeServices | ✅ PublicServices | ✅ PublicServices | ❌ | ❌ | ❌ | Government consumption |
| 83 | BankingServices | ❌ | ✅ ExportServices | ❌ | ❌ | ❌ | Orphan output |
| 84 | ConstructionServices | ✅ Construction | ✅ Construction | ❌ | ❌ | ❌ | OK (construction system) |
| 85 | MaintenanceServices | ✅ MaintenanceWorkshops | ❌ | ❌ | ❌ | ❌ | OK (fixed-asset maintenance) — but no generator seed |
| 86 | LocalServicesCommodity | ❌ | ✅ LocalServices, Hospitality | ❌ | ❌ | ❌ | B2C service (separate clearing) |
| 87 | InsuranceServices | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 88 | Limestone | ❌ | ❌ | ❌ | ❌ | ❌ | Dead (should be cement input) |
| 89 | Vegetables | ❌ | ✅ Agriculture | ❌ | ❌ | ❌ | **Orphan output (name mismatch)** |
| 90 | Cereal | ✅ Agriculture | ❌ | ❌ | ❌ | ✅ B2C | OK in registry, but generator produces wrong variant |
| 91 | Vegetable | ✅ Agriculture | ❌ | ❌ | ❌ | ✅ B2C | OK in registry, but generator produces wrong variant |
| 92 | Protein | ❌ | ❌ | ❌ | ❌ | ✅ B2C | **Orphan B2C demand (no producer)** |
| 93 | Fodder | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 94 | IndustrialFiber | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 95 | Chemicals | ❌ | ❌ | ✅ (5 sectors) | ❌ | ❌ | **Orphan input (critical)** |
| 96 | Seeds | ❌ | ❌ | ✅ Agriculture | ❌ | ❌ | **Orphan input** |
| 97 | SodaAsh | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 98 | Ammonia | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 99 | Luxury | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 100 | Water | ❌ | ❌ | ✅ Energy, Agriculture | ❌ | ❌ | **Orphan input (may be intended free)** |
| 101 | Hydrogen | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 102 | BrownCoal | ❌ | ❌ | ✅ Energy (generator) | ❌ | ❌ | **Orphan input** |
| 103 | HardCoal | ✅ Mining | ❌ (generator consumes) | ✅ Energy, HeavyIndustry | ❌ | ❌ | OK |
| 104 | Fibers | ❌ | ❌ | ✅ LightIndustry | ❌ | ❌ | **Orphan input** |
| 105 | Grains | ❌ | ✅ Agriculture | ❌ | ❌ | ❌ | **Orphan output (name mismatch)** |
| 106 | Gold | ❌ | ✅ Mining | ❌ | ❌ | ❌ | Orphan output (strategic reserve?) |
| 107 | Submarines | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 108 | Iron | ❌ | ✅ Mining | ✅ HeavyIndustry | ❌ | ❌ | OK (generator produces) |
| 109 | Gravel | ❌ | ❌ | ❌ | ❌ | ❌ | Dead |
| 110 | Livestock | ❌ | ❌ | ✅ Agriculture | ❌ | ❌ | **Orphan input** |
| 111-128 | Service/capacity commodities | Various | Various | Various | ❌ | Various | Mostly OK (government systems) |

### Summary Counts

| Category | Count |
|----------|-------|
| Total Commodity enum variants | 128 |
| Dead (no production, no consumption) | ~35 |
| Orphan inputs (consumed, never produced) | ~28 |
| Orphan outputs (produced, never consumed) | ~12 |
| Name mismatches (produced ≠ demanded) | 2 (Grains/Cereal, Vegetables/Vegetable) |
| Fixed assets with zero production | 4 of 6 (ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks; Cars also zero) |
| Sectors with no generator recipe | 11 of 21 |
| New commodities needed | 6 (RareEarthElements, Lithium, Plastics, RefinedFuel, Semiconductors, Batteries) |

---

## Appendix B: Files Examined

| File | Purpose |
|------|---------|
| `state/src/registries/enums.rs` | `Commodity` enum (128 variants), `Sector` enum (21 variants), `is_fixed_asset()`, `is_quality_durable()` |
| `state/src/registries/production_methods_data.rs` | All production methods for 13 sectors (mining, agriculture, heavy_industry, light_industry, armaments, construction, energy, transport, media, medical, education, public_services, maintenance_workshops) |
| `state/src/engine/generator/corporate.rs` | `generate_corporate_entities()`, `sector_recipe()` (10 of 21 sectors covered), `generate_region_companies()` |
| `state/src/economy/b2b_orders.rs` | `submit_company_b2b_orders()`, `submit_fixed_asset_purchase_bids()`, `execute_production_cycle()`, `settle_maintenance_service_trades()` |
| `state/src/economy/fixed_assets.rs` | `FixedAssetCohort` struct, `machinery_factor()`, `degrade_cohorts()`, `restore_cohort_condition()` |
| `state/src/economy/generative_goods_config.rs` | Phase 19 configuration knobs |
| `state/src/registries/blueprint_specs.rs` | `BlueprintSpec` for 13 blueprint-eligible commodities, material roles and substitutes |
| `state/src/construction/bom.rs` | Construction BOM for all building types (all require ConstructionMachinery) |
| `state/src/data/consumption_registry.rs` | B2C consumption baskets (demands Cereal, Vegetable, Protein, HealthCapacity, EducationSlots, Clothing, Furniture) |
| `state/src/economy/retail_registry.rs` | Retail store commodity eligibility |
| `state/src/engine/turn.rs` | Turn loop (confirms `submit_fixed_asset_purchase_bids` is called at l.566, cohort degradation at l.894) |
| `resurrection-phase19-generative-goods-blueprint.md` | Phase 19 design document |

---

## Conclusion

The economy freezes because of a **systemic supply chain failure**, not a single bug. The root causes are:

1. **4 of 6 fixed-asset commodities have zero production methods** — no machinery is ever produced, so no fixed-asset cohorts can form.
2. **11 of 21 sectors have no generator recipe** — they produce nothing at world birth, including critical service sectors (health, education, maintenance).
3. **25+ fundamental intermediate commodities have zero production** — MechanicalComponents, ElectronicComponents, Software, Chemicals, Fibers, Timber, Planks, Stone, Bricks, Glass, Sand, NaturalGas, and more are consumed but never produced.
4. **A commodity name mismatch** breaks the food supply chain (Grains ≠ Cereal, Vegetables ≠ Vegetable).
5. **The generator and registry are disconnected** — the generator creates its own recipes that don't match the registry, creating two parallel, inconsistent economies.
6. **There is no minimum-viable-supply-chain guarantee** — the generator blindly follows budget employment shares with no validation that critical commodities have producers.

The proposed fix involves three coordinated efforts:
- **Part 2:** Add 6 new commodities and activate 25+ dead ones, with a complete modern supply chain map from extraction → intermediates → investment goods → consumer goods.
- **Part 3:** Rewrite the generator to pull from the registry, seed minimum viable supply chain, seed fixed assets and inventory, and validate completeness.

**No implementation has begun. Awaiting user approval on the supply chain map, commodity additions, and bootstrapping strategy before proceeding.**
