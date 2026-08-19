# Stage A: Mathematical Resuscitation & Loop Logic Diagnostics

Comprehensive codebase inspection to diagnose root causes of 100-turn simulation failures and assess feasibility of proposed market fixes.

## PART 1: Mathematical Black Holes - Root Cause Analysis

### 1. State Reserves Exponential Drain Bug

**Location:** `src/government/treasury.rs` lines 52-62

**Root Cause:** Compound multiplier in Black Ops budget calculation creates unconstrained exponential spending.

**Mechanism:**
```rust
// Line 184 in calculate_black_ops_budget
let official = ctx.country.budget.nominal_budget;
(defense + security) * official * 0.02
```

**The Bug:**
- `nominal_budget` is GDP-based (15-30% of GDP during generation)
- Black Ops budget = (defense_allocation + security_allocation) × nominal_budget × 0.02
- This creates a triple compound: allocation_fraction × GDP × constant
- If GDP grows or allocations are high, Black Ops spending scales quadratically
- Line 61: `ctx.country.budget.liquid_reserves += net_balance` where `net_balance = revenue - total_opex`
- No constraint prevents spending beyond available reserves
- Debt compounds without limit - no debt ceiling or bankruptcy trigger

**Why -1.36e36:**
- Turn 0: State Reserves = +18.9B (initial seed)
- Turn 10: +125.8B (positive but growing unsustainably)
- Turn 20: -1.2T (first negative, exponential decay begins)
- Turn 90: -1.37×10^36 (mathematical collapse)
- The pattern suggests a compound interest effect where each turn's deficit becomes the base for the next calculation

**Missing Safeguards:**
- No debt ceiling in `TaxRates::PublicDebt`
- No sovereign bankruptcy logic
- No spending cap relative to revenue
- No emergency spending freeze when reserves < 0

### 2. Frozen Private Capital Bug

**Location:** `src/corporate/manager.rs` lines 99-104

**Root Cause:** Entity loading failure results in empty company list, so private capital sum is always zero.

**Mechanism:**
```rust
let private_capital: f64 = companies
    .iter()
    .filter(|c| c.state_share < 1.0)
    .map(|c| c.company_capital)
    .sum();
country.budget.private_capital = private_capital;
```

**The Bug:**
- Simulation output shows: "Warning: Could not load company sector sektor_wydobywczy for Eldoria: I/O error: Nie można odnaleźć określonego pliku."
- All 11 sectors per country fail to load
- `companies` vector is empty after loading failures
- Empty vector iteration = 0.0 sum
- Private capital never initialized from generated companies

**Why Files Missing:**
- `generate_corporate_entities` in `src/engine/generator/corporate.rs` creates companies
- Companies are saved to `data_dir/entities/{country}/companies/{sector}.json`
- Test uses temporary directory `C:/Users/netse/Downloads/SillyElaborateState/state/test_simulation_data`
- Generation may save to different path than simulation loads from
- No verification that files exist before attempting load

**Impact:**
- Corporate sector effectively non-existent in simulation
- No corporate tax revenue (companies don't exist to pay)
- No employment from private sector
- Economic loop broken at corporate phase

### 3. Frozen Citizen Savings Bug

**Location:** `src/society/geography.rs` lines 767-770 vs `src/state/treasury.rs` line 204

**Root Cause:** Regional class savings are updated but never aggregated into national `citizen_savings`.

**Mechanism:**
```rust
// Regional level - DOES update (geography.rs:767)
class_demographics.savings += disposable_income;

// National level - NEVER updates (treasury.rs:204)
pub citizen_savings: f64,  // Only set during generation
```

**The Bug:**
- `update_class_demographics` function exists and updates `region.class_demographics.rural_classes[*].savings`
- However, no function aggregates these into `country.budget.citizen_savings`
- The national field is only initialized during world generation (line 422 of generator)
- During simulation turns, regional savings accumulate but national total remains static
- This is a data pipeline break: regional → national aggregation missing

**Why 24.9B Static:**
- Initial seed from generation: `citizen_savings: gdp_total * rng.gen_range(0.05..0.20)`
- Regional savings change each turn but never propagate upward
- Telemetry reads from national field, not regional aggregation
- Creates false impression of capital drain (savings accumulate at regional level but invisible nationally)

**Missing Function:**
```rust
// Should exist but doesn't:
fn aggregate_citizen_savings(country: &mut Country) {
    let total: f64 = country.regions.iter()
        .flat_map(|r| r.class_demographics.rural_classes.values())
        .map(|d| d.savings)
        .sum();
    country.budget.citizen_savings = total;
}
```

### 4. Static Market Imbalances Bug

**Location:** `src/economy/production.rs` lines 48-92 (resolve_active_method) and `src/corporate/strategy.rs` (SwitchMethod action)

**Root Cause:** Production methods are fixed at building creation; no AI logic triggers method switching to adapt to market conditions.

**Mechanism:**
```rust
// production.rs:53-55 - Returns existing method without checking market
if !building.active_method.inputs.is_empty() || !building.active_method.outputs.is_empty() {
    return building.active_method.clone();
}
```

**The Bug:**
- `resolve_active_method` only checks if method exists, not if it's appropriate
- Once a building has a method, it never changes
- `SwitchMethod` action exists in `CorporateAction` enum (strategy.rs:70-73)
- However, no `LegalForm` implementation ever returns `SwitchMethod`
- Corporate AI has no logic to detect persistent deficits and trigger method changes

**Why Identical Imbalances for 100 Turns:**
- Turn 0: WegielKamienny deficit = 286,344.04, Stal surplus = 407,991.30
- Turn 90: Exact same numbers
- Production never adapts to reduce coal demand or increase steel consumption
- Market clearing adjusts prices but not production quantities
- No feedback loop from market signals to production method selection

**Missing AI Logic:**
- No detection of "persistent deficit > X turns"
- No evaluation of alternative production methods for same building
- No cost-benefit analysis of method switching vs. continued losses
- No technology unlock checking for synthetic alternatives (e.g., coal-to-liquids)

## PART 2: Feasibility Blueprint for Market Fixes

### 1. Adaptive Production Methods & Synthetic Tech

**Current State:**
- `ActiveProductionMethod` struct exists with inputs/outputs maps
- `ProductionMethodChoice` in `SectorShare` allows sector-level method selection
- `SwitchMethod` action defined but never used
- `registries.production_methods` contains alternative methods by building type/year

**Feasibility:** HIGH

**Required Changes:**

**A. Data Structure Enhancement:**
```rust
// Add to ActiveProductionMethod:
pub alternative_methods: Vec<AlternativeMethod>,  // Available alternatives
pub technology_requirements: Vec<TechId>,        // Required tech

// New struct:
pub struct AlternativeMethod {
    pub method_id: String,
    pub inputs: BTreeMap<Commodity, f64>,
    pub outputs: BTreeMap<Commodity, f64>,
    pub conversion_efficiency: f64,  // 0.0-1.0 efficiency vs primary
    pub tech_requirement: Option<TechId>,
}
```

**B. AI Logic Addition:**
```rust
// In corporate/strategy.rs, add to each LegalForm::decide():
fn evaluate_method_switch(ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
    // Get current production method from company's buildings
    let current_method = ctx.company.buildings.first()
        .map(|b| &b.active_method)?;
    
    // Find alternative methods from registry for this building type
    let alternatives = find_alternative_methods(&ctx.company.buildings.first()?.name, ctx.year);
    
    for alt_method in alternatives {
        // Calculate projected Gross Margin for current method
        let current_gm = calculate_gross_margin(
            current_method,
            &ctx.market_signal.prices,
            ctx.country.macro_indicators.average_wage,
        );
        
        // Calculate projected Gross Margin for alternative method
        let alt_gm = calculate_gross_margin(
            &alt_method,
            &ctx.market_signal.prices,
            ctx.country.macro_indicators.average_wage,
        );
        
        // Calculate switch cost (equipment, retraining, downtime)
        let switch_cost = calculate_switch_cost(current_method, &alt_method);
        
        // Switch only if alternative is more profitable AND switch cost is justified
        // Threshold: alternative must be at least 15% more profitable to justify disruption
        let profit_improvement = (alt_gm - current_gm) / current_gm.abs().max(1.0);
        
        if alt_gm > current_gm && profit_improvement > 0.15 {
            // Verify payback period is reasonable (within 20 turns)
            let incremental_profit = alt_gm - current_gm;
            let payback_turns = switch_cost / incremental_profit.max(0.01);
            
            if payback_turns <= 20.0 {
                return Some(CorporateAction::SwitchMethod {
                    method: alt_method,
                });
            }
        }
    }
    None
}

/// Calculate projected Gross Margin for a production method
/// Gross Margin = Expected Revenue - Expected Input Costs - Wages
fn calculate_gross_margin(
    method: &ActiveProductionMethod,
    market_prices: &HashMap<Commodity, f64>,
    base_wage: f64,
) -> f64 {
    let wage_multiplier = method.experts_ratio * 3.0 + method.skilled_ratio * 2.0 + method.basic_ratio;
    let wages_per_1k = wage_multiplier * base_wage;
    
    // Calculate input costs using current market prices
    let input_costs: f64 = method.inputs.iter()
        .map(|(commodity, amount_per_1k)| {
            let price = market_prices.get(commodity).copied().unwrap_or(100.0);
            amount_per_1k * price
        })
        .sum();
    
    // Calculate output revenue using current market prices
    let output_revenue: f64 = method.outputs.iter()
        .map(|(commodity, amount_per_1k)| {
            let price = market_prices.get(commodity).copied().unwrap_or(100.0);
            amount_per_1k * price
        })
        .sum();
    
    // Gross margin per 1000 workers
    output_revenue - input_costs - wages_per_1k
}

/// Calculate one-time cost to switch production methods
fn calculate_switch_cost(
    current: &ActiveProductionMethod,
    alternative: &ActiveProductionMethod,
) -> f64 {
    // Equipment replacement cost (simplified: 10% of fixed capital)
    let equipment_cost = 10000.0;  // Placeholder - should reference building.fixed_capital
    
    // Worker retraining cost (based on labor ratio differences)
    let labor_diff = (current.experts_ratio - alternative.experts_ratio).abs()
        + (current.skilled_ratio - alternative.skilled_ratio).abs()
        + (current.basic_ratio - alternative.basic_ratio).abs();
    let retraining_cost = labor_diff * 5000.0;
    
    // Downtime cost (estimated 2 turns of lost production)
    let downtime_cost = 2000.0;  // Placeholder
    
    equipment_cost + retraining_cost + downtime_cost
}
```

**C. Registry Enhancement:**
- Add synthetic production methods to `production_methods.json`
- Examples: "CoalToLiquids" (Wegiel → PaliwoSyntetyczne), "BiomassEthanol" (Biomasa → Etanol)
- Tag methods with `synthetic: true` and `efficiency_penalty: 0.7`

**Integration Points:**
- `process_building_cycle`: Check for method switch before production
- `CorporateStrategy::decide`: Add method switch evaluation before expansion
- `registries`: Load alternative methods during initialization

**Estimated Effort:** 2-3 days

### 2. State Interventions (VAT/Excise & Rationing)

**Current State:**
- `TaxRates::vat` exists with per-category rates
- `TradePolicy` has import tariffs but no consumption taxes
- No rationing system exists
- No emergency economic powers framework

**Feasibility:** MEDIUM

**Required Changes:**

**A. Data Structure Enhancement:**
```rust
// Add to TaxRates:
pub excise_taxes: HashMap<Commodity, ExciseTax>,  // Commodity-specific sin taxes

// New struct:
pub struct ExciseTax {
    pub rate: f64,  // 0.0-1.0
    pub reason: ExciseReason,  // Health, Environment, Strategic
}

pub enum ExciseReason {
    Health,  // Tobacco, alcohol
    Environment,  // Carbon-intensive goods
    Strategic,  // Critical inputs during shortage
}

// Add to Country:
pub rationing_system: Option<RationingSystem>,
pub emergency_powers: EmergencyPowers,
```

**B. Rationing System:**
```rust
pub struct RationingSystem {
    pub active: bool,
    pub rationed_goods: HashMap<Commodity, RationingLevel>,
    pub per_capita_limits: HashMap<Commodity, f64>,  // Units per person
    pub enforcement_strictness: f64,  // 0.0-1.0
}

pub enum RationingLevel {
    None,
    Reduced,  // 50% normal consumption
    Critical,  // 25% normal consumption
    Emergency,  // 10% normal consumption
}
```

**C. Policy Logic:**
```rust
// In government/treasury.rs:
pub fn apply_excise_tax(ctx: &mut CountryTurnCtx, good: Commodity, rate: f64) {
    // Apply to cleared market price
    if let Some(price) = ctx.market_prices.get_mut(&good) {
        *price *= (1.0 + rate);
    }
}

pub fn enforce_rationing(country: &mut Country, market_orders: &mut MarketOrders) {
    if let Some(rationing) = &country.rationing_system {
        for (good, limit) in &rationing.per_capita_limits {
            let max_demand = limit * country.budget.population as f64;
            if let Some(order) = market_orders.orders.get_mut(good) {
                order.buy = order.buy.min(max_demand);
            }
        }
        
        // Apply rationing consequences to population wellbeing
        apply_rationing_consequences(country, rationing);
    }
}

/// Apply health and unrest consequences of rationing
/// CRITICAL: Rationing is NOT consequence-free - it directly impacts Stage 3 (Health) and Stage 4 (Rebellion)
fn apply_rationing_consequences(country: &mut Country, rationing: &RationingSystem) {
    for (good, level) in &rationing.rationed_goods {
        // Check if this is an essential good (food, heating fuel, medicine)
        if is_essential_good(*good) {
            match level {
                RationingLevel::Critical => {
                    // 25% normal consumption - significant health impact
                    increase_mortality_from_shortage(country, *good, 0.15);  // +15% mortality
                    increase_social_unrest_from_shortage(country, *good, 20.0);  // +20 unrest
                }
                RationingLevel::Emergency => {
                    // 10% normal consumption - severe health impact
                    increase_mortality_from_shortage(country, *good, 0.35);  // +35% mortality
                    increase_social_unrest_from_shortage(country, *good, 40.0);  // +40 unrest
                    // Emergency rationing on food/heat triggers rebellion risk
                    check_rationing_rebellion_trigger(country, *good);
                }
                RationingLevel::Reduced => {
                    // 50% normal consumption - moderate health impact
                    increase_mortality_from_shortage(country, *good, 0.05);  // +5% mortality
                    increase_social_unrest_from_shortage(country, *good, 10.0);  // +10 unrest
                }
                RationingLevel::None => {
                    // No impact
                }
            }
        }
    }
}

fn is_essential_good(commodity: Commodity) -> bool {
    matches!(commodity, Commodity::Zywnosc | Commodity::WegielKamienny | Commodity::Energia | Commodity::Leki)
}

fn increase_mortality_from_shortage(country: &mut Country, commodity: Commodity, multiplier: f64) {
    // Interface with Stage 3 Health/Mortality system
    // Increase macro_indicators.mortality_rate based on essential good shortage
    let base_mortality = country.macro_indicators.mortality_rate;
    country.macro_indicators.mortality_rate = base_mortality * (1.0 + multiplier);
    
    // For heating fuel shortages, also increase winter mortality via existing utility system
    if matches!(commodity, Commodity::WegielKamienny | Commodity::Energia) {
        // This will interface with UtilityDemand::calculate_winter_mortality()
        // which already handles heating deficit mortality calculations
    }
}

fn increase_social_unrest_from_shortage(country: &mut Country, commodity: Commodity, increase: f64) {
    // Interface with Stage 4 Unrest/Rebellion system
    // Directly increase macro_indicators.social_unrest
    country.macro_indicators.social_unrest = (country.macro_indicators.social_unrest + increase).min(100.0);
    
    // High unrest triggers rebellion checks in politics/rebellions.rs
    // The rebellion system already monitors social_unrest levels
}

fn check_rationing_rebellion_trigger(country: &mut Country, commodity: Commodity) {
    // Emergency rationing of essential goods is a rebellion trigger
    // This interfaces with RebellionTrigger conditions in politics/rebellions.rs
    if country.macro_indicators.social_unrest > 60.0 {
        // Trigger rebellion risk evaluation
        // The rebellion system will check region-by-region conditions
    }
}
```

**D. Trigger Conditions:**
```rust
pub fn check_emergency_conditions(country: &Country) -> EmergencyPowers {
    let deficit_severity = country.budget.liquid_reserves / country.budget.gdp;
    let critical_shortages = count_critical_shortages(country);
    
    if deficit_severity < -0.5 || critical_shortages > 3 {
        EmergencyPowers::RationingEnabled
    } else if deficit_severity < -0.2 {
        EmergencyPowers::ExciseTaxesEnabled
    } else {
        EmergencyPowers::Normal
    }
}
```

**Integration Points:**
- `resolve_market_prices`: Apply excise taxes to cleared prices
- `process_building_cycle`: Respect rationing limits for inputs
- `process_government_spending`: Check emergency conditions each turn
- UI: Add rationing status to economic reports

**Estimated Effort:** 3-4 days

### 3. Physical Warehousing System

**Current State:**
- `CommercialBuilding` struct exists in `src/society/housing.rs` with `CommercialBuildingType::Warehouse` variant
- No storage capacity or inventory tracking integrated with existing warehouse buildings
- Surplus goods are effectively destroyed (no storage cost)
- No logistics companies
- No perishability system
- Stage 6 real estate is fully implemented (housing.rs lines 132-181)

**Feasibility:** MEDIUM (Stage 6 infrastructure exists, needs integration)

**Required Changes:**

**A. Data Structure Enhancement (integrate with existing CommercialBuilding):**
```rust
// Extend existing CommercialBuilding in src/society/housing.rs:
pub struct CommercialBuilding {
    // ... existing fields (id, building_type, micro_region_id, office_capacity, retail_capacity, tenants, rent_per_sqm, utility_connections)
    
    // NEW: Storage-specific fields for Warehouse type
    #[serde(rename = "pojemność_magazynowa", default)]
    pub storage_capacity: HashMap<Commodity, f64>,  // Max units by commodity
    
    #[serde(rename = "aktualny_inwentarz", default)]
    pub current_inventory: HashMap<Commodity, f64>,  // Stored units
    
    #[serde(rename = "typ_przechowywania", default)]
    pub storage_type: StorageType,  // Cold, Dry, Liquid, etc.
}

// New enum for storage specialization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    #[default]
    GeneralWarehouse,  // Standard dry storage
    ColdStorage,      // Refrigerated (food, pharma)
    LiquidTanks,       // Oil, chemicals, liquids
    Hazardous,         // Specialized handling
}
```

**B. Warehousing Logic (integrated with CommercialBuilding):**
```rust
// Extend CommercialBuilding with storage methods:
impl CommercialBuilding {
    /// Calculate dynamic storage fee per unit based on OPEX and utilization
    pub fn calculate_storage_fee(&self, commodity: Commodity, utilization: f64) -> f64 {
        if self.building_type != CommercialBuildingType::Warehouse {
            return 0.0;
        }
        
        // Base fee from utility connections OPEX (heating, electricity, water)
        let utility_opex = self.calculate_utility_opex();
        let base_fee_per_sqm = utility_opex / (self.office_capacity + self.retail_capacity).max(1.0);
        
        // Utilization premium: higher utilization = higher fees (supply/demand)
        let utilization_multiplier = 1.0 + (utilization * 0.5);  // 0% util = 1.0x, 100% util = 1.5x
        
        // Storage type modifier
        let storage_modifier = match self.storage_type {
            StorageType::ColdStorage => 3.0,  // High energy cost
            StorageType::LiquidTanks => 2.0,
            StorageType::Hazardous => 4.0,
            StorageType::GeneralWarehouse => 1.0,
        };
        
        base_fee_per_sqm * utilization_multiplier * storage_modifier
    }
    
    fn calculate_utility_opex(&self) -> f64 {
        // Calculate from utility_connections costs
        // Uses existing UtilityDemand::for_commercial() from utilities/demand.rs
        let demand = UtilityDemand::for_commercial(self, Season::current());
        // Multiply by utility prices from market
        // Returns total OPEX for utilities
        0.0  // Placeholder - integrate with utility pricing
    }
    
    /// Get current utilization rate (0.0-1.0) across all stored commodities
    pub fn utilization_rate(&self) -> f64 {
        let total_capacity: f64 = self.storage_capacity.values().sum();
        let total_stored: f64 = self.current_inventory.values().sum();
        
        if total_capacity > 0.0 {
            total_stored / total_capacity
        } else {
            0.0
        }
    }
}

// Replace surplus destruction with storage using existing CommercialBuilding:
pub fn handle_surplus(
    surplus: f64,
    commodity: Commodity,
    warehouses: &mut Vec<CommercialBuilding>,
    market_prices: &HashMap<Commodity, f64>,
) -> f64 {
    let mut stored = 0.0;
    
    // Sort warehouses by utilization (fill emptier ones first)
    warehouses.sort_by(|a, b| {
        a.utilization_rate().partial_cmp(&b.utilization_rate()).unwrap()
    });
    
    for warehouse in warehouses.iter_mut() {
        if warehouse.building_type != CommercialBuildingType::Warehouse {
            continue;
        }
        
        if let Some(capacity) = warehouse.storage_capacity.get(&commodity) {
            let current = warehouse.current_inventory.get(&commodity).copied().unwrap_or(0.0);
            let available = capacity - current;
            let can_store = available.min(surplus - stored);
            
            if can_store > 0.0 {
                let utilization = warehouse.utilization_rate();
                let fee_per_unit = warehouse.calculate_storage_fee(commodity, utilization);
                let total_fee = can_store * fee_per_unit;
                
                // Logistics Company PROVIDES the service - fee is paid TO them
                // Increase Logistics Company's liquid_capital by total_fee
                // Fee is deducted from market/producer's liquid_capital or added to OPEX
                
                warehouse.current_inventory.insert(commodity, current + can_store);
                stored += can_store;
            }
        }
    }
    
    stored  // Returns amount actually stored, remainder is destroyed
}
```

**C. Perishability System:**
```rust
pub struct PerishabilityRules {
    pub decay_rates: HashMap<Commodity, f64>,  // 0.0-1.0 per turn
    pub storage_modifiers: HashMap<StorageType, f64>,  // Reduces decay
}

// Add method to CommercialBuilding:
impl CommercialBuilding {
    pub fn apply_perishability(&mut self, rules: &PerishabilityRules, region: &mut Region) {
        if self.building_type != CommercialBuildingType::Warehouse {
            return;
        }
        
        for (commodity, quantity) in self.current_inventory.iter_mut() {
            let base_decay = rules.decay_rates.get(commodity).copied().unwrap_or(0.0);
            let storage_modifier = rules.storage_modifiers.get(&self.storage_type)
                .copied().unwrap_or(1.0);
            let actual_decay = base_decay * storage_modifier;
            
            let original_quantity = *quantity;
            *quantity *= (1.0 - actual_decay);
            
            // Decayed goods become waste - route to Stage 6 Landfill system
            if actual_decay > 0.0 {
                let decayed_amount = original_quantity - *quantity;
                
                // Route decayed goods to local Landfill via UtilityDemand::waste_generation
                // This integrates with the existing Landfill module in src/utilities/waste.rs
                // Landfill.process_waste() handles modular upgrades (IncineratorModule, RecyclingModule)
                if let Some(landfill) = region.find_nearest_landfill() {
                    landfill.process_waste(decayed_amount);
                } else {
                    // No landfill available - waste accumulates in region (pollution risk)
                    region.pollution_level += decayed_amount * 0.1;
                }
            }
        }
    }
}
```

**D. 3rd-Party Logistics (integrated with existing LegalForm):**
```rust
// New LegalForm variant:
LegalForm::LogisticsCompany(LogisticsCompanyData)

pub struct LogisticsCompanyData {
    pub warehouse_network: Vec<String>,  // CommercialBuilding IDs with building_type == Warehouse
    pub fleet_capacity: f64,  // Transport capacity
    pub contracts: Vec<StorageContract>,
}

pub struct StorageContract {
    pub client_id: String,  // Company storing goods
    pub commodity: Commodity,
    pub quantity: f64,
    pub duration: u32,  // Turns
    pub warehouse_id: String,  // Specific CommercialBuilding
}

// Logistics companies own CommercialBuilding::Warehouse buildings
// They optimize fee schedules based on utilization and market demand
```

**Integration Points:**
- `CommercialBuilding` (housing.rs): Extend with storage fields and methods
- `resolve_market_prices`: Check warehouse capacity via CommercialBuilding before accepting surplus
- `process_building_cycle`: Add storage cost to OPEX for Warehouse-type buildings
- `CorporateStrategy`: Logistics companies optimize fee schedules based on utilization
- `Stage 6 waste`: Perished goods feed into existing waste management system
- `utilities/demand.rs`: Use existing UtilityDemand::for_commercial() for OPEX calculation

**Dependencies:**
- Stage 6 real estate (fully implemented - CommercialBuilding exists)
- Stage 6 waste (existing land_use_inventory system)
- Legal form system (logistics companies)

**Estimated Effort:** 3-4 days

### 4. Strategic State Reserves

**Current State:**
- No reserve agency legal form exists
- No automatic commodity purchasing logic
- No release mechanism for shortages
- State has `liquid_reserves` but no commodity reserves

**Feasibility:** HIGH

**Required Changes:**

**A. Data Structure Enhancement:**
```rust
// New LegalForm:
LegalForm::StrategicReserveAgency(StrategicReserveData)

pub struct StrategicReserveData {
    pub commodity_reserves: HashMap<Commodity, f64>,
    pub purchase_triggers: HashMap<Commodity, PurchaseTrigger>,
    pub release_triggers: HashMap<Commodity, ReleaseTrigger>,
    pub budget_allocation: f64,  // Portion of state budget for reserves
    pub max_capacity: HashMap<Commodity, f64>,  // Storage limits
}

pub struct PurchaseTrigger {
    pub price_floor: f64,  // Buy when price below this
    pub surplus_threshold: f64,  // Buy when global surplus above this
    pub budget_fraction: f64,  // Fraction of allocation to spend
}

pub struct ReleaseTrigger {
    pub price_ceiling: f64,  // Release when price above this
    pub deficit_threshold: f64,  // Release when global deficit above this
    public release_fraction: f64,  // Fraction of reserves to release
}
```

**B. Agency Logic:**
```rust
pub fn manage_strategic_reserves(
    agency: &mut Company,
    country: &mut Country,
    global_market: &GlobalMarket,
    market_orders: &mut MarketOrders,
) {
    if let LegalForm::StrategicReserveAgency(data) = &mut agency.legal_form {
        // Purchase phase
        for (commodity, trigger) in &data.purchase_triggers {
            let global_price = global_market.base_price(*commodity, 100.0);
            let global_surplus = global_market.surplus(*commodity);
            
            if global_price < trigger.price_floor || global_surplus > trigger.surplus_threshold {
                let budget = data.budget_allocation * trigger.budget_fraction;
                let purchase_amount = budget / global_price;
                
                if country.budget.liquid_reserves >= budget {
                    country.budget.liquid_reserves -= budget;
                    data.commodity_reserves.insert(*commodity, 
                        data.commodity_reserves.get(commodity).copied().unwrap_or(0.0) + purchase_amount);
                    
                    // Add buy order to support price floor
                    market_orders.add_buy(*commodity, purchase_amount);
                }
            }
        }
        
        // Release phase
        for (commodity, trigger) in &data.release_triggers {
            let global_price = global_market.base_price(*commodity, 100.0);
            let global_deficit = -global_market.surplus(*commodity).max(0.0);
            
            if global_price > trigger.price_ceiling || global_deficit > trigger.deficit_threshold {
                let available = data.commodity_reserves.get(commodity).copied().unwrap_or(0.0);
                let release_amount = (available * trigger.release_fraction).min(global_deficit);
                
                if release_amount > 0.0 {
                    data.commodity_reserves.insert(*commodity, available - release_amount);
                    
                    // Add sell order to cap price
                    market_orders.add_sell(*commodity, release_amount);
                }
            }
        }
    }
}
```

**C. Integration with State Budget:**
```rust
// In government/treasury.rs:
pub fn allocate_reserve_budget(ctx: &mut CountryTurnCtx) {
    let reserve_allocation = ctx.country.budget.nominal_budget * 0.05;  // 5% of budget
    
    // Find or create Strategic Reserve Agency
    let reserve_agency = ctx.country.companies.iter_mut()
        .find(|c| matches!(c.legal_form, LegalForm::StrategicReserveAgency(_)));
    
    if let Some(agency) = reserve_agency {
        if let LegalForm::StrategicReserveAgency(data) = &mut agency.legal_form {
            data.budget_allocation = reserve_allocation;
        }
    }
}
```

**D. Automatic Agency Creation:**
```rust
// In engine/generator/corporate.rs:
pub fn create_strategic_reserve_agency(country: &mut Country) {
    let agency = Company {
        id: "STRATEGIC_RESERVE_AGENCY".to_string(),
        name: "Agencja Rezerw Strategicznych".to_string(),
        sector: Sector::PublicServices,
        legal_form: LegalForm::StrategicReserveAgency(StrategicReserveData {
            commodity_reserves: HashMap::new(),
            purchase_triggers: initialize_triggers(),
            release_triggers: initialize_release_triggers(),
            budget_allocation: 0.0,
            max_capacity: HashMap::new(),
        }),
        // ... other fields
    };
    
    country.companies.push(agency);
}
```

**Integration Points:**
- `process_companies`: Add reserve management phase before corporate processing
- `process_government_spending`: Allocate budget to reserves
- `resolve_market_prices`: Reserve orders participate in clearing
- `generate_corporate_entities`: Create agency during world generation

**Estimated Effort:** 2-3 days

## SUMMARY & RECOMMENDATIONS

### Critical Bugs (Fix Before Stage A Implementation):

1. **State Reserves Exponential Drain** (Priority: CRITICAL)
   - Add debt ceiling: `if liquid_reserves < -GDP * 0.5: trigger_default()`
   - Cap Black Ops spending: `min(black_ops, revenue * 0.1)`
   - Add sovereign bankruptcy logic

2. **Private Capital Frozen** (Priority: CRITICAL)
   - Fix entity loading path consistency
   - Add fallback: if companies fail to load, generate in-memory
   - Verify file existence before simulation

3. **Citizen Savings Frozen** (Priority: HIGH)
   - Implement `aggregate_citizen_savings()` function
   - Call after `update_class_demographics()` each turn
   - Add telemetry for regional vs national savings comparison

4. **Static Market Imbalances** (Priority: HIGH)
   - Implement basic `SwitchMethod` triggers in existing legal forms
   - Add deficit persistence tracking (3+ turns = trigger)
   - Enable at least 1-2 synthetic production methods as proof-of-concept

### Implementation Priority:

**Phase 1 (Week 1):** Fix critical bugs
- Debt ceiling and spending caps
- Entity loading verification
- Savings aggregation
- Basic method switching

**Phase 2 (Week 2):** Strategic reserves (highest ROI, lowest complexity)
- Implement Strategic Reserve Agency
- Add automatic budget allocation
- Configure purchase/release triggers for key commodities

**Phase 3 (Week 3):** Adaptive production
- Enhance production method registry with alternatives
- Implement AI logic for method switching
- Add synthetic tech (coal-to-liquids, biomass)

**Phase 4 (Week 4-5):** State interventions
- Excise tax system
- Rationing framework
- Emergency powers trigger conditions

**Phase 5 (Week 4-5):** Physical warehousing (Stage 6 integration)
- Extend CommercialBuilding with storage fields and methods
- Implement dynamic storage fee calculation based on OPEX and utilization
- Integrate perishability with existing Landfill module
- Implement 3rd-party logistics companies

### Risk Assessment:

- **Low Risk:** Strategic reserves, adaptive production (self-contained systems)
- **Medium Risk:** State interventions (requires policy UI, balance tuning)
- **Medium Risk:** Physical warehousing (Stage 6 infrastructure exists, requires integration with CommercialBuilding and Landfill modules)

### Success Metrics:

- State reserves remain within ±20% of initial value for 100 turns
- Private capital tracks actual company capital (not frozen at 0)
- Citizen savings aggregates from regional demographics
- Market imbalances show >10% variation over 100 turns
- At least 1 production method switch occurs in response to persistent deficit
