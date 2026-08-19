//! Dynamic social programs constructed by the Ministry of Social Welfare.
//!
//! Phase 13: Social Policy, NGOs & Religious Charities.
//!
//! Programs are physical transfers of fiat currency subject to strict
//! double-entry accounting. The Ministry AI constructs programs from
//! modular components (target conditions + benefit types), evaluates
//! their cost using per-capita means testing, and resolves funding
//! shortfalls via haircut or sovereign debt issuance.

#![allow(missing_docs)]

use crate::entities::Company;
use crate::politics::ideology::Ideology;
use crate::politics::ministries::{Ministry, MinistrySpendingAction};
use crate::registries::enums::Sector;
use crate::society::geography::ClassDemographics;
use crate::state::Country;
use serde::{Deserialize, Serialize};

// ============================================================================
// CORE STRUCTS
// ============================================================================

/// Who is eligible for this program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum TargetCondition {
    /// Default variant used by `Default` impl.
    #[default]
    /// All citizens regardless of wealth or demographics.
    Universal,
    /// Only citizens matching specific demographic criteria.
    Demographic {
        /// Minimum age in years (None = no lower bound).
        min_age: Option<f64>,
        /// Maximum age in years (None = no upper bound).
        max_age: Option<f64>,
        /// Must match this nationality (None = any).
        nationality: Option<String>,
        /// Must match this religion (None = any).
        religion: Option<String>,
    },
    /// Only citizens below a per-capita wealth threshold.
    MeansTested {
        /// Max savings/population to qualify.
        per_capita_threshold: f64,
        /// How benefits reduce above the threshold.
        taper: TaperMode,
    },
}

/// How benefits reduce above the means-test threshold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum TaperMode {
    /// Benefit drops to 0 immediately at threshold.
    #[default]
    HardCutoff,
    /// Benefit reduces linearly over `taper_range` above the threshold.
    /// benefit * max(0, 1 - (surplus / taper_range))
    MarginalTaper {
        /// Income band over which benefit goes from 100% to 0%.
        taper_range: f64,
    },
}

/// How the benefit is delivered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BenefitType {
    /// Direct cash transfer: Treasury.liquid_reserves -> ClassDemographics.savings.
    CashTransfer {
        /// Per-capita amount in nominal currency.
        per_capita_amount: f64,
    },
    /// Targeted B2C intervention: fund specific sector (e.g., public housing).
    TargetedIntervention {
        /// Per-capita amount in nominal currency.
        per_capita_amount: f64,
        /// Target sector for the intervention.
        target_sector: Sector,
    },
}

impl Default for BenefitType {
    fn default() -> Self {
        BenefitType::CashTransfer { per_capita_amount: 0.0 }
    }
}

/// A social program constructed by the Ministry of Social Welfare.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SocialProgram {
    /// Unique program ID (e.g., "SP-001").
    pub id: String,
    /// Display name (e.g., "Universal Pension").
    pub name: String,
    /// Eligibility condition.
    pub target: TargetCondition,
    /// Benefit delivery mechanism.
    pub benefit: BenefitType,
    /// Turn this program was enacted.
    pub enacted_turn: u32,
    /// Whether the program is currently active.
    pub is_active: bool,
}

// ============================================================================
// PROGRAM EVALUATION
// ============================================================================

/// Per-class evaluation result.
#[derive(Debug, Clone)]
pub struct ClassEligibility {
    /// Region ID.
    pub region_id: String,
    /// Class key (serde-serialized enum variant).
    pub class_key: String,
    /// Eligible population (may be less than total for means-tested).
    pub eligible_population: i64,
    /// Taper factor (1.0 = full benefit, 0.0 = no benefit).
    pub taper_factor: f64,
    /// Per-capita benefit amount after tapering.
    pub per_capita_benefit: f64,
    /// Total benefit for this class.
    pub total_benefit: f64,
}

/// Result of evaluating a program against a country's demographics.
#[derive(Debug, Clone)]
pub struct ProgramEvaluation {
    /// All eligible classes with their benefit amounts.
    pub eligible_classes: Vec<ClassEligibility>,
    /// Total program cost.
    pub total_cost: f64,
    /// Total eligible population.
    pub total_eligible_population: i64,
}

/// Check if a class is eligible and compute the taper factor.
///
/// # Rules
/// * Universal: all classes eligible, taper = 1.0
/// * Demographic: check age/nationality/religion filters, taper = 1.0
/// * MeansTested: per_capita_wealth = savings / population
///   - HardCutoff: eligible if per_capita_wealth < threshold, taper = 1.0
///   - MarginalTaper: eligible if per_capita_wealth < threshold + taper_range
///     taper = max(0, 1 - (surplus / taper_range))
fn check_eligibility(
    target: &TargetCondition,
    class: &ClassDemographics,
    benefit_amount: f64,
) -> (bool, f64) {
    let pop = class.population.max(1) as f64;
    match target {
        TargetCondition::Universal => (true, 1.0),
        TargetCondition::Demographic {
            min_age,
            max_age,
            nationality: _,
            religion,
        } => {
            // Age: we don't have per-class age data, so we use the class's
            // population as a proxy — if min_age/max_age is set, we assume
            // the class is eligible (age filtering is a future enhancement).
            let _ = (min_age, max_age);

            // Religion filter: check class religion if specified.
            if let Some(req_religion) = religion {
                if !class.religion.is_empty() && class.religion != *req_religion {
                    return (false, 0.0);
                }
            }
            (true, 1.0)
        }
        TargetCondition::MeansTested {
            per_capita_threshold,
            taper,
        } => {
            let per_capita_wealth = class.savings / pop;
            match taper {
                TaperMode::HardCutoff => {
                    if per_capita_wealth < *per_capita_threshold {
                        (true, 1.0)
                    } else {
                        (false, 0.0)
                    }
                }
                TaperMode::MarginalTaper { taper_range } => {
                    let surplus = per_capita_wealth - per_capita_threshold;
                    if surplus >= *taper_range {
                        (false, 0.0)
                    } else if surplus <= 0.0 {
                        (true, 1.0)
                    } else {
                        let factor = 1.0 - (surplus / taper_range);
                        (true, factor.max(0.0))
                    }
                }
            }
        }
    }
}

/// Evaluate a program's cost against a country's demographics.
///
/// # Arguments
/// * `program` - The social program to evaluate.
/// * `country` - Country with regions and class demographics.
///
/// # Returns
/// A `ProgramEvaluation` with per-class breakdown and total cost.
pub fn evaluate_program(program: &SocialProgram, country: &Country) -> ProgramEvaluation {
    let benefit_amount = match &program.benefit {
        BenefitType::CashTransfer { per_capita_amount } => *per_capita_amount,
        BenefitType::TargetedIntervention { per_capita_amount, .. } => *per_capita_amount,
    };

    let mut eligible_classes = Vec::new();
    let mut total_cost = 0.0;
    let mut total_eligible_population = 0;

    for region in &country.regions {
        // Rural classes
        for (class_key, demographics) in &region.class_demographics.rural_classes {
            let (eligible, taper_factor) =
                check_eligibility(&program.target, demographics, benefit_amount);
            if eligible && demographics.population > 0 {
                let per_capita_benefit = benefit_amount * taper_factor;
                let total_benefit = per_capita_benefit * demographics.population as f64;
                total_cost += total_benefit;
                total_eligible_population += demographics.population;
                eligible_classes.push(ClassEligibility {
                    region_id: region.id.clone(),
                    class_key: class_key.clone(),
                    eligible_population: demographics.population,
                    taper_factor,
                    per_capita_benefit,
                    total_benefit,
                });
            }
        }
        // Urban classes
        for (class_key, demographics) in &region.class_demographics.urban_classes {
            let (eligible, taper_factor) =
                check_eligibility(&program.target, demographics, benefit_amount);
            if eligible && demographics.population > 0 {
                let per_capita_benefit = benefit_amount * taper_factor;
                let total_benefit = per_capita_benefit * demographics.population as f64;
                total_cost += total_benefit;
                total_eligible_population += demographics.population;
                eligible_classes.push(ClassEligibility {
                    region_id: region.id.clone(),
                    class_key: class_key.clone(),
                    eligible_population: demographics.population,
                    taper_factor,
                    per_capita_benefit,
                    total_benefit,
                });
            }
        }
    }

    ProgramEvaluation {
        eligible_classes,
        total_cost,
        total_eligible_population,
    }
}

// ============================================================================
// PROGRAM CONSTRUCTION (MINISTRY AI)
// ============================================================================

/// Construct social programs based on ideology, fiscal health, and social unrest.
///
/// # Arguments
/// * `ministry` - The Ministry of Social Welfare.
/// * `country` - Country state for fiscal/unrest data.
/// * `current_turn` - The current turn number.
///
/// # Returns
/// A vector of `SocialProgram` structs to be stored on `Country.social_programs`.
///
/// # Rules
/// * Socialist/Left ideologies favor Universal + CashTransfer.
/// * Conservative ideologies favor MeansTested + TargetedIntervention.
/// * Low fiscal health → more MeansTested, lower amounts.
/// * High social unrest → broader targeting, higher amounts.
pub fn construct_social_programs(
    ministry: &Ministry,
    country: &Country,
    current_turn: u32,
) -> Vec<SocialProgram> {
    let ideology = country
        .politics
        .active_parties
        .get(&ministry.minister_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or(Ideology::SocialLiberalism);

    let fiscal_health = if country.budget.gdp > 0.0 {
        country.budget.liquid_reserves / country.budget.gdp
    } else {
        0.0
    };
    let unrest = country.macro_indicators.social_unrest;
    let avg_wage = country.macro_indicators.average_wage.max(1.0);

    let mut programs = Vec::new();

    // Determine program profile based on ideology.
    let (favors_universal, favors_cash, base_amount_multiplier) = match ideology {
        Ideology::OrthodoxMarxism
        | Ideology::MarxismLeninism
        | Ideology::Maoism
        | Ideology::SocialDemocracy => (true, true, 1.2),
        Ideology::GreenPolitics => (true, false, 1.0),
        Ideology::SocialLiberalism => (false, true, 1.0),
        Ideology::ChristianDemocracy => (false, false, 0.8),
        Ideology::Agrarianism => (false, true, 0.7),
        Ideology::ClassicalLiberalism
        | Ideology::Neoliberalism
        | Ideology::AnarchoCapitalism => (false, false, 0.5),
        Ideology::SocialConservatism
        | Ideology::Neoconservatism
        | Ideology::NationalConservatism => (false, false, 0.6),
        Ideology::Fascism => (false, true, 0.8),
    };

    // Unrest increases benefit amounts.
    let unrest_multiplier = 1.0 + (unrest / 100.0).min(0.5);
    let amount = avg_wage * 0.05 * base_amount_multiplier * unrest_multiplier;

    // Fiscal distress reduces amounts and shifts to means-tested.
    let low_fiscal = fiscal_health < 0.05;

    if favors_universal && !low_fiscal {
        // Universal cash transfer.
        programs.push(SocialProgram {
            id: format!("SP-{:03}", programs.len() + 1),
            name: "Universal Social Transfer".to_string(),
            target: TargetCondition::Universal,
            benefit: BenefitType::CashTransfer {
                per_capita_amount: amount,
            },
            enacted_turn: current_turn,
            is_active: true,
        });
    } else {
        // Means-tested program.
        let threshold = avg_wage * 5.0; // 5x average wage per capita
        let taper = if favors_cash {
            TaperMode::MarginalTaper {
                taper_range: avg_wage * 3.0,
            }
        } else {
            TaperMode::HardCutoff
        };

        let benefit = if favors_cash {
            BenefitType::CashTransfer {
                per_capita_amount: amount,
            }
        } else {
            BenefitType::TargetedIntervention {
                per_capita_amount: amount,
                target_sector: Sector::Construction, // public housing
            }
        };

        programs.push(SocialProgram {
            id: format!("SP-{:03}", programs.len() + 1),
            name: "Means-Tested Welfare".to_string(),
            target: TargetCondition::MeansTested {
                per_capita_threshold: threshold,
                taper,
            },
            benefit,
            enacted_turn: current_turn,
            is_active: true,
        });
    }

    // High unrest: add an emergency relief program.
    if unrest > 60.0 {
        programs.push(SocialProgram {
            id: format!("SP-{:03}", programs.len() + 1),
            name: "Emergency Relief".to_string(),
            target: TargetCondition::MeansTested {
                per_capita_threshold: avg_wage * 2.0,
                taper: TaperMode::HardCutoff,
            },
            benefit: BenefitType::CashTransfer {
                per_capita_amount: avg_wage * 0.03,
            },
            enacted_turn: current_turn,
            is_active: true,
        });
    }

    programs
}

// ============================================================================
// FUNDING DILEMMA RESOLUTION
// ============================================================================

/// How the state responds to a funding shortfall.
#[derive(Debug, Clone, PartialEq)]
pub enum FundingResponse {
    /// Sufficient funds — pay 100%.
    FullyFunded,
    /// Pay a proportional fraction of promised benefits.
    Haircut {
        /// Fraction of promised benefit to pay (0.0–1.0).
        payout_ratio: f64,
    },
    /// Issue sovereign debt to cover the shortfall, pay 100%.
    DebtIssuance {
        /// Amount of new debt to issue.
        shortfall: f64,
    },
}

/// Resolve the funding dilemma when program cost exceeds available cash.
///
/// # Arguments
/// * `total_cost` - Total program cost.
/// * `available_cash` - Cash available to the ministry.
/// * `ruling_ideology` - Ideology of the ruling party.
/// * `social_unrest` - Current social unrest (0–100).
/// * `fiscal_health` - liquid_reserves / gdp.
///
/// # Rules
/// * If `available >= cost`: FullyFunded.
/// * Conservative ideology OR fiscal_health < 0.05: Haircut.
/// * Socialist/Populist OR social_unrest > 60: DebtIssuance.
/// * Ambiguous: Haircut if fiscal_health < 0.15, else DebtIssuance.
pub fn resolve_funding_dilemma(
    total_cost: f64,
    available_cash: f64,
    ruling_ideology: Ideology,
    social_unrest: f64,
    fiscal_health: f64,
) -> FundingResponse {
    if available_cash >= total_cost {
        return FundingResponse::FullyFunded;
    }

    if available_cash <= 0.0 {
        // No cash at all — pay nothing. Phase 35: ministries cannot issue
        // sovereign debt beyond their allocation. The Treasury must allocate
        // more in the next budget bill if social programs need more funding.
        return FundingResponse::Haircut { payout_ratio: 0.0 };
    }

    let is_conservative = matches!(
        ruling_ideology,
        Ideology::SocialConservatism
            | Ideology::Neoconservatism
            | Ideology::NationalConservatism
            | Ideology::ClassicalLiberalism
            | Ideology::Neoliberalism
            | Ideology::AnarchoCapitalism
    );

    // Phase 35: All underfunded programs receive a Haircut (pro-rated payout).
    // The DebtIssuance path has been removed — ministries must NOT issue
    // sovereign debt beyond their allocated_cash cap. This was the root cause
    // of the Social Welfare cash leak (42.31M spent vs 322.2K allocated).
    let payout_ratio = available_cash / total_cost;

    if is_conservative || fiscal_health < 0.05 {
        return FundingResponse::Haircut { payout_ratio };
    }

    // Populist-left or high unrest: still a Haircut, but the Treasury should
    // be lobbied to increase the Social Welfare allocation in the next budget.
    FundingResponse::Haircut { payout_ratio }
}

// ============================================================================
// PROGRAM EXECUTION
// ============================================================================

/// Execute social programs: distribute benefits to eligible classes.
///
/// # Arguments
/// * `country` - Mutable country state (Treasury debited, classes credited).
/// * `programs` - Active social programs to execute.
/// * `companies` - Mutable companies (for TargetedIntervention).
/// * `ministry` - Mutable ministry (records spending actions).
/// * `current_turn` - Current turn number.
///
/// # Double-Entry
/// * CashTransfer: Debit Treasury.liquid_reserves, Credit ClassDemographics.savings
/// * TargetedIntervention: Debit Treasury.liquid_reserves, Credit Company.liquid_capital
/// * Phase 35: All payouts are capped at `available = allocated_cash - spent_cash`.
///   The DebtIssuance path has been removed — ministries cannot issue sovereign
///   debt beyond their allocation. Payouts debit `ministry_cash` (the pocket),
///   NOT `liquid_reserves` (which was already debited at allocation time).
pub fn execute_social_programs(
    country: &mut Country,
    programs: &[SocialProgram],
    companies: &mut [Company],
    ministry: &mut Ministry,
    _current_turn: u32,
) {
    let ruling_ideology = country
        .politics
        .active_parties
        .values()
        .next()
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or(Ideology::SocialLiberalism);

    let fiscal_health = if country.budget.gdp > 0.0 {
        country.budget.liquid_reserves / country.budget.gdp
    } else {
        0.0
    };
    let unrest = country.macro_indicators.social_unrest;

    for program in programs {
        if !program.is_active {
            continue;
        }

        let evaluation = evaluate_program(program, country);
        if evaluation.total_cost <= 0.0 || evaluation.eligible_classes.is_empty() {
            continue;
        }

        // Phase 35: available is the hard cap. No DebtIssuance beyond this.
        let available = ministry.allocated_cash - ministry.spent_cash;
        let funding = resolve_funding_dilemma(
            evaluation.total_cost,
            available,
            ruling_ideology,
            unrest,
            fiscal_health,
        );

        let actual_payout = match funding {
            FundingResponse::FullyFunded => evaluation.total_cost,
            FundingResponse::Haircut { payout_ratio } => {
                evaluation.total_cost * payout_ratio
            }
            FundingResponse::DebtIssuance { .. } => {
                // Phase 35: DebtIssuance is no longer produced by
                // resolve_funding_dilemma. This arm is unreachable but kept
                // for exhaustiveness. If it ever fires, cap at available.
                available
            }
        };

        // Phase 35: Hard cap — never spend more than available.
        let actual_payout = actual_payout.min(available).min(ministry.ministry_cash);
        if actual_payout <= 0.0 {
            continue;
        }

        let payout_ratio = actual_payout / evaluation.total_cost;

        // Distribute benefits.
        match &program.benefit {
            BenefitType::CashTransfer { .. } => {
                for elig in &evaluation.eligible_classes {
                    let benefit = elig.total_benefit * payout_ratio;
                    if benefit <= 0.0 {
                        continue;
                    }
                    // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                    ministry.ministry_cash -= benefit;
                    // Credit ClassDemographics.savings.
                    credit_class_savings(country, &elig.region_id, &elig.class_key, benefit);
                }
            }
            BenefitType::TargetedIntervention { target_sector, .. } => {
                // Distribute to companies in the target sector.
                let target_companies: Vec<usize> = companies
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.sector == *target_sector)
                    .map(|(i, _)| i)
                    .collect();
                if !target_companies.is_empty() {
                    let per_company = actual_payout / target_companies.len() as f64;
                    for idx in &target_companies {
                        companies[*idx].liquid_capital += per_company;
                        companies[*idx].available_cash += per_company;
                    }
                }
                // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                ministry.ministry_cash -= actual_payout;
            }
        }

        ministry.spent_cash += actual_payout;
        ministry.spending_actions.push(MinistrySpendingAction::DirectTransfer {
            target: format!("SocialProgram:{}", program.name),
            amount: actual_payout,
        });
    }
}

/// Credit savings to a specific class in a specific region.
///
/// # Arguments
/// * `country` - Mutable country.
/// * `region_id` - Region containing the class.
/// * `class_key` - Serde-serialized class key (e.g., "free_peasant").
/// * `amount` - Amount to credit.
fn credit_class_savings(country: &mut Country, region_id: &str, class_key: &str, amount: f64) {
    for region in &mut country.regions {
        if region.id != region_id {
            continue;
        }
        if let Some(demo) = region.class_demographics.rural_classes.get_mut(class_key) {
            demo.savings += amount;
            return;
        }
        if let Some(demo) = region.class_demographics.urban_classes.get_mut(class_key) {
            demo.savings += amount;
            return;
        }
    }
}

/// Execute the social welfare competency for a ministry.
///
/// This is the entry point called from the turn loop. It:
/// 1. Uses existing persisted programs if available.
/// 2. If no programs exist (first run), constructs them via Ministry AI.
/// 3. Evaluates and executes all active programs.
pub fn execute_social_welfare(
    country: &mut Country,
    companies: &mut [Company],
    current_turn: u32,
) {
    // Find the Social Welfare ministry.
    let (idx, _) = match country
        .politics
        .ministry_config
        .as_ref()
        .and_then(|c| {
            c.ministries
                .iter()
                .enumerate()
                .find(|(_, m)| {
                    m.competencies
                        .contains(&crate::politics::ministries::GovernmentCompetency::SocialWelfare)
                })
        }) {
        Some(x) => x,
        None => return,
    };

    // Check if we need to construct programs (budget year or empty).
    let is_budget_year = current_turn % 4 == 0;
    let needs_construction = is_budget_year || country.social_programs.is_empty();

    if needs_construction {
        // Clone ministry for construction (avoid borrow conflict).
        let ministry_clone = country.politics.ministry_config.as_ref()
            .map(|c| c.ministries[idx].clone())
            .unwrap();
        let new_programs = construct_social_programs(&ministry_clone, country, current_turn);
        country.social_programs = new_programs;
    }

    // Execute programs. Clone ministry out to avoid double mutable borrow of country.
    let programs = country.social_programs.clone();
    let mut ministry_opt = country.politics.ministry_config.as_mut()
        .and_then(|c| if idx < c.ministries.len() { Some(c.ministries[idx].clone()) } else { None });
    if let Some(ref mut ministry) = ministry_opt {
        execute_social_programs(country, &programs, companies, ministry, current_turn);
    }
    // Write back the modified ministry.
    if let Some(config) = country.politics.ministry_config.as_mut() {
        if let Some(m) = ministry_opt {
            if idx < config.ministries.len() {
                config.ministries[idx] = m;
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hard_cutoff_below_threshold() {
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::HardCutoff,
        };
        let class = ClassDemographics {
            population: 100,
            savings: 5000.0, // 50 per capita
            ..Default::default()
        };
        let (eligible, factor) = check_eligibility(&target, &class, 10.0);
        assert!(eligible);
        assert!((factor - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_hard_cutoff_above_threshold() {
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::HardCutoff,
        };
        let class = ClassDemographics {
            population: 100,
            savings: 15000.0, // 150 per capita
            ..Default::default()
        };
        let (eligible, _) = check_eligibility(&target, &class, 10.0);
        assert!(!eligible);
    }

    #[test]
    fn test_marginal_taper_partial() {
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::MarginalTaper {
                taper_range: 50.0,
            },
        };
        let class = ClassDemographics {
            population: 100,
            savings: 12500.0, // 125 per capita, surplus = 25
            ..Default::default()
        };
        let (eligible, factor) = check_eligibility(&target, &class, 10.0);
        assert!(eligible);
        // factor = 1 - 25/50 = 0.5
        assert!((factor - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_marginal_taper_below_threshold() {
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::MarginalTaper {
                taper_range: 50.0,
            },
        };
        let class = ClassDemographics {
            population: 100,
            savings: 5000.0, // 50 per capita
            ..Default::default()
        };
        let (eligible, factor) = check_eligibility(&target, &class, 10.0);
        assert!(eligible);
        assert!((factor - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_marginal_taper_above_range() {
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::MarginalTaper {
                taper_range: 50.0,
            },
        };
        let class = ClassDemographics {
            population: 100,
            savings: 20000.0, // 200 per capita, surplus = 100 > taper_range
            ..Default::default()
        };
        let (eligible, _) = check_eligibility(&target, &class, 10.0);
        assert!(!eligible);
    }

    #[test]
    fn test_funding_fully_funded() {
        let response = resolve_funding_dilemma(
            100.0,
            150.0,
            Ideology::SocialLiberalism,
            30.0,
            0.2,
        );
        assert_eq!(response, FundingResponse::FullyFunded);
    }

    #[test]
    fn test_funding_haircut_conservative() {
        let response = resolve_funding_dilemma(
            100.0,
            60.0,
            Ideology::SocialConservatism,
            30.0,
            0.2,
        );
        assert_eq!(response, FundingResponse::Haircut { payout_ratio: 0.6 });
    }

    #[test]
    fn test_funding_haircut_socialist() {
        // Phase 35: DebtIssuance removed — socialist governments now get a
        // Haircut (pro-rated payout) instead of issuing debt beyond allocation.
        let response = resolve_funding_dilemma(
            100.0,
            60.0,
            Ideology::SocialDemocracy,
            30.0,
            0.2,
        );
        assert_eq!(response, FundingResponse::Haircut { payout_ratio: 0.6 });
    }

    #[test]
    fn test_funding_haircut_high_unrest() {
        // Phase 35: DebtIssuance removed — high unrest now gets a Haircut.
        let response = resolve_funding_dilemma(
            100.0,
            60.0,
            Ideology::SocialLiberalism,
            70.0,
            0.2,
        );
        assert_eq!(response, FundingResponse::Haircut { payout_ratio: 0.6 });
    }

    #[test]
    fn test_per_capita_means_testing_large_population() {
        // Large population with same total savings should be eligible
        // (per-capita metric, not total wealth).
        let target = TargetCondition::MeansTested {
            per_capita_threshold: 100.0,
            taper: TaperMode::HardCutoff,
        };
        let class = ClassDemographics {
            population: 10000,
            savings: 500000.0, // 50 per capita — eligible
            ..Default::default()
        };
        let (eligible, _) = check_eligibility(&target, &class, 10.0);
        assert!(eligible);
    }

    #[test]
    fn test_religion_filter_match() {
        let target = TargetCondition::Demographic {
            min_age: None,
            max_age: None,
            nationality: None,
            religion: Some("Katolicyzm".to_string()),
        };
        let class = ClassDemographics {
            population: 100,
            religion: "Katolicyzm".to_string(),
            ..Default::default()
        };
        let (eligible, _) = check_eligibility(&target, &class, 10.0);
        assert!(eligible);
    }

    #[test]
    fn test_religion_filter_mismatch() {
        let target = TargetCondition::Demographic {
            min_age: None,
            max_age: None,
            nationality: None,
            religion: Some("Katolicyzm".to_string()),
        };
        let class = ClassDemographics {
            population: 100,
            religion: "Islam".to_string(),
            ..Default::default()
        };
        let (eligible, _) = check_eligibility(&target, &class, 10.0);
        assert!(!eligible);
    }
}
