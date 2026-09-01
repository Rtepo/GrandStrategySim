//! Categorical enums that replace stringly-typed dictionary keys.
//!
//! These enums give Rust code exhaustive, compiler-checked `match` handling.

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

/// Conscription policy (`obligatory_service` in `military_law`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConscriptionLaw {
    /// Mandatory training only (`"mandatory_training"`).
    MandatoryTraining,
    /// Full active service (`"full_service"`).
    FullService,
    /// No mandatory service (`"none"`).
    None_,
}

/// Policy on women serving in the armed forces (`women_in_army`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WomenInArmy {
    /// Reserve duty only (`"reserve_only"`).
    ReserveOnly,
    /// Full access to all roles (`"full_access"`).
    FullAccess,
    /// Barred from service (`"banned"`).
    Banned,
}

/// Scope of military draft (`draft_scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DraftScope {
    /// Voluntary enlistment only (`"voluntary"`).
    Voluntary,
    /// Selective draft (`"selective"`).
    Selective,
    /// Universal conscription (`"universal_draft"`).
    UniversalDraft,
}

/// Workforce skill tier used by the labor market.
///
/// # Rules
/// * The Python engine uses the keys `experts`, `skilled`, and `unskilled`
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

/// National prosperity bracket (`basket` / `prosperity_basket`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum WealthBracket {
    /// `"very_high"`.
    VeryHigh,
    /// `"high"`.
    High,
    /// `"medium"`.
    #[default]
    Medium,
    /// `"low"`.
    Low,
}

/// Macroeconomic sector (`sektor_pkb` and keys of `sektory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sector {
    /// `"mining"`.
    #[default]
    Mining,
    /// `"agriculture"`.
    Agriculture,
    /// `"heavy_industry"`.
    HeavyIndustry,
    /// `"light_industry"`.
    LightIndustry,
    /// `"armaments_industry"`.
    ArmamentsIndustry,
    /// `"local_services"`.
    LocalServices,
    /// `"export_services"`.
    ExportServices,
    /// `"construction"`.
    Construction,
    /// `"energy"`.
    Energy,
    /// `"public_services"`.
    PublicServices,
    /// `"medical_services"`.
    MedicalServices,
    /// `"educational_services"`.
    EducationalServices,
    /// `"transport_logistics"`.
    TransportLogistics,
    // STAGE C: Public Administration sector for Tax Offices
    /// `"public_administration"`.
    PublicAdministration,
    // STAGE D PHASE 2: Banking and financial services sector
    /// `"banking"`.
    Banking,
    // PHASE 3: Media and entertainment sector
    /// `"media_and_entertainment"` (radio, TV, publishing, opera, music academies).
    #[serde(rename = "media_and_entertainment")]
    MediaAndEntertainment,
    /// `"waste_management"`.
    WasteManagement,
    /// `"hospitality"` (hotels, resorts, restaurants, casinos).
    Hospitality,
    /// `"ngo"` (non-governmental organizations, charities).
    NGO,
    /// `"religion"` (churches, religious charities, religious institutions).
    Religion,
    /// PHASE 19B: `"maintenance_workshops"` (repair shops producing MaintenanceServices
    /// from generic raw materials â€” the circular-dependency-breaking maintenance sector).
    MaintenanceWorkshops,
    /// PHASE 32: `"government"` â€” Parliament, government buildings, ministerial offices.
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
                Commodity::BrownCoal,
                Commodity::HardCoal,
                Commodity::Iron,
                Commodity::Copper,
                Commodity::Gold,
                Commodity::Silver,
                Commodity::Uranium,
            ],
            Sector::Agriculture => vec![Commodity::Cereal, Commodity::Meat],
            Sector::HeavyIndustry => vec![
                Commodity::Steel,
                Commodity::IndustrialMachinery,
                Commodity::Cement,
            ],
            Sector::LightIndustry => vec![
                Commodity::Clothing,
                Commodity::LuxuryClothing,
                Commodity::Furniture,
                Commodity::Paper,
                Commodity::Glass,
            ],
            Sector::ArmamentsIndustry => vec![
                Commodity::Ammunition,
                Commodity::TowedArtillery,
                Commodity::Trucks,
                Commodity::Steel,
            ],
            Sector::Construction => vec![
                Commodity::ConstructionMachinery,
                Commodity::Cement,
                Commodity::Steel,
                Commodity::Asphalt,
            ],
            Sector::Energy => vec![Commodity::Energy, Commodity::BrownCoal, Commodity::HardCoal],
            Sector::TransportLogistics => vec![Commodity::FreightCapacity, Commodity::Trucks],
            Sector::LocalServices => vec![],
            Sector::ExportServices => vec![Commodity::FreightCapacity],
            Sector::PublicServices => vec![Commodity::OfficeMachinery],
            Sector::MedicalServices => vec![Commodity::MedicalEquipment],
            Sector::EducationalServices => vec![Commodity::ReligiousTexts],
            Sector::PublicAdministration => vec![Commodity::OfficeMachinery],
            Sector::Banking => vec![Commodity::OfficeMachinery],
            Sector::MediaAndEntertainment => vec![Commodity::ReligiousTexts],
            Sector::WasteManagement => vec![
                // Phase 84: Only B2B-tradeable sorted secondary raw materials.
                // Trash streams (MixedWaste, BioWaste, ConstructionWaste,
                // BulkyWaste, HazardousWaste) are B2B-EXCLUDED and flow
                // through WasteGridState logistical transfers only.
                Commodity::MetalWaste,
                Commodity::GlassWaste,
                Commodity::PlasticWaste,
                Commodity::ElectronicWaste,
                Commodity::TextileWaste,
            ],
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
    /// "agd".
    Agd,
    /// "aluminum".
    Aluminum,
    /// "ammunition".
    Ammunition,
    /// "towed_artillery".
    TowedArtillery,
    /// "mobile_artillery".
    MobileArtillery,
    /// "anti_aircraft_artillery".
    AntiAircraftArtillery,
    /// "asphalt".
    Asphalt,
    /// "bitumen".
    Bitumen,
    /// "infantry_fighting_vehicles".
    InfantryFightingVehicles,
    /// "bauxite".
    Bauxite,
    /// "batteries" â€” Phase 20: Energy storage for EVs, electronics, grid storage.
    Batteries,
    /// "bombers".
    Bombers,
    /// "bricks".
    Bricks,
    /// "cement".
    Cement,
    /// "trucks".
    Trucks,
    /// "military_trucks".
    MilitaryTrucks,
    /// "tin".
    Tin,
    /// "zinc".
    Zinc,
    /// "heavy_tanks".
    HeavyTanks,
    /// "light_tanks".
    LightTanks,
    /// "lithium" â€” Phase 20: Battery feedstock; mined from brines/hard rock.
    Lithium,
    /// "medium_tanks".
    MediumTanks,
    /// "electronic_components".
    ElectronicComponents,
    /// "mechanical_components".
    MechanicalComponents,
    /// "planks".
    Planks,
    /// "timber".
    Timber,
    /// "energy".
    Energy,
    /// "frigates".
    Frigates,
    /// "natural_gas".
    NaturalGas,
    /// "clay".
    Clay,
    /// "helicopters".
    Helicopters,
    /// "stone".
    Stone,
    /// "rifles".
    Rifles,
    /// "catalysts".
    Catalysts,
    /// "coke".
    Coke,
    /// "silicon".
    Silicon,
    /// "cruisers".
    Cruisers,
    /// "aircraft_carriers".
    AircraftCarriers,
    /// "magnesium".
    Magnesium,
    /// "office_machinery".
    OfficeMachinery,
    /// "construction_machinery".
    ConstructionMachinery,
    /// "industrial_machinery".
    IndustrialMachinery,
    /// "agricultural_machinery".
    AgriculturalMachinery,
    /// "furniture".
    Furniture,
    /// "luxury_furniture".
    LuxuryFurniture,
    /// "copper".
    Copper,
    /// "meat".
    Meat,
    /// "fighters".
    Fighters,
    /// "fertilizers".
    Fertilizers,
    /// "destroyers".
    Destroyers,
    /// "software".
    Software,
    /// "fruit".
    Fruit,
    /// "lead".
    Lead,
    /// "fuels".
    Fuels,
    /// "battleships".
    Battleships,
    /// "paper".
    Paper,
    /// "sand".
    Sand,
    /// "pistols".
    Pistols,
    /// "plastics" â€” Phase 20: Oil-derived polymer; input for Agd, Cars, packaging.
    Plastics,
    /// "trains".
    Trains,
    /// "prefabricates".
    Prefabricates,
    /// "gunpowder".
    Gunpowder,
    /// "food".
    Food,
    /// "radio".
    Radio,
    /// "rare_earth_elements" â€” Phase 20: Neodymium, dysprosium etc.; input for semiconductors, magnets.
    RareEarthElements,
    /// "oil".
    Oil,
    /// "fish".
    Fish,
    /// "cars".
    Cars,
    /// "airplanes".
    Airplanes,
    /// "sulfur".
    Sulfur,
    /// "support_equipment".
    SupportEquipment,
    /// "silver".
    Silver,
    /// "steel".
    Steel,
    /// "passenger_ships".
    PassengerShips,
    /// "cargo_ships".
    CargoShips,
    /// "naval_vessels".
    NavalVessels,
    /// "mineral_resources".
    MineralResources,
    /// "glass".
    Glass,
    /// "salt".
    Salt,
    /// "rolling_stock".
    RollingStock,
    /// "televisions".
    Televisions,
    /// "peat".
    Peat,
    /// "passenger_transport".
    PassengerTransport,
    /// "clothing".
    Clothing,
    /// "luxury_clothing".
    LuxuryClothing,
    /// "administrative_services".
    AdministrativeServices,
    /// "banking_services".
    BankingServices,
    /// "construction_services".
    ConstructionServices,
    /// "maintenance_services".
    MaintenanceServices,
    /// "local_services".
    #[serde(rename = "local_services")]
    LocalServicesCommodity,
    /// "insurance_services".
    InsuranceServices,
    /// "limestone".
    Limestone,
    /// "cereal" - Grain crops (wheat, corn, rice, barley) - Phase 6.3.5
    Cereal,
    /// "vegetable" - Root and leaf vegetables (potatoes, carrots, tomatoes) - Phase 6.3.5
    Vegetable,
    /// "fodder" - Animal feed crops (alfalfa, clover, beet pulp) - Phase 6.3.5
    Fodder,
    /// "industrial_fiber" - Textile and industrial raw materials (cotton, flax, hemp) - Phase 6.3.5
    IndustrialFiber,
    /// "chemicals" - General chemical inputs - Phase 6.4
    Chemicals,
    /// "seeds" - Agricultural seed inputs - Phase 6.4
    Seeds,
    /// "semiconductors" â€” Phase 20: Silicon-based ICs; input for ElectronicComponents, solar.
    Semiconductors,
    /// "soda_ash" - Solvay process output - Phase 6.4
    SodaAsh,
    /// "ammonia" - Solvay process input - Phase 6.4
    Ammonia,
    /// "luxury" - High-value processed crops (sugar, coffee, tobacco, spices) - Phase 6.3.5
    Luxury,
    /// "water".
    Water,
    /// "hydrogen".
    Hydrogen,
    /// "brown_coal".
    BrownCoal,
    /// "hard_coal".
    HardCoal,
    /// "fibers".
    Fibers,
    /// "gold".
    Gold,
    /// "submarines".
    Submarines,
    /// "iron".
    Iron,
    /// "gravel".
    Gravel,
    /// "livestock".
    Livestock,
    /// "market_research".
    MarketResearch,
    /// "renovation_services".
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
    /// "sanitary_inspection_capacity" - Sanitary inspection capacity produced by sanepid buildings (Phase 15C).
    SanitaryInspectionCapacity,
    /// "building_inspection_capacity" - Building inspection capacity produced by Building Inspectorate (Phase 15C).
    BuildingInspectionCapacity,
    /// "environmental_inspection_capacity" - Environmental inspection capacity produced by Environmental Inspectorate (Phase 15C).
    EnvironmentalInspectionCapacity,
    /// "labor_inspection_capacity" - PIP (State Labor Inspectorate) capacity (Phase 22C).
    LaborInspectionCapacity,
    /// "assimilation_capacity" - Assimilation capacity produced by Integration Centers (Phase 17B).
    AssimilationCapacity,
    /// "religious_texts" - Books/scriptures produced by monasteries (Phase 17C).
    ReligiousTexts,
    /// "refined_fuel" â€” Phase 20: High-grade distillate from crude oil.
    RefinedFuel,
    /// "religious_art" - Icons, sculptures, ritual objects produced by temples (Phase 17C).
    ReligiousArt,
    /// "information" - Media/information service for B2C consumption (Phase 18C).
    Information,
    /// "uranium" â€” Phase 21A: Nuclear fuel feedstock, mined from rift-valley deposits.
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
    /// "cooling_tower" — Phase 81: Fixed asset for closed-loop cooling upgrade
    /// on thermal power plants. Makes the plant drought-resistant.
    CoolingTower,
    /// "photovoltaic_panels" — Phase 81: Consumer good for microgeneration (Wave 2).
    PhotovoltaicPanels,
    /// "coal_gas" — Phase 81 Wave 2: City gas from coal carbonization.
    /// Historically the dominant lighting/heating fuel before electricity.
    /// Calorific value ~17 MJ/m³. Produced by Coal Carbonization, consumed
    /// by Gas Mantle lighting and early gas heating methods.
    CoalGas,
    // ════════════════════════════════════════════════════════════════════════
    // Phase 84: Waste commodities — solid waste management & circular economy.
    // Mass-conserved residual streams derived from consumption/production.
    // B2B-TRADEABLE (sorted secondary raw materials): MetalWaste, GlassWaste,
    //   PlasticWaste, ElectronicWaste, TextileWaste.
    // B2B-EXCLUDED (logistical transfer via WasteGridState only): MixedWaste,
    //   BioWaste, ConstructionWaste, BulkyWaste, HazardousWaste.
    // ════════════════════════════════════════════════════════════════════════
    /// "mixed_waste" — Phase 84: Default unsegregated municipal solid waste.
    /// B2B-EXCLUDED — flows through WasteGridState logistical transfers only.
    MixedWaste,
    /// "bio_waste" — Phase 84: From food/agricultural consumption. Compostable.
    /// B2B-EXCLUDED — flows through WasteGridState logistical transfers only.
    BioWaste,
    /// "metal_waste" — Phase 84: From durables, machinery, consumer goods.
    /// B2B-TRADEABLE — sorted secondary raw material for recycling facilities.
    MetalWaste,
    /// "glass_waste" — Phase 84: From beverages, packaging, construction.
    /// B2B-TRADEABLE — sorted secondary raw material for recycling facilities.
    GlassWaste,
    /// "plastic_waste" — Phase 84: From chemicals, packaging, light industry.
    /// B2B-TRADEABLE — sorted secondary raw material for recycling facilities.
    PlasticWaste,
    /// "electronic_waste" — Phase 84: From electronics, appliances, automation.
    /// B2B-TRADEABLE — sorted secondary raw material for recycling facilities.
    ElectronicWaste,
    /// "bulky_waste" — Phase 84: From furniture, housing goods. PSZOK drop-off.
    /// B2B-EXCLUDED — flows through WasteGridState logistical transfers only.
    BulkyWaste,
    /// "textile_waste" — Phase 84: From clothing, textiles.
    /// B2B-TRADEABLE — sorted secondary raw material for recycling facilities.
    TextileWaste,
    /// "construction_waste" — Phase 84: Generated during ConstructionProject/
    /// UpgradeProject execution. PSZOK drop-off (requires FreightCapacity).
    /// B2B-EXCLUDED — flows through WasteGridState logistical transfers only.
    ConstructionWaste,
    /// "hazardous_waste" — Phase 84: From heavy chemicals, medical, WtE ash.
    /// B2B-EXCLUDED — flows through WasteGridState logistical transfers only.
    HazardousWaste,
}

impl std::fmt::Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| format!("\"{:?}\"", self));
        let trimmed = s.trim_matches('"');
        f.write_str(trimmed)
    }
}

impl Commodity {
    /// Returns the inventory key for this commodity.
    ///
    /// # Returns
    /// * `String` â€” the serde JSON key used for inventory storage.
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
    /// - `"agriculture"` â€” food and agricultural products (typically lower VAT)
    /// - `"industry"` â€” industrial goods, construction, energy, mining (standard VAT)
    /// - `"services"` â€” services, software, maintenance, transport (standard VAT)
    ///
    /// The B2C clearing uses this to dynamically look up the active VAT rate
    /// from `country.tax_rates.vat` for the commodity's category.
    /// Default fallback for unknown commodities: `"industry"` (highest rate).
    pub fn vat_category(&self) -> &'static str {
        use Commodity::*;
        match self {
            // Agricultural products â€” food, crops, livestock
            Meat | Fruit | Cereal | Vegetable | Fodder | IndustrialFiber | Luxury | Livestock
            | Fish | Food | Seeds => "agriculture",

            // Services â€” intangible, labor-based
            Software
            | AdministrativeServices
            | BankingServices
            | ConstructionServices
            | MaintenanceServices
            | LocalServicesCommodity
            | PassengerTransport
            | RenovationServices
            | Information
            | FreightCapacity
            | InnovationPoints
            | HealthCapacity
            | EducationSlots
            | JusticeCapacity
            | SecurityCapacity
            | IntelligenceCapacity
            | FireProtectionCapacity
            | ShelterCapacity
            | BorderEnforcementCapacity
            | CustomsCapacity
            | SanitaryInspectionCapacity
            | BuildingInspectionCapacity
            | EnvironmentalInspectionCapacity
            | LaborInspectionCapacity
            | AssimilationCapacity => "services",

            // Industrial goods â€” everything else (mining, manufacturing, energy, military)
            // This is the default/safest category for treasury revenue.
            _ => "industry",
        }
    }

    /// Phase 94: Returns `true` if this commodity is intangible (zero physical mass).
    ///
    /// Intangible commodities (services, capacity slots, innovation points)
    /// do not consume `FreightCapacity` for cross-region transport and are
    /// excluded from physical mass conservation assertions in the diagnostic
    /// harness.
    ///
    /// # Rules
    /// * Intangible commodities are exactly those classified as `"services"`
    ///   by `vat_category()` — software, administrative/banking/construction/
    ///   maintenance/local/insurance/renovation services, passenger transport,
    ///   information, freight capacity, innovation points, and all capacity-
    ///   slot commodities (health, education, justice, security, etc.).
    /// * Physical commodities (agriculture, mining, manufacturing, energy,
    ///   military goods) return `false`.
    pub fn is_intangible(&self) -> bool {
        self.vat_category() == "services"
    }

    /// Phase 19B: Returns `true` if this commodity is a durable fixed-asset
    /// that, when bought B2B by a company, is installed as a `FixedAssetCohort`
    /// instead of being consumed as a per-turn input.
    ///
    /// # Rules
    /// * Machinery + vehicles: {IndustrialMachinery, ConstructionMachinery,
    ///   AgriculturalMachinery, OfficeMachinery, Trucks, Cars}.
    /// * Cars/Trucks are *also* quality consumer durables (see `is_quality_durable`)
    ///   â€” their role is determined by the transaction channel (B2B asset vs
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
                | Commodity::CoolingTower
        )
    }

    /// Phase 80: Returns `true` if this commodity is a local utility that
    /// cannot be transported and must NOT appear on the global B2B market.
    ///
    /// Energy (electricity) and Heat are local grid utilities — they are
    /// distributed by the grid distribution system (`utilities/grid.rs`),
    /// not traded on the global B2B commodity market. Placing them on the
    /// B2B market creates phantom supply with no matching demand, causing
    /// huge surpluses that distort the market UI.
    ///
    /// Stabilization Sprint: Water and B2B-excluded waste streams are also
    /// local grid utilities managed by the hydro grid (Phase 81) and waste
    /// grid (Phase 84). They must be excluded from the B2B market and the
    /// Market UI snapshot.
    pub fn is_local_utility(&self) -> bool {
        matches!(
            self,
            Commodity::Energy
                | Commodity::Heat
                | Commodity::Water
                | Commodity::MixedWaste
                | Commodity::BioWaste
                | Commodity::BulkyWaste
                | Commodity::ConstructionWaste
                | Commodity::HazardousWaste
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
    /// ALL non-consumable goods are durables â€” the difference is durability,
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
            _ => f64::MAX,                       // Non-durables â€” effectively infinite (not used)
        }
    }

    /// Phase 74: Calorific value in MJ per commodity unit.
    ///
    /// Used by energy production methods to dynamically compute actual energy
    /// output from fuel input quantities at runtime. Returns 0.0 for non-fuel
    /// commodities.
    ///
    /// Values are approximate real-world energy densities:
    /// - HardCoal (bituminous): ~25 MJ/kg
    /// - BrownCoal (lignite): ~10 MJ/kg
    /// - Peat (air-dried): ~6 MJ/kg
    /// - Oil (crude): ~42 MJ/kg
    /// - NaturalGas (methane): ~55 MJ/kg
    /// - Fuels (refined diesel/gasoline): ~34 MJ/kg
    /// - CoalGas (coal carbonization gas): ~17 MJ/m³
    /// - Uranium (enriched, simplified): ~80,000 MJ/kg
    pub fn calorific_value_mj_per_unit(&self) -> f64 {
        match self {
            Commodity::HardCoal => 25.0,
            Commodity::BrownCoal => 10.0,
            Commodity::Peat => 6.0,
            Commodity::Timber => 8.0,
            Commodity::Planks => 7.0,
            Commodity::Oil => 42.0,
            Commodity::NaturalGas => 55.0,
            Commodity::Fuels => 34.0,
            Commodity::CoalGas => 17.0,
            Commodity::Uranium => 80_000.0,
            _ => 0.0,
        }
    }

    /// Phase 74: Returns `true` if this commodity is a combustible fuel
    /// (has a positive calorific value).
    pub fn is_fuel(&self) -> bool {
        self.calorific_value_mj_per_unit() > 0.0
    }

    /// Phase 74: Returns `true` if this durable commodity requires housing
    /// to be purchased. Homeless demographics cannot buy these goods.
    /// Furniture, LuxuryFurniture, Agd, and Televisions need a home to be used.
    pub fn requires_housing(&self) -> bool {
        matches!(
            self,
            Commodity::Furniture
                | Commodity::LuxuryFurniture
                | Commodity::Agd
                | Commodity::Televisions
        )
    }

    /// Phase 19A: Returns `true` if a commodity is blueprint-eligible â€” i.e.
    /// it is either a fixed asset or a quality consumer durable (or both, like
    /// Cars/Trucks). Only blueprint-eligible outputs get `InventoryCohort`s.
    pub fn is_blueprint_eligible(&self) -> bool {
        self.is_fixed_asset() || self.is_quality_durable()
    }

    /// Returns all tradeable commodity variants in canonical (English) JSON order.
    pub fn all() -> [Commodity; 142] {
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
            Commodity::CoolingTower,
            Commodity::PhotovoltaicPanels,
            Commodity::CoalGas,
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
            "vegetable" => Ok(Commodity::Vegetable),
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
            "cooling_tower" => Ok(Commodity::CoolingTower),
            "photovoltaic_panels" => Ok(Commodity::PhotovoltaicPanels),
            "coal_gas" => Ok(Commodity::CoalGas),
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
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{value:?}"))
    }
}
/// Fuel type consumed by a power plant (`paliwo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FuelType {
    /// `"wegiel"`.
    Coal,
    /// `"gaz_ziemny"`.
    NaturalGas,
    /// `"uran"`.
    Uranium,
    /// `"crops"`.
    AgriculturalProduce,
    /// `"none"` â€” no fuel (renewables).
    None_,
}

/// Capacity type for infrastructure buildings.
///
/// Phase A.2.1: Moved here from `infrastructure/mod.rs` to break a circular
/// dependency — `ProductionMethod` (in `registries`) needs to reference
/// `CapacityType` for its typed `seat_type` field, and `infrastructure` already
/// imports from `registries::enums`. The definition lives here; `infrastructure`
/// re-exports it via `pub use crate::registries::enums::CapacityType;` so all
/// existing `crate::infrastructure::CapacityType` references compile unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum CapacityType {
    /// Acute care beds
    HospitalBeds,
    /// Outpatient visits per turn
    ClinicVisits,
    /// Rehabilitation capacity
    RehabSlots,
    /// Preventative care stays
    SanatoriumStays,
    /// 24/7 care home capacity (Social Care Home)
    DPSCapacity,
    /// Daycare capacity (Dom Dziennego Pobytu)
    DDPCapacity,
    /// Childcare seats (0-3 years)
    NurserySeats,
    /// Primary school seats
    PrimarySeats,
    /// Middle school seats
    MiddleSeats,
    /// High school seats
    HighSchoolSeats,
    /// University enrollment slots
    UniversitySlots,
    /// Monastic housing
    MonasteryCells,
    /// Worship capacity
    TempleCapacity,
    /// Cultural events per turn
    CulturalEventCapacity,
    /// Surface water supply (liters per turn) - drawn from rivers/lakes, vulnerable to sewage pollution
    SurfaceWaterSupply,
    /// Groundwater supply (liters per turn) - drawn via underground pumps, immune to surface sewage but higher cost
    GroundwaterSupply,
    /// Sewage treatment capacity (liters per turn)
    SewageTreatment,
    /// District heating capacity (GJ per turn)
    DistrictHeating,
    /// Electricity supply (kWh per turn)
    ElectricitySupply,
    /// Landfill capacity (tons per turn) - modular waste management
    LandfillCapacity,
    /// Phase 82: Thermal grid pipe network capacity (km of pipe).
    /// Distinct from DistrictHeating (which is GJ heat supply).
    /// Determines how many buildings can connect to district heating.
    ThermalGridCapacity,
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
