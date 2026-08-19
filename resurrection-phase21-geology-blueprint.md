# Resurrection Phase 21 — Geology, Exploration & Subsurface Rights

**Blueprint & Dependency Audit for finite-resource mining, exploration mechanics, subsurface property law, and tectonic disasters.**

---

## PART 1: DEPENDENCY AUDIT

### 1.1 Mining Sector — Current State

#### How mining buildings gather resources today

Mining buildings use the **standard production cycle** in `state/src/economy/production.rs` (`process_building_cycle`). There is no mining-specific extraction logic. The formula is:

```
production_scale = effective_employment / 1000.0
output_quantity = method.outputs[commodity] * production_scale
```

The `ActiveProductionMethod` is resolved from `registries.production_methods` by selecting the latest method whose `year <= current_year`. Mining methods are registered in `mining_methods()` inside `state/src/registries/production_methods_data.rs` and produce the following commodities:

| Commodity | Method name | Year |
|---|---|---|
| HardCoal | Manual Mining → CNC Mining | 1880–1970 |
| Iron | Iron Ore Mining | 1880 |
| Copper | Copper Ore Mining / Froth Flotation | 1880–1900 |
| Oil | Oil Drilling | 1880 |
| NaturalGas | Natural Gas Extraction | 1900 |
| Bauxite | Bauxite Mining | 1890 |
| Sand, Gravel | Sand And Gravel Quarry | 1880 |
| Stone | Stone Quarrying | 1880 |
| Clay | Clay Mining | 1880 |
| Limestone | Limestone Quarrying | 1880 |
| Sulfur | Sulfur Mining | 1890 |
| Salt | Salt Mining | 1880 |
| Tin | Tin Ore Mining | 1890 |
| Zinc | Zinc Ore Mining | 1890 |
| Lead | Lead Ore Mining | 1890 |
| Silver | Silver Mining | 1890 |
| Gold | Gold Mining | 1890 |
| Peat | Peat Cutting | 1880 |
| BrownCoal | Brown Coal Mining | 1880 |
| RareEarthElements | Rare Earth Element Mining | 1965 |
| Lithium | Lithium Extraction | 1970 |
| Magnesium | Magnesium Refinery | 1900 |

**Critical finding: There are NO hard limits on extraction.** The `Region` struct has two fields:

```rust
pub limity_wydobycia: BTreeMap<String, i64>,   // extraction limits
pub limity_wykorzystane: BTreeMap<String, i64>, // used limits
```

These are populated by `Climate::mine_limits()` during region generation (`generate_regional_topology` at line 1242 of `geography.rs`) but are **never read or enforced** outside `geography.rs` itself. A grep for `limity_wydobycia|limity_wykorzystane` across the entire `src/` tree returns matches only in `geography.rs`. Mines have **infinite, magical access** to resources.

Additionally, `Region.zasoby` (an opaque `Map<String, Value>`) is seeded by `seed_geological_deposits()` with geological reserve data (`rezerwy_geologiczne`, `rezerwy`, `wydobycie_roczne`, `efektywność`) — but this data is **never consumed** by the production cycle. It is decorative JSON.

#### Existing geological scaffolding (unused)

`geography.rs` already contains substantial geological infrastructure that is **completely disconnected** from the simulation:

- `FormationType` enum (line 91): `MountainRange`, `SedimentaryBasin`, `RiftValley`, `VolcanicArc`, `ContinentalShelf`.
- `ResourceDeposit` struct (line 107): `resource_type`, `estimated_reserves`, `extraction_cost`, `quality`.
- `GeologicalFormation` struct (line 124): `id`, `name`, `formation_type`, `resource_deposits`, `overlapping_regions`, `total_area`.
- `generate_geological_formations()` (line 1524): Creates 2–10 formations per country, each overlapping 2–5 regions, with 1–3 resource deposits. Resources are drawn from formation-appropriate pools (e.g., SedimentaryBasin → coal/oil/gas; VolcanicArc → sulfur/metals/geothermal).
- `get_formations_for_region()` (line 1647) and `get_region_resources_from_formations()` (line 1665): Helper queries.

**None of these are called from outside `geography.rs`.** The `generate_geological_formations()` function is never invoked during world generation (`generate_country` in `engine/generator/mod.rs` does not call it). `Country` has no `geological_formations` field. The entire geological layer is dead code.

#### Commodity coverage gap

`Commodity::Uranium` does **not exist** in the main `Commodity` enum. `Uranium` appears only in `FuelType` (line 925 of `enums.rs`), which is a power-plant fuel category, not a tradeable commodity. Uranium mining cannot produce a tradeable good without adding `Commodity::Uranium`.

### 1.2 Land Ownership & Rents

#### Current land ownership model

Land ownership is tracked at two levels:

1. **Region level**: `Region.land_distribution: BTreeMap<String, ClassLandDistribution>` — maps soil-class keys to ownership breakdowns.
2. **Soil-class level**: `ClassLandDistribution` (line 429 of `geography.rs`) tracks hectares by six owner categories:
   - `aristocracy_hectares` — latifundia estates
   - `free_peasant_hectares` — smallholdings
   - `state_hectares` — state-owned land
   - `corporation_hectares` — corporate land
   - `community_hectares` — communal/village land
   - `municipal_hectares` — municipality (JST) land

`ClassDemographics` (line 691) tracks per-class `population`, `savings`, `income`, `sentiment`, and `health_status`. The `savings` field is the financial receive-point for class-level transfers.

#### Subsurface rights potential

There is **no subsurface rights concept** today. Land ownership is purely surface (agricultural) ownership. The `ClassLandDistribution` struct could be extended with subsurface hectare tracking, or a parallel `SubsurfaceOwnership` struct could be introduced.

The `credit_citizen_savings_region()` function in `transfer_settler.rs` (line 424) distributes a cash amount proportionally across all class demographics by population. For landowner royalties, a **targeted** credit function would be needed — one that credits only the Aristocracy class (or whichever class owns the subsurface rights in the region), not all classes proportionally.

#### Royalty payment precedent

The existing royalty system in `economy/royalties.rs` provides the architectural pattern:
- `process_royalty_payment()`: licensee → licensor (company-to-company, direct cash).
- `process_all_royalty_payments()`: includes state patent royalties (company → `treasury.liquid_reserves`).
- `process_blueprint_royalty_payments()`: uses `TransferSettler::credit_company_by_id` and `debit_company_by_id` for strict double-entry with bank balance-sheet sync.

Subsurface royalties would follow the same pattern but with a new destination: `ClassDemographics.savings` for private-property regimes, or `Treasury.liquid_reserves` for state-concession regimes.

### 1.3 Disasters & Mitigation

#### Current disaster system

`state/src/economy/disasters.rs` implements:

- `DisasterType` enum: `IndustrialFire`, `BuildingCollapse`, `Flood`, `Earthquake`, `Epidemic`, `Pogrom`, `VigilanteMob`, `TerroristAttack`.
- `check_disaster_triggers()`: The main disaster processing function. Currently triggers only:
  - **Floods** (from `WeatherEventType::Flood` events, mitigated by `ShelterCapacity`).
  - **Storms** (from `WeatherEventType::Storm` events, mitigated by `ShelterCapacity`).
  - **Industrial fires** (from building condition < 0.4, mitigated by `FireProtectionCapacity`).
  - **Building collapses** (from building condition < 0.15, no mitigation).
- Mitigation pattern: `mitigation = (capacity / threshold).min(cap); effective_severity = (base_severity - mitigation).max(0.0)`.

**Critical finding: `DisasterType::Earthquake` exists in the enum but is NEVER triggered.** There is no earthquake trigger logic, no volcanic eruption type, and no tectonic region trait.

#### Missing infrastructure for tectonics/volcanism

- `DisasterType::VolcanicEruption` — does not exist, needs to be added.
- `TectonicFault` / `VolcanicZone` region traits — do not exist. `Region` has no geological-trait field.
- `DisasterMitigationCapacity` commodity — does not exist. The disaster system currently only recognizes `FireProtectionCapacity` and `ShelterCapacity`.
- `FormationType::VolcanicArc` exists in `geography.rs` but is not linked to disaster triggering.

#### Injection point for tectonic disasters

The `check_disaster_triggers()` function in `disasters.rs` is the single entry point. New tectonic disaster triggers would be added as new blocks within this function, following the existing pattern:

1. Check region traits for `TectonicFault` or `VolcanicZone`.
2. Roll for event occurrence (deterministic via `rng_seed + turn`).
3. Sum `DisasterMitigationCapacity` from buildings' `last_production` (same pattern as `sum_fire_protection_capacity`).
4. Compute effective severity after mitigation.
5. Apply casualties, economic damage, and building destruction.

---

## PART 2: TECHNICAL BLUEPRINT & PHASING STRATEGY

### Phase 21A: Geological Structures & Deposit Physics

#### 21A.1: Activate and extend the geological formation system

**Goal**: Wire the existing `generate_geological_formations()` into world generation and extend it with depth, depletion, and quality-decay physics.

**Changes**:

1. **`state/src/state/mod.rs`** — Add `geological_formations: Vec<GeologicalFormation>` field to `Country`:
   ```rust
   #[serde(default)]
   pub geological_formations: Vec<crate::society::geography::GeologicalFormation>,
   ```

2. **`state/src/engine/generator/mod.rs`** — In `generate_country()`, after region generation, call:
   ```rust
   let region_ids: Vec<String> = country.regions.iter().map(|r| r.id.clone()).collect();
   country.geological_formations = crate::society::geography::generate_geological_formations(&region_ids, &mut rng);
   ```

3. **`state/src/society/geography.rs`** — Refactor `ResourceDeposit` to use `Commodity` enum natively instead of Polish strings:
   ```rust
   pub struct ResourceDeposit {
       pub commodity: Commodity,       // NATIVE enum variant — no Polish strings
       pub estimated_reserves: f64,    // total original quantity
       pub current_reserves: f64,      // remaining quantity (depletes)
       pub extraction_cost: f64,
       pub quality: f64,               // base quality 0-1
       pub current_quality: f64,       // effective quality (decays with depletion)
       pub depth: f64,                 // meters below surface
       pub discovered: bool,           // fog-of-war: false at world birth
   }
   ```
   The `resource_type: String` field is **removed entirely**. The `commodity` field is not `Option<Commodity>` — every deposit has a concrete tradeable commodity from birth. No mapping table, no Polish strings, no `Option`.

4. **`state/src/society/geography.rs`** — **Completely rewrite `generate_formation_resources()`** to natively emit `Commodity` enum variants. The formation-type → commodity pools become:
   ```rust
   fn generate_formation_resources(
       formation_type: &FormationType,
       rng: &mut impl Rng,
   ) -> BTreeMap<String, ResourceDeposit> {
       let possible: &[Commodity] = match formation_type {
           FormationType::MountainRange => &[
               Commodity::HardCoal, Commodity::Iron, Commodity::Copper,
               Commodity::Zinc, Commodity::Gold, Commodity::Silver,
           ],
           FormationType::SedimentaryBasin => &[
               Commodity::HardCoal, Commodity::Oil, Commodity::NaturalGas,
               Commodity::BrownCoal, Commodity::Peat,
           ],
           FormationType::RiftValley => &[
               Commodity::Oil, Commodity::NaturalGas, Commodity::Uranium,
           ],
           FormationType::VolcanicArc => &[
               Commodity::Sulfur, Commodity::Copper, Commodity::Tin,
               Commodity::Lead, Commodity::Zinc,
           ],
           FormationType::ContinentalShelf => &[
               Commodity::Oil, Commodity::NaturalGas, Commodity::Sand, Commodity::Gravel,
           ],
       };
       // Select 1-3 commodities, build deposits keyed by commodity string name
       // ...
   }
   ```
   The deposit map key becomes `Commodity::to_string()` (the serde name), not a Polish string. **No Polish resource strings exist in the engine logic.** The legacy `seed_geological_deposits()` function (which writes Polish-keyed JSON into `Region.zasoby`) is left untouched for save compatibility but is no longer the authoritative source — `Country.geological_formations` is.

5. **Add `Commodity::Uranium`** to `state/src/registries/enums.rs` (in the main `Commodity` enum, with `try_from` mapping `"uranium" => Ok(Commodity::Uranium)`). Add it to the `all()` list and `is_active()`. Add a uranium mining method to `production_methods_data.rs`:
   ```rust
   m.insert(MethodSlot::Production, "Uranium Mining".into(),
       pm(1945, Some("nuclear_001"), 0.15, 0.35, 0.50, 2.0,
          &[(Commodity::Energy, 15.0), (Commodity::Fuels, 5.0), (Commodity::Chemicals, 3.0)],
          &[(Commodity::Uranium, 5.0)]));
   ```

#### 21A.2: Deposit-to-building linkage

**Goal**: Mining buildings must be linked to specific deposits in their region. Production is constrained by deposit `current_reserves` and `current_quality`.

**Changes**:

1. **`state/src/entities/mod.rs`** — Add a `deposit_id: Option<String>` field to `Building`:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub deposit_id: Option<String>,
   ```

2. **`state/src/engine/generator/corporate.rs`** — When generating mining buildings in `seed_minimum_viable_supply_chain()` or `generate_region_companies()`, assign each mining building a deposit from the region's formations. If no suitable deposit exists, the mining building produces nothing (or produces at a minimal rate from "surface scatter").

3. **New module: `state/src/economy/geology.rs`** — Deposit lookup and depletion logic:
   ```rust
   pub fn find_deposit_for_commodity(
       country: &Country,
       region_id: &str,
       commodity: Commodity,
   ) -> Option<&GeologicalFormation>  // returns formation containing matching deposit

   pub fn deplete_deposit(
       country: &mut Country,
       formation_id: &str,
       resource_type: &str,
       amount: f64,
   ) -> f64  // returns actual amount that could be extracted (may be < requested)
   ```

#### 21A.3: Gradual depletion and quality decay

**Goal**: As a deposit is mined, `current_reserves` drops and `current_quality` decays. Lower quality reduces effective output — an economic death spiral, not a hard wall.

**Depletion formula**:
```
depletion_ratio = 1.0 - (current_reserves / estimated_reserves)
current_quality = base_quality * (1.0 - 0.5 * depletion_ratio^2)
```

At 50% depletion, quality is ~87.5% of base. At 90% depletion, quality is ~59.5% of base. At 100% depletion, quality is 50% of base (but `current_reserves = 0` means no extraction).

**Quality effect on production**: In `process_building_cycle()`, for mining buildings with a linked deposit, multiply output by `deposit.current_quality`:
```
effective_output = method.outputs[commodity] * production_scale * deposit.current_quality
```

**Changes**:

1. **`state/src/economy/production.rs`** — In `process_building_cycle()`, after computing `production_scale`, check if the building has a `deposit_id` and the sector is Mining. If so:
   - Look up the deposit from `Country.geological_formations`.
   - Compute `available = deposit.current_reserves`.
   - Clamp `effective_output` to `min(method_output, available)`.
   - Call `deplete_deposit()` to reduce `current_reserves`.
   - Recompute `current_quality` based on new depletion ratio.

2. **`state/src/engine/turn.rs`** — Pass `&mut Country` directly into the production phase. Each rayon task has exclusive `&mut Country` access, so depletion mutations are applied **synchronously and in-place** within the parallel production block. No post-parallel queue, no snapshot, no delta collection.

#### 21A.4: Depth gating

**Goal**: Deep deposits require advanced tech to access. A deposit at 800m depth cannot be mined with "Manual Mining" (1880) but can be mined with "Mechanized Longwall" (1950).

**Changes**:

1. **`state/src/registries/production_methods_data.rs`** — Add a `max_depth: f64` field to the `pm()` helper or as metadata on mining methods. Map depth capability to tech progression:
   - Manual Mining (1880): max_depth = 200m
   - Pneumatic Drilling (1885): max_depth = 400m
   - Electric Mine Pumps (1890): max_depth = 600m
   - Longwall Mining (1895): max_depth = 800m
   - Mechanized Longwall (1950): max_depth = 1200m
   - CNC Mining (1970): max_depth = 2000m

2. **`state/src/economy/geology.rs`** — Add `can_access_depth(method_year: u32, deposit_depth: f64) -> bool` that checks whether the building's active method year supports the deposit depth.

3. **`state/src/economy/production.rs`** — In the mining deposit check, if `deposit.depth > method.max_depth`, reduce output to 0 (or a tiny "surface scatter" fraction) and log a warning.

---

### Phase 21B: Exploration & Expansion

#### 21B.1: Fog of war — undiscovered deposits

**Goal**: Not all deposits are known at world birth. Only shallow deposits (depth < 200m) in known formations start `discovered = true`. Deep deposits start `discovered = false` and cannot be mined until discovered.

**Changes**:

1. **`state/src/society/geography.rs`** — In `generate_formation_resources()`, set `discovered = depth < 200.0` (with some randomness).

2. **`state/src/economy/production.rs`** — In the mining deposit check, if `!deposit.discovered`, the building cannot link to it. Production falls back to surface scatter or zero.

3. **`state/src/economy/geology.rs`** — Add:
   ```rust
   pub fn discover_deposit(
       country: &mut Country,
       formation_id: &str,
       resource_type: &str,
   ) -> bool  // returns true if a deposit was newly discovered

   pub fn expand_deposit(
       country: &mut Country,
       formation_id: &str,
       resource_type: &str,
       additional_reserves: f64,
   )  // increases estimated_reserves and current_reserves
   ```

#### 21B.2: Corporate exploration

**Goal**: Mining companies can allocate an `ExplorationBudget` from their cash reserves. This yields a probabilistic chance to discover new deposits or expand existing ones.

**Changes**:

1. **`state/src/entities/mod.rs`** — Add `exploration_budget: f64` field to `Company` (default 0.0):
   ```rust
   #[serde(default)]
   pub exploration_budget: f64,
   ```

2. **New module: `state/src/economy/exploration.rs`** — Exploration processing:
   ```rust
   pub fn process_corporate_exploration(
       companies: &mut [Company],
       country: &mut Country,
       rng_seed: u64,
       turn: u32,
   ) -> Vec<ExplorationResult>
   ```

   **Logic**:
   - For each mining company with `exploration_budget > 0`:
     - Deduct `exploration_budget` from `company.available_cash` (via `debit_company_by_id`).
     - For each region where the company has mining buildings:
       - Roll `rng.gen_range(0.0..1.0)` against discovery probability: `P = 0.3 * (budget / 10000.0).min(1.0)`.
       - If success: either discover a new `undiscovered` deposit in a formation overlapping this region, or expand an existing deposit's `current_reserves` by `budget * rng.gen_range(0.5..2.0)`.
     - Reset `exploration_budget = 0.0` after processing.

3. **`state/src/engine/turn.rs`** — Call `process_corporate_exploration()` during the post-parallel phase (after production, before market clearing), since it mutates `Country.geological_formations`.

4. **Company AI** — Mining companies should allocate a fraction of available cash to `exploration_budget` when their linked deposits are depleting (depletion_ratio > 0.5). This can be a simple heuristic in the corporate decision logic.

#### 21B.3: Earth Research Centers

**Goal**: A new state building type that consumes `Software` and `OfficeMachinery`, discovers deposits on behalf of the state, generates `DisasterMitigationCapacity`, and boosts geology-related tech discovery.

**Changes**:

1. **`state/src/registries/enums.rs`** — Add new commodity:
   ```rust
   DisasterMitigationCapacity,
   ```
   Add to `all()`, `try_from()`, `is_active()`.

2. **`state/src/registries/production_methods_data.rs`** — Add Earth Research Center methods under `public_services_methods()` (or a new `earth_research_methods()` group):
   - "Geological Survey Office" (1880): inputs `Paper`, `AdministrativeServices`; outputs `DisasterMitigationCapacity` (small), low discovery boost.
   - "Seismological Observatory" (1900): inputs `Paper`, `Energy`, `MechanicalComponents`; outputs `DisasterMitigationCapacity` (medium), moderate discovery boost.
   - "Earth Research Center" (1960): inputs `Software`, `OfficeMachinery`, `ElectronicComponents`; outputs `DisasterMitigationCapacity` (large), high discovery boost.
   - "Satellite Geology Institute" (1990): inputs `Software`, `ElectronicComponents`, `OfficeMachinery`; outputs `DisasterMitigationCapacity` (large), very high discovery boost + deposit expansion.

3. **`state/src/engine/generator/corporate.rs`** — Add Earth Research Center to the `critical_sectors` list (or seed it as a state-owned building) so every country has at least one.

4. **`state/src/economy/exploration.rs`** — State exploration from Research Centers:
   ```rust
   pub fn process_state_exploration(
       country: &mut Country,
       buildings: &[Building],
       rng_seed: u64,
       turn: u32,
   ) -> Vec<ExplorationResult>
   ```
   - Sum `DisasterMitigationCapacity` from all buildings' `last_production`.
   - Use total capacity as exploration budget for the state: discovers deposits in any region, boosts geology tech discovery rate.

5. **Tech boost**: In the innovation/R&D processing, add a multiplier based on total `DisasterMitigationCapacity` production for geology-related techs (mining_*, renew_004, etc.).

---

### Phase 21C: Subsurface Rights & Concessions (Laws)

#### 21C.1: SubsurfaceRightsLaw

**Goal**: Implement a law with three variants that determines who owns subsurface resources and how mining companies pay for extraction rights.

**Changes**:

1. **`state/src/politics/laws.rs`** — Add new law struct and enum:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   pub enum SubsurfaceRightsRegime {
       /// Subsurface belongs to the landowner. Mines pay royalties to
       /// the ClassDemographics that owns the land (typically Aristocracy).
       PrivateProperty,
       /// The state owns the subsurface. Companies must buy/renew
       /// concessions from the Treasury.
       StateConcessions,
       /// Only state-owned companies can extract. No private mining.
       Nationalization,
   }

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   pub struct SubsurfaceRightsLaw {
       pub regime: SubsurfaceRightsRegime,
       /// Royalty rate as fraction of production value (0.0–1.0).
       pub royalty_rate: f64,
       /// Concession renewal period in turns (for StateConcessions).
       pub concession_period_turns: u32,
       /// Concession fee as fraction of estimated deposit value.
       pub concession_fee_rate: f64,
       /// Nationalization compensation rate (0.0–1.0).
       /// When enacting Nationalization, Treasury pays `book_value * compensation_rate`
       /// to each expropriated private company. 0.0 = brutal uncompensated confiscation.
       /// 1.0 = full book value compensation.
       pub compensation_rate: f64,
   }
   ```

2. **`state/src/politics/laws.rs`** — Add to `LawType` enum:
   ```rust
   SubsurfaceRights(SubsurfaceRightsLaw),
   ```

3. **`state/src/politics/laws.rs`** — Add `enact_law` match arm for `SubsurfaceRights` that:
   - Stores the law on `Country.politics` (requires adding a `subsurface_rights_law: Option<SubsurfaceRightsLaw>` field to `Politics`).
   - If regime is `Nationalization`: immediately expropriates all private mining buildings by reassigning `building.owner_id` to a state-owned mining enterprise (creating one if needed). This is a physical ownership transfer, not a production barrier.

4. **`state/src/state/mod.rs`** — Add `subsurface_rights_law` to `Politics` struct (or directly on `Country`).

#### 21C.2: Royalty & concession payment processing

**Goal**: During the production phase, after mining output is computed, deduct royalties/concessions via strict double-entry accounting.

**Changes**:

1. **New module: `state/src/economy/subsurface_rights.rs`** — Payment processing:
   ```rust
   pub fn process_subsurface_payments(
       companies: &mut [Company],
       country: &mut Country,
       market_history: &MarketHistory,
       buildings: &[Building],
   ) -> Vec<SubsurfacePaymentRecord>
   ```

   **Private Property regime**:
   - For each mining building with a linked deposit:
     - Compute royalty = `output_value * royalty_rate` (where `output_value = output_quantity * last_turn_vwap`).
     - Debit mining company via `debit_company_by_id()`.
     - Credit the landowning class in the region's `ClassDemographics` — specifically the class that owns the subsurface (determined by `Region.land_distribution`). This requires a new targeted credit function that **simultaneously updates the backing bank's balance sheet** to preserve double-entry:
       ```rust
       /// Credit a specific rural class's savings AND sync the backing bank's
       /// deposits + reserves. Without the bank sync, M3 money supply is
       /// destroyed — this is a strict double-entry requirement.
       pub fn credit_class_savings_region(
           region: &mut Region,
           companies: &mut [Company],
           bank_id: &str,
           class: RuralClass,
           amount: f64,
       ) -> f64 {
           if amount <= 0.0 { return 0.0; }
           // 1. Credit the class's savings
           let key = serde_json::to_string(&class).unwrap_or_default();
           if let Some(demo) = region.class_demographics.rural_classes.get_mut(&key) {
               demo.savings += amount;
           }
           // 2. Sync the backing bank's balance sheet (deposits + reserves)
           //    Same pattern as adjust_bank_balance in credit_company_by_id
           adjust_bank_balance(companies, bank_id, amount, amount);
           amount
       }
       ```
     - The `bank_id` is **dynamically queried** at runtime from the `companies` slice: find the first active company where `sector == Sector::Banking` and `bank_type == Some(BankingBankType::Commercial)` (or `Universal`). If no commercial bank is alive (bankrupt), fall back to the Central Bank. **Never hardcode bank ID string prefixes** — the generator naming convention may change or the primary bank may go bankrupt.
     - If the landowning class is Aristocracy, credit `region.class_demographics.rural_classes["Aristocracy"].savings` + bank sync.

   **State Concessions regime**:
   - For each mining building with a linked deposit:
     - Compute concession fee = `output_value * royalty_rate` (same formula, different destination).
     - Debit mining company via `debit_company_by_id()`.
     - Credit `country.budget.liquid_reserves` (same as state patent royalties).

   **Nationalization regime** — True Expropriation:
   - Nationalization means **physical ownership transfer**, not a magical production barrier.
   - When `SubsurfaceRightsLaw` is enacted with `Nationalization`, the `enact_law` function must:
     1. Identify all mining buildings owned by private companies (companies whose `LegalForm` is not a state-owned form).
     2. Transfer `building.owner_id` to a state-owned mining enterprise (or create one if none exists). The state enterprise is a `Company` with `LegalForm::StateOwnedEnterprise` (or the existing state-owned legal form).
     3. The expropriated buildings' `owner_id` is updated to the state enterprise's ID.
     4. **Mandatory compensation**: Calculate the rough book value of each expropriated building (sum of `fixed_assets` estimated value). The Treasury MUST pay `book_value * compensation_rate` to the expropriated private company via `TransferSettler::debit_treasury` (or direct treasury debit) and `credit_company_by_id`. If `compensation_rate` is 0.0, it is a brutal, uncompensated confiscation. If 1.0, full book value is paid. This is not optional.
   - After expropriation, the state enterprise mines normally. Its profits flow to Treasury via normal corporate tax and state-owned-enterprise dividend rules.
   - New private mining buildings **cannot be created** under Nationalization. The building generator checks the regime before allowing private mining construction.
   - If the regime later switches back to Private Property or State Concessions, the state-owned buildings remain state-owned unless explicitly privatized.

2. **`state/src/engine/turn.rs`** — Call `process_subsurface_payments()` during the post-parallel phase, after production and before market clearing. This follows the same pattern as `process_all_royalty_payments()`.

#### 21C.3: Concession lifecycle

**Goal**: For StateConcessions regime, concessions must be purchased and renewed.

**Changes**:

1. **`state/src/entities/mod.rs`** — Add `concession: Option<Concession>` to `Building`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
   pub struct Concession {
       pub deposit_id: String,
       pub purchased_turn: u32,
       pub expires_turn: u32,
       pub fee_paid: f64,
   }
   ```

2. **`state/src/economy/subsurface_rights.rs`** — Concession management:
   - `purchase_concession()`: Company pays fee to Treasury, receives `Concession` on building.
   - `renew_concession()`: If `current_turn >= concession.expires_turn`, company must renew or stop production.
   - `expire_concessions()`: Expired concessions halt production on the linked building.

---

### Phase 21D: Tectonics & Volcanism

#### 21D.1: Region traits for tectonic/volcanic activity

**Goal**: Assign `TectonicFault` and/or `VolcanicZone` traits to regions based on their geological formations.

**Changes**:

1. **`state/src/society/geography.rs`** — Add region trait enum and field:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
   #[serde(rename_all = "snake_case")]
   pub enum TectonicTrait {
       #[default]
       None,
       TectonicFault,
       VolcanicZone,
       // Both fault and volcanic (e.g., Pacific Ring of Fire)
       FaultAndVolcanic,
   }
   ```

2. **`state/src/society/geography.rs`** — Add `tectonic_trait: TectonicTrait` to `Region`:
   ```rust
   #[serde(rename = "cecha_tektoniczna", default)]
   pub tectonic_trait: TectonicTrait,
   ```

3. **`state/src/society/geography.rs`** — In `generate_regional_topology()`, after generating regions and formations, assign tectonic traits:
   - If a region overlaps a `FormationType::VolcanicArc` formation → `VolcanicZone` or `FaultAndVolcanic`.
   - If a region overlaps a `FormationType::RiftValley` formation → `TectonicFault`.
   - Otherwise → `None` (with small random chance of `TectonicFault` for realism).

#### 21D.2: Volcanic eruption and earthquake disasters

**Goal**: Tectonic regions can spawn volcanic eruptions and earthquakes. `DisasterMitigationCapacity` serves as defense.

**Changes**:

1. **`state/src/economy/disasters.rs`** — Add `VolcanicEruption` to `DisasterType`:
   ```rust
   VolcanicEruption,
   ```

2. **`state/src/economy/disasters.rs`** — Add `sum_disaster_mitigation_capacity()`:
   ```rust
   pub fn sum_disaster_mitigation_capacity(buildings: &[Building]) -> f64 {
       buildings.iter()
           .map(|b| *b.last_production.get(&Commodity::DisasterMitigationCapacity).unwrap_or(&0.0))
           .sum()
   }
   ```

3. **`state/src/economy/disasters.rs`** — Add new trigger blocks in `check_disaster_triggers()`:

   **Earthquake trigger** (for `TectonicFault` and `FaultAndVolcanic` regions):
   ```
   for region in country.regions where tectonic_trait in {TectonicFault, FaultAndVolcanic}:
       base_chance = 0.02 per turn  // ~once per 50 turns
       roll = rng.gen_range(0.0..1.0)
       if roll < base_chance:
           base_severity = rng.gen_range(0.3..0.9)
           mitigation = (disaster_mitigation_capacity / 100.0).min(0.6)
           effective_severity = (base_severity - mitigation).max(0.05)
           casualties = region.population * effective_severity * 0.002
           economic_damage = region.gdp * effective_severity * 0.08
           buildings_destroyed = (effective_severity * building_count * 0.1) as u32
           // Apply damage, push DisasterEvent
   ```

   **Volcanic eruption trigger** (for `VolcanicZone` and `FaultAndVolcanic` regions):
   ```
   for region in country.regions where tectonic_trait in {VolcanicZone, FaultAndVolcanic}:
       base_chance = 0.005 per turn  // ~once per 200 turns (rare but devastating)
       roll = rng.gen_range(0.0..1.0)
       if roll < base_chance:
           base_severity = rng.gen_range(0.5..1.0)
           mitigation = (disaster_mitigation_capacity / 200.0).min(0.4)
           effective_severity = (base_severity - mitigation).max(0.1)
           casualties = region.population * effective_severity * 0.005
           economic_damage = region.gdp * effective_severity * 0.15
           buildings_destroyed = (effective_severity * building_count * 0.3) as u32
           // Apply damage, push DisasterEvent
   ```

4. **`state/src/economy/disasters.rs`** — Add `DisasterTurnResult.total_disaster_mitigation_capacity` field.

#### 21D.3: Volcanic risk and reward

**Goal**: Volcanic zones are dangerous but economically beneficial.

**Changes**:

1. **Agricultural bonus**: In the agriculture production cycle, if the building's region has `VolcanicZone` trait, multiply output by `1.2` (volcanic ash soils are fertile). This can be applied as a `region_modifier` in `process_building_cycle()`.

2. **Geothermal energy access**: In the energy sector, `Geothermal Plant` methods (already in `production_methods_data.rs` at line 867) should only be placeable in regions with `VolcanicZone` or `TectonicFault` traits. Add a region-trait check in the building generator.

3. **DisasterMitigationCapacity defense roll**: The mitigation capacity from Earth Research Centers and Seismological Observatories reduces both severity and casualties. The formula above already incorporates this.

---

## CROSS-CUTTING CONCERNS

### Double-Entry Accounting Compliance

All financial flows in Phase 21 must use the existing `TransferSettler` infrastructure:

| Flow | Debit (source) | Credit (destination) | Mechanism |
|---|---|---|---|
| Private property royalties | Mining company `available_cash` | `ClassDemographics.savings` (landowning class) **+ backing bank deposits/reserves** | New `credit_class_savings_region(region, companies, bank_id, class, amount)` + `debit_company_by_id()` |
| State concession fees | Mining company `available_cash` | `country.budget.liquid_reserves` | `debit_company_by_id()` + direct treasury credit |
| Concession purchase | Mining company `available_cash` | `country.budget.liquid_reserves` | `debit_company_by_id()` + direct treasury credit |
| Exploration budget | Mining company `available_cash` | (consumed, not transferred) | `debit_company_by_id()` |
| Nationalization expropriation | (ownership transfer, no cash flow) | (ownership transfer, no cash flow) | `building.owner_id` reassignment to state enterprise |

### Save Compatibility

- All new `Region`, `Country`, `Building`, and `Company` fields use `#[serde(default)]` so existing saves deserialize without error.
- `ResourceDeposit` gains new fields with `#[serde(default)]` — old formation data (if any exists in saves) will get default values for `current_reserves`, `depth`, `discovered`, etc.
- `Commodity::Uranium` and `Commodity::DisasterMitigationCapacity` are new enum variants — no conflict with existing saves.
- `DisasterType::VolcanicEruption` is a new enum variant — no conflict.
- `TectonicTrait::None` is the `#[default]` — existing regions get `None`.

### Parallel Execution Safety

The production phase runs in parallel per country (`tasks.par.iter_mut().for_each()`). Each rayon task has **exclusive `&mut Country` access** — no two tasks ever touch the same country. Therefore, direct mutation of `Country.geological_formations` (deposit depletion) inside the production cycle is **100% thread-safe and correct**. No delta queue, no post-parallel collection, no over-engineering. Deplete deposits directly within the parallel production block.

Subsurface royalty payments and state exploration also mutate `Country` and `companies` per-country within the same task — safe by the same exclusive-access guarantee. The `credit_class_savings_region()` and `debit_company_by_id()` calls happen within the task that owns the `Country` and `companies` slice.

### Integrity Tests

New tests to add to `state/tests/supply_chain_integrity_test.rs` (or a new `geology_integrity_test.rs`):

1. **Formation generation**: Every country has ≥2 geological formations.
2. **Deposit linkage**: Every mining building has a `deposit_id` or is in a region with no deposits.
3. **Depletion physics**: After simulated extraction, `current_reserves < estimated_reserves` and `current_quality < base_quality`.
4. **Depth gating**: A deposit at 1000m depth cannot be mined with an 1880-era method.
5. **Fog of war**: Some deposits start `discovered = false`.
6. **Exploration**: After running `process_corporate_exploration()`, previously undiscovered deposits become discovered.
7. **Subsurface rights — Private Property**: Royalty payments credit `ClassDemographics.savings` of the landowning class **AND** the backing bank's `deposits` + `reserves_at_central_bank` increase by the same amount (double-entry invariant).
8. **Subsurface rights — State Concessions**: Concession payments credit `Treasury.liquid_reserves`.
9. **Subsurface rights — Nationalization**: Enacting Nationalization transfers `building.owner_id` of all private mining buildings to a state-owned enterprise. No private mining buildings remain under private ownership.
10. **Tectonic traits**: Regions overlapping `VolcanicArc` formations get `VolcanicZone` trait.
11. **Earthquake trigger**: `TectonicFault` regions can produce `DisasterType::Earthquake` events.
12. **Volcanic eruption trigger**: `VolcanicZone` regions can produce `DisasterType::VolcanicEruption` events.
13. **Disaster mitigation**: `DisasterMitigationCapacity` reduces effective severity.
14. **Geothermal restriction**: `Geothermal Plant` buildings only exist in `VolcanicZone` or `TectonicFault` regions.
15. **Volcanic agricultural bonus**: Agricultural buildings in `VolcanicZone` regions have higher output.
16. **New commodities**: `Commodity::Uranium` and `Commodity::DisasterMitigationCapacity` exist and are active.
17. **Earth Research Center methods**: Exist in the production registry and produce `DisasterMitigationCapacity`.

---

## IMPLEMENTATION ORDER & FILE MAP

### Phase 21A (Geological Structures & Deposit Physics)

| Step | File | Change |
|---|---|---|
| A1 | `state/src/registries/enums.rs` | Add `Commodity::Uranium` |
| A2 | `state/src/state/mod.rs` | Add `geological_formations` to `Country` |
| A3 | `state/src/society/geography.rs` | Extend `ResourceDeposit` with depth, depletion, discovery fields |
| A4 | `state/src/society/geography.rs` | Rewrite `generate_formation_resources()` to natively use `Commodity` enum (no Polish strings) |
| A5 | `state/src/registries/production_methods_data.rs` | Add `Uranium Mining` production method |
| A6 | `state/src/engine/generator/mod.rs` | Call `generate_geological_formations()` in `generate_country()` |
| A7 | `state/src/entities/mod.rs` | Add `deposit_id` to `Building` |
| A8 | `state/src/engine/generator/corporate.rs` | Assign deposits to mining buildings during generation |
| A9 | `state/src/economy/geology.rs` (new) | Deposit lookup, depletion, quality decay, depth gating |
| A10 | `state/src/economy/production.rs` | Integrate deposit depletion into `process_building_cycle()` |
| A11 | `state/src/economy/mod.rs` | Export `geology` module |
| A12 | `state/src/engine/turn.rs` | Pass country/formations into production phase for depletion |

### Phase 21B (Exploration & Expansion)

| Step | File | Change |
|---|---|---|
| B1 | `state/src/registries/enums.rs` | Add `Commodity::DisasterMitigationCapacity` |
| B2 | `state/src/entities/mod.rs` | Add `exploration_budget` to `Company` |
| B3 | `state/src/economy/exploration.rs` (new) | Corporate and state exploration logic |
| B4 | `state/src/economy/mod.rs` | Export `exploration` module |
| B5 | `state/src/registries/production_methods_data.rs` | Add Earth Research Center production methods |
| B6 | `state/src/engine/generator/corporate.rs` | Seed Earth Research Center buildings |
| B7 | `state/src/engine/turn.rs` | Call exploration processing in post-parallel phase |
| B8 | `state/src/economy/disasters.rs` | Add `sum_disaster_mitigation_capacity()` |

### Phase 21C (Subsurface Rights & Concessions)

| Step | File | Change |
|---|---|---|
| C1 | `state/src/politics/laws.rs` | Add `SubsurfaceRightsRegime`, `SubsurfaceRightsLaw`, `LawType::SubsurfaceRights` |
| C2 | `state/src/politics/laws.rs` | Add `enact_law` arm for `SubsurfaceRights` |
| C3 | `state/src/state/mod.rs` | Add `subsurface_rights_law` to `Politics` or `Country` |
| C4 | `state/src/entities/mod.rs` | Add `Concession` struct and `concession` field to `Building` |
| C5 | `state/src/economy/subsurface_rights.rs` (new) | Royalty/concession payment processing |
| C6 | `state/src/economy/transfer_settler.rs` | Add `credit_class_savings_region()` (targeted class credit + bank balance-sheet sync) |
| C7 | `state/src/economy/mod.rs` | Export `subsurface_rights` module |
| C8 | `state/src/engine/turn.rs` | Call `process_subsurface_payments()` in post-parallel phase |

### Phase 21D (Tectonics & Volcanism)

| Step | File | Change |
|---|---|---|
| D1 | `state/src/society/geography.rs` | Add `TectonicTrait` enum and `tectonic_trait` field to `Region` |
| D2 | `state/src/society/geography.rs` | Assign tectonic traits in `generate_regional_topology()` |
| D3 | `state/src/economy/disasters.rs` | Add `VolcanicEruption` to `DisasterType` |
| D4 | `state/src/economy/disasters.rs` | Add earthquake and volcanic eruption triggers to `check_disaster_triggers()` |
| D5 | `state/src/economy/disasters.rs` | Add `DisasterMitigationCapacity` mitigation to `DisasterTurnResult` |
| D6 | `state/src/economy/production.rs` | Add volcanic agricultural bonus in `process_building_cycle()` |
| D7 | `state/src/engine/generator/corporate.rs` | Restrict Geothermal Plant to tectonic regions |

### Verification

| Command | Purpose |
|---|---|
| `cargo check` | Compilation |
| `cargo test --lib` | Library tests |
| `cargo test --test supply_chain_integrity_test` | Supply-chain integrity |
| `cargo test --test tech_tree_integrity_test` | Tech-tree integrity |
| `cargo test --test simulation_100_turns` | 100-turn integration |
| `cargo test --test golden_master_test` | Golden Master parity |

---

## RISKS & CONSIDERATIONS

1. **Production parallelism vs. deposit mutation**: **Not a risk.** The main turn loop uses `tasks.par.iter_mut()`, giving each rayon task exclusive `&mut Country` access. Direct mutation of `Country.geological_formations` inside the production cycle is thread-safe. No delta queue needed.

2. **Golden Master parity**: Adding depletion to the production cycle will change output quantities for mining buildings, which will break Golden Master parity. The 100-turn simulation test may need recalibration. Consider gating depletion behind a config flag (`geology_config.depletion_enabled: bool`) that defaults to `true` for new games but can be disabled for Golden Master comparison.

3. **Deposit exhaustion death spiral**: If a deposit is fully depleted (`current_reserves = 0`), the mining building produces nothing. Without exploration or new deposit discovery, the mining company goes bankrupt. This is realistic but could cause supply-chain cascading failures. The corporate lifecycle/bankruptcy system should handle this gracefully — mining companies with exhausted deposits should either explore, relocate, or go bankrupt and be replaced.

4. **Subsurface rights regime switching**: If the regime changes mid-game (e.g., from Private Property to Nationalization), the `enact_law` function handles the transition via **physical ownership transfer** (expropriation). Private mining buildings have their `owner_id` reassigned to a state-owned enterprise. If switching from Nationalization back to Private Property, state-owned mining buildings remain state-owned unless explicitly privatized by a separate law/action.

5. **Formation-to-commodity mapping**: **Eliminated by design.** `generate_formation_resources()` is completely rewritten to natively use `Commodity` enum variants. No Polish strings, no mapping table, no `Option<Commodity>`. The legacy `seed_geological_deposits()` (which writes Polish-keyed JSON into `Region.zasoby`) remains for save compatibility but is no longer authoritative.

6. **Uranium commodity addition**: Adding `Commodity::Uranium` requires updating the commodity count in integrity tests (currently 136 variants). The `all()`, `try_from()`, and `is_active()` functions must include it. A uranium mining method must be added to `production_methods_data.rs`.

7. **DisasterMitigationCapacity as a capacity commodity**: Like `FireProtectionCapacity` and `ShelterCapacity`, this is a non-tradeable capacity commodity. It should be added to the capacity-commodity list in `enums.rs` (the `is_capacity_commodity()` or similar function) so it's excluded from B2B/B2C market logic.
