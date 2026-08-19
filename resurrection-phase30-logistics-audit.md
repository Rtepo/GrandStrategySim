# Phase 30 â€” Global Logistics, Trade Routes & Highway Retail Audit

Read-only audit and technical blueprint for dynamic Dijkstra routing, territorial waters, overflight fees, and gas stations. No Rust code to be written until this blueprint is explicitly approved.

---

## Summary

Phase 30 upgrades the logistics system from a basic distanceĂ—friction Dijkstra to a multi-modal route planner with per-mode fuel costs, tags sea lanes as territorial/international for blockades and maritime transit tariffs, introduces air-cargo overflight fees, and adds Gas Stations as a new B2C retail building type that sells `Commodity::Fuels`/`RefinedFuel` to consumers and logistics fleets.

---

## PART 1: Dynamic Trade Routes (Dijkstra Upgrade)

### Current State

`economy/logistics/logistics.rs` implements `compute_freight_route()` â€” a Dijkstra pathfinder over `Region.edges`. The edge weight is:

```
weight = edge.distance Ă— friction_coefficient
```

Where `friction_coefficient` is derived from:
- `EdgeType` (LandBorder / River / Coastline / SeaLane)
- `TransportNetworkOverlay` (NetworkLevel: None / DirtRoad / PavedRoad / RailNetwork / ElectrifiedRail / Highway / Canal)
- `NetworkLink.condition` (0.0â€“1.0, degrades per turn)
- Mountain penalty (1.5Ă— for Mountainous land edges)

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\economy\logistics\logistics.rs" /> lines 124â€“156 (`edge_friction_coefficient`), lines 169â€“296 (`compute_freight_route`).

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\economy\logistics\transport_networks.rs" /> lines 47â€“80 (`NetworkLevel::land_friction_multiplier`).

### Problems

1. **No mode-specific fuel costs.** The freight cost formula is `quantity Ă— distance Ă— friction Ă— base_rate`. It does not differentiate between a truck convoy burning `Fuels` on a Highway vs a train on ElectrifiedRail consuming `Energy`. The production methods already consume `Fuels` as an input (lines 947â€“964 of `production_methods_data.rs`), but the routing layer ignores which mode is being used.

2. **No speed/time dimension.** The Dijkstra minimizes cost only. A Highway route might be cheaper but slower than a RailNetwork route (or vice versa). There's no time-weighted edge attribute.

3. **No congestion modeling.** `NetworkLink.condition` degrades from lack of maintenance, but there's no per-turn congestion from high traffic volume on a link.

4. **Sea nodes are traversable but have no Region entry in the Dijkstra** (line 221: `None => continue`). The comment says "Sea/ocean nodes have no Region entry" but in practice they DO have Region entries (generated in `geography.rs` lines 1551, 1587 with `NodeType::SeaNode`/`OceanNode`). This means sea-node traversal currently fails silently â€” the Dijkstra can only use direct Coastline edges between two land regions, not multi-hop sea routes through intermediate sea nodes.

### Proposed Upgrade

#### 1.1 Multi-Modal Edge Weight

Replace the single `friction_coefficient` with a composite weight:

```
weight = edge.distance Ă— (friction + fuel_cost_per_km + toll_cost)
```

Where:
- `friction` = current friction coefficient (unchanged)
- `fuel_cost_per_km` = mode-specific fuel consumption Ă— fuel price
  - LandBorder + Highway â†’ truck fuel rate Ă— `Fuels` market price
  - LandBorder + RailNetwork/ElectrifiedRail â†’ train fuel rate Ă— `Fuels` or `Energy` price
  - Coastline/SeaLane â†’ ship fuel rate Ă— `Fuels` price
  - River (navigable) â†’ barge fuel rate Ă— `Fuels` price (cheapest)
- `toll_cost` = per-km toll on the link (0.0 by default; can be set by the state)

**Files to modify:**
- `economy/logistics/logistics.rs` â€” `edge_friction_coefficient()` â†’ `edge_weight()` with fuel/toll parameters
- `economy/logistics/transport_networks.rs` â€” add `fuel_consumption_per_km()` to `NetworkLevel`
- `economy/logistics/logistics.rs` â€” `FreightLogisticsConfig` gains `truck_fuel_rate`, `train_fuel_rate`, `ship_fuel_rate`, `barge_fuel_rate` fields

#### 1.2 Fix Sea-Node Traversal

The Dijkstra must traverse through intermediate sea/ocean nodes. The fix:
- Remove the `None => continue` skip at line 221 â€” sea nodes DO have Region entries.
- Instead, check `region.node_type`: if `SeaNode` or `OceanNode`, only traverse `SeaLane`/`Coastline` edges (no land edges from sea nodes).
- If `LandRegion`, only traverse `LandBorder`/`River`/`Coastline` edges (no `SeaLane` from land regions).

**File:** `economy/logistics/logistics.rs` lines 218â€“248.

#### 1.3 Congestion Modeling (Approved for Phase 30)

Add a `congestion: f64` field to `NetworkLink` (0.0 = empty, 1.0 = gridlocked). Each turn:
- Freight routes passing through a link add to its congestion (proportional to `FreightCapacity` consumed on that link).
- The effective friction is scaled by `(1.0 + congestion_penalty Ă— congestion)`.
- Congestion decays each turn by a configurable rate (e.g., 0.10 = 10% decay per turn).

**Files to modify:**
- `economy/logistics/transport_networks.rs` â€” add `congestion: f64` to `NetworkLink`, add `congestion_decay_rate` to config, update `effective_friction()` to include congestion scaling
- `economy/logistics/logistics.rs` â€” after freight procurement, increment congestion on used links
- `engine/turn.rs` â€” call congestion decay at end of turn

### Implementation Steps (Part 1)

1. Add `fuel_consumption_per_km()` method to `NetworkLevel` in `transport_networks.rs`.
2. Add fuel-rate config fields to `FreightLogisticsConfig` in `logistics.rs`.
3. Rename/replace `edge_friction_coefficient()` with `edge_weight()` that takes fuel prices and tolls as parameters.
4. Update `compute_freight_route()` to:
   - Accept fuel prices (`HashMap<Commodity, f64>`) as a parameter.
   - Fix sea-node traversal by checking `region.node_type` instead of skipping missing regions.
   - Use the new composite edge weight.
5. Update `procure_freight_and_split_trades()` to pass fuel prices to the router.
6. Update `freight_cost()` to use the new composite cost (fuel + friction + tolls).
7. Add unit tests for multi-hop sea routes and mode-specific fuel costs.

---

## PART 2: Territorial Waters & Overflight Fees

### Current State

**Maritime:** `infrastructure/maritime.rs` defines `MaritimeInfrastructure` with ports, shipyards, and docks. `EdgeType::Coastline` and `EdgeType::SeaLane` exist in the geography graph. Sea nodes (`NodeType::SeaNode`, `NodeType::OceanNode`) are generated as `Region` entries with `owner_country: String::new()` (empty owner). There is no concept of territorial waters â€” all sea lanes are international.

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\infrastructure\maritime.rs" />

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\society\geography.rs" /> lines 13â€“35 (`NodeType`, `EdgeType`), lines 76â€“89 (`Edge`), lines 612â€“627 (`GeographicTraits`).

**Aviation:** `GeographicTraits.has_airport` exists (line 626) and is set by airport construction. The "Air Cargo" production method exists (line 961 of `production_methods_data.rs`) producing `FreightCapacity` + `PassengerTransport`. However, there is no aviation routing â€” air cargo is treated identically to ground freight. There is no overflight fee system.

**International trade:** `international/trade.rs` `balance_global_trade()` is abstract â€” it computes competitiveness-weighted export/import shares without physical routing. The `DiplomaticRelation` struct has `embargo_penalty`, `free_trade`, `customs_union` fields. Embargoes are checked by `match_orders_with_embargoes()` in `order_book.rs`. Phase 29 added FTA/embargo tariff overrides in `settle_trades_with_tariffs()`.

### Proposed Design

#### 2.1 Territorial Waters

**Concept:** Certain `SeaLane` edges are "territorial waters" belonging to a specific country. Ships transiting these edges are subject to:
- Blockades (if the owner country has an embargo against the trading pair)
- Maritime transit tariffs (a per-ton-km fee paid to the owner country's Treasury)

**Implementation:**

Add a `territorial_owner: Option<String>` field to `Edge` in `geography.rs`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub territorial_owner: Option<String>,
```

- `None` = international waters (no owner, free transit)
- `Some("country_name")` = territorial waters of that country

This field is populated during world generation: `SeaLane` edges adjacent to a country's coastline regions get `territorial_owner = Some(that_country)`.

**Blockade check:** In `compute_freight_route()`, when traversing a `SeaLane` edge with `territorial_owner = Some(owner)`:
- If the owner has an embargo (`ban_import` or `ban_export`) against either the buyer's or seller's country â†’ the edge is impassable (blockade).
- This requires passing `diplomacy` and `company_country` maps to `compute_freight_route()`.

**Maritime transit tariff:** In `procure_freight_and_split_trades()`, after route computation:
- For each `SeaLane` edge in the path with `territorial_owner = Some(owner)`:
  - Compute `transit_fee = quantity Ă— edge.distance Ă— maritime_transit_rate`
  - Debit the buyer company, credit the owner country's Treasury
  - This is NOT a trade tariff on the cargo â€” it's a transit fee for passing through territorial waters

**Route reconstruction:** The Dijkstra currently only stores `dist`, `path_distance`, and `path_uses_water`. It needs to also store the edge sequence (path reconstruction) so we can iterate the path edges for territorial-owner checks and transit-fee calculation.

**Files to modify:**
- `society/geography.rs` â€” add `territorial_owner` to `Edge`
- `economy/logistics/logistics.rs` â€” `FreightRoute` gains `path_edges: Vec<(String, String, EdgeType)>` for route reconstruction; `compute_freight_route()` gains `diplomacy` and `company_country` parameters for blockade checks
- `engine/turn.rs` â€” pass diplomacy/company_country to route computation
- `engine/generator/` â€” populate `territorial_owner` during world generation

#### 2.2 Air Transit & Overflight Fees

**Concept:** Air cargo flying over a country does NOT pay trade tariffs on the cargo, but pays **Overflight Fees (Air Navigation Charges)** to each country's Treasury whose airspace is traversed.

**Implementation:**

Air cargo routing is conceptually different from ground/maritime routing:
- Air cargo flies in a straight line (great-circle) from origin airport to destination airport.
- The "airspace" overflown is determined by which countries' regions are on the path.
- Overflight fees are paid per-km of airspace traversed.

**Step 1: Add 2D coordinates to Region (CORRECTION â€” no hardcoded overlays).**

The user confirmed that Region does NOT currently have X,Y coordinates. Phase 30 must add them. Do NOT create an `AirspaceOverlay` â€” use mathematical 2D spatial computation instead.

Add `coord_x: f64` and `coord_y: f64` to `Region` in `society/geography.rs`:
```rust
/// Phase 30: 2D spatial coordinate for geographic computations
/// (air cargo routing, overflight fee calculation).
#[serde(default, skip_serializing_if = "is_zero_f64")]
pub coord_x: f64,
#[serde(default, skip_serializing_if = "is_zero_f64")]
pub coord_y: f64,
```

- Both fields are `#[serde(default)]` so old saves deserialize to (0.0, 0.0).
- A migration step on first load assigns coordinates based on the graph topology (spring-layout or deterministic hash-based placement).
- World generation populates coordinates for new regions.
- Sea/ocean nodes also get coordinates (they participate in air route crossing detection).

**Step 2: Air cargo routing (2D vector math â€” CORRECTION).**

Add a separate `compute_air_route()` function in a new `economy/logistics/air_cargo.rs` module:
- Only regions with `has_airport = true` are valid endpoints.
- The route is a **straight 2D vector** from origin airport's `(coord_x, coord_y)` to destination airport's `(coord_x, coord_y)`.
- For each other region in the world, mathematically compute the **perpendicular distance** from that region's center to the flight path vector. If the distance is below a threshold (e.g., 100 km), that region's airspace is considered "overflown".
- For each overflown region whose `owner_country` differs from the cargo's origin country, an overflight fee is charged: `fee = path_segment_length_within_airspace Ă— overflight_rate_per_km`.
- The path segment length within each airspace is computed exactly by projecting the perpendicular distance onto the flight vector using Euclidean geometry on the real `coord_x`/`coord_y` coordinates.
- **No hardcoded overlays** â€” all overflight fees are computed from the 2D coordinates using vector math.

**Step 3: Overflight fee settlement.**

`overflight_fee = distance_km Ă— overflight_rate_per_km`

- Debit the transport/aviation company (the one producing `FreightCapacity` via "Air Cargo" production method).
- Credit the overflown country's Treasury.
- This is a transit fee, NOT a trade tariff â€” the cargo itself is not taxed.

**Step 4: Integration with freight procurement.**

In `procure_freight_and_split_trades()`:
- If both buyer and seller regions have `has_airport = true` AND an air-cargo producer has capacity â†’ use air routing.
- Otherwise â†’ use ground/maritime routing (current behavior).
- Air routing is faster (lower time cost) but more expensive (higher fuel cost + overflight fees).

**Files to create/modify:**
- `economy/logistics/air_cargo.rs` (new) â€” `compute_air_route()`, `overflight_fees_for_route()` using 2D vector math
- `economy/logistics/mod.rs` â€” register `air_cargo` module
- `economy/logistics/logistics.rs` â€” `procure_freight_and_split_trades()` gains air-route branch
- `society/geography.rs` â€” add `coord_x`, `coord_y` to `Region`; update all constructors
- `engine/turn.rs` â€” pass region coordinates to air cargo routing
- `engine/generator/` â€” populate `coord_x`, `coord_y` during world generation

### Implementation Steps (Part 2)

1. Add `territorial_owner: Option<String>` to `Edge` in `geography.rs` (serde-defaulted for save compatibility).
2. Add `coord_x: f64` and `coord_y: f64` to `Region` in `geography.rs` (serde-defaulted, skip-serializing-if zero for save compactness). Update all `Region` constructors (mock_for_tests, test_builder, generator, save_manager, rebellions).
3. Write a coordinate migration function: for any region with (0.0, 0.0), assign coordinates using a deterministic spring-layout algorithm on the graph topology (iterative relaxation of edge distances). Call this on first load after deserialization.
4. Populate `territorial_owner` during world generation: SeaLane edges adjacent to a country's coastline regions get `territorial_owner = Some(country)`.
5. Populate `coord_x`/`coord_y` during world generation for all regions (land, sea, ocean).
6. Update `compute_freight_route()` to store path edges (route reconstruction) for territorial-owner checks.
7. Add blockade check in `compute_freight_route()`: if a SeaLane edge's `territorial_owner` has an embargo against the trading pair, the edge is impassable.
8. Add maritime transit tariff calculation in `procure_freight_and_split_trades()`: for each territorial SeaLane edge in the path, debit buyer and credit owner Treasury.
9. Create `economy/logistics/air_cargo.rs` with `compute_air_route()` (2D vector math) and `overflight_fees_for_route()` (perpendicular distance projection).
10. Integrate air cargo into `procure_freight_and_split_trades()` as an alternative routing mode when both endpoints have airports.
11. Add unit tests for blockades, transit tariffs, 2D vector math, and overflight fees.

---

## PART 3: Gas Stations (Highway Retail)

### Current State

**Buildings:** `entities/mod.rs` `Building` struct has `region_id: String` â€” buildings are anchored to a Region (node), not to a NetworkLink (edge). There is no `link_id` or edge-anchoring field.

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\entities\mod.rs" /> lines 960â€“1060 (`Building`).

**B2C Retail:** `economy/trade/retail.rs` handles consumer demand and store offers. `CommercialBuilding` in `society/housing.rs` has a `RetailProfile` with `StoreProfile` enum (Grocery, Butcher, Bakery, Clothing, Household, Electronics, Luxury, CarDealer). There is no `GasStation` profile.

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\society\housing.rs" /> lines 137â€“167 (`CommercialBuildingType`), lines 219â€“236 (`StoreProfile`).

**Consumer demand:** `data/consumption_registry.rs` defines `NeedTier` (Subsistence/Standard/Luxury) with commodity demand per capita. `Commodity::Fuels` and `Commodity::RefinedFuel` are NOT in the consumer demand registry â€” fuel is only consumed as a production input by factories and transport companies.

**File:** <ref_file file="C:\Users\netse\Downloads\SillyElaborateState\state\src\data\consumption_registry.rs" />

### Architectural Evaluation

**Option A: Anchor Gas Stations to NetworkLink (Edge).**
- Add a `link_id: Option<String>` field to `Building`.
- Gas Stations are placed "on the route" between two regions.
- They sell fuel to passing logistics companies during route traversal.

**Problems with Option A:**
- Breaks the invariant that all buildings belong to a Region (for labor, tax, demographics, B2C clearing).
- The B2C retail system (`clear_b2c_markets`) iterates buildings by `region_id` â€” edge-anchored buildings would be invisible to consumer retail.
- The production system (labor allocation, wage clearing) is region-based.
- Requires deep refactoring of every system that touches buildings.

**Option B: Gas Stations as Region-anchored B2C buildings (RECOMMENDED).**
- Gas Stations are `Building` entries with `sector = Sector::LocalServices` (or a new `Sector::Retail`), located inside Regions.
- They are a new `CommercialBuildingType::GasStation` and/or a new `StoreProfile::GasStation`.
- They buy `Commodity::Fuels` / `Commodity::RefinedFuel` via B2B orders from refineries.
- They sell fuel to:
  1. **B2C consumers** â€” add `Fuels`/`RefinedFuel` to the `consumption_registry` under `NeedTier::Standard` (car owners need fuel).
  2. **Logistics companies** â€” during freight procurement, transport companies buy fuel from the nearest Gas Station in the route's region.

**Why Option B is better:**
- No architectural changes to the Building struct or the B2C/B2B systems.
- Gas Stations participate in the existing labor market, tax system, and retail clearing.
- They can be placed in any region â€” regions with Highway connections or high freight traffic are natural locations.
- The corporate AI can build Gas Stations via the existing `ConstructionTender` system.
- Fuel demand from consumers and logistics companies flows through the existing B2B/B2C market, creating a realistic fuel supply chain.

### Proposed Design (Option B)

#### 3.1 New Building Type

Add `GasStation` to `CommercialBuildingType`:
```rust
/// Gas station / fuel retail outlet
GasStation,
```

Add `GasStation` to `StoreProfile`:
```rust
/// Gas station (motor fuel retail)
GasStation,
```

#### 3.2 Consumer Fuel Demand

Add `Fuels` and `RefinedFuel` to the `consumption_registry` under `NeedTier::Standard`:
- Per-capita demand depends on car ownership (which is already tracked for B2C `Cars`/`Trucks` purchases).
- Only urban and suburban populations with car ownership consume fuel.
- The demand is proportional to the number of registered vehicles in the region.

#### 3.3 Logistics Fuel Demand (CORRECTION â€” protect direct B2B link)

**Strict rule:** Large logistics/transport companies buy fuel *wholesale*. They MUST retain the ability to buy `Commodity::Fuels` directly from Refineries/Wholesalers on the B2B OrderBook. The direct Refineryâ†’Transport B2B link must NOT be severed.

Gas Stations serve two roles:
1. **Primary: B2C retail** â€” sell fuel to consumers (citizens with cars) via the existing B2C retail clearing system.
2. **Secondary: Localized B2B fallback** â€” transport companies can optionally buy from a nearby Gas Station if no refinery is accessible on the B2B market, but this is NOT mandatory.

The fuel supply chain is:
- **Direct path (unchanged):** Refinery â†’ Transport Company (B2B OrderBook) â€” this is the primary wholesale channel.
- **Retail path (new):** Refinery â†’ Gas Station (B2B) â†’ Consumer (B2C) â€” this is the retail channel for citizens.
- **Fallback path (new):** Gas Station â†’ Transport Company (B2B) â€” only used when no refinery is available.

This ensures that on Turn 1 (when no Gas Stations exist yet), transport companies can still buy fuel directly from refineries and global logistics will not freeze.

#### 3.4 Gas Station Construction

Gas Stations are built via the existing `ConstructionTender` system:
- The corporate AI identifies regions with high freight traffic (sum of `FreightCapacity` consumed) and high car ownership.
- It publishes a construction tender for a `GasStation` building.
- Construction companies bid and build it.
- On completion, the Gas Station becomes a `CommercialBuilding` with `RetailProfile.profiles = {GasStation}`.

#### 3.5 Strategic Placement Heuristic

The corporate AI should preferentially build Gas Stations in:
- Regions that are endpoints of Highways or RailNetwork links (high traffic).
- Regions with high `FreightCapacity` consumption (logistics hubs).
- Regions with high car ownership (urban centers).

### Implementation Steps (Part 3)

1. Add `GasStation` to `CommercialBuildingType` and `StoreProfile` in `society/housing.rs`.
2. Add `Fuels` and `RefinedFuel` to `consumption_registry.rs` under `NeedTier::Standard` (proportional to car ownership).
3. Update `retail.rs` `generate_store_offers()` to generate fuel offers from Gas Station buildings.
4. Update `retail.rs` `clear_b2c_markets()` to clear fuel purchases by consumers.
5. **Do NOT sever the direct Refineryâ†’Transport B2B link.** Transport companies continue buying `Fuels` directly from refineries via B2B orders (current behavior unchanged). Gas Stations are an additional retail channel, not a mandatory intermediary.
6. Gas Stations submit B2B buy orders for `Fuels`/`RefinedFuel` from refineries (they are fuel distributors for the retail market).
7. Add corporate AI logic in `corporate/strategy.rs` for Gas Station construction decisions (target high car-ownership regions).
8. Add a `GasStation` construction project type in the construction system.
9. Add unit tests for Gas Station B2C fuel sales to consumers.

---

## Files to Modify (Summary)

| File | Change |
|------|--------|
| `economy/logistics/logistics.rs` | Multi-modal edge weight, sea-node fix, route reconstruction, territorial blockade check, maritime transit tariff, air-cargo branch |
| `economy/logistics/transport_networks.rs` | `fuel_consumption_per_km()` on `NetworkLevel`, optional congestion field |
| `economy/logistics/air_cargo.rs` (new) | Air cargo routing, overflight fee calculation |
| `economy/logistics/mod.rs` | Register `air_cargo` module |
| `society/geography.rs` | `territorial_owner` field on `Edge`; `coord_x`/`coord_y` on `Region`; update all constructors |
| `society/housing.rs` | `GasStation` to `CommercialBuildingType` and `StoreProfile` |
| `data/consumption_registry.rs` | `Fuels`/`RefinedFuel` consumer demand |
| `economy/trade/retail.rs` | Gas Station store offers and B2C fuel clearing |
| `corporate/strategy.rs` | Gas Station construction AI |
| `engine/turn.rs` | Pass fuel prices, diplomacy, company_country to freight routing; air-cargo integration; congestion decay |
| `engine/generator/` | Populate `territorial_owner` on SeaLane edges; populate `coord_x`/`coord_y` on all regions during world generation |

---

## Verification Plan

- `cargo build --lib` â€” must compile with no errors
- `cargo test --lib` â€” all existing tests must pass
- New unit tests:
  - Multi-hop sea route through intermediate sea node
  - Mode-specific fuel cost in edge weight
  - Congestion buildup from traffic and decay per turn
  - Territorial waters blockade makes route impassable
  - Maritime transit tariff debited to owner Treasury
  - 2D vector math: perpendicular distance from region to flight path
  - Air cargo route with overflight fees computed from coordinates
  - Region coordinate migration assigns non-zero coords to legacy saves
  - Gas Station B2C fuel sale to consumers
  - Direct Refineryâ†’Transport B2B link still works (no Gas Station required)
  - Gas Station B2B fuel purchase from refinery (distributor role)
- Simulation checks:
  - Freight routes use sea lanes when cheaper than land
  - Blockaded country's trade drops to zero
  - Overflight fees appear in Treasury revenue
  - Gas Stations sell fuel to consumers with cars
  - Transport companies successfully buy fuel wholesale directly from Refineries via the B2B order book (primary channel, no Gas Station required)

---

## Risks & Considerations

1. **Save compatibility:** Adding `territorial_owner` to `Edge` and `coord_x`/`coord_y` to `Region` requires `#[serde(default)]` on all three fields. Old saves will deserialize with `None`/0.0/0.0 â€” territorial waters and coordinates will need to be assigned on first load via a migration step.

2. **Dijkstra performance:** Route reconstruction (storing path edges) increases memory per route lookup. For <100 regions this is negligible. If the region count grows, consider a bidirectional Dijkstra or A* with landmarks.

3. **Air cargo precision:** The airspace-overflight model uses exact Euclidean geometry on the real `coord_x`/`coord_y` coordinates added in this phase. The perpendicular distance from each region's center to the flight path vector determines which airspaces are overflown. This is not an approximation â€” it is exact 2D spatial math.

4. **Fuel demand calibration:** Adding `Fuels` to consumer demand must be carefully calibrated â€” too high demand will cause fuel shortages that crash the economy. Start with small per-capita demand proportional to car ownership.

5. **Deferred from Phase 29:** International transit tariffs were deferred because the trade model was abstract. Phase 30 makes routing physical, which enables territorial-waters transit fees. However, `balance_global_trade()` remains abstract â€” the territorial-waters fees are charged during B2B freight procurement, not during the abstract trade balance.

6. **Congestion modeling** adds a `congestion: f64` field to `NetworkLink` and a decay/update step in the turn engine. The congestion must be reset or decayed each turn to prevent permanent gridlock. The decay rate should be configurable.

7. **B2B fuel supply chain (CORRECTED):** The direct Refineryâ†’Transport B2B link is preserved. Gas Stations are NOT mandatory for transport company fuel procurement. This prevents Turn-1 logistics freeze when no Gas Stations exist yet. Gas Stations primarily serve B2C consumers and act as an optional localized fallback for transport companies.

8. **Region coordinate migration:** Adding `coord_x`/`coord_y` to `Region` requires `#[serde(default)]` on both fields. Old saves will deserialize to (0.0, 0.0). A migration step must assign coordinates on first load â€” either via a deterministic spring-layout algorithm on the graph topology, or via a hash-based placement. All regions with (0.0, 0.0) after migration should be flagged for coordinate assignment.
