//! Categorical enums that replace Python's stringly-typed dictionary keys.
//!
//! Each enum uses `#[serde(rename = "...")]` to (de)serialize verbatim against
//! the existing Polish-keyed JSON data, enabling Golden-master parity while
//! giving Rust code exhaustive, compiler-checked `match` handling.

use serde::{Deserialize, Serialize};

/// Broad regime classification derived from a government form's `typ` field.
///
/// # Rules
/// * Used by black-ops funding logic: democracies siphon a fixed 2% of
///   defense/security allocations, autocracies draw from additional sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegimeType {
    /// Democratic regime (`"democracy"`).
    Democracy,
    /// Authoritarian regime (`"autocracy"`).
    Autocracy,
}

impl RegimeType {
    /// Returns whether this regime is democratic.
    ///
    /// # Returns
    /// `true` for [`RegimeType::Democracy`], `false` otherwise.
    ///
    /// # Rules
    /// * Direct replacement for the Python `is_democratic` helper.
    pub fn is_democratic(self) -> bool {
        matches!(self, RegimeType::Democracy)
    }
}

/// Conscription policy (`obowiazkowa_sluzba` in `prawo_wojskowe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConscriptionLaw {
    /// Mandatory training only (`"obowiazkowe_szkolenia"`).
    #[serde(rename = "obowiazkowe_szkolenia")]
    MandatoryTraining,
    /// Full active service (`"pełna_służba"`).
    #[serde(rename = "pełna_służba")]
    FullService,
    /// No mandatory service (`"brak"`).
    #[serde(rename = "brak")]
    None_,
}

/// Policy on women serving in the armed forces (`kobiety_w_armii`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WomenInArmy {
    /// Reserve duty only (`"jedynie_w_rezerwie"`).
    #[serde(rename = "jedynie_w_rezerwie")]
    ReserveOnly,
    /// Full access to all roles (`"pełny_dostęp"`).
    #[serde(rename = "pełny_dostęp")]
    FullAccess,
    /// Barred from service (`"zakaz"`).
    #[serde(rename = "zakaz")]
    Banned,
}

/// Scope of military draft (`zakres_poboru`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DraftScope {
    /// Voluntary enlistment only (`"dobrowolna"`).
    #[serde(rename = "dobrowolna")]
    Voluntary,
    /// Selective draft (`"selektywny"`).
    #[serde(rename = "selektywny")]
    Selective,
    /// Universal conscription (`"powszechny_pobór"`).
    #[serde(rename = "powszechny_pobór")]
    UniversalDraft,
}

/// Workforce skill tier used by the labor market.
///
/// # Rules
/// * The Python engine uses the keys `eksperci`, `sredni`, and `szeregowi`
///   inside building production methods and labor-market calculations.
/// * The variants are ordered so they can be used as `Map` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaborTier {
    /// Experts / highly educated workforce (`"expert"`).
    Expert,
    /// Skilled / middle-tier workforce (`"skilled"`).
    Skilled,
    /// Unskilled / general workforce (`"unskilled"`).
    Unskilled,
}

/// National prosperity bracket (`koszyk` / `koszyk_zamożności`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WealthBracket {
    /// `"very_high"`.
    VeryHigh,
    /// `"high"`.
    High,
    /// `"medium"`.
    Medium,
    /// `"low"`.
    Low,
}

impl Default for WealthBracket {
    fn default() -> Self {
        WealthBracket::Medium
    }
}

/// Macroeconomic sector (`sektor_pkb` and keys of `sektory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sector {
    /// `"mining"` (was: sektor_wydobywczy).
    #[default]
    Mining,
    /// `"agriculture"` (was: rolnictwo).
    Agriculture,
    /// `"heavy_industry"` (was: przemysł_ciężki).
    HeavyIndustry,
    /// `"light_industry"` (was: przemysł_lekki).
    LightIndustry,
    /// `"armaments_industry"` (was: przemysł_zbrojeniowy).
    ArmamentsIndustry,
    /// `"local_services"` (was: usługi_lokalne).
    LocalServices,
    /// `"export_services"` (was: usługi_eksportowe).
    ExportServices,
    /// `"construction"` (was: budownictwo).
    Construction,
    /// `"energy"` (was: energetyka).
    Energy,
    /// `"public_services"` (was: usługi_publiczne).
    PublicServices,
    /// `"medical_services"` (was: usługi_medyczne).
    MedicalServices,
    /// `"educational_services"` (was: usługi_edukacyjne).
    EducationalServices,
    /// `"transport_logistics"` (was: transport_i_logistyka).
    TransportLogistics,
    // STAGE C: Public Administration sector for Tax Offices
    /// `"public_administration"` (was: administracja_publiczna).
    PublicAdministration,
    // STAGE D PHASE 2: Banking and financial services sector
    /// `"banking"` (was: sektor_bankowy).
    Banking,
    // PHASE 3: Media and entertainment sector
    /// `"media_and_entertainment"` (radio, TV, publishing, opera, music academies).
    #[serde(rename = "media_and_entertainment")]
    MediaAndEntertainment,
    /// `"waste_management"` (was: gospodarka_odpadami).
    WasteManagement,
    /// `"hospitality"` (hotels, resorts, restaurants, casinos).
    Hospitality,
    /// `"ngo"` (non-governmental organizations, charities).
    NGO,
    /// `"religion"` (churches, religious charities, religious institutions).
    Religion,
    /// PHASE 19B: `"maintenance_workshops"` (repair shops producing MaintenanceServices
    /// from generic raw materials — the circular-dependency-breaking maintenance sector).
    MaintenanceWorkshops,
    /// PHASE 32: `"government"` — Parliament, government buildings, ministerial offices.
    /// Employs politicians and administrative staff, consumes Paper/Energy/Services,
    /// and is funded from Treasury payroll.
    Government,
}

impl Sector {
    /// Get localized display name for UI.
    pub fn display_name(&self) -> String {
        let key = match self {
            Sector::Mining => "mining",
            Sector::Agriculture => "agriculture",
            Sector::HeavyIndustry => "heavy_industry",
            Sector::LightIndustry => "light_industry",
            Sector::ArmamentsIndustry => "armaments_industry",
            Sector::LocalServices => "local_services",
            Sector::ExportServices => "export_services",
            Sector::Construction => "construction",
            Sector::Energy => "energy",
            Sector::PublicServices => "public_services",
            Sector::MedicalServices => "medical_services",
            Sector::EducationalServices => "educational_services",
            Sector::TransportLogistics => "transport_logistics",
            Sector::PublicAdministration => "public_administration",
            Sector::Banking => "banking",
            Sector::MediaAndEntertainment => "media_and_entertainment",
            Sector::WasteManagement => "waste_management",
            Sector::Hospitality => "hospitality",
            Sector::NGO => "ngo",
            Sector::Religion => "religion",
            Sector::MaintenanceWorkshops => "maintenance_workshops",
            Sector::Government => "government",
        };
        crate::i18n::i18n().sector(key)
    }

    /// Phase 29: Map this sector to the set of commodities it primarily produces.
    ///
    /// Used by the PMI diffusion index to filter OrderBook bids/asks and
    /// settled trades by sector.
    pub fn primary_commodities(&self) -> Vec<Commodity> {
        match self {
            Sector::Mining => vec![
                Commodity::BrownCoal, Commodity::HardCoal, Commodity::Iron,
                Commodity::Copper, Commodity::Gold, Commodity::Silver,
                Commodity::Uranium,
            ],
            Sector::Agriculture => vec![
                Commodity::Cereal, Commodity::Meat,
            ],
            Sector::HeavyIndustry => vec![
                Commodity::Steel, Commodity::IndustrialMachinery,
                Commodity::Cement,
            ],
            Sector::LightIndustry => vec![
                Commodity::Clothing, Commodity::LuxuryClothing,
                Commodity::Furniture, Commodity::Paper,
                Commodity::Glass,
            ],
            Sector::ArmamentsIndustry => vec![
                Commodity::Ammunition, Commodity::TowedArtillery,
                Commodity::Trucks, Commodity::Steel,
            ],
            Sector::Construction => vec![
                Commodity::ConstructionMachinery, Commodity::Cement,
                Commodity::Steel, Commodity::Asphalt,
            ],
            Sector::Energy => vec![
                Commodity::Energy, Commodity::BrownCoal, Commodity::HardCoal,
            ],
            Sector::TransportLogistics => vec![
                Commodity::FreightCapacity, Commodity::Trucks,
            ],
            Sector::LocalServices => vec![],
            Sector::ExportServices => vec![Commodity::FreightCapacity],
            Sector::PublicServices => vec![Commodity::OfficeMachinery],
            Sector::MedicalServices => vec![Commodity::MedicalEquipment],
            Sector::EducationalServices => vec![Commodity::ReligiousTexts],
            Sector::PublicAdministration => vec![Commodity::OfficeMachinery],
            Sector::Banking => vec![Commodity::OfficeMachinery],
            Sector::MediaAndEntertainment => vec![Commodity::ReligiousTexts],
            Sector::WasteManagement => vec![],
            Sector::Hospitality => vec![],
            Sector::NGO => vec![],
            Sector::Religion => vec![Commodity::ReligiousTexts],
            Sector::MaintenanceWorkshops => vec![Commodity::MaintenanceServices],
            // Phase 32: Government sector doesn't produce tradeable commodities;
            // it consumes Paper, Energy, and Services from the market.
            Sector::Government => vec![],
        }
    }
}

/// Tradeable commodity consumed or produced by production methods.
///
/// # Rules
/// * This enum covers every good listed in `market.json` and every input/output
///   good used in `spatial_registry` building recipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Commodity {
    /// "agd" (was: AGD).
    Agd,
    /// "aluminum" (was: Aluminium).
    Aluminum,
    /// "ammunition" (was: Amunicja).
    Ammunition,
    /// "towed_artillery" (was: Artyleria Holowana).
    TowedArtillery,
    /// "mobile_artillery" (was: Artyleria Mobilna).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    MobileArtillery,
    /// "anti_aircraft_artillery" (was: Artyleria Przeciwlotnicza).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    AntiAircraftArtillery,
    /// "asphalt" (was: Asfalt).
    Asphalt,
    /// "bitumen" (was: Bitumin).
    Bitumen,
    /// "infantry_fighting_vehicles" (was: Bojowe Wozy Piechoty).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    InfantryFightingVehicles,
    /// "bauxite" (was: Boksyt).
    Bauxite,
    /// "batteries" — Phase 20: Energy storage for EVs, electronics, grid storage.
    Batteries,
    /// "bombers" (was: Bombowce).
    Bombers,
    /// "bricks" (was: Cegły).
    Bricks,
    /// "cement" (was: Cement).
    Cement,
    /// "trucks" (was: Ciężarówki).
    Trucks,
    /// "military_trucks" (was: Ciężarówki Wojskowe).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    MilitaryTrucks,
    /// "tin" (was: Cyna).
    Tin,
    /// "zinc" (was: Cynk).
    Zinc,
    /// "heavy_tanks" (was: Czołgi Ciężkie).
    HeavyTanks,
    /// "light_tanks" (was: Czołgi Lekkie).
    LightTanks,
    /// "lithium" — Phase 20: Battery feedstock; mined from brines/hard rock.
    Lithium,
    /// "medium_tanks" (was: Czołgi Średnie).
    MediumTanks,
    /// "electronic_components" (was: Części Elektroniczne).
    ElectronicComponents,
    /// "mechanical_components" (was: Części Mechaniczne).
    MechanicalComponents,
    /// "planks" (was: Deski).
    Planks,
    /// "timber" (was: Drewno).
    Timber,
    /// "energy" (was: Energia).
    Energy,
    /// "frigates" (was: Fregaty).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Frigates,
    /// "natural_gas" (was: Gaz Ziemny).
    NaturalGas,
    /// "clay" (was: Glina).
    Clay,
    /// "helicopters" (was: Helikoptery).
    Helicopters,
    /// "stone" (was: Kamień).
    Stone,
    /// "rifles" (was: Karabiny).
    Rifles,
    /// "catalysts" (was: Katalizatory).
    Catalysts,
    /// "coke" (was: Koks).
    Coke,
    /// "silicon" (was: Krzem).
    Silicon,
    /// "cruisers" (was: Krążowniki).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Cruisers,
    /// "aircraft_carriers" (was: Lotniskowce).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    AircraftCarriers,
    /// "magnesium" (was: Magnez).
    Magnesium,
    /// "office_machinery" (was: Maszyny Biurowe).
    OfficeMachinery,
    /// "construction_machinery" (was: Maszyny Budowlane).
    ConstructionMachinery,
    /// "industrial_machinery" (was: Maszyny Przemysłowe).
    IndustrialMachinery,
    /// "agricultural_machinery" (was: Maszyny Rolne).
    AgriculturalMachinery,
    /// "furniture" (was: Meble).
    Furniture,
    /// "luxury_furniture" (was: Meble Luksusowe).
    LuxuryFurniture,
    /// "copper" (was: Miedź).
    Copper,
    /// "meat" (was: Mięso).
    Meat,
    /// "fighters" (was: Myśliwce).
    Fighters,
    /// "fertilizers" (was: Nawozy).
    Fertilizers,
    /// "destroyers" (was: Niszczyciele).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Destroyers,
    /// "software" (was: Oprogramowanie).
    Software,
    /// "fruit" (was: Owoce).
    Fruit,
    /// "lead" (was: Ołów).
    Lead,
    /// "fuels" (was: Paliwa).
    Fuels,
    /// "battleships" (was: Pancerniki).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Battleships,
    /// "paper" (was: Papier).
    Paper,
    /// "sand" (was: Piasek).
    Sand,
    /// "pistols" (was: Pistolety).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Pistols,
    /// "plastics" — Phase 20: Oil-derived polymer; input for Agd, Cars, packaging.
    Plastics,
    /// "trains" (was: Pociągi).
    Trains,
    /// "prefabricates" (was: Prefabrykaty).
    Prefabricates,
    /// "gunpowder" (was: Proch).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Gunpowder,
    /// "food" (was: Produkty Spożywcze).
    Food,
    /// "radio" (was: Radio).
    Radio,
    /// "rare_earth_elements" — Phase 20: Neodymium, dysprosium etc.; input for semiconductors, magnets.
    RareEarthElements,
    /// "oil" (was: Ropa Naftowa).
    Oil,
    /// "fish" (was: Ryby).
    Fish,
    /// "cars" (was: Samochody).
    Cars,
    /// "airplanes" (was: Samoloty).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    Airplanes,
    /// "sulfur" (was: Siarka).
    Sulfur,
    /// "support_equipment" (was: Sprzęt Wsparcia).
    SupportEquipment,
    /// "silver" (was: Srebro).
    Silver,
    /// "steel" (was: Stal).
    Steel,
    /// "passenger_ships" (was: Statki Pasażerskie).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    PassengerShips,
    /// "cargo_ships" (was: Statki Towarowe).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    CargoShips,
    /// "naval_vessels" (was: Statki Wojskowe).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    NavalVessels,
    /// "mineral_resources" (was: Surowce Mineralne).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    MineralResources,
    /// "glass" (was: Szkło).
    Glass,
    /// "salt" (was: Sól).
    Salt,
    /// "rolling_stock" (was: Tabór Kolejowy).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    RollingStock,
    /// "televisions" (was: Telewizory).
    Televisions,
    /// "peat" (was: Torf).
    Peat,
    /// "passenger_transport" (was: Transport Pasażerski).
    PassengerTransport,
    /// "clothing" (was: Ubrania).
    Clothing,
    /// "luxury_clothing" (was: Ubrania Luksusowe).
    LuxuryClothing,
    /// "administrative_services" (was: Usługi Administracyjne).
    AdministrativeServices,
    /// "banking_services" (was: Usługi Bankowe).
    BankingServices,
    /// "construction_services" (was: Usługi Budowlane).
    ConstructionServices,
    /// "maintenance_services" (was: Usługi Konserwacyjne).
    MaintenanceServices,
    /// "local_services" (was: Usługi Lokalne).
    #[serde(rename = "local_services")]
    LocalServicesCommodity,
    /// "insurance_services" (was: Usługi Ubezpieczeniowe).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    InsuranceServices,
    /// "limestone" (was: Wapień).
    Limestone,
    /// "cereal" - Grain crops (wheat, corn, rice, barley) - Phase 6.3.5
    #[serde(alias = "grains")]
    Cereal,
    /// "vegetable" - Root and leaf vegetables (potatoes, carrots, tomatoes) - Phase 6.3.5
    #[serde(alias = "vegetables")]
    Vegetable,
    /// "protein" - Legumes and oilseeds (beans, soybeans, peas) - Phase 6.3.5
    Protein,
    /// "fodder" - Animal feed crops (alfalfa, clover, beet pulp) - Phase 6.3.5
    Fodder,
    /// "industrial_fiber" - Textile and industrial raw materials (cotton, flax, hemp) - Phase 6.3.5
    IndustrialFiber,
    /// "chemicals" - General chemical inputs - Phase 6.4
    Chemicals,
    /// "seeds" - Agricultural seed inputs - Phase 6.4
    Seeds,
    /// "semiconductors" — Phase 20: Silicon-based ICs; input for ElectronicComponents, solar.
    Semiconductors,
    /// "soda_ash" - Solvay process output - Phase 6.4
    SodaAsh,
    /// "ammonia" - Solvay process input - Phase 6.4
    Ammonia,
    /// "luxury" - High-value processed crops (sugar, coffee, tobacco, spices) - Phase 6.3.5
    Luxury,
    /// "water" (was: Woda).
    Water,
    /// "hydrogen" (was: Wodór).
    Hydrogen,
    /// "brown_coal" (was: Węgiel Brunatny).
    BrownCoal,
    /// "hard_coal" (was: Węgiel Kamienny).
    HardCoal,
    /// "fibers" (was: Włókna).
    Fibers,
    /// "gold" (was: Złoto).
    Gold,
    /// "submarines" (was: Łodzie Podwodne).
    Submarines,
    /// "iron" (was: Żelazo).
    Iron,
    /// "gravel" (was: Żwir).
    Gravel,
    /// "livestock" (was: Żywiec).
    Livestock,
    /// "market_research" (was: Analizy Rynkowe).
    #[deprecated(note = "Phase 20: no producer or consumer — use is_active() filter")]
    MarketResearch,
    /// "renovation_services" (was: Usługi Remontowe).
    RenovationServices,
    /// "innovation_points" - University output for state research (Phase 7).
    InnovationPoints,
    /// "health_capacity" - Hospital output for B2C trading (Phase 7).
    HealthCapacity,
    /// "education_slots" - School output for B2C trading (Phase 7).
    EducationSlots,
    /// "pharmaceuticals" - Medical drugs and medicines (Phase 7).
    Pharmaceuticals,
    /// "medical_equipment" - Medical devices and equipment (Phase 7).
    MedicalEquipment,
    /// "heat" - District heating commodity (Phase 8).
    Heat,
    /// "justice_capacity" - Justice service capacity produced by courthouses (Phase 14).
    JusticeCapacity,
    /// "security_capacity" - Security service capacity produced by police stations (Phase 14).
    SecurityCapacity,
    /// "intelligence_capacity" - Intelligence capacity produced by intelligence HQ (Phase 14.5).
    IntelligenceCapacity,
    /// "fire_protection_capacity" - Fire protection capacity produced by fire brigades (Phase 15A).
    FireProtectionCapacity,
    /// "shelter_capacity" - Flood shelter capacity produced by levees (Phase 15A).
    ShelterCapacity,
    /// "border_enforcement_capacity" - Border enforcement capacity produced by border guard buildings (Phase 15B).
    BorderEnforcementCapacity,
    /// "customs_capacity" - Customs inspection capacity produced by customs house buildings (Phase 15B).
    CustomsCapacity,
    /// "sanitary_inspection_capacity" - Sanitary inspection capacity produced by Sanepid buildings (Phase 15C).
    SanitaryInspectionCapacity,
    /// "building_inspection_capacity" - Building inspection capacity produced by Building Inspectorate (Phase 15C).
    BuildingInspectionCapacity,
    /// "environmental_inspection_capacity" - Environmental inspection capacity produced by Environmental Inspectorate (Phase 15C).
    EnvironmentalInspectionCapacity,
    /// "labor_inspection_capacity" - PIP (Państwowa Inspekcja Pracy) capacity (Phase 22C).
    LaborInspectionCapacity,
    /// "assimilation_capacity" - Assimilation capacity produced by Integration Centers (Phase 17B).
    AssimilationCapacity,
    /// "religious_texts" - Books/scriptures produced by monasteries (Phase 17C).
    ReligiousTexts,
    /// "refined_fuel" — Phase 20: High-grade distillate from crude oil.
    RefinedFuel,
    /// "religious_art" - Icons, sculptures, ritual objects produced by temples (Phase 17C).
    ReligiousArt,
    /// "information" - Media/information service for B2C consumption (Phase 18C).
    Information,
    /// "uranium" — Phase 21A: Nuclear fuel feedstock, mined from rift-valley deposits.
    Uranium,
    /// "freight_capacity" - B2B freight transport service (Phase 23A).
    /// Ephemeral service commodity (like MaintenanceServices): produced and
    /// consumed in the same turn, never stockpiled. Required to move goods
    /// between regions via the freight procurement gate.
    FreightCapacity,
    /// "draft_animals" - Oxen/horses/mules as fixed-asset cohorts (Phase 23A).
    /// A fixed asset (is_fixed_asset) installed as a FixedAssetCohort; used for
    /// early-game transport and agriculture. Maintained with Fodder + Water
    /// instead of MaintenanceServices.
    DraftAnimals,
}

impl std::fmt::Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self)
            .unwrap_or_else(|_| format!("\"{:?}\"", self));
        let trimmed = s.trim_matches('"');
        f.write_str(trimmed)
    }
}

impl Commodity {
    /// Returns the inventory key for this commodity.
    ///
    /// # Returns
    /// * `String` — the serde JSON key used for inventory storage.
    ///
    /// # Rules
    /// * Direct replacement for `format!("{:?}", commodity)` in inventory code.
    /// * Uses the serde `rename` mapping to ensure Polish-keyed compatibility.
    pub fn inventory_key(&self) -> String {
        String::from(*self)
    }

    /// Phase 41: Returns the VAT category for this commodity.
    ///
    /// Maps each commodity to one of three VAT categories:
    /// - `"agriculture"` — food and agricultural products (typically lower VAT)
    /// - `"industry"` — industrial goods, construction, energy, mining (standard VAT)
    /// - `"services"` — services, software, maintenance, transport (standard VAT)
    ///
    /// The B2C clearing uses this to dynamically look up the active VAT rate
    /// from `country.tax_rates.vat` for the commodity's category.
    /// Default fallback for unknown commodities: `"industry"` (highest rate).
    pub fn vat_category(&self) -> &'static str {
        use Commodity::*;
        match self {
            // Agricultural products — food, crops, livestock
            Meat | Fruit | Cereal | Vegetable | Protein | Fodder | IndustrialFiber
            | Luxury | Livestock | Fish | Food | Seeds => "agriculture",

            // Services — intangible, labor-based
            Software | AdministrativeServices | BankingServices | ConstructionServices
            | MaintenanceServices | LocalServicesCommodity | PassengerTransport
            | RenovationServices | Information | FreightCapacity | InnovationPoints
            | HealthCapacity | EducationSlots | JusticeCapacity | SecurityCapacity
            | IntelligenceCapacity | FireProtectionCapacity | ShelterCapacity
            | BorderEnforcementCapacity | CustomsCapacity | SanitaryInspectionCapacity
            | BuildingInspectionCapacity | EnvironmentalInspectionCapacity
            | LaborInspectionCapacity | AssimilationCapacity => "services",

            // Industrial goods — everything else (mining, manufacturing, energy, military)
            // This is the default/safest category for treasury revenue.
            _ => "industry",
        }
    }

    /// Phase 19B: Returns `true` if this commodity is a durable fixed-asset
    /// that, when bought B2B by a company, is installed as a `FixedAssetCohort`
    /// instead of being consumed as a per-turn input.
    ///
    /// # Rules
    /// * Machinery + vehicles: {IndustrialMachinery, ConstructionMachinery,
    ///   AgriculturalMachinery, OfficeMachinery, Trucks, Cars}.
    /// * Cars/Trucks are *also* quality consumer durables (see `is_quality_durable`)
    ///   — their role is determined by the transaction channel (B2B asset vs
    ///   B2C durable), not the commodity alone.
    /// * Phase 23A: DraftAnimals are fixed assets (installed as cohorts) but
    ///   are maintained with Fodder + Water, not MaintenanceServices.
    /// * Phase 45: Trains added as fixed assets for logistics capital.
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
                | Commodity::Trains
        )
    }

    /// Phase 19C: Returns `true` if this commodity is a quality consumer
    /// durable that, when sold B2C, carries blueprint quality and is tracked
    /// in `InventoryCohort`s rather than as a flat aggregate quantity.
    ///
    /// # Rules
    /// * Includes the durables list {Cars, Agd, Televisions, Radio, Furniture,
    ///   LuxuryFurniture, Clothing, LuxuryClothing}.
    /// * Cars overlap with `is_fixed_asset` (channel-dependent role).
    pub fn is_quality_durable(&self) -> bool {
        matches!(
            self,
            Commodity::Cars
                | Commodity::Agd
                | Commodity::Televisions
                | Commodity::Radio
                | Commodity::Furniture
                | Commodity::LuxuryFurniture
                | Commodity::Clothing
                | Commodity::LuxuryClothing
        )
    }

    /// Phase 47: Returns `true` if this commodity is a household durable
    /// (persisted in `household_durables` on `ClassDemographics`) vs. a
    /// perishable (consumed per-turn).
    ///
    /// ALL non-consumable goods are durables — the difference is durability,
    /// not classification. Cheap `Clothing` wears out fast (24 turns) but is
    /// still a durable holding; `LuxuryClothing` lasts 100 turns.
    pub fn is_household_durable(&self) -> bool {
        matches!(
            self,
            Commodity::Furniture
                | Commodity::LuxuryFurniture
                | Commodity::Cars
                | Commodity::Televisions
                | Commodity::Radio
                | Commodity::Agd
                | Commodity::Clothing
                | Commodity::LuxuryClothing
        )
    }

    /// Phase 47: Returns the durability (in turns) for a household durable.
    /// Durability = turns to fully degrade from condition 1.0 to 0.0.
    /// Only meaningful for commodities where `is_household_durable()` is true.
    pub fn household_durable_turns(&self) -> f64 {
        match self {
            Commodity::Clothing => 24.0,         // ~1 year
            Commodity::LuxuryClothing => 100.0,  // ~4 years
            Commodity::Radio => 200.0,           // ~8 years
            Commodity::Agd => 150.0,             // ~6 years
            Commodity::Televisions => 180.0,     // ~7.5 years
            Commodity::Furniture => 240.0,       // 10 years
            Commodity::LuxuryFurniture => 300.0, // ~12.5 years
            Commodity::Cars => 120.0,            // 5 years
            _ => f64::MAX, // Non-durables — effectively infinite (not used)
        }
    }

    /// Phase 19A: Returns `true` if a commodity is blueprint-eligible — i.e.
    /// it is either a fixed asset or a quality consumer durable (or both, like
    /// Cars/Trucks). Only blueprint-eligible outputs get `InventoryCohort`s.
    pub fn is_blueprint_eligible(&self) -> bool {
        self.is_fixed_asset() || self.is_quality_durable()
    }

    /// Phase 20 Final Audit: Returns `true` if this commodity is active in the
    /// economy. Deprecated variants are preserved in the enum for save
    /// compatibility but should be skipped by the generator, market, and
    /// supply-chain integrity checks.
    ///
    /// Deprecated variants have no producer, no consumer, and no gameplay role.
    /// They exist solely so that old save files can deserialize without error.
    pub fn is_active(&self) -> bool {
        !matches!(
            self,
            Commodity::MobileArtillery
                | Commodity::AntiAircraftArtillery
                | Commodity::InfantryFightingVehicles
                | Commodity::MilitaryTrucks
                | Commodity::Frigates
                | Commodity::Cruisers
                | Commodity::AircraftCarriers
                | Commodity::Destroyers
                | Commodity::Battleships
                | Commodity::NavalVessels
                | Commodity::Pistols
                | Commodity::Gunpowder
                | Commodity::Airplanes
                | Commodity::PassengerShips
                | Commodity::CargoShips
                | Commodity::RollingStock
                | Commodity::InsuranceServices
                | Commodity::MineralResources
                | Commodity::MarketResearch
        )
    }

    /// Returns all tradeable commodity variants in canonical (English) JSON order.
    pub fn all() -> [Commodity; 140] {
        [
            Commodity::Agd,
            Commodity::Aluminum,
            Commodity::Ammunition,
            Commodity::TowedArtillery,
            Commodity::MobileArtillery,
            Commodity::AntiAircraftArtillery,
            Commodity::Asphalt,
            Commodity::Bitumen,
            Commodity::InfantryFightingVehicles,
            Commodity::Bauxite,
            Commodity::Batteries,
            Commodity::Bombers,
            Commodity::Bricks,
            Commodity::Cement,
            Commodity::Trucks,
            Commodity::MilitaryTrucks,
            Commodity::Tin,
            Commodity::Zinc,
            Commodity::HeavyTanks,
            Commodity::LightTanks,
            Commodity::Lithium,
            Commodity::MediumTanks,
            Commodity::ElectronicComponents,
            Commodity::MechanicalComponents,
            Commodity::Planks,
            Commodity::Timber,
            Commodity::Energy,
            Commodity::Frigates,
            Commodity::NaturalGas,
            Commodity::Clay,
            Commodity::Helicopters,
            Commodity::Stone,
            Commodity::Rifles,
            Commodity::Catalysts,
            Commodity::Coke,
            Commodity::Silicon,
            Commodity::Cruisers,
            Commodity::AircraftCarriers,
            Commodity::Magnesium,
            Commodity::OfficeMachinery,
            Commodity::ConstructionMachinery,
            Commodity::IndustrialMachinery,
            Commodity::AgriculturalMachinery,
            Commodity::Furniture,
            Commodity::LuxuryFurniture,
            Commodity::Copper,
            Commodity::Meat,
            Commodity::Fighters,
            Commodity::Fertilizers,
            Commodity::Destroyers,
            Commodity::Software,
            Commodity::Fruit,
            Commodity::Lead,
            Commodity::Fuels,
            Commodity::Battleships,
            Commodity::Paper,
            Commodity::Sand,
            Commodity::Pistols,
            Commodity::Plastics,
            Commodity::Trains,
            Commodity::Prefabricates,
            Commodity::Gunpowder,
            Commodity::Food,
            Commodity::Radio,
            Commodity::RareEarthElements,
            Commodity::Oil,
            Commodity::Fish,
            Commodity::Cars,
            Commodity::Airplanes,
            Commodity::Sulfur,
            Commodity::SupportEquipment,
            Commodity::Silver,
            Commodity::Steel,
            Commodity::PassengerShips,
            Commodity::CargoShips,
            Commodity::NavalVessels,
            Commodity::MineralResources,
            Commodity::Glass,
            Commodity::Salt,
            Commodity::RollingStock,
            Commodity::Televisions,
            Commodity::Peat,
            Commodity::PassengerTransport,
            Commodity::Clothing,
            Commodity::LuxuryClothing,
            Commodity::AdministrativeServices,
            Commodity::BankingServices,
            Commodity::ConstructionServices,
            Commodity::MaintenanceServices,
            Commodity::LocalServicesCommodity,
            Commodity::InsuranceServices,
            Commodity::Limestone,
            Commodity::Cereal,
            Commodity::Vegetable,
            Commodity::Protein,
            Commodity::Fodder,
            Commodity::IndustrialFiber,
            Commodity::Luxury,
            Commodity::Water,
            Commodity::Hydrogen,
            Commodity::BrownCoal,
            Commodity::HardCoal,
            Commodity::Fibers,
            Commodity::Gold,
            Commodity::Submarines,
            Commodity::Iron,
            Commodity::Gravel,
            Commodity::Livestock,
            Commodity::MarketResearch,
            Commodity::RenovationServices,
            Commodity::InnovationPoints,
            Commodity::HealthCapacity,
            Commodity::EducationSlots,
            Commodity::Pharmaceuticals,
            Commodity::MedicalEquipment,
            Commodity::Heat,
            Commodity::JusticeCapacity,
            Commodity::SecurityCapacity,
            Commodity::IntelligenceCapacity,
            Commodity::FireProtectionCapacity,
            Commodity::ShelterCapacity,
            Commodity::BorderEnforcementCapacity,
            Commodity::CustomsCapacity,
            Commodity::SanitaryInspectionCapacity,
            Commodity::BuildingInspectionCapacity,
            Commodity::EnvironmentalInspectionCapacity,
            Commodity::LaborInspectionCapacity,
            Commodity::AssimilationCapacity,
            Commodity::ReligiousTexts,
            Commodity::RefinedFuel,
            Commodity::ReligiousArt,
            Commodity::Information,
            Commodity::Uranium,
            Commodity::Chemicals,
            Commodity::Seeds,
            Commodity::Semiconductors,
            Commodity::SodaAsh,
            Commodity::Ammonia,
            Commodity::FreightCapacity,
            Commodity::DraftAnimals,
        ]
    }
}

impl TryFrom<&str> for Commodity {
    type Error = String;
    /// Parses an English JSON commodity key back into its enum variant.
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "agd" => Ok(Commodity::Agd),
            "aluminum" => Ok(Commodity::Aluminum),
            "ammunition" => Ok(Commodity::Ammunition),
            "towed_artillery" => Ok(Commodity::TowedArtillery),
            "mobile_artillery" => Ok(Commodity::MobileArtillery),
            "anti_aircraft_artillery" => Ok(Commodity::AntiAircraftArtillery),
            "asphalt" => Ok(Commodity::Asphalt),
            "bitumen" => Ok(Commodity::Bitumen),
            "infantry_fighting_vehicles" => Ok(Commodity::InfantryFightingVehicles),
            "bauxite" => Ok(Commodity::Bauxite),
            "batteries" => Ok(Commodity::Batteries),
            "bombers" => Ok(Commodity::Bombers),
            "bricks" => Ok(Commodity::Bricks),
            "cement" => Ok(Commodity::Cement),
            "trucks" => Ok(Commodity::Trucks),
            "military_trucks" => Ok(Commodity::MilitaryTrucks),
            "tin" => Ok(Commodity::Tin),
            "zinc" => Ok(Commodity::Zinc),
            "heavy_tanks" => Ok(Commodity::HeavyTanks),
            "light_tanks" => Ok(Commodity::LightTanks),
            "lithium" => Ok(Commodity::Lithium),
            "medium_tanks" => Ok(Commodity::MediumTanks),
            "electronic_components" => Ok(Commodity::ElectronicComponents),
            "mechanical_components" => Ok(Commodity::MechanicalComponents),
            "planks" => Ok(Commodity::Planks),
            "timber" => Ok(Commodity::Timber),
            "energy" => Ok(Commodity::Energy),
            "frigates" => Ok(Commodity::Frigates),
            "natural_gas" => Ok(Commodity::NaturalGas),
            "clay" => Ok(Commodity::Clay),
            "helicopters" => Ok(Commodity::Helicopters),
            "stone" => Ok(Commodity::Stone),
            "rifles" => Ok(Commodity::Rifles),
            "catalysts" => Ok(Commodity::Catalysts),
            "coke" => Ok(Commodity::Coke),
            "silicon" => Ok(Commodity::Silicon),
            "cruisers" => Ok(Commodity::Cruisers),
            "aircraft_carriers" => Ok(Commodity::AircraftCarriers),
            "magnesium" => Ok(Commodity::Magnesium),
            "office_machinery" => Ok(Commodity::OfficeMachinery),
            "construction_machinery" => Ok(Commodity::ConstructionMachinery),
            "industrial_machinery" => Ok(Commodity::IndustrialMachinery),
            "agricultural_machinery" => Ok(Commodity::AgriculturalMachinery),
            "furniture" => Ok(Commodity::Furniture),
            "luxury_furniture" => Ok(Commodity::LuxuryFurniture),
            "copper" => Ok(Commodity::Copper),
            "meat" => Ok(Commodity::Meat),
            "fighters" => Ok(Commodity::Fighters),
            "fertilizers" => Ok(Commodity::Fertilizers),
            "destroyers" => Ok(Commodity::Destroyers),
            "software" => Ok(Commodity::Software),
            "fruit" => Ok(Commodity::Fruit),
            "lead" => Ok(Commodity::Lead),
            "fuels" => Ok(Commodity::Fuels),
            "battleships" => Ok(Commodity::Battleships),
            "paper" => Ok(Commodity::Paper),
            "sand" => Ok(Commodity::Sand),
            "pistols" => Ok(Commodity::Pistols),
            "plastics" => Ok(Commodity::Plastics),
            "trains" => Ok(Commodity::Trains),
            "prefabricates" => Ok(Commodity::Prefabricates),
            "gunpowder" => Ok(Commodity::Gunpowder),
            "food" => Ok(Commodity::Food),
            "radio" => Ok(Commodity::Radio),
            "rare_earth_elements" => Ok(Commodity::RareEarthElements),
            "oil" => Ok(Commodity::Oil),
            "fish" => Ok(Commodity::Fish),
            "cars" => Ok(Commodity::Cars),
            "airplanes" => Ok(Commodity::Airplanes),
            "sulfur" => Ok(Commodity::Sulfur),
            "support_equipment" => Ok(Commodity::SupportEquipment),
            "silver" => Ok(Commodity::Silver),
            "steel" => Ok(Commodity::Steel),
            "passenger_ships" => Ok(Commodity::PassengerShips),
            "cargo_ships" => Ok(Commodity::CargoShips),
            "naval_vessels" => Ok(Commodity::NavalVessels),
            "mineral_resources" => Ok(Commodity::MineralResources),
            "glass" => Ok(Commodity::Glass),
            "salt" => Ok(Commodity::Salt),
            "rolling_stock" => Ok(Commodity::RollingStock),
            "televisions" => Ok(Commodity::Televisions),
            "peat" => Ok(Commodity::Peat),
            "passenger_transport" => Ok(Commodity::PassengerTransport),
            "clothing" => Ok(Commodity::Clothing),
            "luxury_clothing" => Ok(Commodity::LuxuryClothing),
            "administrative_services" => Ok(Commodity::AdministrativeServices),
            "banking_services" => Ok(Commodity::BankingServices),
            "construction_services" => Ok(Commodity::ConstructionServices),
            "maintenance_services" => Ok(Commodity::MaintenanceServices),
            "local_services" => Ok(Commodity::LocalServicesCommodity),
            "insurance_services" => Ok(Commodity::InsuranceServices),
            "limestone" => Ok(Commodity::Limestone),
            "cereal" => Ok(Commodity::Cereal),
            "grains" => Ok(Commodity::Cereal),  // Phase 20: legacy alias for migrated saves
            "vegetable" => Ok(Commodity::Vegetable),
            "vegetables" => Ok(Commodity::Vegetable),  // Phase 20: legacy alias for migrated saves
            "protein" => Ok(Commodity::Protein),
            "fodder" => Ok(Commodity::Fodder),
            "industrial_fiber" => Ok(Commodity::IndustrialFiber),
            "chemicals" => Ok(Commodity::Chemicals),
            "seeds" => Ok(Commodity::Seeds),
            "semiconductors" => Ok(Commodity::Semiconductors),
            "soda_ash" => Ok(Commodity::SodaAsh),
            "ammonia" => Ok(Commodity::Ammonia),
            "luxury" => Ok(Commodity::Luxury),
            "water" => Ok(Commodity::Water),
            "hydrogen" => Ok(Commodity::Hydrogen),
            "brown_coal" => Ok(Commodity::BrownCoal),
            "hard_coal" => Ok(Commodity::HardCoal),
            "fibers" => Ok(Commodity::Fibers),
            "gold" => Ok(Commodity::Gold),
            "submarines" => Ok(Commodity::Submarines),
            "iron" => Ok(Commodity::Iron),
            "gravel" => Ok(Commodity::Gravel),
            "livestock" => Ok(Commodity::Livestock),
            "market_research" => Ok(Commodity::MarketResearch),
            "renovation_services" => Ok(Commodity::RenovationServices),
            "innovation_points" => Ok(Commodity::InnovationPoints),
            "health_capacity" => Ok(Commodity::HealthCapacity),
            "education_slots" => Ok(Commodity::EducationSlots),
            "pharmaceuticals" => Ok(Commodity::Pharmaceuticals),
            "medical_equipment" => Ok(Commodity::MedicalEquipment),
            "heat" => Ok(Commodity::Heat),
            "justice_capacity" => Ok(Commodity::JusticeCapacity),
            "security_capacity" => Ok(Commodity::SecurityCapacity),
            "intelligence_capacity" => Ok(Commodity::IntelligenceCapacity),
            "fire_protection_capacity" => Ok(Commodity::FireProtectionCapacity),
            "shelter_capacity" => Ok(Commodity::ShelterCapacity),
            "border_enforcement_capacity" => Ok(Commodity::BorderEnforcementCapacity),
            "customs_capacity" => Ok(Commodity::CustomsCapacity),
            "sanitary_inspection_capacity" => Ok(Commodity::SanitaryInspectionCapacity),
            "building_inspection_capacity" => Ok(Commodity::BuildingInspectionCapacity),
            "environmental_inspection_capacity" => Ok(Commodity::EnvironmentalInspectionCapacity),
            "labor_inspection_capacity" => Ok(Commodity::LaborInspectionCapacity),
            "assimilation_capacity" => Ok(Commodity::AssimilationCapacity),
            "religious_texts" => Ok(Commodity::ReligiousTexts),
            "refined_fuel" => Ok(Commodity::RefinedFuel),
            "religious_art" => Ok(Commodity::ReligiousArt),
            "information" => Ok(Commodity::Information),
            "uranium" => Ok(Commodity::Uranium),
            "freight_capacity" => Ok(Commodity::FreightCapacity),
            "draft_animals" => Ok(Commodity::DraftAnimals),
            _ => Err(format!("unknown commodity: {s}")),
        }
    }
}

impl std::str::FromStr for Commodity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Commodity::try_from(s)
    }
}

impl From<Commodity> for String {
    fn from(value: Commodity) -> Self {
        serde_json::to_value(&value)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{value:?}"))
    }
}
/// Fuel type consumed by a power plant (`paliwo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FuelType {
    /// `"węgiel"`.
    #[serde(rename = "węgiel")]
    Coal,
    /// `"gaz_ziemny"`.
    #[serde(rename = "gaz_ziemny")]
    NaturalGas,
    /// `"uran"`.
    #[serde(rename = "uran")]
    Uranium,
    /// `"płody_rolne"`.
    #[serde(rename = "płody_rolne")]
    AgriculturalProduce,
    /// `"brak"` — no fuel (renewables).
    #[serde(rename = "brak")]
    None_,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regime_democracy_check() {
        assert!(RegimeType::Democracy.is_democratic());
        assert!(!RegimeType::Autocracy.is_democratic());
    }

    #[test]
    fn regime_serde_roundtrip() {
        let json = serde_json::to_string(&RegimeType::Autocracy).unwrap();
        assert_eq!(json, "\"autocracy\"");
        let back: RegimeType = serde_json::from_str("\"democracy\"").unwrap();
        assert_eq!(back, RegimeType::Democracy);
    }

    #[test]
    fn commodity_english_rename() {
        let c: Commodity = serde_json::from_str("\"ammunition\"").unwrap();
        assert_eq!(c, Commodity::Ammunition);
    }

    #[test]
    fn sector_english_rename() {
        let s: Sector = serde_json::from_str("\"heavy_industry\"").unwrap();
        assert_eq!(s, Sector::HeavyIndustry);
    }
}
