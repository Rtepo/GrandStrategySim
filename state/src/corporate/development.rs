//! Property developer AI for automated construction decisions
//!
//! This module implements AI logic for private developers to automatically initiate
//! construction projects when they detect profitable market demand.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::strategy::CorporateDecisionCtx;
use crate::construction::projects::{ConstructionProject, ConstructionProjectType};
use crate::registries::enums::Commodity;
use crate::society::housing::{HousingType, HousingInventory};

/// Property developer AI agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PropertyDeveloper {
    /// Company ID
    #[serde(rename = "id_firmy", default)]
    pub company_id: String,
    
    /// Risk tolerance 0-1
    #[serde(rename = "tolerancja_ryzyka", default)]
    pub risk_tolerance: f64,
    
    /// Capital reserve for construction
    #[serde(rename = "kapitał_budowlany", default)]
    pub construction_capital: f64,
    
    /// Preferred project types
    #[serde(rename = "typy_projektów_preferowanych", default)]
    pub preferred_types: Vec<ConstructionProjectType>,
    
    /// Minimum ROI threshold for project initiation
    #[serde(rename = "próg_roi", default)]
    pub min_roi_threshold: f64,
    
    /// Down payment percentage required
    #[serde(rename = "procent_zaliczki", default)]
    pub down_payment_percentage: f64,
}

/// Market opportunity analysis result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketOpportunity {
    /// Housing shortage by type
    #[serde(rename = "niedobór_mieszkal", default)]
    pub housing_shortage: BTreeMap<String, f64>,
    
    /// Commercial vacancy rate
    #[serde(rename = "stopa_wakatów_komercyjnych", default)]
    pub commercial_vacancy_rate: f64,
    
    /// Expected ROI for new construction
    #[serde(rename = "oczekiwany_roi", default)]
    pub expected_roi: f64,
    
    /// Recommended project type
    #[serde(rename = "zalecany_typ_projektu")]
    pub recommended_project_type: Option<ConstructionProjectType>,
    
    /// Recommended housing type (if residential)
    #[serde(rename = "zalecany_typ_mieszkania")]
    pub recommended_housing_type: Option<HousingType>,
}

impl PropertyDeveloper {
    /// Evaluate market opportunity in a micro-region
    ///
    /// # Arguments
    /// * `housing_inventory` - Housing inventory for the micro-region
    /// * `market_prices` - Current market prices for construction materials
    /// * `population` - Population in the micro-region
    ///
    /// # Returns
    /// * Market opportunity analysis or None if no viable opportunity
    pub fn evaluate_market_opportunity(
        &self,
        housing_inventory: &HousingInventory,
        market_prices: &BTreeMap<String, f64>,
        population: i64,
    ) -> Option<MarketOpportunity> {
        // Calculate housing shortage
        let housing_shortage = self.calculate_housing_shortage(housing_inventory, population);
        
        // Calculate commercial vacancy
        let commercial_vacancy_rate = self.calculate_commercial_vacancy(housing_inventory);
        
        // Calculate expected ROI
        let expected_roi = self.calculate_expected_roi(&housing_shortage, commercial_vacancy_rate, market_prices);
        
        // Determine if opportunity meets threshold
        if expected_roi < self.min_roi_threshold {
            return None;
        }
        
        // Recommend project type based on shortage
        let (recommended_project_type, recommended_housing_type) = 
            self.recommend_project_type(&housing_shortage, commercial_vacancy_rate);
        
        Some(MarketOpportunity {
            housing_shortage,
            commercial_vacancy_rate,
            expected_roi,
            recommended_project_type,
            recommended_housing_type,
        })
    }
    
    /// Create a construction project based on market opportunity
    ///
    /// # Arguments
    /// * `opportunity` - Market opportunity analysis
    /// * `micro_region_id` - Target micro-region
    ///
    /// # Returns
    /// * Construction project or None if not viable
    pub fn create_project(
        &self,
        opportunity: &MarketOpportunity,
        micro_region_id: String,
    ) -> Option<ConstructionProject> {
        let project_type = opportunity.recommended_project_type?;
        let target_building_type = opportunity.recommended_housing_type
            .map(|t| format!("{:?}", t));
        
        let total_cost = self.estimate_project_cost(project_type, &target_building_type);
        
        // Check if developer has sufficient capital for down payment
        let down_payment = total_cost * self.down_payment_percentage;
        if self.construction_capital < down_payment {
            return None;
        }
        
        let duration = self.estimate_project_duration(project_type);
        
        Some(ConstructionProject {
            id: format!("proj_{}_{}", self.company_id, micro_region_id),
            project_type,
            micro_region_id,
            target_building_type: target_building_type.clone().unwrap_or_default(),
            required_materials: self.estimate_material_requirements(project_type, &target_building_type),
            delivered_materials: BTreeMap::new(),
            target_capacity_increase: 0,
            target_capital_increase: total_cost,
            is_new_building: true,
            total_cost,
            cost_spent: 0.0,
            duration_turns: duration,
            turns_elapsed: 0,
            progress: 0.0,
            on_hold: false,
            consecutive_hold_turns: 0,
            hold_reason: None,
            investor_id: String::new(),
            main_contractor_id: String::new(),
            subcontractors: Vec::new(),
            tranches: Vec::new(),
            paid_tranches: 0,
            contract_price: 0.0,
            contractor_margin: 0.0,
            structural_defect: 0.0,
            ohs_health_required: 0.0,
            ohs_education_required: 0.0,
            ohs_health_delivered: 0.0,
            ohs_education_delivered: 0.0,
            ohs_coverage_ratio: 1.0,
            ohs_accidents: 0,
            network_link_target: None,
            network_target_level: None,
        })
    }
    
    /// Calculate housing shortage by type
    fn calculate_housing_shortage(
        &self,
        housing_inventory: &HousingInventory,
        population: i64,
    ) -> BTreeMap<String, f64> {
        let mut shortage = BTreeMap::new();
        
        // Count total housing capacity
        let total_capacity: u32 = housing_inventory.buildings
            .iter()
            .map(|b| b.total_capacity())
            .sum();
        
        // Calculate shortage as percentage of population
        let shortage_ratio = if (total_capacity as i64) < population {
            (population - total_capacity as i64) as f64 / population as f64
        } else {
            0.0
        };
        
        // Distribute shortage across housing types based on current distribution
        for building in housing_inventory.buildings.iter() {
            let type_key = format!("{:?}", building.housing_type);
            let entry = shortage.entry(type_key).or_insert(0.0);
            *entry += shortage_ratio * building.total_capacity() as f64;
        }
        
        shortage
    }
    
    /// Calculate commercial vacancy rate
    fn calculate_commercial_vacancy(&self, _housing_inventory: &HousingInventory) -> f64 {
        // Placeholder: calculate based on commercial building occupancy
        // For now, return a default value
        0.2 // 20% vacancy rate
    }
    
    /// Calculate expected ROI for new construction
    fn calculate_expected_roi(
        &self,
        housing_shortage: &BTreeMap<String, f64>,
        commercial_vacancy: f64,
        market_prices: &BTreeMap<String, f64>,
    ) -> f64 {
        // Calculate total shortage
        let total_shortage: f64 = housing_shortage.values().sum();
        
        // Base ROI from housing shortage
        let housing_roi = total_shortage * 0.5;
        
        // ROI from commercial opportunity (inverse of vacancy)
        let commercial_roi = (1.0 - commercial_vacancy) * 0.3;
        
        // Adjust for material costs
        let material_cost_factor = market_prices.values().sum::<f64>().min(1.0);
        let cost_adjustment = 1.0 - material_cost_factor * 0.2;
        
        // Apply risk tolerance
        let risk_adjusted = (housing_roi + commercial_roi) * cost_adjustment * (1.0 + self.risk_tolerance);
        
        risk_adjusted
    }
    
    /// Recommend project type based on market analysis
    fn recommend_project_type(
        &self,
        housing_shortage: &BTreeMap<String, f64>,
        commercial_vacancy: f64,
    ) -> (Option<ConstructionProjectType>, Option<HousingType>) {
        let total_housing_shortage: f64 = housing_shortage.values().sum();
        
        // Prioritize housing if shortage is significant
        if total_housing_shortage > 0.1 {
            // Find most needed housing type
            let most_needed = housing_shortage
                .iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(k, _)| k.clone());
            
            let housing_type = most_needed.and_then(|s| {
                match s.as_str() {
                    "Hut" => Some(HousingType::Hut),
                    "Slum" => Some(HousingType::Slum),
                    "Familok" => Some(HousingType::Familok),
                    "Beamciok" => Some(HousingType::Beamciok),
                    "Tenement" => Some(HousingType::Tenement),
                    "CityPalace" => Some(HousingType::CityPalace),
                    "Palace" => Some(HousingType::Palace),
                    "Rectory" => Some(HousingType::Rectory),
                    "Monastery" => Some(HousingType::Monastery),
                    "SocialHousing" => Some(HousingType::SocialHousing),
                    "FolwarkHousing" => Some(HousingType::FolwarkHousing),
                    _ => None,
                }
            });
            
            (Some(ConstructionProjectType::Residential), housing_type)
        } else if commercial_vacancy < 0.3 {
            // Low commercial vacancy suggests opportunity
            (Some(ConstructionProjectType::Commercial), None)
        } else {
            // No clear opportunity
            (None, None)
        }
    }
    
    /// Estimate project cost
    fn estimate_project_cost(
        &self,
        project_type: ConstructionProjectType,
        target_building_type: &Option<String>,
    ) -> f64 {
        let base_cost = match project_type {
            ConstructionProjectType::Residential => 100_000.0,
            ConstructionProjectType::Commercial => 250_000.0,
            ConstructionProjectType::UtilityNetwork => 500_000.0,
            ConstructionProjectType::Infrastructure => 750_000.0,
            ConstructionProjectType::SocialHousing => 80_000.0,
            ConstructionProjectType::Factory => 300_000.0,
            ConstructionProjectType::TransportNetwork => 1_000_000.0,
            // Phase 39: Statecraft buildings
            ConstructionProjectType::Court => 200_000.0,
            ConstructionProjectType::CustomsOffice => 150_000.0,
            ConstructionProjectType::Embassy => 300_000.0,
            ConstructionProjectType::ResearchInstitute => 500_000.0,
            ConstructionProjectType::LaborInspectorate => 180_000.0,
            ConstructionProjectType::PublicWorksSite => 120_000.0,
            ConstructionProjectType::NationalTheater => 400_000.0,
            ConstructionProjectType::NationalLibrary => 350_000.0,
            ConstructionProjectType::TransportDepot => 200_000.0,
        };
        
        // Adjust for building type
        let type_multiplier = target_building_type.as_ref().map(|t| {
            match t.as_str() {
                "Palace" | "CityPalace" => 3.0,
                "Monastery" | "Rectory" => 2.0,
                "Tenement" | "Beamciok" => 1.5,
                "Familok" => 1.2,
                "SocialHousing" => 0.8,
                _ => 1.0,
            }
        }).unwrap_or(1.0);
        
        base_cost * type_multiplier
    }
    
    /// Estimate project duration in turns
    fn estimate_project_duration(&self, project_type: ConstructionProjectType) -> u32 {
        match project_type {
            ConstructionProjectType::Residential => 5,
            ConstructionProjectType::Commercial => 8,
            ConstructionProjectType::UtilityNetwork => 12,
            ConstructionProjectType::Infrastructure => 15,
            ConstructionProjectType::SocialHousing => 6,
            ConstructionProjectType::Factory => 10,
            ConstructionProjectType::TransportNetwork => 20,
            // Phase 39: Statecraft buildings
            ConstructionProjectType::Court => 8,
            ConstructionProjectType::CustomsOffice => 6,
            ConstructionProjectType::Embassy => 10,
            ConstructionProjectType::ResearchInstitute => 12,
            ConstructionProjectType::LaborInspectorate => 7,
            ConstructionProjectType::PublicWorksSite => 5,
            ConstructionProjectType::NationalTheater => 10,
            ConstructionProjectType::NationalLibrary => 9,
            ConstructionProjectType::TransportDepot => 8,
        }
    }
    
    /// Estimate material requirements
    fn estimate_material_requirements(
        &self,
        project_type: ConstructionProjectType,
        target_building_type: &Option<String>,
    ) -> BTreeMap<Commodity, f64> {
        let mut materials = BTreeMap::new();
        
        match project_type {
            ConstructionProjectType::Residential => {
                materials.insert(Commodity::Timber, 50.0);
                materials.insert(Commodity::Bricks, 100.0);
                materials.insert(Commodity::Cement, 20.0);
            }
            ConstructionProjectType::Commercial => {
                materials.insert(Commodity::Timber, 30.0);
                materials.insert(Commodity::Bricks, 200.0);
                materials.insert(Commodity::Cement, 50.0);
                materials.insert(Commodity::Glass, 40.0);
            }
            ConstructionProjectType::UtilityNetwork => {
                materials.insert(Commodity::Steel, 100.0);
                materials.insert(Commodity::Cement, 80.0);
            }
            ConstructionProjectType::Infrastructure => {
                materials.insert(Commodity::Stone, 200.0);
                materials.insert(Commodity::Cement, 100.0);
                materials.insert(Commodity::Asphalt, 50.0);
            }
            ConstructionProjectType::SocialHousing => {
                materials.insert(Commodity::Timber, 40.0);
                materials.insert(Commodity::Bricks, 80.0);
                materials.insert(Commodity::Cement, 15.0);
            }
            ConstructionProjectType::Factory => {
                materials.insert(Commodity::Steel, 300.0);
                materials.insert(Commodity::Cement, 400.0);
                materials.insert(Commodity::Bricks, 200.0);
                materials.insert(Commodity::ConstructionMachinery, 30.0);
            }
            ConstructionProjectType::TransportNetwork => {
                // Network BOM is computed separately via get_network_construction_bom.
                // This is a fallback estimate for AI planning.
                materials.insert(Commodity::Steel, 500.0);
                materials.insert(Commodity::Cement, 800.0);
                materials.insert(Commodity::Timber, 200.0);
                materials.insert(Commodity::ConstructionMachinery, 50.0);
            }
            // Phase 39: Statecraft buildings — BOMs
            ConstructionProjectType::Court => {
                materials.insert(Commodity::Bricks, 150.0);
                materials.insert(Commodity::Timber, 50.0);
                materials.insert(Commodity::Cement, 30.0);
            }
            ConstructionProjectType::CustomsOffice => {
                materials.insert(Commodity::Bricks, 100.0);
                materials.insert(Commodity::Timber, 30.0);
                materials.insert(Commodity::Cement, 20.0);
            }
            ConstructionProjectType::Embassy => {
                materials.insert(Commodity::Bricks, 200.0);
                materials.insert(Commodity::Timber, 80.0);
                materials.insert(Commodity::Glass, 30.0);
                materials.insert(Commodity::Cement, 40.0);
            }
            ConstructionProjectType::ResearchInstitute => {
                materials.insert(Commodity::Steel, 100.0);
                materials.insert(Commodity::Cement, 200.0);
                materials.insert(Commodity::Glass, 50.0);
                materials.insert(Commodity::ElectronicComponents, 30.0);
            }
            ConstructionProjectType::LaborInspectorate => {
                materials.insert(Commodity::Bricks, 120.0);
                materials.insert(Commodity::Timber, 40.0);
                materials.insert(Commodity::Cement, 25.0);
            }
            ConstructionProjectType::PublicWorksSite => {
                materials.insert(Commodity::Timber, 60.0);
                materials.insert(Commodity::Cement, 30.0);
                materials.insert(Commodity::ConstructionMachinery, 10.0);
            }
            ConstructionProjectType::NationalTheater => {
                materials.insert(Commodity::Bricks, 200.0);
                materials.insert(Commodity::Timber, 100.0);
                materials.insert(Commodity::Glass, 40.0);
                materials.insert(Commodity::Cement, 50.0);
            }
            ConstructionProjectType::NationalLibrary => {
                materials.insert(Commodity::Bricks, 180.0);
                materials.insert(Commodity::Timber, 120.0);
                materials.insert(Commodity::Cement, 40.0);
                materials.insert(Commodity::Glass, 30.0);
            }
            ConstructionProjectType::TransportDepot => {
                materials.insert(Commodity::Steel, 80.0);
                materials.insert(Commodity::Cement, 100.0);
                materials.insert(Commodity::Bricks, 80.0);
            }
        }
        
        // Adjust for building type
        let type_multiplier = target_building_type.as_ref().map(|t| {
            match t.as_str() {
                "Palace" | "CityPalace" => 2.0,
                "Monastery" | "Rectory" => 1.5,
                "Tenement" | "Beamciok" => 1.3,
                _ => 1.0,
            }
        }).unwrap_or(1.0);
        
        materials
            .into_iter()
            .map(|(k, v)| (k, v * type_multiplier))
            .collect()
    }
    
    /// Estimate labor requirement
    fn estimate_labor_requirement(&self, project_type: ConstructionProjectType) -> u32 {
        match project_type {
            ConstructionProjectType::Residential => 10,
            ConstructionProjectType::Commercial => 20,
            ConstructionProjectType::UtilityNetwork => 30,
            ConstructionProjectType::Infrastructure => 40,
            ConstructionProjectType::SocialHousing => 12,
            ConstructionProjectType::Factory => 25,
            ConstructionProjectType::TransportNetwork => 50,
            // Phase 39: Statecraft buildings
            ConstructionProjectType::Court => 15,
            ConstructionProjectType::CustomsOffice => 10,
            ConstructionProjectType::Embassy => 20,
            ConstructionProjectType::ResearchInstitute => 25,
            ConstructionProjectType::LaborInspectorate => 12,
            ConstructionProjectType::PublicWorksSite => 15,
            ConstructionProjectType::NationalTheater => 20,
            ConstructionProjectType::NationalLibrary => 18,
            ConstructionProjectType::TransportDepot => 15,
        }
    }
}

/// Phase 24C.6: Evaluate market opportunities and publish construction tenders
/// for all eligible property developers in a country.
///
/// This function integrates the `PropertyDeveloper` AI with the Phase 22
/// tender market. For each company that has a `PropertyDeveloper` capability,
/// it evaluates market opportunities and publishes `ConstructionTender`s
/// when ROI thresholds are met.
///
/// # Arguments
/// * `companies` - Mutable companies (to debit construction capital)
/// * `housing_inventory` - Housing inventory for shortage calculation
/// * `tenders` - Mutable list of active tenders to append new ones
/// * `market_prices` - Current market prices for ROI calculation
/// * `population` - Population for shortage calculation
/// * `micro_region_id` - Target micro-region
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Only companies with `construction_capital > 0` act as developers
/// * Tenders are published with `TenderInvestorType::Corporation`
/// * The developer's `down_payment_percentage` determines the encumbered amount
/// * Information quality (Phase 24C.7) modulates cost estimation accuracy
pub fn publish_developer_tenders(
    companies: &mut [crate::entities::Company],
    housing_inventory: &HousingInventory,
    tenders: &mut Vec<crate::construction::tenders::ConstructionTender>,
    market_prices: &BTreeMap<String, f64>,
    population: i64,
    micro_region_id: &str,
    current_turn: u32,
    start_year: u32,
) {
    use crate::construction::tender_market::publish_tender;
    use crate::construction::tenders::TenderInvestorType;
    use crate::corporate::bounded_rationality::{InformationQuality, apply_estimation_error};

    for company in companies.iter_mut() {
        // Only construction companies can act as property developers
        if company.sector != crate::registries::enums::Sector::Construction {
            continue;
        }
        if company.available_cash < 10_000.0 {
            continue;
        }

        // Create a transient PropertyDeveloper from the company's state
        let developer = PropertyDeveloper {
            company_id: company.id.clone(),
            risk_tolerance: 0.5,
            construction_capital: company.available_cash * 0.3, // Reserve 30% for construction
            preferred_types: vec![
                ConstructionProjectType::Residential,
                ConstructionProjectType::Commercial,
            ],
            min_roi_threshold: 0.15,
            down_payment_percentage: 0.2,
        };

        // Evaluate market opportunity
        let opportunity = developer.evaluate_market_opportunity(
            housing_inventory,
            market_prices,
            population,
        );

        if let Some(opp) = opportunity {
            let project_type = match opp.recommended_project_type {
                Some(pt) => pt,
                None => continue,
            };
            let target_building_type_opt = opp.recommended_housing_type
                .map(|t| format!("{:?}", t));

            let true_cost = developer.estimate_project_cost(project_type, &target_building_type_opt);

            // Phase 24C.7: Apply information quality estimation error
            let quality = company.information_quality.unwrap_or(InformationQuality::Blind);
            let estimated_cost = apply_estimation_error(true_cost, quality);

            let down_payment = estimated_cost * developer.down_payment_percentage;
            if company.available_cash < down_payment {
                continue;
            }

            // Debit the down payment from the company
            company.available_cash -= down_payment;
            company.debit_cash += down_payment;

            let target_building_type = target_building_type_opt
                .unwrap_or_else(|| format!("{:?}", project_type));

            // Publish the tender
            let tender = publish_tender(
                company.id.clone(),
                TenderInvestorType::Corporation,
                project_type,
                micro_region_id.to_string(),
                target_building_type,
                100, // target capacity increase
                estimated_cost,
                estimated_cost, // target capital increase
                5,   // 5-turn bidding window
                current_turn,
                company.sector,
                start_year,
            );
            tenders.push(tender);
        }
    }
}

/// Phase 40: Publish State-funded construction tenders from ministries with
/// available cash. Each ministry with a relevant competency publishes one
/// tender per turn (up to a cap) using its `ministry_cash` as the budget.
///
/// This fixes the root cause of the tender deadlock: the State never published
/// tenders, so the only tenders in the market were private developer tenders.
///
/// # Arguments
/// * `country` - Country with ministry_config and budget.
/// * `tenders` - Mutable list of active tenders to append new ones.
/// * `micro_region_id` - Target region for state construction.
/// * `current_turn` - Current turn number.
///
/// # Rules
/// * Only ministries with `ministry_cash > 50_000` publish tenders.
/// * The tender `estimated_cost` is capped at 30% of `ministry_cash` to avoid
///   draining a ministry's entire budget on one project.
/// * State tenders use a 2-turn bidding window (fast procurement).
/// * The ministry's cash is NOT debited here — the Treasury pays tranches
///   as the project progresses (via `process_tender_awards` → mobilization
///   advance for State-backed projects).
pub fn publish_state_tenders(
    country: &crate::state::Country,
    tenders: &mut Vec<crate::construction::tenders::ConstructionTender>,
    micro_region_id: &str,
    current_turn: u32,
    start_year: u32,
) {
    use crate::construction::tender_market::publish_tender;
    use crate::construction::tenders::{TenderInvestorType, TenderStatus};
    use crate::politics::ministries::GovernmentCompetency;

    let Some(ref config) = country.politics.ministry_config else {
        return;
    };

    // Cap concurrent state tenders to avoid flooding the market.
    let active_state_tenders = tenders
        .iter()
        .filter(|t| t.investor_type == TenderInvestorType::State && t.status == TenderStatus::Open)
        .count();
    if active_state_tenders >= 5 {
        return;
    }

    for ministry in &config.ministries {
        if ministry.ministry_cash < 50_000.0 {
            continue;
        }

        // Map ministry competencies to construction project types.
        // Each ministry publishes at most one tender per turn.
        let project_type = ministry.competencies.iter().find_map(|c| match c {
            GovernmentCompetency::Infrastructure => Some(ConstructionProjectType::Infrastructure),
            GovernmentCompetency::Transport => Some(ConstructionProjectType::TransportNetwork),
            GovernmentCompetency::Justice => Some(ConstructionProjectType::Court),
            GovernmentCompetency::Treasury => Some(ConstructionProjectType::CustomsOffice),
            GovernmentCompetency::ForeignAffairs => Some(ConstructionProjectType::Embassy),
            GovernmentCompetency::Science => Some(ConstructionProjectType::ResearchInstitute),
            GovernmentCompetency::Labor => Some(ConstructionProjectType::LaborInspectorate),
            GovernmentCompetency::SocialWelfare => Some(ConstructionProjectType::SocialHousing),
            GovernmentCompetency::Culture => Some(ConstructionProjectType::NationalTheater),
            GovernmentCompetency::Education => Some(ConstructionProjectType::NationalLibrary),
            _ => None,
        });

        let Some(project_type) = project_type else { continue };

        // Estimated cost = 30% of ministry cash, capped at 500K.
        let estimated_cost = (ministry.ministry_cash * 0.3).min(500_000.0);
        if estimated_cost < 50_000.0 {
            continue;
        }

        let target_building_type = format!("{:?}", project_type);
        let investor_id = format!("STATE:{}", micro_region_id);

        let tender = publish_tender(
            investor_id,
            TenderInvestorType::State,
            project_type,
            micro_region_id.to_string(),
            target_building_type,
            50, // target capacity increase
            estimated_cost,
            estimated_cost,
            2, // 2-turn bidding window (fast procurement)
            current_turn,
            crate::registries::enums::Sector::PublicServices,
            start_year,
        );
        tenders.push(tender);
    }
}

/// Phase 30: Evaluate gas station construction opportunities and publish
/// construction tenders.
///
/// Gas stations are built based on:
/// - Freight traffic through the region (congestion on network links)
/// - Highway/rail endpoint status (regions that are logistics hubs)
/// - Vehicle/car ownership in the region (consumer fuel demand)
/// - Existing gas station count (avoid oversaturation)
///
/// # Arguments
/// * `companies` - Mutable companies (to debit construction capital)
/// * `tenders` - Mutable list of active tenders to append new ones
/// * `region_id` - Target region for gas station construction
/// * `freight_traffic_score` - 0.0–1.0 score of freight activity in the region
/// * `vehicle_ownership_rate` - 0.0–1.0 fraction of population with cars
/// * `existing_gas_stations` - Count of existing gas stations in the region
/// * `is_logistics_hub` - Whether the region is a highway/rail endpoint
/// * `current_turn` - Current turn number
pub fn publish_gas_station_tenders(
    companies: &mut [crate::entities::Company],
    tenders: &mut Vec<crate::construction::tenders::ConstructionTender>,
    region_id: &str,
    freight_traffic_score: f64,
    vehicle_ownership_rate: f64,
    existing_gas_stations: usize,
    is_logistics_hub: bool,
    current_turn: u32,
    start_year: u32,
) {
    use crate::construction::tender_market::publish_tender;
    use crate::construction::tenders::TenderInvestorType;

    // Demand score: weighted combination of freight traffic and vehicle ownership.
    let demand_score = freight_traffic_score * 0.4 + vehicle_ownership_rate * 0.6;

    // Logistics hub bonus: +0.2 if the region is a highway/rail endpoint.
    let hub_bonus = if is_logistics_hub { 0.2 } else { 0.0 };
    let total_score = (demand_score + hub_bonus).min(1.0);

    // Threshold: only build if demand score is high enough and no existing
    // gas stations in the region (or very few relative to population).
    if total_score < 0.3 {
        return;
    }

    // Saturation check: don't build if there are already enough gas stations.
    // Rule of thumb: 1 gas station per 5000 car-owning residents.
    let max_stations = (vehicle_ownership_rate * 0.2).max(1.0) as usize;
    if existing_gas_stations >= max_stations {
        return;
    }

    for company in companies.iter_mut() {
        if company.sector != crate::registries::enums::Sector::Construction {
            continue;
        }
        if company.available_cash < 50_000.0 {
            continue;
        }

        let estimated_cost = 150_000.0; // Gas station construction cost
        let down_payment = estimated_cost * 0.2;

        if company.available_cash < down_payment {
            continue;
        }

        // Debit the down payment from the company.
        company.available_cash -= down_payment;
        company.debit_cash += down_payment;

        let tender = publish_tender(
            company.id.clone(),
            TenderInvestorType::Corporation,
            ConstructionProjectType::Commercial,
            region_id.to_string(),
            "GasStation".to_string(),
            100,    // target_capacity_increase
            5000.0, // target_capital_increase
            estimated_cost,
            8,      // deadline_turns
            current_turn,
            crate::registries::enums::Sector::TransportLogistics,
            start_year,
        );
        tenders.push(tender);

        // Only one gas station tender per call.
        break;
    }
}
