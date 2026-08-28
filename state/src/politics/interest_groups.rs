use std::collections::{BTreeMap, HashMap};
use serde::{Deserialize, Serialize};

use crate::entities::Company;
use crate::entities::Union;
use crate::registries::enums::Sector;
use crate::society::geography::{RegionalClassDemographics, RuralClass};
use crate::state::macro_data::LaborMarket;
use crate::state::treasury::{BudgetAllocations, SectorShare};
use crate::state::Country;

/// Interest group with bifurcated power metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InterestGroup {
    /// Nominal power: raw population numbers and voting base
    #[serde(default)]
    pub nominal_power: f64,

    /// Financial power: lobbying potential via liquid capital
    #[serde(default)]
    pub financial_power: f64,

    /// Total political weight (normalized percentage)
    #[serde(default)]
    pub total_political_weight: f64,

    /// Mobilization factor (0-1): how effectively the group converts members to political action
    #[serde(default)]
    pub mobilization: f64,

    /// Radicalization potential (0-1): volatility and propensity for extreme actions
    #[serde(default)]
    pub radicalization: f64,
}

/// Suffrage system configuration for weighting nominal vs financial power.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SuffrageSystem {
    /// Weight of nominal power in total political weight (0-1)
    #[serde(default)]
    pub nominal_weight: f64,

    /// Weight of financial power in total political weight (0-1)
    #[serde(default)]
    pub financial_weight: f64,

    /// Type of suffrage system
    #[serde(default)]
    pub suffrage_type: SuffrageType,
}

/// Type of suffrage system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum SuffrageType {
    #[default]

    UniversalSuffrage,

    WealthWeightedVoting,

    CensusRestrictedVoting,

    NoVoting,
}

/// Configuration for a rural class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RuralClassConfig {
    /// Interest group this class belongs to
    #[serde(default)]
    pub interest_group: String,

    /// Land value per capita (illiquid real estate wealth proxy)
    #[serde(default)]
    pub land_value_per_capita: f64,

    /// Voting weight (0-1, disenfranchised classes have lower weight)
    #[serde(default)]
    pub voting_weight: f64,
}

/// Data-driven mapping from demographic classes and entities to interest groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClassToGroupMapping {
    /// Maps rural classes to their configuration
    #[serde(default)]
    pub rural_class_mapping: BTreeMap<String, RuralClassConfig>,

    /// Maps urban classes to interest groups
    #[serde(default)]
    pub urban_class_mapping: BTreeMap<String, String>,

    /// Maps education levels to interest groups with percentage shares
    /// Format: "higher" -> {"Students": 0.5, "Specialists": 0.3, "Intelligentsia": 0.2}
    #[serde(default)]
    pub education_mapping: BTreeMap<String, HashMap<String, f64>>,

    /// Maps company legal forms to interest groups
    /// Format: "Corporation" -> "Capitalists", "SoleProprietorship" -> "Petty Bourgeoisie"
    #[serde(default)]
    pub company_form_mapping: BTreeMap<String, String>,

    /// Interest group for Union entities
    #[serde(default)]
    pub trade_union_group: String,

    /// Interest group for manufacturing employment
    #[serde(default)]
    pub manufacturing_employment_group: String,

    /// Default group for unmapped entities
    #[serde(default)]
    pub default_group: String,
}

/// Calculate nominal power (population-based) for all interest groups.
///
/// # Arguments
/// * `class_group_mapping` - Data-driven mapping from classes to groups
/// * `country` - Country with demographic data
/// * `regions` - Regional class demographics
///
/// # Returns
/// HashMap mapping interest group names to their nominal power (population count)
fn calculate_nominal_power(
    class_group_mapping: &ClassToGroupMapping,
    country: &Country,
    regions: &[crate::society::geography::Region],
) -> HashMap<String, f64> {
    let mut nominal_power: HashMap<String, f64> = HashMap::new();

    // Aggregate rural class nominal power
    for region in regions {
        for (class_key, class_data) in &region.class_demographics.rural_classes {
            let class_config = class_group_mapping.rural_class_mapping
                .get(class_key)
                .cloned()
                .unwrap_or_else(|| {
                    // Default config for unmapped classes
                    RuralClassConfig {
                        interest_group: class_group_mapping.default_group.clone(),
                        land_value_per_capita: 0.0,
                        voting_weight: 1.0,
                    }
                });

            let population = class_data.population as f64 * class_config.voting_weight;
            *nominal_power.entry(class_config.interest_group.clone()).or_insert(0.0) += population;
        }
    }

    // Aggregate education-based nominal power using dynamic mapping
    let education = &country.macro_indicators.demographics.education;
    let total_population = country.budget.population as f64;

    // No education
    if let Some(group_shares) = class_group_mapping.education_mapping.get("none") {
        let no_edu_pop = total_population * education.none;
        for (group_name, share) in group_shares {
            *nominal_power.entry(group_name.clone()).or_insert(0.0) += no_edu_pop * share;
        }
    }

    // Basic education
    if let Some(group_shares) = class_group_mapping.education_mapping.get("basic") {
        let basic_edu_pop = total_population * education.basic;
        for (group_name, share) in group_shares {
            *nominal_power.entry(group_name.clone()).or_insert(0.0) += basic_edu_pop * share;
        }
    }

    // Secondary education
    let _secondary_edu_pop = total_population * education.secondary_share();
    for share in education.secondary.values() {
        let edu_pop = total_population * share;
        if let Some(group_shares) = class_group_mapping.education_mapping.get("secondary") {
            for (group_name, group_share) in group_shares {
                *nominal_power.entry(group_name.clone()).or_insert(0.0) += edu_pop * group_share;
            }
        }
    }

    // Higher education
    let higher_edu_pop = total_population * education.higher_share();
    if let Some(group_shares) = class_group_mapping.education_mapping.get("higher") {
        for (group_name, share) in group_shares {
            *nominal_power.entry(group_name.clone()).or_insert(0.0) += higher_edu_pop * share;
        }
    }

    // Add manufacturing employment to configured group
    // Use GDP share as proxy for employment since SectorShare doesn't have employment field
    let manufacturing_employment = country.budget.sectors.get(&Sector::HeavyIndustry)
        .map(|s| s.gdp_share * 1000.0) // Scale GDP share to approximate employment
        .unwrap_or(0.0) + country.budget.sectors.get(&Sector::LightIndustry)
        .map(|s| s.gdp_share * 1000.0)
        .unwrap_or(0.0);
    *nominal_power.entry(class_group_mapping.manufacturing_employment_group.clone()).or_insert(0.0) += manufacturing_employment;

    nominal_power
}

/// Calculate financial power (capital-based) for all interest groups.
///
/// # Arguments
/// * `class_group_mapping` - Data-driven mapping from classes to groups
/// * `companies` - Company entities with brokerage accounts
/// * `unions` - Union entities with budgets
/// * `regions` - Regional class demographics
///
/// # Returns
/// HashMap mapping interest group names to their financial power (log10 of total assets)
fn calculate_financial_power(
    class_group_mapping: &ClassToGroupMapping,
    companies: &[Company],
    unions: &[Union],
    regions: &[crate::society::geography::Region],
) -> HashMap<String, f64> {
    let mut financial_power: HashMap<String, f64> = HashMap::new();

    // Aggregate company financial power by legal form mapping
    for company in companies {
        let legal_form_str = format!("{:?}", company.legal_form);
        let group_name = class_group_mapping.company_form_mapping
            .get(&legal_form_str)
            .unwrap_or(&class_group_mapping.default_group);

        let total_assets = company.brokerage_account.as_ref()
            .map(|a| a.cash).unwrap_or(0.0) + company.fixed_capital;

        *financial_power.entry(group_name.clone()).or_insert(0.0) += total_assets;
    }

    // Aggregate union financial power
    for union in unions {
        let group_name = &class_group_mapping.trade_union_group;
        let total_funds = union.budget + union.strike_fund;
        *financial_power.entry(group_name.clone()).or_insert(0.0) += total_funds;
    }

    // Aggregate rural class financial power with land value proxy
    for region in regions {
        for (class_key, class_data) in &region.class_demographics.rural_classes {
            let class_config = class_group_mapping.rural_class_mapping
                .get(class_key)
                .cloned()
                .unwrap_or_else(|| {
                    // Default config for unmapped classes
                    RuralClassConfig {
                        interest_group: class_group_mapping.default_group.clone(),
                        land_value_per_capita: 0.0,
                        voting_weight: 1.0,
                    }
                });

            let land_value = class_data.population as f64 * class_config.land_value_per_capita;
            let total_wealth = class_data.savings + land_value;
            *financial_power.entry(class_config.interest_group.clone()).or_insert(0.0) += total_wealth;
        }
    }

    // Logarithmic scaling to prevent extreme dominance
    financial_power.into_iter()
        .map(|(k, v)| {
            let scaled = if v > 1.0 { v.log10() } else { 0.0 };
            (k, scaled)
        })
        .collect()
}

/// Calculate total political weight from nominal and financial power.
///
/// # Arguments
/// * `nominal_power` - Population-based power per group
/// * `financial_power` - Capital-based power per group
/// * `suffrage_system` - Suffrage configuration with weights
/// * `mobilization_factors` - Mobilization factor per group
///
/// # Returns
/// HashMap mapping interest group names to their total political weight (normalized to 100.0)
fn calculate_total_political_weight(
    nominal_power: &HashMap<String, f64>,
    financial_power: &HashMap<String, f64>,
    suffrage_system: &SuffrageSystem,
    mobilization_factors: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut total_weight: HashMap<String, f64> = HashMap::new();

    let alpha = suffrage_system.nominal_weight;
    let beta = suffrage_system.financial_weight;

    // Calculate raw total weight for each group
    for group_name in nominal_power.keys() {
        let nominal = nominal_power.get(group_name).copied().unwrap_or(0.0);
        let financial = financial_power.get(group_name).copied().unwrap_or(0.0);
        let mobilization = mobilization_factors.get(group_name).copied().unwrap_or(0.5);

        let w_total = (alpha * nominal * mobilization) + (beta * financial);
        total_weight.insert(group_name.clone(), w_total);
    }

    // Also include groups that only have financial power
    for group_name in financial_power.keys() {
        if !total_weight.contains_key(group_name) {
            let financial = financial_power.get(group_name).copied().unwrap_or(0.0);
            let _mobilization = mobilization_factors.get(group_name).copied().unwrap_or(0.5);
            let w_total = beta * financial ;
            total_weight.insert(group_name.clone(), w_total);
        }
    }

    // Normalize to sum to 100.0
    let total: f64 = total_weight.values().sum();
    if total > 0.0 {
        total_weight.into_iter()
            .map(|(k, v)| (k, (v / total) * 100.0))
            .collect()
    } else {
        total_weight
    }
}

/// National interest group power percentages.
///
/// This is a simplified port of `society/interest_groups.py` national mode. It
/// uses the same structural inputs (GDP, budget shares, sector shares,
/// education, unemployment, private capital, stock confidence, and selected
/// laws) but deliberately ignores the regional path and the leader popularity
/// lookup table because those data slices are not yet loaded in the Rust state.
///
/// # Arguments
/// * `country` - Country with demographic and economic data
/// * `companies` - Company entities
/// * `unions` - Union entities
/// * `regions` - Regional class demographics
/// * `class_group_mapping` - Data-driven mapping configuration
///
/// # Returns
/// HashMap mapping interest group names to InterestGroup structs with bifurcated power metrics
///
/// # Rules
/// * Returned values are normalized to sum to `100.0` for total_political_weight.
/// * Uses `f64` `log10` for capital scaling, matching the Python math.
pub fn calculate_interest_groups_power(
    country: &Country,
    companies: &[Company],
    unions: &[Union],
    regions: &[crate::society::geography::Region],
    class_group_mapping: &ClassToGroupMapping,
) -> HashMap<String, InterestGroup> {
    let nominal_power = calculate_nominal_power(class_group_mapping, country, regions);
    let financial_power = calculate_financial_power(class_group_mapping, companies, unions, regions);

    // Default mobilization factors from blueprint
    let mut mobilization_factors: HashMap<String, f64> = HashMap::new();
    mobilization_factors.insert("Trade Unions".to_string(), 0.8);
    mobilization_factors.insert("Students".to_string(), 0.8);
    mobilization_factors.insert("Capitalists".to_string(), 0.3);
    mobilization_factors.insert("Aristocracy".to_string(), 0.3);
    mobilization_factors.insert("Petty Bourgeoisie".to_string(), 0.5);
    mobilization_factors.insert("Clergy".to_string(), 0.5);
    mobilization_factors.insert("Agrarians".to_string(), 0.5);
    mobilization_factors.insert("Intelligentsia".to_string(), 0.6);
    mobilization_factors.insert("Specialists".to_string(), 0.6);
    mobilization_factors.insert("Artisans".to_string(), 0.5);
    mobilization_factors.insert("Bureaucrats".to_string(), 0.4);
    mobilization_factors.insert("Armed Forces".to_string(), 0.7);
    mobilization_factors.insert("Internal Cliques".to_string(), 0.6);

    // Default radicalization factors from blueprint
    let mut radicalization_factors: HashMap<String, f64> = HashMap::new();
    radicalization_factors.insert("Trade Unions".to_string(), 0.7);
    radicalization_factors.insert("Students".to_string(), 0.7);
    radicalization_factors.insert("Capitalists".to_string(), 0.2);
    radicalization_factors.insert("Aristocracy".to_string(), 0.2);
    radicalization_factors.insert("Petty Bourgeoisie".to_string(), 0.4);
    radicalization_factors.insert("Clergy".to_string(), 0.4);
    radicalization_factors.insert("Agrarians".to_string(), 0.4);
    radicalization_factors.insert("Intelligentsia".to_string(), 0.5);
    radicalization_factors.insert("Specialists".to_string(), 0.4);
    radicalization_factors.insert("Artisans".to_string(), 0.3);
    radicalization_factors.insert("Bureaucrats".to_string(), 0.2);
    radicalization_factors.insert("Armed Forces".to_string(), 0.3);
    radicalization_factors.insert("Internal Cliques".to_string(), 0.8);

    let suffrage_system = &country.politics.constitution.suffrage_system;
    let total_weight = calculate_total_political_weight(&nominal_power, &financial_power, suffrage_system, &mobilization_factors);

    // Build InterestGroup structs
    let mut interest_groups: HashMap<String, InterestGroup> = HashMap::new();
    for (group_name, weight) in total_weight {
        let nominal = nominal_power.get(&group_name).copied().unwrap_or(0.0);
        let financial = financial_power.get(&group_name).copied().unwrap_or(0.0);
        let mobilization = mobilization_factors.get(&group_name).copied().unwrap_or(0.5);
        let radicalization = radicalization_factors.get(&group_name).copied().unwrap_or(0.3);

        interest_groups.insert(group_name, InterestGroup {
            nominal_power: nominal,
            financial_power: financial,
            total_political_weight: weight,
            mobilization,
            radicalization,
        });
    }

    interest_groups
}

/// Legacy function for backward compatibility (will be removed after migration).
/// DEPRECATED: Use calculate_interest_groups_power with full parameters instead.
#[deprecated(note = "Use calculate_interest_groups_power with full parameters instead")]
pub fn calculate_interest_groups_power_legacy(country: &Country) -> HashMap<String, f64> {
    let budget = &country.budget;
    let makro = &country.macro_indicators;
    let politics = &country.politics;

    let gdp = budget.gdp.max(1.0);
    let population = budget.population.max(1) as f64;
    let sectors = &budget.sectors;
    let allocations = &budget.allocations;
    let education = &makro.demographics.education;
    let unemployment = makro.labor_market.unemployment_rate;
    let private_capital = budget.private_capital;
    let stock_confidence = budget.stock_market.confidence;
    let gini = makro.gini;

    let illiteracy_rate = education.none;
    let higher_education_total = education.higher.values().sum::<f64>();

    let heavy_industry_share = sector_share(sectors, &Sector::HeavyIndustry);
    let light_industry_share = sector_share(sectors, &Sector::LightIndustry);
    let industry_total = heavy_industry_share + light_industry_share;
    let agriculture_share = sector_share(sectors, &Sector::Agriculture);
    let local_services_share = sector_share(sectors, &Sector::LocalServices);
    let export_services_share = sector_share(sectors, &Sector::ExportServices);
    let public_services_share = sector_share(sectors, &Sector::PublicServices);

    let mut trade_union_strength = (industry_total * 100.0) * (1.0 - (unemployment / 100.0)) * (1.0 + illiteracy_rate);

    let kapital_scaled = if private_capital > 1.0 {
        private_capital.log10()
    } else {
        1.0
    };
    let mut capitalist_strength = (kapital_scaled * 2.0) + (stock_confidence * 0.2) + (export_services_share * 50.0);
    let mut petty_bourgeois_strength = (local_services_share * 150.0) + (kapital_scaled * 1.5);
    let mut agrarian_strength = (agriculture_share * 150.0) * (1.0 + (illiteracy_rate * 3.0));

    let education_budget = allocation_share(allocations, "Education and Propaganda");
    let mut intelligentsia_strength = ((export_services_share * 50.0) + (education_budget * 50.0)) * (1.0 + (higher_education_total * 6.0));

    let military_expenditure = allocation_share(allocations, "Armed Forces");
    let military_strength = military_expenditure * 300.0;

    let religious_law_multiplier = match politics.religious_law.as_str() {
        "Secularism" => 0.1,
        "State" => 1.5,
        _ => 1.0,
    };
    // Phase 17A: Use dynamic ReligiousAuthority if available, otherwise fall back to flat law multiplier.
    let max_authority = country
        .religious_authority_state
        .authority
        .values()
        .copied()
        .fold(0.0_f64, f64::max);
    let authority_multiplier = if max_authority > 0.0 {
        0.1 + max_authority * 1.4 // maps 0.0→0.1, 1.0→1.5
    } else {
        religious_law_multiplier
    };
    let mut clergy_strength = 10.0 * (1.0 + (illiteracy_rate * 2.0)) * authority_multiplier;
    if politics.government_form == crate::politics::system::GovernmentForm::Theocracy {
        clergy_strength *= 3.0;
    }

    let mut student_strength = (higher_education_total * 200.0) + (education_budget * 150.0) * (1.0 - illiteracy_rate);
    let mut aristocracy_strength = (kapital_scaled * 3.0) + (gini * 200.0);
    if matches!(
        politics.government_form,
        crate::politics::system::GovernmentForm::MilitaryDictatorship
            | crate::politics::system::GovernmentForm::OnePartyState
            | crate::politics::system::GovernmentForm::Theocracy
            | crate::politics::system::GovernmentForm::AbsoluteMonarchy
            | crate::politics::system::GovernmentForm::DualistMonarchy
            | crate::politics::system::GovernmentForm::ElectiveMonarchy
    ) {
        aristocracy_strength *= 2.5;
    }

    let nominal_budget = budget.nominal_budget.max(gdp * 0.2);
    let mut bureaucrat_strength = (nominal_budget / gdp) * 200.0;
    if matches!(
        politics.government_form,
        crate::politics::system::GovernmentForm::OnePartyState
            | crate::politics::system::GovernmentForm::MilitaryDictatorship
            | crate::politics::system::GovernmentForm::Theocracy
    ) {
        bureaucrat_strength *= 1.5;
    }

    let higher_tech = education.higher.get("Techniczne").unwrap_or(&0.0);
    let higher_med = education.higher.get("Medyczne").unwrap_or(&0.0);
    let specialist_strength = (higher_tech + higher_med) * 300.0 + (export_services_share + public_services_share) * 150.0;

    let artisan_strength = (light_industry_share + local_services_share + agriculture_share)
        * 100.0
        * (1.0 - (private_capital.max(10.0).log10() * 0.05));

    let police_budget = allocation_share(allocations, "Public Security");
    let mut clique_power = 5.0;
    if matches!(
        politics.government_form,
        crate::politics::system::GovernmentForm::OnePartyState
            | crate::politics::system::GovernmentForm::AbsoluteMonarchy
            | crate::politics::system::GovernmentForm::Theocracy
    ) {
        clique_power = (military_expenditure + police_budget) * 800.0 + (gdp / population * 10.0);
    } else if matches!(
        politics.government_form,
        crate::politics::system::GovernmentForm::MilitaryDictatorship
            | crate::politics::system::GovernmentForm::DualistMonarchy
            | crate::politics::system::GovernmentForm::ElectiveMonarchy
    ) {
        clique_power = (military_expenditure + police_budget) * 400.0;
    }

    match politics.emancipation_law.as_str() {
        "Traditionalism" => {
            clergy_strength *= 1.25;
            agrarian_strength *= 1.15;
            intelligentsia_strength *= 0.85;
        }
        "Property Rights" => {
            capitalist_strength *= 1.10;
            petty_bourgeois_strength *= 1.10;
        }
        "Limited Suffrage" => {
            intelligentsia_strength *= 1.15;
            trade_union_strength *= 1.10;
        }
        "Full Emancipation" => {
            intelligentsia_strength *= 1.30;
            trade_union_strength *= 1.25;
            student_strength *= 1.20;
            clergy_strength *= 0.80;
        }
        _ => {}
    }

    let raw = [
        ("Trade Unions", trade_union_strength.max(1.0)),
        ("Capitalists", capitalist_strength.max(1.0)),
        ("Petty Bourgeoisie", petty_bourgeois_strength.max(1.0)),
        ("Agrarians", agrarian_strength.max(1.0)),
        ("Intelligentsia", intelligentsia_strength.max(1.0)),
        ("Armed Forces", military_strength.max(1.0)),
        ("Clergy", clergy_strength.max(1.0)),
        ("Students", student_strength.max(1.0)),
        ("Aristocracy", aristocracy_strength.max(1.0)),
        ("Bureaucrats", bureaucrat_strength.max(1.0)),
        ("Specialists", specialist_strength.max(1.0)),
        ("Artisans", artisan_strength.max(1.0)),
        ("Internal Cliques", clique_power.max(1.0)),
    ];

    let total: f64 = raw.iter().map(|(_, v)| v).sum();
    raw.into_iter()
        .map(|(k, v)| (k.to_string(), (v / total) * 100.0))
        .collect()
}

fn sector_share(sectors: &HashMap<Sector, SectorShare>, sector: &Sector) -> f64 {
    sectors.get(sector).map(|s| s.gdp_share).unwrap_or(0.1)
}

fn allocation_share(allocations: &BudgetAllocations, name: &str) -> f64 {
    match name {
        "Industry" => allocations.industry,
        "Education and Propaganda" => allocations.education_propaganda,
        "Healthcare" => allocations.healthcare,
        "Infrastruktura i Transport" => allocations.infrastructure_transport,
        "Programy Socjalne" => allocations.social_programs,
        "Rolnictwo i Gospodarka Wiejska" => allocations.agriculture_rural,
        "Armed Forces" => allocations.armed_forces,
        _ => allocations
            .extra
            .get(name)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.1),
    }
}

/// Returns the allocation share for a given budget category from the ministry system.
///
/// Phase 8 replacement for `allocation_share` that reads from `MinistryConfig`
/// instead of the legacy `BudgetAllocations` struct.
pub fn allocation_share_from_ministries(
    ministry_config: &Option<crate::politics::ministries::MinistryConfig>,
    name: &str,
) -> f64 {
    let config = match ministry_config {
        Some(c) => c,
        None => return 0.1,
    };

    use crate::politics::ministries::GovernmentCompetency;

    let target_competency = match name {
        "Industry" => vec![GovernmentCompetency::HeavyIndustry, GovernmentCompetency::LightIndustry],
        "Education and Propaganda" => vec![GovernmentCompetency::Education],
        "Healthcare" => vec![GovernmentCompetency::Healthcare],
        "Infrastruktura i Transport" => vec![GovernmentCompetency::Infrastructure, GovernmentCompetency::Transport],
        "Programy Socjalne" => vec![GovernmentCompetency::SocialWelfare],
        "Rolnictwo i Gospodarka Wiejska" => vec![GovernmentCompetency::Agriculture],
        "Armed Forces" => vec![GovernmentCompetency::Defense],
        "Public Security" => vec![GovernmentCompetency::InternalSecurity],
        "Justice System" => vec![GovernmentCompetency::Justice],
        _ => return 0.1,
    };

    let total: f64 = config.ministries.iter().map(|m| m.allocated_cash).sum();
    if total <= 0.0 {
        return 0.0;
    }

    let matching: f64 = config
        .ministries
        .iter()
        .filter(|m| m.competencies.iter().any(|c| target_competency.contains(c)))
        .map(|m| m.allocated_cash)
        .sum();

    matching / total
}

/// Calculate available unskilled labor for factories
/// 
/// # Rules
/// * Explicitly build pool from available classes - DO NOT subtract serfs
/// * Serfs are completely invisible to cash labor market (tied to latifundia)
/// * Landless Laborers (Komornicy): Fully included in available unskilled labor pool
/// * Free Peasants: Partially included (fraction may seek secondary employment)
/// * Aristocracy: Excluded (capital owners, not laborers)
/// * Urban unemployed: Included
pub fn calculate_available_unskilled_labor(
    labor_market: &LaborMarket,
    class_demographics: &RegionalClassDemographics,
) -> f64 {
    // Explicitly build pool from available classes - DO NOT subtract serfs
    let landless_laborers = class_demographics.get_class(RuralClass::LandlessLaborer)
        .map(|d| d.population as f64 * d.labor_participation)
        .unwrap_or(0.0);
    
    let free_peasants = class_demographics.get_class(RuralClass::FreePeasant)
        .map(|d| d.population as f64 * d.labor_participation * 0.3) // 30% seek secondary work
        .unwrap_or(0.0);
    
    let urban_unemployed = labor_market.unskilled_tier.unemployed;
    
    landless_laborers + free_peasants + urban_unemployed
}
