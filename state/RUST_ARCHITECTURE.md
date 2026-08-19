# Rust Architecture Snapshot — `sim_engine` crate

**Generated:** Target 1.5 review snapshot.  
**Scope:** `src/state/`, `src/registries/`, `src/io/`, `src/math/`.  
**Convention:** `#[derive(...)]` macros are omitted. The `extra: Map<String, Value>` fields are `#[serde(flatten)]` catch-alls for lossless Python JSON round-trips.

---

## 1. Core state structs (`src/state/`)

### `src/state/mod.rs`

```rust
pub struct Country {
    pub name: String,
    pub budget: Treasury,
    pub macro_indicators: MacroData,
    pub tax_rates: TaxRates,
}

pub struct GameState {
    pub countries: HashMap<String, Country>,
    pub extra: Map<String, Value>,
}
```

---

### `src/state/treasury.rs`

```rust
pub type TechId = String; // defined in src/registries/tech_tree.rs

pub struct StockMarket {
    pub index: f64,
    pub confidence: f64,
    pub last_change: f64,
    pub sector_indices: Value,
    pub extra: Map<String, Value>,
}

pub struct BudgetAllocations {
    pub industry: f64,
    pub education_propaganda: f64,
    pub healthcare: f64,
    pub infrastructure_transport: f64,
    pub social_programs: f64,
    pub agriculture_rural: f64,
    pub armed_forces: f64,
    pub extra: Map<String, Value>,
}

pub struct ProductionMethodChoice {
    pub automation: String,
    pub production: String,
    pub organization: String,
    pub extra: Map<String, Value>,
}

pub struct SectorShare {
    pub gdp_share: f64,
    pub crisis_vulnerability: Option<f64>,
    pub active_method: Option<ProductionMethodChoice>,
    pub extra: Map<String, Value>,
}

pub struct ScienceState {
    pub innovation_points: f64,
    pub researching: Option<TechId>,
    pub discovered: Vec<TechId>,
    pub base_innovativeness: f64,
    pub extra: Map<String, Value>,
}

pub struct Treasury {
    pub gdp: f64,
    pub population: u64,
    pub nominal_budget: f64,
    pub liquid_reserves: f64,
    pub citizen_savings: f64,
    pub private_capital: f64,
    pub infrastructure_level: f64,
    pub energy_infrastructure: f64,
    pub stock_market: StockMarket,
    pub allocations: BudgetAllocations,
    pub black_ops_budget: f64,
    pub sectors: HashMap<Sector, SectorShare>,
    pub science: ScienceState,
    pub last_balance_log: String,
    pub trade_balance: Option<f64>,
    pub extra: Map<String, Value>,
}
```

---

### `src/state/macro_data.rs`

```rust
pub type CurrencyCode = String;

pub struct EnergyMix {
    pub coal: f64,
    pub natural_gas: f64,
    pub uranium: f64,
    pub renewables: f64,
    pub extra: Map<String, Value>,
}

pub struct MacroData {
    pub inflation: f64,
    pub gini: f64,
    pub social_unrest: f64,
    pub wealth_bracket: WealthBracket,
    pub productivity: f64,
    pub currency: CurrencyCode,
    pub energy_mix: EnergyMix,
    pub average_wage: f64,
    pub culture: String,
    pub cultural_group: String,
    pub religion: String,
    pub labor_market: LaborMarket,
    pub demographics: Demographics,
    pub extra: Map<String, Value>,
}
```

---

### `src/state/tax.rs`

```rust
pub struct IncomeTax {
    pub rate: f64,
    pub structure: String,
    pub extra: Map<String, Value>,
}

pub struct VatBracket {
    pub rate: f64,
    pub consumption_share: f64,
    pub extra: Map<String, Value>,
}

pub struct PublicDebt {
    pub current_debt: f64,
    pub interest_rate: f64,
    pub extra: Map<String, Value>,
}

pub struct TaxRates {
    pub income_tax: IncomeTax,
    pub corporate_tax: f64,
    pub vat: HashMap<String, VatBracket>,
    pub public_debt: PublicDebt,
    pub extra: Map<String, Value>,
}
```

---

## 2. Registries (`src/registries/`)

### `src/registries/enums.rs`

```rust
pub enum RegimeType {
    Democracy,
    Autocracy,
}

pub enum ConscriptionLaw {
    MandatoryTraining,
    FullService,
    None_,
}

pub enum WomenInArmy {
    ReserveOnly,
    FullAccess,
    Banned,
}

pub enum DraftScope {
    Voluntary,
    Selective,
    UniversalDraft,
}

pub enum WealthBracket {
    VeryHigh,
    High,
    Medium,
    Low,
}

pub enum Sector {
    Mining,
    Agriculture,
    HeavyIndustry,
    LightIndustry,
    ArmamentsIndustry,
    LocalServices,
    ExportServices,
    Construction,
    Energy,
    MedicalServices,
    EducationServices,
    PublicServices,
    TransportLogistics,
}

pub enum Commodity {
    Weapons,
    Ammunition,
    Fuel,
    Food,
    Uniforms,
    Vehicles,
    Electronics,
    B2bServices,
    Paper,
}

pub enum FuelType {
    Coal,
    NaturalGas,
    Uranium,
    AgriculturalProduce,
    None_,
}
```

---

### `src/registries/tech_tree.rs`

```rust
pub type TechId = String;

pub struct TechNode {
    pub name: String,
    pub year: u32,
    pub cost: u32,
    pub description: String,
    pub unlocks_methods: HashMap<String, HashMap<String, String>>,
    pub unlocks_projects: Vec<String>,
    pub prerequisites: Vec<TechId>,
}

pub fn load_tech_tree(json: &str) -> Result<HashMap<TechId, TechNode>, serde_json::Error>;
```

---

### `src/registries/production_methods.rs`

```rust
pub struct ProductionMethod {
    pub year: u32,
    pub required_tech: Option<String>,
    pub experts_ratio: f64,
    pub skilled_ratio: f64,
    pub basic_ratio: f64,
    pub efficiency: f64,
    pub inputs: HashMap<Commodity, f64>,
    pub outputs: HashMap<Commodity, f64>,
}

pub type MethodSet = HashMap<String, ProductionMethod>;

pub fn state_building_methods() -> HashMap<String, MethodSet>;
```

---

### `src/registries/buildings.rs`

```rust
pub type BuildingKind = String;

pub struct BuildingTemplate {
    pub sector: Sector,
    pub build_cost: u64,
    pub build_time_turns: u32,
    pub worker_capacity: u32,
    pub min_year: u32,
    pub required_tech: Option<TechId>,
    pub lower_tier: Option<BuildingKind>,
    pub area_ha: u32,
}

impl BuildingTemplate {
    pub fn is_available(&self, current_year: u32, discovered_tech: &[TechId]) -> bool;
}

pub fn load_building_registry(json: &str) -> Result<HashMap<BuildingKind, BuildingTemplate>, serde_json::Error>;
pub fn state_apparatus_templates() -> HashMap<BuildingKind, BuildingTemplate>;
```

---

### `src/registries/government.rs`

```rust
pub struct GovernmentForm {
    pub regime_type: RegimeType,
    pub election_cycle: u32,
    pub revolution_threshold: f64,
    pub chambers: u32,
    pub head_of_government: String,
    pub head_of_state: String,
    pub subtypes: Vec<String>,
}

pub fn government_forms() -> HashMap<String, GovernmentForm>;
```

---

### `src/registries/mod.rs`

```rust
pub struct Registries {
    pub tech_tree: HashMap<TechId, TechNode>,
    pub production_methods: HashMap<String, MethodSet>,
    pub building_templates: HashMap<BuildingKind, BuildingTemplate>,
    pub government_forms: HashMap<String, GovernmentForm>,
}

impl Registries {
    pub fn from_tech_tree_json(tech_tree_json: &str) -> Result<Arc<Self>, serde_json::Error>;
    pub fn native_only() -> Arc<Self>;
}
```

---

## 3. I/O bridge (`src/io/`)

### `src/io/save_manager.rs`

```rust
pub enum SaveError {
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingCountry(String),
}

pub fn load_named_map<T: DeserializeOwned>(path: &Path) -> Result<HashMap<String, T>, SaveError>;
pub fn load_country_data(data_dir: &Path, country: &str) -> Result<Country, SaveError>;
pub fn load_game_state(data_dir: &Path) -> Result<GameState, SaveError>;
```

---

## 4. Core math (`src/math/`)

```rust
pub fn apply_decay(value: f64, rate: f64) -> f64;
pub fn gain_experience(current: f64, gain: f64, cap: f64) -> f64;
pub fn siphon_fraction(budget: f64, fraction: f64) -> f64;
pub fn normalize(weights: &[f64]) -> Vec<f64>;
pub fn clamp_percent(value: f64) -> f64;
```

---

## 5. Key architectural invariants

- **All structs derive `Serialize, Deserialize, Debug, Clone, PartialEq`**, except `Registries` which derives `Debug, Clone, PartialEq` (not `Serialize`/`Deserialize` by design; it is built at runtime).
- **Polish JSON keys are preserved via `#[serde(rename = "...")]`** on every modeled field.
- **Lossless round-trips** are guaranteed by `#[serde(flatten)] extra: Map<String, Value>` on every struct that mirrors a Python dictionary. Volatile subtrees (`polityka`, `rynek_pracy`, `demografia`, etc.) and runtime-added fields are not dropped.
- **No global mutable state** as of Target 1. `Registries` is shared via `Arc<Registries>` and state is passed explicitly (`Country`, `GameState`).
- **Static vs. dynamic split:** `registries/` holds immutable world-gen definitions; `state/` holds mutable per-country and global state; `io/` is the serde bridge; `math/` is pure numeric utilities.

## 6. Target 2 onward placeholders

- `src/economy/` — deterministic economic turn functions; will consume `Country` and `Registries` (and later global `GameState`) and produce updated `Treasury` / `MacroData`.
- `tests/golden_master_test.rs` — parity harness comparing Rust turn output to Python-generated expected outputs.
