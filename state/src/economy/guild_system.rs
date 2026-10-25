//! Phase 85: Guild system — lifecycle, production aggregation, dividends.
//!
//! Guilds are companies with LegalForm::Guild. They do NOT hire workers
//! directly (fulfilled_fte = 0). Member craftsmen allocate FTE to workshops
//! in HousingBuilding.commercial_slots. The guild coordinates:
//! - Raw material purchasing (B2B, from guild liquid_capital)
//! - Distribution to member workshops
//! - Production aggregation
//! - Finished goods sales (B2C/B2B with quality premium)
//! - Dividend distribution (pro-rata by production volume)
//! - Welfare fund management (HealthCapacity + EducationSlots)
//!
//! # Lifecycle (Rule 4)
//! - **Birth**: Macro-triggered when aggregate cottage_fte in a GuildBurgher
//!   domain exceeds a threshold. Seed capital extracted pro-rata from class savings.
//! - **Life**: Per-turn production cycle with inventory-based temporal causality.
//! - **Death**: Dissolution when below min_members for grace_turns.
//! - **Evolution**: Master craftsmen can break away to form FamilyBusiness/Cooperative.

#![allow(missing_docs)]

use crate::entities::legal_form::{GuildData, LegalForm};
use crate::entities::Company;
use crate::registries::enums::{Commodity, Sector};
use crate::society::geography::MicroRegion;
use std::collections::BTreeMap;

/// Configuration for guild system — no magic numbers (Rule 2).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuildConfig {
    /// Aggregate cottage_fte threshold in a domain/sector to trigger guild birth.
    /// Scaled by domain population (Rule 15).
    pub formation_fte_threshold: f64,
    /// Minimum members to sustain a guild (used for dissolution check).
    pub min_members: u32,
    /// Turns below min_members before dissolution.
    pub dissolution_grace_turns: u32,
    /// Seed capital = N × average_wage (dynamic, not magic number).
    pub min_seed_capital_wage_multiple: f64,
    /// Default welfare contribution rate (fraction of profits).
    pub default_welfare_contribution_rate: f64,
    /// Default quality standard for new guilds (financial premium only).
    pub default_quality_standard: f64,
    /// Share of member workshop revenue collected as guild dues each turn.
    /// The guild's income is bounded to a realistic fraction of what its
    /// member workshops actually produce (valued at market prices), instead
    /// of the old `liquid_capital * 0.1` book-equity proxy that let the
    /// "profit" estimate compound far beyond any real earnings.
    #[serde(default = "default_member_dues_rate")]
    pub member_dues_rate: f64,
    /// Hard cap on the welfare fund as a share of national GDP. Guilds are
    /// mutual-aid associations, not sovereign wealth funds — a fund larger
    /// than a few percent of GDP is an accounting artifact. Balances above
    /// `gdp * welfare_fund_gdp_cap_share` are swept into the member payout
    /// pool (and the unbacked accrual written down — see
    /// `plan_welfare_disbursement`), keeping every guild fund well under
    /// the 5%-of-GDP realism bound.
    #[serde(default = "default_welfare_fund_gdp_cap_share")]
    pub welfare_fund_gdp_cap_share: f64,
    /// Fraction of the welfare fund paid out to members each turn. The fund
    /// is a benefit pool (sickness, retirement, widows) — it must disburse
    /// regularly, not accumulate without bound. A 25%/turn payout yields a
    /// steady-state balance of ~4× the per-turn dues accrual.
    #[serde(default = "default_welfare_disbursement_rate")]
    pub welfare_disbursement_rate: f64,
}

fn default_member_dues_rate() -> f64 {
    0.15
}
fn default_welfare_fund_gdp_cap_share() -> f64 {
    0.04
}
fn default_welfare_disbursement_rate() -> f64 {
    0.25
}

impl Default for GuildConfig {
    fn default() -> Self {
        Self {
            formation_fte_threshold: 50.0,
            min_members: 3,
            dissolution_grace_turns: 12,
            min_seed_capital_wage_multiple: 50.0,
            default_welfare_contribution_rate: 0.10,
            default_quality_standard: 0.15,
            member_dues_rate: default_member_dues_rate(),
            welfare_fund_gdp_cap_share: default_welfare_fund_gdp_cap_share(),
            welfare_disbursement_rate: default_welfare_disbursement_rate(),
        }
    }
}

/// Result of guild production for a turn.
#[derive(Debug, Clone, Default)]
pub struct GuildProductionResult {
    /// Total physical output by commodity
    pub output: BTreeMap<Commodity, f64>,
    /// Raw materials consumed
    pub raw_consumed: BTreeMap<Commodity, f64>,
    /// Waste generated (for routing to WasteGridState)
    pub waste_generated: Vec<(Commodity, f64)>,
    /// Revenue from sales (after B2C/B2B clearing)
    pub revenue: f64,
    /// Profit = revenue - raw_material_costs - overhead
    pub profit: f64,
    /// Welfare contribution
    pub welfare_contribution: f64,
    /// Dividends distributed to members
    pub dividends_distributed: f64,
}

/// Check if a guild should be formed in a domain.
///
/// Macro-trigger (Fix 7): Uses aggregate cottage_fte, not individual craftsmen.
pub fn check_guild_formation_trigger(
    domain: &MicroRegion,
    cottage_fte_by_sector: &BTreeMap<String, f64>,
    config: &GuildConfig,
) -> Option<String> {
    if !matches!(
        domain.faction_type,
        crate::society::geography::FactionDomainType::GuildBurgher
    ) {
        return None;
    }

    // Scale threshold by domain population (Rule 15 — no flat rates)
    let pop_factor = (domain.population as f64 / 1000.0).max(1.0);
    let scaled_threshold = config.formation_fte_threshold * pop_factor;

    // Find the sector with the highest cottage_fte exceeding the threshold
    let mut best_sector: Option<(String, f64)> = None;
    for (sector, fte) in cottage_fte_by_sector {
        if *fte > scaled_threshold
            && (best_sector.is_none() || fte > &best_sector.as_ref().unwrap().1)
        {
            best_sector = Some((sector.clone(), *fte));
        }
    }

    best_sector.map(|(s, _)| s)
}

/// Compute the per-class seed-capital extraction for a new guild.
///
/// Pro-rata by cottage FTE share, capped at each class's actual savings.
/// Shared by `create_guild` (which credits the guild) and by callers that
/// must debit the same amounts from the real class savings (double-entry).
///
/// Returns `Vec<(class_id, extraction_amount)>`.
pub fn compute_seed_extraction(
    contributing_classes: &[(String, f64, f64)],
    average_wage: f64,
    config: &GuildConfig,
) -> Vec<(String, f64)> {
    let seed_capital = config.min_seed_capital_wage_multiple * average_wage;
    let total_cottage_fte: f64 = contributing_classes.iter().map(|(_, fte, _)| *fte).sum();
    if total_cottage_fte <= 0.0 {
        return Vec::new();
    }
    contributing_classes
        .iter()
        .map(|(class_id, fte_share, savings)| {
            let proportion = fte_share / total_cottage_fte;
            (class_id.clone(), (seed_capital * proportion).min(*savings))
        })
        .collect()
}

/// Create a new guild company.
///
/// Seed capital is extracted pro-rata from the aggregate savings of the
/// demographic classes that allocated cottage FTE to the triggering sector.
/// Double-entry: class.savings -= share, guild.liquid_capital += total_seed.
pub fn create_guild(
    domain: &MicroRegion,
    sector: Sector,
    contributing_classes: &[(String, f64, f64)], // (class_id, cottage_fte_share, savings)
    average_wage: f64,
    config: &GuildConfig,
) -> Company {
    // Extract seed capital pro-rata from contributing classes.
    // The CALLER is responsible for debiting each class's savings by the
    // amounts returned — the guild only receives what was really extracted
    // (double-entry, Rule 1).
    let extractions = compute_seed_extraction(contributing_classes, average_wage, config);
    let total_extracted: f64 = extractions.iter().map(|(_, amt)| *amt).sum();

    let sector_str = format!("{:?}", sector);
    let guild_id = format!("GUILD-{}-{}", domain.id, sector_str);

    let guild_data = GuildData {
        member_workshop_ids: Vec::new(),
        master_class_ids: contributing_classes
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect(),
        welfare_fund: 0.0,
        welfare_contribution_rate: config.default_welfare_contribution_rate,
        quality_standard: config.default_quality_standard,
        has_charter: true, // State-granted
        jurisdiction_domain_id: domain.id.clone(),
        guild_sector: sector_str.clone(),
        guild_raw_inventory: BTreeMap::new(),
    };

    let company = Company {
        id: guild_id.clone(),
        name: format!("{} Guild", sector_str),
        sector,
        region_id: domain.parent_region_id.clone(),
        legal_form: LegalForm::Guild(guild_data),
        liquid_capital: total_extracted,
        available_cash: total_extracted,
        ..Default::default()
    };

    company
}

/// Execute guild production for a turn.
///
/// Production consumes from `guild_raw_inventory` (purchased in Turn N-1).
/// Physical output = input_mass - waste_mass (Fix 1 — quality_standard is
/// financial only, NOT a physical multiplier).
///
/// # Arguments
/// * `company` - The guild company (must have LegalForm::Guild)
/// * `member_fte` - Total FTE allocated by members this turn
/// * `recipe_input` - Raw material commodity for this guild's sector
/// * `recipe_output` - Finished good commodity
/// * `input_per_unit` - Physical units of input per unit output
/// * `fte_per_unit` - FTE per unit output
/// * `waste_output` - Waste commodity
/// * `waste_per_unit` - Waste per unit output
#[allow(clippy::too_many_arguments)]
pub fn execute_guild_production(
    company: &mut Company,
    member_fte: f64,
    recipe_input: Commodity,
    recipe_output: Commodity,
    input_per_unit: f64,
    fte_per_unit: f64,
    waste_output: Commodity,
    waste_per_unit: f64,
) -> GuildProductionResult {
    let mut result = GuildProductionResult::default();

    // Get guild data
    let guild_data = match &mut company.legal_form {
        LegalForm::Guild(data) => data,
        _ => return result, // Not a guild
    };

    if member_fte <= 0.0 {
        return result;
    }

    // Available raw material from inventory (purchased in N-1)
    let available_raw = guild_data
        .guild_raw_inventory
        .get(&recipe_input)
        .copied()
        .unwrap_or(0.0);
    if available_raw <= 0.0 {
        return result;
    }

    // Max output from available raw material (mass conservation)
    let max_output_from_raw = available_raw / input_per_unit;

    // Max output from member FTE
    let max_output_from_fte = member_fte / fte_per_unit;

    // Actual output: limited by both raw material and FTE (Fix 5 + Rule 1)
    let output = max_output_from_raw.min(max_output_from_fte);
    if output <= 0.0 {
        return result;
    }

    // Consume raw material (mass conservation)
    let raw_consumed = output * input_per_unit;
    let current_raw = guild_data
        .guild_raw_inventory
        .entry(recipe_input)
        .or_insert(0.0);
    *current_raw = (*current_raw - raw_consumed).max(0.0);

    // Generate waste byproduct (mass conservation: input = output + waste)
    let waste = output * waste_per_unit;
    result.waste_generated.push((waste_output, waste));

    // Store output (quality_standard does NOT multiply physical volume — Fix 1)
    result.output.insert(recipe_output, output);
    result.raw_consumed.insert(recipe_input, raw_consumed);

    result
}

/// Distribute guild dividends and welfare contribution.
///
/// Double-entry:
/// - Welfare: profit * welfare_contribution_rate → welfare_fund
/// - Dividends: profit - welfare → distributed pro-rata by member production volume
///
/// # Arguments
/// * `company` - The guild company (mutated: welfare_fund updated)
/// * `profit` - Net profit this turn
/// * `member_production_shares` - (class_id, production_volume) pairs
pub fn distribute_guild_dividends(
    company: &mut Company,
    profit: f64,
    member_production_shares: &[(String, f64)], // (class_id, production_volume)
) -> BTreeMap<String, f64> {
    let mut dividends = BTreeMap::new();

    let guild_data = match &mut company.legal_form {
        LegalForm::Guild(data) => data,
        _ => return dividends,
    };

    if profit <= 0.0 {
        // No profit → no dividends. Welfare fund stays unchanged (clamped >= 0.0).
        return dividends;
    }

    // Welfare contribution
    let welfare = profit * guild_data.welfare_contribution_rate;
    guild_data.welfare_fund += welfare;

    // Dividends: profit - welfare, distributed pro-rata by production volume
    let dividend_pool = profit - welfare;
    let total_production: f64 = member_production_shares.iter().map(|(_, v)| *v).sum();

    if total_production > 0.0 {
        for (class_id, production) in member_production_shares {
            let share = dividend_pool * production / total_production;
            dividends.insert(class_id.clone(), share);
        }
    }

    dividends
}

/// Assess this turn's guild dues from member workshop revenue.
///
/// Member workshops' production for the turn is valued at real market
/// prices (`member_revenue`); the guild assesses
/// `member_revenue * config.member_dues_rate` in dues, split pro-rata
/// across the contributing classes by `member_shares`. This is the ONLY
/// income channel the guild has — the returned map is what the caller
/// debits from each member class's real savings and credits into the
/// guild's `available_cash` (strict double-entry; per-class debits are
/// clamped to what that class actually holds).
///
/// # Arguments
/// * `company` - The guild company (read-only).
/// * `member_revenue` - Market value of this turn's guild production.
/// * `member_shares` - (class_id, weight) pairs — same weights used for
///   dividend distribution, so dues and payouts share the member split.
/// * `config` - Guild configuration (`member_dues_rate`).
pub fn assess_guild_dues(
    company: &Company,
    member_revenue: f64,
    member_shares: &[(String, f64)],
    config: &GuildConfig,
) -> BTreeMap<String, f64> {
    let mut dues = BTreeMap::new();
    if !matches!(company.legal_form, LegalForm::Guild(_)) {
        return dues;
    }
    let pool = member_revenue * config.member_dues_rate;
    if !pool.is_finite() || pool <= 0.0 {
        return dues;
    }
    let total_weight: f64 = member_shares.iter().map(|(_, w)| *w).sum();
    if total_weight <= 0.0 {
        return dues;
    }
    for (class_id, weight) in member_shares {
        let share = pool * weight / total_weight;
        if share > 0.0 {
            *dues.entry(class_id.clone()).or_insert(0.0) += share;
        }
    }
    dues
}

/// Plan this turn's welfare-fund disbursement to members.
///
/// Two payout components, both real claims on guild cash:
/// 1. **Regular disbursement**: `welfare_fund * welfare_disbursement_rate` —
///    the fund pays out a share of its balance every turn (it is a mutual
///    benefit pool, not a hoard).
/// 2. **GDP cap sweep**: any balance above
///    `gdp * welfare_fund_gdp_cap_share` is scheduled for disbursement so
///    the fund cannot exceed a realistic share of national output.
///
/// The fund balance is reduced by the full planned amount here. The caller
/// performs the double-entry transfer — debiting `available_cash` and
/// crediting member savings — clamped to the cash actually on hand.
///
/// Historical note: legacy saves may carry welfare-fund balances that were
/// accrued against a `liquid_capital`-proxy profit estimate rather than
/// real member revenue and were never backed by segregated cash. The GDP
/// cap therefore also writes the unbacked excess down to the cap: the
/// claim is extinguished up to `cap` without a matching cash movement
/// (the accrual itself never moved cash either — it is a pure ledger
/// correction, M0-neutral). Members still receive the largest cash-backed
/// disbursement the guild can actually fund.
///
/// # Returns
/// The planned cash disbursement (≤ `welfare_fund` before reduction).
pub fn plan_welfare_disbursement(
    company: &mut Company,
    gdp: f64,
    config: &GuildConfig,
) -> f64 {
    let guild_data = match &mut company.legal_form {
        LegalForm::Guild(data) => data,
        _ => return 0.0,
    };
    let fund = guild_data.welfare_fund;
    if !fund.is_finite() || fund <= 0.0 {
        guild_data.welfare_fund = fund.max(0.0);
        return 0.0;
    }

    let cap = (gdp * config.welfare_fund_gdp_cap_share).max(0.0);
    let regular = fund * config.welfare_disbursement_rate;
    let excess_over_cap = (fund - cap).max(0.0);
    let planned = (regular + excess_over_cap).min(fund);

    // Reduce the claim by what is scheduled to be paid; if the fund still
    // exceeds the cap after scheduling (e.g. a legacy accrual dwarfs real
    // cash), write the unbacked remainder down to the cap. Cash actually
    // paid is subtracted below only once — the claim reduction IS the
    // accounting entry for the disbursement.
    guild_data.welfare_fund = (fund - planned).min(cap).max(0.0);

    planned
}

/// Check if a guild should dissolve.
///
/// Returns true if the guild has been below min_members for grace_turns.
pub fn check_guild_dissolution(
    company: &Company,
    turns_below_min: u32,
    config: &GuildConfig,
) -> bool {
    let member_count = match &company.legal_form {
        LegalForm::Guild(data) => data.member_workshop_ids.len() as u32,
        _ => return false,
    };

    member_count < config.min_members && turns_below_min >= config.dissolution_grace_turns
}

/// Dissolve a guild — distribute welfare fund to remaining members.
///
/// Double-entry: welfare_fund → member class savings pro-rata.
pub fn dissolve_guild(
    company: &mut Company,
    remaining_members: &[(String, f64)], // (class_id, share_weight)
) -> BTreeMap<String, f64> {
    let mut distributions = BTreeMap::new();

    let guild_data = match &mut company.legal_form {
        LegalForm::Guild(data) => data,
        _ => return distributions,
    };

    let welfare = guild_data.welfare_fund;
    if welfare <= 0.0 {
        return distributions;
    }

    let total_weight: f64 = remaining_members.iter().map(|(_, w)| *w).sum();
    if total_weight <= 0.0 {
        return distributions;
    }

    for (class_id, weight) in remaining_members {
        let share = welfare * weight / total_weight;
        distributions.insert(class_id.clone(), share);
    }

    // Clear welfare fund (all distributed)
    guild_data.welfare_fund = 0.0;

    distributions
}

/// Submit B2B buy orders for raw materials to replenish guild inventory.
///
/// Called during the B2B order submission phase. Materials will arrive
/// at end of turn via B2B settlement and be stored in guild_raw_inventory
/// for consumption in Turn N+1 (Fix 5 — temporal causality).
///
/// Returns: (commodity, quantity, max_price) for B2B order submission.
pub fn plan_guild_raw_material_purchase(
    company: &Company,
    target_output: f64,
    recipe_input: Commodity,
    input_per_unit: f64,
    market_price: f64,
) -> Option<(Commodity, f64, f64)> {
    let guild_data = match &company.legal_form {
        LegalForm::Guild(data) => data,
        _ => return None,
    };

    if target_output <= 0.0 {
        return None;
    }

    // How much raw material do we need?
    let needed = target_output * input_per_unit;

    // How much do we already have in inventory?
    let current = guild_data
        .guild_raw_inventory
        .get(&recipe_input)
        .copied()
        .unwrap_or(0.0);
    let to_buy = (needed - current).max(0.0);

    if to_buy <= 0.0 {
        return None;
    }

    // Max price: guild can afford from available_cash
    let affordable_qty = if market_price > 0.0 {
        company.available_cash / market_price
    } else {
        to_buy
    };

    let buy_qty = to_buy.min(affordable_qty);
    if buy_qty <= 0.0 {
        return None;
    }

    Some((recipe_input, buy_qty, market_price))
}

/// Get the welfare fund amount for a guild.
pub fn get_welfare_fund(company: &Company) -> f64 {
    match &company.legal_form {
        LegalForm::Guild(data) => data.welfare_fund,
        _ => 0.0,
    }
}

/// Get the quality standard for a guild (financial premium only — Fix 1).
pub fn get_quality_standard(company: &Company) -> f64 {
    match &company.legal_form {
        LegalForm::Guild(data) => data.quality_standard,
        _ => 0.0,
    }
}

/// Get the guild's sector string.
pub fn get_guild_sector(company: &Company) -> Option<String> {
    match &company.legal_form {
        LegalForm::Guild(data) => Some(data.guild_sector.clone()),
        _ => None,
    }
}
