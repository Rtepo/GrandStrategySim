//! Rebellion proto-state system for civil war mechanics

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::society::geography::Region;

/// Type of rebellion based on ideological motivation
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RebellionType {
    /// Peasant uprising seeking land reform
    PeasantUprising,
    /// Separatist movement seeking independence
    Separatist,
    /// Ideological revolution (communist, fascist, etc.)
    IdeologicalRevolution,
    /// Military coup attempt
    MilitaryCoup,
    /// Religious fundamentalist movement
    ReligiousFundamentalist,
}

/// Conditions that can trigger a rebellion
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RebellionTrigger {
    /// Minimum social unrest threshold (0-100)
    #[serde(default)]
    pub unrest_threshold: f64,
    
    /// Maximum tax burden threshold (0-1)
    #[serde(default)]
    pub tax_burden_threshold: f64,
    
    /// Minimum war exhaustion threshold (0-100)
    #[serde(default)]
    pub war_exhaustion_threshold: f64,
    
    /// Minimum region poverty rate (0-1)
    #[serde(default)]
    pub poverty_threshold: f64,
    
    /// Minimum support from specific rural class
    #[serde(default)]
    pub class_support_threshold: f64,
}

impl RebellionTrigger {
    /// Create default rebellion trigger thresholds
    /// 
    /// # Returns
    /// Default trigger conditions
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        RebellionTrigger {
            unrest_threshold: 60.0,
            tax_burden_threshold: 0.35,
            war_exhaustion_threshold: 50.0,
            poverty_threshold: 0.4,
            class_support_threshold: 0.6,
        }
    }
    
    /// Check if conditions are met for rebellion in a region
    /// 
    /// # Arguments
    /// * `region` - Region to check
    /// * `country_unrest` - Country-wide social unrest
    /// * `tax_burden` - Current tax burden
    /// * `war_exhaustion` - Current war exhaustion
    /// 
    /// # Returns
    /// True if rebellion conditions are met
    pub fn check_conditions(
        &self,
        region: &Region,
        country_unrest: f64,
        tax_burden: f64,
        war_exhaustion: f64,
    ) -> bool {
        // Check social unrest
        if country_unrest < self.unrest_threshold {
            return false;
        }
        
        // Check tax burden
        if tax_burden < self.tax_burden_threshold {
            return false;
        }
        
        // Check war exhaustion (if applicable)
        if war_exhaustion > 0.0 && war_exhaustion < self.war_exhaustion_threshold {
            return false;
        }
        
        // Check regional poverty
        let region_poverty = self.calculate_region_poverty(region);
        if region_poverty < self.poverty_threshold {
            return false;
        }
        
        true
    }
    
    /// Calculate poverty rate for a region
    /// 
    /// # Arguments
    /// * `region` - Region to analyze
    /// 
    /// # Returns
    /// Poverty rate (0-1)
    fn calculate_region_poverty(&self, region: &Region) -> f64 {
        // Simplified: count population in poverty classes
        let total_pop = region.population as f64;
        if total_pop == 0.0 {
            return 0.0;
        }
        
        let poor_pop: i64 = region.class_demographics.rural_classes.values()
            .filter(|d| d.economic_status == crate::society::geography::EconomicStatus::Destitute)
            .map(|d| d.population)
            .sum();
        
        (poor_pop as f64 / total_pop).min(1.0)
    }
    
    /// Determine rebellion type based on conditions
    /// 
    /// # Arguments
    /// * `region` - Region spawning rebellion
    /// * `country_politics` - Current political system
    /// 
    /// # Returns
    /// Most likely rebellion type
    pub fn determine_rebellion_type(
        &self,
        region: &Region,
        country_politics: &crate::politics::Politics,
    ) -> RebellionType {
        // Check for peasant uprising (high serf/peasant population)
        // Note: rural_classes uses String keys, so we check by string comparison
        let peasant_pop: i64 = region.class_demographics.rural_classes.iter()
            .filter(|(class, _)| *class == "Serf" || *class == "FreePeasant")
            .map(|(_, d)| d.population)
            .sum();
        
        if peasant_pop > region.population / 2 {
            return RebellionType::PeasantUprising;
        }
        
        // Check for separatist (region far from capital)
        if !region.is_capital {
            return RebellionType::Separatist;
        }
        
        // Check for ideological revolution (extreme political polarization)
        if country_politics.iron_fist > 50 {
            return RebellionType::IdeologicalRevolution;
        }
        
        // Default to peasant uprising
        RebellionType::PeasantUprising
    }
}

/// Spawn a rebel proto-state from a region
/// 
/// # Arguments
/// * `mother_country` - Original country
/// * `rebel_region` - Region that will form the rebellion
/// * `rebellion_type` - Type of rebellion
/// * `goals` - Ideological goals of the rebellion
/// 
/// # Returns
/// New rebel Country instance
pub fn spawn_rebel_proto_state(
    mother_country: &crate::state::Country,
    rebel_region: Region,
    rebellion_type: RebellionType,
    goals: Vec<String>,
) -> crate::state::Country {
    let rebel_name = format!("Powstanie w {}", rebel_region.id);
    
    // Create rebel proto-state with inherited systems
    let mut rebel_country = crate::state::Country {
        name: rebel_name,
        budget: mother_country.budget.clone(), // Inherit treasury structure
        macro_indicators: mother_country.macro_indicators.clone(), // Inherit macro data
        tax_rates: mother_country.tax_rates.clone(), // Inherit tax system
        trade_policy: mother_country.trade_policy.clone(), // Inherit trade policy
        politics: mother_country.politics.clone(), // Inherit political structure
        regions: vec![rebel_region], // Only the rebel region
        megaregions: Vec::new(), // No megaregions initially
        is_rebellion: true,
        mother_country: Some(mother_country.name.clone()),
        rebellion_type: Some(rebellion_type.clone()),
        rebellion_goals: Some(goals),
        economic_policy: mother_country.economic_policy.clone(), // Inherit economic policy
        order_of_battle: crate::military::oob::OrderOfBattle::default(), // No military units initially
        military_fronts: Vec::new(), // No fronts initially
        military_stockpile: rustc_hash::FxHashMap::default(),
        military_config: crate::military::config::MilitaryCombatConfig::default(),
        war_economy: crate::military::war_economy::WarEconomyState::default(),
        at_war_with: Vec::new(),
        pending_defense_orders: Vec::new(),
        rationing_system: crate::state::RationingSystem::default(),
        emergency_powers: crate::state::EmergencyPowers::default(),
        emergency_escalation_counter: 0,
        emergency_deescalation_counter: 0,
        ministry_public_service_pool: 0.0,
        intelligence_budget: crate::state::IntelligenceBudget::default(),
        active_lobbying_operations: Vec::new(), // No lobbying operations initially
        central_bank: mother_country.central_bank.clone(), // Inherit central bank
        currency_zone: mother_country.currency_zone.clone(), // Inherit currency zone
        interbank_market: crate::state::InterbankMarket::default(), // New interbank market
        bfg_fund: crate::state::BfgFund::default(), // New BFG fund
        sobk_scheme: crate::state::SobkScheme::default(), // New SOBK scheme
        bank_resolution: crate::state::BankResolution::default(), // New bank resolution
        bank_tax: crate::state::BankTax::default(), // New bank tax
        stock_exchange: crate::securities::StockExchange::default(), // New stock exchange
        dividend_queue: Vec::new(), ipo_queue: Vec::new(), bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(), demolition_queue: Vec::new(), halt_queue: Vec::new(), // Phase 24A.6
        knf: crate::securities::KNF::default(), // New KNF
        capital_gains_tax: crate::state::capital_gains_tax::CapitalGainsTaxRegistry::default(),
        sovereign_default_turns_remaining: 0, // No default initially
        foreign_debt: 0.0, // No foreign debt initially
        minimum_wage: mother_country.minimum_wage, // Inherit minimum wage policy
        debt_market: crate::economy::debt_market::DebtMarket::default(),
        cultural_institutions: Vec::new(),
        maritime_infrastructure: crate::infrastructure::maritime::MaritimeInfrastructure::default(),
        cultural_relief_config: crate::infrastructure::cultural::CulturalReliefConfig::default(),
        building_condition_config: crate::infrastructure::building_condition::BuildingConditionConfig::default(),
        maritime_config: crate::infrastructure::maritime::MaritimeConfig::default(),
        securities_config: crate::securities::SecuritiesMarketConfig::default(),
        central_counterparty: crate::securities::CentralCounterparty::default(),
        mbs_pool: Vec::new(),
        covered_bonds_issued: Vec::new(),
        active_derivatives: Vec::new(),
        active_futures: Vec::new(),
        bills_of_lading: Vec::new(),
        working_capital_loans: Vec::new(),
        b2b_order_config: crate::economy::b2b_config::B2bOrderConfig::default(),
        fishing_config: crate::economy::fishing_config::FishingConfig::default(),
        service_pricing_config: crate::economy::service_config::ServicePricingConfig::default(),
        infrastructure_config: crate::economy::infrastructure_config::InfrastructureConfig::default(),
        innovation_config: crate::economy::innovation_config::InnovationConfig::default(),
        corporate_tech_config: crate::economy::corporate_config::CorporateTechConfig::default(),
        fish_stocks: Vec::new(),
        fish_farms: Vec::new(),
        fishing_policies: Vec::new(),
        special_economic_zones: Vec::new(),
        conservation_policies: Vec::new(),
        national_parks: Vec::new(),
        landscape_parks: Vec::new(),
        utility_pricing_config: crate::utilities::UtilityPricingConfig::default(),
        utility_config: crate::utilities::UtilityConfig::default(),
        natural_wonders: Vec::new(),
        tourism_destinations: BTreeMap::new(),
        social_programs: Vec::new(),
        weather_state: crate::economy::weather::WeatherState::default(),
        maintenance_config: crate::economy::maintenance::MaintenanceConfig::default(),
        state_forest_state: crate::economy::state_forests::ForestDistrictState::default(),
        religious_authority_state: crate::society::religious_authority::ReligiousAuthorityState::default(),
        generative_goods_config: crate::economy::generative_goods_config::GenerativeGoodsConfig::default(),
        geological_formations: Vec::new(),
        phase22_tenders: Vec::new(),
        phase22_lawsuits: Vec::new(),
        phase22_kio_appeals: Vec::new(),
        freight_logistics_config: crate::economy::logistics::FreightLogisticsConfig::default(),
        deferred_trades: Vec::new(),
        transport_networks: crate::economy::transport_networks::TransportNetworkOverlay::default(),
        commuting_config: crate::economy::commuting::CommutingConfig::default(),
        regional_overflow_fees: std::collections::BTreeMap::new(),
        last_tax_result: None,
        accumulated_vat: 0.0,
        cadastre: crate::society::cadastre::Cadastre::default(),
        cadastre_config: crate::society::cadastre::CadastreConfig::default(),
        land_price_history: crate::society::cadastre::LandPriceHistoryRegistry::default(),
        arbitration_config: crate::society::cadastre::ArbitrationConfig::default(),
        arbitration_court: crate::society::cadastre::ArbitrationCourt::default(),
        border_conflicts: crate::society::cadastre::BorderConflictRegistry::default(),
        legal_certainty_config: crate::society::cadastre::LegalCertaintyConfig::default(),
        externality_config: crate::society::cadastre::ExternalityConfig::default(),
        national_zoning_quota: crate::society::cadastre::NationalZoningQuota::default(),
        subsurface_rights_law: crate::society::cadastre::SubsurfaceRightsLaw::default(),
        global_reputation: crate::international::reputation::GlobalReputation::default(),
        geopolitical_doctrine: crate::international::ai_doctrines::GeopoliticalDoctrine::default(),
        power_grid_state: crate::energy::PowerGridState::default(),
        ppa_registry: crate::energy::types::PpaRegistry::default(),
    };
    
    // Set rebel government type based on rebellion type
    match rebellion_type {
        RebellionType::PeasantUprising => {
            rebel_country.politics.government_form = crate::politics::system::GovernmentForm::OnePartyState;
        }
        RebellionType::Separatist => {
            rebel_country.politics.government_form = crate::politics::system::GovernmentForm::ParliamentaryDemocracy;
        }
        RebellionType::IdeologicalRevolution => {
            rebel_country.politics.government_form = crate::politics::system::GovernmentForm::OnePartyState;
        }
        RebellionType::MilitaryCoup => {
            rebel_country.politics.government_form = crate::politics::system::GovernmentForm::MilitaryDictatorship;
        }
        RebellionType::ReligiousFundamentalist => {
            rebel_country.politics.government_form = crate::politics::system::GovernmentForm::Theocracy;
        }
    }
    
    rebel_country
}

/// Check for rebellion triggers across all regions
/// 
/// # Arguments
/// * `country` - Country to check
/// * `trigger` - Rebellion trigger conditions
/// * `tax_burden` - Current tax burden
/// * `war_exhaustion` - Current war exhaustion
/// 
/// # Returns
/// Vector of regions at risk of rebellion
pub fn check_rebellion_risk(
    country: &crate::state::Country,
    trigger: &RebellionTrigger,
    tax_burden: f64,
    war_exhaustion: f64,
) -> Vec<Region> {
    let mut at_risk_regions = Vec::new();
    
    for region in &country.regions {
        if trigger.check_conditions(
            region,
            country.macro_indicators.social_unrest,
            tax_burden,
            war_exhaustion,
        ) {
            at_risk_regions.push(region.clone());
        }
    }
    
    at_risk_regions
}

/// Process rebellion spawning for a turn
/// 
/// # Arguments
/// * `country` - Country to process
/// * `trigger` - Rebellion trigger conditions
/// * `tax_burden` - Current tax burden
/// * `war_exhaustion` - Current war exhaustion
/// 
/// # Returns
/// (spawned_rebels, messages)
pub fn process_rebellion_spawning(
    country: &mut crate::state::Country,
    trigger: &RebellionTrigger,
    tax_burden: f64,
    war_exhaustion: f64,
) -> (Vec<crate::state::Country>, Vec<String>) {
    let mut messages = Vec::new();
    let mut spawned_rebels = Vec::new();
    
    let at_risk_regions = check_rebellion_risk(country, trigger, tax_burden, war_exhaustion);
    
    for region in at_risk_regions {
        // 10% chance per at-risk region to actually spawn rebellion
        if rand::random::<f64>() < 0.1 {
            let rebellion_type = trigger.determine_rebellion_type(&region, &country.politics);
            let goals = vec![
                match rebellion_type {
                    RebellionType::PeasantUprising => "Reforma rolna".to_string(),
                    RebellionType::Separatist => "Independence".to_string(),
                    RebellionType::IdeologicalRevolution => "Zmiana ustroju".to_string(),
                    RebellionType::MilitaryCoup => "Stabilizacja".to_string(),
                    RebellionType::ReligiousFundamentalist => "Prawo boskie".to_string(),
                }
            ];
            
            let rebel = spawn_rebel_proto_state(country, region.clone(), rebellion_type.clone(), goals);
            messages.push(format!(
                "[REBELLION] Rebellion of type {:?} erupted in region {}",
                rebellion_type, region.id
            ));
            spawned_rebels.push(rebel);
        }
    }
    
    (spawned_rebels, messages)
}
