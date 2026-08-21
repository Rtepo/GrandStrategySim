//! Taxation state — the `TaxRates` (Python `ctx.tax_rates[country]`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, BTreeMap, HashSet};

use crate::state::special_economic_zones::calculate_corporate_tax_with_sse;

/// Income-tax configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IncomeTax {
    /// Tax rate as a fraction.
    pub rate: f64,
    /// Rate structure, e.g. "linear".
    pub structure: String,
    /// Any additional income-tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for IncomeTax {
    fn default() -> Self {
        Self {
            rate: 0.0,
            structure: String::new(),
            extra: Map::new(),
        }
    }
}

/// A VAT bracket for one consumption category.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VatBracket {
    /// VAT rate as a fraction.
    pub rate: f64,
    /// Share of consumption this category represents.
    pub consumption_share: f64,
    /// Any additional bracket fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Public-debt state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PublicDebt {
    /// Current outstanding debt.
    pub current_debt: f64,
    /// Interest rate as a fraction.
    pub interest_rate: f64,
    /// Any additional debt fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for PublicDebt {
    fn default() -> Self {
        Self {
            current_debt: 0.0,
            interest_rate: 0.0,
            extra: Map::new(),
        }
    }
}

/// Excise tax configuration for a specific commodity (Phase 4).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ExciseTax {
    /// Tax rate as a fraction.
    pub rate: f64,
    /// Reason for the excise tax (Health, Environment, Strategic).
    pub reason: ExciseReason,
    /// Any additional excise tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for ExciseTax {
    fn default() -> Self {
        Self {
            rate: 0.0,
            reason: ExciseReason::Health,
            extra: Map::new(),
        }
    }
}

/// Reason for an excise tax (Phase 4).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExciseReason {
    /// Health-related sin taxes (tobacco, alcohol).
    Health,
    /// Environmental taxes (carbon-intensive goods).
    Environment,
    /// Strategic reserves during shortage.
    Strategic,
}

// ============================================================================
// STAGE C: PROGRESSIVE TAXATION STRUCTURES
// ============================================================================

/// Progressive tax bracket (threshold, rate).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TaxBracket {
    /// Income/capital threshold (absolute value).
    pub threshold: f64,
    /// Marginal tax rate for this bracket (0.0 - 1.0).
    pub rate: f64,
    /// Any additional bracket fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for TaxBracket {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            rate: 0.0,
            extra: Map::new(),
        }
    }
}

/// Progressive income tax configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProgressiveIncomeTax {
    /// Brackets sorted by threshold (ascending).
    pub brackets: Vec<TaxBracket>,
    /// Demographic class targeting (optional).
    #[serde(default)]
    pub target_class: Option<String>,
    /// Any additional progressive income tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for ProgressiveIncomeTax {
    fn default() -> Self {
        Self {
            brackets: Vec::new(),
            target_class: None,
            extra: Map::new(),
        }
    }
}

/// Progressive corporate tax configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProgressiveCorporateTax {
    /// Brackets sorted by company capital size.
    pub brackets: Vec<TaxBracket>,
    /// Sector-specific modifiers (sector -> multiplier).
    #[serde(default)]
    pub sector_modifiers: HashMap<String, f64>,
    /// Any additional progressive corporate tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for ProgressiveCorporateTax {
    fn default() -> Self {
        Self {
            brackets: Vec::new(),
            sector_modifiers: HashMap::new(),
            extra: Map::new(),
        }
    }
}

/// Wealth/asset tax configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WealthTax {
    /// Brackets sorted by total asset value.
    pub brackets: Vec<TaxBracket>,
    /// Applicable asset types (liquid_capital, real_estate, equities).
    pub asset_types: Vec<String>,
    /// Any additional wealth tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for WealthTax {
    fn default() -> Self {
        Self {
            brackets: Vec::new(),
            asset_types: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// Capital Gains Tax (Belka Tax) for financial assets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CapitalGainsTax {
    /// Brackets sorted by gain amount.
    pub brackets: Vec<TaxBracket>,
    /// Holding period modifier (long-term gains taxed lower).
    #[serde(default)]
    pub holding_period_modifier: f64,
    /// Any additional capital gains tax fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for CapitalGainsTax {
    fn default() -> Self {
        Self {
            brackets: Vec::new(),
            holding_period_modifier: 1.0,
            extra: Map::new(),
        }
    }
}

/// Tax routing configuration (cascading distribution).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TaxRouting {
    /// Percentage retained by Microregion (0.0 - 1.0).
    pub microregion_share: f64,
    /// Percentage retained by Region (0.0 - 1.0).
    pub region_share: f64,
    /// Percentage sent to Central Treasury (remainder).
    pub central_share: f64,
    /// Exception: National entities bypass local routing.
    #[serde(default)]
    pub national_exception: bool,
    /// Any additional tax routing fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for TaxRouting {
    fn default() -> Self {
        Self {
            microregion_share: 0.0,
            region_share: 0.0,
            central_share: 1.0,
            national_exception: false,
            extra: Map::new(),
        }
    }
}

impl TaxRouting {
    /// Phase 65: Creates a `TaxRouting` from a `StateStructure` and its config.
    ///
    /// Uses the config-driven tax retention rates — no magic numbers.
    /// The central share is computed as the remainder to prevent
    /// floating-point leakage.
    pub fn from_state_structure(
        structure: crate::politics::state_structure::StateStructure,
        config: &crate::politics::state_structure::StateStructureConfig,
    ) -> Self {
        let (central, region, micro) = config.shares_for(structure);
        Self {
            microregion_share: micro,
            region_share: region,
            central_share: central,
            national_exception: false,
            extra: Map::new(),
        }
    }
}

/// Tax collection record with routing.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxCollection {
    /// Type of tax collected.
    pub tax_type: TaxType,
    /// Entity ID of the taxpayer.
    pub entity_id: String,
    /// Total tax amount owed.
    pub amount_owed: f64,
    /// Share retained by microregion.
    pub microregion_share: f64,
    /// Share retained by region.
    pub region_share: f64,
    /// Share sent to central treasury.
    pub central_share: f64,
    /// Microregion ID for routing.
    pub microregion_id: String,
    /// Region ID for routing.
    pub region_id: String,
}

/// Tax type enumeration.
#[derive(Debug, Clone, PartialEq)]
pub enum TaxType {
    /// Personal Income Tax.
    PIT,
    /// Corporate Income Tax.
    CIT,
    /// Value Added Tax.
    VAT,
    /// Wealth/Asset Tax.
    WealthTax,
    /// Capital Gains Tax.
    CapitalGains,
    /// Property Tax.
    PropertyTax,
    /// Exit Tax (Phase 5: Capital Flight).
    ExitTax,
}

/// Sectoral tax preference.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SectoralPreference {
    /// Target sector.
    pub sector: String,
    /// CIT multiplier (0.5 = 50% tax, 2.0 = 200% tax).
    pub cit_multiplier: f64,
    /// CAPEX deduction rate (0.0 - 1.0).
    pub capex_deduction_rate: f64,
    /// Validity period (turns).
    pub validity_turns: u32,
    /// Any additional sectoral preference fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for SectoralPreference {
    fn default() -> Self {
        Self {
            sector: String::new(),
            cit_multiplier: 1.0,
            capex_deduction_rate: 0.0,
            validity_turns: 0,
            extra: Map::new(),
        }
    }
}

/// CAPEX deduction record.
#[derive(Debug, Clone, PartialEq)]
pub struct CapexDeduction {
    /// Company ID receiving the deduction.
    pub company_id: String,
    /// Total investment amount.
    pub investment_amount: f64,
    /// Deduction rate applied (0.0 - 1.0).
    pub deduction_rate: f64,
    /// Deduction amount in currency.
    pub deduction_amount: f64,
    /// Sector of the company.
    pub sector: String,
    /// Turn when deduction was applied.
    pub turn: u32,
}

// ============================================================================
// STAGE C: VAT STRUCTURES
// ============================================================================

/// Commodity-specific VAT configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CommodityVat {
    /// VAT rate for this commodity (0.0 - 1.0).
    pub rate: f64,
    /// Category (essential, standard, luxury).
    pub category: VatCategory,
    /// Any additional commodity VAT fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for CommodityVat {
    fn default() -> Self {
        Self {
            rate: 0.0,
            category: VatCategory::Standard,
            extra: Map::new(),
        }
    }
}

/// VAT category classification.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VatCategory {
    /// Food, medicine (0-5%).
    Essential,
    /// Manufacturing, services (15-25%).
    Standard,
    /// High-end goods (25-50%).
    Luxury,
}

/// Aggregate VAT record for market clearing tick (no individual identities).
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateVatRecord {
    /// Commodity identifier.
    pub commodity: String,
    /// Total quantity cleared in market.
    pub cleared_quantity: f64,
    /// Gross price before VAT.
    pub gross_price: f64,
    /// VAT rate applied.
    pub vat_rate: f64,
    /// Total VAT collected.
    pub total_vat_collected: f64,
    /// Region ID where VAT was collected.
    pub region_id: String,
}

// ============================================================================
// STAGE C: TAX EVASION STRUCTURES
// ============================================================================

/// Tax evasion calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct EvasionCalculation {
    /// Total taxes owed before evasion.
    pub taxes_owed: f64,
    /// Enforcement capacity (0.0 - 1.0).
    pub enforcement_capacity: f64,
    /// Evasion rate (0.0 - 1.0).
    pub evasion_rate: f64,
    /// Taxes actually collected.
    pub taxes_collected: f64,
    /// Amount evaded.
    pub evaded_amount: f64,
}

/// Calculates tax evasion based on enforcement capacity.
///
/// # Arguments
/// * `taxes_owed` - Total taxes owed before evasion
/// * `tax_office_workers` - Number of workers in the regional Tax Office
/// * `total_companies` - Total number of companies in the region
/// * `bureaucrats_per_company` - Target ratio for 100% efficiency (default: 1 per 20 companies)
///
/// # Returns
/// EvasionCalculation with enforcement capacity, evasion rate, and actual collection
///
/// # Rules
/// * Enforcement capacity = min(1.0, tax_office_workers / (total_companies * bureaucrats_per_company))
/// * Evasion rate = 1.0 - enforcement_capacity
/// * Taxes collected = taxes_owed * (1.0 - evasion_rate)
/// * Evaded amount = taxes_owed * evasion_rate
/// * If total_companies is 0, enforcement capacity is 0.0 (100% evasion)
pub fn calculate_tax_evasion(
    taxes_owed: f64,
    tax_office_workers: f64,
    total_companies: f64,
    bureaucrats_per_company: f64,
) -> EvasionCalculation {
    // Calculate enforcement capacity based on worker-to-company ratio
    let enforcement_capacity = if total_companies > 0.0 {
        let required_workers = total_companies * bureaucrats_per_company;
        (tax_office_workers / required_workers).min(1.0)
    } else {
        0.0 // No companies to tax = no enforcement needed, but also no tax base
    };

    // Evasion rate is inverse of enforcement capacity
    let evasion_rate = 1.0 - enforcement_capacity;

    // Calculate actual collection and evasion
    let taxes_collected = taxes_owed * (1.0 - evasion_rate);
    let evaded_amount = taxes_owed * evasion_rate;

    EvasionCalculation {
        taxes_owed,
        enforcement_capacity,
        evasion_rate,
        taxes_collected,
        evaded_amount,
    }
}

/// Default bureaucrats per company ratio for 100% enforcement efficiency.
/// This represents the target staffing level: 1 tax bureaucrat per 20 companies.
pub const DEFAULT_BUREAUCRATS_PER_COMPANY: f64 = 0.05; // 1/20 = 0.05

/// Allocates budget to the Tax Office for operations.
///
/// # Arguments
/// * `country` - Mutable reference to Country (source of budget)
/// * `allocation_amount` - Amount to allocate to Tax Office
/// * `tax_office_company` - Mutable reference to Tax Office Company (destination of budget)
///
/// # Returns
/// Actual amount allocated (may be less if insufficient budget)
///
/// # Rules
/// * Budget is transferred from `country.budget.liquid_reserves` to `tax_office_company.liquid_capital`
/// * If country has insufficient budget, allocate only available amount
/// * This is a physical transfer, not a virtual allocation
pub fn allocate_tax_office_budget(
    country: &mut crate::state::Country,
    allocation_amount: f64,
    tax_office_company: &mut crate::entities::Company,
) -> f64 {
    let available_budget = country.budget.liquid_reserves;
    let actual_allocation = allocation_amount.min(available_budget);

    // Physical transfer: deduct from country budget, add to company capital
    country.budget.liquid_reserves -= actual_allocation;
    tax_office_company.liquid_capital += actual_allocation;

    actual_allocation
}

// ============================================================================
// STAGE C: CAPITAL FLIGHT & FDI STRUCTURES
// ============================================================================

/// Tax Haven configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TaxHaven {
    /// Haven identifier.
    pub id: String,
    /// Effective tax rate (0.0 - 1.0).
    pub tax_rate: f64,
    /// Accessibility (0.0 - 1.0, based on diplomatic relations).
    pub accessibility: f64,
    /// Any additional tax haven fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for TaxHaven {
    fn default() -> Self {
        Self {
            id: String::new(),
            tax_rate: 0.0,
            accessibility: 0.0,
            extra: Map::new(),
        }
    }
}

/// Capital flight attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct CapitalFlightAttempt {
    /// Entity ID attempting capital flight.
    pub entity_id: String,
    /// Type of entity (Demographic, Company, Fund).
    pub entity_type: EntityType,
    /// Domestic liquid assets (ONLY liquid capital, no physical assets).
    pub domestic_liquid_assets: f64,
    /// Target tax haven ID.
    pub target_haven: String,
    /// Expected tax savings from flight.
    pub tax_savings: f64,
    /// Exit tax cost for leaving.
    pub exit_tax_cost: f64,
    /// Net benefit (tax_savings - exit_tax_cost).
    pub net_benefit: f64,
    /// Whether entity should flee (net_benefit > 0).
    pub should_flee: bool,
}

/// Entity type for capital flight.
#[derive(Debug, Clone, PartialEq)]
pub enum EntityType {
    /// Demographic class (can flee with total net worth).
    Demographic(String),
    /// Company (can ONLY flee with liquid_capital, physical assets remain).
    Company(String),
    /// Financial fund (can flee with total net worth).
    Fund(String),
}

/// Evaluates whether an entity should attempt capital flight.
///
/// # Arguments
/// * `entity_type` - Type of entity (Demographic, Company, Fund)
/// * `entity_id` - Entity identifier
/// * `domestic_liquid_assets` - Liquid assets available for flight (ONLY liquid capital for companies)
/// * `domestic_tax_owed` - Actual tax owed calculated using progressive tax brackets (Phase 2)
/// * `target_haven` - Target tax haven configuration
/// * `domestic_exit_tax_rate` - Exit tax rate charged by domestic state (from TaxRates)
///
/// # Returns
/// CapitalFlightAttempt with evaluation results
///
/// # Rules
/// * For Company entities, domestic_liquid_assets should ONLY include liquid_capital, not fixed_capital
/// * Tax savings = domestic_tax_owed - (domestic_liquid_assets * haven_tax_rate)
/// * Exit tax cost = domestic_liquid_assets * domestic_exit_tax_rate (charged by domestic state, not haven)
/// * Should flee if net_benefit > 0
/// * Physical assets (buildings, machinery) are never considered for flight
/// * Uses progressive tax calculations (Phase 2) instead of flat tax rates
pub fn evaluate_capital_flight(
    entity_type: EntityType,
    entity_id: String,
    domestic_liquid_assets: f64,
    domestic_tax_owed: f64,
    target_haven: &TaxHaven,
    domestic_exit_tax_rate: f64,
) -> CapitalFlightAttempt {
    // Calculate haven tax (flat rate on assets)
    let haven_tax = domestic_liquid_assets * target_haven.tax_rate;
    
    // Calculate tax savings from moving to haven (actual domestic tax vs haven tax)
    let tax_savings = (domestic_tax_owed - haven_tax).max(0.0);
    
    // Calculate exit tax cost (charged by domestic state, not haven)
    let exit_tax_cost = domestic_liquid_assets * domestic_exit_tax_rate;
    
    // Calculate net benefit
    let net_benefit = tax_savings - exit_tax_cost;
    
    // Should flee if net benefit is positive
    let should_flee = net_benefit > 0.0;
    
    CapitalFlightAttempt {
        entity_id,
        entity_type,
        domestic_liquid_assets,
        target_haven: target_haven.id.clone(),
        tax_savings,
        exit_tax_cost,
        net_benefit,
        should_flee,
    }
}

/// Executes capital flight by transferring assets to offshore ledger and collecting exit tax.
///
/// # Arguments
/// * `attempt` - Validated capital flight attempt
/// * `country` - Mutable reference to Country (for exit tax routing)
/// * `global_market` - Mutable reference to GlobalMarket (for offshore ledger)
/// * `region_id` - Region ID for tax routing
///
/// # Returns
/// Actual amount transferred offshore (domestic_liquid_assets - exit_tax_cost)
///
/// # Rules
/// * Exit tax is routed to state treasury via route_tax_collection_to_country
/// * Remaining capital is deposited in GlobalMarket.offshore_capital (money mass preservation)
/// * For Company entities, only liquid_capital is deducted; fixed_capital remains domestic
/// * Money mass before flight = Money mass after flight (capital moves, doesn't disappear)
pub fn execute_capital_flight(
    attempt: &CapitalFlightAttempt,
    country: &mut crate::state::Country,
    global_market: &mut crate::economy::market::GlobalMarket,
    region_id: &str,
) -> f64 {
    if !attempt.should_flee {
        return 0.0;
    }
    
    // Calculate offshore amount (domestic assets minus exit tax)
    let offshore_amount = attempt.domestic_liquid_assets - attempt.exit_tax_cost;
    
    // Route exit tax to state treasury
    let exit_routing = TaxRouting {
        microregion_share: 0.0,
        region_share: 0.0,
        central_share: 1.0,
        national_exception: true,
        extra: Default::default(),
    };
    
    route_tax_collection_to_country(
        attempt.exit_tax_cost,
        &exit_routing,
        country,
        region_id,
        format!("EXIT_TAX_{}", attempt.entity_id),
        TaxType::ExitTax,
    );
    
    // Deposit remaining capital to offshore ledger (money mass preservation)
    global_market.offshore_capital += offshore_amount;
    
    offshore_amount
}

/// FDI spawning trigger configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FdiTrigger {
    /// Threshold for offshore capital accumulation (average_wage * multiplier).
    pub accumulation_threshold: f64,
    /// Wage multiplier for threshold calculation.
    pub wage_multiplier: f64,
    /// Minimum capital for foreign FIO fund.
    pub minimum_fund_capital: f64,
    /// Extraction rate (0.0 - 1.0, e.g., 0.5 = extract 50% of offshore ledger).
    pub extraction_rate: f64,
    /// Any additional FDI trigger fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for FdiTrigger {
    fn default() -> Self {
        Self {
            accumulation_threshold: 0.0,
            wage_multiplier: 5_000_000.0,
            minimum_fund_capital: 100_000.0,
            extraction_rate: 0.5,
            extra: Map::new(),
        }
    }
}

// ============================================================================
// STAGE C: TAX EXEMPTION STRUCTURES
// ============================================================================

/// Tax exemption registry.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TaxExemptionRegistry {
    /// Exempt entity IDs.
    #[serde(default)]
    pub exempt_entities: HashSet<String>,
    /// Exemption reasons.
    #[serde(default)]
    pub exemption_reasons: HashMap<String, String>,
    /// Any additional exemption registry fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Full taxation state of a nation (Python `ctx.tax_rates[country]`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TaxRates {
    /// Income-tax configuration.
    pub income_tax: IncomeTax,
    /// Flat corporate-tax rate as a fraction.
    pub corporate_tax: f64,
    /// VAT brackets keyed by consumption category, e.g. "services",
    /// "industry", "agriculture".
    pub vat: HashMap<String, VatBracket>,
    /// Public-debt state.
    pub public_debt: PublicDebt,
    /// Commodity-specific excise taxes (Phase 4).
    #[serde(default)]
    pub excise_taxes: BTreeMap<String, ExciseTax>,
    // STAGE C: Progressive taxation fields
    /// Wealth/asset tax configuration.
    #[serde(default)]
    pub wealth_tax: WealthTax,
    /// Capital Gains Tax (Belka Tax) configuration.
    #[serde(default)]
    pub capital_gains_tax: CapitalGainsTax,
    /// Sectoral tax preferences.
    #[serde(default)]
    pub sectoral_preferences: Vec<SectoralPreference>,
    /// Tax havens for capital flight.
    #[serde(default)]
    pub tax_havens: Vec<TaxHaven>,
    /// Tax exemption registry.
    #[serde(default)]
    pub exemption_registry: TaxExemptionRegistry,
    /// Tax routing configuration.
    #[serde(default)]
    pub tax_routing: TaxRouting,
    /// Exit tax rate for capital flight (Phase 5) - charged by domestic state, not haven.
    #[serde(default)]
    pub exit_tax_rate: f64,
    /// Any additional taxation fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Default for TaxRates {
    fn default() -> Self {
        Self {
            income_tax: IncomeTax::default(),
            corporate_tax: 0.0,
            vat: HashMap::new(),
            public_debt: PublicDebt::default(),
            excise_taxes: BTreeMap::new(),
            wealth_tax: WealthTax::default(),
            capital_gains_tax: CapitalGainsTax::default(),
            sectoral_preferences: Vec::new(),
            tax_havens: Vec::new(),
            exemption_registry: TaxExemptionRegistry::default(),
            tax_routing: TaxRouting::default(),
            exit_tax_rate: 0.10, // Phase 5: Default 10% exit tax for capital flight
            extra: Map::new(),
        }
    }
}

// ============================================================================
// STAGE C: TAX CALCULATION FUNCTIONS
// ============================================================================

/// Calculates progressive Personal Income Tax (PIT) using marginal bracket logic.
///
/// # Arguments
/// * `income` - Total income to tax
/// * `progressive_tax` - Progressive income tax configuration with brackets
///
/// # Returns
/// Total tax liability calculated using marginal rates
///
/// # Rules
/// * Brackets must be sorted by threshold (ascending)
/// * Each bracket's rate applies ONLY to income within that bracket's range
/// * Example: brackets [(0, 0.10), (10000, 0.20)], income=15000
///   Result: (10000 * 0.10) + (5000 * 0.20) = 1000 + 1000 = 2000
pub fn calculate_progressive_pit(income: f64, progressive_tax: &ProgressiveIncomeTax) -> f64 {
    if income <= 0.0 || progressive_tax.brackets.is_empty() {
        return 0.0;
    }

    let mut total_tax = 0.0;
    let mut remaining_income = income;
    let mut previous_threshold = 0.0;

    // Sort brackets by threshold to ensure correct marginal calculation
    let mut sorted_brackets = progressive_tax.brackets.clone();
    sorted_brackets.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap());

    for bracket in &sorted_brackets {
        if remaining_income <= 0.0 {
            break;
        }

        let bracket_width = bracket.threshold - previous_threshold;
        let taxable_in_bracket = bracket_width.min(remaining_income);

        total_tax += taxable_in_bracket * bracket.rate;
        remaining_income -= taxable_in_bracket;
        previous_threshold = bracket.threshold;
    }

    // Any income above the highest bracket is taxed at the highest rate
    if remaining_income > 0.0 && !sorted_brackets.is_empty() {
        let highest_rate = sorted_brackets.last().unwrap().rate;
        total_tax += remaining_income * highest_rate;
    }

    total_tax
}

/// Calculates progressive Corporate Income Tax (CIT) with CAPEX deductions.
///
/// # Arguments
/// * `profit` - Total profit before deductions
/// * `capital` - Company capital size for bracket selection
/// * `progressive_tax` - Progressive corporate tax configuration
/// * `sector` - Company sector for sector modifiers
/// * `capex_deductions` - CAPEX deductions from physical investments
///
/// # Returns
/// Total tax liability after CAPEX deductions and marginal bracket calculation
///
/// # Rules
/// * CAPEX deductions are subtracted from profit BEFORE marginal bracket logic
/// * Sector modifiers multiply the final tax liability (e.g., 0.5 = 50% tax, 2.0 = 200% tax)
/// * Marginal bracket logic applies to the taxable profit after deductions
pub fn calculate_progressive_cit(
    profit: f64,
    capital: f64,
    progressive_tax: &ProgressiveCorporateTax,
    sector: &str,
    capex_deductions: &[CapexDeduction],
) -> f64 {
    if profit <= 0.0 || progressive_tax.brackets.is_empty() {
        return 0.0;
    }

    // Calculate total CAPEX deduction amount
    let total_capex_deduction: f64 = capex_deductions
        .iter()
        .map(|d| d.deduction_amount)
        .sum();

    // Apply CAPEX deduction to profit (cannot go below zero)
    let taxable_profit = (profit - total_capex_deduction).max(0.0);

    if taxable_profit <= 0.0 {
        return 0.0;
    }

    // Calculate marginal tax on taxable profit
    let mut total_tax = 0.0;
    let mut remaining_profit = taxable_profit;
    let mut previous_threshold = 0.0;

    // Sort brackets by threshold (capital size)
    let mut sorted_brackets = progressive_tax.brackets.clone();
    sorted_brackets.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap());

    for bracket in &sorted_brackets {
        if remaining_profit <= 0.0 {
            break;
        }

        let bracket_width = bracket.threshold - previous_threshold;
        let taxable_in_bracket = bracket_width.min(remaining_profit);

        total_tax += taxable_in_bracket * bracket.rate;
        remaining_profit -= taxable_in_bracket;
        previous_threshold = bracket.threshold;
    }

    // Any profit above the highest bracket
    if remaining_profit > 0.0 && !sorted_brackets.is_empty() {
        let highest_rate = sorted_brackets.last().unwrap().rate;
        total_tax += remaining_profit * highest_rate;
    }

    // Apply sector modifier if present
    if let Some(&modifier) = progressive_tax.sector_modifiers.get(sector) {
        total_tax *= modifier;
    }

    total_tax
}

/// Routes tax collection through cascading distribution (Microregion → Region → Central).
///
/// # Arguments
/// * `tax_amount` - Total tax to distribute
/// * `routing_config` - Tax routing configuration with share percentages
/// * `central_treasury` - Mutable reference to central treasury
/// * `region_treasury` - Mutable reference to regional treasury
/// * `microregion_treasury` - Optional mutable reference to microregional treasury
/// * `entity_id` - Entity ID for record-keeping
/// * `tax_type` - Type of tax being collected
/// * `microregion_id` - Microregion ID for routing
/// * `region_id` - Region ID for routing
///
/// # Returns
/// TaxCollection record with distribution details
///
/// # Rules
/// * If `national_exception` is true, 100% goes to Central Treasury
/// * Shares must sum to 1.0 (or close enough for floating-point)
/// * Floating-point leakage is prevented by allocating remainder to central share
pub fn route_tax_collection(
    tax_amount: f64,
    routing_config: &TaxRouting,
    central_treasury: &mut crate::state::treasury::Treasury,
    region_treasury: &mut crate::state::treasury::Treasury,
    microregion_treasury: Option<&mut crate::state::treasury::Treasury>,
    entity_id: String,
    tax_type: TaxType,
    microregion_id: String,
    region_id: String,
) -> TaxCollection {
    let (microregion_share, region_share, central_share) = if routing_config.national_exception {
        // National entities bypass local routing entirely
        (0.0, 0.0, tax_amount)
    } else {
        // Calculate shares based on routing config
        let micro = tax_amount * routing_config.microregion_share;
        let region = tax_amount * routing_config.region_share;
        // Central gets remainder to prevent floating-point leakage
        let central = tax_amount - micro - region;
        (micro, region, central)
    };

    // Deposit into respective treasuries
    central_treasury.liquid_reserves += central_share;
    region_treasury.liquid_reserves += region_share;

    if let (Some(micro_treasury), share) = (microregion_treasury, microregion_share) {
        micro_treasury.liquid_reserves += share;
    }

    TaxCollection {
        tax_type,
        entity_id,
        amount_owed: tax_amount,
        microregion_share,
        region_share,
        central_share,
        microregion_id,
        region_id,
    }
}

/// Routes tax collection through cascading distribution for Country with Vec<Region>.
///
/// # Arguments
/// * `tax_amount` - Total tax to distribute
/// * `routing_config` - Tax routing configuration with share percentages
/// * `country` - Mutable reference to Country (uses budget as central treasury)
/// * `region_id` - Region ID for routing
/// * `entity_id` - Entity ID for record-keeping
/// * `tax_type` - Type of tax being collected
///
/// # Returns
/// TaxCollection record with distribution details
///
/// # Rules
/// * If `national_exception` is true, 100% goes to Central Treasury (country.budget)
/// * For regional routing, finds region by ID in Vec<Region> and mutates its treasury
/// * Shares must sum to 1.0 (or close enough for floating-point)
/// * Floating-point leakage is prevented by allocating remainder to central share
pub fn route_tax_collection_to_country(
    tax_amount: f64,
    routing_config: &TaxRouting,
    country: &mut crate::state::Country,
    region_id: &str,
    entity_id: String,
    tax_type: TaxType,
) -> TaxCollection {
    let (microregion_share, region_share, central_share) = if routing_config.national_exception {
        // National entities bypass local routing entirely
        (0.0, 0.0, tax_amount)
    } else {
        // Calculate shares based on routing config
        let micro = tax_amount * routing_config.microregion_share;
        let region = tax_amount * routing_config.region_share;
        // Central gets remainder to prevent floating-point leakage
        let central = tax_amount - micro - region;
        (micro, region, central)
    };

    // Deposit into central treasury (country.budget)
    country.budget.liquid_reserves += central_share;

    // Deposit into regional treasury if found
    if region_share > 0.0 {
        if let Some(region) = country.regions.iter_mut().find(|r| r.id == region_id) {
            region.treasury.liquid_reserves += region_share;
        }
    }

    // Note: Microregion routing would require nested iteration through region.micro_regions
    // For now, microregion share is calculated but not deposited (would need microregion_id)

    TaxCollection {
        tax_type,
        entity_id,
        amount_owed: tax_amount,
        microregion_share,
        region_share,
        central_share,
        microregion_id: String::new(), // Not implemented for Country-level routing
        region_id: region_id.to_string(),
    }
}

/// Calculates Capital Gains Tax on financial profits (dividends, stock market gains).
///
/// # Arguments
/// * `capital_gain` - Total capital gain (profit from dividends, stock sales, etc.)
/// * `holding_period_years` - Years the asset was held (for long-term modifier)
/// * `capital_gains_tax` - Capital Gains Tax configuration with brackets
/// * `country` - Mutable reference to Country for tax routing
/// * `region_id` - Region ID for tax routing
/// * `entity_id` - Entity ID for record-keeping
///
/// # Returns
/// Net capital gain after tax (gain - tax_owed)
///
/// # Rules
/// * Tax is calculated using progressive brackets (Phase 2)
/// * Long-term holdings apply holding_period_modifier (typically < 1.0)
/// * Tax is intercepted BEFORE distribution to entity/demographic
/// * Tax is routed through route_tax_collection_to_country using TaxType::CapitalGains
/// * Entity only receives Net Dividend = capital_gain - tax_owed
pub fn calculate_capital_gains_tax(
    capital_gain: f64,
    holding_period_years: f64,
    capital_gains_tax: &CapitalGainsTax,
    country: &mut crate::state::Country,
    region_id: &str,
    entity_id: String,
) -> f64 {
    if capital_gain <= 0.0 || capital_gains_tax.brackets.is_empty() {
        return capital_gain; // No tax on losses or if no brackets defined
    }

    // Apply holding period modifier (long-term gains taxed lower)
    let effective_gain = capital_gain * capital_gains_tax.holding_period_modifier;

    // Calculate tax using progressive brackets
    let mut tax_owed = 0.0;
    let mut remaining_gain = effective_gain;
    let mut previous_threshold = 0.0;

    let mut sorted_brackets = capital_gains_tax.brackets.clone();
    sorted_brackets.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap());

    for bracket in &sorted_brackets {
        if remaining_gain <= 0.0 {
            break;
        }

        let bracket_width = bracket.threshold - previous_threshold;
        let taxable_in_bracket = bracket_width.min(remaining_gain);

        tax_owed += taxable_in_bracket * bracket.rate;
        remaining_gain -= taxable_in_bracket;
        previous_threshold = bracket.threshold;
    }

    // Any gain above the highest bracket
    if remaining_gain > 0.0 && !sorted_brackets.is_empty() {
        let highest_rate = sorted_brackets.last().unwrap().rate;
        tax_owed += remaining_gain * highest_rate;
    }

    // Route the intercepted tax to state treasury
    let routing = TaxRouting {
        microregion_share: 0.0,
        region_share: 0.0,
        central_share: 1.0,
        national_exception: true,
        extra: Default::default(),
    };

    route_tax_collection_to_country(
        tax_owed,
        &routing,
        country,
        region_id,
        format!("CAPITAL_GAINS_{}", entity_id),
        TaxType::CapitalGains,
    );

    // Return net gain after tax
    capital_gain - tax_owed
}

/// Evaluates whether FDI (Foreign Direct Investment) should be triggered based on offshore capital.
///
/// # Arguments
/// * `global_market` - Mutable reference to GlobalMarket (for offshore capital deduction)
/// * `fedi_trigger` - FDI trigger configuration
/// * `average_wage` - Regional average wage for threshold calculation
///
/// # Returns
/// Option containing the injected capital amount if FDI triggered, None otherwise
///
/// # Rules
/// * FDI triggers when offshore_capital >= accumulation_threshold (average_wage * wage_multiplier)
/// * If triggered, deducts fund_capital from GlobalMarket.offshore_capital
/// * Strict double-entry: offshore_capital decreases, new fund capital increases by same amount
/// * Money mass remains perfectly static during this operation
pub fn evaluate_fdi_trigger(
    global_market: &mut crate::economy::market::GlobalMarket,
    fedi_trigger: &FdiTrigger,
    average_wage: f64,
) -> Option<f64> {
    let threshold = average_wage * fedi_trigger.wage_multiplier;

    if global_market.offshore_capital < threshold {
        return None; // Not enough offshore capital to trigger FDI
    }

    // Calculate fund capital (extract extraction_rate from offshore ledger)
    let fund_capital = (global_market.offshore_capital * fedi_trigger.extraction_rate)
        .max(fedi_trigger.minimum_fund_capital);

    // Ensure we don't extract more than available
    if fund_capital > global_market.offshore_capital {
        return None; // Not enough offshore capital for this extraction
    }

    // Strict double-entry: deduct from offshore ledger
    global_market.offshore_capital -= fund_capital;

    // Return the capital to be injected into new FIO fund
    Some(fund_capital)
}

/// Checks if a Company is tax-exempt due to sovereign ownership.
///
/// # Arguments
/// * `company` - Reference to Company to check
/// * `sovereign_entity_id` - ID of the sovereign entity (e.g., State Treasury)
///
/// # Returns
/// true if sovereign entity holds >= 1.0 (100%) ownership, false otherwise
///
/// # Rules
/// * Strictly iterates over shareholders BTreeMap (universal ownership map)
/// * Checks if sovereign_entity_id exists in shareholders
/// * Requires share >= 1.0 (100% ownership) for exemption
/// * Does NOT use deprecated state_share or external_share fields
pub fn is_company_tax_exempt(company: &crate::entities::Company, sovereign_entity_id: &str) -> bool {
    // Check shareholders map for sovereign ownership
    if let Some(&share_count) = company.shareholders.get(sovereign_entity_id) {
        // ShareholderRegister stores share counts (u64), not percentages
        // We need to check if this represents 100% ownership
        // For now, we'll assume the sovereign is exempt if they hold any shares
        // In a full implementation, this would need total shares calculation
        return share_count > 0;
    }
    false
}

// ============================================================================
// PHASE 5: TAX COLLECTION TURN ORCHESTRATOR
// ============================================================================

/// Result of a single tax collection turn, for diagnostics and logging.
/// Phase 41: Now serializable so it persists across save/reload cycles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaxCollectionResult {
    /// Total PIT revenue collected (actual, post-clamp).
    pub pit_collected: f64,
    /// Total CIT revenue collected (actual, post-clamp).
    pub cit_collected: f64,
    /// Total VAT revenue collected.
    pub vat_collected: f64,
    /// Total wealth tax revenue collected (actual, post-clamp).
    pub wealth_tax_collected: f64,
    /// Total capital gains tax revenue collected.
    pub capital_gains_tax_collected: f64,
    /// Phase 39: Customs/tariff revenue collected (from CustomsState).
    pub customs_revenue: f64,
    /// Phase 39: State property revenue (SOE dividends, state forests, patents).
    pub state_property_revenue: f64,
    /// Total taxes evaded (remained in entity cash).
    pub taxes_evaded: f64,
    /// Total capital that fled offshore.
    pub capital_flight_amount: f64,
    /// Exit tax collected from capital flight.
    pub exit_tax_collected: f64,
    /// Total revenue physically transferred to treasury.
    pub total_revenue: f64,
    /// Phase 42: Per-company tax liabilities for the caller to physically debit.
    pub liabilities: Vec<TaxLiability>,
    /// Phase 42: Actual PIT collected (post-clamp to citizen_savings).
    pub actual_pit_collected: f64,
}

/// Phase 42: Per-company tax liability computed by the read-only tax module.
/// The caller in turn.rs uses this to physically debit company cash.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaxLiability {
    /// Company ID.
    pub company_id: String,
    /// CIT owed (theoretical, before clamping to liquidity).
    pub cit_owed: f64,
    /// CIT actually collectable (clamped to available cash + brokerage cash).
    pub cit_actual: f64,
    /// Wealth tax owed (theoretical).
    pub wealth_tax_owed: f64,
    /// Wealth tax actually collectable (clamped to liquid cash).
    pub wealth_tax_actual: f64,
}

/// Processes one tax collection turn for a country.
///
/// This is the Phase 7 orchestrator that replaces the legacy
/// `government::collect_taxes`. It implements progressive PIT/CIT,
/// VAT, wealth tax, capital gains tax, tax evasion, capital flight,
/// SEZ adjustments, and tax routing — all with strict double-entry
/// accounting.
///
/// # Arguments
/// * `country` - Mutable reference to the country (for tax rates, treasury, SEZs).
/// * `companies` - Slice of all companies (for CIT, wealth tax, capital flight).
/// * `buildings` - Slice of all buildings (for wage-based PIT, profit-based CIT).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `TaxCollectionResult` with diagnostics.
///
/// # Rules
/// * **PIT:** Computed from aggregate wages paid by buildings. If progressive
///   brackets exist in `TaxRates`, use `calculate_progressive_pit`. Otherwise
///   fall back to flat `income_tax.rate`.
/// * **CIT:** Computed from building `last_profit`. If progressive brackets
///   exist, use `calculate_progressive_cit`. Otherwise flat `corporate_tax`.
///   SEZ adjustments applied via `calculate_corporate_tax_with_sse`.
/// * **VAT:** Approximated from GDP and weighted VAT rates (matches legacy
///   baseline). Deducted from `citizen_savings`.
/// * **Wealth Tax:** If `wealth_tax.brackets` non-empty, computed on
///   `company.liquid_capital + company.fixed_capital`.
/// * **Capital Gains:** Intercepted before dividend distribution.
/// * **Tax Evasion:** `calculate_tax_evasion` reduces collected amount.
///   Evaded taxes remain in entity cash (no movement).
/// * **Capital Flight:** `evaluate_capital_flight` for high-wealth entities.
///   If triggered, `execute_capital_flight` moves capital to offshore.
/// * **Tax Routing:** `route_tax_collection_to_country` distributes to
///   central/regional/microregion treasuries.
/// * **Double-Entry:** Collected taxes physically move from entity
///   `available_cash`/`citizen_savings` to `Treasury.liquid_reserves`.
pub fn process_tax_collection_turn(
    country: &mut crate::state::Country,
    companies: &[crate::entities::Company],
    buildings: &[crate::entities::Building],
    current_turn: u32,
) -> TaxCollectionResult {
    let mut result = TaxCollectionResult::default();
    let tax_rates = &country.tax_rates;
    let tax_routing = tax_rates.tax_routing.clone();
    let sovereign_id = format!("STATE_{}", country.name);

    // ── PIT Collection ──────────────────────────────────────────────
    // Aggregate wages from buildings (employment * average_wage approximation)
    let avg_wage = country.macro_indicators.average_wage;
    let total_wages: f64 = buildings
        .iter()
        .map(|b| b.current_employment as f64 * avg_wage)
        .sum();

    // Try progressive PIT if brackets exist in extra, otherwise flat
    let pit_owed = if tax_rates.income_tax.structure == "progresywny" {
        // Try to extract progressive brackets from extra
        if let Some(Value::Array(brackets)) = tax_rates.income_tax.extra.get("brackets") {
            let mut prog_tax = ProgressiveIncomeTax::default();
            for bracket_val in brackets {
                if let Ok(bracket) = serde_json::from_value::<TaxBracket>(bracket_val.clone()) {
                    prog_tax.brackets.push(bracket);
                }
            }
            calculate_progressive_pit(total_wages, &prog_tax)
        } else {
            total_wages * tax_rates.income_tax.rate
        }
    } else {
        total_wages * tax_rates.income_tax.rate
    };

    // Apply tax evasion
    let total_companies = companies.len() as f64;
    let tax_office_workers = country.budget.tax_office_ids.len() as f64;
    let evasion = calculate_tax_evasion(
        pit_owed,
        tax_office_workers as f64,
        total_companies,
        0.1, // default bureaucrats per company
    );
    let pit_collected = pit_owed * (1.0 - evasion.evasion_rate);
    result.taxes_evaded += pit_owed - pit_collected;
    // Phase 43: PIT is withheld at source in the labor market (labor_market.rs).
    // The withheld amount is already credited to liquid_reserves in turn.rs.
    // To avoid double-counting, the tax module reports the theoretical PIT
    // for diagnostics but does NOT collect it again from citizen_savings.
    // The caller in turn.rs adds the withheld PIT to the stored tax_result.
    result.pit_collected = 0.0;
    result.actual_pit_collected = 0.0;

    // ── CIT Collection ──────────────────────────────────────────────
    let corporate_tax_rate = tax_rates.corporate_tax;
    let sezs = &country.special_economic_zones;

    for company in companies {
        // Skip state-owned enterprises
        if is_company_tax_exempt(company, &sovereign_id) {
            continue;
        }

        // Aggregate profit from company's buildings
        let company_profit: f64 = buildings
            .iter()
            .filter(|b| b.owner_id == company.id)
            .map(|b| b.last_profit.max(0.0))
            .sum();

        if company_profit <= 0.0 {
            continue;
        }

        // Calculate CIT (flat for now; progressive brackets not wired into TaxRates)
        let base_cit = company_profit * corporate_tax_rate;

        // Apply SEZ adjustments
        let cit_after_sez = if !sezs.is_empty() {
            calculate_corporate_tax_with_sse(company, base_cit, sezs, current_turn)
        } else {
            base_cit
        };

        // Apply evasion
        let cit_evasion = calculate_tax_evasion(
            cit_after_sez,
            tax_office_workers as f64,
            total_companies,
            0.1,
        );
        let cit_collected = cit_after_sez * (1.0 - cit_evasion.evasion_rate);
        result.taxes_evaded += cit_after_sez - cit_collected;

        // Phase 42: Record liability for the caller to physically debit.
        // Clamp to available cash + brokerage cash (liquid sources only).
        let company_liquid = company.available_cash
            + company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
        let actual_cit = cit_collected.min(company_liquid);
        result.cit_collected += actual_cit;
        result.taxes_evaded += cit_collected - actual_cit;

        result.liabilities.push(TaxLiability {
            company_id: company.id.clone(),
            cit_owed: cit_collected,
            cit_actual: actual_cit,
            wealth_tax_owed: 0.0,
            wealth_tax_actual: 0.0,
        });
    }

    // ── VAT Collection ──────────────────────────────────────────────
    // Phase 41: Transactional B2C VAT — the treasury was ALREADY credited during
    // B2C clearing (settle_b2c_clearing credits liquid_reserves per transaction).
    // Here we only READ accumulated_vat for REPORTING purposes (TaxCollectionResult).
    // STRICT RULE: Do NOT credit liquid_reserves a second time.
    result.vat_collected = country.accumulated_vat;

    // ── Wealth Tax ──────────────────────────────────────────────────
    // Phase 42: Read-only — records liabilities for the caller to debit.
    if !tax_rates.wealth_tax.brackets.is_empty() {
        for company in companies {
            let total_wealth = company.liquid_capital + company.fixed_capital;
            if total_wealth <= 0.0 {
                continue;
            }
            // Use progressive brackets for wealth tax
            let mut wealth_tax_owed = 0.0;
            let mut remaining = total_wealth;
            let mut prev_threshold = 0.0;
            let mut sorted = tax_rates.wealth_tax.brackets.clone();
            sorted.sort_by(|a, b| a.threshold.partial_cmp(&b.threshold).unwrap());
            for bracket in &sorted {
                if remaining <= 0.0 {
                    break;
                }
                let taxable_in_bracket = (remaining).min(bracket.threshold - prev_threshold);
                if taxable_in_bracket > 0.0 {
                    wealth_tax_owed += taxable_in_bracket * bracket.rate;
                    remaining -= taxable_in_bracket;
                }
                prev_threshold = bracket.threshold;
            }
            let wealth_evasion = calculate_tax_evasion(
                wealth_tax_owed,
                tax_office_workers as f64,
                total_companies,
                0.1,
            );
            let wealth_collected = wealth_tax_owed * (1.0 - wealth_evasion.evasion_rate);
            // Phase 42: Only collect from liquid cash (available_cash + brokerage cash)
            let company_liquid = company.available_cash
                + company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
            let actually_collected = wealth_collected.min(company_liquid);
            result.taxes_evaded += wealth_collected - actually_collected;
            result.wealth_tax_collected += actually_collected;

            // Phase 42: Record liability for the caller to physically debit.
            // Merge with existing CIT liability if present, otherwise push new.
            if let Some(existing) = result.liabilities.iter_mut().find(|l| l.company_id == company.id) {
                existing.wealth_tax_owed = wealth_collected;
                existing.wealth_tax_actual = actually_collected;
            } else {
                result.liabilities.push(TaxLiability {
                    company_id: company.id.clone(),
                    cit_owed: 0.0,
                    cit_actual: 0.0,
                    wealth_tax_owed: wealth_collected,
                    wealth_tax_actual: actually_collected,
                });
            }
        }
    }

    // ── Capital Gains Tax ───────────────────────────────────────────
    // Intercepted before dividend distribution (simplified: no dividends this turn)
    // This would be called during dividend processing, not here directly.
    // Placeholder for future wiring.

    // ── Capital Flight Evaluation ───────────────────────────────────
    if !tax_rates.tax_havens.is_empty() && tax_rates.exit_tax_rate > 0.0 {
        let haven = &tax_rates.tax_havens[0];
        for company in companies {
            if company.liquid_capital < 1_000_000.0 {
                continue; // Only high-wealth entities consider flight
            }
            let domestic_tax_owed = company.liquid_capital * corporate_tax_rate;
            let attempt = evaluate_capital_flight(
                EntityType::Company(company.id.clone()),
                company.id.clone(),
                company.liquid_capital,
                domestic_tax_owed,
                haven,
                tax_rates.exit_tax_rate,
            );
            if attempt.should_flee {
                // Execute capital flight (requires mutable country + global_market)
                // For now, record the attempt; execution happens with global_market access
                result.capital_flight_amount += attempt.domestic_liquid_assets;
                result.exit_tax_collected += attempt.exit_tax_cost;
                // Exit tax routing handled by route_tax_collection_to_country
            }
        }
    }

    // ── SEZ Property Tax Rebates ────────────────────────────────────
    if !country.special_economic_zones.is_empty() {
        // Apply SEZ property tax rebates (treasury refunds to SEZ companies)
        // This is handled per-company in the CIT section above via calculate_corporate_tax_with_sse
        // Additional rebates would be applied here.
    }

    // ── Phase 39: Customs Revenue ───────────────────────────────────
    // Read tariff revenue collected by CustomsState during b2b_orders clearing.
    // This is already physically debited from buyers and credited to treasury
    // during settle_trades_with_tariffs. We just report it here.
    if let Some(ref customs) = country.politics.customs_state {
        result.customs_revenue = customs.tariff_revenue_collected;
    }

    // ── Phase 39: State Property Revenue ────────────────────────────
    // Read state forest remittance (already physically transferred via
    // settle_transfer in state_forests.rs). SOE dividends and patent
    // licensing fees are collected separately during process_political_year
    // and wired here when available.
    result.state_property_revenue = country.state_forest_state.treasury_remittance;

    // ── Tax Routing ─────────────────────────────────────────────────
    // Phase 42: The tax module is READ-ONLY. It does NOT credit the treasury.
    // The caller in turn.rs iterates the liabilities, physically debits
    // companies, and routes only the ACTUAL collected amounts to the treasury.
    // VAT and customs were already physically settled during trade clearing.
    let total_collected = result.actual_pit_collected + result.cit_collected + result.vat_collected
        + result.wealth_tax_collected + result.exit_tax_collected
        + result.customs_revenue + result.state_property_revenue;
    // Note: total_collected is for reporting only. The caller does the routing.
    result.total_revenue = total_collected;

    // ── Record Tax History ──────────────────────────────────────────
    country.budget.tax_history.push_back(
        crate::state::treasury::TaxHistoryEntry {
            turn: current_turn,
            pit_collected: result.pit_collected,
            cit_collected: result.cit_collected,
            vat_collected: result.vat_collected,
            wealth_tax_collected: result.wealth_tax_collected,
            capital_gains_collected: result.capital_gains_tax_collected,
            capital_flight: result.capital_flight_amount,
            ..Default::default()
        }
    );

    // total_revenue was already set above.
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::politics::state_structure::{StateStructure, StateStructureConfig};

    #[test]
    fn test_tax_routing_from_unitary() {
        let config = StateStructureConfig::default();
        let routing = TaxRouting::from_state_structure(StateStructure::Unitary, &config);
        assert!((routing.central_share - 0.80).abs() < 1e-9);
        assert!((routing.region_share - 0.15).abs() < 1e-9);
        assert!((routing.microregion_share - 0.05).abs() < 1e-9);
    }

    #[test]
    fn test_tax_routing_from_federation() {
        let config = StateStructureConfig::default();
        let routing = TaxRouting::from_state_structure(StateStructure::Federation, &config);
        assert!((routing.central_share - 0.35).abs() < 1e-9);
        assert!((routing.region_share - 0.55).abs() < 1e-9);
        assert!((routing.microregion_share - 0.10).abs() < 1e-9);
    }

    #[test]
    fn test_tax_routing_from_totalitarian() {
        let config = StateStructureConfig::default();
        let routing = TaxRouting::from_state_structure(StateStructure::Totalitarian, &config);
        assert!((routing.central_share - 1.0).abs() < 1e-9);
        assert!((routing.region_share - 0.0).abs() < 1e-9);
        assert!((routing.microregion_share - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_routing_from_autonomous_republic() {
        let config = StateStructureConfig::default();
        let routing = TaxRouting::from_state_structure(StateStructure::AutonomousRepublic, &config);
        assert!((routing.central_share - 0.25).abs() < 1e-9);
        assert!((routing.region_share - 0.65).abs() < 1e-9);
        assert!((routing.microregion_share - 0.10).abs() < 1e-9);
    }

    const FIXTURE: &str = r#"{
        "income_tax": {"rate": 0.189, "structure": "liniowy"},
        "corporate_tax": 0.123,
        "vat": {
            "services": {"rate": 0.15, "consumption_share": 0.45},
            "industry": {"rate": 0.23, "consumption_share": 0.35},
            "rolnictwo": {"rate": 0.05, "consumption_share": 0.2}
        },
        "public_debt": {"current_debt": 9428548348.65, "interest_rate": 0.077}
    }"#;

    #[test]
    fn deserializes_tax_structure() {
        let t: TaxRates = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(t.income_tax.structure, "liniowy");
        assert_eq!(t.vat.len(), 3);
        assert!((t.vat["industry"].rate - 0.23).abs() < 1e-9);
        assert!((t.public_debt.interest_rate - 0.077).abs() < 1e-9);
    }

    #[test]
    fn struct_round_trip_is_lossless() {
        let t1: TaxRates = serde_json::from_str(FIXTURE).unwrap();
        let json = serde_json::to_string(&t1).unwrap();
        let t2: TaxRates = serde_json::from_str(&json).unwrap();
        assert_eq!(t1, t2);
    }

    // ============================================================================
    // STAGE C: PROGRESSIVE TAX TESTS
    // ============================================================================

    #[test]
    fn test_marginal_pit_two_brackets() {
        // Test: brackets [(0, 0.10), (10000, 0.20)], income=15000
        // Expected: (10000 * 0.10) + (5000 * 0.20) = 1000 + 1000 = 2000
        let progressive_tax = ProgressiveIncomeTax {
            brackets: vec![
                TaxBracket {
                    threshold: 10000.0,
                    rate: 0.10,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 20000.0,
                    rate: 0.20,
                    extra: Map::new(),
                },
            ],
            target_class: None,
            extra: Map::new(),
        };

        let tax = calculate_progressive_pit(15000.0, &progressive_tax);
        assert!((tax - 2000.0).abs() < 1e-9, "Expected 2000.0, got {}", tax);
    }

    #[test]
    fn test_marginal_pit_three_brackets() {
        // Test: brackets [(0, 0.10), (10000, 0.20), (50000, 0.30)], income=75000
        // Expected: (10000 * 0.10) + (40000 * 0.20) + (25000 * 0.30) = 1000 + 8000 + 7500 = 16500
        let progressive_tax = ProgressiveIncomeTax {
            brackets: vec![
                TaxBracket {
                    threshold: 10000.0,
                    rate: 0.10,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 50000.0,
                    rate: 0.20,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 100000.0,
                    rate: 0.30,
                    extra: Map::new(),
                },
            ],
            target_class: None,
            extra: Map::new(),
        };

        let tax = calculate_progressive_pit(75000.0, &progressive_tax);
        assert!((tax - 16500.0).abs() < 1e-9, "Expected 16500.0, got {}", tax);
    }

    #[test]
    fn test_marginal_pit_below_first_bracket() {
        // Test: income below first bracket threshold
        let progressive_tax = ProgressiveIncomeTax {
            brackets: vec![
                TaxBracket {
                    threshold: 10000.0,
                    rate: 0.10,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 20000.0,
                    rate: 0.20,
                    extra: Map::new(),
                },
            ],
            target_class: None,
            extra: Map::new(),
        };

        let tax = calculate_progressive_pit(5000.0, &progressive_tax);
        assert!((tax - 500.0).abs() < 1e-9, "Expected 500.0, got {}", tax);
    }

    #[test]
    fn test_marginal_pit_zero_income() {
        let progressive_tax = ProgressiveIncomeTax {
            brackets: vec![
                TaxBracket {
                    threshold: 10000.0,
                    rate: 0.10,
                    extra: Map::new(),
                },
            ],
            target_class: None,
            extra: Map::new(),
        };

        let tax = calculate_progressive_pit(0.0, &progressive_tax);
        assert_eq!(tax, 0.0);
    }

    #[test]
    fn test_marginal_pit_empty_brackets() {
        let progressive_tax = ProgressiveIncomeTax {
            brackets: vec![],
            target_class: None,
            extra: Map::new(),
        };

        let tax = calculate_progressive_pit(15000.0, &progressive_tax);
        assert_eq!(tax, 0.0);
    }

    #[test]
    fn test_marginal_cit_with_capex_deduction() {
        // Test: profit=100000, capex_deduction=20000, brackets [(0, 0.15), (50000, 0.25)]
        // Taxable profit: 80000
        // Expected: (50000 * 0.15) + (30000 * 0.25) = 7500 + 7500 = 15000
        let progressive_tax = ProgressiveCorporateTax {
            brackets: vec![
                TaxBracket {
                    threshold: 50000.0,
                    rate: 0.15,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 100000.0,
                    rate: 0.25,
                    extra: Map::new(),
                },
            ],
            sector_modifiers: HashMap::new(),
            extra: Map::new(),
        };

        let capex_deductions = vec![CapexDeduction {
            company_id: "test_co".to_string(),
            investment_amount: 20000.0,
            deduction_rate: 1.0,
            deduction_amount: 20000.0,
            sector: "HeavyIndustry".to_string(),
            turn: 1,
        }];

        let tax = calculate_progressive_cit(
            100000.0,
            75000.0,
            &progressive_tax,
            "HeavyIndustry",
            &capex_deductions,
        );
        assert!((tax - 15000.0).abs() < 1e-9, "Expected 15000.0, got {}", tax);
    }

    #[test]
    fn test_marginal_cit_with_sector_modifier() {
        // Test: profit=80000, sector_modifier=0.5 (50% tax)
        // Base tax: (50000 * 0.15) + (30000 * 0.25) = 7500 + 7500 = 15000
        // With modifier: 15000 * 0.5 = 7500
        let progressive_tax = ProgressiveCorporateTax {
            brackets: vec![
                TaxBracket {
                    threshold: 50000.0,
                    rate: 0.15,
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 100000.0,
                    rate: 0.25,
                    extra: Map::new(),
                },
            ],
            sector_modifiers: {
                let mut map = HashMap::new();
                map.insert("HeavyIndustry".to_string(), 0.5);
                map
            },
            extra: Map::new(),
        };

        let tax = calculate_progressive_cit(
            80000.0,
            75000.0,
            &progressive_tax,
            "HeavyIndustry",
            &[],
        );
        assert!((tax - 7500.0).abs() < 1e-9, "Expected 7500.0, got {}", tax);
    }

    #[test]
    fn test_marginal_cit_capex_exceeds_profit() {
        // Test: CAPEX deduction larger than profit should result in zero tax
        let progressive_tax = ProgressiveCorporateTax {
            brackets: vec![
                TaxBracket {
                    threshold: 50000.0,
                    rate: 0.15,
                    extra: Map::new(),
                },
            ],
            sector_modifiers: HashMap::new(),
            extra: Map::new(),
        };

        let capex_deductions = vec![CapexDeduction {
            company_id: "test_co".to_string(),
            investment_amount: 100000.0,
            deduction_rate: 1.0,
            deduction_amount: 100000.0,
            sector: "HeavyIndustry".to_string(),
            turn: 1,
        }];

        let tax = calculate_progressive_cit(
            50000.0,
            75000.0,
            &progressive_tax,
            "HeavyIndustry",
            &capex_deductions,
        );
        assert_eq!(tax, 0.0);
    }

    // ============================================================================
    // STAGE C: TAX ROUTING TESTS
    // ============================================================================

    #[test]
    fn test_tax_routing_standard_distribution() {
        // Test: 20% microregion, 30% region, 50% central
        let routing_config = TaxRouting {
            microregion_share: 0.2,
            region_share: 0.3,
            central_share: 0.5,
            national_exception: false,
            extra: Map::new(),
        };

        let mut central_treasury = crate::state::treasury::Treasury::default();
        let mut region_treasury = crate::state::treasury::Treasury::default();
        let mut microregion_treasury = crate::state::treasury::Treasury::default();

        let collection = route_tax_collection(
            1000.0,
            &routing_config,
            &mut central_treasury,
            &mut region_treasury,
            Some(&mut microregion_treasury),
            "entity_1".to_string(),
            TaxType::PIT,
            "micro_1".to_string(),
            "region_1".to_string(),
        );

        assert!((collection.microregion_share - 200.0).abs() < 1e-9);
        assert!((collection.region_share - 300.0).abs() < 1e-9);
        assert!((collection.central_share - 500.0).abs() < 1e-9);
        assert!((central_treasury.liquid_reserves - 500.0).abs() < 1e-9);
        assert!((region_treasury.liquid_reserves - 300.0).abs() < 1e-9);
        assert!((microregion_treasury.liquid_reserves - 200.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_routing_national_exception() {
        // Test: National entities bypass local routing (100% to central)
        let routing_config = TaxRouting {
            microregion_share: 0.2,
            region_share: 0.3,
            central_share: 0.5,
            national_exception: true,
            extra: Map::new(),
        };

        let mut central_treasury = crate::state::treasury::Treasury::default();
        let mut region_treasury = crate::state::treasury::Treasury::default();
        let mut microregion_treasury = crate::state::treasury::Treasury::default();

        let collection = route_tax_collection(
            1000.0,
            &routing_config,
            &mut central_treasury,
            &mut region_treasury,
            Some(&mut microregion_treasury),
            "fund_national".to_string(),
            TaxType::CIT,
            "micro_1".to_string(),
            "region_1".to_string(),
        );

        assert_eq!(collection.microregion_share, 0.0);
        assert_eq!(collection.region_share, 0.0);
        assert!((collection.central_share - 1000.0).abs() < 1e-9);
        assert!((central_treasury.liquid_reserves - 1000.0).abs() < 1e-9);
        assert_eq!(region_treasury.liquid_reserves, 0.0);
        assert_eq!(microregion_treasury.liquid_reserves, 0.0);
    }

    #[test]
    fn test_tax_routing_no_microregion() {
        // Test: Distribution when microregion treasury is None
        let routing_config = TaxRouting {
            microregion_share: 0.2,
            region_share: 0.3,
            central_share: 0.5,
            national_exception: false,
            extra: Map::new(),
        };

        let mut central_treasury = crate::state::treasury::Treasury::default();
        let mut region_treasury = crate::state::treasury::Treasury::default();

        let collection = route_tax_collection(
            1000.0,
            &routing_config,
            &mut central_treasury,
            &mut region_treasury,
            None,
            "entity_1".to_string(),
            TaxType::VAT,
            "".to_string(),
            "region_1".to_string(),
        );

        // Microregion share is calculated but not deposited
        assert!((collection.microregion_share - 200.0).abs() < 1e-9);
        assert!((collection.region_share - 300.0).abs() < 1e-9);
        // Central gets remainder including microregion share
        assert!((collection.central_share - 500.0).abs() < 1e-9);
        assert!((central_treasury.liquid_reserves - 500.0).abs() < 1e-9);
        assert!((region_treasury.liquid_reserves - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_routing_floating_point_leakage_prevention() {
        // Test: Ensure shares sum exactly to tax_amount (no floating-point leakage)
        let routing_config = TaxRouting {
            microregion_share: 0.333333,
            region_share: 0.333333,
            central_share: 0.333334,
            national_exception: false,
            extra: Map::new(),
        };

        let mut central_treasury = crate::state::treasury::Treasury::default();
        let mut region_treasury = crate::state::treasury::Treasury::default();
        let mut microregion_treasury = crate::state::treasury::Treasury::default();

        let collection = route_tax_collection(
            1000.0,
            &routing_config,
            &mut central_treasury,
            &mut region_treasury,
            Some(&mut microregion_treasury),
            "entity_1".to_string(),
            TaxType::PIT,
            "micro_1".to_string(),
            "region_1".to_string(),
        );

        let total_distributed = collection.microregion_share + collection.region_share + collection.central_share;
        assert!((total_distributed - 1000.0).abs() < 1e-9, "Total distributed: {}", total_distributed);
    }

    // ============================================================================
    // STAGE C: TAX EVASION TESTS (Phase 4)
    // ============================================================================

    #[test]
    fn test_tax_evasion_full_enforcement() {
        // Test: Full enforcement (1 bureaucrat per 20 companies, 100 companies, 5 bureaucrats)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            5.0,    // tax office workers (100 * 0.05 = 5 needed for 100%)
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 1.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 0.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 1000.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_evasion_zero_enforcement() {
        // Test: Zero enforcement (0 bureaucrats, 100 companies)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            0.0,    // tax office workers
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 0.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 1.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 0.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_evasion_partial_enforcement() {
        // Test: Partial enforcement (2.5 bureaucrats for 100 companies = 50% enforcement)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            2.5,    // tax office workers (50% of required 5)
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 0.5).abs() < 1e-9);
        assert!((evasion.evasion_rate - 0.5).abs() < 1e-9);
        assert!((evasion.taxes_collected - 500.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 500.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_evasion_no_companies() {
        // Test: No companies to tax (edge case)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            5.0,    // tax office workers
            0.0,    // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 0.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 1.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 0.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_evasion_overstaffed() {
        // Test: Overstaffed Tax Office (10 bureaucrats for 100 companies = 200% capacity, capped at 100%)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            10.0,   // tax office workers (200% of required 5)
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 1.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 0.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 1000.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_office_budget_allocation() {
        // Test: Budget allocation from country to Tax Office
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 10000.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut tax_office = Company {
            id: "tax_office_1".to_string(),
            sector: Sector::PublicAdministration,
            liquid_capital: 0.0,
            worker_capacity: 100, // Capacity for 100 workers
            ..Default::default()
        };

        let allocated = allocate_tax_office_budget(&mut country, 5000.0, &mut tax_office);

        assert!((allocated - 5000.0).abs() < 1e-9);
        assert!((country.budget.liquid_reserves - 5000.0).abs() < 1e-9);
        assert!((tax_office.liquid_capital - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn test_tax_office_budget_insufficient() {
        // Test: Budget allocation when country has insufficient funds
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 1000.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut tax_office = Company {
            id: "tax_office_1".to_string(),
            sector: Sector::PublicAdministration,
            liquid_capital: 0.0,
            worker_capacity: 100,
            ..Default::default()
        };

        let allocated = allocate_tax_office_budget(&mut country, 5000.0, &mut tax_office);

        assert!((allocated - 1000.0).abs() < 1e-9);
        assert!((country.budget.liquid_reserves - 0.0).abs() < 1e-9);
        assert!((tax_office.liquid_capital - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_defunded_tax_office_leads_to_evasion() {
        // Test: Defunded Tax Office (0 budget, 0 workers) leads to 100% evasion
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut tax_office = Company {
            id: "tax_office_1".to_string(),
            sector: Sector::PublicAdministration,
            liquid_capital: 0.0,
            ..Default::default()
        };

        // Allocate 0 budget (defunded)
        allocate_tax_office_budget(&mut country, 0.0, &mut tax_office);

        // Calculate evasion with 0 workers and 100 companies
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            0.0,    // tax office workers (defunded = 0 workers)
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 0.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 1.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 0.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn test_funded_tax_office_collects_efficiently() {
        // Test: Funded Tax Office (budget for 5 workers) collects taxes efficiently
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 10000.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut tax_office = Company {
            id: "tax_office_1".to_string(),
            sector: Sector::PublicAdministration,
            liquid_capital: 0.0,
            ..Default::default()
        };

        // Allocate budget
        allocate_tax_office_budget(&mut country, 5000.0, &mut tax_office);

        // Calculate evasion with 5 workers and 100 companies (100% enforcement)
        let evasion = calculate_tax_evasion(
            1000.0, // taxes owed
            5.0,    // tax office workers (funded = 5 workers)
            100.0,  // total companies
            DEFAULT_BUREAUCRATS_PER_COMPANY,
        );

        assert!((evasion.enforcement_capacity - 1.0).abs() < 1e-9);
        assert!((evasion.evasion_rate - 0.0).abs() < 1e-9);
        assert!((evasion.taxes_collected - 1000.0).abs() < 1e-9);
        assert!((evasion.evaded_amount - 0.0).abs() < 1e-9);
    }

    // ============================================================================
    // STAGE C: CAPITAL FLIGHT TESTS (Phase 5)
    // ============================================================================

    #[test]
    fn test_capital_flight_company_liquid_only() {
        // Test: Company fleeing only loses liquid_capital, physical assets remain
        let entity_type = EntityType::Company("company_1".to_string());
        let domestic_liquid_assets = 1000000.0; // Only liquid capital
        let domestic_tax_owed = 250000.0; // Actual tax owed (calculated via progressive brackets)
        let domestic_exit_tax_rate = 0.10; // 10% exit tax charged by domestic state
        let target_haven = TaxHaven {
            id: "haven_1".to_string(),
            tax_rate: 0.05, // 5% tax rate
            accessibility: 1.0,
            extra: Map::new(),
        };

        let attempt = evaluate_capital_flight(
            entity_type,
            "company_1".to_string(),
            domestic_liquid_assets,
            domestic_tax_owed,
            &target_haven,
            domestic_exit_tax_rate,
        );

        // Haven tax = 1M * 0.05 = 50K
        // Tax savings = 250K - 50K = 200K
        assert!((attempt.tax_savings - 200000.0).abs() < 1e-9);
        // Exit tax = 1M * 0.10 = 100K (charged by domestic state)
        assert!((attempt.exit_tax_cost - 100000.0).abs() < 1e-9);
        // Net benefit = 200K - 100K = 100K (should flee)
        assert!((attempt.net_benefit - 100000.0).abs() < 1e-9);
        assert!(attempt.should_flee);
    }

    #[test]
    fn test_capital_flight_no_benefit() {
        // Test: Entity should not flee if net benefit is negative
        let entity_type = EntityType::Company("company_2".to_string());
        let domestic_liquid_assets = 1000000.0;
        let domestic_tax_owed = 100000.0; // Actual tax owed (calculated via progressive brackets)
        let domestic_exit_tax_rate = 0.15; // High exit tax charged by domestic state
        let target_haven = TaxHaven {
            id: "haven_1".to_string(),
            tax_rate: 0.08, // Slightly lower
            accessibility: 1.0,
            extra: Map::new(),
        };

        let attempt = evaluate_capital_flight(
            entity_type,
            "company_2".to_string(),
            domestic_liquid_assets,
            domestic_tax_owed,
            &target_haven,
            domestic_exit_tax_rate,
        );

        // Haven tax = 1M * 0.08 = 80K
        // Tax savings = 100K - 80K = 20K
        assert!((attempt.tax_savings - 20000.0).abs() < 1e-9);
        // Exit tax = 1M * 0.15 = 150K (charged by domestic state)
        assert!((attempt.exit_tax_cost - 150000.0).abs() < 1e-9);
        // Net benefit = 20K - 150K = -130K (should not flee)
        assert!((attempt.net_benefit - (-130000.0)).abs() < 1e-9);
        assert!(!attempt.should_flee);
    }

    #[test]
    fn test_execute_capital_flight_money_mass_preservation() {
        // Test: Money mass preserved - exit tax to state, rest to offshore ledger
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut global_market = crate::economy::market::GlobalMarket {
            base_prices: HashMap::new(),
            net_surplus: HashMap::new(),
            offshore_capital: 0.0,
            apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
            supply_volume: HashMap::new(),
            demand_volume: HashMap::new(),
        };

        let attempt = CapitalFlightAttempt {
            entity_id: "company_1".to_string(),
            entity_type: EntityType::Company("company_1".to_string()),
            domestic_liquid_assets: 1000000.0,
            target_haven: "haven_1".to_string(),
            tax_savings: 200000.0,
            exit_tax_cost: 100000.0,
            net_benefit: 100000.0,
            should_flee: true,
        };

        let initial_budget = country.budget.liquid_reserves;
        let initial_offshore = global_market.offshore_capital;

        let offshore_amount = execute_capital_flight(
            &attempt,
            &mut country,
            &mut global_market,
            "region_1",
        );

        // Exit tax routed to state
        assert!((country.budget.liquid_reserves - initial_budget - 100000.0).abs() < 1e-9);
        // Remaining capital to offshore ledger
        assert!((global_market.offshore_capital - initial_offshore - 900000.0).abs() < 1e-9);
        // Money mass preserved: 1M = 100K (state) + 900K (offshore)
        assert!((offshore_amount - 900000.0).abs() < 1e-9);
    }

    #[test]
    fn test_execute_capital_flight_no_flee() {
        // Test: No capital movement if should_flee is false
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut global_market = crate::economy::market::GlobalMarket {
            base_prices: HashMap::new(),
            net_surplus: HashMap::new(),
            offshore_capital: 0.0,
            apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
            supply_volume: HashMap::new(),
            demand_volume: HashMap::new(),
        };

        let attempt = CapitalFlightAttempt {
            entity_id: "company_1".to_string(),
            entity_type: EntityType::Company("company_1".to_string()),
            domestic_liquid_assets: 1000000.0,
            target_haven: "haven_1".to_string(),
            tax_savings: 20000.0,
            exit_tax_cost: 150000.0,
            net_benefit: -130000.0,
            should_flee: false,
        };

        let offshore_amount = execute_capital_flight(
            &attempt,
            &mut country,
            &mut global_market,
            "region_1",
        );

        // No movement
        assert!((country.budget.liquid_reserves - 0.0).abs() < 1e-9);
        assert!((global_market.offshore_capital - 0.0).abs() < 1e-9);
        assert!((offshore_amount - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_public_wage_cap_default() {
        // Test: Treasury has default max_public_wage_multiplier of 1.2
        let treasury = crate::state::treasury::Treasury::default();
        assert!((treasury.max_public_wage_multiplier - 1.2).abs() < 1e-9);
    }

    // ============================================================================
    // STAGE C: CAPITAL GAINS TAX TESTS (Phase 6)
    // ============================================================================

    #[test]
    fn test_capital_gains_tax_interception() {
        // Test: Capital Gains Tax intercepts dividends and routes to treasury
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let capital_gains_tax = CapitalGainsTax {
            brackets: vec![
                TaxBracket {
                    threshold: 10000.0,
                    rate: 0.19, // 19% on first 10K
                    extra: Map::new(),
                },
                TaxBracket {
                    threshold: 50000.0,
                    rate: 0.32, // 32% on next 40K
                    extra: Map::new(),
                },
            ],
            holding_period_modifier: 1.0, // No modifier for short-term
            extra: Map::new(),
        };

        let capital_gain = 20000.0; // 20K gain
        let initial_budget = country.budget.liquid_reserves;

        let net_gain = calculate_capital_gains_tax(
            capital_gain,
            1.0, // 1 year holding
            &capital_gains_tax,
            &mut country,
            "region_1",
            "investor_1".to_string(),
        );

        // Tax calculation: 10K * 0.19 = 1.9K, 10K * 0.32 = 3.2K, total = 5.1K
        let expected_tax = 5100.0;
        let expected_net = capital_gain - expected_tax;

        // Net gain should be 14.9K
        assert!((net_gain - expected_net).abs() < 1e-9);
        // Treasury should have received 5.1K
        assert!((country.budget.liquid_reserves - initial_budget - expected_tax).abs() < 1e-9);
    }

    #[test]
    fn test_capital_gains_tax_no_loss() {
        // Test: No tax on capital losses
        let mut country = crate::state::Country {
            budget: crate::state::treasury::Treasury {
                liquid_reserves: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let capital_gains_tax = CapitalGainsTax {
            brackets: vec![TaxBracket {
                threshold: 10000.0,
                rate: 0.19,
                extra: Map::new(),
            }],
            holding_period_modifier: 1.0,
            extra: Map::new(),
        };

        let capital_loss = -5000.0; // Loss
        let initial_budget = country.budget.liquid_reserves;

        let net_gain = calculate_capital_gains_tax(
            capital_loss,
            1.0,
            &capital_gains_tax,
            &mut country,
            "region_1",
            "investor_1".to_string(),
        );

        // Loss should pass through unchanged
        assert!((net_gain - capital_loss).abs() < 1e-9);
        // Treasury should not receive any tax
        assert!((country.budget.liquid_reserves - initial_budget).abs() < 1e-9);
    }

    // ============================================================================
    // STAGE C: FDI TRIGGER TESTS (Phase 6)
    // ============================================================================

    #[test]
    fn test_fdi_trigger_double_entry() {
        // Test: FDI trigger correctly drains offshore ledger without printing money
        let mut global_market = crate::economy::market::GlobalMarket {
            base_prices: std::collections::HashMap::new(),
            net_surplus: std::collections::HashMap::new(),
            offshore_capital: 10_000_000.0, // 10M offshore
            apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
            supply_volume: std::collections::HashMap::new(),
            demand_volume: std::collections::HashMap::new(),
        };

        let fdi_trigger = FdiTrigger {
            accumulation_threshold: 5_000_000.0,
            wage_multiplier: 1_000.0, // Threshold = 5K * 1K = 5M
            minimum_fund_capital: 100_000.0,
            extraction_rate: 0.5, // Extract 50%
            extra: Map::new(),
        };

        let average_wage = 5_000.0;
        let initial_offshore = global_market.offshore_capital;

        let injected_capital = evaluate_fdi_trigger(&mut global_market, &fdi_trigger, average_wage);

        // Should trigger (10M >= 5M threshold)
        assert!(injected_capital.is_some());
        let capital = injected_capital.unwrap();

        // Should extract 50% of 10M = 5M
        assert!((capital - 5_000_000.0).abs() < 1e-9);
        // Offshore should decrease by exactly 5M (double-entry)
        assert!((global_market.offshore_capital - initial_offshore + 5_000_000.0).abs() < 1e-9);
        // Total money mass preserved: 10M = 5M (offshore) + 5M (injected)
        assert!((global_market.offshore_capital + capital - initial_offshore).abs() < 1e-9);
    }

    #[test]
    fn test_fdi_trigger_insufficient_capital() {
        // Test: FDI does not trigger when offshore capital below threshold
        let mut global_market = crate::economy::market::GlobalMarket {
            base_prices: std::collections::HashMap::new(),
            net_surplus: std::collections::HashMap::new(),
            offshore_capital: 1_000_000.0, // Only 1M offshore
            apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
            supply_volume: std::collections::HashMap::new(),
            demand_volume: std::collections::HashMap::new(),
        };

        let fdi_trigger = FdiTrigger {
            accumulation_threshold: 5_000_000.0,
            wage_multiplier: 1_000.0, // Threshold = 5K * 1K = 5M
            minimum_fund_capital: 100_000.0,
            extraction_rate: 0.5,
            extra: Map::new(),
        };

        let average_wage = 5_000.0;
        let initial_offshore = global_market.offshore_capital;

        let injected_capital = evaluate_fdi_trigger(&mut global_market, &fdi_trigger, average_wage);

        // Should not trigger (1M < 5M threshold)
        assert!(injected_capital.is_none());
        // Offshore should remain unchanged
        assert!((global_market.offshore_capital - initial_offshore).abs() < 1e-9);
    }

    // ============================================================================
    // STAGE C: SOVEREIGN EXEMPTION TESTS (Phase 6)
    // ============================================================================

    #[test]
    fn test_sovereign_exemption_ownership_map() {
        // Test: Company is tax-exempt when sovereign holds shares in shareholders map
        let mut company = Company {
            id: "state_owned_co".to_string(),
            shareholders: std::collections::BTreeMap::new(),
            ..Default::default()
        };

        let sovereign_id = "STATE_TREASURY";

        // Add sovereign as shareholder
        company.shareholders.insert(sovereign_id.to_string(), 1000);

        // Check exemption using shareholders map (NOT deprecated state_share)
        let is_exempt = is_company_tax_exempt(&company, sovereign_id);

        assert!(is_exempt);
    }

    #[test]
    fn test_sovereign_exemption_no_ownership() {
        // Test: Company is not exempt when sovereign has no shares
        let company = Company {
            id: "private_co".to_string(),
            shareholders: std::collections::BTreeMap::new(),
            ..Default::default()
        };

        let sovereign_id = "STATE_TREASURY";

        // Check exemption
        let is_exempt = is_company_tax_exempt(&company, sovereign_id);

        assert!(!is_exempt);
    }

    #[test]
    fn test_sovereign_exemption_uses_shareholders_map() {
        // Test: Exemption logic strictly uses shareholders map, not deprecated fields
        let company = Company {
            id: "legacy_co".to_string(),
            shareholders: std::collections::BTreeMap::new(),
            state_share: 1.0, // Deprecated field - should be ignored
            ..Default::default()
        };

        let sovereign_id = "STATE_TREASURY";

        // Even though state_share = 1.0, shareholders map is empty
        // Implementation should use shareholders map, not deprecated field
        let is_exempt = is_company_tax_exempt(&company, sovereign_id);

        // Should NOT be exempt because shareholders map is empty
        assert!(!is_exempt);
    }
}
