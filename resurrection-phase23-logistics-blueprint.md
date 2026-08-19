# Resurrection Phase 23 — Transport, Logistics, and Infrastructural Networks

**Blueprint & Dependency Audit for spatial friction, freight logistics, infrastructural networks (built via Phase 22 Tenders), corporate/municipal passenger transport, commuting & labor mobility, and technological evolution (Draft Animals -> Aviation).**

---

## PART 1: DEPENDENCY AUDIT

### 1.1 Spatial Friction — Current State

#### How B2B trades handle distance today

The B2B market is a **global, co-located order book** with zero spatial friction. The full pipeline is:

1. **Order submission** (`economy/b2b_orders.rs`, `submit_company_b2b_orders`, lines 168-356): Each company iterates its buildings, computes desired input quantities from the active production method's BOM, and submits `Bid`s into the global `OrderBook`. Sell `Ask`s are submitted for outputs. **No region, distance, or routing information is attached to any bid or ask.**

2. **Matching** (`economy/order_book.rs`, `match_orders`, lines 99-169): A deterministic price-time priority matcher. Bids sorted descending by `limit_price`, asks ascending. Execution price = midpoint of spread. **Matching is purely financial — a buyer in region A matches a seller in region Z with no distance penalty.** The only spatially-aware variant is `match_orders_with_embargoes` (lines 186-292), which filters trades by *country* (cross-border embargoes), proving the pattern for attribute-based trade filtering exists.

3. **Settlement** (`economy/b2b_orders.rs`, `settle_trades`, lines 375-482): Cash moves (`buyer.debit_cash -= value`, `seller.available_cash += value`) and physical inventory routes directly from the seller's building to the buyer's building. **Goods teleport instantaneously across any distance.** There is no delivery delay, no freight cost, no capacity check.

4. **Tariff variant** (`settle_trades_with_tariffs`, lines 484+): Wraps `settle_trades` and adds a second pass collecting tariffs on *cross-border* trades. This is the only existing "friction" — and it is fiscal, not spatial.

#### The `Trade` struct — the core gap

```rust
// economy/order_book.rs lines 55-75
pub struct Trade {
    pub buyer_id: String,
    pub seller_id: String,
    pub commodity: Commodity,
    pub quantity: f64,
    pub execution_price: f64,
    pub bid_limit_price: f64,
    pub blueprint_id: Option<String>,
    pub quality: Option<f64>,
}
```

**`Trade` has NO `source_region`, NO `dest_region`, NO `distance`, NO `freight_cost`.** The same applies to `Bid` (lines 11-29) and `Ask` (lines 32-52). This is the single most important structural gap for Phase 23: every trade is treated as if buyer and seller share a warehouse.

#### Retail (B2C) — same teleportation

`economy/retail.rs` (`clear_b2c_markets`, lines 347-471) clears consumer demand against store offers by utility (price + inertia + quality). **Consumers buy from any store regardless of distance.** A peasant in a remote village buys from a store in the capital with no transport cost. `StoreOffer` (lines 18-46) has no region field beyond the implicit `store_id`.

#### Critical findings — Spatial Friction

- **Trades are instantaneous and free across any distance.** No delivery cost, no delay, no capacity gate.
- **`Trade`, `Bid`, `Ask` carry no spatial metadata.** Region information lives on `Company.region_id` and `Building.region_id` but is never consulted during matching or settlement.
- **The geography graph exists but is unused by the economy.** `Region.edges: Vec<Edge>` (geography.rs line 528) with `Edge { target_node, edge_type, distance, is_navigable }` is a ready-made adjacency/routing graph, but no economic code path reads it.
- **Injection point:** A new "freight procurement & spatial settlement" phase between `match_orders` and `settle_trades` (in `engine/turn.rs`, between the current B2B matching and settlement blocks).

---

### 1.2 Labor Mobility — Current State

#### How hiring works today

`economy/labor_market.rs` (`resolve_regional_labor_market`, lines 134-400) is **strictly intra-regional**:

1. **Company filtering** (lines 155-158): `companies.iter_mut().filter(|c| c.region_id == region.id)` — only companies in the *same* region as the labor pool can hire.
2. **Labor pool** (lines 210-232): Built exclusively from `region.class_demographics.rural_classes` and `urban_classes` — the region's own residents.
3. **Clearing** (lines 236-304): Bids sorted by wage descending; highest-paying companies consume FTE first, distributed across classes by suitability multiplier.
4. **Wage settlement** (lines 314-389): Double-entry via direct `brokerage_account.cash` mutation (gross wage debited from company, net wage credited to class savings, PIT/remittances/garnishments withheld).

#### Critical findings — Labor Mobility

- **Workers are locked to their home region.** A worker in region A cannot accept a job in adjacent region B. There is no commuting. This causes **artificial localized labor shortages** even when adjacent regions have surplus labor.
- **No `PassengerTransport` consumption in hiring.** The labor market never consults transport capacity or ticket prices.
- **No adjacency/edge lookup.** `resolve_regional_labor_market` receives a single `&mut Region` — it has no access to the country's region graph or neighboring regions' labor pools.
- **Migration exists but is cross-country & permanent.** `economy/migration.rs` handles emigration/immigration between *countries* (driven by unrest/poverty, gated by border enforcement). It is NOT daily commuting between adjacent regions of the same country. Migration provides population-movement patterns but is a different system.
- **Injection point:** Either (a) widen `resolve_regional_labor_market` to accept a commuter pool from adjacent regions, or (b) add a pre-phase that builds a "commuter-eligible" labor pool map before the regional market resolves. Option (b) is cleaner and less invasive.

---

### 1.3 Existing Logistics — Current State

#### What `TransportLogistics` companies do today

`Sector::TransportLogistics` ("transport_logistics") is a fully seeded sector:

- **Corporate generation** (`engine/generator/corporate.rs`): Transport companies are seeded as "Transport Depot" buildings (line 806), base worker capacity 80 (line 898), with `Commodity::Trucks` fixed-asset cohorts of count 4.0 (lines 1059, 1067). They are in the `critical_sectors` list (line 846).
- **Production methods** (`registries/production_methods_data.rs`, `transport_methods`, lines 906-935): Seven production methods, ALL producing **only `Commodity::PassengerTransport`**:

| Method | Year | Inputs | Output (PassengerTransport) |
|---|---|---|---|
| Horse-Drawn Wagons | 1880 | Food (5), Fuels (2) | 10 |
| Steam Locomotives | 1885 | Fuels (15), Steel (5) | 30 |
| Electric Trams | 1895 | Energy (10), Steel (3) | 40 |
| Diesel Locomotives | 1930 | Fuels (12), MechanicalComponents (5) | 60 |
| Container Shipping | 1960 | Fuels (15), Steel (10) | 100 |
| High-Speed Rail | 1980 | Energy (20), ElectronicComponents (8), Steel (10) | 180 |
| Logistics Networks | 1990 | Fuels (10), Software (5), ElectronicComponents (5) | 250 |

#### What produces and consumes `Commodity::PassengerTransport`

- **Producers:** `Sector::TransportLogistics` buildings (via the methods above). The output is deposited into `building.inventory` and sold as Sell Asks on the B2B order book.
- **Consumers:** **NONE.** A full search of `state/src` confirms `PassengerTransport` appears only in the enum definition (`enums.rs` lines 389, 697, 844) and in production method outputs. **There is no `clear_passenger_transport_b2c`, no commuting consumption, no tourism consumption.** The commodity is produced and traded but never consumed for its intended purpose. It is a "dead" commodity — produced, sold B2B, and accumulated in inventories with no economic function.

#### Critical findings — Existing Logistics

- **`Commodity::FreightCapacity` does NOT exist.** There is no freight transport commodity. Goods move via B2B settlement with no freight service purchased. This is the core Phase 23A gap.
- **`Commodity::DraftAnimals` does NOT exist.** The earliest transport method ("Horse-Drawn Wagons", 1880) uses `Fuels` as an input — an anachronism. Pre-industrial transport and agriculture have no draft-animal asset class.
- **`PassengerTransport` is a dead commodity.** Produced but never consumed. Phase 23C must create the consumption sink (commuting).
- **Transport methods produce only passenger capacity.** None produce freight capacity. Phase 23A must add freight-producing methods (or split existing methods into passenger + freight outputs).
- **`Commodity::RollingStock` exists** (rail stock) but is not used by transport methods — a hook for Phase 23B electrified rail.
- **`Commodity::Fodder` and `Commodity::Water` already exist** (Phase 6.3.5) — ready as maintenance inputs for DraftAnimals.

---

### 1.4 Geography Constraints — Current State

#### The geography graph (`society/geography.rs`)

The geography system is **graph-based and already supports transport modalities**:

- **`NodeType`** (lines 13-21): `LandRegion`, `SeaNode`, `OceanNode`.
- **`EdgeType`** (lines 26-35): `LandBorder`, `Coastline`, `SeaLane`, `River`. **These are exactly the hooks needed for land/maritime/river transport.**
- **`Edge`** (lines 75-89): `{ target_node: String, edge_type: EdgeType, distance: f64 (km), is_navigable: bool }`. **Distance is already in kilometers — directly usable for spatial friction calculations.**
- **`Region`** (lines 509-586): Has `node_type: NodeType`, `edges: Vec<Edge>`, `adjacency: Vec<String>` (deprecated, kept for migration). Also has `governance: Option<RegionalGovernance>` (JST), `treasury: Treasury` (regional treasury), `capacity_pool`, `micro_regions`, `climate_profile: ClimateProfile`.
- **`ClimateProfile`** (lines 40-62): Already includes `Coastal` and `Mountainous` variants — partial geographic traits exist.

#### What is MISSING from geography

- **No explicit `Coastal` / `NavigableRiver` boolean traits on `Region`.** `ClimateProfile::Coastal` is a climate, not a port-eligibility flag. A region could be coastal-climate but landlocked, or continental-climate but on a navigable river. We need dedicated `has_coastline: bool` and `has_navigable_river: bool` (or a `GeographicTraits` bitset) on `Region`.
- **No port/airport siting data.** The `Edge` knows if an edge is a `Coastline` or `River`, but there is no flag saying "this region has a usable harbor site."
- **No edge-level infrastructure modifier.** `Edge` has `distance` and `is_navigable` but no `road_level` / `rail_level` / `electrified` field. Infrastructure Networks (Phase 23B) must attach transport-quality data to edges or to a parallel edge-overlay structure.

#### Maritime infrastructure (`infrastructure/maritime.rs`) — exists but decoupled

A complete maritime system already exists:
- **`MaritimeInfrastructure`** (lines 70-81): Country-level aggregate with `shipyards: Vec<Shipyard>`, `ports: Vec<Port>`, `docks: Vec<Dock>`, `available_cash: f64`.
- **`ShipType`** (lines 86-95): `CargoVessel`, `PassengerLiner`, `FishingBoat`, `NavalVessel`.
- **`Port`** (lines 224-265): `cargo_throughput`, `loading_speed`, `berth_count`, `utilization`. Has `region_id`.
- **`Shipyard`** (lines 141-221): Builds ships via `ShipConstructionProject` (progress-by-cost model).
- **B2B integration** (lines 376-473): `submit_shipyard_construction_orders` submits B2B buy bids with `buyer_id = "shipyard_{id}"`. `refund_unfilled_shipyard_bids_maritime` handles refunds.

#### Critical findings — Geography & Maritime

- **The geography graph is ready for spatial friction** but no economic code reads `Region.edges`. Distance and navigability data are present and unused.
- **Maritime infrastructure is a parallel, decoupled system.** It has its own `available_cash`, its own construction projects, and its own B2B bid prefix (`shipyard_`). It is **NOT connected to the regional graph edges** (Coastline/SeaLane) and **NOT connected to freight logistics.** Ports have `region_id` but don't gate maritime freight between coastal regions. Phase 23D must integrate this.
- **No `Coastal`/`NavigableRiver` region traits.** Needed to gate where ports and river barges can be built.
- **No aviation system.** No airports, no aircraft, no `Commodity::AviationFuel` linkage to transport. Phase 23D adds this as a late-game modality.
- **`ClimateProfile::Mountainous`** exists — could gate mountain-pass transport difficulty.

---

## PART 2: TECHNICAL BLUEPRINT & PHASING STRATEGY

### Architecture Overview

Phase 23 introduces **spatial friction** as a first-class economic constraint. The core architectural principle is **route-before-settle**: every cross-region trade must secure a freight route before physical goods move. All cash flows route through `TransferSettler` (`settle_transfer`, `settle_company_to_company`, `settle_b2c_purchase`, `settle_treasury_to_company`) to keep bank balance sheets synchronized — no direct `available_cash += / -=` outside the settler.

| Role | Entity | Responsibility |
|---|---|---|
| **Freight Producer** | `Sector::TransportLogistics` company | Produces `FreightCapacity` (and `PassengerTransport`); sells B2B |
| **Freight Buyer** | Any company with cross-region B2B trades | Buys `FreightCapacity` to move goods; trade freezes if none available |
| **Network Owner** | State or Region (JST) | Owns `TransportNetwork` assets (roads, rail, canals); built via Phase 22 Tenders |
| **Passenger Operator** | JST (subsidized) or private company | Produces `PassengerTransport`; sells B2C to commuters |
| **Commuter** | Worker class in a region | Buys `PassengerTransport` (B2C) to commute to adjacent-region jobs |
| **Draft Animal Cohort** | `FixedAssetCohort` of `Commodity::DraftAnimals` | Early-game transport/agriculture asset; maintained with Fodder + Water |

**CPU/RAM discipline (per task rules):** Infrastructure is handled abstractly as `Networks` owned by State/Region — no individual bus stops, depots, or parking lots. Freight capacity is a commodity scalar, not a per-vehicle simulation. Networks are edge-overlay quality levels, not graph rewiring.

---

### Phase 23A: Spatial Friction & Freight Logistics (B2B)

#### 23A.1: New commodities & enum extensions

**File: `state/src/registries/enums.rs`**

Add to `Commodity` enum (after `PassengerTransport`, line ~389):
```rust
/// "freight_capacity" - B2B freight transport service (Phase 23A).
FreightCapacity,
/// "draft_animals" - Oxen/horses/mules as fixed-asset cohorts (Phase 23A).
DraftAnimals,
```
- Add both to the active-commodity list (~line 693) and the `TryFrom<&str>` mapping (~line 844): `"freight_capacity" => Ok(Commodity::FreightCapacity)`, `"draft_animals" => Ok(Commodity::DraftAnimals)`.
- Extend `is_fixed_asset()` (line 539) to include `Commodity::DraftAnimals` so it is installed as a `FixedAssetCohort` and not consumed per-turn.
- `FreightCapacity` is a **service commodity** (like `MaintenanceServices`) — produced and consumed ephemerally, NOT stockpiled. It does NOT go in `is_fixed_asset` or `is_quality_durable`.

#### 23A.2: Spatial friction model — route computation

**New module: `state/src/economy/logistics.rs`**

A pure-function routing layer over the existing geography graph. No new graph structure — it reads `Region.edges`.

```rust
/// Result of a route lookup between two regions.
#[derive(Debug, Clone, Default)]
pub struct FreightRoute {
    /// Total path distance in km (sum of edge distances).
    pub distance_km: f64,
    /// Cheapest modal cost multiplier (1.0 = baseline dirt road).
    pub friction_multiplier: f64,
    /// Whether a maritime/river segment is usable (reduces cost drastically).
    pub uses_waterborne: bool,
    /// Whether the route is impassable (no path / mountain blockade).
    pub impassable: bool,
}

/// Compute the freight route between a buyer region and a seller region.
/// Uses BFS/Dijkstra over `Region.edges`, summing `edge.distance` and
/// applying a per-edge friction coefficient based on edge type and the
/// TransportNetwork overlay level (Phase 23B).
pub fn compute_freight_route(
    buyer_region_id: &str,
    seller_region_id: &str,
    regions: &[Region],
    network_overlay: &TransportNetworkOverlay,
) -> FreightRoute;

/// Freight cost = quantity * distance_km * friction_multiplier * base_freight_rate.
pub fn freight_cost(route: &FreightRoute, quantity: f64, base_rate: f64) -> f64;

/// Freight capacity required = quantity * distance_km * capacity_per_ton_km.
pub fn freight_capacity_required(route: &FreightRoute, quantity: f64, capacity_per_ton_km: f64) -> f64;
```

**Rules:**
- Same-region trades (buyer.region_id == seller.region_id) -> `distance_km = 0`, `freight_cost = 0`, no capacity required. **Intra-regional trade is frictionless** (preserves current behavior for local economies).
- Cross-region trades require a path through `edges`. If no path exists -> `impassable = true` -> trade freezes.
- Edge friction by type (baseline, before network upgrades): `LandBorder` = 1.0, `River` = 0.8 (if navigable), `Coastline`/`SeaLane` = 0.3 (waterborne, very cheap), `Mountainous` regions impose a 1.5x penalty.
- The `TransportNetworkOverlay` (Phase 23B) multiplies down the friction per edge (e.g., Highway on a `LandBorder` edge -> 0.4x).

#### 23A.3: Freight capacity market — two-phase B2B settlement

**Modify: `state/src/economy/b2b_orders.rs`** and **`state/src/engine/turn.rs`**

The current flow is: `submit_company_b2b_orders` -> `match_orders` -> `settle_trades`. Phase 23A inserts a **freight procurement gate** between matching and settlement:

```
PHASE 23A-1: submit_company_b2b_orders   (unchanged — bids/asks into OrderBook)
PHASE 23A-2: match_orders                (unchanged — produces Vec<Trade>)
PHASE 23A-3: FREIGHT PROCUREMENT         (NEW)
  For each Trade where buyer.region_id != seller.region_id:
    1. compute_freight_route(buyer_region, seller_region)
    2. If impassable -> defer trade (freeze), remove from settlement batch
    3. Compute freight_capacity_required and freight_cost
    4. Buyer submits a FreightCapacity buy bid (separate mini order book OR
       direct capacity allocation from regional TransportLogistics inventory)
    5. If capacity secured -> trade proceeds to settlement; freight cost
       debited from buyer via settle_company_to_company(freight_producer)
    6. If capacity NOT secured -> trade freezes this turn (deferred)
PHASE 23A-4: settle_trades               (only settles freight-secured trades)
```

**New function in `economy/logistics.rs`:**
```rust
/// Split matched trades into (freight-secured, deferred) batches.
/// Secures freight capacity and settles freight payments.
/// Returns the subset of trades safe to settle physically.
pub fn procure_freight_and_split_trades(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    regions: &[Region],
    network_overlay: &TransportNetworkOverlay,
    config: &LogisticsConfig,
    country: &mut Country,
) -> (Vec<Trade>, Vec<DeferredTrade>);
```

**`DeferredTrade`** — a trade that could not secure freight this turn. Stored on the country and retried next turn (capacity may become available). Has a `deferred_turns` counter; trades deferred too long are cancelled with bid-refund.

**Freight payment:** The buyer pays the freight-producing company via `settle_company_to_company` (double-entry, bank balance sheets synced). The freight producer's `FreightCapacity` inventory is decremented (ephemeral service — consumed on delivery).

**Freeze behavior:** "Trade freezes" = the trade is NOT settled this turn. Cash encumbrance on the buyer's bid is **refunded** (via `refund_unfilled_bids` pattern). The buyer may re-bid next turn. This prevents money vanishing into phantom deliveries.

#### 23A.4: Freight-producing production methods

**File: `state/src/registries/production_methods_data.rs`** (`transport_methods`)

Add freight outputs to existing methods (split passenger/freight) and add early-game freight methods:

| Method | Year | Inputs | FreightCapacity Output | Notes |
|---|---|---|---|---|
| Pack Caravans (Draft) | 1850 | Fodder (8), Water (4) | 5 | Requires `DraftAnimals` cohort |
| Horse-Drawn Freight Wagons | 1880 | Fodder (6), Water (3) | 12 | Requires `DraftAnimals` cohort |
| Steam Freight Trains | 1885 | Fuels (15), Steel (5) | 40 | Requires RailNetwork (23B) |
| Diesel Freight Trains | 1930 | Fuels (12), MechanicalComponents (5) | 80 | Requires RailNetwork |
| Container Trucking | 1960 | Fuels (15), Steel (10) | 120 | Requires Highway |
| Air Cargo | 1960 | Fuels (25), Aluminum (8) | 60 | Requires Airport (23D) |

The existing passenger methods retain `PassengerTransport` outputs. Methods can output BOTH (e.g., a rail method outputs PassengerTransport + FreightCapacity).

#### 23A.5: Draft Animals as FixedAssetCohort

**File: `state/src/economy/fixed_assets.rs`**

`Commodity::DraftAnimals` is a fixed asset (`is_fixed_asset() -> true`). It is installed as a `FixedAssetCohort` with:
- `commodity: Commodity::DraftAnimals`
- `durability`: lower than machinery (animals age — e.g., 80 turns vs 200).
- `condition`: degrades via `degrade_cohorts` (same machinery path).

**Animal-specific maintenance:** The existing maintenance system consumes `MaintenanceServices` (produced by `MaintenanceWorkshops`). Draft animals require **`Fodder` + `Water`** instead. Implementation:

**New function in `fixed_assets.rs`:**
```rust
/// Compute Fodder + Water needed to sustain draft-animal cohorts.
/// Only counts cohorts where commodity == DraftAnimals.
pub fn draft_animal_maintenance_needed(
    cohorts: &[FixedAssetCohort],
    config: &GenerativeGoodsConfig,
) -> BTreeMap<Commodity, f64>;
// Returns {Fodder: X, Water: Y}

/// Restore draft-animal cohort condition by consuming Fodder + Water.
/// Mirrors restore_cohort_condition but with animal feed inputs.
pub fn feed_draft_animals(
    cohorts: &mut [FixedAssetCohort],
    fodder_available: f64,
    water_available: f64,
    config: &GenerativeGoodsConfig,
) -> (f64, f64); // (fodder consumed, water consumed)
```

**Integration:** Buildings with `DraftAnimals` cohorts submit B2B buy bids for `Fodder` and `Water` (alongside their normal inputs) in `submit_company_b2b_orders`. After B2B settlement, `feed_draft_animals` runs. Animals not fed degrade faster (condition drops, eventually scrapped = perished).

**Agriculture hook:** `Commodity::DraftAnimals` cohorts on farms boost `machinery_factor` (already handled — `machinery_factor` sums all non-scrapped cohorts). This gives pre-industrial farms a mechanization path without IndustrialMachinery.

---

### Phase 23B: Infrastructural Networks & Tenders

#### 23B.1: Network data structures

**New module: `state/src/economy/transport_networks.rs`**

```rust
/// Level of transport infrastructure on a connection between two regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkLevel {
    #[default]
    /// No improved infrastructure (baseline dirt paths).
    None,
    /// Gravel/dirt road (slight friction reduction).
    DirtRoad,
    /// Paved road (moderate reduction).
    PavedRoad,
    /// Rail network (large reduction; unlocks trains).
    RailNetwork,
    /// Electrified rail (unlocks electric trains; requires Energy).
    ElectrifiedRail,
    /// Highway (modern; large reduction for trucks).
    Highway,
    /// Canal (waterborne; for non-coastal freight).
    Canal,
}

/// A bidirectional transport network link between two regions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NetworkLink {
    pub region_a: String,
    pub region_b: String,
    pub level: NetworkLevel,
    /// Condition 0.0-1.0 (degrades; requires maintenance).
    pub condition: f64,
    /// Turn constructed.
    pub built_turn: u32,
}

/// Overlay mapping region-pair keys -> NetworkLink.
/// Stored on Country (national networks) and Region (regional/feeder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TransportNetworkOverlay {
    /// Key = "min(a,b)|max(a,b)" for canonical bidirectional lookup.
    pub links: BTreeMap<String, NetworkLink>,
}

impl TransportNetworkOverlay {
    pub fn get_link(&self, a: &str, b: &str) -> Option<&NetworkLink>;
    pub fn get_link_mut(&mut self, a: &str, b: &str) -> Option<&mut NetworkLink>;
    pub fn friction_multiplier(&self, a: &str, b: &str, edge_type: &EdgeType) -> f64;
}
```

**Friction multipliers by NetworkLevel (applied to `LandBorder` edges):**

| Level | Friction x | Unlocks |
|---|---|---|
| None | 1.0 | Draft animals, pack caravans |
| DirtRoad | 0.8 | Horse-drawn wagons |
| PavedRoad | 0.6 | Early trucks |
| RailNetwork | 0.35 | Steam/Diesel trains |
| ElectrifiedRail | 0.30 | Electric trains (requires Energy input) |
| Highway | 0.40 | Container trucking |
| Canal | 0.25 | Barges (non-coastal waterborne) |

#### 23B.2: Construction via Phase 22 Tenders

Networks are built using the existing Phase 22 tender system (`construction/tenders.rs`, `construction/tender_market.rs`). No new construction machinery — reuse `ConstructionTender` + `ConstructionProject`.

**Extend `ConstructionProjectType`** (`construction/projects.rs`, line 13):
```rust
pub enum ConstructionProjectType {
    Residential,
    Commercial,
    UtilityNetwork,
    Infrastructure,
    SocialHousing,
    Factory,
    // ── Phase 23B ──
    TransportNetwork,  // Roads, rail, highways, canals
}
```

**Extend `ConstructionProject`** with network-link target metadata:
```rust
    // ── Phase 23B: Network link target ──
    /// Region pair this network link connects (None for non-network projects).
    pub network_link_target: Option<(String, String)>,
    /// Network level to build/upgrade to.
    pub network_target_level: Option<NetworkLevel>,
```

**New BOM function** (`construction/bom.rs`):
```rust
/// BOM for transport network construction. MASSIVE material requirements.
pub fn get_network_construction_bom(level: NetworkLevel, distance_km: f64) -> BTreeMap<Commodity, f64>;
```

BOM scales with distance. Examples (per 100 km):

| Level | Timber | Steel | Cement | Bricks | ConstructionMachinery | Stone |
|---|---|---|---|---|---|---|
| DirtRoad | 200 | 20 | 100 | 50 | 10 | 200 |
| PavedRoad | 100 | 100 | 800 | 400 | 30 | 600 |
| RailNetwork | 300 | 1500 | 1000 | 200 | 80 | 500 |
| ElectrifiedRail | 200 | 2000 | 1200 | 200 | 100 | 400 |
| Highway | 50 | 500 | 2000 | 600 | 120 | 1500 |
| Canal | 50 | 100 | 3000 | 1000 | 150 | 2000 |

**Tender flow (reuses Phase 22A verbatim):**
1. **State publishes tender**: `investor_id = "STATE:{region_id}"`, `investor_type = State`, `project_type = TransportNetwork`, `required_materials = get_network_construction_bom(...)`, `target_building_type = "RailNetwork"` (or equivalent).
2. **Construction companies bid**: Same `submit_bid` / `award_tender` path. Tranches released via `settle_treasury_to_company`.
3. **On completion** (`advance_construction_projects`): Instead of adding worker_capacity to a building, the project **installs a `NetworkLink`** into `country.transport_networks.links` with the target `NetworkLevel`. The `network_link_target` fields drive this.

**Network condition degradation & maintenance:** `NetworkLink.condition` degrades each turn (like building condition). Low-condition links lose their friction bonus. Maintenance is funded by the State/Region treasury (JST for local roads, central Treasury for national rail). A new `process_network_maintenance` function debits the owning treasury and restores condition.

#### 23B.3: Unlocking advanced fixed assets

**File: `state/src/economy/production.rs`** (or production method gating)

Production methods that require infrastructure (e.g., "Electric Trams" requires `ElectrifiedRail`, "Steam Freight Trains" requires `RailNetwork`) are gated: a `TransportLogistics` building can only activate these methods if its region is connected to the network at the required level. This is a **method-eligibility check** at production time:
```rust
/// Returns true if the building's region has the required network level
/// on at least one of its edges.
pub fn network_requirement_met(
    building_region_id: &str,
    regions: &[Region],
    overlay: &TransportNetworkOverlay,
    required: NetworkLevel,
) -> bool;
```

Production methods gain an optional `requires_network: Option<NetworkLevel>` field in their definition. Methods whose requirement is unmet fall back to the highest available lower-tier method.

---

### Phase 23C: Commuting & Municipal Transport (B2C/Labor)

#### 23C.1: Commuter-eligible labor pool

**New module: `state/src/economy/commuting.rs`**

A pre-phase that builds a map of which regions' labor can commute to which other regions, based on the geography graph + network overlay + transport availability.

```rust
/// A worker's commute eligibility to a target region.
#[derive(Debug, Clone, Default)]
pub struct CommuteOption {
    pub home_region_id: String,
    pub target_region_id: String,
    pub distance_km: f64,
    /// PassengerTransport units required per FTE per turn.
    pub commute_cost_units: f64,
    /// Cash ticket price (set by transport operator, subsidized or private).
    pub ticket_price: f64,
    /// Whether the worker can afford the ticket (computed per-class).
    pub affordable: bool,
}

/// Build commute options for all region pairs reachable via the graph.
pub fn build_commute_map(
    regions: &[Region],
    overlay: &TransportNetworkOverlay,
    passenger_transport_price: f64,
    config: &CommutingConfig,
) -> Vec<CommuteOption>;
```

**Rules:**
- A worker can commute only to **directly adjacent** regions (one edge hop) — no multi-hop commuting (keeps CPU bounded; multi-hop is freight's domain).
- `commute_cost_units` = `distance_km * commute_capacity_per_km * class_commute_frequency`.
- The ticket price comes from the `PassengerTransport` market clearing (23C.2).

#### 23C.2: Passenger transport B2C clearing (municipal vs private)

**New function in `economy/b2c_services.rs`** (mirrors `clear_education_slots_b2c` / `clear_health_capacity_b2c`):

```rust
/// Clear the PassengerTransport B2C market for commuting workers.
/// Public (JST) operators are subsidized; private operators charge market price.
pub fn clear_passenger_transport_b2c(
    transport_buildings: &mut [Building],  // TransportLogistics buildings
    companies: &mut [Company],
    country: &mut Country,
    commute_map: &[CommuteOption],
    config: &ServicePricingConfig,
) -> BTreeMap<String, f64>; // region_id -> commute coverage ratio
```

**Pattern (identical to existing education/health B2C):**
- `compute_service_transactions` (read-only): For each transport building with `PassengerTransport` inventory, determine `is_public` (owner starts with `LOCAL_` or `STATE_`). Public -> government subsidy from `country.budget.liquid_reserves` (or `region.governance.budget.liquid_reserves` for JST). Private -> citizen pays full price.
- Settle: `settle_b2c_purchase` for citizen payments; `country.budget.liquid_reserves -= subsidy` for public subsidy (double-entry).
- **Coverage ratio** = `transport_capacity_available / commute_demand`. If coverage < 1.0, some workers cannot commute.

#### 23C.3: Labor market integration — commuter FTE

**Modify: `state/src/economy/labor_market.rs`** (`resolve_regional_labor_market`)

Before the existing clearing loop, inject commuter-supplied FTE:

```rust
// NEW: Add commuter FTE to the regional labor pool.
// Workers from adjacent regions who secured PassengerTransport join this
// region's labor pool as "commuter FTE" — tagged by home region for wage remittance.
for commute in commute_map.iter().filter(|c| c.target_region_id == region.id && c.affordable) {
    let home_region = regions.iter().find(|r| r.id == commute.home_region_id)?;
    // Pull available_fte from home region's classes (proportional, clamped)
    // Add to this region's pool as commuter entries
    // Tag allocations for wage remittance back to home region
}
```

**Wage remittance for commuters:** Commuters' net wages are credited to their **home region's** class savings (not the host region). This uses `TransferRecipient::CitizenSavings { region_idx: home_region_idx, ... }`. The host region's companies still debit the full wage (double-entry preserved).

**Affordability gate:** A class can only commute if `class.savings >= ticket_price * commute_frequency` (or the ticket is subsidized to near-zero by JST). **If a class cannot afford the ticket, its FTE is NOT added to the commuter pool** -> localized labor shortage in the host region, surplus unemployment in the home region. This is the "lower-class unrest due to job exclusion" mechanism.

#### 23C.4: JST vs Private transport — neoliberal privatization law

**File: `state/src/politics/laws.rs`**

Add a new law variant:
```rust
pub enum LawType {
    // ... existing ...
    /// Phase 23C: Transport ownership law.
    Transport(TransportLaw),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportLaw {
    /// If Privatized, JST-owned transport buildings are sold to private companies.
    /// Ticket subsidies removed -> prices rise -> lower classes may be excluded.
    pub ownership: TransportOwnership,
    /// Subsidy fraction for public transport (0.0-1.0 of ticket price).
    pub public_subsidy_fraction: f64,
}

pub enum TransportOwnership {
    Public,      // JST operates; subsidized
    Privatized,  // Sold to private operators; market pricing
}
```

**Privatization effect:** When enacted, all `TransportLogistics` buildings with `owner_id` starting with `LOCAL_` or `STATE_` are transferred to the highest-bidding private `TransportLogistics` company (or a new spin-off). Subsidy drops to `public_subsidy_fraction` (typically 0.0). Ticket prices rise to market clearing -> lower-class commuters priced out -> unrest driver (via existing `SentimentDrivers` / `mass_movements`).

**Unrest linkage:** The commute coverage ratio and affordability feed into `calculate_yoy_drivers` (geography.rs line 823) as a new `transport_exclusion_rate` field -> increases radical sentiment among excluded classes.

---

### Phase 23D: Geography & Modalities

#### 23D.1: Geographic traits on Region

**File: `state/src/society/geography.rs`**

Add to `Region` (after `holy_site`, line 585):
```rust
    // ── Phase 23D: Geographic transport traits ──
    /// True if this region borders a sea/ocean (enables ports, maritime freight).
    #[serde(rename = "wybrzeze", default)]
    pub has_coastline: bool,
    /// True if this region has a navigable river (enables river barges).
    #[serde(rename = "rzeka_zeglowna", default)]
    pub has_navigable_river: bool,
    /// True if this region contains a mountain pass (high-friction edge).
    #[serde(rename = "przelecz_gorska", default)]
    pub has_mountain_pass: bool,
```

**Assignment at generation:** `has_coastline` = any edge with `edge_type == Coastline`. `has_navigable_river` = any edge with `edge_type == River && is_navigable`. `has_mountain_pass` = `climate_profile == Mountainous` (or a dedicated check).

#### 23D.2: Maritime freight integration

**File: `state/src/infrastructure/maritime.rs`** + **`state/src/economy/logistics.rs`**

Integrate the existing maritime system into the freight route computation:

- In `compute_freight_route`, if **both** endpoints have `has_coastline = true` and a `SeaLane`/`Coastline` path exists through sea nodes, use the **waterborne route**: `friction_multiplier = 0.2` (extremely cheap, high volume), `uses_waterborne = true`.
- Waterborne freight requires **port capacity** at both endpoints: `Port.cargo_throughput * Port.utilization` (existing fields). If either port is at capacity, maritime freight is capped and overflow routes via land.
- `FreightCapacity` for maritime is produced by a new method "Maritime Shipping" (CargoVessel-linked) with very high output but requires a Port building in a coastal region.

**River freight:** If both endpoints are on the same navigable river (`has_navigable_river` and connected via `River` edges with `is_navigable = true`), use `friction_multiplier = 0.25`. Requires `Canal` network level OR natural river navigability. Barges are a `DraftAnimals`-towed or steam-towed early option.

#### 23D.3: Aviation (late-game)

**New commodity & assets:**
- Aviation fuel: **reuse `Commodity::Fuels`** with a 2.5x input multiplier to avoid adding a new commodity (keep the enum lean).
- `Commodity::Aluminum` — already exists (enums.rs line 210). Aircraft are aluminum-intensive.

**New production method** in `transport_methods`:
```rust
m.insert(MethodSlot::Production, "Air Cargo".into(),
    pm(1960, Some("auto3_002"), 0.25, 0.40, 0.35, 6.0,
       &[(Commodity::Fuels, 25.0), (Commodity::Aluminum, 8.0)],
       &[(Commodity::FreightCapacity, 60.0), (Commodity::PassengerTransport, 40.0)]));
```
- **Extremely high speed** (friction multiplier 0.1 — near-instant), **extremely high cost** (Fuels + Aluminum).
- **Requires Airport building** in the region. Airports are built via Phase 22 Tenders (`ConstructionProjectType::TransportNetwork` with a new `Airport` target, or a dedicated building type).
- **No network link required** — aviation bypasses the land graph entirely. Route = direct distance between region centroids (approximated by edge-sum to a common reference).

**Airport building:**
- New building type "Airport" in `registries/buildings.rs`.
- Can only be built in regions with sufficient flat land (approximation: not `Mountainous` climate, or `has_mountain_pass = false`).
- Built via State tender; owned by State or privatized (same `TransportLaw` mechanism).
- Enables `Air Cargo` and (optionally) "Airliner" passenger methods.

---

## CROSS-CUTTING CONCERNS

### Double-Entry Accounting (strict `TransferSettler`)

| Cash flow | Settler function | Payer -> Recipient |
|---|---|---|
| Freight payment (B2B) | `settle_company_to_company` | Buyer company -> Freight producer company |
| Public transport subsidy | treasury debit (following `b2c_services.rs` precedent) | Treasury -> Transport operator |
| Private transport ticket | `settle_b2c_purchase` | Citizen savings -> Transport company |
| Network tender tranche (State) | `settle_treasury_to_company` | Treasury -> Construction contractor |
| Commuter wage remittance | `settle_transfer` with `CitizenSavings { home_region_idx }` | Host company -> Home-region citizen savings |
| Network maintenance | treasury debit + `credit_company_by_id` | Treasury -> Maintenance contractor |

**Rule:** No direct `available_cash += / -=` or `brokerage_account.cash +=` outside `TransferSettler`. All bank balance sheets (deposits + reserves) stay synchronized. The existing `settle_trades` in `b2b_orders.rs` uses direct mutations (lines 387-403) — this is a known legacy pattern; Phase 23 freight payments use the settler properly. (Migrating `settle_trades` itself to the settler is out of scope for Phase 23 but noted as future cleanup.)

### Language & Naming

- **All struct properties, enums, and variables strictly in English** (per task rule). New fields use English names. `#[serde(rename = "...")]` is used only where matching the existing Polish-serialization convention is required for consistency (the codebase uses Polish serde renames extensively for backward compatibility); new Phase 23 fields use English serde renames or snake_case English where no prior convention exists.
- New modules: `economy/logistics.rs`, `economy/transport_networks.rs`, `economy/commuting.rs`. All registered in `economy/mod.rs` and `lib.rs` (which re-exports `pub mod economy`).

### Memory Safety & CPU Discipline

- **Networks are edge-overlay quality levels**, not graph rewiring — O(E) storage where E = number of region-pair links, bounded by the number of adjacent region pairs.
- **Freight routing is BFS over `Region.edges`** — bounded by region count (typically < 50 regions per country); memoized per turn.
- **Commuting is single-hop only** — O(R * avg_degree), not all-pairs.
- **FreightCapacity is an ephemeral service commodity** — produced and consumed in the same turn, not stockpiled (like `MaintenanceServices`). No per-vehicle state.
- **DraftAnimals use `FixedAssetCohort`** — the existing compaction system (`compact_cohorts`) bounds RAM.
- **Deferred trades** have a max retry counter; expired deferrals are cancelled with refund (no unbounded queue growth).

---

## IMPLEMENTATION ORDER & FILE MAP

### Phase 23A (Spatial Friction & Freight)
1. `registries/enums.rs` — add `FreightCapacity`, `DraftAnimals` commodities; extend `is_fixed_asset`.
2. **NEW** `economy/logistics.rs` — `FreightRoute`, `compute_freight_route`, `freight_cost`, `freight_capacity_required`, `procure_freight_and_split_trades`, `DeferredTrade`, `LogisticsConfig`.
3. `economy/mod.rs` — register `logistics` module.
4. `registries/production_methods_data.rs` — add freight methods to `transport_methods`.
5. `economy/fixed_assets.rs` — add `draft_animal_maintenance_needed`, `feed_draft_animals`.
6. `economy/b2b_orders.rs` — `submit_company_b2b_orders` adds Fodder/Water bids for DraftAnimals buildings.
7. `engine/turn.rs` — insert `PHASE 23A-3: FREIGHT PROCUREMENT` between matching and settlement.

### Phase 23B (Networks & Tenders)
8. **NEW** `economy/transport_networks.rs` — `NetworkLevel`, `NetworkLink`, `TransportNetworkOverlay`.
9. `economy/mod.rs` — register `transport_networks` module.
10. `construction/projects.rs` — add `TransportNetwork` to `ConstructionProjectType`; add `network_link_target`, `network_target_level` to `ConstructionProject`.
11. `construction/bom.rs` — add `get_network_construction_bom`.
12. `construction/orders.rs` / `construction/tender_market.rs` — State publishes network tenders; on completion install `NetworkLink`.
13. `state/mod.rs` (Country) — add `transport_networks: TransportNetworkOverlay` field.
14. `economy/logistics.rs` — `compute_freight_route` consults `TransportNetworkOverlay`.
15. Production method gating — `requires_network` eligibility check.

### Phase 23C (Commuting & Municipal Transport)
16. **NEW** `economy/commuting.rs` — `CommuteOption`, `build_commute_map`, `CommutingConfig`.
17. `economy/mod.rs` — register `commuting` module.
18. `economy/b2c_services.rs` — add `clear_passenger_transport_b2c` (mirror education/health pattern).
19. `economy/labor_market.rs` — inject commuter FTE into `resolve_regional_labor_market`; wage remittance to home region.
20. `politics/laws.rs` — add `Transport(TransportLaw)`, `TransportOwnership`, privatization enactment.
21. `society/geography.rs` — add `transport_exclusion_rate` to sentiment drivers.

### Phase 23D (Geography & Modalities)
22. `society/geography.rs` — add `has_coastline`, `has_navigable_river`, `has_mountain_pass` to `Region`; assign at generation.
23. `economy/logistics.rs` — waterborne route branch in `compute_freight_route`; port-capacity gating.
24. `infrastructure/maritime.rs` — integrate ports into freight capacity production.
25. `registries/buildings.rs` — add "Airport" building type.
26. `registries/production_methods_data.rs` — add "Air Cargo" method.
27. World generator — assign coastal/river traits based on edges.

---

## VERIFICATION

- [ ] `cargo build --release` compiles with no errors after each sub-phase.
- [ ] `cargo test` — existing tests pass (no regressions in B2B settlement, labor market, construction).
- [ ] New unit tests in `economy/logistics.rs`: `compute_freight_route` for same-region (distance 0), adjacent regions, impassable (no path), waterborne route.
- [ ] New unit tests in `economy/fixed_assets.rs`: `draft_animal_maintenance_needed` and `feed_draft_animals` — animals perish without Fodder.
- [ ] New unit tests in `economy/transport_networks.rs`: `friction_multiplier` for each `NetworkLevel`.
- [ ] Integration test: a cross-region trade with no `FreightCapacity` available freezes; with capacity, settles and freight payment routes through `TransferSettler`.
- [ ] Integration test: a worker in region A commutes to adjacent region B when `PassengerTransport` is affordable; cannot commute when unaffordable.
- [ ] Save/load compatibility: new `Region` fields (`has_coastline`, etc.) and `Country.transport_networks` use `#[serde(default)]` so legacy saves load without migration.

## RISKS / CONSIDERATIONS

- **Performance:** Freight routing runs per-trade per-turn. With N regions and T trades, naive Dijkstra is O(T * (N + E)). Mitigation: memoize routes per (buyer_region, seller_region) pair per turn; the pair count is bounded by O(R^2) and typically far smaller.
- **Cold-start deadlock risk:** If no `TransportLogistics` companies exist at game start, all cross-region trades freeze. Mitigation: seed at least one transport company per region in the generator; allow `DraftAnimals`-based freight (Pack Caravans) as a fallback that requires no machinery.
- **`settle_trades` legacy:** The existing B2B settlement uses direct cash mutation, not the `TransferSettler`. Phase 23A freight payments use the settler, creating a temporary inconsistency. This is acceptable — the settler is used for all NEW cash flows. Full migration of `settle_trades` is a future cleanup.
- **Backward compatibility:** All new `Region`/`Country`/`ConstructionProject` fields use `#[serde(default)]` so existing saves load. New commodities (`FreightCapacity`, `DraftAnimals`) are appended to the enum and added to the active list / TryFrom map — no renumbering of existing variants.
- **Maritime integration scope:** The existing `MaritimeInfrastructure` has its own `available_cash` and construction flow. Phase 23D integrates it into freight routing but does NOT refactor its construction system (out of scope). Port capacity is read; shipyards remain as-is.
