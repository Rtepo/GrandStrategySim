# Resurrection Phase 19 — Generative Investment Goods, Blueprints & Maintenance

A Dependency Audit of the existing machinery/IP/B2C systems, followed by a three-sub-phase Technical Blueprint (19A Blueprints & IP, 19B Fixed Asset Cohorts & Maintenance-as-a-Service, 19C Quality-Driven Markets) implementing Generative Blueprints, Quality, Durability, and Fixed Asset Maintenance.

> Clarifications confirmed by the user:
> 1. `fixed_assets: Vec<FixedAssetCohort>` goes on **both** `Building` (factory machinery) and `CommercialBuilding` (retail fixtures).
> 2. Fixed-asset commodities = {IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks, Cars}; Quality consumer durables = {Cars, Agd, Televisions, Radio, Furniture, LuxuryFurniture, Clothing, LuxuryClothing} (Cars overlap — channel-dependent role).
> 3. Cross-border blueprint licensing uses a **sequential global queue** (FX outflow in parallel phase, foreign-licensor credit aggregated after parallel tasks, mirroring SEE-remittance/tourism).
>
> **Strict architectural corrections (user-mandated, Round 2):**
> 4. **NO `Commodity::SpareParts` physical inventory.** Maintenance is re-conceptualized as a **B2B Service**: new `Sector::MaintenanceWorkshops` buildings consume generic, highly-available raw materials (`Steel`, `ElectronicComponents`, `MechanicalComponents`) and output the existing unused `Commodity::MaintenanceServices` capacity. Factories buy this service (via TransferSettler) to restore fixed-asset cohorts. This breaks the circular dependency (machinery↔parts) that would freeze a new world / post-shock economy.
> 5. **Technological Obsolescence is mandatory.** `machinery_factor` applies a `TechnologicalGap` penalty: a cohort's `base_tech` is compared against the most advanced technology currently patented in the domestic market; outdated cohorts lose efficiency aggressively toward 0.0, forcing scrap-and-renew investment cycles. A Turn-10 machine must not stay competitive at Turn-500.
> 6. **B2B purchasing is income-segmented, not strictly durability-greedy.** A company's willingness-to-pay for durability/quality is bottlenecked by its `available_cash` (+ credit line via `Borrower`). Cash-poor, struggling factories are *forced* to buy cheap, low-quality, low-durability substitute machinery because they cannot encumber enough to win high-durability bids. This keeps the Generative Substitutes mechanic alive in B2B.

---

## PART 1 — DEPENDENCY AUDIT

### 1.1 B2B Consumption of Investment Goods — *currently instant flow, not stock*
- Investment goods (`IndustrialMachinery`, `ConstructionMachinery`, `AgriculturalMachinery`, `OfficeMachinery`, `Trucks`, `Cars`, `MechanicalComponents`, `ElectronicComponents`) are modeled as **flow commodities**, identical to raw materials.
- In `registries/production_methods_data.rs`, machinery appears as **per-1000-worker inputs** (e.g. construction PM consumes `ConstructionMachinery` 5.0–8.0 / 1k workers; energy PM consumes `ConstructionMachinery` 3.0; agriculture consumes `AgriculturalMachinery` 3.0) and as **outputs** (heavy-industry `Electrified Factories`/`CNC Manufacturing` produce `IndustrialMachinery` 15.0–30.0).
- `economy/b2b_orders.rs::submit_company_b2b_orders` (l.168) submits Buy Bids for **every** input in a building's `active_method.inputs` (machinery included) and Sell Asks for outputs, with cash encumbrance.
- `economy/b2b_orders.rs::execute_production_cycle` (l.675) consumes inputs each turn: `building.inventory[commodity] -= required × fulfillment_ratio` (l.727-740), produces outputs into `inventory` (l.744-750). Machinery is **fully consumed the turn it is bought**; nothing persists.
- **Verdict:** Machinery is instantly consumed as an input flow. There is **no persistence, no durability, no fixed-asset stock**. A factory "burns" 15 IndustrialMachinery / 1000 workers / turn. This is exactly what Phase 19B replaces.

### 1.2 Building Infrastructure — *no machine inventory; capacity is purely labor-driven*
- `Building` (`entities/mod.rs` l.917): production capacity = `worker_capacity × scale_factor × current_employment`. Carries `inventory: BTreeMap<Commodity, f64>` (flat scalar per commodity, no quality/batch), `condition: f64` (0–1, single scalar for the **building shell**, not installed machines), `active_method`, `inventory_capacity`.
- `CommercialBuilding` (`society/housing.rs` l.366): retail/wholesale/office only. Carries `current_inventory: BTreeMap<String, Vec<InventoryBatch>>` (FEFO-batched, `InventoryBatch { quantity, storage_turn, owner_id, accumulated_fees, warehouse_id, fire_sale_discount, acquisition_cost_per_unit }` — l.186), `retail_profile`, `wholesale_profile`. **No `fixed_assets` field.**
- Building-shell `condition` degrades via `economy/maintenance.rs::process_condition_degradation` (l.63, base 0.2%/turn) and is restored by `process_maintenance_spending` (l.90) — a **cash→condition** transaction (double-entry via `debit_company_by_id`/`credit_company_by_id` TransferSettler helpers, crediting a Construction-sector contractor). This is the existing maintenance template Phase 19B generalizes.
- **Verdict:** No concept of a physical machine inventory / fixed assets. Capacity is labor-only. `condition` is one scalar for the shell, not per-machine.

### 1.3 Technology & Royalties (Phase 7/12) — *IP is method-level, no product designs*
- `TechNode` (`registries/tech_tree.rs` l.28): `{ name, year, cost, unlocks_methods (sector→slot→method name), prerequisites, tech_type (Fundamental/Commercial), patent_duration_turns, royalty_vwap_ratio }`.
- `Patent` (`entities/mod.rs` l.97): `{ tech_id, granted_turn, expires_turn, royalty_vwap_ratio }`. `LicensedMethod` (l.114): `{ tech_id, licensor_company_id, licensed_turn }`. Both keyed by **TechId**, i.e. a whole production method.
- `economy/royalties.rs::process_all_royalty_payments` (l.214): VWAP-anchored; private royalties licensee→licensor (mutates `available_cash` directly, l.191-192 — **not** via `settle_transfer`); state patents (`licensor_company_id == "STATE"`) → `treasury.liquid_reserves` (l.256-258). Graceful degradation via fulfillment ratio. Called in `engine/turn.rs` l.1889.
- `economy/corporate_rd.rs::execute_corporate_method_research` (l.88): companies spend `rd_budget` to discover Commercial techs in-sector → grant `Patent`. `allocate_corporate_rd_budget` (l.25) moves excess cash into `rd_budget`.
- **Verdict:** IP exists only at the technology/method level. Companies license **registry-defined production methods**, never a specific product design. **No `ProductBlueprint`**; companies design nothing. No product-level licensing.

### 1.4 B2C Market Clearing — *price-only utility, no quality, no wealth segmentation*
- `StoreOffer` (`economy/retail.rs` l.19): `{ store_id, commodity, quantity, price_per_unit, effective_attractiveness }`. **No quality field.**
- `build_consumer_demand` (l.206): per-capita needs × population from `data/consumption_registry`, tiers Subsistence→Standard→Luxury, cultural taboo/obsession modifiers (authority-scaled). Demand is keyed by `Commodity` only — all units of a commodity are identical.
- `clear_b2c_markets` (l.338): `utility = (1.0 / price_per_unit) + inertia_bonus` (l.382). Greedy allocation by sorted utility (largest-remainder-ish). **No quality, no price-to-quality, no wealth-class preference.**
- `settle_b2c_clearing` (l.451): revenue split across citizen classes by demand share, debited from class savings via `settle_b2c_purchase` (TransferSettler) — already double-entry-compliant.
- `WealthBracket` enum exists (`registries/enums.rs` l.96: VeryHigh/High/Medium/Low) but is **not used** in demand/clearing.
- **Verdict:** Consumers care only about price (+ brand inertia + cultural mods). No quality. All units of a commodity are fungible.

### Integration map (turn sequencing in `engine/turn.rs`)
| Phase | Line | Function | Phase 19 hook |
|---|---|---|---|
| 6.3 B2B order submit | 530 | `submit_company_b2b_orders` | 19B maintenance-service bids + 19C asset bids w/ blueprint_id (cash-bottlenecked limit price); 19C asks carry blueprint/quality |
| 6.4a trade settle | 562 | `settle_trades_with_tariffs` | 19B strict-TransferSettler maintenance-service cash leg (filtered out of `settle_trades_with_tariffs`) |
| 8 wave production | 858/907 | `execute_production_cycle` | 19A output→cohort w/ blueprint; 19B skip machinery consumption, capacity from fixed_assets **× obsolescence factor** |
| 15A degrade | 832 | `process_condition_degradation` | 19B cohort condition degrade |
| 15A maintain | 1816 | `process_maintenance_spending` | 19B cohort maintenance via MaintenanceServices (condition restore after service trades settle) |
| 9.2 royalties | 1889 | `process_all_royalty_payments` | 19A blueprint royalties + cross-border queue |
| 6.5 B2C | (later) | `clear_b2c_markets` / `settle_b2c_clearing` | 19C quality-segmented clearing |

---

## PART 2 — TECHNICAL BLUEPRINT & PHASING STRATEGY

### Shared foundations (built incrementally, used by all sub-phases)

**New module `economy/blueprints.rs`** — the generative core.

```rust
/// A product design created by a company from known technologies + chosen materials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductBlueprint {
    pub id: String,                       // deterministic hash(owner+output+tech+inputs+turn)
    pub owner_company_id: String,         // designer / licensor
    pub output_commodity: Commodity,      // e.g. IndustrialMachinery, Cars, Agd...
    pub base_tech: TechId,                // underlying Commercial tech (must be patented or licensed)
    pub inputs: BTreeMap<Commodity, f64>, // chosen bill of materials (incl. substitutes)
    pub required_slot: MethodSlot,        // production / automation
    pub quality: f64,                     // 0.0..~2.0, computed from materials + tech
    pub durability: f64,                  // expected lifespan in turns (maintenance cadence)
    pub royalty_vwap_ratio: f64,          // licensing fee = qty × ratio × last_turn_vwap
    pub granted_turn: u32,
    pub expires_turn: u32,                // patent-style expiry
}

/// A blueprint a company has licensed from another (domestic or foreign).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicensedBlueprint {
    pub blueprint_id: String,
    pub licensor_company_id: String,      // "STATE" for state designs
    pub licensed_turn: u32,
}
```

**New registry `registries/blueprint_specs.rs`** — defines, per output commodity, the *ideal* material roles and acceptable substitutes (the generative axis):
```rust
pub struct MaterialRole {
    pub ideal: Commodity,                 // e.g. Aluminum
    pub substitutes: Vec<(Commodity, f64, f64)>, // (substitute, quality_factor, durability_factor)
    pub share: f64,                       // fraction of bill this role occupies
}
pub struct BlueprintSpec {
    pub commodity: Commodity,
    pub roles: Vec<MaterialRole>,
    pub base_quality: f64,
    pub base_durability_turns: f64,
}
```
Quality = `base_quality × Σ(role.share × material_quality_factor)`; Durability = `base_durability × Σ(role.share × material_durability_factor)`. Substituting Iron for Aluminum lowers both factors but Iron is cheaper (market-priced), so the designer trades cost vs quality/durability. **This is the "Generative Substitutes" engine.**

**`Commodity` classification helpers** (in `registries/enums.rs` or `blueprints.rs`):
- `FIXED_ASSET_COMMODITIES = {IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks, Cars}` — when bought B2B by a company → installed as a `FixedAssetCohort`.
- `QUALITY_CONSUMER_DURABLES = {Cars, Agd, Televisions, Radio, Furniture, LuxuryFurniture, Clothing, LuxuryClothing}` — B2C goods that carry blueprint quality.
- `is_fixed_asset(c)`, `is_quality_durable(c)` predicates. (Cars/Trucks are in both: B2B purchase → asset; B2C purchase → durable.)

**New config `economy/generative_goods_config.rs`** (`GenerativeGoodsConfig` on `Country`): all tuning knobs — cohort caps, degradation rate, maintenance-per-condition-point, obsolescence aggressiveness `k`, allow-asset-purchase-on-credit flag, quality weights per wealth tier, asset-capacity multipliers, blueprint design cost, cross-border royalty FX rate. `#[serde(default)]` for backward save compat.

---

### Phase 19A — Generative Blueprints & Intellectual Property

**Goal:** companies design `ProductBlueprint`s from known techs + chosen materials (with substitutes), and license them domestically and cross-border.

**A1. Blueprint design decision** (new `corporate/blueprint_strategy.rs`, hooked into the corporate strategy/manager phase alongside `execute_corporate_method_research`):
- A company with a patented (or licensed) Commercial `base_tech` for an output commodity may spend `rd_budget` (a `blueprint_design_cost` from config) to design a blueprint.
- The strategy enumerates material choices per `BlueprintSpec` role (ideal vs each substitute) and picks the bundle maximizing a target (e.g. margin-weighted `quality × durability / expected_input_cost`), bounded by a small search (role count is small → cheap).
- Result: a `ProductBlueprint` stored on `Company.blueprints: Vec<ProductBlueprint>` (new field).

**A2. Generative substitutes:** encoded entirely by `BlueprintSpec.roles[].substitutes`; the design search above is what makes substitution generative. No hardcoded recipes.

**A3. Linking production to blueprints:**
- `ActiveProductionMethod` (`entities/mod.rs` l.881) gains `active_blueprint: Option<String>` (blueprint_id) per output commodity it designs against. When `execute_production_cycle` produces an output that is blueprint-eligible and the building has an active blueprint, the output batch carries that blueprint's `quality` (→ cohort in 19C).
- New `Company.blueprints: Vec<ProductBlueprint>` and `Company.licensed_blueprints: Vec<LicensedBlueprint>` fields (`#[serde(default)]`).

**A4. IP & Licensing — domestic:** extend `economy/royalties.rs`:
- New `process_blueprint_royalty_payments` mirroring `process_all_royalty_payments` but iterating `licensed_blueprints`. Fee = `actual_output_qty × blueprint.royalty_vwap_ratio × last_turn_vwap(output)`.
- **Strict TransferSettler:** the cash leg MUST use `settle_transfer(... TransferRecipient::OtherCompany { recipient_idx })` (or `Treasury` for state designs) instead of the current direct `available_cash` mutation. This both complies with the Phase 19 double-entry rule and **fixes** the existing royalties path's bank-balance-sheet drift (note: existing `process_all_royalty_payments` mutates `available_cash` directly without syncing `bank.balance_sheet` — a latent inconsistency; 19A's new path will be correct-by-construction and the old path may be migrated later).

**A5. IP & Licensing — cross-border (sequential global queue):**
- During the parallel per-country royalty pass, a domestic licensee paying a **foreign** licensor emits an FX outflow: `settle_transfer(... TransferRecipient::ForeignEntity)` (money leaves the domestic banking system) and pushes `(licensor_company_id, licensor_country, amount, blueprint_id)` onto a `CrossBorderRoyaltyQueue` collected from all tasks.
- After parallel tasks complete, a sequential pass (mirroring `see_remittance` / `tourism_result` aggregation in `turn.rs`) credits each foreign licensor's brokerage account via `credit_company_by_id` and syncs the recipient bank's balance sheet. **Both sides double-entry; deterministic.**

**A6. Turn integration:** blueprint design runs in the corporate strategy phase; blueprint royalties run alongside `process_all_royalty_payments` at turn.rs l.1889; cross-border credits run in the sequential post-parallel section.

**Files touched (19A):** new `economy/blueprints.rs`, new `registries/blueprint_specs.rs`, new `economy/generative_goods_config.rs`, new `corporate/blueprint_strategy.rs`; edit `entities/mod.rs` (Company + ActiveProductionMethod fields), `economy/royalties.rs`, `economy/mod.rs` (exports), `engine/turn.rs` (hooks), `state/...` (Country config field).

---

### Phase 19B — Fixed Asset Cohorts, Maintenance-as-a-Service & Technological Obsolescence

**Goal:** machinery is no longer an instant input; it is purchased into durable `FixedAssetCohort`s that degrade, become technologically obsolete, and must be restored by purchasing a **B2B Maintenance Service** (not a physical spare-parts commodity) from dedicated workshop buildings.

**B1. Cohort structs** (new `economy/fixed_assets.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixedAssetCohort {
    pub blueprint_id: String,
    pub commodity: Commodity,     // IndustrialMachinery, Trucks, Cars, ...
    pub count: f64,               // number of machines in this cohort (cohort size)
    pub condition: f64,           // 0.0..1.0 average condition
    pub quality: f64,             // cached from blueprint (affects capacity + degradation)
    pub durability: f64,          // turns-to-fully-degrade (from blueprint)
    pub base_tech: TechId,        // cached from blueprint (for obsolescence penalty)
    pub base_tech_year: u32,      // cached from TechNode (for obsolescence penalty)
    pub acquired_turn: u32,
}
```
**Fields added:** `Building.fixed_assets: Vec<FixedAssetCohort>` (`entities/mod.rs`) and `CommercialBuilding.fixed_assets: Vec<FixedAssetCohort>` (`society/housing.rs`) — both `#[serde(default)]`, backward-compatible with old saves (empty → behaves like today).

**B2. Maintenance as a B2B Service (NO SpareParts commodity).**
- **Reuse the existing, currently-unused `Commodity::MaintenanceServices`** (enum l.372, already in `all()` + `TryFrom`, zero enum churn) as the maintenance-service capacity output. (If a distinct name is preferred later, a `MaintenanceCapacity` alias variant is a trivial swap — but reusing the existing placeholder is the conservative choice.)
- **New `Sector::MaintenanceWorkshops`** (`registries/enums.rs`): a new commercial sector. Its production methods (added to `production_methods_data.rs`) consume **generic, highly-available raw materials** — `Steel`, `MechanicalComponents`, `ElectronicComponents`, `Energy`, `Fuels` — and output `Commodity::MaintenanceServices`. Era-gated methods scale capacity (manual → electrified → CNC repair shops). This is the **circular-dependency breaker**: workshops never consume machinery or MaintenanceServices as inputs, so a new world / post-shock economy can always bootstrap maintenance from basic raw materials that any mining/light-industry sector produces. No deadlock is possible.
- `MaintenanceServices` is a **service-capacity commodity**, following the exact precedent of `SecurityCapacity` (Komisariat outputs it from Rifles+Cars+Paper, `production_methods.rs:218`), `JusticeCapacity`, `FireProtectionCapacity`, etc. It is produced into `Building.inventory` like any output and traded on the B2B order book (Buy Bids from factories with degrading cohorts; Sell Asks from workshops).
- **No `Commodity::SpareParts` variant is created.** The original 19B design that introduced a physical SpareParts commodity is **rejected** — it would create the machinery↔parts circular dependency the user identified.

**B3. Production capacity rework + Technological Obsolescence** (`economy/fixed_assets.rs` + `economy/b2b_orders.rs::execute_production_cycle`):
- For `FIXED_ASSET_COMMODITIES` in `method.inputs`, **skip per-turn consumption** (machinery is no longer burned). Capacity instead comes from installed cohorts.
- **TechnologicalGap penalty (mandatory):** for each cohort, compute
  `obsolescence_factor = clamp(1.0 - k × (frontier_year - cohort.base_tech_year) / frontier_year, 0.0, 1.0)`
  where `frontier_year` = the highest `TechNode.year` among all **currently-patented-or-known** technologies for this commodity's sector in the domestic market, and `k` is an obsolescence aggressiveness knob from `GenerativeGoodsConfig` (default ~2.0 so a 50-year-old tech contributes near-zero). A Turn-10 machine at Turn-500 has a tiny `obsolescence_factor` → its `machinery_factor` contribution collapses toward 0, forcing the factory to scrap and buy modern machinery to stay competitive. This is the investment-cycle engine.
- `machinery_factor = 1.0 + Σ_cohort(count × quality × condition × obsolescence_factor × machine_unit_capacity)` (config-scaled). The `1.0` baseline = manual mode for empty cohorts → **no save breakage, no GDP cliff on cutover**.
- Effective production scale = `employment/1000 × machinery_factor × efficiency`. Buildings with cohorts but no labor still need workers; machinery multiplies labor productivity, doesn't replace it.

**B4. Machinery acquisition (B2B → cohort):**
- In `submit_company_b2b_orders`, a building that wants to expand/replace capacity places a Buy Bid for a fixed-asset commodity carrying the desired `blueprint_id` (19C extends `Ask`/`Bid` with optional blueprint metadata — see C3).
- On settlement, a fixed-asset trade does **not** route to `Building.inventory`; instead a new `install_fixed_asset` helper appends a `FixedAssetCohort { blueprint_id, commodity, count: trade.quantity, condition: 1.0, quality, durability, base_tech, base_tech_year, acquired_turn }` to the buyer building's `fixed_assets`, then runs **cohort compaction** (B7).

**B5. Degradation & maintenance-as-a-service** (new `economy/fixed_assets.rs`, paralleling `maintenance.rs`):
- Each turn: `cohort.condition -= (1.0 / cohort.durability) × stress_factor` (stress from building condition/disruption). Clamp [0,1]. If `condition ≤ 0` → cohort scrapped (`count` dropped, capacity falls). High-quality blueprints degrade slower (durability is per-blueprint). Obsolescence does *not* change condition — it changes the *efficiency contribution* (B3), so an obsolete-but-pristine machine is still worth little. Factories are thus pushed to scrap for *modernity*, not just for *wear*.
- **Maintenance = buying MaintenanceServices capacity**, not consuming a physical parts inventory:
  `maintenance_services_needed = Σ_cohort count × (1.0 - condition) × maintenance_per_condition_point` (config).
  The factory submits a Buy Bid for `Commodity::MaintenanceServices` on the B2B order book (derived demand from its total cohort condition deficit). When the bid is filled, the bought `MaintenanceServices` quantity is **consumed immediately** (it is a service, not stockpiled) to restore cohort condition: `cohort.condition += restored / (count × maintenance_per_condition_point)`, capped at `max_restore_per_turn` (config). If the bid is only partially filled → partial restore; remaining condition stays low (factory rusts until workshops catch up).
- **Double-entry for the maintenance service payment:** the cash leg of the MaintenanceServices trade routes through the strict TransferSettler path (B6). The service quantity itself is ephemeral (consumed on delivery, not stored), so there is no inventory-routing leg — only the cash leg, which is strictly double-entry.

**B6. MaintenanceServices B2B market — strict TransferSettler:**
- MaintenanceServices Buy Bids are submitted in `submit_company_b2b_orders` (derived demand from cohort condition deficit × `maintenance_per_condition_point`). Sell Asks come from MaintenanceWorkshops buildings.
- **New `settle_maintenance_service_trades`** (in `b2b_orders.rs` or `fixed_assets.rs`): performs **only the cash leg**, routing it through `settle_transfer(... TransferRecipient::OtherCompany)` so bank balance sheets (deposits + reserves) sync on both sides — strictly complying with the Phase 19 double-entry rule. (The legacy `settle_trades` mutates `available_cash`/`brokerage_account.cash` without syncing `bank.balance_sheet`; 19B's maintenance path will be the reference correct implementation and may later be generalized to all B2B.) Tariffs on cross-border maintenance-service trades are still collected via the existing tariff pass. The condition restoration is applied to the buyer's cohorts in the same pass.
- Run order: maintenance-service bids join the normal global order book; their trades are filtered out of `settle_trades_with_tariffs` and routed through `settle_maintenance_service_trades` instead (identified by `commodity == MaintenanceServices`).

**B7. Cohort compaction (memory safety — see dedicated section):** caps `fixed_assets.len()` per building (default 12) by merging same-blueprint cohorts (count-sum, condition-weighted average) or closest-condition cohorts when the cap is exceeded. Same compaction reused for inventory cohorts in 19C.

**B8. Turn integration:** degradation runs next to `process_condition_degradation` (turn.rs l.832); obsolescence factor is recomputed each turn inside `machinery_factor` (frontier_year may advance as new patents are granted); maintenance-service condition restoration runs next to `process_maintenance_spending` (l.1816) after the maintenance-service trades have settled in the trade-settlement block (l.554).

**Files touched (19B):** new `economy/fixed_assets.rs`; edit `registries/enums.rs` (new `Sector::MaintenanceWorkshops` variant + display_name; **no** new commodity — reuse `MaintenanceServices`), `registries/production_methods_data.rs` (MaintenanceWorkshops PMs consuming Steel/MechanicalComponents/ElectronicComponents/Energy → MaintenanceServices; remove any machinery inputs from these PMs to preserve the no-circular-dependency invariant), `entities/mod.rs` + `society/housing.rs` (fixed_assets fields), `economy/b2b_orders.rs` (capacity rework with obsolescence, install, maintenance-service bids, strict settlement), `engine/turn.rs` (hooks).

---

### Phase 19C — Quality-Driven Markets (B2B & B2C)

**Goal:** goods on the market are tied to their `ProductBlueprint` (so quality/durability is known per batch); consumers buy on price-to-quality with wealth segmentation; factories buy assets on price-to-durability.

**C1. Inventory cohort tracking** (memory-bounded):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryCohort {
    pub blueprint_id: String,
    pub quantity: f64,
    pub quality: f64,
    pub storage_turn: u32,
}
```
- Add `Building.inventory_cohorts: BTreeMap<Commodity, Vec<InventoryCohort>>` (`#[serde(default)]`). **Keep** `Building.inventory: BTreeMap<Commodity, f64>` as the aggregate (sum of cohort quantities) so all existing production/B2B code still compiles and works for non-blueprint goods.
- Cohorts are created **only** for blueprint-eligible outputs (quality durables + fixed assets while in inventory pre-install). Raw materials (Iron, Steel, Energy, …) stay flat in `inventory` — no cohorts. This bounds memory.
- Production output push: append an `InventoryCohort` (blueprint_id, quality, qty, turn). Consumption/pop: FIFO by `storage_turn`, decrement quantity; on any change, recompute the aggregate `inventory[commodity] = Σ cohort.quantity`. Run cohort compaction when a commodity's cohort count exceeds its cap (default 8).

**C2. Store offers carry quality** (`economy/retail.rs`):
- `StoreOffer` gains `quality: f64` and `blueprint_id: Option<String>` (quality = quantity-weighted average of the cohorts behind the offer). Old offers default `quality = 1.0`, `blueprint_id = None` → unchanged behavior for non-blueprint goods.

**C3. B2C clearing — price-to-quality with wealth segmentation** (`clear_b2c_markets`):
- Replace `utility = 1.0/price + inertia` with **per-wealth-tier utility**: `utility = (quality ^ α_tier) / price + inertia`, where `α_tier` comes from `GenerativeGoodsConfig.quality_weights` keyed by `WealthBracket` (VeryHigh/High → high α, quality-loving; Medium/Low → low α, price-sensitive). Map each demographic class to a `WealthBracket` from its relative income/savings (computed in `build_consumer_demand`).
- **Segmented allocation:** loop wealth tiers **poor→rich**; each tier allocates its demand against the **remaining** offer quantities using its own utility ranking. Poor tiers consume cheap/low-quality offers first; rich tiers then buy high-quality leftovers. This produces real market segmentation (luxury vs mass-market) without per-individual tracking.
- Settlement still uses `settle_b2c_clearing` (already TransferSettler-compliant) — no double-entry change needed, only the allocation policy.

**C4. B2B clearing — cash-constrained price-to-durability for fixed assets (income-segmented)** (`submit_company_b2b_orders` + order book):
- Extend `Ask`/`Bid` (`economy/order_book.rs`) with `blueprint_id: Option<String>`, `quality: Option<f64>`, `durability: Option<f64>` (`#[serde(default)]`, backward compatible — None for non-blueprint goods). Matching stays price-time priority.
- **Willingness-to-pay is durability-aware AND cash-bottlenecked** (the user's strict correction — B2B must be income-segmented like B2C, or cheap low-quality substitutes never sell):
  `wtp = reference_price × (1 + durability_factor(durability, quality))`  ← the *desire*
  `affordable_wtp = min(wtp, liquid × max_cash_encumbrance_ratio / desired_qty)`  ← the *wallet*
  `limit_price = affordable_wtp`
  where `liquid = computed_liquid_capital()` (+ optional credit-line headroom via the `Borrower` trait / banking system, gated by `GenerativeGoodsConfig.allow_asset_purchase_on_credit`).
- **Effect (the income segmentation):** a cash-poor, struggling factory has a tiny `affordable_wtp` → its `limit_price` is clamped low → it **cannot** win matching against premium high-durability asks and is *forced* to bid on cheap, low-quality, low-durability substitute machinery (which has lower ask prices it *can* afford). A cash-rich company bids near the full `wtp` and wins the premium assets. This mirrors B2C wealth segmentation (C3) and keeps the Generative Substitutes mechanic economically meaningful in B2B — cheap substitute blueprints now have a real market among capital-constrained buyers.
- `match_orders` is unchanged (still sorts by `limit_price`); both the durability preference *and* the cash constraint are encoded entirely in the buyer's `limit_price`, keeping the matching engine untouched. The existing encumbrance guard (`max_encumber = liquid * max_cash_encumbrance_ratio`, `b2b_orders.rs:187`) already prevents over-bidding; the new logic simply makes the *per-asset* limit price reflect the company's cash position rather than a flat reference-price premium.

**C5. Turn integration:** cohort-aware offer generation in `generate_store_offers`; quality-segmented `clear_b2c_markets`; blueprint-aware B2B bid/ask submission. All in the existing 6.5 B2C and 6.3 B2B phases.

**Files touched (19C):** edit `economy/retail.rs` (StoreOffer, clear_b2c_markets, build_consumer_demand wealth mapping), `economy/order_book.rs` (Ask/Bid fields), `economy/b2b_orders.rs` (cohort inventory in settle + production, blueprint-aware asks/bids), `entities/mod.rs` (inventory_cohorts field), `engine/turn.rs` (cohort sync points).

---

## MEMORY SAFETY — COHORTS (explicit)

The cardinal rule: **never track individual items; track aggregates of identical provenance.**

1. **Fixed-asset cohorts:** one `FixedAssetCohort` = N identical machines (same blueprint, same acquire turn, average condition). A factory with 500 lathes is 1 cohort, not 500 structs. Condition is a cohort-level scalar (average); degradation applies to the scalar, not per-machine.
2. **Inventory cohorts:** one `InventoryCohort` = a batch of units sharing `blueprint_id + quality + storage_turn`. Created only for blueprint-eligible commodities (durables + pre-install assets). Raw materials never cohort.
3. **Caps + compaction:**
   - `MAX_FIXED_COHORTS_PER_BUILDING = 12`, `MAX_INVENTORY_COHORTS_PER_COMMODITY = 8` (config).
   - On insert over the cap, merge: prefer same `blueprint_id` (sum `count`/`quantity`, condition-/quantity-weighted average `condition`/`quality`); else merge the two cohorts with the closest `condition`/`storage_turn`.
4. **Worst-case bound:** `buildings × MAX_FIXED_COHORTS + buildings × commodities × MAX_INVENTORY_COHORTS`. For 5k buildings × 12 = 60k fixed-asset structs; inventory cohorts only for ~10 blueprint commodities × 8 = 80/building → 400k structs. All small fixed-size structs (no per-item allocation). RAM stays flat and predictable.
5. **Aggregate mirror:** `Building.inventory` (flat sum) is always recomputed from cohorts, so existing O(1) lookups (`inventory.get(&commodity)`) are unchanged — no perf regression in hot paths.

---

## DOUBLE-ENTRY COMPLIANCE (explicit)

- **Licensing fees (19A domestic):** `settle_transfer(TransferRecipient::OtherCompany)` — debits licensee `brokerage_account.cash` + payer bank deposits/reserves, credits licensor + recipient bank. State designs → `TransferRecipient::Treasury`.
- **Licensing fees (19A cross-border):** licensee side `settle_transfer(TransferRecipient::ForeignEntity)` (FX outflow, bank reserves exit) + `CrossBorderRoyaltyQueue` entry; sequential post-parallel pass credits foreign licensor via `credit_company_by_id` + recipient bank sync. Both sides balance.
- **B2B maintenance-service transactions (19B):** new `settle_maintenance_service_trades` routes the cash leg through `settle_transfer(TransferRecipient::OtherCompany)` (strictly syncing bank balance sheets), unlike legacy `settle_trades` which mutates cash without bank sync. Tariffs still collected via the existing tariff pass. The service quantity is consumed on delivery (no inventory-routing leg).
- **Maintenance contractor payment for building shells (unchanged 19B neighbor):** `debit_company_by_id` → `credit_company_by_id` (TransferSettler helpers, mirroring existing `process_maintenance_spending`).
- **B2C (19C):** unchanged — `settle_b2c_clearing` already uses `settle_b2c_purchase` (TransferSettler).
- No new money is created or destroyed outside `settle_transfer`/its helpers/Treasury paths.

---

## FILES TO CREATE / MODIFY

**Create:**
- `economy/blueprints.rs` — `ProductBlueprint`, `LicensedBlueprint`, quality/durability calc, design search.
- `registries/blueprint_specs.rs` — `BlueprintSpec`/`MaterialRole` registry (ideal + substitutes per commodity).
- `economy/generative_goods_config.rs` — `GenerativeGoodsConfig` (all knobs incl. obsolescence aggressiveness `k`, maintenance_per_condition_point, asset-purchase-on-credit flag; on `Country`).
- `economy/fixed_assets.rs` — `FixedAssetCohort`, degradation, **TechnologicalGap/obsolescence factor**, maintenance-via-MaintenanceServices, cohort compaction, `install_fixed_asset`, `settle_maintenance_service_trades` (or in b2b_orders).
- `corporate/blueprint_strategy.rs` — blueprint design decision (hooks corporate manager/strategy).

**Modify:**
- `registries/enums.rs` — **new `Sector::MaintenanceWorkshops`** variant + `display_name`; `is_fixed_asset`/`is_quality_durable` predicates. (**No new commodity** — reuse existing unused `Commodity::MaintenanceServices`.)
- `registries/production_methods_data.rs` — **MaintenanceWorkshops PMs** consuming `Steel`/`MechanicalComponents`/`ElectronicComponents`/`Energy`/`Fuels` → `Commodity::MaintenanceServices` (era-gated; **no machinery/MaintenanceServices inputs** to preserve the no-circular-dependency invariant).
- `entities/mod.rs` — `Company.blueprints`, `Company.licensed_blueprints`; `ActiveProductionMethod.active_blueprint`; `Building.fixed_assets`, `Building.inventory_cohorts`.
- `society/housing.rs` — `CommercialBuilding.fixed_assets`.
- `economy/order_book.rs` — `Ask`/`Bid` optional `blueprint_id`/`quality`/`durability`.
- `economy/b2b_orders.rs` — capacity-from-cohorts **with obsolescence penalty** in `execute_production_cycle`; skip machinery consumption; cohort inventory in `settle_trades`/production; blueprint-aware asks/bids **with cash-bottlenecked limit price (income segmentation)**; `install_fixed_asset`; maintenance-service bids.
- `economy/retail.rs` — `StoreOffer` quality/blueprint_id; wealth-mapped demand; price-to-quality segmented `clear_b2c_markets`.
- `economy/royalties.rs` — `process_blueprint_royalty_payments` (TransferSettler); cross-border queue emission.
- `economy/mod.rs` + `corporate/mod.rs` — exports.
- `engine/turn.rs` — hooks for design, blueprint royalties, cross-border sequential credits, fixed-asset degradation/maintenance, maintenance-service settlement.
- `state/...` (Country) — `generative_goods_config` field.

---

## VERIFICATION STRATEGY

- **Build/typecheck:** `cargo build` then `cargo build --release` (Windows/PowerShell).
- **Unit tests (per sub-phase):**
  - 19A: blueprint quality/durability calc with ideal vs substitute materials (assert Iron-substitute lowers both); royalty fee = qty×ratio×vwap; cross-border queue balances (outflow == inbound credit).
  - 19B: cohort degradation `condition -= 1/durability`; scrap at 0; **TechnologicalGap**: a cohort with `base_tech_year` 50 years behind frontier → `obsolescence_factor ≈ 0` → `machinery_factor` contribution ≈ 0; **no circular dependency**: a MaintenanceWorkshops PM whose inputs are only `Steel`+`Energy` (no machinery/MaintenanceServices) can run in a cold-start world; maintenance-service trade cash leg syncs both banks (deposits+reserves); condition restored proportionally to MaintenanceServices bought; cohort compaction respects cap.
  - 19C: poor tier buys low-quality first, rich tier buys high-quality; **B2B income segmentation**: a cash-poor company's `limit_price` is clamped below premium asks → it can only win low-quality substitute asks; a cash-rich company wins the premium ask; aggregate `inventory` == Σ cohorts after push/pop.
- **Integration:** run a handful of turns on an existing save (empty `fixed_assets`/`blueprints` → `machinery_factor=1.0`, behavior == today) to prove no GDP cliff / no save breakage. Then a **cold-start test**: a fresh world with only basic raw-material sectors + one MaintenanceWorkshops building → confirm maintenance never deadlocks (workshops need no machinery to produce MaintenanceServices).
- **Memory:** assert cohort counts never exceed caps after a long run (instrument a debug counter).
- **Golden-master:** compare pre/post VWAPs for a control run with config knobs at neutral values.

## RISKS / CONSIDERATIONS

- **Production-capacity cutover:** removing machinery as a per-turn input changes GDP composition. Mitigated by `machinery_factor=1.0` default for empty cohorts + config scaling; tuning via `GenerativeGoodsConfig`.
- **Circular dependency (RESOLVED):** the rejected `Commodity::SpareParts` design would have frozen cold-starts/shocks (machinery needs parts, parts need machinery). The Maintenance-as-a-Service design breaks this — MaintenanceWorkshops consume only generic raw materials and never machinery, so maintenance can always bootstrap from basic mining/light-industry output. **Verified by construction** (PM inputs enumerated in B2).
- **Technological obsolescence tuning:** too-aggressive `k` obsoletes machinery too fast (capex treadmill); too-low `k` lets old machines dominate forever (no investment cycle). Default `k ≈ 2.0` is a starting point; needs tuning per sector in `GenerativeGoodsConfig`. The frontier-year lookup must be cheap (cache per-country per-turn).
- **B2B income segmentation edge case:** a company with `available_cash ≈ 0` and credit disabled cannot buy *any* asset → its cohorts decay to scrap and it shuts down. This is intended (creative destruction) but must not cascade into a depression; mitigated by the `machinery_factor=1.0` manual baseline (a stripped factory still produces at base labor productivity, just without the machinery multiplier) and by credit-line headroom (config-gated).
- **Save compatibility:** all new fields `#[serde(default)]`; `Building`/`CommercialBuilding` keep `#[serde(flatten)] extra` catch-all. Old saves load with empty cohorts/blueprints.
- **Legacy royalties bank-drift:** existing `process_all_royalty_payments` mutates `available_cash` without bank sync. 19A's blueprint path is correct; migrating the legacy path is out of scope but noted.
- **`settle_trades` vs TransferSettler:** 19B introduces a strict maintenance-service path but does **not** rewrite all B2B settlement (avoid destabilizing the whole economy). Flagged for a future phase.
- **Cross-border licensing determinism:** sequential post-parallel crediting must be order-stable (sort by `(licensor_country, licensor_id, blueprint_id)`).
- **Cars/Trucks dual role:** same commodity is a B2B asset (cohort) and a B2C durable (cohort in store). Role is determined by the transaction channel, not the commodity alone — handled in `submit_company_b2b_orders` (asset bid) vs `generate_store_offers` (durable offer).
