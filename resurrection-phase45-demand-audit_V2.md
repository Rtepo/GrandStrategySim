# Phase 45 V2 — The Demand Awakening: Physical Military Units, Capital Reserves, Holistic Commodity Audit & Dynamic Pricing

A physically-grounded, mathematically-sound rewrite of Phase 45 that mandates physical `MilitaryUnit` entities with Table of Equipment (ToE) procurement, capital-goods depreciation for fixed assets, a systematic audit of all 140 commodities, and concrete Rust code projections with exact structs, signatures, and formulas.

## Summary

Phase 45 V2 fixes the Demand Desert by (1) spawning physical standing armies/navies at genesis with ToE-driven procurement, (2) extending fixed-asset degradation to Trains/Ships/Submarines and issuing replacement orders based on condition deficits, (3) auditing all 140 commodities to ensure every active commodity has a valid producer→consumer path, (4) routing a single global `HashSet<String>` through all VIP generation, (5) implementing wealth-tiered, era-aware B2C demand with cultural/religious modifiers, and (6) implementing dynamic pricing feedback loops with concrete math.

---

## Part 1: Physical Military Units & ToE Procurement

### 1.1 Current State

`MilitaryUnit` (`military/units.rs:177-219`) already exists with:
- `unit_type: UnitType` (Infantry, Tanks, Artillery, AirForce, Naval, PeasantBattalion)
- `manpower: i64`
- `stockpile: HashMap<Commodity, f64>` (field supply)
- `equipment_quality: f64`
- `calculate_commodity_upkeep()` — returns per-turn consumption rates

**Problem:** `military_units` is always `Vec::new()` at genesis (`generator/mod.rs:252`). No units are ever spawned. Therefore `submit_defense_b2b_orders` (`military/upkeep.rs:117`) generates zero orders, and the entire ArmamentsIndustry has no customers.

**Problem:** `UnitType::commodity_upkeep()` (`units.rs:118-158`) only returns Ammunition/Fuels/Steel — it does NOT include Rifles, Clothing (Uniforms), TowedArtillery, or Submarines. There is no concept of a Table of Equipment (ToE) — the initial equipment a unit needs to be combat-ready.

### 1.2 New Struct: `EquipmentReserve`

Add to `MilitaryUnit` a field tracking the unit's installed equipment reserves and their condition:

```rust
// In military/units.rs

/// A single equipment reserve entry for a military unit.
/// Represents the unit's installed capital equipment (rifles, uniforms, artillery pieces, vessels).
/// Equipment degrades over time and must be replaced via B2B procurement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EquipmentReserve {
    /// The commodity type of this equipment (Rifles, Clothing, TowedArtillery, Submarines, etc.)
    pub commodity: Commodity,
    /// Target quantity per the unit's Table of Equipment (ToE).
    /// e.g., an Infantry Division with 10,000 soldiers has toe_rifles = 10,000.
    pub toe_quantity: f64,
    /// Currently installed quantity (may be < toe_quantity due to losses/wear).
    pub current_quantity: f64,
    /// Average condition in [0.0, 1.0]. Degrades by `depreciation_rate` per turn.
    pub condition: f64,
    /// Per-turn depreciation rate (fraction of condition lost each turn).
    /// Rifles: 0.01 (durable), Uniforms: 0.05 (wear fast), Artillery: 0.02.
    pub depreciation_rate: f64,
}

impl EquipmentReserve {
    /// Compute the replacement demand: quantity needed to restore to ToE,
    /// plus quantity needed to restore condition to 1.0.
    ///
    /// replacement_demand = (toe_quantity - current_quantity) + current_quantity * (1.0 - condition)
    pub fn replacement_demand(&self) -> f64 {
        let quantity_deficit = (self.toe_quantity - self.current_quantity).max(0.0);
        let condition_deficit = self.current_quantity * (1.0 - self.condition);
        quantity_deficit + condition_deficit
    }

    /// Degrade condition by one turn.
    pub fn degrade(&mut self) {
        self.condition = (self.condition - self.depreciation_rate).max(0.0);
        // If condition reaches 0, the equipment is scrapped (quantity drops).
        if self.condition <= 0.0 {
            self.current_quantity = 0.0;
        }
    }

    /// Install new equipment (from B2B deliveries). Restores quantity and condition.
    pub fn install(&mut self, quantity: f64) {
        self.current_quantity += quantity;
        // New equipment arrives at condition 1.0; blend with existing.
        let total = self.current_quantity;
        if total > 0.0 {
            self.condition = (self.condition * (total - quantity) + 1.0 * quantity) / total;
        }
    }
}
```

### 1.3 Modified `MilitaryUnit` Struct

```rust
// Add to MilitaryUnit in military/units.rs:

pub struct MilitaryUnit {
    // ... existing fields ...
    pub id: String,
    pub unit_type: UnitType,
    pub stats: UnitStats,
    pub manpower: i64,
    pub manpower_origin: HashMap<RuralClass, i64>,
    pub home_region: String,
    pub location: String,
    pub experience: f64,
    pub equipment_quality: f64,
    pub stockpile: HashMap<Commodity, f64>,

    /// Phase 45: Table of Equipment (ToE) — the unit's installed capital equipment.
    /// Each entry tracks target quantity, current quantity, and condition.
    /// B2B procurement orders are generated to fill replacement_demand().
    #[serde(default)]
    pub equipment_reserves: Vec<EquipmentReserve>,
}
```

### 1.4 ToE Definitions per Unit Type

```rust
// In military/units.rs — add to UnitType impl:

impl UnitType {
    /// Returns the Table of Equipment (ToE) for this unit type.
    /// Defines the capital equipment a full-strength unit requires.
    /// Quantities are per 1000 soldiers (scaled by manpower at spawn).
    ///
    /// Era gating: `year` determines which equipment is available.
    ///   year < 1916: Infantry gets Rifles + Clothing + TowedArtillery only.
    ///   year >= 1916: Infantry gets Rifles + Clothing + TowedArtillery + LightTanks.
    ///   year >= 1935: Naval gets Submarines.
    ///   year >= 1940: AirForce gets Fighters + Bombers.
    ///   year >= 1960: AirForce gets Helicopters; Tanks gets MediumTanks/HeavyTanks.
    pub fn table_of_equipment(&self, year: u32) -> Vec<EquipmentReserve> {
        let scale = 1.0; // Per 1000 soldiers; caller multiplies by manpower/1000
        match self {
            UnitType::Infantry => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Rifles,
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.01,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Clothing, // Uniforms
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 5000.0 * scale, // Combat reserve
                        current_quantity: 5000.0 * scale,
                        condition: 1.0,
                        depreciation_rate: 0.0, // Ammo doesn't degrade
                    },
                ];
                if year >= 1880 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::TowedArtillery,
                        toe_quantity: 20.0 * scale,
                        current_quantity: 20.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    });
                }
                toe
            }
            UnitType::Artillery => {
                vec![
                    EquipmentReserve {
                        commodity: Commodity::TowedArtillery,
                        toe_quantity: 100.0 * scale,
                        current_quantity: 100.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 20000.0 * scale,
                        current_quantity: 20000.0 * scale,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                ]
            }
            UnitType::Tanks => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 15000.0 * scale,
                        current_quantity: 15000.0 * scale,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1916 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::LightTanks,
                        toe_quantity: 50.0 * scale,
                        current_quantity: 50.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                if year >= 1935 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::MediumTanks,
                        toe_quantity: 30.0 * scale,
                        current_quantity: 30.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                if year >= 1942 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::HeavyTanks,
                        toe_quantity: 15.0 * scale,
                        current_quantity: 15.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.03,
                    });
                }
                toe
            }
            UnitType::AirForce => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 10000.0 * scale,
                        current_quantity: 10000.0 * scale,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1940 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Fighters,
                        toe_quantity: 20.0 * scale,
                        current_quantity: 20.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Bombers,
                        toe_quantity: 10.0 * scale,
                        current_quantity: 10.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                }
                if year >= 1960 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Helicopters,
                        toe_quantity: 15.0 * scale,
                        current_quantity: 15.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.04,
                    });
                }
                toe
            }
            UnitType::Naval => {
                let mut toe = vec![
                    EquipmentReserve {
                        commodity: Commodity::Clothing,
                        toe_quantity: 1000.0 * scale,
                        current_quantity: 1000.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.05,
                    },
                    EquipmentReserve {
                        commodity: Commodity::Ammunition,
                        toe_quantity: 20000.0 * scale,
                        current_quantity: 20000.0 * scale,
                        condition: 1.0,
                        depreciation_rate: 0.0,
                    },
                ];
                if year >= 1935 {
                    toe.push(EquipmentReserve {
                        commodity: Commodity::Submarines,
                        toe_quantity: 5.0 * scale,
                        current_quantity: 5.0 * scale,
                        condition: 0.9,
                        depreciation_rate: 0.02,
                    });
                }
                toe
            }
            UnitType::PeasantBattalion => {
                vec![] // No equipment — peasant militias are unarmed
            }
        }
    }
}
```

### 1.5 Genesis Army Spawning

```rust
// In engine/generator/mod.rs — add after country creation:

/// Phase 45: Spawn a standing army for a country based on its population, budget, and era.
///
/// # Rules
/// * Army size = max(1000, population * 0.005) — 0.5% of population under arms.
/// * If country is democratic and year < 1940: 1 Infantry Division, no Tanks/AirForce/Naval.
/// * If year >= 1940: add 1 Tank Brigade and 1 Air Wing.
/// * If country has coastline (any region with is_coastal): add 1 Naval Fleet.
/// * Equipment is seeded at condition 0.9 (not 1.0 — represents existing stock).
/// * Manpower is drawn proportionally from rural classes (FreePeasant, LandlessLaborer).
pub fn spawn_standing_army(
    country: &Country,
    regions: &[Region],
    start_year: u32,
    rng: &mut impl Rng,
) -> Vec<MilitaryUnit> {
    let total_pop: i64 = regions.iter()
        .map(|r| r.class_demographics.total_population())
        .sum();
    let army_size = (total_pop as f64 * 0.005).max(1000.0) as i64;

    // Determine unit composition based on era and geography
    let has_coast = regions.iter().any(|r| r.is_coastal);
    let is_autocratic = !country.politics.government_form.is_democratic();

    let mut units = Vec::new();

    // Infantry Division (always present)
    let infantry_manpower = army_size / 2;
    let manpower_origin = draw_manpower_from_rural_classes(regions, infantry_manpower);
    let home_region = regions.first().map(|r| r.id.clone()).unwrap_or_default();
    let mut infantry = MilitaryUnit::new(
        format!("{}-INF-1", country.id),
        UnitType::Infantry,
        infantry_manpower,
        manpower_origin,
        home_region.clone(),
    );
    infantry.equipment_reserves = UnitType::Infantry
        .table_of_equipment(start_year)
        .into_iter()
        .map(|mut r| {
            r.toe_quantity *= infantry_manpower as f64 / 1000.0;
            r.current_quantity = r.toe_quantity * 0.9; // Start at 90% strength
            r
        })
        .collect();
    units.push(infantry);

    // Artillery Brigade (if year >= 1880)
    if start_year >= 1880 {
        let arty_manpower = army_size / 10;
        let manpower_origin = draw_manpower_from_rural_classes(regions, arty_manpower);
        let mut artillery = MilitaryUnit::new(
            format!("{}-ART-1", country.id),
            UnitType::Artillery,
            arty_manpower,
            manpower_origin,
            home_region.clone(),
        );
        artillery.equipment_reserves = UnitType::Artillery
            .table_of_equipment(start_year)
            .into_iter()
            .map(|mut r| {
                r.toe_quantity *= arty_manpower as f64 / 1000.0;
                r.current_quantity = r.toe_quantity * 0.9;
                r
            })
            .collect();
        units.push(artillery);
    }

    // Tank Brigade (if year >= 1916)
    if start_year >= 1916 {
        let tank_manpower = army_size / 20;
        let manpower_origin = draw_manpower_from_rural_classes(regions, tank_manpower);
        let mut tanks = MilitaryUnit::new(
            format!("{}-TNK-1", country.id),
            UnitType::Tanks,
            tank_manpower,
            manpower_origin,
            home_region.clone(),
        );
        tanks.equipment_reserves = UnitType::Tanks
            .table_of_equipment(start_year)
            .into_iter()
            .map(|mut r| {
                r.toe_quantity *= tank_manpower as f64 / 1000.0;
                r.current_quantity = r.toe_quantity * 0.9;
                r
            })
            .collect();
        units.push(tanks);
    }

    // Air Wing (if year >= 1940)
    if start_year >= 1940 {
        let air_manpower = army_size / 50;
        let manpower_origin = draw_manpower_from_rural_classes(regions, air_manpower);
        let mut air = MilitaryUnit::new(
            format!("{}-AIR-1", country.id),
            UnitType::AirForce,
            air_manpower,
            manpower_origin,
            home_region.clone(),
        );
        air.equipment_reserves = UnitType::AirForce
            .table_of_equipment(start_year)
            .into_iter()
            .map(|mut r| {
                r.toe_quantity *= air_manpower as f64 / 1000.0;
                r.current_quantity = r.toe_quantity * 0.9;
                r
            })
            .collect();
        units.push(air);
    }

    // Naval Fleet (if coastal and year >= 1880)
    if has_coast && start_year >= 1880 {
        let naval_manpower = army_size / 20;
        let coastal_region = regions.iter().find(|r| r.is_coastal).map(|r| r.id.clone()).unwrap_or(home_region);
        let manpower_origin = draw_manpower_from_rural_classes(regions, naval_manpower);
        let mut naval = MilitaryUnit::new(
            format!("{}-NAV-1", country.id),
            UnitType::Naval,
            naval_manpower,
            manpower_origin,
            coastal_region,
        );
        naval.equipment_reserves = UnitType::Naval
            .table_of_equipment(start_year)
            .into_iter()
            .map(|mut r| {
                r.toe_quantity *= naval_manpower as f64 / 1000.0;
                r.current_quantity = r.toe_quantity * 0.9;
                r
            })
            .collect();
        units.push(naval);
    }

    units
}
```

### 1.6 ToE-Driven Procurement (replaces magic-formula procurement)

```rust
// In military/upkeep.rs — REPLACE submit_defense_b2b_orders with:

/// Phase 45: Submit B2B buy orders for military equipment based on ToE deficits.
///
/// For each military unit, iterate its `equipment_reserves`. For each reserve,
/// compute `replacement_demand()` = (toe - current) + current * (1 - condition).
/// Aggregate demand across all units, then submit B2B bids capped by available cash.
///
/// # Arguments
/// * `units` - All military units for this country
/// * `available_cash` - Cash available to the Ministry of Defense
/// * `market_prices` - Current market prices per commodity (for limit price)
///
/// # Returns
/// Vec of Bid orders to store in pending_defense_orders
pub fn submit_defense_b2b_orders_toe(
    units: &[MilitaryUnit],
    available_cash: f64,
    market_prices: &HashMap<Commodity, f64>,
) -> Vec<Bid> {
    // 1. Aggregate replacement demand across all units
    let mut total_demand: HashMap<Commodity, f64> = HashMap::new();
    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }
        for reserve in &unit.equipment_reserves {
            let demand = reserve.replacement_demand();
            if demand > 0.0 {
                *total_demand.entry(reserve.commodity).or_insert(0.0) += demand;
            }
        }
        // Also include per-turn upkeep (Food, Fuels) from existing commodity_upkeep()
        let upkeep = unit.calculate_commodity_upkeep();
        for (commodity, qty) in upkeep {
            *total_demand.entry(commodity).or_insert(0.0) += qty;
        }
    }

    // 2. Compute total cost at market price * 1.2 (20% procurement premium)
    let mut bids = Vec::new();
    let mut total_cost = 0.0;
    for (commodity, quantity) in &total_demand {
        let base_price = market_prices.get(commodity).copied().unwrap_or(10.0);
        let limit_price = base_price * 1.2;
        total_cost += quantity * limit_price;
    }

    // 3. Scale down proportionally if over budget (double-entry constraint)
    let scale = if total_cost > available_cash && total_cost > 0.0 {
        available_cash / total_cost
    } else {
        1.0
    };

    // 4. Generate bids
    for (commodity, quantity) in &total_demand {
        let base_price = market_prices.get(commodity).copied().unwrap_or(10.0);
        let limit_price = base_price * 1.2;
        let scaled_quantity = quantity * scale;
        if scaled_quantity > 0.0 {
            bids.push(Bid {
                buyer_id: "MIN-DEF".to_string(),
                commodity: *commodity,
                quantity: scaled_quantity,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
        }
    }

    bids
}
```

### 1.7 Equipment Degradation Per Turn

```rust
// In military/upkeep.rs — new function:

/// Phase 45: Degrade all military equipment reserves by one turn.
/// Called at the start of each turn, BEFORE procurement orders are generated.
/// This ensures that the ToE deficit grows naturally over time, driving
/// recurring procurement demand.
pub fn degrade_military_equipment(units: &mut [MilitaryUnit]) {
    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }
        for reserve in &mut unit.equipment_reserves {
            reserve.degrade();
        }
    }
}
```

### 1.8 Equipment Delivery from B2B Trades

```rust
// In military/upkeep.rs — modify deliver_military_supplies to also install equipment:

/// Phase 45: Deliver military supplies AND equipment from B2B trades.
/// Scans executed trades for buyer_id == "MIN-DEF" and credits:
///   - Upkeep commodities (Food, Fuels, Ammo) → unit.stockpile
///   - Equipment commodities (Rifles, Clothing, Tanks, etc.) → unit.equipment_reserves
pub fn deliver_military_supplies_and_equipment(
    trades: &[Trade],
    units: &mut [MilitaryUnit],
) -> HashMap<Commodity, f64> {
    let mut delivered = HashMap::new();
    for trade in trades.iter().filter(|t| t.buyer_id == "MIN-DEF") {
        *delivered.entry(trade.commodity).or_insert(0.0) += trade.quantity;
    }

    // Distribute to units proportionally by manpower
    let total_manpower: i64 = units.iter()
        .filter(|u| !u.is_peasant_battalion())
        .map(|u| u.manpower)
        .sum();

    for unit in units {
        if unit.is_peasant_battalion() || total_manpower == 0 {
            continue;
        }
        let unit_share = unit.manpower as f64 / total_manpower as f64;
        for reserve in &mut unit.equipment_reserves {
            if let Some(&qty) = delivered.get(&reserve.commodity) {
                let install_qty = qty * unit_share;
                reserve.install(install_qty);
            }
        }
        // Also refill stockpile for upkeep commodities
        for (commodity, &qty) in &delivered {
            if !unit.equipment_reserves.iter().any(|r| r.commodity == *commodity) {
                *unit.stockpile.entry(*commodity).or_insert(0.0) += qty * unit_share;
            }
        }
    }

    delivered
}
```

---

## Part 2: Capital Reserves & Depreciation (Fixed Assets)

### 2.1 Current State

`FixedAssetCohort` (`economy/production/fixed_assets.rs:46-65`) already tracks:
- `commodity: Commodity` (IndustrialMachinery, ConstructionMachinery, AgriculturalMachinery, OfficeMachinery, Trucks, Cars, DraftAnimals)
- `count: f64`
- `condition: f64` (0.0 to 1.0)
- `durability: f64` (turns to fully degrade)

`degrade_cohorts()` (`fixed_assets.rs:159-181`) already degrades cohorts per turn.
`submit_fixed_asset_purchase_bids()` (`b2b_orders.rs:1354-1431`) already submits B2B buy bids for fixed-asset commodities based on production method inputs.

**Problem:** `Trains`, `Submarines`, `Frigates`, `CargoShips`, and other capital goods are NOT in the `is_fixed_asset()` list (`enums.rs:645-656`). They are treated as regular consumables, which means they vanish per-turn instead of being installed as durable cohorts.

**Problem:** `submit_fixed_asset_purchase_bids` generates demand based on `method.inputs` — but if no production method lists `Trains` as a fixed-asset input, no demand is generated. The current logistics production methods produce `FreightCapacity` but don't consume `Trains` as a fixed asset.

### 2.2 Extension of `is_fixed_asset()`

```rust
// In registries/enums.rs — modify is_fixed_asset():

pub fn is_fixed_asset(&self) -> bool {
    matches!(
        self,
        Commodity::IndustrialMachinery
            | Commodity::ConstructionMachinery
            | Commodity::AgriculturalMachinery
            | Commodity::OfficeMachinery
            | Commodity::Trucks
            | Commodity::Cars
            | Commodity::DraftAnimals
            // Phase 45: Capital goods that should be installed as cohorts:
            | Commodity::Trains       // Rail logistics capital
            | Commodity::Submarines   // Naval capital (also in military ToE)
            | Commodity::LightTanks   // Military capital (also in military ToE)
            | Commodity::MediumTanks
            | Commodity::HeavyTanks
            | Commodity::Fighters
            | Commodity::Bombers
            | Commodity::Helicopters
    )
}
```

**Note:** Military equipment (Tanks, Fighters, Submarines) is handled via the `EquipmentReserve` system in Part 1, NOT via `FixedAssetCohort`. The `is_fixed_asset()` extension is for B2B market filtering — when the MoD buys Submarines via B2B, the trade settlement must recognize them as capital goods, not consumables. The `deliver_military_supplies_and_equipment` function (Part 1.8) routes them to `EquipmentReserve`, not `FixedAssetCohort`.

### 2.3 Replacement Order Logic for Fixed Assets

The existing `submit_fixed_asset_purchase_bids` generates demand from `method.inputs` — but it doesn't account for degradation. A company should issue replacement orders when its existing cohorts degrade.

```rust
// In economy/trade/b2b_orders.rs — ADD to submit_fixed_asset_purchase_bids:

// Phase 45: After generating demand from method.inputs, ALSO generate
// replacement demand from degraded cohorts.
//
// For each building's fixed_assets, compute the condition deficit:
//   replacement_demand = sum(count * (1.0 - condition)) per commodity
// This represents the quantity of new machinery needed to restore
// the building's production capacity to full.
//
// The replacement bid is submitted at the same limit price as new asset bids,
// and is capped by the same cash encumbrance limit.

for building in buildings.iter().filter(|b| b.owner_id == company.id) {
    // Aggregate condition deficit by commodity
    let mut replacement_needed: HashMap<Commodity, f64> = HashMap::new();
    for cohort in &building.fixed_assets {
        if cohort.is_scrapped() {
            continue;
        }
        let deficit = cohort.count * (1.0 - cohort.condition);
        if deficit > 0.0 {
            *replacement_needed.entry(cohort.commodity).or_insert(0.0) += deficit;
        }
    }

    // Submit replacement bids
    for (&commodity, &qty) in &replacement_needed {
        if qty <= 0.0 {
            continue;
        }
        let ref_price = match get_reference_price(&commodity, market_history) {
            Some(p) => p,
            None => continue,
        };
        let remaining_encumbrance = (max_encumber - total_encumbered).max(0.0);
        if remaining_encumbrance <= 0.0 {
            break;
        }
        let desired_wtp = ref_price * gen_config.asset_quality_wtp_multiplier;
        let affordable_wtp = remaining_encumbrance / qty;
        let limit_price = desired_wtp.min(affordable_wtp);
        if limit_price < ref_price * gen_config.asset_purchase_starvation_ratio {
            continue;
        }
        let encumbrance = qty * limit_price;
        company.available_cash -= encumbrance;
        company.debit_cash += encumbrance;
        total_encumbered += encumbrance;
        order_book.bids
            .entry(commodity)
            .or_insert_with(Vec::new)
            .push(Bid {
                buyer_id: company.id.clone(),
                commodity,
                quantity: qty,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
    }
}
```

### 2.4 Trains as Fixed Assets for Logistics

Add `Trains` as a fixed-asset input to logistics production methods:

```rust
// In registries/production_methods_data.rs — modify logistics methods:

// "Steam Freight Trains" currently:
//   pm(1885, Some("steam_002"), 0.10, 0.25, 0.65, 2.5,
//      &[(Commodity::Fuels, 15.0), (Commodity::Steel, 5.0)],
//      &[(Commodity::FreightCapacity, 40.0)]));

// Phase 45: Add Trains as a fixed-asset input. Since is_fixed_asset() now
// returns true for Trains, the B2B order submission will route Trains to
// submit_fixed_asset_purchase_bids instead of per-turn consumption.
// The quantity represents the number of train sets needed per 1000 FTE.
m.insert(MethodSlot::Production, "Steam Freight Trains".into(),
    pm(1885, Some("steam_002"), 0.10, 0.25, 0.65, 2.5,
       &[(Commodity::Fuels, 15.0), (Commodity::Steel, 5.0), (Commodity::Trains, 2.0)],
       &[(Commodity::FreightCapacity, 40.0)]));

m.insert(MethodSlot::Production, "Diesel Freight Trains".into(),
    pm(1930, Some("auto_002"), 0.15, 0.30, 0.55, 3.5,
       &[(Commodity::Fuels, 12.0), (Commodity::MechanicalComponents, 5.0), (Commodity::Trains, 2.0)],
       &[(Commodity::FreightCapacity, 60.0)]));
```

### 2.5 DraftAnimals as Fixed Assets for Early Agriculture

```rust
// In registries/production_methods_data.rs — modify agriculture methods:

// "Manual Farming" currently uses Seeds + Food.
// Phase 45: Add DraftAnimals as a fixed-asset input for pre-tractor farming.
// Since is_fixed_asset() returns true for DraftAnimals, the B2B system will
// route this to submit_fixed_asset_purchase_bids, installing draft animal cohorts.
m.insert(MethodSlot::Production, "Manual Farming".into(),
    pm(1880, None, 0.02, 0.10, 0.88, 1.0,
       &[(Commodity::Seeds, 5.0), (Commodity::Food, 3.0), (Commodity::DraftAnimals, 3.0)],
       &[(Commodity::Cereal, 15.0)]));

// "Horse-Drawn Machinery" — replace Livestock with DraftAnimals
m.insert(MethodSlot::Production, "Horse-Drawn Machinery".into(),
    pm(1885, Some("mech_002"), 0.05, 0.15, 0.80, 1.5,
       &[(Commodity::Seeds, 5.0), (Commodity::Food, 5.0), (Commodity::DraftAnimals, 5.0)],
       &[(Commodity::Cereal, 25.0)]));
```

### 2.6 Genesis Seeding of Trains and DraftAnimals

```rust
// In engine/generator/corporate.rs — modify seed_fixed_assets():

fn seed_fixed_assets(sector: Sector, start_year: u32, rng: &mut impl Rng) -> Vec<FixedAssetCohort> {
    let mut cohorts = Vec::new();

    // Primary machinery (existing logic)
    let machinery_commodity = match sector {
        Sector::HeavyIndustry => Commodity::IndustrialMachinery,
        Sector::Construction => Commodity::ConstructionMachinery,
        Sector::Agriculture => {
            // Phase 45: Pre-tractor agriculture uses DraftAnimals, not AgriculturalMachinery
            if start_year < 1920 {
                Commodity::DraftAnimals
            } else {
                Commodity::AgriculturalMachinery
            }
        }
        Sector::PublicServices | Sector::PublicAdministration | Sector::Banking => Commodity::OfficeMachinery,
        Sector::TransportLogistics | Sector::ExportServices => {
            // Phase 45: Pre-truck logistics uses DraftAnimals; rail era uses Trains
            if start_year < 1900 {
                Commodity::DraftAnimals
            } else if start_year < 1930 {
                Commodity::Trains
            } else {
                Commodity::Trucks
            }
        }
        _ => Commodity::IndustrialMachinery,
    };

    let count = match sector {
        Sector::HeavyIndustry | Sector::Mining | Sector::Energy => 8.0,
        Sector::Construction | Sector::Agriculture => 5.0,
        Sector::LightIndustry | Sector::ArmamentsIndustry => 5.0,
        Sector::TransportLogistics => 4.0,
        _ => 3.0,
    };

    cohorts.push(FixedAssetCohort {
        blueprint_id: "legacy_seed".to_string(),
        commodity: machinery_commodity,
        count,
        condition: 0.7 + rng.gen::<f64>() * 0.3,
        quality: 0.8,
        durability: 240.0,
        base_tech: "legacy".to_string(),
        base_tech_year: start_year.saturating_sub(20),
        acquired_turn: 0,
    });

    cohorts
}
```

---

## Part 3: Holistic 140-Commodity Audit

### 3.1 Audit Results

The Python audit script (`audit_commodities.py`) identified the following orphaned commodities:

**Active commodities with a producer but NO consumer (orphaned supply):**

| Commodity | Producer | Required Consumer |
|-----------|----------|-------------------|
| Asphalt | 1900: Asphalt Production | Construction BOM (roads) |
| Bombers | 1942: Bomber Production | Military ToE (AirForce) |
| BrownCoal | 1880: Brown Coal Mining | Energy production input |
| Coke | 1880: Coke Production | Steel production input |
| Fighters | 1940: Fighter Production | Military ToE (AirForce) |
| HeavyTanks | 1942: Heavy Tank Production | Military ToE (Tanks) |
| Helicopters | 1960: Helicopter Production | Military ToE (AirForce) |
| Hydrogen | 1970: Hydrogen Production | Chemical industry input |
| LightTanks | 1916: Tank Production | Military ToE (Tanks) |
| Magnesium | 1900: Magnesium Refinery | Alloy production input |
| MediumTanks | 1935: Medium Tank Production | Military ToE (Tanks) |
| Peat | 1880: Peat Cutting | Energy production input |
| Prefabricates | 1900: Prefabricates Plant | Construction BOM (1975+ era) |
| RefinedFuel | 1920: Advanced Refining | Transport input (replaces Fuels) |
| Rifles | 1920: Small Arms Automation | Military ToE (Infantry) |
| Silver | 1890: Silver Mining | Luxury goods input |
| Submarines | 1935: Submarine Production | Military ToE (Naval) |
| SupportEquipment | 1965: Guided Munitions | Military ToE (Infantry/Artillery) |
| TowedArtillery | 1880: Artillery Workshop | Military ToE (Infantry/Artillery) |
| Trains | 1890: Locomotive Works | Logistics fixed asset |
| Uranium | 1945: Uranium Mining | Energy production input |
| Zinc | 1890: Zinc Ore Mining | Alloy/chemical input |

**Active commodities with NO producer and NO consumer:**

| Commodity | Fix |
|-----------|-----|
| Fish | Add fishing production method (or use existing `process_fishing_turn`) |
| ReligiousArt | Add to religious economy production |
| ReligiousTexts | Add to religious economy production |

### 3.2 Fix Plan for Each Orphaned Commodity

#### Military commodities (fixed by Part 1 ToE):
- **Rifles, TowedArtillery, Submarines, LightTanks, MediumTanks, HeavyTanks, Fighters, Bombers, Helicopters, SupportEquipment** → All consumed by `EquipmentReserve` in military units spawned at genesis.

#### Construction commodities:
- **Asphalt** → Add to `bom_road()` and `bom_paved_road()` in construction BOMs.
- **Prefabricates** → Add to 1975+ era construction BOMs (see Part 4).

#### Industrial input commodities:
- **BrownCoal** → Add as input to "Steam Power" energy production methods.
- **Coke** → Add as input to "Bessemer Converters" and "Open Hearth Furnace" steel production.
- **Hydrogen** → Add as input to "Advanced Chemicals" production (1970+).
- **Magnesium** → Add as input to "Aluminum Alloy" production (1900+).
- **Peat** → Add as input to "Peat Power Plant" energy production.
- **RefinedFuel** → Add as input to "Diesel Freight Trains" and "Truck Transport" (replaces Fuels for advanced transport).
- **Silver** → Add as input to "Luxury Goods" production.
- **Uranium** → Add as input to "Nuclear Power Plant" energy production (1945+).
- **Zinc** → Add as input to "Brass Production" and "Galvanizing" (chemical industry).

#### Religious commodities:
- **Fish** → The `process_fishing_turn` function already exists in `economy/state_sector/fishing.rs`. Fish is produced by fishing boats, not by production methods. Verify that fishing boats are spawned at genesis.
- **ReligiousArt, ReligiousTexts** → Add production methods to `PublicServices` or a religious economy sector. These should be produced by religious institutions (churches, monasteries) and consumed by citizens via B2C demand (religious needs).

### 3.3 Verification: No Commodity Left Orphaned

A new test will be added to verify that every active commodity has at least one consumer path:

```rust
// In registries/production_methods.rs — add test:

#[test]
fn test_no_active_commodity_is_orphaned() {
    let active_commodities: Vec<Commodity> = Commodity::all()
        .iter()
        .copied()
        .filter(|c| c.is_active())
        .collect();

    // Collect all consumers: production method inputs, construction BOMs,
    // consumption registry, military ToE, fixed asset seeding.
    let mut all_consumers: HashSet<Commodity> = HashSet::new();

    // 1. Production method inputs
    let methods = state_building_methods();
    for (_, building_methods) in methods.iter() {
        for pm in building_methods.iter_all() {
            for c in pm.inputs.keys() {
                all_consumers.insert(*c);
            }
        }
    }

    // 2. Construction BOMs (all sectors)
    for sector in [Sector::HeavyIndustry, Sector::LightIndustry, Sector::Mining, Sector::Agriculture, Sector::Construction, Sector::Energy, Sector::PublicServices, Sector::PublicAdministration, Sector::Banking, Sector::ArmamentsIndustry, Sector::TransportLogistics, Sector::Retail, Sector::Hospitality] {
        let bom = get_construction_bom(sector, 2000);
        for c in bom.keys() {
            all_consumers.insert(*c);
        }
    }

    // 3. Consumption registry (B2C)
    let registry = consumption_registry();
    for basket in registry.values() {
        for tier_commodities in basket.tiers.values() {
            for c in tier_commodities.keys() {
                all_consumers.insert(*c);
            }
        }
    }

    // 4. Military ToE
    for unit_type in &[UnitType::Infantry, UnitType::Artillery, UnitType::Tanks, UnitType::AirForce, UnitType::Naval] {
        for reserve in unit_type.table_of_equipment(2000) {
            all_consumers.insert(reserve.commodity);
        }
    }

    // 5. Fixed asset commodities (seeded at genesis, replaced via degradation)
    for c in &[Commodity::IndustrialMachinery, Commodity::ConstructionMachinery, Commodity::AgriculturalMachinery, Commodity::OfficeMachinery, Commodity::Trucks, Commodity::DraftAnimals, Commodity::Trains] {
        all_consumers.insert(*c);
    }

    // 6. Service commodities (consumed by special systems)
    let services = [Commodity::Food, Commodity::Water, Commodity::Energy, Commodity::Heat,
        Commodity::FreightCapacity, Commodity::HealthCapacity, Commodity::EducationSlots,
        Commodity::JusticeCapacity, Commodity::SecurityCapacity, Commodity::IntelligenceCapacity,
        Commodity::FireProtectionCapacity, Commodity::ShelterCapacity,
        Commodity::BorderEnforcementCapacity, Commodity::CustomsCapacity,
        Commodity::SanitaryInspectionCapacity, Commodity::BuildingInspectionCapacity,
        Commodity::EnvironmentalInspectionCapacity, Commodity::LaborInspectionCapacity,
        Commodity::AssimilationCapacity, Commodity::PassengerTransport, Commodity::Information,
        Commodity::InnovationPoints, Commodity::AdministrativeServices, Commodity::BankingServices,
        Commodity::ConstructionServices, Commodity::MaintenanceServices,
        Commodity::LocalServicesCommodity, Commodity::RenovationServices,
        Commodity::InsuranceServices, Commodity::MarketResearch];
    for c in &services {
        all_consumers.insert(*c);
    }

    // Check: every active commodity must be in all_consumers
    let orphaned: Vec<Commodity> = active_commodities
        .iter()
        .filter(|c| !all_consumers.contains(c))
        .copied()
        .collect();

    assert!(orphaned.is_empty(),
        "Phase 45: Orphaned commodities with no consumer path: {:?}",
        orphaned);
}
```

---

## Part 4: Era-Aware Construction BOMs

### 4.1 Modified `get_construction_bom` Signature — Sector Enum Dispatch (NO STRINGS)

**CORRECTION:** The previous V2 draft used string matching (`.contains("steel")`). This is REJECTED.
The function MUST accept a `Sector` enum and dispatch via `match sector { ... }`. No string parsing.

```rust
// In construction/bom.rs:

use crate::registries::enums::Sector;

/// Phase 45: Returns the construction BOM for a sector, era-aware.
///
/// # Arguments
/// * `sector` - The Sector enum variant (HeavyIndustry, Agriculture, Mining, etc.)
/// * `start_year` - Year of construction start (determines material mix)
///
/// # Rules
/// * Dispatch is via `match sector { ... }` — NO string matching.
/// * year <= 1925: BOMs use Bricks, Timber, Planks (traditional construction)
/// * year >= 1950: BOMs shift to Cement, Steel, Prefabricates (modern construction)
/// * 1925 < year < 1950: Transitional mix (both materials)
pub fn get_construction_bom(sector: Sector, start_year: u32) -> BTreeMap<Commodity, f64> {
    let era_factor = if start_year <= 1925 {
        0.0 // Traditional
    } else if start_year >= 1950 {
        1.0 // Modern
    } else {
        (start_year - 1925) as f64 / 25.0 // Transitional blend
    };

    // Strict enum dispatch — no string matching, no .contains(), no .to_lowercase()
    match sector {
        Sector::HeavyIndustry => bom_heavy_factory(era_factor),
        Sector::LightIndustry => bom_light_factory(era_factor),
        Sector::Mining => bom_mine(era_factor),
        Sector::Agriculture => bom_farm(era_factor),
        Sector::Construction => bom_warehouse(era_factor), // Construction yards use warehouse-like BOM
        Sector::Energy => bom_heavy_factory(era_factor),   // Power plants are heavy industrial
        Sector::TransportLogistics => bom_warehouse(era_factor),
        Sector::PublicServices => bom_commercial(era_factor),
        Sector::PublicAdministration => bom_commercial(era_factor),
        Sector::Banking => bom_commercial(era_factor),
        Sector::ArmamentsIndustry => bom_heavy_factory(era_factor),
        Sector::MaintenanceWorkshops => bom_light_factory(era_factor),
        Sector::Retail => bom_commercial(era_factor),
        Sector::Hospitality => bom_commercial(era_factor),
        Sector::ExportServices => bom_commercial(era_factor),
        // Fallback for any future sectors
        _ => bom_light_factory(era_factor),
    }
}

/// Era-aware heavy factory BOM.
/// era_factor = 0.0 (1900) → Bricks/Timber/Planks dominant
/// era_factor = 1.0 (1975) → Cement/Steel/Prefabricates dominant
fn bom_heavy_factory(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    // Steel: always needed, but more in modern era
    bom.insert(Commodity::Steel, 200.0 + 200.0 * era);
    // Cement: minimal in 1900, dominant in 1975
    bom.insert(Commodity::Cement, 100.0 + 300.0 * era);
    // Bricks: dominant in 1900, minimal in 1975
    bom.insert(Commodity::Bricks, 400.0 * (1.0 - era) + 50.0);
    // Timber: dominant in 1900, minimal in 1975
    bom.insert(Commodity::Timber, 300.0 * (1.0 - era) + 50.0);
    // Planks: only in 1900 era
    bom.insert(Commodity::Planks, 150.0 * (1.0 - era));
    // Prefabricates: only in 1975 era
    bom.insert(Commodity::Prefabricates, 200.0 * era);
    // Construction machinery: always needed
    bom.insert(Commodity::ConstructionMachinery, 20.0);
    // Glass: always needed
    bom.insert(Commodity::Glass, 50.0);
    bom
}

/// Era-aware light factory BOM.
fn bom_light_factory(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 100.0 + 100.0 * era);
    bom.insert(Commodity::Cement, 50.0 + 200.0 * era);
    bom.insert(Commodity::Bricks, 300.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Timber, 200.0 * (1.0 - era) + 30.0);
    bom.insert(Commodity::Planks, 100.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 100.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 10.0);
    bom.insert(Commodity::Glass, 30.0);
    bom
}

// Similar era-aware modifications for bom_mine, bom_farm, bom_warehouse, bom_commercial, etc.
```

### 4.2 Call Site Updates

All callers of `get_construction_bom` must pass `Sector` and `start_year`:
- `construction/tender_market.rs:52` — `get_construction_bom(sector, start_year)` (must look up sector from the tender's target building)
- `construction/bom.rs:119` — `get_construction_bom(sector, start_year)` in `get_expansion_bom` (signature also changes to accept `Sector`)

The `start_year` is available from `Country.macro_indicators.current_year` or the turn number.

---

## Part 5: Global VIP HashSet Routing

### 5.1 Current State

5 separate `HashSet<String>` instances exist:
1. `ministries.rs:421` — coalition branch of `form_government`
2. `ministries.rs:471` — single-party branch of `form_government`
3. `parliament.rs:321` — `speaker_names` in `initialize_parliament`
4. `parliament.rs:534` — `used_names` in `build_vips`
5. `ministries.rs:427` — `leader_used` (separate set in same function)

`generate_deputy_speakers` (`parliament.rs:492-522`) uses NO HashSet.

### 5.2 Implementation

```rust
// In politics/turn.rs — process_political_year:

pub fn process_political_year(
    country: &mut Country,
    companies: &mut Vec<crate::entities::Company>,
    unions: &mut [crate::entities::Union],
    year: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Phase 45: Single global VIP deduplication set.
    // Created ONCE at political-year scope, passed through ALL VIP generation.
    // Pre-populated with all party leader names.
    let mut used_vip_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for party in country.politics.active_parties.values() {
        if !party.leader.name.is_empty() {
            used_vip_names.insert(party.leader.name.clone());
        }
    }

    // ... existing logic ...

    // When form_government is called:
    let new_config = super::ministries::form_government(
        country,
        &country.politics.coalition,
        &active_parties,
        current_turn,
        &mut used_vip_names,  // Phase 45: pass global set
    );

    // When initialize_parliament is called:
    let parliament = super::parliament::initialize_parliament(
        &country.politics,
        cultural_group,
        current_turn,
        &mut rng,
        &mut used_vip_names,  // Phase 45: pass global set
    );

    // ... rest of function ...
}
```

### 5.3 Modified Function Signatures

```rust
// politics/ministries.rs:
pub fn form_government(
    country: &Country,
    coalition: &[String],
    active_parties: &HashMap<String, Party>,
    current_turn: u32,
    used_names: &mut HashSet<String>,  // Phase 45: global set
) -> MinistryConfig {
    // Remove all local HashSet creation.
    // Use the passed-in used_names for all VIP generation.
    // ...
}

// politics/parliament.rs:
pub fn initialize_parliament(
    politics: &Politics,
    cultural_group: &str,
    current_turn: u32,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,  // Phase 45: global set
) -> Parliament {
    // Remove local speaker_names and build_vips' local used_names.
    // Pass used_names to generate_speaker, generate_deputy_speakers, build_vips.
    // ...
}

fn generate_deputy_speakers(
    politics: &Politics,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,  // Phase 45: now accepts the set
) -> Vec<NamedVip> {
    // Generate unique VIPs for deputy speakers instead of cloning party leaders.
    politics.coalition.iter().take(2).map(|party_id| {
        let vip = generate_unique_vip(cultural_group, rng, used_names);
        // ...
    }).collect()
}

fn build_vips(
    politics: &Politics,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,  // Phase 45: use global set
) -> Vec<NamedVip> {
    // Remove local used_names creation. Use the passed-in set.
    // ...
}
```

---

## Part 6: Wealth-Tiered, Era-Aware B2C Demand

### 6.1 Current State

`build_consumer_demand` (`retail.rs:218-262`) multiplies per-capita needs by population with NO wealth/savings gating, NO era-awareness, and NO cultural/religious modifier application.

`CulturalDemandModifier` (`retail.rs:71-150`) already exists with `from_definitions()` and `apply()` methods, but is never called from `build_consumer_demand`.

`ClassDemographics` (`geography.rs:770`) has:
- `savings: f64` — total class savings
- `savings_per_capita: f64` — average per-capita savings
- `debt: f64` — outstanding consumer debt
- `religion: String` — religion practiced by this class

### 6.2 Modified `build_consumer_demand`

```rust
// In economy/trade/retail.rs:

pub fn build_consumer_demand(
    region: &Region,
    current_turn: u32,
    start_year: u32,                                    // Phase 45: era parameter
    culture: &CultureDefinition,                        // Phase 45: cultural modifiers
    religion: &ReligionDefinition,                      // Phase 45: religious modifiers
    religious_authority: f64,                           // Phase 45: authority scaling
) -> ConsumerDemand {
    let mut demand = ConsumerDemand {
        demand: BTreeMap::new(),
        total_demand: BTreeMap::new(),
    };

    let consumption = consumption_registry();
    let cultural_modifier = CulturalDemandModifier::from_definitions(culture, religion, religious_authority);

    // Era gating: which commodities are available in this era?
    let era_unlocked = |commodity: &Commodity| -> bool {
        match commodity {
            // Pre-1925: no consumer electronics, no cars for masses
            Commodity::Televisions => start_year >= 1935,
            Commodity::Agd => start_year >= 1930,
            Commodity::Cars => start_year >= 1920,  // Only for wealthy classes
            Commodity::Radio => start_year >= 1900,
            // Luxury electronics
            Commodity::OfficeMachinery => start_year >= 1900, // Typewriters
            _ => true,  // All other commodities are era-agnostic
        }
    };

    // Process rural classes
    for (class_id, demographics) in &region.class_demographics.rural_classes {
        if let Some(basket) = consumption.get(class_id) {
            let mut class_demand: BTreeMap<Commodity, f64> = BTreeMap::new();

            // Compute wealth tier for this class
            let per_capita_savings = demographics.savings_per_capita;
            let wealth_tier = if per_capita_savings > 5000.0 {
                3 // Luxury
            } else if per_capita_savings > 1000.0 {
                2 // Standard
            } else if per_capita_savings > 100.0 {
                1 // Basic standard
            } else {
                0 // Subsistence only
            };

            for (tier, tier_commodities) in &basket.tiers {
                // Wealth gating: skip tiers the class can't afford
                let tier_affordable = match tier {
                    NeedTier::Subsistence => true,  // Always
                    NeedTier::Standard => wealth_tier >= 1,
                    NeedTier::Luxury => wealth_tier >= 3,
                };
                if !tier_affordable {
                    continue;
                }

                // Budget share scaling: demand is scaled by tier_budget_share
                let budget_share = basket.tier_budget_share.get(tier).copied().unwrap_or(0.0);
                if budget_share <= 0.0 {
                    continue;
                }

                for (commodity, per_capita) in tier_commodities {
                    // Era gating
                    if !era_unlocked(commodity) {
                        continue;
                    }

                    // Wealth scaling: richer classes consume more
                    let wealth_multiplier = match tier {
                        NeedTier::Subsistence => 1.0,
                        NeedTier::Standard => {
                            // Scale from 0.5x at wealth_tier=1 to 1.5x at wealth_tier=3
                            0.5 + (wealth_tier as f64 - 1.0) * 0.5
                        }
                        NeedTier::Luxury => {
                            // Luxury demand scales aggressively with wealth
                            per_capita_savings / 5000.0
                        }
                    };

                    let class_demand_qty = per_capita * (demographics.population as f64) * budget_share * wealth_multiplier;
                    *class_demand.entry(*commodity).or_insert(0.0) += class_demand_qty;
                }
            }

            // Apply cultural/religious modifiers
            cultural_modifier.apply(&mut class_demand);

            // Merge into total demand
            let key = (region.id.clone(), DemographyType::Rural, class_id.clone());
            for (commodity, qty) in class_demand {
                *demand.demand.entry(key.clone()).or_insert_with(BTreeMap::new)
                    .entry(commodity).or_insert(0.0) += qty;
                *demand.total_demand.entry(commodity).or_insert(0.0) += qty;
            }
        }
    }

    // Same logic for urban classes (duplicate the loop with urban_classes)
    for (class_id, demographics) in &region.class_demographics.urban_classes {
        // ... identical logic ...
    }

    demand
}
```

### 6.3 Call Site Update

The caller of `build_consumer_demand` in `turn.rs` must pass `start_year`, `culture`, `religion`, and `religious_authority`. These are available from:
- `country.macro_indicators.current_year` (or derive from turn number)
- `country.macro_indicators.culture` → look up `CultureDefinition` from `culture_registry()`
- `country.macro_indicators.religion` → look up `ReligionDefinition` from `religion_registry()`
- Religious authority is tracked in the religious economy module.

---

## Part 7: Dynamic Pricing — Concrete Math

### 7.1 Seller Feedback (Unsold Inventory → Lower Ask)

**State storage:** Use `Company.extra` JSON map to avoid struct changes:

```rust
// Key in company.extra: "phase45_unsold" → JSON { "CommodityName": quantity }
// Key in company.extra: "phase45_unfilled" → JSON { "CommodityName": quantity }

// Helper functions to read/write from extra:
fn get_unsold_quantities(company: &Company) -> HashMap<Commodity, f64> {
    company.extra.get("phase45_unsold")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn set_unsold_quantities(company: &mut Company, quantities: &HashMap<Commodity, f64>) {
    if let Ok(val) = serde_json::to_value(quantities) {
        company.extra.insert("phase45_unsold".to_string(), val);
    }
}

fn get_unfilled_demand(company: &Company) -> HashMap<Commodity, f64> {
    company.extra.get("phase45_unfilled")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn set_unfilled_demand(company: &mut Company, quantities: &HashMap<Commodity, f64>) {
    if let Ok(val) = serde_json::to_value(quantities) {
        company.extra.insert("phase45_unfilled".to_string(), val);
    }
}
```

### 7.2 Collecting Unfilled Orders After Matching

```rust
// In economy/market/order_book.rs — add after match_orders:

/// Phase 45: Collect unfilled asks (unsold inventory) per seller.
///
/// After match_orders runs, remaining asks in the order book represent
/// inventory that the seller could not sell at their asking price.
/// This is stored as feedback for next turn's pricing.
///
/// # Returns
/// HashMap<seller_id, HashMap<Commodity, unfilled_quantity>>
pub fn collect_unfilled_asks(order_book: &OrderBook) -> HashMap<String, HashMap<Commodity, f64>> {
    let mut unfilled: HashMap<String, HashMap<Commodity, f64>> = HashMap::new();
    for (commodity, asks) in &order_book.asks {
        for ask in asks {
            *unfilled.entry(ask.seller_id.clone())
                .or_insert_with(HashMap::new)
                .entry(*commodity)
                .or_insert(0.0) += ask.quantity;
        }
    }
    unfilled
}

/// Phase 45: Collect unfilled bids (unmet demand) per buyer.
pub fn collect_unfilled_bids(order_book: &OrderBook) -> HashMap<String, HashMap<Commodity, f64>> {
    let mut unfilled: HashMap<String, HashMap<Commodity, f64>> = HashMap::new();
    for (commodity, bids) in &order_book.bids {
        for bid in bids {
            *unfilled.entry(bid.buyer_id.clone())
                .or_insert_with(HashMap::new)
                .entry(*commodity)
                .or_insert(0.0) += bid.quantity;
        }
    }
    unfilled
}
```

### 7.3 Seller Price Adjustment Formula

In `submit_company_b2b_orders`, when computing the sell ask limit price:

```rust
// Phase 45: Dynamic seller pricing feedback.
//
// If the company had unsold inventory last turn, lower the ask price.
//
// Formula:
//   adjustment = -SELLER_ADJUSTMENT_RATE * (unsold_qty / (unsold_qty + sold_qty))
//   adjusted_ask = base_ask * (1.0 + adjustment)
//   floor = unit_cost * 1.01  (never sell below cost + 1%)
//   final_ask = max(adjusted_ask, floor)
//
// Where:
//   SELLER_ADJUSTMENT_RATE = 0.10 (10% max reduction per turn)
//   unsold_qty = company's unsold quantity of this commodity from last turn
//   sold_qty = company's sold quantity of this commodity from last turn
//   base_ask = unit_cost * (1 + dynamic_markup)  (existing logic)

const SELLER_ADJUSTMENT_RATE: f64 = 0.10;

let unsold = get_unsold_quantities(company);
let unfilled_qty = unsold.get(&commodity).copied().unwrap_or(0.0);

// Estimate sold quantity from inventory change (simplified: use 1.0 as denominator floor)
let sold_qty = 1.0; // Placeholder — actual sold qty tracked via trade settlement

let seller_adjustment = if unfilled_qty > 0.0 {
    -SELLER_ADJUSTMENT_RATE * (unfilled_qty / (unfilled_qty + sold_qty)).min(1.0)
} else {
    0.0
};

let base_ask = unit_cost * (1.0 + dynamic_markup);
let adjusted_ask = base_ask * (1.0 + seller_adjustment);
let seller_floor = unit_cost * 1.01;  // Never sell below cost + 1%
let final_ask = adjusted_ask.max(seller_floor);
```

### 7.4 Buyer Price Adjustment Formula — Cash-Only Ceiling (No Profitability Cap)

**CORRECTION:** A profitability ceiling (e.g., `max_bid <= expected_output_price * output_qty / input_qty`) was considered and REJECTED. It is mathematically fragile because multiple inputs feed into multiple outputs and expected prices are volatile. The ONLY ceiling is `max_affordable_budget`. If a company bids 5x base price for Coal due to shortage and blows its cash reserves, its `unit_cost` skyrockets, forcing it to raise its own asking price or face bankruptcy. That is organic capitalism.

In `submit_company_b2b_orders`, when computing the buy bid limit price:

```rust
// Phase 45: Dynamic buyer pricing feedback.
//
// If the company had unfilled demand last turn, raise the bid price.
//
// Formula:
//   adjustment = +BUYER_ADJUSTMENT_RATE * (unfilled_qty / (unfilled_qty + filled_qty))
//   adjusted_bid = base_bid * (1.0 + adjustment)
//   ceiling = max_affordable_budget = available_cash * max_cash_encumbrance_ratio / quantity
//   final_bid = min(adjusted_bid, ceiling)
//
// Where:
//   BUYER_ADJUSTMENT_RATE = 0.10 (10% max increase per turn)
//   unfilled_qty = company's unfilled demand from last turn
//   filled_qty = quantity actually purchased last turn

const BUYER_ADJUSTMENT_RATE: f64 = 0.10;

let unfilled = get_unfilled_demand(company);
let unfilled_qty = unfilled.get(&commodity).copied().unwrap_or(0.0);

let buyer_adjustment = if unfilled_qty > 0.0 {
    BUYER_ADJUSTMENT_RATE * (unfilled_qty / (unfilled_qty + 1.0)).min(1.0)
} else {
    0.0
};

let base_bid = reference_price * (1.0 + config.buy_premium_ratio);
let adjusted_bid = base_bid * (1.0 + buyer_adjustment);
let max_affordable = (company.available_cash * config.max_cash_encumbrance_ratio) / desired_qty;
let final_bid = adjusted_bid.min(max_affordable);
```

### 7.5 VWAP Organic Shift

```rust
// In economy/market/market_history.rs — modify update_vwap:

pub fn update_vwap(history: &mut MarketHistory, trades: &[Trade]) {
    // ... existing VWAP calculation from trades ...

    // Phase 45: REMOVE the base-price seeding loop.
    // Do NOT seed VWAP from global_base_prices for commodities with no trades.
    // This allows VWAP to remain unset for untraded commodities, so the
    // reference price falls back to last_trade_price or global_base_prices
    // via get_reference_price(), without anchoring VWAP to base prices.
    //
    // REMOVED:
    // for (commodity, &base_price) in &history.global_base_prices {
    //     if !history.vwap_per_commodity.contains_key(commodity) {
    //         history.vwap_per_commodity.insert(*commodity, base_price);
    //     }
    // }
    //
    // This means VWAP only reflects ACTUAL executed trades, allowing prices
    // to organically deviate from base prices as the dynamic pricing feedback
    // loops in Parts 7.3 and 7.4 shift bid/ask prices.
}
```

### 7.6 B2C Unmet Demand Tracking

```rust
// In economy/trade/retail.rs — modify clear_b2c_markets:

// After the allocation loop, compute unmet demand per commodity:
for (commodity, commodity_offers) in &by_commodity {
    let total_demand = demand.total_demand.get(commodity).copied().unwrap_or(0.0);
    let total_supply: f64 = commodity_offers.iter().map(|o| o.quantity).sum();
    let total_sold = units_sold.get(commodity).copied().unwrap_or(0.0);
    let unmet = (total_demand - total_sold).max(0.0);

    // Phase 45: Store unmet demand in each store's retail_profile
    if unmet > 0.0 {
        for offer in commodity_offers {
            if let Some(store) = stores.iter_mut().find(|s| s.id == offer.store_id) {
                if let Some(profile) = &mut store.retail_profile {
                    *profile.unmet_demand_last_turn.entry(*commodity).or_insert(0.0) += unmet;
                }
            }
        }
    }
}
```

---

## Implementation Steps (Chronological)

1. **Global VIP HashSet** — Modify `politics/turn.rs`, `politics/ministries.rs`, `politics/parliament.rs`.
2. **Era-Aware Construction BOMs** — Modify `construction/bom.rs`, update callers in `construction/tender_market.rs`.
3. **Production Method Input Fixes** — Modify `registries/production_methods_data.rs` (DraftAnimals, Trains, Bricks, Planks, Coke, BrownCoal, etc.).
4. **Physical Military Units & ToE** — Add `EquipmentReserve` struct, modify `MilitaryUnit`, add `spawn_standing_army`, replace `submit_defense_b2b_orders` with ToE-driven version, add equipment degradation and delivery.
5. **Fixed Asset Extension** — Extend `is_fixed_asset()`, add replacement demand to `submit_fixed_asset_purchase_bids`, modify `seed_fixed_assets` for era-aware Trains/DraftAnimals.
6. **Wealth-Tiered B2C Demand** — Modify `build_consumer_demand` with wealth gating, era gating, and cultural/religious modifiers.
7. **Dynamic Pricing** — Add unfilled order collection, seller/buyer price feedback, remove VWAP base-price seeding, populate B2C unmet demand.
8. **Orphaned Commodity Fixes** — Add missing production method inputs for BrownCoal, Coke, Hydrogen, Magnesium, Peat, RefinedFuel, Silver, Uranium, Zinc. Add Fish/ReligiousArt/ReligiousTexts production.
9. **Build, Test, Verify** — `cargo build`, `cargo test --lib`, verify no orphaned commodities, verify Market Tab shows fluctuating prices.

## Files to Modify

- `state/src/politics/turn.rs` — Global VIP HashSet creation and routing
- `state/src/politics/ministries.rs` — Accept `&mut HashSet<String>` in `form_government`
- `state/src/politics/parliament.rs` — Accept `&mut HashSet<String>` in `initialize_parliament`, `build_vips`, `generate_deputy_speakers`
- `state/src/construction/bom.rs` — Era-aware BOMs with `start_year` parameter
- `state/src/construction/tender_market.rs` — Pass `start_year` to `get_construction_bom`
- `state/src/registries/production_methods_data.rs` — Add DraftAnimals, Trains, Bricks, Planks, Coke, BrownCoal, etc. as inputs
- `state/src/registries/enums.rs` — Extend `is_fixed_asset()` for Trains, military equipment
- `state/src/military/units.rs` — Add `EquipmentReserve` struct, `equipment_reserves` field, `table_of_equipment()` method
- `state/src/military/upkeep.rs` — Replace `submit_defense_b2b_orders` with ToE-driven version, add degradation and delivery functions
- `state/src/military/config.rs` — Add equipment depreciation config if needed
- `state/src/engine/generator/mod.rs` — Call `spawn_standing_army` at genesis
- `state/src/engine/generator/corporate.rs` — Era-aware `seed_fixed_assets` with Trains/DraftAnimals
- `state/src/engine/turn.rs` — Call equipment degradation, ToE procurement, pass `start_year` to B2C demand
- `state/src/economy/trade/retail.rs` — Wealth-tiered demand, cultural modifiers, unmet demand tracking, `start_year` parameter
- `state/src/economy/trade/b2b_orders.rs` — Dynamic pricing feedback, replacement demand for fixed assets
- `state/src/economy/market/order_book.rs` — `collect_unfilled_asks`, `collect_unfilled_bids`
- `state/src/economy/market/market_history.rs` — Remove VWAP base-price seeding
- `state/src/data/consumption_registry.rs` — Verify era-appropriate goods in baskets (may need minor additions)

## Verification

- [ ] `cargo build` succeeds
- [ ] `cargo test --lib -- --test-threads=1` passes (697+ tests)
- [ ] `test_no_active_commodity_is_orphaned` passes — no active commodity lacks a consumer
- [ ] Military units are spawned at genesis with equipment reserves
- [ ] MoD submits B2B orders for Rifles, Clothing, TowedArtillery, Ammunition
- [ ] Market prices deviate from base 100.0 for commodities with imbalanced supply/demand
- [ ] B2C demand for Fruit, Furniture is non-zero for wealthy Aristocracy
- [ ] B2C demand for Televisions/Agd is zero in 1900, non-zero in 1975
- [ ] B2B demand for Bricks is non-zero (construction projects)
- [ ] B2B demand for DraftAnimals is non-zero in 1900 agriculture
- [ ] B2B demand for Trains is non-zero in 1900 logistics
- [ ] ArmamentsIndustry companies have positive revenue from MoD procurement
- [ ] No VIP name appears twice in the same country's government
- [ ] Sellers lower prices when inventory doesn't sell (down to unit_cost * 1.01)
- [ ] Buyers raise prices when demand is unfilled (up to max_affordable_budget)
- [ ] VWAP shifts organically based on executed trades, not anchored to base prices

## Risks/Considerations

- **Save compatibility:** `EquipmentReserve` and `equipment_reserves` field use `#[serde(default)]`. The `extra` map on `Company` is used for dynamic pricing state, requiring no struct changes.
- **`is_fixed_asset()` extension:** Adding Trains and military equipment to `is_fixed_asset()` changes B2B order routing. The `submit_company_b2b_orders` function skips fixed assets (`b2b_orders.rs:212`), routing them to `submit_fixed_asset_purchase_bids`. This is correct for Trains (logistics buildings need them as capital). For military equipment, the MoD's `submit_defense_b2b_orders_toe` generates bids directly (not through the company B2B path), so the `is_fixed_asset()` change doesn't affect MoD procurement.
- **Performance:** Equipment degradation and ToE demand calculation are O(units × equipment_types) per turn — negligible for typical army sizes (5-10 units, 3-5 equipment types each).
- **Genesis army size:** 0.5% of population under arms is historically reasonable for peacetime (19th century average was 0.5-1%). This creates meaningful but not overwhelming procurement demand.
- **BOM caller updates:** Only 2 call sites of `get_construction_bom` need `start_year` added.
- **Pre-existing test failure:** `real_game_state_struct_round_trip` was failing before Phase 45 — not a regression.
