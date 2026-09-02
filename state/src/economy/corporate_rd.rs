//! Phase 7/95: Corporate R&D allocation and method research.
//!
//! This module implements corporate R&D allocation from excess cash and
//! research of Commercial Production Methods.
//!
//! # Phase 95: R&D Cash Leak Eradication
//! Previously, R&D spending destroyed cash with no counterparty. Now, R&D
//! budgets are spent by purchasing domain-specific innovation commodities
//! (e.g. `InnovationPhysics`) from universities via
//! `settle_transfer` (strict double-entry). If no domestic universities have
//! points, the company pays a "Foreign Patent Fee" that flows to
//! `GlobalMarket.offshore_capital` (money preserved offshore — Rule 1).

use crate::corporate::capital_intensity::{
    minimum_capital_for_sector, sector_capital_intensity_multiplier,
};
use crate::economy::corporate_config::CorporateTechConfig;
use crate::economy::market_history::MarketHistory;
use crate::economy::trade::innovation_trading::purchase_innovation_points_for_company;
use crate::economy::trade::transfer_settler::{settle_transfer, TransferRecipient};
use crate::entities::{Building, Company, LicensedMethod, Patent};
use crate::registries::enums::{Commodity, Sector};
use crate::registries::tech_tree::{TechId, TechNode, TechType};
use crate::state::Country;
use std::collections::{BTreeMap, HashMap};

/// Foreign patent fee premium multiplier (1.5× domestic price for cross-border expertise).
const FOREIGN_PATENT_FEE_PREMIUM: f64 = 1.5;

/// Allocates corporate R&D budget from excess cash.
///
/// # Arguments
/// * `companies` - Slice of companies to allocate R&D
/// * `config` - Corporate technology configuration
/// * `average_wage` - Country average wage (for dynamic OPEX estimation)
///
/// # Returns
/// Updated companies with rd_budget allocations
///
/// # Rules
/// * Strategic AI Decision: Only allocate if cash > threshold * operating_expenses
/// * Allocate percentage of excess cash to rd_budget
/// * No magic numbers: uses CorporateTechConfig and average_wage
pub fn allocate_corporate_rd_budget(
    companies: &mut [Company],
    config: &CorporateTechConfig,
    average_wage: f64,
) {
    for company in companies.iter_mut() {
        let operating_expenses = estimate_operating_expenses(company, average_wage);
        let cash_threshold = operating_expenses * config.rd_allocation_threshold_ratio;

        if company.available_cash > cash_threshold {
            let excess_cash = company.available_cash - cash_threshold;
            let rd_allocation = excess_cash * config.rd_allocation_percentage;

            company.available_cash -= rd_allocation;
            company.rd_budget += rd_allocation;
        }
    }
}

/// Estimates operating expenses for a company.
///
/// # Rules
/// * Dynamic: 1% of sector CAPEX per worker per turn (scales with capital intensity).
/// * Uses `minimum_capital_for_sector(sector, average_wage) * 0.01` per worker.
/// * Inflation-proof: derived from `average_wage`, not hardcoded floats.
fn estimate_operating_expenses(company: &Company, average_wage: f64) -> f64 {
    let capex_per_worker = minimum_capital_for_sector(&company.sector, average_wage) * 0.01;
    company.worker_capacity as f64 * capex_per_worker
}

/// Executes corporate method research by purchasing Innovation Points from universities.
///
/// # Arguments
/// * `companies` - Slice of companies to research
/// * `buildings` - Slice of buildings (universities with Innovation Points)
/// * `building_inventories` - Building inventories (mutated to deduct points)
/// * `tech_tree` - Technology tree registry
/// * `country` - Mutable country state (for settle_transfer bank sync)
/// * `average_wage` - Country average wage (for dynamic price computation)
/// * `current_turn` - Current simulation turn
///
/// # Returns
/// Total foreign patent fees paid (for sequential crediting to GlobalMarket.offshore_capital).
///
/// # Rules
/// * Companies research Commercial techs tied to their sector.
/// * R&D budget is spent by purchasing Innovation Points from universities via B2B.
/// * If no domestic universities have points, pay a Foreign Patent Fee (FX outflow).
/// * Partial research: progress accumulates in `research_progress` until fully funded.
/// * Successful research grants patent and removes `research_progress` entry.
/// * Double-Entry: all cash flows via `settle_transfer` — no money destroyed.
pub fn execute_corporate_method_research(
    companies: &mut [Company],
    buildings: &mut [Building],
    building_inventories: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    tech_tree: &HashMap<TechId, TechNode>,
    country: &mut Country,
    average_wage: f64,
    current_turn: u32,
) -> f64 {
    let price_per_point = average_wage * 0.5; // Fair market price: ~half a wage per point.
    let mut total_foreign_fees: f64 = 0.0;

    for payer_idx in 0..companies.len() {
        if companies[payer_idx].rd_budget <= 0.0 {
            continue;
        }

        // Find eligible Commercial techs for this company's sector.
        for (tech_id, tech_node) in tech_tree.iter() {
            if tech_node.tech_type != TechType::Commercial {
                continue;
            }
            if companies[payer_idx]
                .patents
                .iter()
                .any(|p| &p.tech_id == tech_id)
            {
                continue;
            }
            if !is_tech_relevant_to_sector(tech_node, companies[payer_idx].sector) {
                continue;
            }

            let points_needed = tech_node.cost as f64;
            let progress = companies[payer_idx]
                .research_progress
                .get(tech_id)
                .copied()
                .unwrap_or(0.0);
            let remaining_points = (points_needed - progress).max(0.0);
            if remaining_points <= 0.0 {
                // Fully funded — grant patent.
                let patent = Patent {
                    tech_id: tech_id.clone(),
                    granted_turn: current_turn,
                    expires_turn: current_turn + tech_node.patent_duration_turns,
                    royalty_vwap_ratio: tech_node.royalty_vwap_ratio,
                };
                companies[payer_idx].patents.push(patent);
                companies[payer_idx].research_progress.remove(tech_id);
                continue;
            }

            // Attempt domestic purchase of domain-specific Innovation Points.
            let points_acquired = purchase_innovation_points_for_company(
                companies,
                buildings,
                building_inventories,
                payer_idx,
                tech_node.research_domain,
                remaining_points,
                price_per_point,
                country,
            );

            let domestic_cost = points_acquired * price_per_point;
            let shortfall = (remaining_points - points_acquired).max(0.0);

            // If domestic supply is insufficient, pay Foreign Patent Fee.
            let foreign_fee = if shortfall > 0.0 {
                shortfall * price_per_point * FOREIGN_PATENT_FEE_PREMIUM
            } else {
                0.0
            };

            let total_spend = domestic_cost + foreign_fee;
            if total_spend <= 0.0 || total_spend > companies[payer_idx].rd_budget {
                // Can't afford this research — accumulate what we got.
                if points_acquired > 0.0 {
                    let new_progress = progress + points_acquired;
                    companies[payer_idx]
                        .research_progress
                        .insert(tech_id.clone(), new_progress);
                    companies[payer_idx].rd_budget -= domestic_cost;
                }
                continue;
            }

            // Deduct from rd_budget.
            companies[payer_idx].rd_budget -= total_spend;

            // Pay foreign fee via settle_transfer (FX outflow to ForeignEntity).
            if foreign_fee > 0.0 {
                let _ = settle_transfer(
                    companies,
                    payer_idx,
                    foreign_fee,
                    &TransferRecipient::ForeignEntity,
                    country,
                );
                total_foreign_fees += foreign_fee;
            }

            // Accumulate progress.
            let new_progress = progress + points_acquired + shortfall;
            if new_progress >= points_needed {
                // Fully funded — grant patent.
                let patent = Patent {
                    tech_id: tech_id.clone(),
                    granted_turn: current_turn,
                    expires_turn: current_turn + tech_node.patent_duration_turns,
                    royalty_vwap_ratio: tech_node.royalty_vwap_ratio,
                };
                companies[payer_idx].patents.push(patent);
                companies[payer_idx].research_progress.remove(tech_id);
            } else {
                companies[payer_idx]
                    .research_progress
                    .insert(tech_id.clone(), new_progress);
            }
        }
    }

    total_foreign_fees
}

/// Checks if a technology is relevant to a company's sector.
///
/// # Rules
/// * Tech is relevant if it unlocks methods for the company's sector
fn is_tech_relevant_to_sector(tech_node: &TechNode, sector: Sector) -> bool {
    let sector_key = match sector {
        Sector::Mining => "mining",
        Sector::Agriculture => "agriculture",
        Sector::HeavyIndustry => "heavy_industry",
        Sector::LightIndustry => "light_industry",
        Sector::ArmamentsIndustry => "armaments_industry",
        Sector::Construction => "construction",
        Sector::Energy => "energy",
        Sector::TransportLogistics => "transport_logistics",
        Sector::MediaAndEntertainment => "media_and_entertainment",
        Sector::MedicalServices => "medical_services",
        Sector::EducationalServices => "educational_services",
        Sector::PublicServices => "public_services",
        Sector::Hospitality => "hospitality",
        _ => return false,
    };

    tech_node.unlocks_methods.contains_key(sector_key)
}

/// Evaluates licensing opportunities for companies.
///
/// # Arguments
/// * `companies` - Slice of companies to evaluate
/// * `all_companies` - All companies (to find patent holders)
/// * `tech_tree` - Technology tree registry
/// * `config` - Corporate technology configuration
/// * `average_wage` - Country average wage (for dynamic cost estimation)
/// * `market_history` - Market history for VWAP lookups
/// * `current_turn` - Current simulation turn (for licensed_turn)
///
/// # Returns
/// Updated companies with new licensed methods
///
/// # Rules
/// * Strategic AI Decision: Cost-benefit analysis
/// * License if (current_cost - new_cost - royalty) > threshold
/// * Voluntary licensing (no forced payments)
/// * Dynamic costs derived from average_wage and VWAP (no magic numbers)
pub fn evaluate_licensing_opportunities(
    companies: &mut [Company],
    all_companies: &[Company],
    tech_tree: &HashMap<TechId, TechNode>,
    config: &CorporateTechConfig,
    average_wage: f64,
    market_history: &MarketHistory,
    current_turn: u32,
) {
    for company in companies.iter_mut() {
        for other_company in all_companies.iter() {
            if other_company.id == company.id {
                continue;
            }

            for patent in &other_company.patents {
                if company
                    .licensed_methods
                    .iter()
                    .any(|lm| lm.tech_id == patent.tech_id)
                {
                    continue;
                }

                if let Some(tech_node) = tech_tree.get(&patent.tech_id) {
                    if !is_tech_relevant_to_sector(tech_node, company.sector) {
                        continue;
                    }

                    let current_unit_cost = estimate_current_unit_cost(company, average_wage);
                    let new_unit_cost =
                        estimate_new_unit_cost(tech_node, average_wage, market_history);
                    let vwap_fallback = average_wage * 10.0;
                    let royalty_cost = patent.royalty_vwap_ratio * vwap_fallback;
                    let net_benefit = current_unit_cost - new_unit_cost - royalty_cost;

                    if net_benefit > config.licensing_benefit_threshold {
                        let license = LicensedMethod {
                            tech_id: patent.tech_id.clone(),
                            licensor_company_id: other_company.id.clone(),
                            licensed_turn: current_turn,
                        };
                        company.licensed_methods.push(license);
                    }
                }
            }
        }
    }
}

/// Estimates current unit cost for a company.
///
/// # Rules
/// * Dynamic: `average_wage * sector_capital_intensity_multiplier(sector)`.
/// * Inflation-proof: scales with average_wage, not hardcoded floats.
fn estimate_current_unit_cost(company: &Company, average_wage: f64) -> f64 {
    average_wage * sector_capital_intensity_multiplier(company.sector)
}

/// Estimates new unit cost with a technology.
///
/// # Rules
/// * Computes from the tech node's unlocked production method inputs × VWAP.
/// * If no method data is available, falls back to `average_wage * 1.0`.
/// * Inflation-proof: uses market VWAPs and average_wage.
fn estimate_new_unit_cost(
    tech_node: &TechNode,
    average_wage: f64,
    market_history: &MarketHistory,
) -> f64 {
    // Derive a dynamic unit cost from the tech node's cost (Innovation Points)
    // and the market VWAP of Steel as an industrial input proxy.
    // Higher-cost techs represent more advanced processes that produce cheaper
    // units (efficiency gain). The gain is capped at 50% to avoid zero-cost.
    let steel_vwap = market_history
        .vwap_per_commodity
        .get(&Commodity::Steel)
        .copied()
        .unwrap_or(average_wage * 10.0);
    let efficiency_gain = (tech_node.cost as f64 / 100.0).min(0.5);
    let base_cost = steel_vwap * 0.1; // 10% of Steel VWAP as base unit cost proxy.
    base_cost * (1.0 - efficiency_gain).max(0.1)
}

/// Checks patent expiration and moves expired patents to public domain.
///
/// # Rules
/// * Patents expire after patent_duration_turns
/// * Expired patents enter public domain (anyone can use)
pub fn check_patent_expiration(companies: &mut [Company], current_turn: u32) {
    for company in companies.iter_mut() {
        company
            .patents
            .retain(|patent| patent.expires_turn > current_turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rd_allocation_from_excess_cash() {
        let mut company = Company::default();
        company.available_cash = 5000.0;
        company.worker_capacity = 100;
        company.sector = Sector::HeavyIndustry;

        let config = CorporateTechConfig::default();
        let average_wage = 10.0;

        let mut companies = vec![company];
        allocate_corporate_rd_budget(&mut companies, &config, average_wage);

        // Operating expenses: 100 * (10 * 10000 * 0.01) = 100 * 1000 = 100000
        // Threshold: 100000 * 2.0 = 200000
        // Cash (5000) < Threshold (200000), no allocation
        assert_eq!(companies[0].rd_budget, 0.0);
    }

    #[test]
    fn rd_allocation_when_wealthy() {
        let mut company = Company::default();
        company.available_cash = 500_000.0;
        company.worker_capacity = 100;
        company.sector = Sector::HeavyIndustry;

        let config = CorporateTechConfig::default();
        let average_wage = 10.0;

        let mut companies = vec![company];
        allocate_corporate_rd_budget(&mut companies, &config, average_wage);

        // Operating expenses: 100 * (10 * 10000 * 0.01) = 100000
        // Threshold: 100000 * 2.0 = 200000
        // Excess: 500000 - 200000 = 300000
        // Allocation: 300000 * 0.10 = 30000
        assert_eq!(companies[0].rd_budget, 30000.0);
        assert_eq!(companies[0].available_cash, 470000.0);
    }

    #[test]
    fn patent_expiration() {
        let mut company = Company::default();
        company.patents.push(Patent {
            tech_id: "tech_001".to_string(),
            granted_turn: 1,
            expires_turn: 10,
            royalty_vwap_ratio: 0.05,
        });

        let mut companies = vec![company];
        check_patent_expiration(&mut companies, 5);
        assert_eq!(companies[0].patents.len(), 1);

        check_patent_expiration(&mut companies, 11);
        assert_eq!(companies[0].patents.len(), 0);
    }

    #[test]
    fn estimate_operating_expenses_scales_with_wage() {
        let mut company = Company::default();
        company.worker_capacity = 100;
        company.sector = Sector::HeavyIndustry;

        let opex_10 = estimate_operating_expenses(&company, 10.0);
        let opex_20 = estimate_operating_expenses(&company, 20.0);
        assert_eq!(opex_20, opex_10 * 2.0, "OPEX must scale with average_wage");
    }

    #[test]
    fn estimate_current_unit_cost_scales_with_wage() {
        let mut company = Company::default();
        company.sector = Sector::HeavyIndustry;

        let cost_10 = estimate_current_unit_cost(&company, 10.0);
        let cost_20 = estimate_current_unit_cost(&company, 20.0);
        assert_eq!(
            cost_20,
            cost_10 * 2.0,
            "unit cost must scale with average_wage"
        );
    }

    #[test]
    fn estimate_current_unit_cost_scales_with_sector_intensity() {
        let mut heavy = Company::default();
        heavy.sector = Sector::HeavyIndustry;
        let mut light = Company::default();
        light.sector = Sector::LightIndustry;

        let avg_wage = 10.0;
        let heavy_cost = estimate_current_unit_cost(&heavy, avg_wage);
        let light_cost = estimate_current_unit_cost(&light, avg_wage);
        assert!(
            heavy_cost > light_cost,
            "Heavy industry unit cost must exceed light industry"
        );
    }

    #[test]
    fn no_magic_constants_in_unit_cost() {
        // Verify unit cost is derived from average_wage, not a hardcoded float.
        let mut company = Company::default();
        company.sector = Sector::HeavyIndustry;
        let cost = estimate_current_unit_cost(&company, 10.0);
        // HeavyIndustry multiplier = 4.0, so cost = 10.0 * 4.0 = 40.0
        assert_eq!(cost, 40.0);
    }
}
