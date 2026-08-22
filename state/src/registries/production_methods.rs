//! Production Method (PM) registry for state-owned security buildings.
//!
//! Faithful port of `STATE_BUILDING_METHODS` in
//! `economy/production/methods/state.py`. Each building kind has one or more
//! era-gated methods defining labor composition, efficiency, and commodity
//! inputs/outputs.

use crate::registries::enums::Commodity;
use crate::registries::tech_tree::TechId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single production method: labor mix, efficiency, and material flows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProductionMethod {
    /// Earliest year this method may be adopted (`"year"`).

    pub year: u32,

    /// TechId required to unlock this method, if any
    /// (`"required_tech"`). Stores a stable TechId (e.g. `"steam_003"`),
    /// never a display name — i18n safe.
    #[serde(default)]
    pub required_tech: Option<TechId>,

    /// Fraction of staff who are experts (`"experts_ratio"`).

    pub experts_ratio: f64,

    /// Fraction of staff who are skilled workers (`"skilled_ratio"`).

    pub skilled_ratio: f64,

    /// Fraction of staff who are basic workers (`"basic_ratio"`).

    pub basic_ratio: f64,

    /// Output multiplier of this method (`"efficiency"`).

    pub efficiency: f64,

    /// Per-turn commodity inputs consumed (`"inputs"`).
    #[serde(default)]
    pub inputs: HashMap<Commodity, f64>,

    /// Per-turn commodity outputs produced (`"outputs"`). Empty for pure
    /// service/consumption buildings such as the state apparatus.
    #[serde(default)]
    pub outputs: HashMap<Commodity, f64>,

    /// Phase 74: Thermal efficiency (0.0�1.0) for energy production methods.
    /// Fraction of fuel calorific energy converted to useful Energy/Heat output.
    /// 0.0 for non-energy methods (default). When > 0.0, `process_building_cycle()`
    /// dynamically computes fuel consumption from the target energy output and
    /// the actual `calorific_value_mj_per_unit()` of input fuels, rather than
    /// using fixed input quantities.
    #[serde(default)]
    pub thermal_efficiency: f64,

    /// Phase 79: Round-trip storage efficiency (0.0-1.0) for energy storage methods.
    /// Fraction of input Energy that can be recovered as output Energy.
    /// 0.0 for non-storage methods (default). When > 0.0, `process_building_cycle()`
    /// enforces strict conservation: `output_energy = input_energy * storage_efficiency`.
    /// Used by `PumpedStoragePlant` (~0.72) and `BatteryBank` (~0.87).
    /// A method should have either `thermal_efficiency > 0.0` (fuel->energy) OR
    /// `storage_efficiency > 0.0` (energy->energy), never both.
    #[serde(default)]
    pub storage_efficiency: f64,
}

/// Production method slot, mirroring `ProductionMethodChoice` fields.
/// A tech's `unlocks_methods` inner key maps directly to one of these variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodSlot {
    /// Automation method slot (corresponds to `ProductionMethodChoice.automation`).
    Automation,
    /// Production method slot (corresponds to `ProductionMethodChoice.production`).
    Production,
    /// Organization method slot (corresponds to `ProductionMethodChoice.organization`).
    Organization,
}

impl MethodSlot {
    /// Parse a slot from its string key as used in `unlocks_methods`.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "automation" => Some(MethodSlot::Automation),
            "production" => Some(MethodSlot::Production),
            "organization" => Some(MethodSlot::Organization),
            _ => None,
        }
    }
}

/// All production methods for one sector, grouped by slot.
/// Directly mirrors `ProductionMethodChoice { automation, production, organization }`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BuildingMethods {
    /// Methods for the automation slot.
    #[serde(default)]
    pub automation: HashMap<String, ProductionMethod>,
    /// Methods for the production slot.
    #[serde(default)]
    pub production: HashMap<String, ProductionMethod>,
    /// Methods for the organization slot.
    #[serde(default)]
    pub organization: HashMap<String, ProductionMethod>,
}

impl BuildingMethods {
    /// Look up a method by slot and name.
    pub fn get(&self, slot: MethodSlot, name: &str) -> Option<&ProductionMethod> {
        match slot {
            MethodSlot::Automation => self.automation.get(name),
            MethodSlot::Production => self.production.get(name),
            MethodSlot::Organization => self.organization.get(name),
        }
    }

    /// Insert a method into a specific slot.
    pub fn insert(&mut self, slot: MethodSlot, name: String, pm: ProductionMethod) {
        match slot {
            MethodSlot::Automation => {
                self.automation.insert(name, pm);
            }
            MethodSlot::Production => {
                self.production.insert(name, pm);
            }
            MethodSlot::Organization => {
                self.organization.insert(name, pm);
            }
        }
    }

    /// Iterate all methods across all slots (for year-based fallback lookups).
    pub fn iter_all(&self) -> impl Iterator<Item = &ProductionMethod> {
        self.automation
            .values()
            .chain(self.production.values())
            .chain(self.organization.values())
    }
}

/// Builds the state-apparatus production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// natively encoding `STATE_BUILDING_METHODS` from the Python source.
///
/// # Rules
/// * Covers `military_base`, `police_station`, `courthouse`, and `intelligence_hq`.
/// * Labor ratios (`experts + skilled + basic`) sum to `1.0` for every method.
/// * State buildings produce no outputs; they consume inputs to deliver
///   intangible security/justice effects.
pub fn state_building_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- military_base (Military Base) --
    let mut baza = BuildingMethods::default();
    baza.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Rifles, 5.0),
                (Commodity::Ammunition, 10.0),
                (Commodity::Fuels, 15.0),
                (Commodity::Food, 20.0),
                (Commodity::Clothing, 5.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    baza.insert(MethodSlot::Production, "Mechanized".to_string(),
        ProductionMethod {
            year: 1910,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.30,
            basic_ratio: 0.50,
            efficiency: 1.3,
            inputs: HashMap::from([
                (Commodity::Rifles, 8.0),
                (Commodity::Ammunition, 15.0),
                (Commodity::Fuels, 20.0),
                (Commodity::Food, 18.0),
                (Commodity::Clothing, 5.0),
                (Commodity::Cars, 2.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    baza.insert(MethodSlot::Production, "Modern".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.25,
            skilled_ratio: 0.25,
            basic_ratio: 0.50,
            efficiency: 1.8,
            inputs: HashMap::from([
                (Commodity::Rifles, 10.0),
                (Commodity::Ammunition, 20.0),
                (Commodity::Fuels, 25.0),
                (Commodity::Food, 15.0),
                (Commodity::Clothing, 5.0),
                (Commodity::Cars, 5.0),
                (Commodity::ElectronicComponents, 3.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("military_base".to_string(), baza);

    // -- police_station (Police Station) --
    let mut komis = BuildingMethods::default();
    komis.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.60,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Rifles, 1.0),
                (Commodity::Cars, 5.0),
                (Commodity::AdministrativeServices, 2.0),
                (Commodity::Paper, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SecurityCapacity, 12.0),
            ]),
            ..Default::default()
        },
    );
    komis.insert(MethodSlot::Production, "Upgraded".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.55,
            basic_ratio: 0.30,
            efficiency: 1.4,
            inputs: HashMap::from([
                (Commodity::Rifles, 2.0),
                (Commodity::Cars, 6.0),
                (Commodity::AdministrativeServices, 3.0),
                (Commodity::Paper, 2.0),
                (Commodity::ElectronicComponents, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SecurityCapacity, 20.0),
            ]),
            ..Default::default()
        },
    );
    komis.insert(MethodSlot::Production, "Digital".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.50,
            basic_ratio: 0.30,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::Rifles, 2.0),
                (Commodity::Cars, 6.0),
                (Commodity::AdministrativeServices, 4.0),
                (Commodity::Paper, 1.0),
                (Commodity::ElectronicComponents, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SecurityCapacity, 35.0),
            ]),
            ..Default::default()
        },
    );
    komis.insert(MethodSlot::Production, "Militarized".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.45,
            basic_ratio: 0.40,
            efficiency: 1.6,
            inputs: HashMap::from([
                (Commodity::Rifles, 5.0),
                (Commodity::Ammunition, 8.0),
                (Commodity::Cars, 4.0),
                (Commodity::AdministrativeServices, 3.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SecurityCapacity, 30.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("police_station".to_string(), komis);

    // -- courthouse (Courthouse) --
    let mut sad = BuildingMethods::default();
    sad.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 5.0),
                (Commodity::AdministrativeServices, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::JusticeCapacity, 10.0),
            ]),
            ..Default::default()
        },
    );
    sad.insert(MethodSlot::Production, "Upgraded".to_string(),
        ProductionMethod {
            year: 1930,
            required_tech: None,
            experts_ratio: 0.45,
            skilled_ratio: 0.35,
            basic_ratio: 0.20,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Paper, 4.0),
                (Commodity::AdministrativeServices, 6.0),
                (Commodity::ElectronicComponents, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::JusticeCapacity, 18.0),
            ]),
            ..Default::default()
        },
    );
    sad.insert(MethodSlot::Production, "Digital".to_string(),
        ProductionMethod {
            year: 2000,
            required_tech: None,
            experts_ratio: 0.50,
            skilled_ratio: 0.30,
            basic_ratio: 0.20,
            efficiency: 2.2,
            inputs: HashMap::from([
                (Commodity::Paper, 2.0),
                (Commodity::AdministrativeServices, 7.0),
                (Commodity::ElectronicComponents, 4.0),
            ]),
            outputs: HashMap::from([
                (Commodity::JusticeCapacity, 30.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("courthouse".to_string(), sad);

    // -- intelligence_hq (Intelligence HQ) --
    let mut sluzby = BuildingMethods::default();
    sluzby.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.30,
            skilled_ratio: 0.50,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Rifles, 2.0),
                (Commodity::Cars, 3.0),
                (Commodity::AdministrativeServices, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::IntelligenceCapacity, 8.0),
            ]),
            ..Default::default()
        },
    );
    sluzby.insert(MethodSlot::Production, "Upgraded".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.35,
            skilled_ratio: 0.45,
            basic_ratio: 0.20,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::ElectronicComponents, 15.0),
                (Commodity::Rifles, 3.0),
                (Commodity::Cars, 4.0),
                (Commodity::AdministrativeServices, 6.0),
                (Commodity::Paper, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::IntelligenceCapacity, 15.0),
            ]),
            ..Default::default()
        },
    );
    sluzby.insert(MethodSlot::Production, "Modern".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::ElectronicComponents, 20.0),
                (Commodity::Rifles, 3.0),
                (Commodity::Cars, 4.0),
                (Commodity::AdministrativeServices, 7.0),
                (Commodity::Paper, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::IntelligenceCapacity, 25.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("intelligence_hq".to_string(), sluzby);

    // -- prison (Prison) --
    // PMs vary by PrisonType. The active PM is selected based on the
    // country's PrisonLaborLaw.prison_type at runtime.
    // VoluntaryLabor and StatePenalColony produce goods via building inventory.
    // PrivateLaborCamps and IsolationCamp produce nothing — they operate
    // through the labor market phase instead (see economy/prison_labor.rs).
    let mut wiezienie = BuildingMethods::default();

    // VoluntaryLabor: workshop production
    wiezienie.insert(MethodSlot::Production, "Workshop".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.30,
            basic_ratio: 0.65,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 10.0),
                (Commodity::Timber, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::Furniture, 5.0),
            ]),
            ..Default::default()
        },
    );

    // StatePenalColony: forced heavy labor producing raw materials
    wiezienie.insert(MethodSlot::Production, "Quarry".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.02,
            skilled_ratio: 0.18,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 8.0),
                (Commodity::MechanicalComponents, 10.0),
            ]),
            outputs: HashMap::from([
                (Commodity::Stone, 30.0),
                (Commodity::HardCoal, 20.0),
            ]),
            ..Default::default()
        },
    );

    // PrivateLaborCamps: no building output — FTEs injected into labor market
    wiezienie.insert(MethodSlot::Production, "Private Labor Camp".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.02,
            skilled_ratio: 0.08,
            basic_ratio: 0.90,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 5.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );

    // IsolationCamp: no production — prisoners removed from workforce
    wiezienie.insert(MethodSlot::Production, "Detention Camp".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.15,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 5.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );

    registry.insert("prison".to_string(), wiezienie);

    // -- fire_station (Professional State Fire Brigade) --
    let mut straz = BuildingMethods::default();
    straz.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.40,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Rifles, 0.0),
                (Commodity::Cars, 2.0),
                (Commodity::Water, 10.0),
            ]),
            outputs: HashMap::from([
                (Commodity::FireProtectionCapacity, 8.0),
            ]),
            ..Default::default()
        },
    );
    straz.insert(MethodSlot::Production, "Motorized".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.45,
            basic_ratio: 0.40,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Cars, 4.0),
                (Commodity::Water, 15.0),
                (Commodity::Chemicals, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::FireProtectionCapacity, 20.0),
            ]),
            ..Default::default()
        },
    );
    straz.insert(MethodSlot::Production, "Professional".to_string(),
        ProductionMethod {
            year: 1980,
            required_tech: None,
            experts_ratio: 0.25,
            skilled_ratio: 0.45,
            basic_ratio: 0.30,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::Cars, 5.0),
                (Commodity::ElectronicComponents, 3.0),
                (Commodity::Chemicals, 8.0),
            ]),
            outputs: HashMap::from([
                (Commodity::FireProtectionCapacity, 35.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("fire_station".to_string(), straz);

    // -- flood_shelter (Flood Shelter / Levee) --
    let mut schron = BuildingMethods::default();
    schron.insert(MethodSlot::Production, "Flood Embankment".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.25,
            basic_ratio: 0.70,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Bricks, 20.0),
            ]),
            outputs: HashMap::from([
                (Commodity::ShelterCapacity, 10.0),
            ]),
            ..Default::default()
        },
    );
    schron.insert(MethodSlot::Production, "Upgraded Embankment".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Bricks, 15.0),
                (Commodity::Cement, 10.0),
            ]),
            outputs: HashMap::from([
                (Commodity::ShelterCapacity, 25.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("flood_shelter".to_string(), schron);

    // -- border_guard (Border Guard) --
    let mut straz_gran = BuildingMethods::default();
    straz_gran.insert(MethodSlot::Production, "Border Patrol".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.25,
            basic_ratio: 0.70,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 10.0),
                (Commodity::Rifles, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::BorderEnforcementCapacity, 10.0),
            ]),
            ..Default::default()
        },
    );
    straz_gran.insert(MethodSlot::Production, "Motorized Patrol".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.35,
            basic_ratio: 0.55,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Food, 10.0),
                (Commodity::Cars, 3.0),
                (Commodity::Rifles, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::BorderEnforcementCapacity, 25.0),
            ]),
            ..Default::default()
        },
    );
    straz_gran.insert(MethodSlot::Production, "Modern Border Guard".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::Food, 10.0),
                (Commodity::Cars, 5.0),
                (Commodity::ElectronicComponents, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::BorderEnforcementCapacity, 50.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("border_guard".to_string(), straz_gran);

    // -- customs_office (Customs House) --
    let mut urzad_cel = BuildingMethods::default();
    urzad_cel.insert(MethodSlot::Production, "Customs Post".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 8.0),
                (Commodity::Paper, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::CustomsCapacity, 10.0),
            ]),
            ..Default::default()
        },
    );
    urzad_cel.insert(MethodSlot::Production, "Upgraded Customs Office".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.40,
            basic_ratio: 0.45,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Food, 8.0),
                (Commodity::Paper, 1.0),
                (Commodity::ElectronicComponents, 3.0),
            ]),
            outputs: HashMap::from([
                (Commodity::CustomsCapacity, 25.0),
            ]),
            ..Default::default()
        },
    );
    urzad_cel.insert(MethodSlot::Production, "e-Toll Customs System".to_string(),
        ProductionMethod {
            year: 2000,
            required_tech: None,
            experts_ratio: 0.25,
            skilled_ratio: 0.45,
            basic_ratio: 0.30,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::Food, 8.0),
                (Commodity::ElectronicComponents, 8.0),
                (Commodity::Software, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::CustomsCapacity, 50.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("customs_office".to_string(), urzad_cel);

    // -- sanepid (Sanitary Inspectorate) --
    let mut sanepid = BuildingMethods::default();
    sanepid.insert(MethodSlot::Production, "Sanitary Station".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Chemicals, 3.0),
                (Commodity::Paper, 3.0),
                (Commodity::Food, 4.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SanitaryInspectionCapacity, 10.0),
            ]),
            ..Default::default()
        },
    );
    sanepid.insert(MethodSlot::Production, "Upgraded sanepid".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.25,
            skilled_ratio: 0.45,
            basic_ratio: 0.30,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::Chemicals, 5.0),
                (Commodity::ElectronicComponents, 4.0),
                (Commodity::Paper, 2.0),
                (Commodity::Food, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::SanitaryInspectionCapacity, 25.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("sanepid".to_string(), sanepid);

    // -- construction_inspectorate (Building Inspectorate) --
    let mut insp_bud = BuildingMethods::default();
    insp_bud.insert(MethodSlot::Production, "Supervision Office".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 5.0),
                (Commodity::Food, 5.0),
                (Commodity::Cars, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::BuildingInspectionCapacity, 8.0),
            ]),
            ..Default::default()
        },
    );
    insp_bud.insert(MethodSlot::Production, "Upgraded Inspectorate".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 2.0,
            inputs: HashMap::from([
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Paper, 2.0),
                (Commodity::Food, 4.0),
                (Commodity::Cars, 3.0),
            ]),
            outputs: HashMap::from([
                (Commodity::BuildingInspectionCapacity, 20.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("construction_inspectorate".to_string(), insp_bud);

    // -- environmental_inspectorate (Environmental Inspectorate) --
    let mut insp_srod = BuildingMethods::default();
    insp_srod.insert(MethodSlot::Production, "Control Station".to_string(),
        ProductionMethod {
            year: 1970,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Chemicals, 5.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::Paper, 3.0),
                (Commodity::Food, 4.0),
            ]),
            outputs: HashMap::from([
                (Commodity::EnvironmentalInspectionCapacity, 12.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("environmental_inspectorate".to_string(), insp_srod);

    registry
}

/// Builds the industrial production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// covering industrial recipes like Solvay process and seed production.
///
/// # Rules
/// * All inputs/outputs are per-1000-worker.
/// * Era-gated by year and technology.
pub fn industrial_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- Solvay Process (Soda Ash Production) --
    let mut solvay = BuildingMethods::default();
    solvay.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1860,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Salt, 2.5),
                (Commodity::Limestone, 2.0),
                (Commodity::Ammonia, 0.5),
            ]),
            outputs: HashMap::from([(Commodity::SodaAsh, 1.0)]),
            ..Default::default()
        },
    );
    registry.insert("soda_ash_plant".to_string(), solvay);

    // -- Seed Mill --
    let mut seed_mill = BuildingMethods::default();
    seed_mill.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Cereal, 1.5), (Commodity::Meat, 0.3)]),
            outputs: HashMap::from([(Commodity::Seeds, 1.0)]),
            ..Default::default()
        },
    );
    registry.insert("seed_mill".to_string(), seed_mill);

    // -- forest_district (Forest District — commercial building owned by State Forests company) --
    let mut state_forest_methods = BuildingMethods::default();
    state_forest_methods.insert(MethodSlot::Production, "Forestry Management".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.20,
            basic_ratio: 0.75,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Fuels, 2.0),
                (Commodity::Food, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::Timber, 10.0),
            ]),
            ..Default::default()
        },
    );
    state_forest_methods.insert(MethodSlot::Production, "Sustainable Forestry".to_string(),
        ProductionMethod {
            year: 1990,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Fuels, 1.5),
                (Commodity::ElectronicComponents, 2.0),
                (Commodity::Food, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::Timber, 15.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("forest_district".to_string(), state_forest_methods);

    // -- Phase 39: Statecraft Buildings --
    // Each new building type has a production method that consumes inputs
    // and produces outputs relevant to its ministry competency.

    // Court — Justice ministry: produces ProsecutionCapacity (service output)
    let mut court_methods = BuildingMethods::default();
    court_methods.insert(MethodSlot::Production, "Court Operations".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.30,
            skilled_ratio: 0.40,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 1.0),
                (Commodity::AdministrativeServices, 2.0),
            ]),
            outputs: HashMap::new(), // Service output: ProsecutionCapacity
            ..Default::default()
        },
    );
    registry.insert("Court".to_string(), court_methods);

    // CustomsOffice — Treasury: facilitates tariff collection
    let mut customs_methods = BuildingMethods::default();
    customs_methods.insert(MethodSlot::Production, "Customs Operations".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.30,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 1.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("CustomsOffice".to_string(), customs_methods);

    // Embassy — Foreign Affairs: diplomatic output
    let mut embassy_methods = BuildingMethods::default();
    embassy_methods.insert(MethodSlot::Production, "Diplomatic Operations".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 2.0),
                (Commodity::AdministrativeServices, 3.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("Embassy".to_string(), embassy_methods);

    // ResearchInstitute — Science: produces ResearchOutput
    let mut research_methods = BuildingMethods::default();
    research_methods.insert(MethodSlot::Production, "Research Program".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.50,
            skilled_ratio: 0.35,
            basic_ratio: 0.15,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 2.0),
                (Commodity::ElectronicComponents, 5.0),
                (Commodity::AdministrativeServices, 3.0),
            ]),
            outputs: HashMap::new(), // Service output: ResearchOutput
            ..Default::default()
        },
    );
    registry.insert("ResearchInstitute".to_string(), research_methods);

    // LaborInspectorate — Labor: enforces labor regulations
    let mut labor_inspectorate_methods = BuildingMethods::default();
    labor_inspectorate_methods.insert(MethodSlot::Production, "Labor Inspection".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.30,
            skilled_ratio: 0.40,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 1.0),
                (Commodity::AdministrativeServices, 1.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("LaborInspectorate".to_string(), labor_inspectorate_methods);

    // PublicWorksSite — Labor: public employment program
    let mut public_works_methods = BuildingMethods::default();
    public_works_methods.insert(MethodSlot::Production, "Public Works".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.15,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 2.0),
                (Commodity::ConstructionMachinery, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::AdministrativeServices, 3.0), // Public infrastructure services
            ]),
            ..Default::default()
        },
    );
    registry.insert("PublicWorksSite".to_string(), public_works_methods);

    // NationalTheater — Culture: produces CulturalOutput
    let mut theater_methods = BuildingMethods::default();
    theater_methods.insert(MethodSlot::Production, "Theatrical Production".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 2.0),
                (Commodity::Clothing, 1.0),
                (Commodity::AdministrativeServices, 2.0),
            ]),
            outputs: HashMap::new(), // Service output: CulturalOutput
            ..Default::default()
        },
    );
    registry.insert("NationalTheater".to_string(), theater_methods);

    // NationalLibrary — Culture: produces CulturalOutput (knowledge)
    let mut library_methods = BuildingMethods::default();
    library_methods.insert(MethodSlot::Production, "Library Services".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.35,
            skilled_ratio: 0.45,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Food, 1.0),
                (Commodity::AdministrativeServices, 2.0),
            ]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("NationalLibrary".to_string(), library_methods);

    // TransportDepot — Transport: public transport hub
    let mut transport_depot_methods = BuildingMethods::default();
    transport_depot_methods.insert(MethodSlot::Production, "Public Transport".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.20,
            basic_ratio: 0.70,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Fuels, 5.0),
                (Commodity::Food, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::AdministrativeServices, 10.0), // Transport services
            ]),
            ..Default::default()
        },
    );
    registry.insert("TransportDepot".to_string(), transport_depot_methods);

    // -- Phase 17C: Monastery/Temple Production Methods --
    // These are used by CulturalBuilding (not Building), linked via production_method field.
    // Revenue credits the owning company via TransferSettler, not building.available_cash.

    // monastery_wine_production — consumes Fruit, produces Luxury (wine proxy)
    let mut monastery_wine = BuildingMethods::default();
    monastery_wine.insert(MethodSlot::Production, "monastery_wine_production".to_string(),
        ProductionMethod {
            year: 1000,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.15,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Cereal, 5.0),
                (Commodity::Food, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::LuxuryFurniture, 3.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("monastery_wine_production".to_string(), monastery_wine);

    // monastery_scriptorium — consumes Paper, produces ReligiousTexts
    let mut monastery_scriptorium = BuildingMethods::default();
    monastery_scriptorium.insert(MethodSlot::Production, "monastery_scriptorium".to_string(),
        ProductionMethod {
            year: 1000,
            required_tech: None,
            experts_ratio: 0.30,
            skilled_ratio: 0.40,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 8.0),
                (Commodity::Food, 3.0),
            ]),
            outputs: HashMap::from([
                (Commodity::ReligiousTexts, 5.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("monastery_scriptorium".to_string(), monastery_scriptorium);

    // monastery_workshop — consumes Wood/Furniture, produces LuxuryFurniture + ReligiousArt
    let mut monastery_workshop = BuildingMethods::default();
    monastery_workshop.insert(MethodSlot::Production, "monastery_workshop".to_string(),
        ProductionMethod {
            year: 1000,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Furniture, 3.0),
                (Commodity::Timber, 5.0),
                (Commodity::Food, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::LuxuryFurniture, 2.0),
                (Commodity::ReligiousArt, 2.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("monastery_workshop".to_string(), monastery_workshop);

    // temple_artisan_workshop — consumes LuxuryFurniture, produces ReligiousArt
    let mut temple_artisan = BuildingMethods::default();
    temple_artisan.insert(MethodSlot::Production, "temple_artisan_workshop".to_string(),
        ProductionMethod {
            year: 1000,
            required_tech: None,
            experts_ratio: 0.25,
            skilled_ratio: 0.45,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::LuxuryFurniture, 2.0),
                (Commodity::Food, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::ReligiousArt, 4.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("temple_artisan_workshop".to_string(), temple_artisan);

    // monastery_herbal_garden — consumes Seeds, produces Pharmaceuticals
    let mut monastery_herbal = BuildingMethods::default();
    monastery_herbal.insert(MethodSlot::Production, "monastery_herbal_garden".to_string(),
        ProductionMethod {
            year: 1000,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.30,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Seeds, 3.0),
                (Commodity::Food, 2.0),
            ]),
            outputs: HashMap::from([
                (Commodity::Pharmaceuticals, 4.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("monastery_herbal_garden".to_string(), monastery_herbal);

    registry
}

/// Builds Phase 6.5 retail production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// for retail/wholesale buildings in the B2C market.
///
/// # Rules
/// * Covers `marketplace`, `wholesale`, `retail_shop`, `supermarket`, `department_store`, `shopping_mall`.
/// * Retail buildings consume no inputs; they provide service capacity for B2C clearing.
/// * Labor ratios (`experts + skilled + basic`) sum to `1.0` for every method.
pub fn retail_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- marketplace (Marketplace) --
    let mut targ = BuildingMethods::default();
    targ.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.15,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("marketplace".to_string(), targ);

    // -- wholesale (Wholesaler) --
    let mut hurtownia = BuildingMethods::default();
    hurtownia.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    hurtownia.insert(MethodSlot::Automation, "Mechanized".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.3,
            inputs: HashMap::from([(Commodity::Fuels, 5.0)]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("wholesale".to_string(), hurtownia);

    // -- retail_shop (Retail Store) --
    let mut sklep = BuildingMethods::default();
    sklep.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.20,
            basic_ratio: 0.75,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("retail_shop".to_string(), sklep);

    // -- supermarket --
    let mut supermarket = BuildingMethods::default();
    supermarket.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Energy, 10.0)]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("supermarket".to_string(), supermarket);

    // -- department_store (Department Store) --
    let mut dom_towarowy = BuildingMethods::default();
    dom_towarowy.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("department_store".to_string(), dom_towarowy);

    // -- shopping_mall (Shopping Center) --
    let mut centrum = BuildingMethods::default();
    centrum.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1970,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Energy, 50.0)]),
            outputs: HashMap::new(),
            ..Default::default()
        },
    );
    registry.insert("shopping_mall".to_string(), centrum);

    registry
}

/// Builds Phase 7 university production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// for universities that generate Innovation Points.
///
/// # Rules
/// * Universities consume Paper, Chemicals, Electronics as inputs
/// * Output is Innovation Points (Commodity::InnovationPoints)
/// * High Expert labor requirement for research output
/// * Physical Limits: No inputs = zero innovation points
pub fn university_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- university (University) --
    let mut uniwersytet = BuildingMethods::default();
    uniwersytet.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 20.0),
                (Commodity::Chemicals, 10.0),
            ]),
            outputs: HashMap::from([(Commodity::InnovationPoints, 5.0)]),
            ..Default::default()
        },
    );
    uniwersytet.insert(MethodSlot::Automation, "Mechanized".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.45,
            skilled_ratio: 0.35,
            basic_ratio: 0.20,
            efficiency: 1.5,
            inputs: HashMap::from([
                (Commodity::Paper, 25.0),
                (Commodity::Chemicals, 15.0),
                (Commodity::ElectronicComponents, 5.0),
            ]),
            outputs: HashMap::from([(Commodity::InnovationPoints, 8.0)]),
            ..Default::default()
        },
    );
    registry.insert("university".to_string(), uniwersytet);

    // -- technical_university (Polytechnic) --
    let mut politechnika = BuildingMethods::default();
    politechnika.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.45,
            skilled_ratio: 0.35,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 25.0),
                (Commodity::Chemicals, 15.0),
            ]),
            outputs: HashMap::from([(Commodity::InnovationPoints, 6.0)]),
            ..Default::default()
        },
    );
    politechnika.insert(MethodSlot::Automation, "Advanced".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.50,
            skilled_ratio: 0.30,
            basic_ratio: 0.20,
            efficiency: 1.6,
            inputs: HashMap::from([
                (Commodity::Paper, 30.0),
                (Commodity::Chemicals, 20.0),
                (Commodity::ElectronicComponents, 10.0),
                (Commodity::Software, 5.0),
            ]),
            outputs: HashMap::from([(Commodity::InnovationPoints, 10.0)]),
            ..Default::default()
        },
    );
    registry.insert("technical_university".to_string(), politechnika);

    registry
}

/// Builds Phase 7 healthcare production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// for hospitals that generate Health Capacity.
///
/// # Rules
/// * Hospitals consume Pharmaceuticals, MedicalEquipment as inputs
/// * Output is Health Capacity (Commodity::HealthCapacity)
/// * High Expert/Skilled labor requirement for medical services
/// * Physical Limits: No inputs = zero health capacity
pub fn healthcare_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- clinic (Clinic) --
    let mut przychodnia = BuildingMethods::default();
    przychodnia.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.50,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Pharmaceuticals, 5.0)]),
            outputs: HashMap::from([(Commodity::HealthCapacity, 10.0)]),
            ..Default::default()
        },
    );
    registry.insert("clinic".to_string(), przychodnia);

    // -- hospital (Hospital) --
    let mut szpital = BuildingMethods::default();
    szpital.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.30,
            skilled_ratio: 0.50,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Pharmaceuticals, 15.0),
                (Commodity::MedicalEquipment, 5.0),
            ]),
            outputs: HashMap::from([(Commodity::HealthCapacity, 30.0)]),
            ..Default::default()
        },
    );
    szpital.insert(MethodSlot::Automation, "Advanced".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.35,
            skilled_ratio: 0.45,
            basic_ratio: 0.20,
            efficiency: 1.4,
            inputs: HashMap::from([
                (Commodity::Pharmaceuticals, 20.0),
                (Commodity::MedicalEquipment, 10.0),
            ]),
            outputs: HashMap::from([(Commodity::HealthCapacity, 45.0)]),
            ..Default::default()
        },
    );
    registry.insert("hospital".to_string(), szpital);

    // -- hospital Badawczy (Research Hospital) --
    let mut szpital_badawczy = BuildingMethods::default();
    szpital_badawczy.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.40,
            skilled_ratio: 0.40,
            basic_ratio: 0.20,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Pharmaceuticals, 25.0),
                (Commodity::MedicalEquipment, 15.0),
                (Commodity::Chemicals, 10.0),
            ]),
            outputs: HashMap::from([(Commodity::HealthCapacity, 50.0)]),
            ..Default::default()
        },
    );
    registry.insert("research_hospital".to_string(), szpital_badawczy);

    registry
}

/// Builds Phase 7 education production-method registry.
///
/// # Returns
/// A map of `building kind -> { method name -> `[`ProductionMethod`]` }`,
/// for schools that generate Education Slots.
///
/// # Rules
/// * Schools consume Paper, OfficeSupplies as inputs
/// * Output is Education Slots (Commodity::EducationSlots)
/// * Moderate Skilled labor requirement for teaching
/// * Physical Limits: No inputs = zero education slots
pub fn education_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- primary_school (Primary School) --
    let mut szkola_podstawowa = BuildingMethods::default();
    szkola_podstawowa.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.60,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 10.0),
                (Commodity::OfficeMachinery, 2.0),
            ]),
            outputs: HashMap::from([(Commodity::EducationSlots, 20.0)]),
            ..Default::default()
        },
    );
    registry.insert("primary_school".to_string(), szkola_podstawowa);

    // -- high_school (High School) --
    let mut liceum = BuildingMethods::default();
    liceum.insert(MethodSlot::Production, "Basic".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.55,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Paper, 15.0),
                (Commodity::OfficeMachinery, 3.0),
            ]),
            outputs: HashMap::from([(Commodity::EducationSlots, 30.0)]),
            ..Default::default()
        },
    );
    liceum.insert(MethodSlot::Automation, "Advanced".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.50,
            basic_ratio: 0.30,
            efficiency: 1.3,
            inputs: HashMap::from([
                (Commodity::Paper, 20.0),
                (Commodity::OfficeMachinery, 5.0),
                (Commodity::ElectronicComponents, 2.0),
            ]),
            outputs: HashMap::from([(Commodity::EducationSlots, 40.0)]),
            ..Default::default()
        },
    );
    registry.insert("high_school".to_string(), liceum);

    registry
}

/// Builds the OSP (Volunteer Fire Brigade) production-method registry.
///
/// # Returns
/// A map with one building kind: "volunteer_fire_station" (volunteer firehouse).
///
/// # Rules
/// * OSP buildings are CommercialBuildings owned by NGO companies.
/// * They produce FireProtectionCapacity at lower efficiency than professional fire brigades.
/// * Volunteer labor is injected by `process_osp_volunteer_allocation()`.
/// * Low input costs — funded by community donations via Phase 13 charity.
pub fn osp_building_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    let mut remiza = BuildingMethods::default();
    remiza.insert(MethodSlot::Production, "Volunteer Station".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.20,
            basic_ratio: 0.75,
            efficiency: 1.0,
            inputs: HashMap::from([
                (Commodity::Water, 5.0),
            ]),
            outputs: HashMap::from([
                (Commodity::FireProtectionCapacity, 3.0),
            ]),
            ..Default::default()
        },
    );
    remiza.insert(MethodSlot::Production, "Motorized Station".to_string(),
        ProductionMethod {
            year: 1920,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.3,
            inputs: HashMap::from([
                (Commodity::Water, 8.0),
                (Commodity::Cars, 1.0),
            ]),
            outputs: HashMap::from([
                (Commodity::FireProtectionCapacity, 8.0),
            ]),
            ..Default::default()
        },
    );
    registry.insert("volunteer_fire_station".to_string(), remiza);

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_state_buildings_present() {
        let reg = state_building_methods();
        assert!(reg.contains_key("military_base"));
        assert!(reg.contains_key("police_station"));
        assert!(reg.contains_key("courthouse"));
        assert!(reg.contains_key("intelligence_hq"));
    }

    #[test]
    fn labor_ratios_sum_to_one() {
        let reg = state_building_methods();
        for (_, methods) in reg.iter() {
            for pm in methods.iter_all() {
                let sum = pm.experts_ratio + pm.skilled_ratio + pm.basic_ratio;
                assert!((sum - 1.0).abs() < 1e-9, "ratios must sum to 1.0");
            }
        }
    }

    #[test]
    fn state_buildings_have_no_outputs() {
        let reg = state_building_methods();
        for (_, methods) in reg.iter() {
            for pm in methods.iter_all() {
                // Phase 14: courthouse/police_station produce JusticeCapacity/SecurityCapacity;
                // prison Workshop produces Furniture; Quarry produces Stone/HardCoal.
                let allowed: Vec<&Commodity> = pm.outputs.keys()
                    .filter(|c| !matches!(c, Commodity::JusticeCapacity | Commodity::SecurityCapacity | Commodity::IntelligenceCapacity | Commodity::Furniture | Commodity::Stone | Commodity::HardCoal | Commodity::FireProtectionCapacity | Commodity::ShelterCapacity | Commodity::BorderEnforcementCapacity | Commodity::CustomsCapacity | Commodity::SanitaryInspectionCapacity | Commodity::BuildingInspectionCapacity | Commodity::EnvironmentalInspectionCapacity))
                    .collect();
                assert!(allowed.is_empty(), "unexpected outputs: {:?}", allowed);
            }
        }
    }

    #[test]
    fn base_military_inputs_match_python() {
        let reg = state_building_methods();
        let pm = reg["military_base"].get(MethodSlot::Production, "Basic").unwrap();
        assert_eq!(pm.inputs[&Commodity::Ammunition], 10.0);
        assert_eq!(pm.inputs[&Commodity::Food], 20.0);
        assert_eq!(pm.efficiency, 1.0);
    }
}

