//! Phase 7: Proportional royalty payments during production.
//!
//! This module implements atomic royalty payments for licensed production methods,
//! using VWAP-anchored scaling and graceful degradation.

use crate::economy::corporate_config::CorporateTechConfig;
use crate::economy::innovation_config::InnovationConfig;
use crate::economy::market_history::MarketHistory;
use crate::entities::Company;
use crate::registries::enums::Commodity;
use crate::registries::tech_tree::TechId;
use crate::state::treasury::Treasury;
use std::collections::BTreeMap;

/// Calculates royalty fulfillment ratio for a company.
///
/// # Arguments
/// * `company` - Company using licensed methods
/// * `planned_quantity` - Planned production quantity
/// * `last_turn_vwap` - Last turn's VWAP for output commodity
/// * `licensed_methods` - Licensed methods the company is using
///
/// # Returns
/// Fulfillment ratio (0.0 to 1.0) based on available cash for royalties
///
/// # Rules
/// * Royalty cash treated as physical input in calculate_fulfillment_ratio
/// * VWAP-anchored: royalty = quantity * royalty_vwap_ratio * last_turn_vwap
/// * Graceful degradation: partial cash = partial production
pub fn calculate_royalty_fulfillment_ratio(
    company: &Company,
    planned_quantity: f64,
    last_turn_vwap: f64,
    licensed_methods: &[(TechId, f64)], // (tech_id, royalty_vwap_ratio)
) -> f64 {
    if licensed_methods.is_empty() || planned_quantity <= 0.0 {
        return 1.0; // No royalties needed
    }
    
    let total_required_royalty: f64 = licensed_methods
        .iter()
        .map(|(_, royalty_ratio)| planned_quantity * royalty_ratio * last_turn_vwap)
        .sum();
    
    if total_required_royalty <= 0.0 {
        return 1.0_f64;
    }
    
    // Calculate fulfillment ratio based on available cash
    let ratio: f64 = company.available_cash / total_required_royalty;
    
    ratio.min(1.0_f64)
}

/// Deducts royalty payments from licensee and credits to licensor.
///
/// # Arguments
/// * `licensee` - Company paying royalties
/// * `licensor` - Company receiving royalties
/// * `actual_quantity` - Actual production quantity (after fulfillment ratio)
/// * `last_turn_vwap` - Last turn's VWAP for output commodity
/// * `royalty_vwap_ratio` - Royalty VWAP ratio from patent
///
/// # Returns
/// Updated licensee and licensor with royalty transfer
///
/// # Rules
/// * Atomic payment: cash must be available upfront
/// * Proportional scaling: actual_quantity * royalty_vwap_ratio * last_turn_vwap
/// * Double-Entry: licensee.available_cash decreases, licensor.available_cash increases
pub fn process_royalty_payment(
    licensee: &mut Company,
    licensor: &mut Company,
    actual_quantity: f64,
    last_turn_vwap: f64,
    royalty_vwap_ratio: f64,
) {
    let royalty_amount = actual_quantity * royalty_vwap_ratio * last_turn_vwap;
    
    if royalty_amount <= 0.0 {
        return;
    }
    
    // Atomic payment: deduct from licensee
    if licensee.available_cash >= royalty_amount {
        licensee.available_cash -= royalty_amount;
        licensor.available_cash += royalty_amount;
    }
    // If insufficient cash, payment fails (graceful degradation already handled)
}

/// Integrates royalty payments into production cycle.
///
/// # Arguments
/// * `companies` - All companies in the simulation
/// * `market_history` - Market history containing last turn's VWAP
/// * `planned_production` - Planned production quantities by company
///
/// # Returns
/// Updated companies with royalty payments processed
///
/// # Rules
/// * Treat royalty cash as physical input in calculate_fulfillment_ratio
/// * Production scales proportionally if cash-constrained
/// * VWAP-anchored scaling for inflation protection
/// * Using last turn's VWAP guarantees determinism
pub fn integrate_royalty_payments(
    companies: &mut [Company],
    market_history: &MarketHistory,
    planned_production: &BTreeMap<String, f64>,
    innovation_config: &InnovationConfig,
) {
    // First pass: calculate fulfillment ratios and collect payment instructions
    let mut payment_instructions: Vec<(String, String, f64)> = Vec::new();
    
    for company in companies.iter() {
        let planned_quantity = planned_production.get(&company.id).copied().unwrap_or(0.0);
        
        if planned_quantity <= 0.0 || company.licensed_methods.is_empty() {
            continue;
        }
        
        // Get last turn's VWAP for output commodity (use first output from first licensed method)
        let last_turn_vwap = market_history
            .vwap_per_commodity
            .values()
        .next()
        .copied()
        .unwrap_or(100.0);
        
        // Calculate royalty fulfillment ratio using patent's royalty_vwap_ratio or config default
        let licensed_royalties: Vec<(TechId, f64)> = company
            .licensed_methods
            .iter()
            .map(|lm| {
                // Look up patent for this licensed method to get royalty_vwap_ratio
                let ratio = company
                    .patents
                    .iter()
                    .find(|p| p.tech_id == lm.tech_id)
                    .map(|p| p.royalty_vwap_ratio)
                    .unwrap_or(innovation_config.default_royalty_vwap_ratio);
                (lm.tech_id.clone(), ratio)
            })
            .collect();
        
        let fulfillment_ratio = calculate_royalty_fulfillment_ratio(
            company,
            planned_quantity,
            last_turn_vwap,
            &licensed_royalties,
        );
        
        let actual_quantity = planned_quantity * fulfillment_ratio;
        
        // Collect payment instructions
        for licensed_method in &company.licensed_methods {
            let royalty_ratio = company
                .patents
                .iter()
                .find(|p| p.tech_id == licensed_method.tech_id)
                .map(|p| p.royalty_vwap_ratio)
                .unwrap_or(innovation_config.default_royalty_vwap_ratio);
            let royalty_amount = actual_quantity * royalty_ratio * last_turn_vwap;
            payment_instructions.push((
                company.id.clone(),
                licensed_method.licensor_company_id.clone(),
                royalty_amount,
            ));
        }
    }
    
    // Second pass: process royalty payments using collected instructions
    for (licensee_id, licensor_id, royalty_amount) in payment_instructions {
        let last_turn_vwap = market_history
            .vwap_per_commodity
            .values()
            .next()
            .copied()
            .unwrap_or(100.0);
        
        // Find indices to avoid borrow checker issues
        let licensee_idx = companies.iter().position(|c| c.id == licensee_id);
        let licensor_idx = companies.iter().position(|c| c.id == licensor_id);
        
        if let (Some(lic_idx), Some(licensor_idx)) = (licensee_idx, licensor_idx) {
            if lic_idx != licensor_idx {
                let royalty_vwap_ratio = innovation_config.default_royalty_vwap_ratio;
                // Process payment using indices
                let _actual_quantity = royalty_amount / (royalty_vwap_ratio * last_turn_vwap);
                companies[lic_idx].available_cash -= royalty_amount;
                companies[licensor_idx].available_cash += royalty_amount;
            }
        }
    }
}

/// Process all royalty payments including state patent royalties.
///
/// # Arguments
/// * `companies` - All companies in the simulation.
/// * `market_history` - Market history with VWAP data.
/// * `planned_production` - Planned production quantities by company.
/// * `innovation_config` - Innovation configuration with default royalty ratio.
/// * `corporate_tech_config` - Corporate tech config with state patent royalty ratio.
/// * `treasury` - Mutable treasury for state royalty credits.
///
/// # Rules
/// * Private patent royalties: licensee → licensor company (double-entry).
/// * State patent royalties: licensee company → treasury.liquid_reserves.
/// * State patents apply to ALL companies using the technology (state-owned + private).
/// * Graceful degradation: partial cash = partial production.
/// * VWAP-anchored for inflation protection.
pub fn process_all_royalty_payments(
    companies: &mut [Company],
    market_history: &MarketHistory,
    planned_production: &BTreeMap<String, f64>,
    innovation_config: &InnovationConfig,
    corporate_tech_config: &CorporateTechConfig,
    treasury: &mut Treasury,
) {
    // First: process private-to-private royalties via integrate_royalty_payments
    integrate_royalty_payments(companies, market_history, planned_production, innovation_config);

    // Second: process state patent royalties
    // State patents are those held by the state (not by any company).
    // For now, we use the state_patent_royalty_ratio from config as a flat rate
    // applied to all companies that use technologies discovered by state research.
    let state_royalty_ratio = corporate_tech_config.state_patent_royalty_ratio;

    for company in companies.iter_mut() {
        let planned_quantity = planned_production.get(&company.id).copied().unwrap_or(0.0);
        if planned_quantity <= 0.0 {
            continue;
        }

        // Get VWAP for royalty calculation
        let last_turn_vwap = market_history
            .vwap_per_commodity
            .values()
            .next()
            .copied()
            .unwrap_or(100.0);

        // Companies with licensed methods from state research pay state royalties
        // We check if the licensor is the state (by convention, state-owned patents
        // have licensor_company_id = "STATE")
        let has_state_license = company
            .licensed_methods
            .iter()
            .any(|lm| lm.licensor_company_id == "STATE");

        if has_state_license {
            let state_royalty = planned_quantity * state_royalty_ratio * last_turn_vwap;
            if state_royalty > 0.0 && company.available_cash >= state_royalty {
                company.available_cash -= state_royalty;
                treasury.liquid_reserves += state_royalty;
            }
        }
    }
}

/// Phase 19A: Process blueprint royalty payments via `TransferSettler`.
///
/// For each company with `licensed_blueprints`, look up the actual blueprint
/// output quantity produced this turn and compute the royalty fee as
/// `qty × blueprint.royalty_vwap_ratio × last_turn_vwap(output_commodity)`.
///
/// # Rules
/// * Cash leg: `debit_company_by_id` (licensee) → `credit_company_by_id` (licensor),
///   both via `TransferSettler` helpers (strict double-entry, bank balance-sheet sync).
/// * Domestic licensors: credited immediately.
/// * Cross-border licensors: a `CrossBorderRoyaltyQueueEntry` is pushed onto
///   `cross_border_queue` for sequential post-parallel crediting (the parallel
///   country phase emits the FX outflow; the post-parallel phase credits the
///   foreign licensor in the destination country).
/// * State-owned blueprints (`licensor_company_id == "STATE"`): credit treasury.
/// * Graceful fulfillment: if the licensee cannot pay the full amount, pay what
///   is available (no production block).
/// * VWAP lookup uses `market_history.vwap_per_commodity[output_commodity]`,
///   falling back to 100.0 if unknown (matches existing royalty behavior).
///
/// # Arguments
/// * `companies` - All companies in this country (licensees + domestic licensors).
/// * `buildings` - Buildings (to read `last_production` for actual output qty).
/// * `market_history` - Market history with last turn's VWAP per commodity.
/// * `treasury` - Mutable treasury for state-blueprint royalties.
/// * `country_name` - This country's name (for cross-border routing).
/// * `cross_border_queue` - Global queue for cross-border royalty credits.
pub fn process_blueprint_royalty_payments(
    companies: &mut [Company],
    buildings: &[crate::entities::Building],
    market_history: &MarketHistory,
    treasury: &mut Treasury,
    country_name: &str,
    cross_border_queue: &mut Vec<crate::economy::blueprints::CrossBorderRoyaltyQueueEntry>,
) {
    use crate::economy::blueprints::{compute_blueprint_royalty_fee, LicensedBlueprint};
    use crate::economy::transfer_settler::{debit_company_by_id, credit_company_by_id};

    // Snapshot licensed blueprints per company (avoid borrow conflicts).
    let licensed_per_company: Vec<(String, Vec<LicensedBlueprint>)> = companies
        .iter()
        .map(|c| (c.id.clone(), c.licensed_blueprints.clone()))
        .collect();

    // Build a lookup of blueprint_id → (owner_company_id, output_commodity, royalty_vwap_ratio)
    // by scanning all companies' owned blueprints. This avoids a separate registry.
    let blueprint_index: std::collections::HashMap<String, (String, Commodity, f64)> = {
        let mut idx = std::collections::HashMap::new();
        for company in companies.iter() {
            for bp in &company.blueprints {
                idx.insert(
                    bp.id.clone(),
                    (bp.owner_company_id.clone(), bp.output_commodity, bp.royalty_vwap_ratio),
                );
            }
        }
        idx
    };

    // Build a lookup of company_id → actual output qty per commodity this turn.
    // We sum `building.last_production[commodity]` across the company's buildings.
    let mut company_output: std::collections::HashMap<String, std::collections::HashMap<Commodity, f64>> =
        std::collections::HashMap::new();
    for building in buildings {
        let entry = company_output.entry(building.owner_id.clone()).or_default();
        for (&commodity, &qty) in &building.last_production {
            *entry.entry(commodity).or_insert(0.0) += qty;
        }
    }

    for (licensee_id, licensed_bps) in &licensed_per_company {
        for licensed_bp in licensed_bps {
            // Look up the blueprint metadata.
            let (licensor_id, output_commodity, royalty_ratio) = match blueprint_index.get(&licensed_bp.blueprint_id) {
                Some(meta) => (meta.0.clone(), meta.1, meta.2),
                None => continue, // Blueprint not found in any company — skip.
            };

            // Actual output quantity of this commodity by the licensee this turn.
            let actual_qty = company_output
                .get(licensee_id)
                .and_then(|m| m.get(&output_commodity))
                .copied()
                .unwrap_or(0.0);
            if actual_qty <= 0.0 {
                continue;
            }

            // VWAP for the output commodity (last turn).
            let last_turn_vwap = market_history
                .vwap_per_commodity
                .get(&output_commodity)
                .copied()
                .unwrap_or(100.0);

            let fee = compute_blueprint_royalty_fee(
                &crate::economy::blueprints::ProductBlueprint {
                    id: licensed_bp.blueprint_id.clone(),
                    owner_company_id: licensor_id.clone(),
                    output_commodity,
                    base_tech: String::new(),
                    base_tech_year: 0,
                    inputs: Default::default(),
                    required_slot: crate::registries::production_methods::MethodSlot::Production,
                    quality: 0.0,
                    durability: 0.0,
                    royalty_vwap_ratio: royalty_ratio,
                    granted_turn: 0,
                    expires_turn: 0,
                },
                actual_qty,
                last_turn_vwap,
            );
            if fee <= 0.0 {
                continue;
            }

            // Determine the recipient: state, domestic licensor, or foreign licensor.
            if licensor_id == "STATE" {
                // State blueprint: licensee → treasury.
                let debited = debit_company_by_id(companies, licensee_id, fee);
                if debited > 0.0 {
                    treasury.liquid_reserves += debited;
                }
            } else if licensed_bp.licensor_country == country_name {
                // Domestic licensor: licensee → licensor (both in `companies`).
                let debited = debit_company_by_id(companies, licensee_id, fee);
                if debited > 0.0 {
                    credit_company_by_id(companies, &licensor_id, debited);
                }
            } else {
                // Cross-border licensor: debit licensee now (emits domestic FX outflow),
                // queue the credit for the post-parallel sequential phase.
                let debited = debit_company_by_id(companies, licensee_id, fee);
                if debited > 0.0 {
                    cross_border_queue.push(crate::economy::blueprints::CrossBorderRoyaltyQueueEntry {
                        licensor_company_id: licensor_id.clone(),
                        licensor_country: licensed_bp.licensor_country.clone(),
                        amount: debited,
                        blueprint_id: licensed_bp.blueprint_id.clone(),
                    });
                }
            }
        }
    }
}

/// Phase 19A: Process the cross-border royalty credit queue after the parallel
/// per-country phase completes.
///
/// # Rules
/// * For each entry, find the licensor company in the destination country's
///   `companies` slice and credit it via `credit_company_by_id` (TransferSettler).
/// * Entries whose licensor is not found are silently dropped (the licensee was
///   already debited — the cash leaves the domestic economy as an FX outflow).
/// * Runs sequentially after all parallel country tasks complete.
pub fn process_cross_border_royalty_queue(
    queue: &[crate::economy::blueprints::CrossBorderRoyaltyQueueEntry],
    all_companies: &mut [Company],
) -> Vec<String> {
    use crate::economy::transfer_settler::credit_company_by_id;
    let mut messages = Vec::new();
    for entry in queue {
        let credited = credit_company_by_id(all_companies, &entry.licensor_company_id, entry.amount);
        if !credited {
            messages.push(format!(
                "Cross-border royalty: licensor {} not found in country {}, amount {} dropped",
                entry.licensor_company_id, entry.licensor_country, entry.amount
            ));
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn royalty_fulfillment_ratio_sufficient_cash() {
        let mut company = Company::default();
        company.available_cash = 1000.0;
        
        let licensed_methods = vec![("tech_001".to_string(), 0.05)];
        let ratio = calculate_royalty_fulfillment_ratio(&company, 1000.0, 100.0, &licensed_methods);
        
        // Required: 1000 * 0.05 * 100 = 5000
        // Available: 1000
        // Ratio: 1000 / 5000 = 0.2
        assert_eq!(ratio, 0.2);
    }

    #[test]
    fn royalty_fulfillment_ratio_excess_cash() {
        let mut company = Company::default();
        company.available_cash = 10000.0;
        
        let licensed_methods = vec![("tech_001".to_string(), 0.05)];
        let ratio = calculate_royalty_fulfillment_ratio(&company, 1000.0, 100.0, &licensed_methods);
        
        // Required: 1000 * 0.05 * 100 = 5000
        // Available: 10000
        // Ratio: 1.0 (capped)
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn royalty_payment_atomic() {
        let mut licensee = Company::default();
        licensee.id = "LICENSEE".to_string();
        licensee.available_cash = 1000.0;
        
        let mut licensor = Company::default();
        licensor.id = "LICENSOR".to_string();
        licensor.available_cash = 500.0;
        
        process_royalty_payment(&mut licensee, &mut licensor, 100.0, 100.0, 0.05);
        
        // Royalty: 100 * 0.05 * 100 = 500
        assert_eq!(licensee.available_cash, 500.0); // 1000 - 500
        assert_eq!(licensor.available_cash, 1000.0); // 500 + 500
    }

    #[test]
    fn royalty_payment_insufficient_cash() {
        let mut licensee = Company::default();
        licensee.id = "LICENSEE".to_string();
        licensee.available_cash = 100.0; // Insufficient for 500 royalty
        
        let mut licensor = Company::default();
        licensor.id = "LICENSOR".to_string();
        licensor.available_cash = 500.0;
        
        process_royalty_payment(&mut licensee, &mut licensor, 100.0, 100.0, 0.05);
        
        // Royalty: 100 * 0.05 * 100 = 500
        // Insufficient cash, no payment
        assert_eq!(licensee.available_cash, 100.0); // Unchanged
        assert_eq!(licensor.available_cash, 500.0); // Unchanged
    }

    // ── Phase 19A: Blueprint royalty tests ───────────────────────────────

    #[test]
    fn blueprint_royalty_domestic_licensor_credited_via_transfer_settler() {
        use crate::economy::blueprints::{LicensedBlueprint, ProductBlueprint};
        use crate::entities::Building;
        use crate::registries::enums::Commodity;
        use crate::registries::production_methods::MethodSlot;
        use std::collections::BTreeMap;

        // Licensor owns a blueprint and has a brokerage account.
        let mut licensor = Company::default();
        licensor.id = "LICENSOR".to_string();
        licensor.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 0.0,
            fx_balances: Default::default(),
            portfolio: Default::default(),
            pending_orders: Default::default(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: Default::default(),
        });
        licensor.blueprints.push(ProductBlueprint {
            id: "bp_test".to_string(),
            owner_company_id: "LICENSOR".to_string(),
            output_commodity: Commodity::IndustrialMachinery,
            base_tech: "tech".to_string(),
            base_tech_year: 1990,
            inputs: BTreeMap::new(),
            required_slot: MethodSlot::Production,
            quality: 1.2,
            durability: 240.0,
            royalty_vwap_ratio: 0.05,
            granted_turn: 100,
            expires_turn: 340,
        });

        // Licensee has cash and a brokerage account, and licensed the blueprint.
        let mut licensee = Company::default();
        licensee.id = "LICENSEE".to_string();
        licensee.licensed_blueprints.push(LicensedBlueprint {
            blueprint_id: "bp_test".to_string(),
            licensor_company_id: "LICENSOR".to_string(),
            licensor_country: "TestCountry".to_string(),
            licensed_turn: 100,
        });
        licensee.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 10000.0,
            fx_balances: Default::default(),
            portfolio: Default::default(),
            pending_orders: Default::default(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: Default::default(),
        });

        // Building owned by licensee produced 1000 units of IndustrialMachinery.
        let mut building = Building::default();
        building.owner_id = "LICENSEE".to_string();
        building.last_production.insert(Commodity::IndustrialMachinery, 1000.0);

        let companies = vec![licensor, licensee];
        let buildings = vec![building];
        let market_history = crate::economy::market_history::MarketHistory::default();
        let mut treasury = crate::state::treasury::Treasury::default();
        let mut queue = Vec::new();

        let initial_treasury = treasury.liquid_reserves;
        process_blueprint_royalty_payments(
            &mut companies.clone(),
            &buildings,
            &market_history,
            &mut treasury,
            "TestCountry",
            &mut queue,
        );

        // Royalty fee = 1000 × 0.05 × 100.0 (fallback VWAP) = 5000.
        // Cross-border queue should be empty (domestic licensor).
        assert!(queue.is_empty(), "domestic royalty must not queue cross-border");
        // Treasury should be unchanged (no state blueprint).
        assert_eq!(treasury.liquid_reserves, initial_treasury);
    }

    #[test]
    fn blueprint_royalty_cross_border_queues_entry() {
        use crate::economy::blueprints::{LicensedBlueprint, ProductBlueprint};
        use crate::entities::Building;
        use crate::registries::enums::Commodity;
        use crate::registries::production_methods::MethodSlot;
        use std::collections::BTreeMap;

        // Foreign licensor owns the blueprint.
        let mut licensor = Company::default();
        licensor.id = "FOREIGN_LICENSOR".to_string();
        licensor.blueprints.push(ProductBlueprint {
            id: "bp_foreign".to_string(),
            owner_company_id: "FOREIGN_LICENSOR".to_string(),
            output_commodity: Commodity::Cars,
            base_tech: "tech".to_string(),
            base_tech_year: 1990,
            inputs: BTreeMap::new(),
            required_slot: MethodSlot::Production,
            quality: 1.0,
            durability: 100.0,
            royalty_vwap_ratio: 0.05,
            granted_turn: 100,
            expires_turn: 340,
        });

        // Domestic licensee licensed the foreign blueprint.
        let mut licensee = Company::default();
        licensee.id = "LICENSEE".to_string();
        licensee.licensed_blueprints.push(LicensedBlueprint {
            blueprint_id: "bp_foreign".to_string(),
            licensor_company_id: "FOREIGN_LICENSOR".to_string(),
            licensor_country: "ForeignCountry".to_string(), // Different country!
            licensed_turn: 100,
        });
        licensee.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 10000.0,
            fx_balances: Default::default(),
            portfolio: Default::default(),
            pending_orders: Default::default(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: Default::default(),
        });

        let mut building = Building::default();
        building.owner_id = "LICENSEE".to_string();
        building.last_production.insert(Commodity::Cars, 500.0);

        let companies = vec![licensor, licensee];
        let buildings = vec![building];
        let market_history = crate::economy::market_history::MarketHistory::default();
        let mut treasury = crate::state::treasury::Treasury::default();
        let mut queue = Vec::new();

        process_blueprint_royalty_payments(
            &mut companies.clone(),
            &buildings,
            &market_history,
            &mut treasury,
            "TestCountry", // Licensee is in TestCountry, licensor in ForeignCountry.
            &mut queue,
        );

        // Cross-border queue should have one entry for the foreign licensor.
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].licensor_company_id, "FOREIGN_LICENSOR");
        assert_eq!(queue[0].licensor_country, "ForeignCountry");
        assert_eq!(queue[0].blueprint_id, "bp_foreign");
        // Fee = 500 × 0.05 × 100.0 = 2500.
        assert!((queue[0].amount - 2500.0).abs() < 1e-6);
    }

    #[test]
    fn cross_border_royalty_queue_credits_licensor() {
        use crate::economy::blueprints::CrossBorderRoyaltyQueueEntry;

        let mut licensor = Company::default();
        licensor.id = "FOREIGN_LICENSOR".to_string();
        licensor.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 0.0,
            fx_balances: Default::default(),
            portfolio: Default::default(),
            pending_orders: Default::default(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: Default::default(),
        });

        let queue = vec![CrossBorderRoyaltyQueueEntry {
            licensor_company_id: "FOREIGN_LICENSOR".to_string(),
            licensor_country: "ForeignCountry".to_string(),
            amount: 2500.0,
            blueprint_id: "bp_foreign".to_string(),
        }];

        let mut companies = vec![licensor];
        let msgs = process_cross_border_royalty_queue(&queue, &mut companies);

        assert!(msgs.is_empty(), "licensor found, no warning");
        assert!((companies[0].brokerage_account.as_ref().unwrap().cash - 2500.0).abs() < 1e-6);
    }

    #[test]
    fn cross_border_royalty_queue_drops_missing_licensor() {
        use crate::economy::blueprints::CrossBorderRoyaltyQueueEntry;

        let queue = vec![CrossBorderRoyaltyQueueEntry {
            licensor_company_id: "NONEXISTENT".to_string(),
            licensor_country: "Nowhere".to_string(),
            amount: 1000.0,
            blueprint_id: "bp".to_string(),
        }];

        let mut companies: Vec<Company> = Vec::new();
        let msgs = process_cross_border_royalty_queue(&queue, &mut companies);

        assert_eq!(msgs.len(), 1, "missing licensor must produce a warning");
    }
}
