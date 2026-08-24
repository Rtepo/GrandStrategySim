//! Phase 7: Corporate R&D allocation and method research.
//!
//! This module implements corporate R&D allocation from excess cash and
//! research of Commercial Production Methods.

use crate::economy::corporate_config::CorporateTechConfig;
use crate::entities::{Company, LicensedMethod, Patent};
use crate::registries::enums::Sector;
use crate::registries::tech_tree::{TechId, TechNode, TechType};
use std::collections::HashMap;

/// Allocates corporate R&D budget from excess cash.
///
/// # Arguments
/// * `companies` - Slice of companies to allocate R&D
/// * `config` - Corporate technology configuration
///
/// # Returns
/// Updated companies with rd_budget allocations
///
/// # Rules
/// * Strategic AI Decision: Only allocate if cash > threshold * operating_expenses
/// * Allocate percentage of excess cash to rd_budget
/// * No magic numbers: uses CorporateTechConfig
pub fn allocate_corporate_rd_budget(companies: &mut [Company], config: &CorporateTechConfig) {
    for company in companies.iter_mut() {
        // Calculate operating expenses (simplified as last turn's costs)
        let operating_expenses = estimate_operating_expenses(company);
        let cash_threshold = operating_expenses * config.rd_allocation_threshold_ratio;
        
        // Only allocate if cash exceeds threshold
        if company.available_cash > cash_threshold {
            let excess_cash = company.available_cash - cash_threshold;
            let rd_allocation = excess_cash * config.rd_allocation_percentage;
            
            // Deduct from available_cash, add to rd_budget
            company.available_cash -= rd_allocation;
            company.rd_budget += rd_allocation;
        }
    }
}

/// Estimates operating expenses for a company.
///
/// # Arguments
/// * `company` - Company to estimate expenses for
///
/// # Returns
/// Estimated operating expenses
///
/// # Rules
/// * Simplified estimate based on worker capacity and sector
fn estimate_operating_expenses(company: &Company) -> f64 {
    let cost_per_worker = match company.sector {
        Sector::Mining => 200.0,
        Sector::Agriculture => 150.0,
        Sector::HeavyIndustry => 250.0,
        Sector::LightIndustry => 180.0,
        Sector::ArmamentsIndustry => 300.0,
        Sector::Construction => 220.0,
        Sector::Energy => 280.0,
        Sector::TransportLogistics => 200.0,
        Sector::MediaAndEntertainment => 170.0,
        Sector::MedicalServices => 260.0,
        Sector::EducationalServices => 220.0,
        Sector::PublicServices => 160.0,
        _ => 150.0,
    };
    
    company.worker_capacity as f64 * cost_per_worker
}

/// Executes corporate method research using rd_budget.
///
/// # Arguments
/// * `companies` - Slice of companies to research
/// * `tech_tree` - Technology tree registry
/// * `current_turn` - Current simulation turn
///
/// # Returns
/// Updated companies with new patents and discovered methods
///
/// # Rules
/// * Companies research Commercial techs tied to their sector
/// * Prerequisites: Fundamental techs must be discovered by State
/// * Sector-Strict: Can only research methods for active sector
/// * Successful research grants patent
pub fn execute_corporate_method_research(
    companies: &mut [Company],
    tech_tree: &HashMap<TechId, TechNode>,
    current_turn: u32,
) {
    for company in companies.iter_mut() {
        // Skip if no rd_budget
        if company.rd_budget <= 0.0 {
            continue;
        }
        
        // Find eligible Commercial techs for this company's sector
        for (tech_id, tech_node) in tech_tree.iter() {
            // Only Commercial techs
            if tech_node.tech_type != TechType::Commercial {
                continue;
            }
            
            // Skip if already has patent
            if company.patents.iter().any(|p| &p.tech_id == tech_id) {
                continue;
            }
            
            // Check if tech is relevant to company's sector
            if !is_tech_relevant_to_sector(tech_node, company.sector) {
                continue;
            }
            
            // Check prerequisites (Fundamental techs must be discovered)
            // Note: This requires access to State's discovered techs
            // For now, assume prerequisites are met if tech is available
            
            // Check if company can afford research
            let research_cost = tech_node.cost as f64 * 1000.0; // Scale cost for corporate
            if company.rd_budget >= research_cost {
                // Deduct from rd_budget
                company.rd_budget -= research_cost;
                
                // Grant patent
                let patent = Patent {
                    tech_id: tech_id.clone(),
                    granted_turn: current_turn,
                    expires_turn: current_turn + tech_node.patent_duration_turns,
                    royalty_vwap_ratio: tech_node.royalty_vwap_ratio,
                };
                company.patents.push(patent);
            }
        }
    }
}

/// Checks if a technology is relevant to a company's sector.
///
/// # Arguments
/// * `tech_node` - Technology node to check
/// * `sector` - Company's sector
///
/// # Returns
/// true if tech is relevant to sector
///
/// # Rules
/// * Tech is relevant if it unlocks methods for the company's sector
fn is_tech_relevant_to_sector(tech_node: &TechNode, sector: Sector) -> bool {
    // Check if tech unlocks methods for this sector
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
///
/// # Returns
/// Updated companies with new licensed methods
///
/// # Rules
/// * Strategic AI Decision: Cost-benefit analysis
/// * License if (current_cost - new_cost - royalty) > threshold
/// * Voluntary licensing (no forced payments)
pub fn evaluate_licensing_opportunities(
    companies: &mut [Company],
    all_companies: &[Company],
    tech_tree: &HashMap<TechId, TechNode>,
    config: &CorporateTechConfig,
) {
    for company in companies.iter_mut() {
        // Find available patented methods relevant to this company
        for other_company in all_companies.iter() {
            if other_company.id == company.id {
                continue; // Skip self
            }
            
            for patent in &other_company.patents {
                // Check if already licensed
                if company.licensed_methods.iter().any(|lm| lm.tech_id == patent.tech_id) {
                    continue;
                }
                
                // Get tech node
                if let Some(tech_node) = tech_tree.get(&patent.tech_id) {
                    // Check if relevant to sector
                    if !is_tech_relevant_to_sector(tech_node, company.sector) {
                        continue;
                    }
                    
                    // Calculate cost-benefit
                    let current_unit_cost = estimate_current_unit_cost(company);
                    let new_unit_cost = estimate_new_unit_cost(tech_node);
                    let royalty_cost = patent.royalty_vwap_ratio * 100.0; // Simplified VWAP estimate
                    let net_benefit = current_unit_cost - new_unit_cost - royalty_cost;
                    
                    // License if net benefit exceeds threshold
                    if net_benefit > config.licensing_benefit_threshold {
                        let license = LicensedMethod {
                            tech_id: patent.tech_id.clone(),
                            licensor_company_id: other_company.id.clone(),
                            licensed_turn: 0, // Would need current_turn parameter
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
/// # Arguments
/// * `company` - Company to estimate for
///
/// # Returns
/// Estimated unit cost
fn estimate_current_unit_cost(company: &Company) -> f64 {
    // Simplified estimate based on sector
    match company.sector {
        Sector::Mining => 50.0,
        Sector::Agriculture => 30.0,
        Sector::HeavyIndustry => 80.0,
        Sector::LightIndustry => 60.0,
        Sector::ArmamentsIndustry => 120.0,
        Sector::Construction => 70.0,
        Sector::Energy => 90.0,
        Sector::TransportLogistics => 55.0,
        Sector::MediaAndEntertainment => 45.0,
        Sector::MedicalServices => 85.0,
        Sector::EducationalServices => 65.0,
        Sector::PublicServices => 40.0,
        _ => 50.0,
    }
}

/// Estimates new unit cost with a technology.
///
/// # Arguments
/// * `tech_node` - Technology node
///
/// # Returns
/// Estimated new unit cost
fn estimate_new_unit_cost(_tech_node: &TechNode) -> f64 {
    // Simplified: assume 20% cost reduction from new tech
    40.0 // Placeholder
}

/// Checks patent expiration and moves expired patents to public domain.
///
/// # Arguments
/// * `companies` - Slice of companies with patents
/// * `current_turn` - Current simulation turn
///
/// # Returns
/// Updated companies with expired patents removed
///
/// # Rules
/// * Patents expire after patent_duration_turns
/// * Expired patents enter public domain (anyone can use)
pub fn check_patent_expiration(companies: &mut [Company], current_turn: u32) {
    for company in companies.iter_mut() {
        company.patents.retain(|patent| patent.expires_turn > current_turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::tech_tree::TechType;

    #[test]
    fn rd_allocation_from_excess_cash() {
        let mut company = Company::default();
        company.available_cash = 5000.0;
        company.worker_capacity = 100;
        company.sector = Sector::HeavyIndustry;
        
        let config = CorporateTechConfig::default();
        
        let mut companies = vec![company];
        allocate_corporate_rd_budget(&mut companies, &config);

        // Operating expenses: 100 * 250 = 25000
        // Threshold: 25000 * 2.0 = 50000
        // Cash (5000) < Threshold (50000), no allocation
        assert_eq!(companies[0].rd_budget, 0.0);
    }

    #[test]
    fn rd_allocation_when_wealthy() {
        let mut company = Company::default();
        company.available_cash = 100000.0;
        company.worker_capacity = 100;
        company.sector = Sector::HeavyIndustry;
        
        let config = CorporateTechConfig::default();
        
        let mut companies = vec![company];
        allocate_corporate_rd_budget(&mut companies, &config);

        // Operating expenses: 100 * 250 = 25000
        // Threshold: 25000 * 2.0 = 50000
        // Excess: 100000 - 50000 = 50000
        // Allocation: 50000 * 0.10 = 5000
        assert_eq!(companies[0].rd_budget, 5000.0);
        assert_eq!(companies[0].available_cash, 95000.0);
    }

    #[test]
    fn patent_granted_on_successful_research() {
        let mut tech_tree = HashMap::new();
        tech_tree.insert(
            "tech_001".to_string(),
            TechNode {
                name: "Steel Production".to_string(),
                year: 1850,
                cost: 50,
                description: "Steel manufacturing".to_string(),
                unlocks_methods: HashMap::from([("heavy_industry".to_string(), HashMap::new())]),
                unlocks_projects: Vec::new(),
                prerequisites: Vec::new(),
                tech_type: TechType::Commercial,
                patent_duration_turns: 240,
                royalty_vwap_ratio: 0.05,
            },
        );
        
        let mut company = Company::default();
        company.rd_budget = 60000.0; // Sufficient for 50 * 1000 = 50000
        company.sector = Sector::HeavyIndustry;
        
        let mut companies = vec![company];
        execute_corporate_method_research(&mut companies, &tech_tree, 1);

        assert_eq!(companies[0].patents.len(), 1);
        assert_eq!(companies[0].patents[0].tech_id, "tech_001");
        assert_eq!(companies[0].rd_budget, 10000.0); // 60000 - 50000
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
        assert_eq!(companies[0].patents.len(), 1); // Not expired yet

        check_patent_expiration(&mut companies, 11);
        assert_eq!(companies[0].patents.len(), 0); // Expired
    }
}
