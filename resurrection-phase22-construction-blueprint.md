# Resurrection Phase 22 — Construction Market, Corporate Fraud & Regulatory Oversight

**Blueprint & Dependency Audit for B2B tenders, consortia, milestone payments, corporate fraud (cutting corners), structural defects, fleet-based mobile inspectorates, bribery, and private lawsuits.**

---

## PART 1: DEPENDENCY AUDIT

### 1.1 Construction System — Current State

#### How buildings are constructed today

Construction is a **self-build, investor-as-contractor** model. There is no separation between the Investor (who wants a building) and the Contractor (who builds it). The flow is:

1. **Decision** (`corporate/strategy.rs`): A company's strategy function (e.g. `joint_stock_expansion`, `family_expansion`) emits a `CorporateAction::Expand { investment, new_workers, finance }`. The `apply_action` helper sets `company.pending_expansion = Some(PendingExpansion { ... })`.

2. **Project creation** (`corporate/manager.rs`, lines 69–104): `process_companies` consumes `pending_expansion`, finds the first owned building without an `active_project`, computes an expansion BOM via `construction::bom::get_expansion_bom`, and attaches a `ConstructionProject` to `building.active_project`. The project is created **directly on the investor's own building** — no tender, no bid, no contractor.

3. **Material sourcing** (`construction/orders.rs`, `submit_construction_b2b_orders`): Each turn, the investor company submits B2B **buy bids** on the `OrderBook` for the missing materials in its projects. Cash is encumbered (`available_cash -= encumbrance`, `debit_cash += encumbrance`). Bids are clamped to `max_cash_encumbrance_ratio` of liquid capital. There is no concept of a contractor buying materials on behalf of the investor.

4. **Progress** (`construction/orders.rs`, `advance_construction_projects`): Each turn, delivered materials are consumed from `building.inventory` into `project.delivered_materials`. Progress = `min(delivered/required)` across all BOM lines. The project completes when all materials are delivered. On completion: `building.worker_capacity += cap_increase`, `company.fixed_capital += capital_increase`, `building.active_project = None`.

#### Critical findings

- **No cash deduction for construction labor or contractor margin.** The only cash flow is the B2B purchase of materials. There is no labor cost, no contractor fee, no profit margin. Buildings effectively "build themselves" for the cost of raw materials.
- **No contractor entity.** `Sector::Construction` companies exist (seeded in `engine/generator/corporate.rs` with `ConstructionMachinery` cohorts) and produce `Commodity::ConstructionServices` via `construction_methods()` in `registries/production_methods_data.rs`. But `ConstructionServices` is **never consumed by `ConstructionProject`** — it is a generic tradeable good with no link to actual building construction.
- **No milestone payments.** The investor pays incrementally as materials are delivered (via B2B settlement), but there is no tranche structure, no escrow, no payment-on-milestone.
- **No quality control.** The BOM is fixed by `get_construction_bom`. There is no mechanism to substitute cheaper materials, no `structural_defect` field on `Building`, no inspection of the construction process itself.
- **`ConstructionProject` struct** (`construction/projects.rs`) has fields: `id`, `project_type`, `micro_region_id`, `target_building_type`, `required_materials`, `delivered_materials`, `target_capacity_increase`, `target_capital_increase`, `is_new_building`, `total_cost`, `cost_spent`, `duration_turns`, `turns_elapsed`, `progress`, `on_hold`, `hold_reason`. There is **no `contractor_id`, no `investor_id`, no `tranches`, no `defects`**.

#### Cash & material flow diagram (current)

```
Investor Company ──(B2B buy bid)──> OrderBook ──(settlement)──> Material Supplier
       │                                                                  │
       └──── encumber cash ──── bank balance sheet synced via TransferSettler
                                  on bid settlement

Material delivered to Building.inventory
       │
       v
advance_construction_projects: consume inventory → delivered_materials
       │
       v (on completion)
Building.worker_capacity += N;  Company.fixed_capital += K
```

### 1.2 Inspectorates — Current State

#### How inspection capacity is produced and consumed

`economy/inspectorates.rs` (`process_inspectorates_turn`) implements three inspectorates:

1. **Sanepid (Sanitary)**: inspects `Sector::Agriculture | LightIndustry | MedicalServices` companies whose buildings have `condition < 0.5`.
2. **Building Inspectorate**: inspects any building with `condition < 0.3`.
3. **Environmental Inspectorate**: inspects `Sector::Mining | HeavyIndustry | Energy | WasteManagement` companies with `pollution_proxy > 100`.

Capacity is summed from `building.last_production` for three `Commodity` variants:
- `Commodity::SanitaryInspectionCapacity`
- `Commodity::BuildingInspectionCapacity`
- `Commodity::EnvironmentalInspectionCapacity`

Coverage ratio = `min(1.0, capacity / target_count)`. Violations trigger fines via `settle_transfer_to_treasury` (strict double-entry) and add to `JusticeSystemState.justice_demand`.

#### Phase 18A: Shadow employment raids

The same function also conducts labor raids using `sanepid_capacity + building_inspection_capacity` as a proxy labor inspection capacity. It detects `Company.shadow_employment` (off-the-books workers), fines triple PIT evaded, and deports illegals per `DeportationPolicy`.

#### Critical findings

- **No `LaborInspectionCapacity` commodity.** Labor inspection piggybacks on sanitary + building capacity. There is no dedicated PIP (Państwowa Inspekcja Pracy) building type or commodity.
- **No fleet/vehicle constraint.** Inspectorate capacity is a scalar sum from `last_production`. There is **no link to `FixedAssetCohort` of Cars/Trucks**. An inspectorate building with zero vehicles can still produce full capacity. The Phase 19 `FixedAssetCohort` system (which tracks `Commodity::Cars`, `Commodity::Trucks` cohorts on `Building.fixed_assets`) is not consulted by `process_inspectorates_turn`.
- **No geographic range.** Capacity is a national-pool scalar. An inspectorate in region A can inspect a building in region B with no distance penalty. There is no per-region capacity allocation.
- **No bribery mechanic.** When a violation is detected, the fine is levied deterministically (clamped to available cash). The contractor cannot offer a bribe; the inspector cannot be corrupt.
- **No construction-site inspection.** The Building Inspectorate inspects **operational** buildings (`condition < 0.3`). It does **not** inspect active `ConstructionProject`s for material fraud or OHS violations during construction.
- **`InspectorateState`** (`politics/laws.rs`) stores: `sanepid_capacity`, `building_inspectorate_capacity`, `environmental_inspection_capacity`, `violations_detected`, `fines_issued`, `recent_violations: Vec<Violation>`. No `labor_inspection_capacity`, no `corruption_index`, no `fleet_range`.

### 1.3 Justice System — Current State

#### Court capacity and coverage

`economy/justice_system.rs` (`process_justice_turn`):
- Sums `Commodity::JusticeCapacity` and `Commodity::SecurityCapacity` from `building_inventories`.
- Calculates dynamic demand from demographics (poverty, unemployment, unrest, health).
- Coverage ratio = `min(1.0, capacity / demand)`.
- Low coverage **freezes** company cash: `freeze_amount = available_cash * (1 - coverage) * 0.15 * court_wait_multiplier`. Frozen cash is tracked in `JusticeSystemState.frozen_company_cash: HashMap<String, f64>`.
- `levy_fines` collects fines with ideological scaling (pro-business flat cap, pro-worker percentage, ambiguous hybrid). Strict double-entry via direct `country.budget.liquid_reserves += actual_fine`.

#### Sentencing (Phase 18B)

`economy/sentencing.rs`:
- `CrimeCategory` (Misdemeanor / Felony / Capital) determined by coverage gap + radical fraction.
- `generate_sentence` produces `SentenceOutcome` (Imprisonment / LifeImprisonment / DeathPenalty / CommunityService / Acquittal) with legal-dualism multipliers for minorities.
- `PrisonerCohort` stored in `JusticeSystemState.prisoner_cohorts`.
- `process_ombudsman_turn` detects rights violations and generates unrest.
- `check_vigilante_justice` triggers summary executions in low-capacity regions.
- `can_execute_state_action` blocks illegal state actions via `AdministrativeCourtState`.

#### Critical findings — hooks for civil lawsuits

- **No civil lawsuit entity.** The justice system is entirely **criminal**: it processes crime demand → fines → imprisonment. There is no `CivilLawsuit` struct, no plaintiff/defendant pairing, no damages award.
- **Asset freezing exists but is coverage-driven, not case-driven.** `frozen_company_cash` is populated by the coverage-gap formula, not by a specific lawsuit. A civil-lawsuit freeze would need a separate field or a tagged entry in `frozen_company_cash` (e.g. key = `"lawsuit:{case_id}:{defendant_id}"`).
- **Fines go to Treasury only.** `levy_fines` and `settle_transfer_to_treasury` always credit `country.budget.liquid_reserves`. There is no mechanism for a fine/damages award to be paid to a **private plaintiff** (the Investor). A new `TransferRecipient::OtherCompany` route via `settle_company_to_company` (which already exists at line 358 of `transfer_settler.rs`) would be needed.
- **No reputation system.** `Company` has no `reputation` field. There is no blacklist, no KIO (National Appeal Chamber), no tender exclusion. A company fined for fraud faces only a cash hit — it can bid on the next tender immediately.
- **`SentenceOutcome` is person-oriented.** It applies to `PrisonerCohort` (individuals), not companies. Corporate sanctions (asset freeze, fine, blacklist) would need a parallel `CorporateSanction` enum.

### 1.4 Disasters — Current State

#### BuildingCollapse trigger today

`economy/disasters.rs` (`check_disaster_triggers`):
- `BuildingCollapse` is triggered when `building.condition < 0.15`. The collapse chance = `(0.15 - condition) * 0.1` per turn. On collapse: severity 0.9, `buildings_destroyed = 1`, casualties = `employment * 0.2`, economic damage = `reserve * 0.9`.
- `IndustrialFire` is triggered when `condition < 0.4` with chance `(0.4 - condition) * 0.05`, mitigated by `FireProtectionCapacity`.
- `DisasterType::Earthquake` exists but is **never triggered** (noted in Phase 21 audit).

#### Critical findings — defect-driven collapse

- **Collapse is purely condition-driven.** `Building.condition` degrades via `economy/maintenance.rs` (`process_condition_degradation`). There is no `structural_defect` field. A building with `condition = 1.0` but catastrophic hidden defects would never collapse.
- **No construction-phase disaster.** Disasters only affect **completed** buildings. A `ConstructionProject` mid-build cannot collapse, catch fire from OHS negligence, or suffer a partial failure.
- **No liability attribution.** When a building collapses, the disaster event records `region_id`, `severity`, `casualties`, `economic_damage`. It does **not** record `contractor_id` or `investor_id`. There is no trail to trigger a lawsuit.
- **Injection point**: `check_disaster_triggers` is the single function. A new block checking `Building.structural_defect` (new field) would be added alongside the existing `condition < 0.15` check, with a separate collapse chance formula.

---

## PART 2: TECHNICAL BLUEPRINT & PHASING STRATEGY

### Architecture Overview

Phase 22 transforms construction from a self-build material-accumulation mechanic into a multi-actor B2B market with fraud, oversight, and legal liability. The core architectural principle is **role separation**:

| Role | Entity | Responsibility |
|---|---|---|
| **Investor** | State or any `Company` | Publishes tender, funds tranches, owns the finished building |
| **Main Contractor** | `Sector::Construction` company | Wins tender, may subcontract, is liable for defects |
| **Subcontractor** | `Sector::Construction` company | Performs a scoped task for a tranche payment |
| **Inspectorate** | State buildings (Sanepid/Building/PIP) | Audits sites for defects & OHS, limited by fleet range |
| **Private Inspector** | Hired by Investor | Pre-acceptance audit, paid from Investor cash |
| **Court** | `JusticeSystemState` | Processes civil lawsuits, freezes assets, awards damages |

All cash flows route through `TransferSettler` (`settle_transfer`, `settle_transfer_to_treasury`, `settle_company_to_company`) to keep bank balance sheets synchronized. No direct `available_cash += / -=` mutations outside the settler.

---

### Phase 22A: The B2B Tender Market & Consortia

#### 22A.1: New data structures

**New module: `state/src/construction/tenders.rs`**

```rust
/// A published construction tender (Investor seeking a contractor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConstructionTender {
    pub id: String,
    pub investor_id: String,           // Company ID or "STATE:{region_id}"
    pub investor_type: TenderInvestorType,  // State | Corporation
    pub project_type: ConstructionProjectType,
    pub micro_region_id: String,
    pub target_building_type: String,
    pub required_materials: BTreeMap<Commodity, f64>,  // BOM (from get_construction_bom)
    pub target_capacity_increase: u32,
    pub target_capital_increase: f64,
    pub estimated_cost: f64,           // Investor's budget ceiling
    pub deadline_turns: u32,           // Bidding window
    pub published_turn: u32,
    pub status: TenderStatus,          // Open | Awarded | Cancelled | Distressed
    pub bids: Vec<Bid>,
    pub awarded_bid: Option<String>,   // Bid ID
}

pub enum TenderInvestorType { State, Corporation }
pub enum TenderStatus { Open, Awarded, Cancelled, Distressed }

/// A contractor's bid on a tender.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Bid {
    pub id: String,
    pub tender_id: String,
    pub bidder_id: String,             // Main contractor company ID
    pub bid_cost: f64,                 // Contractor's cost estimate
    pub bid_margin: f64,               // Profit margin (0.0–1.0 of cost)
    pub bid_price: f64,                // cost + margin (what investor pays)
    pub is_consortium: bool,
    pub consortium_members: Vec<String>, // Subcontractor company IDs (empty if solo)
    pub submitted_turn: u32,
    pub reputation_score: f64,         // Snapshot of bidder reputation at bid time
}
```

**Extend `ConstructionProject`** (`construction/projects.rs`) with contractor linkage:

```rust
    pub investor_id: String,           // Who funds (may differ from old owner_id)
    pub main_contractor_id: String,    // Who builds
    pub subcontractors: Vec<SubcontractorAssignment>,
    pub tranches: Vec<Tranche>,
    pub paid_tranches: u32,
    pub contract_price: f64,           // Total price investor pays contractor
    pub contractor_margin: f64,        // Margin retained by contractor
    pub structural_defect: f64,        // 0.0–1.0, accumulated by fraud (Phase 22B)
    pub ohs_coverage_ratio: f64,       // 0.0–1.0, B2B Health/Education coverage (1.0 = full safety)
```

```rust
/// A scoped task assigned to a subcontractor by the main contractor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SubcontractorAssignment {
    pub subcontractor_id: String,
    pub task_materials: BTreeMap<Commodity, f64>,  // Subset of BOM
    pub tranche_payment: f64,          // What subcontractor receives on completion
    pub completed: bool,
    pub paid: bool,
}

/// A milestone payment tranche.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Tranche {
    pub tranche_id: String,
    pub trigger_progress: f64,         // Release when project progress >= this
    pub amount: f64,                   // Cash paid to main contractor
    pub released: bool,
    pub released_turn: u32,
}
```

#### 22A.2: Tender publication & bidding

**New module: `state/src/construction/tender_market.rs`**

- `publish_tender(investor_id, project_spec, ...) -> ConstructionTender` — Investor creates a tender. For State investors, the tender is funded from `country.budget.liquid_reserves` (encumbered, not spent). For Corporate investors, cash is encumbered via `company.available_cash -= estimated_cost; company.debit_cash += estimated_cost`.
- `submit_bid(contractor, tender, cost, margin) -> Bid` — A `Sector::Construction` company submits a bid. The bidder's `reputation_score` is snapshotted. Bids below a floor (e.g. `cost < 0.5 * estimated_cost`) are rejected as "dumping".
- `award_tender(tender, bid_id)` — The Investor selects the winning bid. Selection criteria: lowest `bid_price` among bidders with `reputation_score > blacklist_threshold`. On award:
  1. A `ConstructionProject` is created on the target building with `main_contractor_id = bidder_id`, `investor_id`, `contract_price = bid_price`, `tranches` derived from `bid_price` split into N milestones (e.g. 20% / 30% / 30% / 20% at progress 0.0 / 0.33 / 0.66 / 1.0).
  2. Unencumbered cash from losing bids is refunded.
  3. The tender `status` becomes `Awarded`.

**Integration**: A new turn phase `PHASE 22A: TENDER MARKET` runs before `submit_construction_b2b_orders`. It processes: (a) new tender publication by the Investor AI / State, (b) bid submission by Construction companies, (c) award resolution for tenders past their deadline.

#### 22A.3: Milestone payments (tranches)

**Modify `advance_construction_projects`** (`construction/orders.rs`):

After updating `project.progress`, check each `Tranche`:
```
if !tranche.released && project.progress >= tranche.trigger_progress:
    settle_company_to_company(investor, main_contractor, tranche.amount)
    tranche.released = true
```
- For **State investor**: `settle_transfer_to_treasury` is reversed — Treasury pays the contractor via a new `settle_transfer_from_treasury` (or `settle_transfer` with `TransferRecipient::OtherCompany` and payer = Treasury's holding company). The Treasury's `liquid_reserves` is debited; the contractor's cash is credited; bank balance sheets sync.
- For **Corporate investor**: `settle_company_to_company(investor_idx, contractor_idx, amount)`.
- The main contractor then pays subcontractors their `tranche_payment` for completed tasks via `settle_company_to_company`.

#### 22A.4: Consortia & subcontracting

When a bid has `is_consortium = true`, the main contractor distributes `SubcontractorAssignment`s. Each subcontractor is responsible for a subset of the BOM. The main contractor buys the remaining materials directly. Subcontractor tranche payments are released when their assigned `task_materials` subset reaches 100% delivery.

**Subcontractor B2B orders**: Subcontractors submit their own B2B buy bids for their assigned materials (extending `submit_construction_b2b_orders` to iterate `project.subcontractors` and submit bids under each subcontractor's `company.id`).

#### 22A.5: Domino bankruptcies

**Investor bankruptcy mid-construction**:
- The `ConstructionProject` is detached from the building and becomes a `DistressedAsset` entry in the existing `BankruptcyAuctionPool` (`corporate/bankruptcy.rs`). The asset's `book_value` = `project.cost_spent` (materials already consumed). Creditor claims = the main contractor's unpaid tranches.
- The commercial bank (or Treasury for State projects) auctions the half-built site. A buyer inherits the project with a new `investor_id`.
- The main contractor's unpaid tranches become unsecured claims in the bankruptcy waterfall.

**Main Contractor bankruptcy mid-construction**:
- The contractor defaults on all subcontractors. Each subcontractor's `tranche_payment` for incomplete tasks becomes an unsecured claim against the contractor in the bankruptcy waterfall.
- The Investor may: (a) hire a replacement contractor (new tender with `status = Distressed` → re-awarded), or (b) cancel the project and reclaim encumbered funds.
- Subcontractor domino: if a subcontractor was relying on the tranche payment to pay its own suppliers, those B2B obligations remain on the `OrderBook` and may trigger a cascading default. This is emergent — no special cascade code needed; the existing bankruptcy + OrderBook refund logic handles it.

**CRITICAL — Defect retention on distressed assets**: When a project becomes a `DistressedAsset` (either due to Investor or Main Contractor bankruptcy), it **permanently retains its accumulated `structural_defect`** and any committed `MaterialSubstitution` fraud. The half-built skeleton carries the hidden flaws of the failed contractor. When auctioned to a new Investor:
- The `ConstructionProject.structural_defect` field is preserved verbatim on the auctioned asset.
- The new Investor inherits the defects with no visibility — the `structural_defect` is a **hidden** field (not exposed in the auction listing).
- The new Investor **must** hire a `PrivateInspector` (Phase 22D) immediately after purchase to evaluate the skeleton's true defect level before deciding whether to continue, demolish, or remediate.
- Remediation (if chosen) requires a new tranche of materials to replace the substituted commodities, reducing `structural_defect` proportionally.
- If the new Investor continues without inspection and the building later collapses, the new Investor bears the liability (they had the opportunity to inspect and chose not to — contributory negligence).

This closes the corporate loophole where bankruptcy could be used to "launder" defective construction.

**Integration**: Hook into `corporate/bankruptcy.rs` `execute_liquidation`. When liquidating a company, scan all `ConstructionProject`s where `main_contractor_id == company.id` or `investor_id == company.id` and apply the above rules. The `DistressedAsset` entry in `BankruptcyAuctionPool` must serialize the full `ConstructionProject` (including `structural_defect`) so it survives the auction transfer.

---

### Phase 22B: Cutting Corners, Defects & OHS (BHP)

#### 22B.1: Material fraud (BOM substitution)

**New module: `state/src/construction/fraud.rs`**

When the main contractor (or a subcontractor) submits B2B buy bids for a `ConstructionProject`'s required materials, they may **fraudulently substitute** an expensive BOM material with a cheaper, lower-quality one. This is a bounded-rationality decision: the contractor spends less on the B2B market and naturally retains the difference in their `available_cash`.

```rust
/// A material substitution fraud event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MaterialSubstitution {
    pub original_commodity: Commodity,    // e.g. Steel
    pub substitute_commodity: Commodity,  // e.g. Timber (cheaper, weaker)
    pub quantity_substituted: f64,
    pub cash_retained: f64,               // (orig_price - subst_price) * qty, naturally kept in available_cash
    pub defect_added: f64,                // structural_defect points added
}
```

**Fraud decision logic** (`try_material_fraud`):
- Triggered when the contractor submits B2B buy bids for project materials (integrated into `submit_construction_b2b_orders`).
- Probability scales with: (a) contractor's `reputation_score` (low reputation → more likely to cheat), (b) `JusticeSystemState.justice_coverage` (low coverage → impunity), (c) `InspectorateState` fleet range coverage of the region (Phase 22C).
- If fraud occurs: the contractor submits a B2B buy bid for the **substitute** commodity (e.g. Timber) instead of the **original** commodity (e.g. Steel). The cash naturally remains in the contractor's `available_cash` because the substitute is cheaper — no synthetic transfer is needed. This mirrors the OHS fraud logic (22B.2): the economic benefit of fraud is lower cash outflow, not a synthetic pocketing transfer. `project.structural_defect += defect_added`.
- The substitution is **hidden**: the project's `delivered_materials` records the **original** commodity as "delivered" (the fraud is not visible without inspection — the books show Steel was delivered, but Timber was actually consumed).

**Defect formula**:
```
defect_added = quantity_substituted * (1.0 - substitute_quality / original_quality)
```
where quality is derived from `Commodity` tier (e.g. Steel = 1.0, Timber = 0.4, Scrap = 0.2). A full substitution of 500t Steel with Timber adds ~0.6 * 500 / total_bom_mass defect points, normalized to `[0.0, 1.0]` on the project.

#### 22B.2: OHS (BHP) as a physical B2B service

OHS is **not** an abstract cash budget. Construction site safety is a physical service that the Main Contractor must procure from the B2B market. This links the construction sector's safety performance to the **medical** and **educational** sectors of the economy.

**Required B2B inputs for a safe construction site**:
- `Commodity::HealthCapacity` — on-site medical coverage (first aid, emergency response). Produced by medical-sector buildings (hospitals, clinics).
- `Commodity::EducationSlots` — safety training slots for construction workers. Produced by educational-sector buildings.

**Extend `ConstructionProject`** with:
```rust
    pub ohs_health_required: f64,      // HealthCapacity units needed per turn
    pub ohs_education_required: f64,   // EducationSlots needed per turn (one-time per worker cohort)
    pub ohs_health_delivered: f64,     // HealthCapacity actually procured this turn
    pub ohs_education_delivered: f64,  // EducationSlots actually procured
    pub ohs_coverage_ratio: f64,       // min(health_delivered/health_required, edu_delivered/edu_required)
    pub ohs_accidents: u32,            // Accumulated accident count
```

**OHS procurement** (integrated into `submit_construction_b2b_orders`):
- Each turn, for every active `ConstructionProject`, the Main Contractor submits B2B buy bids for `HealthCapacity` and `EducationSlots` in addition to the BOM materials.
- The required quantities scale with site employment: `ohs_health_required = site_fte * health_per_fte`, `ohs_education_required = new_workers_this_turn * edu_per_worker`.
- `ohs_coverage_ratio = min(ohs_health_delivered / ohs_health_required, ohs_education_delivered / ohs_education_required)`, clamped to `[0.0, 1.0]`.

**OHS fraud (cutting corners)** (`try_ohs_cut`):
- "Cutting corners on OHS" means the contractor **actively refuses to submit** the `HealthCapacity` and `EducationSlots` buy bids (or submits deliberately insufficient quantities). This is a bounded-rationality decision: the contractor saves the cash that would have been spent on medical/training services.
- The decision to cut OHS follows the same probability model as material fraud (22B.1): scales with low `reputation_score`, low `justice_coverage`, and low in-range PIP inspection probability (Phase 22C).
- The unspent OHS cash is **not** pocketed directly — it is simply never encumbered. The contractor's `available_cash` remains higher because no B2B bids were submitted. This is the economic benefit of cutting corners: lower cash outflow.

**Accident probability** per turn:
```
accident_chance = base_accident_rate * (1.0 - ohs_coverage_ratio) * (1.0 + project_progress)
```
- `base_accident_rate` = e.g. 0.02 (2% base chance per turn at zero coverage).
- As construction progresses (`project_progress` → 1.0) and OHS coverage drops, accidents compound. A site with full coverage (`ohs_coverage_ratio = 1.0`) has zero accident chance.

**Accident consequences** (reusing Phase 18 labor physics):
- A random number of FTEs on the construction site (drawn from the contractor's `current_employment` at the site) are shifted from healthy to disabled/dead.
- `LaborMarket.active_disabled += disabled_count` and `LaborMarket.unable_to_work += dead_count` (in the contractor's home region).
- **Compensation payout**: the contractor must pay `compensation_per_casualty` to the affected workers' `ClassDemographics.savings` via `settle_transfer` with `TransferRecipient::CitizenSavings`. If the contractor cannot pay, the unpaid compensation becomes a liability that triggers a lawsuit (Phase 22D) or state enforcement.
- `Building.accidents_last_year` is incremented on the construction site building.

**Sectoral linkage**: This design means that a country with an underdeveloped medical sector (low `HealthCapacity` production) or underdeveloped education sector (low `EducationSlots`) will inherently have higher construction accident rates — even if contractors want to buy OHS services, the B2B market may not supply them. This is emergent systemic risk, not a hardcoded penalty.

#### 22B.3: Consequences of defects

**On completion** (`advance_construction_projects` when `is_complete()`):
- The finished building inherits `structural_defect = project.structural_defect` (new `Building.structural_defect: f64` field, default 0.0).
- **Passive penalty**: `structural_defect > 0.3` reduces residential satisfaction (for `Residential`/`SocialHousing`) or business efficiency (for `Commercial`/`Factory`). The efficiency multiplier = `1.0 - 0.3 * structural_defect` (clamped to `[0.5, 1.0]`). This is applied in the production cycle as a multiplier on `production_scale`.
- **Collapse risk**: In `check_disaster_triggers` (`disasters.rs`), add a new block:
  ```
  if building.structural_defect > 0.5:
      collapse_chance = (structural_defect - 0.5) * 0.15
      if rng_roll < collapse_chance:
          trigger DisasterType::BuildingCollapse
          record contractor_id & investor_id in DisasterEvent.extra
          (for lawsuit attribution in Phase 22D)
  ```
  This is **separate** from the existing `condition < 0.15` collapse — a building can be in perfect condition (`condition = 1.0`) but collapse due to hidden defects.

**New `Building` field**:
```rust
    #[serde(default, skip_serializing_if = "is_zero")]
    pub structural_defect: f64,         // 0.0 = sound, 1.0 = catastrophic
```

---

### Phase 22C: Fleet-Based Inspectorates, PIP & Corruption

#### 22C.1: New Labor Inspectorate (PIP) commodity & capacity

**Add to `Commodity` enum** (`registries/enums.rs`):
```rust
    /// "labor_inspection_capacity" — PIP (Państwowa Inspekcja Pracy) capacity.
    LaborInspectionCapacity,
```
Add to `all()`, `is_active()`, and `try_from` mapping. Seed a PIP building type in the generator (or extend existing inspectorate buildings to also produce `LaborInspectionCapacity`).

**Extend `InspectorateState`** (`politics/laws.rs`):
```rust
    pub labor_inspection_capacity: f64,
    pub pip_fleet_range_km: f64,        // Derived from vehicle cohorts
    pub corruption_index: f64,          // 0.0–1.0, fraction of bribes accepted
    pub bribes_accepted_this_turn: u32,
    pub bribes_total_value: f64,
```

#### 22C.2: Fleet-based operational range

**New module: `state/src/economy/inspectorate_fleet.rs`**

Inspectorate capacity is no longer a free national-pool scalar. It is constrained by the **vehicles** (`FixedAssetCohort` of `Commodity::Cars` / `Commodity::Trucks`) installed at the inspectorate building and the **employment** at that building.

```rust
/// Compute the effective inspection range (in km) for an inspectorate building.
pub fn inspectorate_fleet_range(building: &Building) -> f64 {
    let vehicles: f64 = building.fixed_assets.iter()
        .filter(|c| c.commodity == Commodity::Cars || c.commodity == Commodity::Trucks)
        .map(|c| c.count * c.condition)  // broken-down vehicles don't count
        .sum();
    let staff = building.current_employment as f64;
    // Range scales with min(vehicles, staff) — you need both drivers and cars.
    let effective_units = vehicles.min(staff);
    effective_units * KM_PER_VEHICLE_UNIT  // e.g. 50.0 km per vehicle-staff unit
}

/// Check if a target building is within range of any inspectorate of the given type.
pub fn is_within_inspection_range(
    inspectorate_buildings: &[&Building],
    target_building: &Building,
    region_distances: &RegionDistanceMatrix,
) -> bool
```

**Geographic distance**: A `RegionDistanceMatrix` (or a simple great-circle / adjacency lookup on `Region`) provides the distance between the inspectorate's region and the target's region. If `distance > fleet_range`, the inspectorate **cannot** inspect that target this turn.

**Modify `process_inspectorates_turn`** (`inspectorates.rs`):
- For each inspectorate building, compute `fleet_range`.
- For each potential target, check `is_within_inspection_range`. Out-of-range targets are skipped.
- The coverage ratio becomes per-inspectorate: `capacity_of_in_range_inspectorates / in_range_target_count`.
- **Employment gate**: if `building.current_employment == 0`, the inspectorate produces zero capacity regardless of `last_production` (no staff = no inspections).

#### 22C.3: Construction-site inspections

**New function**: `process_construction_inspections(country, companies, buildings, turn)`

- Iterates all active `ConstructionProject`s.
- For each project, checks if any in-range inspectorate (Building Inspectorate for material fraud, PIP for OHS) can audit it this turn.
- **Audit probability** = `in_range_inspection_capacity / active_project_count` (clamped to `[0, 1]`).
- If audited:
  - **Material fraud detection**: if `project.structural_defect > 0` and the audit rolls a detection success (probability scales with `defect_severity`), the fraud is **discovered**. The contractor is fined (via `settle_transfer_to_treasury`) and the defect is recorded for lawsuit attribution (Phase 22D).
  - **OHS violation detection**: if `project.ohs_coverage_ratio < 1.0` and the audit detects it, the contractor is fined and forced to submit the missing `HealthCapacity` / `EducationSlots` B2B buy bids (restoring safety procurement).
- **Shadow employment on sites**: PIP can also raid construction sites for `Company.shadow_employment` (reusing the existing Phase 18A raid logic, but scoped to construction sites).

#### 22C.4: Bribery

**New module: `state/src/economy/bribery.rs`**

When an inspector detects a violation (fraud or OHS), before the fine is levied, the contractor can **offer a bribe**:

```rust
/// A bribery attempt during an inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BribeAttempt {
    pub inspector_building_id: String,
    pub contractor_id: String,
    pub bribe_amount: f64,             // Offered from contractor's available_cash
    pub accepted: bool,
    pub turn: u32,
}
```

**Bribery decision** (`try_bribe`):
- The contractor offers a bribe proportional to the fine they would face: `bribe_amount = fine * bribe_ratio` (e.g. 0.3–0.5 of the fine). The bribe is drawn from the contractor's `available_cash` (shadow economy — no invoice, no tax).
- **Acceptance probability** = `InspectorateState.corruption_index` (a country-level political parameter, influenced by `JusticeLaw.corruption_index` which already exists). If `corruption_index = 0.0`, bribes are never accepted. If `corruption_index = 1.0`, bribes are always accepted.
- **If accepted**:
  - The bribe enriches the **corrupt official personally**, not the state building. The bribe cash flows via `settle_transfer` with `TransferRecipient::CitizenSavings`, targeting the specific `ClassDemographics` of the inspectorate's staff (typically the **Bourgeoisie / Public Servants** urban class in the inspectorate's home region). The state building's `reserve` is **never touched**.
  - **Double-entry flow**: `contractor.available_cash -= bribe_amount` (contractor's bank deposits/reserves debited via `settle_transfer`) → `region.class_demographics.urban_classes[public_servant_class].savings += bribe_amount` (citizen savings credited via `TransferRecipient::CitizenSavings`). The bank balance sheet is synchronized: the contractor's bank loses deposits and reserves; the citizen's bank (if same bank) gains them, or an inter-bank reserve transfer occurs. No phantom money — the bribe is a real transfer from contractor cash to official personal wealth.
  - The violation is **hidden**: no fine, no `justice_demand` increase, no lawsuit trigger. The `structural_defect` remains on the building.
  - `InspectorateState.bribes_accepted_this_turn += 1`, `bribes_total_value += bribe_amount`.
  - The enriched class's `savings_per_capita` increases, creating a visible wealth signal in the inspector's demographic cohort — a potential audit trail for the Ombudsman (Phase 18B) or future anti-corruption mechanics.
- **If rejected**:
  - The bribe attempt itself becomes an **additional crime** (bribery is a felony). The fine is doubled, `justice_demand` increases by 2.0, and the contractor's `reputation_score` drops (Phase 22D).

**Corruption index dynamics**: `InspectorateState.corruption_index` is not static. It drifts upward when bribes are accepted (entrenchment) and downward when justice coverage is high and ombudsman is active. This creates a feedback loop: a corrupt inspectorate becomes more corrupt over time unless oversight intervenes.

---

### Phase 22D: Private Oversight, Lawsuits & Reputation

#### 22D.1: Private inspectors

**New module: `state/src/construction/private_inspection.rs`**

Because state inspectorates may be out-of-range, understaffed, or corrupt, the Investor can hire a `PrivateInspector`:

```rust
/// A private inspection engagement hired by the investor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PrivateInspection {
    pub id: String,
    pub tender_id: String,
    pub project_id: String,
    pub investor_id: String,
    pub fee: f64,                      // Paid from investor cash
    pub hired_turn: u32,
    pub report: Option<InspectionReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InspectionReport {
    pub defects_found: f64,            // structural_defect measured
    pub ohs_violations_found: bool,
    pub fraud_detected: Vec<MaterialSubstitution>,
    pub inspected_turn: u32,
}
```

**Logic** (`hire_private_inspector` / `conduct_private_inspection`):
- The Investor pays `fee` from `available_cash` via `settle_transfer` (recipient = a private inspection company, or if none exists, a `ForeignEntity` outflow simulating consulting fees).
- The private inspector **always detects** the true `structural_defect` and `ohs_coverage_ratio` (no corruption, no range limit — the investor pays for thoroughness).
- The `InspectionReport` is attached to the project. If defects are found, the Investor can file a lawsuit (22D.2).

#### 22D.2: Civil lawsuits via the justice system

**New module: `state/src/economy/civil_lawsuits.rs`**

```rust
/// A civil lawsuit (Investor suing Contractor, or State suing Contractor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CivilLawsuit {
    pub id: String,
    pub plaintiff_id: String,          // Investor company ID or "STATE"
    pub defendant_id: String,          // Main contractor company ID
    pub case_type: CivilCaseType,
    pub damages_claimed: f64,
    pub evidence: LawsuitEvidence,     // InspectionReport or DisasterEvent
    pub filed_turn: u32,
    pub status: LawsuitStatus,         // Pending | Won | Lost | Settled
    pub resolution_turn: u32,
    pub damages_awarded: f64,
}

pub enum CivilCaseType {
    StructuralDefect,      // From private inspection report
    BuildingCollapse,      // From DisasterEvent attribution
    OhsNegligence,         // From accident casualties
    Bribery,               // From rejected bribe (state prosecutor)
}

pub enum LawsuitStatus { Pending, Won, Lost, Settled }
```

**Lawsuit processing** (`process_civil_lawsuits`):
- Runs as a new sub-phase after `process_justice_turn` (so `justice_coverage` is current).
- **Filing**: triggered by (a) a private `InspectionReport` with `defects_found > threshold`, (b) a `DisasterEvent::BuildingCollapse` with contractor attribution in `extra`, or (c) an OHS accident with casualties.
- **Asset freeze**: on filing, the defendant's cash is frozen via a tagged entry in `JusticeSystemState.frozen_company_cash` with key `"lawsuit:{case_id}:{defendant_id}"`. The existing freeze mechanism (coverage-gap-driven) is untouched; this is an **additional** case-driven freeze.
- **Resolution probability** per turn = `justice_coverage * evidence_strength`. Strong evidence (private inspection report) → high `evidence_strength`. Weak evidence (rumor) → low.
- **On resolution (plaintiff wins)**:
  1. **Damages awarded** = `damages_claimed * evidence_strength * (1.0 + penalty_multiplier)`. The penalty multiplier scales with `structural_defect` severity (catastrophic defects → 3x damages).
  2. **Cash flow**: `settle_company_to_company(defendant, plaintiff, damages_awarded)` — the contractor pays the investor. If the defendant is the State prosecutor's target, `settle_transfer_to_treasury`.
  3. **Reputation penalty**: `defendant.reputation_score -= reputation_hit` (see 22D.3).
  4. **Asset unfreeze**: the tagged freeze entry is removed; remaining frozen cash returns to the defendant.
- **On resolution (defendant wins)**: asset unfreeze, no damages. The plaintiff may face a countersuit for defamation (optional, emergent).

#### 22D.3: Reputation system

**New `Company` field**:
```rust
    #[serde(default = "default_reputation")]
    pub reputation_score: f64,         // 0.0 (ruined) – 100.0 (exemplary)
```
`default_reputation() = 50.0` (neutral start).

**Reputation dynamics**:
- **Decrease**: lawsuit loss (`-10 * severity`), rejected bribe detected (`-15`), bankruptcy (`-20`), OHS accident with casualties (`-5 per casualty`).
- **Increase**: successful project completion with zero defects (`+2`), winning a tender cleanly (`+1`), long streak without violations (`+0.5/turn`).
- Clamped to `[0.0, 100.0]`.

**Tender blacklist**: In `award_tender`, bids from companies with `reputation_score < blacklist_threshold` (e.g. 20.0) are **excluded**. This is the KIO mechanism.

#### 22D.4: KIO (National Appeal Chamber)

**New module: `state/src/government/kio.rs`**

```rust
/// KIO appeal — a competitor reports a blacklisted/rule-breaking winner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KioAppeal {
    pub id: String,
    pub tender_id: String,
    pub appellant_id: String,          // Competitor company ID
    pub respondent_id: String,         // Awarded contractor
    pub grounds: KioGrounds,           // Blacklisted | FraudHistory | BriberyRecord
    pub filed_turn: u32,
    pub upheld: bool,
}

pub enum KioGrounds { Blacklisted, FraudHistory, BriberyRecord }
```

**Logic** (`process_kio_appeals`):
- A competitor (any `Sector::Construction` company that bid on the same tender) can file a KIO appeal if the awarded contractor has `reputation_score < blacklist_threshold` or a recent lawsuit loss.
- **Uphold probability** = `justice_coverage * evidence_strength`. If upheld:
  1. The tender is **re-awarded** to the next-best bid (excluding blacklisted bidders).
  2. The respondent's `reputation_score -= 10`.
  3. The appellant pays a filing fee (small, via `settle_transfer_to_treasury`) which is refunded if the appeal is upheld.
- KIO appeals run in the `PHASE 22D: PRIVATE OVERSIGHT & LAWSUITS` turn block, after civil lawsuits.

---

## Cross-Cutting Concerns

### Double-Entry Accounting Summary

| Transaction | Payer | Recipient | Settler Function |
|---|---|---|---|
| Tranche payment (Corporate investor) | Investor company | Main contractor | `settle_company_to_company` |
| Tranche payment (State investor) | Treasury | Main contractor | `settle_transfer` (Treasury → OtherCompany) |
| Subcontractor payment | Main contractor | Subcontractor | `settle_company_to_company` |
| Material fraud (substitution) | Contractor buys cheaper substitute on B2B | Cash naturally retained in contractor `available_cash` | No transfer — lower B2B outflow |
| OHS compensation | Contractor | Worker class savings | `settle_transfer` (CitizenSavings) |
| Inspectorate fine | Contractor | Treasury | `settle_transfer_to_treasury` |
| Bribe (accepted) | Contractor | Inspector's demographic class savings | `settle_transfer` (CitizenSavings) |
| OHS B2B procurement | Contractor | Medical/Education sector | `settle_company_to_company` (B2B settlement) |
| Private inspector fee | Investor | Private inspector / ForeignEntity | `settle_transfer` |
| Lawsuit damages | Defendant contractor | Plaintiff investor | `settle_company_to_company` |
| KIO filing fee | Appellant | Treasury | `settle_transfer_to_treasury` |

**Rule**: No `company.available_cash +=` or `country.budget.liquid_reserves +=` outside `TransferSettler`. All mutations route through the settler to keep bank `deposits` and `reserves_at_central_bank` synchronized.

### New `Commodity` Variants

- `LaborInspectionCapacity` — PIP building output.

### New `Building` Fields

- `structural_defect: f64` (default 0.0) — hidden defect accumulation.

### New `Company` Fields

- `reputation_score: f64` (default 50.0) — tender eligibility & lawsuit penalty driver.

### New `ConstructionProject` Fields

- `investor_id`, `main_contractor_id`, `subcontractors`, `tranches`, `paid_tranches`, `contract_price`, `contractor_margin`, `structural_defect`, `ohs_health_required`, `ohs_education_required`, `ohs_health_delivered`, `ohs_education_delivered`, `ohs_coverage_ratio`, `ohs_accidents`.

### New Modules

| Module | Purpose |
|---|---|
| `construction/tenders.rs` | Tender & Bid structs |
| `construction/tender_market.rs` | Publication, bidding, award logic |
| `construction/fraud.rs` | Material substitution & OHS fraud |
| `construction/private_inspection.rs` | Private inspector hiring & reports |
| `economy/inspectorate_fleet.rs` | Fleet-based range computation |
| `economy/bribery.rs` | Bribe attempt & acceptance |
| `economy/civil_lawsuits.rs` | Civil lawsuit filing & resolution |
| `government/kio.rs` | KIO appeals & tender re-evaluation |

### Turn Loop Integration (in `engine/turn.rs`)

New phases inserted into the existing turn sequence:

1. **PHASE 22A: TENDER MARKET** (before `submit_construction_b2b_orders` at step 3.8):
   - Publish new tenders (Investor AI / State).
   - Submit bids (Construction companies).
   - Award expired tenders → create `ConstructionProject` with contractor linkage.

2. **PHASE 22B: FRAUD & OHS** (inside `advance_construction_projects` at step 6.4b-PRE):
   - `try_material_fraud` per active project.
   - `try_ohs_cut` per active project.
   - Accident consequences (FTE shifts, compensation).

3. **PHASE 22C: INSPECTIONS & BRIBERY** (after `process_inspectorates_turn` at step 15C):
   - `process_construction_inspections` (fleet-range-limited).
   - `try_bribe` on detected violations.

4. **PHASE 22D: PRIVATE OVERSIGHT & LAWSUITS** (after justice turn):
   - `hire_private_inspector` / `conduct_private_inspection`.
   - `process_civil_lawsuits`.
   - `process_kio_appeals`.
   - Reputation updates.

### Save Migration

All new fields use `#[serde(default = "...")]` so existing saves load without error. New fields default to neutral values (0.0 defect, 50.0 reputation, empty subcontractors/tranches). A migration pass (in `entities/mod.rs` `CompanyDef` / `Building` deserialization) is **not required** for loading, but a post-load normalization pass should ensure `reputation_score` is in `[0, 100]` and `structural_defect` is in `[0, 1]`.

### Risks & Open Questions

1. **State investor funding**: The Treasury paying a contractor requires a `settle_transfer_from_treasury` or a reverse `settle_transfer`. The existing `settle_transfer` takes a `payer_idx` (company index). The Treasury is not a `Company`. Options: (a) create a synthetic Treasury holding company, (b) add a `TransferRecipient` variant `FromTreasury` that debits `country.budget.liquid_reserves` and credits the recipient company with bank sync. **Recommendation**: option (b), a new `settle_treasury_to_company` function in `transfer_settler.rs`.

2. **Region distance matrix**: Fleet range requires inter-region distances. The codebase has `Region` with coordinates? Need to verify. If no distance data exists, a simple adjacency graph (regions sharing a border) with fixed per-hop distance (e.g. 100 km) is a fallback.

3. **Construction company AI for bidding**: Construction companies need AI logic to decide which tenders to bid on and at what margin. This extends `corporate/strategy.rs` with a `ConstructionBidStrategy`. The bounded-rationality `InformationQuality` tier (Phase 18) determines how accurately the contractor estimates `bid_cost` (Blind → ±30% error, Predictive → exact).

4. **Performance**: The tender market adds a new O(tenders × bidders) matching step per turn. With ~100 construction companies and ~10 active tenders, this is negligible. The fleet-range check is O(inspectorates × targets) per region — also small.

5. **Emergent cascade risk**: Domino bankruptcies (investor → contractor → subcontractor) could trigger a systemic construction-sector collapse if multiple large projects fail simultaneously. This is **intended** (realistic) but may need a circuit-breaker (e.g. State bailout tender) if it causes excessive GDP contraction in testing.
