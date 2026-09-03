//! Phase D7: Three-Pillar Pension System with Sovereign Liability
//!
//! Implements:
//! - Pillar 1: State PAYG (Pay-As-You-Go) with hard sovereign liabilities.
//!   Unpaid benefits are recorded as interest-bearing internal debt owed to
//!   specific demographic classes. No haircuts (Rule 1 + Rule 8).
//! - Pillar 2: Employer-matched capital pension funds (future phase).
//! - Pillar 3: Private tax-advantaged accounts (fix to existing shell).
//!
//! All flows are strict double-entry:
//! - Contributions: worker savings debited → Treasury credited (Pillar 1).
//! - Benefits: Treasury debited → retired class savings credited.
//! - Unfunded benefits: Treasury pays what it can; remainder → PensionLiability.

use serde::{Deserialize, Serialize};

use crate::state::macro_data::annual_to_per_turn_rate;
use crate::state::Country;

// ════════════════════════════════════════════════════════════════════════
// Configuration
// ════════════════════════════════════════════════════════════════════════

/// Pension system configuration. `None` on a country means no pension system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PensionLaw {
    /// Pillar 1: State PAYG configuration.
    #[serde(default)]
    pub pillar1: Option<Pillar1Config>,
    /// Pillar 2: Employer-matched capital pension (future phase).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pillar2: Option<Pillar2Config>,
    /// Pillar 3: Private tax-advantaged accounts (future phase).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pillar3: Option<Pillar3Config>,
}

/// Pillar 1: State Pay-As-You-Go pension.
///
/// Workers contribute a fraction of gross wages; retirees receive a fraction
/// of average_wage. If the Treasury cannot pay, the shortfall becomes a
/// sovereign liability with interest and unrest consequences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pillar1Config {
    /// Fraction of gross wages withheld as pension contribution (e.g., 0.15).
    pub contribution_rate: f64,
    /// Fraction of average_wage paid per retiree per year (e.g., 0.40).
    pub benefit_rate: f64,
    /// Minimum years of contributions required to collect benefits.
    pub min_years_of_contribution: u32,
    /// Annual interest rate on unfunded pension liabilities (e.g., 0.05).
    #[serde(default = "default_liability_interest")]
    pub unfunded_liability_interest_rate: f64,
    /// Unrest increase per turn per unit of (liability / average_wage).
    #[serde(default = "default_unrest_per_unit")]
    pub unfunded_liability_unrest_per_turn: f64,
    /// Crisis threshold: if total liability > crisis_threshold × annual_gdp,
    /// trigger pension crisis (mass unrest, government collapse risk).
    #[serde(default = "default_crisis_threshold")]
    pub crisis_threshold: f64,
    /// Max turns a liability can persist before radical opposition triggers.
    #[serde(default = "default_max_liability_turns")]
    pub max_liability_turns: u32,
}

fn default_liability_interest() -> f64 {
    0.05
}
fn default_unrest_per_unit() -> f64 {
    0.01
}
fn default_crisis_threshold() -> f64 {
    10.0
}
fn default_max_liability_turns() -> u32 {
    48
}

impl Default for Pillar1Config {
    fn default() -> Self {
        Pillar1Config {
            contribution_rate: 0.15,
            benefit_rate: 0.40,
            min_years_of_contribution: 10,
            unfunded_liability_interest_rate: default_liability_interest(),
            unfunded_liability_unrest_per_turn: default_unrest_per_unit(),
            crisis_threshold: default_crisis_threshold(),
            max_liability_turns: default_max_liability_turns(),
        }
    }
}

/// Pillar 2: Employer-matched capital pension (stub for future phase).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Pillar2Config {
    /// Fraction of gross wages contributed by the employer.
    pub employer_contribution_rate: f64,
    /// Fraction of gross wages contributed by the employee.
    pub employee_contribution_rate: f64,
    /// Number of turns before employer contributions vest.
    pub vesting_turns: u32,
}

/// Pillar 3: Private tax-advantaged accounts (stub for future phase).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Pillar3Config {
    /// Maximum contribution as a fraction of gross wages.
    pub contribution_limit_fraction_of_wage: f64,
    /// Penalty fraction applied to early withdrawals.
    pub early_withdrawal_penalty: f64,
}

// ════════════════════════════════════════════════════════════════════════
// Phase D7: Disability Pension
// ════════════════════════════════════════════════════════════════════════

/// Phase D7: Disability pension configuration.
///
/// Provides a state-funded pension for disabled citizens who cannot work
/// (or can only work at reduced capacity). This covers workplace accidents,
/// disaster casualties, inborn disability, and war-wounded veterans — all
/// routed through the same generic pool (Rule 10, Rule 14).
///
/// Benefits are real transfers: Treasury debited → class savings credited.
/// If the Treasury cannot pay, the shortfall becomes a `PensionLiability`
/// with interest and unrest consequences — no fiat creation (Rule 1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisabilityPensionConfig {
    /// Fraction of average_wage paid per fully disabled person per year.
    /// Partially disabled receive this × their severity factor.
    pub benefit_rate: f64,
    /// Whether partially disabled citizens (severity < 0.5) qualify.
    #[serde(default)]
    pub include_partial: bool,
    /// Annual interest rate on unfunded disability pension liabilities.
    #[serde(default = "default_liability_interest")]
    pub unfunded_liability_interest_rate: f64,
    /// Unrest increase per turn per unit of (liability / average_wage).
    #[serde(default = "default_unrest_per_unit")]
    pub unfunded_liability_unrest_per_turn: f64,
}

impl Default for DisabilityPensionConfig {
    fn default() -> Self {
        DisabilityPensionConfig {
            benefit_rate: 0.30,
            include_partial: false,
            unfunded_liability_interest_rate: default_liability_interest(),
            unfunded_liability_unrest_per_turn: default_unrest_per_unit(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// Sovereign Liability
// ════════════════════════════════════════════════════════════════════════

/// An unpaid PAYG pension obligation owed to a specific demographic class.
///
/// This is sovereign domestic debt — it carries interest and triggers unrest.
/// It is NEVER haircut or written off (Rule 1 + Rule 8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PensionLiability {
    /// Owed to which demographic class (e.g., "Nordia-Region1:Rural:Worker").
    pub class_key: String,
    /// In which region.
    pub region_id: String,
    /// Unpaid fiat amount.
    pub amount: f64,
    /// Turn when it was accrued.
    pub accrued_turn: u32,
    /// Annual interest rate.
    pub interest_rate: f64,
}

// ════════════════════════════════════════════════════════════════════════
// Per-Turn Processing
// ════════════════════════════════════════════════════════════════════════

/// Result of one turn of pension processing.
#[derive(Debug, Default, Clone)]
pub struct PensionTurnResult {
    /// Total contributions collected from workers (credited to Treasury).
    pub contributions_collected: f64,
    /// Total benefits paid to retirees (debited from Treasury).
    pub benefits_paid: f64,
    /// Total new unfunded liability accrued this turn.
    pub liability_accrued: f64,
    /// Total interest accrued on existing liabilities.
    pub interest_accrued: f64,
    /// Total liability repaid this turn (from Treasury surplus).
    pub liability_repaid: f64,
    /// Number of retirees receiving benefits.
    pub retiree_count: f64,
    /// Number of workers contributing.
    pub contributor_count: f64,
    /// Unrest increase from unfunded liabilities.
    pub unrest_increase: f64,
    /// Whether a pension crisis was triggered.
    pub crisis_triggered: bool,
}

/// Processes one turn of the pension system.
///
/// This must be called AFTER wage payment (so contributions can be withheld)
/// and BEFORE the end-of-turn treasury sweep (so surplus can repay liabilities).
///
/// # Flow
/// 1. Accrue interest on existing liabilities.
/// 2. Collect contributions from workers (Treasury credited).
/// 3. Pay benefits to retirees (Treasury debited).
/// 4. If Treasury cannot pay full benefits, record shortfall as liability.
/// 5. If Treasury has surplus, repay oldest liabilities (FIFO).
/// 6. Apply unrest from outstanding liabilities.
/// 7. Check for pension crisis.
pub fn process_pension_turn(country: &mut Country, turn: u32) -> PensionTurnResult {
    let mut result = PensionTurnResult::default();

    let law = match &country.pension_law {
        Some(l) if l.pillar1.is_some() => l.clone(),
        _ => return result, // No pension system.
    };
    let pillar1 = law.pillar1.unwrap();

    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    let gdp = country.budget.gdp.max(1.0);
    let per_turn_interest = annual_to_per_turn_rate(pillar1.unfunded_liability_interest_rate);

    // Country-level age shares (regions don't have their own age_groups).
    let adults_share = country
        .macro_indicators
        .demographics
        .age_groups
        .adults
        .max(0.0)
        .min(1.0);
    let elderly_share = country
        .macro_indicators
        .demographics
        .age_groups
        .elderly
        .max(0.0)
        .min(1.0);
    let labor_participation =
        country.macro_indicators.labor_market.labor_force_participation / 100.0;

    // Total national population (sum of regional populations).
    let total_pop: f64 = country.regions.iter().map(|r| r.population as f64).sum();

    // ── Step 1: Accrue interest on existing liabilities ──────────────
    for liability in &mut country.pension_liabilities {
        let interest = liability.amount * per_turn_interest;
        liability.amount += interest;
        result.interest_accrued += interest;
    }

    // ── Step 2: Collect contributions from workers ───────────────────
    // Contributions are withheld from wages. Since wages are already paid
    // by the time this runs (Phase 6 labor market), we collect from the
    // aggregate worker savings pool. The contribution_rate is applied to
    // the total wage bill (approximated by avg_wage × labor_force).
    //
    // Double-entry: class savings debited → Treasury credited.
    let contribution_rate_per_turn = annual_to_per_turn_rate(pillar1.contribution_rate);
    let mut total_contributions = 0.0;
    let mut total_contributors = 0.0;

    for region in &mut country.regions {
        let region_pop = region.population as f64;
        if region_pop <= 0.0 {
            continue;
        }
        let adult_pop = region_pop * adults_share;
        let labor_force = adult_pop * labor_participation;
        let regional_wage_bill = labor_force * avg_wage;
        let contribution = regional_wage_bill * contribution_rate_per_turn;

        // Debit from class savings proportionally.
        let total_class_savings: f64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|c| c.savings)
            .sum::<f64>()
            + region
                .class_demographics
                .urban_classes
                .values()
                .map(|c| c.savings)
                .sum::<f64>();

        if total_class_savings > 0.0 && contribution > 0.0 {
            let debit_fraction = (contribution / total_class_savings).min(0.5);
            let actual_contribution = total_class_savings * debit_fraction;

            for cd in region.class_demographics.rural_classes.values_mut() {
                let share = cd.savings / total_class_savings;
                let debit = actual_contribution * share;
                cd.savings -= debit;
            }
            for cd in region.class_demographics.urban_classes.values_mut() {
                let share = cd.savings / total_class_savings;
                let debit = actual_contribution * share;
                cd.savings -= debit;
            }
            total_contributions += actual_contribution;
            total_contributors += labor_force;
        }
    }

    country.budget.liquid_reserves += total_contributions;
    result.contributions_collected = total_contributions;
    result.contributor_count = total_contributors;

    // Update contribution history for each class (per region).
    for region in &country.regions {
        let region_pop = region.population as f64;
        if region_pop <= 0.0 {
            continue;
        }
        let adult_pop = region_pop * adults_share;
        if adult_pop > 0.0 {
            for rural_class in region.class_demographics.rural_classes.keys() {
                let key = format!("{}:Rural:{:?}", region.id, rural_class);
                *country.pension_contribution_history.entry(key).or_insert(0) += 1;
            }
            for urban_class in region.class_demographics.urban_classes.keys() {
                let key = format!("{}:Urban:{:?}", region.id, urban_class);
                *country.pension_contribution_history.entry(key).or_insert(0) += 1;
            }
        }
    }

    // ── Step 3: Pay benefits to retirees ─────────────────────────────
    // Benefits = benefit_rate × average_wage per retiree per year.
    // Only classes with sufficient contribution history are eligible.
    let benefit_rate_per_turn = annual_to_per_turn_rate(pillar1.benefit_rate);
    let benefit_per_retiree = avg_wage * benefit_rate_per_turn;
    let min_turns = pillar1.min_years_of_contribution * 24; // 24 turns/year

    // Collect (region_id, class_key, retiree_count, benefit_amount) tuples.
    let mut benefit_payments: Vec<(String, String, f64, f64)> = Vec::new();

    for region in &country.regions {
        let region_pop = region.population as f64;
        if region_pop <= 0.0 {
            continue;
        }
        let elderly_pop = region_pop * elderly_share;
        let n_rural = region.class_demographics.rural_classes.len().max(1);
        let n_urban = region.class_demographics.urban_classes.len().max(1);

        for rural_class in region.class_demographics.rural_classes.keys() {
            let history_key = format!("{}:Rural:{:?}", region.id, rural_class);
            let contribution_turns = country
                .pension_contribution_history
                .get(&history_key)
                .copied()
                .unwrap_or(0);
            if contribution_turns < min_turns {
                continue;
            }
            let class_key = format!("Rural:{:?}", rural_class);
            let retiree_count = elderly_pop / n_rural as f64;
            let benefit = retiree_count * benefit_per_retiree;
            if benefit > 0.0 {
                benefit_payments.push((region.id.clone(), class_key, retiree_count, benefit));
            }
        }
        for urban_class in region.class_demographics.urban_classes.keys() {
            let history_key = format!("{}:Urban:{:?}", region.id, urban_class);
            let contribution_turns = country
                .pension_contribution_history
                .get(&history_key)
                .copied()
                .unwrap_or(0);
            if contribution_turns < min_turns {
                continue;
            }
            let class_key = format!("Urban:{:?}", urban_class);
            let retiree_count = elderly_pop / n_urban as f64;
            let benefit = retiree_count * benefit_per_retiree;
            if benefit > 0.0 {
                benefit_payments.push((region.id.clone(), class_key, retiree_count, benefit));
            }
        }
    }

    let total_benefits_due: f64 = benefit_payments.iter().map(|(_, _, _, b)| *b).sum();
    let total_retirees: f64 = benefit_payments.iter().map(|(_, _, r, _)| *r).sum();
    result.retiree_count = total_retirees;

    // ── Step 4: Pay from Treasury; record shortfall as liability ─────
    let available = country.budget.liquid_reserves;
    let payable = available.min(total_benefits_due);

    if total_benefits_due > 0.0 {
        let pay_fraction = payable / total_benefits_due;

        for (region_id, class_key, _retirees, benefit) in &benefit_payments {
            let payment = benefit * pay_fraction;
            let shortfall = benefit - payment;

            if payment > 0.0 {
                credit_class_savings(country, region_id, class_key, payment);
            }

            if shortfall > 0.001 {
                country.pension_liabilities.push(PensionLiability {
                    class_key: format!("{}:{}", region_id, class_key),
                    region_id: region_id.clone(),
                    amount: shortfall,
                    accrued_turn: turn,
                    interest_rate: pillar1.unfunded_liability_interest_rate,
                });
                result.liability_accrued += shortfall;
            }
        }

        country.budget.liquid_reserves -= payable;
        result.benefits_paid = payable;
    }

    // ── Step 5: Repay oldest liabilities from surplus (FIFO) ─────────
    let surplus = country.budget.liquid_reserves;
    if surplus > 0.0 && !country.pension_liabilities.is_empty() {
        country.pension_liabilities.sort_by_key(|l| l.accrued_turn);
        let mut repayment_budget = surplus * 0.5; // Use 50% of surplus for liability repayment.
        let mut repaid = 0.0;
        let mut i = 0;
        while i < country.pension_liabilities.len() && repayment_budget > 0.0 {
            let liability_amount = country.pension_liabilities[i].amount;
            let repayment = liability_amount.min(repayment_budget);

            // Clone strings to avoid borrow conflict with credit_class_savings.
            let class_key_full = country.pension_liabilities[i].class_key.clone();
            let region_id = country.pension_liabilities[i].region_id.clone();
            // class_key_full format: "{region_id}:{class_key}" where
            // class_key = "Rural:Worker" or "Urban:Worker".
            if let Some(first_colon) = class_key_full.find(':') {
                let class_part = &class_key_full[first_colon + 1..];
                credit_class_savings(country, &region_id, class_part, repayment);
            }

            country.pension_liabilities[i].amount -= repayment;
            repaid += repayment;
            repayment_budget -= repayment;

            if country.pension_liabilities[i].amount < 0.001 {
                country.pension_liabilities.remove(i);
            } else {
                i += 1;
            }
        }

        if repaid > 0.0 {
            country.budget.liquid_reserves -= repaid;
            result.liability_repaid = repaid;
        }
    }

    // ── Step 6: Apply unrest from outstanding liabilities ────────────
    let total_liability: f64 = country.pension_liabilities.iter().map(|l| l.amount).sum();
    if total_liability > 0.0 {
        let liability_in_wage_units = total_liability / avg_wage;
        result.unrest_increase = pillar1.unfunded_liability_unrest_per_turn * liability_in_wage_units;
        let current_unrest = country
            .macro_indicators
            .extra
            .get("social_unrest")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        country.macro_indicators.extra.insert(
            "social_unrest".to_string(),
            serde_json::Value::from(current_unrest + result.unrest_increase),
        );
    }

    // ── Step 7: Check for pension crisis ─────────────────────────────
    if total_liability > pillar1.crisis_threshold * gdp {
        result.crisis_triggered = true;
        let current_unrest = country
            .macro_indicators
            .extra
            .get("social_unrest")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        country.macro_indicators.extra.insert(
            "social_unrest".to_string(),
            serde_json::Value::from(current_unrest + 30.0),
        );
    }

    // ── Step 8: Long-term liability persistence → radical opposition ─
    let max_turns = pillar1.max_liability_turns;
    let has_old_liabilities = country
        .pension_liabilities
        .iter()
        .any(|l| turn.saturating_sub(l.accrued_turn) > max_turns);
    if has_old_liabilities {
        let current_pressure = country
            .macro_indicators
            .extra
            .get("elderly_emigration_pressure")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        country.macro_indicators.extra.insert(
            "elderly_emigration_pressure".to_string(),
            serde_json::Value::from(current_pressure + 0.01),
        );
    }

    let _ = total_pop; // Suppress unused warning.
    result
}

/// Credits a class's savings by the given amount.
///
/// `class_key` format: "Rural:Worker" or "Urban:Worker" etc.
fn credit_class_savings(country: &mut Country, region_id: &str, class_key: &str, amount: f64) {
    let region = match country.regions.iter_mut().find(|r| r.id == region_id) {
        Some(r) => r,
        None => return,
    };

    let parts: Vec<&str> = class_key.splitn(2, ':').collect();
    if parts.len() != 2 {
        return;
    }
    let category = parts[0];
    let class_name = parts[1];

    if category == "Rural" {
        for (rc, cd) in &mut region.class_demographics.rural_classes {
            if format!("{:?}", rc) == class_name {
                cd.savings += amount;
                return;
            }
        }
    } else if category == "Urban" {
        for (uc, cd) in &mut region.class_demographics.urban_classes {
            if format!("{:?}", uc) == class_name {
                cd.savings += amount;
                return;
            }
        }
    }
}

/// Returns the total outstanding pension liability.
pub fn total_liability(country: &Country) -> f64 {
    country.pension_liabilities.iter().map(|l| l.amount).sum()
}

/// Returns the number of outstanding pension liabilities.
pub fn liability_count(country: &Country) -> usize {
    country.pension_liabilities.len()
}

/// Returns true if the pension system is in crisis (liability > threshold × GDP).
pub fn is_in_crisis(country: &Country) -> bool {
    let law = match &country.pension_law {
        Some(l) if l.pillar1.is_some() => l,
        _ => return false,
    };
    let pillar1 = law.pillar1.as_ref().unwrap();
    let total = total_liability(country);
    let gdp = country.budget.gdp.max(1.0);
    total > pillar1.crisis_threshold * gdp
}

// ════════════════════════════════════════════════════════════════════════
// Phase D7: Disability Pension Processing
// ════════════════════════════════════════════════════════════════════════

/// Result of one turn of disability pension processing.
#[derive(Debug, Default, Clone)]
pub struct DisabilityPensionTurnResult {
    /// Total benefits paid to disabled citizens (debited from Treasury).
    pub benefits_paid: f64,
    /// Total new unfunded liability accrued this turn.
    pub liability_accrued: f64,
    /// Number of disabled citizens receiving benefits.
    pub beneficiary_count: f64,
    /// Unrest increase from unfunded liabilities.
    pub unrest_increase: f64,
}

/// Phase D7: Process one turn of disability pension payments.
///
/// This runs after the main pension turn and uses the same liability
/// mechanism for shortfalls. Benefits are paid to `active_disabled`
/// citizens across all classes, scaled by disability severity.
///
/// # Flow
/// 1. Compute per-class benefit based on `active_disabled` count and severity.
/// 2. Pay from Treasury; record shortfall as `PensionLiability`.
/// 3. Apply unrest from outstanding liabilities.
///
/// No fiat is created — shortfalls become interest-bearing liabilities.
pub fn process_disability_pension_turn(
    country: &mut Country,
    config: &DisabilityPensionConfig,
    turn: u32,
) -> DisabilityPensionTurnResult {
    let mut result = DisabilityPensionTurnResult::default();

    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    let benefit_rate_per_turn = annual_to_per_turn_rate(config.benefit_rate);
    let benefit_per_disabled = avg_wage * benefit_rate_per_turn;

    // Collect (region_id, class_key, disabled_count, benefit_amount) tuples.
    let mut benefit_payments: Vec<(String, String, f64, f64)> = Vec::new();

    for region in &country.regions {
        for (rural_class, demo) in &region.class_demographics.rural_classes {
            if demo.active_disabled <= 0 {
                continue;
            }
            let severity = demo.disability_severity;
            if !config.include_partial && severity < 0.5 {
                continue;
            }
            // Benefit scales with severity: full disabled get 1.0×,
            // partial get severity/0.5 (capped at 1.0).
            let severity_factor = if severity >= 0.5 {
                1.0
            } else {
                (severity / 0.5).max(0.0)
            };
            let disabled_count = demo.active_disabled as f64;
            let benefit = disabled_count * benefit_per_disabled * severity_factor;
            if benefit > 0.0 {
                let class_key = format!("Rural:{:?}", rural_class);
                benefit_payments.push((region.id.clone(), class_key, disabled_count, benefit));
            }
        }
        for (urban_class, demo) in &region.class_demographics.urban_classes {
            if demo.active_disabled <= 0 {
                continue;
            }
            let severity = demo.disability_severity;
            if !config.include_partial && severity < 0.5 {
                continue;
            }
            let severity_factor = if severity >= 0.5 {
                1.0
            } else {
                (severity / 0.5).max(0.0)
            };
            let disabled_count = demo.active_disabled as f64;
            let benefit = disabled_count * benefit_per_disabled * severity_factor;
            if benefit > 0.0 {
                let class_key = format!("Urban:{:?}", urban_class);
                benefit_payments.push((region.id.clone(), class_key, disabled_count, benefit));
            }
        }
    }

    let total_benefits_due: f64 = benefit_payments.iter().map(|(_, _, _, b)| *b).sum();
    let total_beneficiaries: f64 = benefit_payments.iter().map(|(_, _, c, _)| *c).sum();
    result.beneficiary_count = total_beneficiaries;

    // Pay from Treasury; record shortfall as liability.
    let available = country.budget.liquid_reserves;
    let payable = available.min(total_benefits_due);

    if total_benefits_due > 0.0 {
        let pay_fraction = payable / total_benefits_due;

        for (region_id, class_key, _disabled, benefit) in &benefit_payments {
            let payment = benefit * pay_fraction;
            let shortfall = benefit - payment;

            if payment > 0.0 {
                credit_class_savings(country, region_id, class_key, payment);
            }

            if shortfall > 0.001 {
                country.pension_liabilities.push(PensionLiability {
                    class_key: format!("{}:{}", region_id, class_key),
                    region_id: region_id.clone(),
                    amount: shortfall,
                    accrued_turn: turn,
                    interest_rate: config.unfunded_liability_interest_rate,
                });
                result.liability_accrued += shortfall;
            }
        }

        country.budget.liquid_reserves -= payable;
        result.benefits_paid = payable;
    }

    // Apply unrest from new unfunded liabilities.
    if result.liability_accrued > 0.0 {
        let liability_in_wage_units = result.liability_accrued / avg_wage;
        result.unrest_increase =
            config.unfunded_liability_unrest_per_turn * liability_in_wage_units;
        let current_unrest = country
            .macro_indicators
            .extra
            .get("social_unrest")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        country.macro_indicators.extra.insert(
            "social_unrest".to_string(),
            serde_json::Value::from(current_unrest + result.unrest_increase),
        );
    }

    result
}
