//! Fiscal transfer processing and regional tax collection with closed-loop macroeconomics

use crate::politics::local_government::AdministrativeStatus;
use crate::politics::local_council::{calculate_curial_faction_alignment, calculate_seat_count};
use crate::politics::system::FiscalTransferConfig;
use crate::society::geography::{Region, RuralClass, EconomicStatus};
use crate::state::Country;
use std::collections::BTreeMap;

/// Collect local taxes in all regions with closed-loop macroeconomic integrity
///
/// # CRITICAL: Closed-Loop Taxation
/// Taxes are NOT spawned from nowhere. They are explicitly deducted from the
/// savings of the classes that own the land in ClassLandDistribution.
/// Serfs (RuralClass::Serf) do not pay taxes as they have no cash economy.
///
/// # Arguments
/// * `country` - Mutable reference to the country
///
/// # Returns
/// Total property tax collected across all regions (for national tax reporting).
///
/// # Rules
/// * Property tax is calculated based on land ownership by class
/// * Taxes are deducted from class savings in RegionalClassDemographics
/// * If a class cannot afford the tax, savings go negative (debt) and EconomicStatus drops
/// * Serfs are exempt from taxation
pub fn process_regional_taxes(country: &mut Country) -> f64 {
    let mut total_property_tax: f64 = 0.0;
    for region in &mut country.regions {
        // Calculate property tax based on land ownership
        let (property_tax, tax_by_class) = calculate_property_tax(region);

        // Calculate local service fees (simplified - could be expanded)
        let local_fees = calculate_local_fees(region);

        // Deduct taxes from class savings (closed-loop)
        deduct_taxes_from_classes(region, &tax_by_class);

        // Update regional budget
        if let Some(governance) = region.governance.as_mut() {
            let total_revenue = property_tax + local_fees;
            governance.budget.tax_revenue = total_revenue;
            governance.budget.property_tax = property_tax;
            governance.budget.local_fees = local_fees;
            // Phase 33: Credit liquid_reserves so the region can spend.
            // This is double-entry consistent: taxes were debited from class savings above.
            governance.budget.liquid_reserves += total_revenue;
        }
        total_property_tax += property_tax;
    }
    total_property_tax
}

/// Calculate property tax and tax burden by class
/// 
/// # Arguments
/// * `region` - Reference to the region
/// 
/// # Returns
/// (total_property_tax, tax_by_class: BTreeMap<RuralClass, f64>)
fn calculate_property_tax(region: &Region) -> (f64, BTreeMap<RuralClass, f64>) {
    let mut tax_by_class = BTreeMap::new();
    let mut total_tax = 0.0;
    
    let tax_rate = 0.02; // 2% property tax rate (configurable)
    
    for land_dist in region.land_distribution.values() {
        let tax_per_hectare = tax_rate * 100.0; // Simplified: 2 currency units per hectare
        
        // Aristocracy pays tax on their land
        let aristocracy_tax = land_dist.aristocracy_hectares as f64 * tax_per_hectare;
        tax_by_class.insert(RuralClass::Aristocracy, aristocracy_tax);
        total_tax += aristocracy_tax;
        
        // Free Peasants pay tax on their land
        let peasant_tax = land_dist.free_peasant_hectares as f64 * tax_per_hectare;
        tax_by_class.insert(RuralClass::FreePeasant, peasant_tax);
        total_tax += peasant_tax;
        
        // Note: Corporations and Municipalities are not RuralClass
        // Their taxes would be handled separately in corporate tax logic
    }
    
    (total_tax, tax_by_class)
}

/// Calculate local service fees (simplified placeholder)
fn calculate_local_fees(_region: &Region) -> f64 {
    // Placeholder: could be based on population, services provided, etc.
    0.0
}

/// Deduct taxes from class savings with tax oppression mechanics
/// 
/// # CRITICAL: Tax Oppression
/// If a class cannot afford the tax, their savings go negative (debt) or
/// their EconomicStatus drops, simulating tax oppression.
/// 
/// # Arguments
/// * `region` - Mutable reference to the region
/// * `tax_by_class` - Tax burden by class
fn deduct_taxes_from_classes(region: &mut Region, tax_by_class: &BTreeMap<RuralClass, f64>) {
    let class_demographics = &mut region.class_demographics;
    
    for (class, tax_amount) in tax_by_class {
        // Serfs do not pay taxes (no cash economy)
        if *class == RuralClass::Serf {
            continue;
        }
        
        if let Some(demographics) = class_demographics.get_class_mut(*class) {
            if demographics.savings >= *tax_amount {
                demographics.savings -= *tax_amount;
            } else {
                // Cannot afford tax - go into debt or drop economic status
                let shortfall = *tax_amount - demographics.savings;
                demographics.savings = -shortfall; // Debt
                
                // Drop economic status due to tax oppression
                demographics.economic_status = match demographics.economic_status {
                    EconomicStatus::Prosperous => EconomicStatus::Stable,
                    EconomicStatus::Stable => EconomicStatus::Struggling,
                    EconomicStatus::Struggling => EconomicStatus::Destitute,
                    EconomicStatus::Destitute => EconomicStatus::Destitute,
                };
            }
            
            // Recalculate per-capita savings
            if demographics.population > 0 {
                demographics.savings_per_capita = demographics.savings / demographics.population as f64;
            }
        }
    }
}

/// Process upward fiscal transfers with no double-dipping
/// 
/// # CRITICAL: No Double Dipping
/// Region splits revenue exactly once according to FiscalTransferConfig.
/// Megaregion keeps 100% of its transfer - no second upward transfer to Central.
/// 
/// # Arguments
/// * `country` - Mutable reference to the country
/// * `transfer_config` - Fiscal transfer configuration
pub fn process_fiscal_transfers(country: &mut Country, transfer_config: &FiscalTransferConfig) {
    for region in &mut country.regions {
        let Some(governance) = region.governance.as_mut() else {
            continue;
        };
        
        // Check if region belongs to a Megaregion
        let has_megaregion = country.megaregions.iter()
            .any(|m| m.regions.contains(&region.id));
        
        let (local_retained, megaregion_transfer, central_transfer) = 
            transfer_config.calculate_transfers(
                governance.budget.tax_revenue,
                has_megaregion,
            );
        
        governance.budget.megaregion_transfer = megaregion_transfer;
        governance.budget.central_transfer = central_transfer;
        governance.budget.budget_balance = local_retained - governance.budget.local_expenditures;
        
        // Phase 33: Debit liquid_reserves for upward transfers (double-entry).
        // The regional budget was credited in process_regional_taxes.
        // Now we debit the portion that flows upward.
        let total_upward = megaregion_transfer + central_transfer;
        let debit = total_upward.min(governance.budget.liquid_reserves);
        governance.budget.liquid_reserves -= debit;
        
        // Transfer to Megaregion (if applicable)
        if has_megaregion && megaregion_transfer > 0.0 {
            if let Some(megaregion) = country.megaregions.iter_mut()
                .find(|m| m.regions.contains(&region.id)) {
                if let Some(meg_gov) = megaregion.governance.as_mut() {
                    meg_gov.budget.regional_transfers += megaregion_transfer;
                    // Phase 33: Also credit megaregion liquid_reserves.
                    meg_gov.budget.liquid_reserves += megaregion_transfer;
                }
            }
        }
        
        // Transfer to Central Budget
        country.budget.liquid_reserves += central_transfer;
    }
    
    // CRITICAL: Megaregions do NOT transfer to Central
    // They keep 100% of their regional transfers for development/coordination spending
}

/// Check for Commissary Administration trigger
/// 
/// # Arguments
/// * `country` - Mutable reference to the country
/// 
/// # Rules
/// * If debt-to-revenue ratio exceeds 5.0, trigger Commissary Administration
/// * Commissary Administration freezes local spending and central government takes over
pub fn check_commissary_administration(country: &mut Country) {
    for region in &mut country.regions {
        let Some(governance) = region.governance.as_mut() else {
            continue;
        };
        
        // Check debt-to-revenue ratio
        let debt_ratio = governance.debt.debt_to_revenue_ratio;
        
        if debt_ratio > 5.0 && governance.admin_status != AdministrativeStatus::CommissaryAdministration {
            // Trigger Commissary Administration
            governance.admin_status = AdministrativeStatus::CommissaryAdministration;
            
            // Freeze local spending
            governance.budget.local_expenditures = 0.0;
            
            // Central government bail-out (would be implemented in full fiscal logic)
            // For now, just mark the status change
        }
    }
}

/// Process municipal debt service
/// 
/// # Arguments
/// * `country` - Mutable reference to the country
pub fn process_municipal_debt_service(country: &mut Country) {
    for region in &mut country.regions {
        let Some(governance) = region.governance.as_mut() else {
            continue;
        };
        
        // Calculate debt service payments
        let mut total_debt_service = 0.0;
        
        for bond in &governance.debt.municipal_bonds {
            let interest_payment = bond.principal * bond.interest_rate;
            total_debt_service += interest_payment;
        }
        
        // Deduct from budget
        governance.budget.debt_service = total_debt_service;
        governance.budget.liquid_reserves -= total_debt_service;
        
        // Update debt-to-revenue ratio
        let annual_revenue = governance.budget.tax_revenue;
        if annual_revenue > 0.0 {
            governance.debt.debt_to_revenue_ratio = governance.debt.total_debt / annual_revenue;
        }
    }
}

/// Process local elections for all regions
/// 
/// # Arguments
/// * `country` - Mutable reference to the country
/// * `year` - Current simulation year
pub fn process_local_elections(country: &mut Country, year: u32) {
    for region in &mut country.regions {
        let Some(governance) = region.governance.as_mut() else {
            continue;
        };
        
        if governance.council.years_to_next_election == 0 {
            // Calculate dynamic seat count based on population
            let new_seat_count = calculate_seat_count(region.population);
            governance.council.total_seats = new_seat_count;
            
            // Simplified election logic - would be expanded with full election simulation
            match governance.council.election_system {
                crate::politics::local_council::LocalElectionSystem::Curial => {
                    // Curial: seats allocated by class, hereditary/appointed
                    let aristocracy_seats = (governance.council.total_seats as f64 * 0.5) as u32;
                    let burgher_seats = (governance.council.total_seats as f64 * 0.3) as u32;
                    let peasant_seats = governance.council.total_seats - aristocracy_seats - burgher_seats;
                    governance.council.faction_distribution.optimates_count = aristocracy_seats;
                    governance.council.faction_distribution.moderates_count = burgher_seats;
                    governance.council.faction_distribution.populares_count = peasant_seats;
                }
                crate::politics::local_council::LocalElectionSystem::Census => {
                    // Census: weighted voting based on wealth
                    let wealthy_seats = (governance.council.total_seats as f64 * 0.6) as u32;
                    let middle_seats = (governance.council.total_seats as f64 * 0.3) as u32;
                    let poor_seats = governance.council.total_seats - wealthy_seats - middle_seats;
                    governance.council.faction_distribution.optimates_count = wealthy_seats;
                    governance.council.faction_distribution.moderates_count = middle_seats;
                    governance.council.faction_distribution.populares_count = poor_seats;
                }
                crate::politics::local_council::LocalElectionSystem::Democratic => {
                    // Democratic: universal suffrage
                    let populares_seats = (governance.council.total_seats as f64 * 0.4) as u32;
                    let moderates_seats = (governance.council.total_seats as f64 * 0.35) as u32;
                    let optimates_seats = governance.council.total_seats - populares_seats - moderates_seats;
                    governance.council.faction_distribution.populares_count = populares_seats;
                    governance.council.faction_distribution.moderates_count = moderates_seats;
                    governance.council.faction_distribution.optimates_count = optimates_seats;
                }
            }
            
            governance.last_election_year = year;
            
            // Set next election cycle based on configuration
            let term_length = match &governance.council.election_config {
                crate::politics::local_council::ElectionConfig::Democratic(cfg) => cfg.term_length,
                _ => 4, // Default 4-year cycle for Curial/Census
            };
            governance.council.years_to_next_election = term_length;
        } else {
            governance.council.years_to_next_election -= 1;
        }
    }
}

/// Update Curial faction alignments (yearly)
/// 
/// # Arguments
/// * `country` - Mutable reference to the country
pub fn update_curial_faction_alignments(country: &mut Country) {
    for region in &mut country.regions {
        // Only apply to Curial systems
        let is_curial = region.governance.as_ref()
            .map(|g| g.council.election_system == crate::politics::local_council::LocalElectionSystem::Curial)
            .unwrap_or(false);
        
        if !is_curial {
            continue;
        }
        
        // Calculate revolt risk from class demographics
        let revolt_risk = {
            let serf_demographics = region.class_demographics.get_class(RuralClass::Serf);
            if let Some(serf_data) = serf_demographics {
                let misery_factor = match serf_data.economic_status {
                    EconomicStatus::Destitute => 1.0,
                    EconomicStatus::Struggling => 0.7,
                    EconomicStatus::Stable => 0.3,
                    EconomicStatus::Prosperous => 0.0,
                };
                let serf_ratio = if region.population > 0 {
                    serf_data.population as f64 / region.population as f64
                } else {
                    0.0
                };
                serf_ratio * misery_factor
            } else {
                0.0
            }
        };
        
        // Calculate economic stability
        let economic_stability = {
            let classes = [RuralClass::Aristocracy, RuralClass::FreePeasant, RuralClass::Serf, RuralClass::LandlessLaborer];
            let mut total_stability = 0.0;
            let mut class_count = 0;
            
            for class in classes {
                if let Some(demographics) = region.class_demographics.get_class(class) {
                    let stability = match demographics.economic_status {
                        EconomicStatus::Prosperous => 1.0,
                        EconomicStatus::Stable => 0.75,
                        EconomicStatus::Struggling => 0.5,
                        EconomicStatus::Destitute => 0.25,
                    };
                    total_stability += stability;
                    class_count += 1;
                }
            }
            
            if class_count > 0 {
                total_stability / class_count as f64
            } else {
                0.5
            }
        };
        
        // Update faction alignment
        if let Some(governance) = region.governance.as_mut() {
            calculate_curial_faction_alignment(
                &mut governance.council,
                revolt_risk,
                economic_stability,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::system::FiscalTransferConfig;
    use crate::state::Country;
    use crate::society::geography::Region;

    /// Phase 33: Test that process_regional_taxes credits liquid_reserves.
    #[test]
    fn test_regional_taxes_credit_liquid_reserves() {
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        // Create a region with governance initialized.
        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.owner_country = "TestLand".to_string();
        region.governance = Some(
            crate::politics::local_government::initialize_regional_governance("REG-001", "TestLand"),
        );
        // Add some land distribution so property tax is non-zero.
        use crate::society::geography::ClassLandDistribution;
        let mut land_dist = ClassLandDistribution::default();
        land_dist.aristocracy_hectares = 100;
        region.land_distribution.insert("1".to_string(), land_dist);
        // Add aristocracy class with savings.
        use crate::society::geography::{RegionalClassDemographics, ClassDemographics, RuralClass};
        let mut demos = RegionalClassDemographics::default();
        let mut aristo = ClassDemographics::default();
        aristo.population = 100;
        aristo.savings = 10000.0;
        // The key is the serde_json serialization of the RuralClass enum.
        let aristo_key = serde_json::to_string(&RuralClass::Aristocracy).unwrap_or_default();
        demos.rural_classes.insert(aristo_key, aristo);
        region.class_demographics = demos;

        country.regions = vec![region];

        let reserves_before = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        process_regional_taxes(&mut country);
        let reserves_after = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;

        assert!(reserves_after > reserves_before, "Tax collection should credit liquid_reserves");
        assert!(reserves_after > 0.0, "Liquid reserves should be positive after tax collection");
    }

    /// Phase 33: Test that fiscal transfers debit regional liquid_reserves (double-entry).
    #[test]
    fn test_fiscal_transfers_debit_regional_reserves() {
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.owner_country = "TestLand".to_string();
        let mut gov = crate::politics::local_government::initialize_regional_governance("REG-001", "TestLand");
        // Pre-fund the budget.
        gov.budget.liquid_reserves = 1000.0;
        gov.budget.tax_revenue = 1000.0;
        region.governance = Some(gov);
        country.regions = vec![region];

        let transfer_config = FiscalTransferConfig {
            local_retention: 0.6,
            megaregion_share: 0.0,
            central_share: 0.4,
            minimum_local_retention: 0.3,
        };
        let central_before = country.budget.liquid_reserves;
        let regional_before = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;

        process_fiscal_transfers(&mut country, &transfer_config);

        let central_after = country.budget.liquid_reserves;
        let regional_after = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;

        // Central budget should have gained from the transfer.
        assert!(central_after > central_before, "Central budget should gain from transfer");
        // Regional budget should have lost the transfer amount.
        assert!(regional_after < regional_before, "Regional budget should be debited for transfer");
    }
}
