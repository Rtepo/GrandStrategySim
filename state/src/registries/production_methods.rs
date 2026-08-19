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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionMethod {
    /// Earliest year this method may be adopted (`"rok"`).
    #[serde(rename = "rok")]
    pub year: u32,

    /// TechId required to unlock this method, if any
    /// (`"wymagana_technologia"`). Stores a stable TechId (e.g. `"steam_003"`),
    /// never a display name â€” i18n safe.
    #[serde(rename = "wymagana_technologia", default)]
    pub required_tech: Option<TechId>,

    /// Fraction of staff who are experts (`"eksperci"`).
    #[serde(rename = "eksperci")]
    pub experts_ratio: f64,

    /// Fraction of staff who are skilled workers (`"sredni"`).
    #[serde(rename = "sredni")]
    pub skilled_ratio: f64,

    /// Fraction of staff who are basic workers (`"szeregowi"`).
    #[serde(rename = "szeregowi")]
    pub basic_ratio: f64,

    /// Output multiplier of this method (`"wydajnosc"`).
    #[serde(rename = "wydajnosc")]
    pub efficiency: f64,

    /// Per-turn commodity inputs consumed (`"inputs"`).
    #[serde(rename = "inputs", default)]
    pub inputs: HashMap<Commodity, f64>,

    /// Per-turn commodity outputs produced (`"outputs"`). Empty for pure
    /// service/consumption buildings such as the state apparatus.
    #[serde(rename = "outputs", default)]
    pub outputs: HashMap<Commodity, f64>,
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
/// * Covers `Baza Wojskowa`, `Komisariat`, `SÄ…d`, and `Siedziba SĹ‚uĹĽb`.
/// * Labor ratios (`experts + skilled + basic`) sum to `1.0` for every method.
/// * State buildings produce no outputs; they consume inputs to deliver
///   intangible security/justice effects.
pub fn state_building_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- Baza Wojskowa (Military Base) --
    let mut baza = BuildingMethods::default();
    baza.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    baza.insert(MethodSlot::Production, "Zmechanizowana".to_string(),
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
        },
    );
    baza.insert(MethodSlot::Production, "Wspolczesna".to_string(),
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
        },
    );
    registry.insert("Baza Wojskowa".to_string(), baza);

    // -- Komisariat (Police Station) --
    let mut komis = BuildingMethods::default();
    komis.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    komis.insert(MethodSlot::Production, "Zmodernizowana".to_string(),
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
        },
    );
    komis.insert(MethodSlot::Production, "Cyfrowa".to_string(),
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
        },
    );
    komis.insert(MethodSlot::Production, "Zmilitaryzowana".to_string(),
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
        },
    );
    registry.insert("Komisariat".to_string(), komis);

    // -- SÄ…d (Courthouse) --
    let mut sad = BuildingMethods::default();
    sad.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    sad.insert(MethodSlot::Production, "Zmodernizowana".to_string(),
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
        },
    );
    sad.insert(MethodSlot::Production, "Cyfrowa".to_string(),
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
        },
    );
    registry.insert("SÄ…d".to_string(), sad);

    // -- Siedziba SĹ‚uĹĽb (Intelligence HQ) --
    let mut sluzby = BuildingMethods::default();
    sluzby.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    sluzby.insert(MethodSlot::Production, "Zmodernizowana".to_string(),
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
        },
    );
    sluzby.insert(MethodSlot::Production, "Wspolczesna".to_string(),
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
        },
    );
    registry.insert("Siedziba SĹ‚uĹĽb".to_string(), sluzby);

    // -- WiÄ™zienie (Prison) --
    // PMs vary by PrisonType. The active PM is selected based on the
    // country's PrisonLaborLaw.prison_type at runtime.
    // VoluntaryLabor and StatePenalColony produce goods via building inventory.
    // PrivateLaborCamps and IsolationCamp produce nothing â€” they operate
    // through the labor market phase instead (see economy/prison_labor.rs).
    let mut wiezienie = BuildingMethods::default();

    // VoluntaryLabor: workshop production
    wiezienie.insert(MethodSlot::Production, "Warsztat".to_string(),
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
        },
    );

    // StatePenalColony: forced heavy labor producing raw materials
    wiezienie.insert(MethodSlot::Production, "KamienioĹ‚om".to_string(),
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
        },
    );

    // PrivateLaborCamps: no building output â€” FTEs injected into labor market
    wiezienie.insert(MethodSlot::Production, "ObĂłz Pracy Prywatnej".to_string(),
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
        },
    );

    // IsolationCamp: no production â€” prisoners removed from workforce
    wiezienie.insert(MethodSlot::Production, "ObĂłz Odosobnienia".to_string(),
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
        },
    );

    registry.insert("WiÄ™zienie".to_string(), wiezienie);

    // -- StraĹĽ PoĹĽarna (Professional State Fire Brigade) --
    let mut straz = BuildingMethods::default();
    straz.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    straz.insert(MethodSlot::Production, "Zmotoryzowana".to_string(),
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
        },
    );
    straz.insert(MethodSlot::Production, "Zawodowa".to_string(),
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
        },
    );
    registry.insert("StraĹĽ PoĹĽarna".to_string(), straz);

    // -- Schron Przeciwpowodziowy (Flood Shelter / Levee) --
    let mut schron = BuildingMethods::default();
    schron.insert(MethodSlot::Production, "WaĹ‚ Przeciwpowodziowy".to_string(),
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
        },
    );
    schron.insert(MethodSlot::Production, "Zmodernizowany WaĹ‚".to_string(),
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
        },
    );
    registry.insert("Schron Przeciwpowodziowy".to_string(), schron);

    // -- StraĹĽ Graniczna (Border Guard) --
    let mut straz_gran = BuildingMethods::default();
    straz_gran.insert(MethodSlot::Production, "Patrol Graniczny".to_string(),
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
        },
    );
    straz_gran.insert(MethodSlot::Production, "Zmotoryzowany Patrol".to_string(),
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
        },
    );
    straz_gran.insert(MethodSlot::Production, "StraĹĽ Graniczna Nowoczesna".to_string(),
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
        },
    );
    registry.insert("StraĹĽ Graniczna".to_string(), straz_gran);

    // -- UrzÄ…d Celny (Customs House) --
    let mut urzad_cel = BuildingMethods::default();
    urzad_cel.insert(MethodSlot::Production, "Posterunek Celny".to_string(),
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
        },
    );
    urzad_cel.insert(MethodSlot::Production, "UrzÄ…d Celny Zmodernizowany".to_string(),
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
        },
    );
    urzad_cel.insert(MethodSlot::Production, "System Celny e-Toll".to_string(),
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
        },
    );
    registry.insert("UrzÄ…d Celny".to_string(), urzad_cel);

    // -- Sanepid (Sanitary Inspectorate) --
    let mut sanepid = BuildingMethods::default();
    sanepid.insert(MethodSlot::Production, "Stacja Sanitarna".to_string(),
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
        },
    );
    sanepid.insert(MethodSlot::Production, "Zmodernizowany Sanepid".to_string(),
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
        },
    );
    registry.insert("Sanepid".to_string(), sanepid);

    // -- Inspektorat Nadzoru Budowlanego (Building Inspectorate) --
    let mut insp_bud = BuildingMethods::default();
    insp_bud.insert(MethodSlot::Production, "UrzÄ…d Nadzoru".to_string(),
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
        },
    );
    insp_bud.insert(MethodSlot::Production, "Zmodernizowany Inspektorat".to_string(),
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
        },
    );
    registry.insert("Inspektorat Nadzoru Budowlanego".to_string(), insp_bud);

    // -- Inspektorat Ochrony Ĺšrodowiska (Environmental Inspectorate) --
    let mut insp_srod = BuildingMethods::default();
    insp_srod.insert(MethodSlot::Production, "Stacja Kontroli".to_string(),
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
        },
    );
    registry.insert("Inspektorat Ochrony Ĺšrodowiska".to_string(), insp_srod);

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
    solvay.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    registry.insert("ZakĹ‚ad Solvaya".to_string(), solvay);

    // -- Seed Mill --
    let mut seed_mill = BuildingMethods::default();
    seed_mill.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Cereal, 1.5), (Commodity::Protein, 0.5)]),
            outputs: HashMap::from([(Commodity::Seeds, 1.0)]),
        },
    );
    registry.insert("MĹ‚yn Nasienny".to_string(), seed_mill);

    // -- StateForest (Forest District â€” commercial building owned by State Forests company) --
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
        },
    );
    registry.insert("StateForest".to_string(), state_forest_methods);

    // -- Phase 39: Statecraft Buildings --
    // Each new building type has a production method that consumes inputs
    // and produces outputs relevant to its ministry competency.

    // Court â€” Justice ministry: produces ProsecutionCapacity (service output)
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
        },
    );
    registry.insert("Court".to_string(), court_methods);

    // CustomsOffice â€” Treasury: facilitates tariff collection
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
        },
    );
    registry.insert("CustomsOffice".to_string(), customs_methods);

    // Embassy â€” Foreign Affairs: diplomatic output
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
        },
    );
    registry.insert("Embassy".to_string(), embassy_methods);

    // ResearchInstitute â€” Science: produces ResearchOutput
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
        },
    );
    registry.insert("ResearchInstitute".to_string(), research_methods);

    // LaborInspectorate â€” Labor: enforces labor regulations
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
        },
    );
    registry.insert("LaborInspectorate".to_string(), labor_inspectorate_methods);

    // PublicWorksSite â€” Labor: public employment program
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
        },
    );
    registry.insert("PublicWorksSite".to_string(), public_works_methods);

    // NationalTheater â€” Culture: produces CulturalOutput
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
        },
    );
    registry.insert("NationalTheater".to_string(), theater_methods);

    // NationalLibrary â€” Culture: produces CulturalOutput (knowledge)
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
        },
    );
    registry.insert("NationalLibrary".to_string(), library_methods);

    // TransportDepot â€” Transport: public transport hub
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
        },
    );
    registry.insert("TransportDepot".to_string(), transport_depot_methods);

    // -- Phase 17C: Monastery/Temple Production Methods --
    // These are used by CulturalBuilding (not Building), linked via production_method field.
    // Revenue credits the owning company via TransferSettler, not building.available_cash.

    // monastery_wine_production â€” consumes Fruit, produces Luxury (wine proxy)
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
        },
    );
    registry.insert("monastery_wine_production".to_string(), monastery_wine);

    // monastery_scriptorium â€” consumes Paper, produces ReligiousTexts
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
        },
    );
    registry.insert("monastery_scriptorium".to_string(), monastery_scriptorium);

    // monastery_workshop â€” consumes Wood/Furniture, produces LuxuryFurniture + ReligiousArt
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
        },
    );
    registry.insert("monastery_workshop".to_string(), monastery_workshop);

    // temple_artisan_workshop â€” consumes LuxuryFurniture, produces ReligiousArt
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
        },
    );
    registry.insert("temple_artisan_workshop".to_string(), temple_artisan);

    // monastery_herbal_garden â€” consumes Seeds, produces Pharmaceuticals
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
/// * Covers `Targ`, `Hurtownia`, `Sklep Detaliczny`, `Supermarket`, `Dom Towarowy`, `Centrum Handlowe`.
/// * Retail buildings consume no inputs; they provide service capacity for B2C clearing.
/// * Labor ratios (`experts + skilled + basic`) sum to `1.0` for every method.
pub fn retail_production_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    // -- Targ (Marketplace) --
    let mut targ = BuildingMethods::default();
    targ.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.15,
            basic_ratio: 0.80,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Targ".to_string(), targ);

    // -- Hurtownia (Wholesaler) --
    let mut hurtownia = BuildingMethods::default();
    hurtownia.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        },
    );
    hurtownia.insert(MethodSlot::Automation, "Zmechanizowana".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.3,
            inputs: HashMap::from([(Commodity::Fuels, 5.0)]),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Hurtownia".to_string(), hurtownia);

    // -- Sklep Detaliczny (Retail Store) --
    let mut sklep = BuildingMethods::default();
    sklep.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.05,
            skilled_ratio: 0.20,
            basic_ratio: 0.75,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Sklep Detaliczny".to_string(), sklep);

    // -- Supermarket --
    let mut supermarket = BuildingMethods::default();
    supermarket.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1950,
            required_tech: None,
            experts_ratio: 0.10,
            skilled_ratio: 0.30,
            basic_ratio: 0.60,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Energy, 10.0)]),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Supermarket".to_string(), supermarket);

    // -- Dom Towarowy (Department Store) --
    let mut dom_towarowy = BuildingMethods::default();
    dom_towarowy.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1900,
            required_tech: None,
            experts_ratio: 0.15,
            skilled_ratio: 0.35,
            basic_ratio: 0.50,
            efficiency: 1.0,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Dom Towarowy".to_string(), dom_towarowy);

    // -- Centrum Handlowe (Shopping Center) --
    let mut centrum = BuildingMethods::default();
    centrum.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1970,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.40,
            basic_ratio: 0.40,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Energy, 50.0)]),
            outputs: HashMap::new(),
        },
    );
    registry.insert("Centrum Handlowe".to_string(), centrum);

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

    // -- Uniwersytet (University) --
    let mut uniwersytet = BuildingMethods::default();
    uniwersytet.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    uniwersytet.insert(MethodSlot::Automation, "Zmechanizowana".to_string(),
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
        },
    );
    registry.insert("Uniwersytet".to_string(), uniwersytet);

    // -- Politechnika (Polytechnic) --
    let mut politechnika = BuildingMethods::default();
    politechnika.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    politechnika.insert(MethodSlot::Automation, "Zaawansowana".to_string(),
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
        },
    );
    registry.insert("Politechnika".to_string(), politechnika);

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

    // -- Przychodnia (Clinic) --
    let mut przychodnia = BuildingMethods::default();
    przychodnia.insert(MethodSlot::Production, "Podstawowa".to_string(),
        ProductionMethod {
            year: 1850,
            required_tech: None,
            experts_ratio: 0.20,
            skilled_ratio: 0.50,
            basic_ratio: 0.30,
            efficiency: 1.0,
            inputs: HashMap::from([(Commodity::Pharmaceuticals, 5.0)]),
            outputs: HashMap::from([(Commodity::HealthCapacity, 10.0)]),
        },
    );
    registry.insert("Przychodnia".to_string(), przychodnia);

    // -- Szpital (Hospital) --
    let mut szpital = BuildingMethods::default();
    szpital.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    szpital.insert(MethodSlot::Automation, "Zaawansowana".to_string(),
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
        },
    );
    registry.insert("Szpital".to_string(), szpital);

    // -- Szpital Badawczy (Research Hospital) --
    let mut szpital_badawczy = BuildingMethods::default();
    szpital_badawczy.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    registry.insert("Szpital Badawczy".to_string(), szpital_badawczy);

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

    // -- SzkoĹ‚a Podstawowa (Primary School) --
    let mut szkola_podstawowa = BuildingMethods::default();
    szkola_podstawowa.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    registry.insert("SzkoĹ‚a Podstawowa".to_string(), szkola_podstawowa);

    // -- Liceum (High School) --
    let mut liceum = BuildingMethods::default();
    liceum.insert(MethodSlot::Production, "Podstawowa".to_string(),
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
        },
    );
    liceum.insert(MethodSlot::Automation, "Zaawansowana".to_string(),
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
        },
    );
    registry.insert("Liceum".to_string(), liceum);

    registry
}

/// Builds the OSP (Volunteer Fire Brigade) production-method registry.
///
/// # Returns
/// A map with one building kind: "Remiza OSP" (volunteer firehouse).
///
/// # Rules
/// * OSP buildings are CommercialBuildings owned by NGO companies.
/// * They produce FireProtectionCapacity at lower efficiency than professional fire brigades.
/// * Volunteer labor is injected by `process_osp_volunteer_allocation()`.
/// * Low input costs â€” funded by community donations via Phase 13 charity.
pub fn osp_building_methods() -> HashMap<String, BuildingMethods> {
    let mut registry: HashMap<String, BuildingMethods> = HashMap::new();

    let mut remiza = BuildingMethods::default();
    remiza.insert(MethodSlot::Production, "Remiza Ochotnicza".to_string(),
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
        },
    );
    remiza.insert(MethodSlot::Production, "Zmotoryzowana Remiza".to_string(),
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
        },
    );
    registry.insert("Remiza OSP".to_string(), remiza);

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_state_buildings_present() {
        let reg = state_building_methods();
        assert!(reg.contains_key("Baza Wojskowa"));
        assert!(reg.contains_key("Komisariat"));
        assert!(reg.contains_key("SÄ…d"));
        assert!(reg.contains_key("Siedziba SĹ‚uĹĽb"));
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
                // Phase 14: SÄ…d/Komisariat produce JusticeCapacity/SecurityCapacity;
                // WiÄ™zienie Warsztat produces Furniture; KamienioĹ‚om produces Stone/HardCoal.
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
        let pm = reg["Baza Wojskowa"].get(MethodSlot::Production, "Podstawowa").unwrap();
        assert_eq!(pm.inputs[&Commodity::Ammunition], 10.0);
        assert_eq!(pm.inputs[&Commodity::Food], 20.0);
        assert_eq!(pm.efficiency, 1.0);
    }
}

