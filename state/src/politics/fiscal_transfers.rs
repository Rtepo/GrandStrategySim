//! Fiscal transfer processing and regional tax collection with closed-loop macroeconomics
//!
//! Phase 94: Property tax is now cadastre-based. The old `ClassLandDistribution`
//! tax path has been completely deleted. The `Cadastre` is the single, absolute
//! source of truth for all land valuation and taxation (Rule 14).

use crate::entities::Company;
use crate::politics::local_council::{calculate_curial_faction_alignment, calculate_seat_count};
use crate::politics::local_government::AdministrativeStatus;
use crate::politics::system::FiscalTransferConfig;
use crate::society::cadastre::{self, ParcelOwnerType, PropertyTaxConfig};
use crate::society::geography::{EconomicStatus, RuralClass};
use crate::state::Country;
use std::collections::BTreeMap;

/// Collect property taxes based on cadastre parcel valuations with strict
/// double-entry bookkeeping.
///
/// # CRITICAL: Closed-Loop Taxation (Rule 1)
/// The `Cadastre` is the single source of truth for land ownership and valuation.
/// State-owned and Municipal-owned parcels are explicitly tax-exempt (self-taxation
/// is prohibited). For every currency unit credited to a regional budget, exactly
/// one currency unit is debited from a payer's cash or savings. Shortfalls reduce
/// the credit — no fiat is ever created.
///
/// # Arguments
/// * `country` - Mutable reference to the country (contains the cadastre).
/// * `companies` - Mutable slice of companies (for `available_cash` debits).
///
/// # Returns
/// Total property tax **actually collected** across all regions.
pub fn process_regional_taxes(country: &mut Country, companies: &mut [Company]) -> f64 {
    let property_tax_config = PropertyTaxConfig::default();

    // Compute nominal tax per owner from the cadastre (revalues parcels first).
    // State and Municipal parcels are explicitly skipped inside this function.
    let tax_by_owner = cadastre::calculate_cadastre_property_tax(
        &mut country.cadastre,
        &country.cadastre_config,
        &property_tax_config,
    );

    // Build a map: owner_id -> region_id for regional budget crediting.
    let owner_region_map: BTreeMap<String, String> = country
        .cadastre
        .parcels
        .values()
        .filter(|p| {
            p.owner_type != ParcelOwnerType::State && p.owner_type != ParcelOwnerType::Municipal
        })
        .map(|p| (p.owner_id.clone(), p.region_id.clone()))
        .collect();

    // Debit from payers and track actual collected per region.
    let mut actual_collected_per_region: BTreeMap<String, f64> = BTreeMap::new();

    for (owner_id, nominal_tax) in &tax_by_owner {
        let actual_paid = debit_tax_from_payer(country, companies, owner_id, *nominal_tax);
        if actual_paid > 0.0 {
            if let Some(region_id) = owner_region_map.get(owner_id) {
                *actual_collected_per_region
                    .entry(region_id.clone())
                    .or_insert(0.0) += actual_paid;
            }
        }
    }

    // Credit regional budgets with the ACTUAL collected amounts (double-entry).
    let mut total_property_tax: f64 = 0.0;
    for region in &mut country.regions {
        let collected = actual_collected_per_region
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        if let Some(governance) = region.governance.as_mut() {
            governance.budget.tax_revenue = collected;
            governance.budget.property_tax = collected;
            governance.budget.local_fees = 0.0;
            // Credit liquid_reserves with EXACTLY what was debited from payers.
            governance.budget.liquid_reserves += collected;
        }
        total_property_tax += collected;
    }
    total_property_tax
}

/// Debit property tax from a payer's cash or savings.
///
/// # Strict Double-Entry (Rule 1)
/// Debits only what the payer can afford (clamped to 0, no negative cash).
/// Returns the ACTUAL amount debited, which may be less than the nominal tax.
/// The uncollected shortfall is simply not credited to any budget — no fiat.
///
/// # Owner Resolution
/// * `Corporate` / `ForeignFund` / `Religious` / `Cooperative` owners: debited
///   from `Company.available_cash` by `owner_id`.
/// * `Private` owners with `DYNASTY_` prefix: debited from Aristocracy class savings.
/// * `Private` owners with `PEASANT_` prefix: debited from FreePeasant class savings.
fn debit_tax_from_payer(
    country: &mut Country,
    companies: &mut [Company],
    owner_id: &str,
    nominal_tax: f64,
) -> f64 {
    if nominal_tax <= 0.0 {
        return 0.0;
    }

    // Find the owner's parcel to determine owner_type and region.
    let (owner_type, region_id) = match country
        .cadastre
        .parcels
        .values()
        .find(|p| p.owner_id == owner_id)
        .map(|p| (p.owner_type, p.region_id.clone()))
    {
        Some(x) => x,
        None => return 0.0, // Owner not found — cannot debit. No fiat creation.
    };

    match owner_type {
        ParcelOwnerType::Corporate
        | ParcelOwnerType::ForeignFund
        | ParcelOwnerType::Religious
        | ParcelOwnerType::Cooperative => {
            if let Some(company) = companies.iter_mut().find(|c| c.id == owner_id) {
                let debit = nominal_tax.min(company.available_cash.max(0.0));
                company.available_cash -= debit;
                debit
            } else {
                0.0 // Company not found — no fiat creation.
            }
        }
        ParcelOwnerType::Private => {
            let rural_class = if owner_id.starts_with("DYNASTY_") {
                RuralClass::Aristocracy
            } else if owner_id.starts_with("PEASANT_") {
                RuralClass::FreePeasant
            } else {
                RuralClass::Aristocracy
            };

            let region_idx = country.regions.iter().position(|r| r.id == region_id);
            let Some(idx) = region_idx else {
                return 0.0;
            };

            if let Some(demographics) =
                country.regions[idx].class_demographics.get_class_mut(rural_class)
            {
                let debit = nominal_tax.min(demographics.savings.max(0.0));
                demographics.savings -= debit;
                if demographics.population > 0 {
                    demographics.savings_per_capita =
                        demographics.savings / demographics.population as f64;
                }
                if debit > 0.0 && demographics.savings <= 0.0 {
                    demographics.economic_status = match demographics.economic_status {
                        EconomicStatus::Prosperous => EconomicStatus::Stable,
                        EconomicStatus::Stable => EconomicStatus::Struggling,
                        EconomicStatus::Struggling => EconomicStatus::Destitute,
                        EconomicStatus::Destitute => EconomicStatus::Destitute,
                    };
                }
                debit
            } else {
                0.0
            }
        }
        // State and Municipal are tax-exempt — should never reach here.
        ParcelOwnerType::State | ParcelOwnerType::Municipal => 0.0,
    }
}

/// Process upward fiscal transfers with no double-dipping.
///
/// # CRITICAL: No Double Dipping
/// Region splits revenue exactly once according to FiscalTransferConfig.
/// Megaregion keeps 100% of its transfer - no second upward transfer to Central.
///
/// # CRITICAL: Strict Double-Entry (Rule 1)
/// When a region's liquid_reserves are insufficient to cover the full upward
/// transfer, the debit is clamped to available reserves. The megaregion and
/// central budgets are credited ONLY the actual debited fraction, scaled
/// proportionally. The equation `actual_debit == actual_megaregion + actual_central`
/// always holds exactly. No fiat is created when reserves are insufficient.
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
        let has_megaregion = country
            .megaregions
            .iter()
            .any(|m| m.regions.contains(&region.id));

        let (local_retained, megaregion_transfer, central_transfer) =
            transfer_config.calculate_transfers(governance.budget.tax_revenue, has_megaregion);

        governance.budget.megaregion_transfer = megaregion_transfer;
        governance.budget.central_transfer = central_transfer;
        governance.budget.budget_balance = local_retained - governance.budget.local_expenditures;

        // Phase 94: Strict double-entry — debit clamped to available reserves.
        // Credits to megaregion/central are scaled proportionally so that
        // actual_debit == actual_megaregion + actual_central always holds.
        let total_upward = megaregion_transfer + central_transfer;
        let actual_debit = total_upward.min(governance.budget.liquid_reserves.max(0.0));
        governance.budget.liquid_reserves -= actual_debit;

        if total_upward > 0.0 {
            let collection_ratio = actual_debit / total_upward;
            let actual_megaregion = megaregion_transfer * collection_ratio;
            let actual_central = central_transfer * collection_ratio;

            // Transfer to Megaregion (if applicable)
            if has_megaregion && actual_megaregion > 0.0 {
                if let Some(megaregion) = country
                    .megaregions
                    .iter_mut()
                    .find(|m| m.regions.contains(&region.id))
                {
                    if let Some(meg_gov) = megaregion.governance.as_mut() {
                        meg_gov.budget.regional_transfers += actual_megaregion;
                        meg_gov.budget.liquid_reserves += actual_megaregion;
                    }
                }
            }

            // Transfer to Central Budget
            country.budget.liquid_reserves += actual_central;
        }

        // CRITICAL: Megaregions do NOT transfer to Central
        // They keep 100% of their regional transfers for development/coordination spending
    }
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

        if debt_ratio > 5.0
            && governance.admin_status != AdministrativeStatus::CommissaryAdministration
        {
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
                    let peasant_seats =
                        governance.council.total_seats - aristocracy_seats - burgher_seats;
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
                    let optimates_seats =
                        governance.council.total_seats - populares_seats - moderates_seats;
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
        let is_curial = region
            .governance
            .as_ref()
            .map(|g| {
                g.council.election_system
                    == crate::politics::local_council::LocalElectionSystem::Curial
            })
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
            let classes = [
                RuralClass::Aristocracy,
                RuralClass::FreePeasant,
                RuralClass::Serf,
                RuralClass::LandlessLaborer,
            ];
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
    use crate::society::cadastre::{Cadastre, CadastreConfig, ParcelChunk, PropertyTaxConfig};
    use crate::society::geography::Region;
    use crate::state::Country;

    /// Phase 94: Test that cadastre-based property tax credits liquid_reserves
    /// with exactly the amount debited from the payer (double-entry).
    #[test]
    fn test_cadastre_property_tax_double_entry() {
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        country.cadastre_config = CadastreConfig::default();

        // Create a region with governance initialized.
        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.owner_country = "TestLand".to_string();
        region.governance = Some(
            crate::politics::local_government::initialize_regional_governance("REG-001", "TestLand"),
        );

        // Add aristocracy class with savings.
        use crate::society::geography::{ClassDemographics, RegionalClassDemographics, RuralClass};
        let mut demos = RegionalClassDemographics::default();
        let mut aristo = ClassDemographics::default();
        aristo.population = 100;
        aristo.savings = 100_000.0;
        let aristo_key = serde_json::to_string(&RuralClass::Aristocracy).unwrap_or_default();
        demos.rural_classes.insert(aristo_key, aristo);
        region.class_demographics = demos;

        country.regions = vec![region];

        // Add a Private (Aristocracy) parcel to the cadastre.
        let mut parcel = ParcelChunk::default();
        parcel.owner_type = crate::society::cadastre::ParcelOwnerType::Private;
        parcel.owner_id = "DYNASTY_REG-001_0".to_string();
        parcel.region_id = "REG-001".to_string();
        parcel.size_hectares = 100.0;
        parcel.soil_class = "Class_III".to_string();
        country.cadastre.insert(parcel);

        let reserves_before = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;
        let savings_before = country.regions[0]
            .class_demographics
            .get_class(RuralClass::Aristocracy)
            .map(|d| d.savings)
            .unwrap_or(0.0);

        let companies: Vec<Company> = Vec::new();
        let total_collected = process_regional_taxes(&mut country, &mut companies.to_vec());

        let reserves_after = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;
        let savings_after = country.regions[0]
            .class_demographics
            .get_class(RuralClass::Aristocracy)
            .map(|d| d.savings)
            .unwrap_or(0.0);

        // Double-entry: reserves gained exactly what savings lost.
        let reserves_gain = reserves_after - reserves_before;
        let savings_loss = savings_before - savings_after;
        assert!(
            reserves_gain > 0.0,
            "Tax collection should credit liquid_reserves"
        );
        assert!(
            (reserves_gain - savings_loss).abs() < 0.01,
            "Double-entry: reserves gain ({}) must equal savings loss ({})",
            reserves_gain,
            savings_loss
        );
        assert!(
            (total_collected - reserves_gain).abs() < 0.01,
            "Total collected must equal reserves gain"
        );
    }

    /// Phase 94: Test that State-owned parcels are tax-exempt.
    #[test]
    fn test_state_parcels_tax_exempt() {
        let mut cadastre = Cadastre::default();
        let config = CadastreConfig::default();
        let tax_config = PropertyTaxConfig::default();

        // Add a State-owned parcel.
        let mut state_parcel = ParcelChunk::default();
        state_parcel.owner_type = crate::society::cadastre::ParcelOwnerType::State;
        state_parcel.owner_id = "TREASURY".to_string();
        state_parcel.region_id = "REG-001".to_string();
        state_parcel.size_hectares = 1000.0;
        state_parcel.soil_class = "Class_I".to_string();
        cadastre.insert(state_parcel);

        // Add a Private parcel for comparison.
        let mut private_parcel = ParcelChunk::default();
        private_parcel.owner_type = crate::society::cadastre::ParcelOwnerType::Private;
        private_parcel.owner_id = "DYNASTY_REG-001_0".to_string();
        private_parcel.region_id = "REG-001".to_string();
        private_parcel.size_hectares = 100.0;
        private_parcel.soil_class = "Class_III".to_string();
        cadastre.insert(private_parcel);

        let tax_by_owner =
            crate::society::cadastre::calculate_cadastre_property_tax(&mut cadastre, &config, &tax_config);

        // State-owned parcel should NOT appear in the tax map.
        assert!(
            !tax_by_owner.contains_key("TREASURY"),
            "State-owned parcels must be tax-exempt"
        );
        // Private parcel SHOULD appear.
        assert!(
            tax_by_owner.contains_key("DYNASTY_REG-001_0"),
            "Private parcels must be taxed"
        );
    }

    /// Phase 94: Test that insufficient savings result in no fiat creation.
    #[test]
    fn test_insufficient_savings_no_fiat() {
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        country.cadastre_config = CadastreConfig::default();

        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.owner_country = "TestLand".to_string();
        region.governance = Some(
            crate::politics::local_government::initialize_regional_governance("REG-001", "TestLand"),
        );

        // Aristocracy with very low savings (cannot afford full tax).
        use crate::society::geography::{ClassDemographics, RegionalClassDemographics, RuralClass};
        let mut demos = RegionalClassDemographics::default();
        let mut aristo = ClassDemographics::default();
        aristo.population = 100;
        aristo.savings = 10.0; // Very low — tax will be much higher
        let aristo_key = serde_json::to_string(&RuralClass::Aristocracy).unwrap_or_default();
        demos.rural_classes.insert(aristo_key, aristo);
        region.class_demographics = demos;

        country.regions = vec![region];

        // Add a high-value Private (Aristocracy) parcel.
        let mut parcel = ParcelChunk::default();
        parcel.owner_type = crate::society::cadastre::ParcelOwnerType::Private;
        parcel.owner_id = "DYNASTY_REG-001_0".to_string();
        parcel.region_id = "REG-001".to_string();
        parcel.size_hectares = 1000.0;
        parcel.soil_class = "Class_I".to_string(); // Highest value
        country.cadastre.insert(parcel);

        let reserves_before = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;

        let mut companies: Vec<Company> = Vec::new();
        let total_collected = process_regional_taxes(&mut country, &mut companies);

        let reserves_after = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;

        let reserves_gain = reserves_after - reserves_before;
        // The collected amount must equal the reserves gain (no fiat).
        assert!(
            (total_collected - reserves_gain).abs() < 0.01,
            "No fiat: total_collected ({}) must equal reserves_gain ({})",
            total_collected,
            reserves_gain
        );
        // The collected amount must NOT exceed the available savings (10.0).
        assert!(
            total_collected <= 10.01,
            "Collected ({}) must not exceed available savings (10.0) — no fiat",
            total_collected
        );
        // Savings must not go negative.
        let savings_after = country.regions[0]
            .class_demographics
            .get_class(RuralClass::Aristocracy)
            .map(|d| d.savings)
            .unwrap_or(0.0);
        assert!(
            savings_after >= 0.0,
            "Savings must not go negative — no fiat debt creation"
        );
    }

    /// Phase 33: Test that fiscal transfers debit regional liquid_reserves (double-entry).
    #[test]
    fn test_fiscal_transfers_debit_regional_reserves() {
        let mut country = Country::default();
        country.name = "TestLand".to_string();
        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.owner_country = "TestLand".to_string();
        let mut gov = crate::politics::local_government::initialize_regional_governance(
            "REG-001", "TestLand",
        );
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
        let regional_before = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;

        process_fiscal_transfers(&mut country, &transfer_config);

        let central_after = country.budget.liquid_reserves;
        let regional_after = country.regions[0]
            .governance
            .as_ref()
            .unwrap()
            .budget
            .liquid_reserves;

        // Central budget should have gained from the transfer.
        assert!(
            central_after > central_before,
            "Central budget should gain from transfer"
        );
        // Regional budget should have lost the transfer amount.
        assert!(
            regional_after < regional_before,
            "Regional budget should be debited for transfer"
        );
        // Phase 94: Double-entry — central gain must equal regional loss.
        let central_gain = central_after - central_before;
        let regional_loss = regional_before - regional_after;
        assert!(
            (central_gain - regional_loss).abs() < 0.01,
            "Double-entry: central gain ({}) must equal regional loss ({})",
            central_gain,
            regional_loss
        );
    }
}
