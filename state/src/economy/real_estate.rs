//! Real estate system for retail leases (Phase 6.5).
//!
//! Implements retail rent accrual, lease signing, and diversity bonus
//! for shopping centers.
//!
//! Phase 24C-Final: Rent payments now route through `TransferSettler`
//! for strict double-entry accounting (tenant's owner is debited,
//! shopping center's owner is credited).

use crate::society::housing::{CommercialBuilding, RetailLease, StoreProfile};
use crate::entities::Company;
use crate::state::Country;
use std::collections::BTreeSet;

/// Accrue retail rents for shopping center landlords (Phase 6.5, Phase R7).
///
/// Phase 24C-Final: Rent payments are now routed through `TransferSettler`
/// for strict double-entry accounting. The tenant building's owner company
/// is debited and the shopping center's owner company is credited.
///
/// # Arguments
/// * `shopping_center` - Shopping center building with profile
/// * `tenant_owner_ids` - Pre-resolved owner company IDs for each tenant
///   (parallel to the active leases after filtering; empty string if no owner)
/// * `companies` - Mutable companies (for TransferSettler debit/credit)
/// * `country` - Mutable country state (for bank balance sheet sync)
/// * `current_turn` - Current turn number
///
/// # Returns
/// * `f64` - Total rent actually collected this turn
///
/// # Rules
/// * Collects rent from active leases (within their duration)
/// * Tenant's owner is debited via TransferSettler
/// * Shopping center's owner is credited via TransferSettler
/// * If tenant's owner cannot pay, rent is skipped (no forced collection)
/// * Expired leases are dropped
/// * Used in R7 phase after B2C clearing
pub fn accrue_retail_rents(
    shopping_center: &mut CommercialBuilding,
    tenant_owner_ids: &[(String, f64, String)], // (tenant_building_id, rent_due, owner_company_id)
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
) -> f64 {
    let _ = current_turn; // Used for lease filtering done by caller

    let mut total_rent = 0.0;

    // The shopping center's owner is the landlord (recipient)
    let landlord_owner_id = shopping_center.owner_id.clone();

    // Update the lease list: keep only active leases
    let active_count = tenant_owner_ids.len();
    let _ = active_count;

    // Phase 24C-Final: Route each rent payment through TransferSettler
    for (_tenant_building_id, rent_due, tenant_owner_id) in tenant_owner_ids {
        if *rent_due <= 0.0 {
            continue;
        }

        // Skip if tenant owner is empty
        if tenant_owner_id.is_empty() {
            continue;
        }

        // Skip if tenant and landlord are the same owner (intra-company transfer is a no-op)
        if *tenant_owner_id == landlord_owner_id {
            total_rent += rent_due;
            continue;
        }

        // Find company indices for TransferSettler
        let payer_idx = companies.iter().position(|c| c.id == *tenant_owner_id);
        let recipient_idx = companies.iter().position(|c| c.id == landlord_owner_id);

        match (payer_idx, recipient_idx) {
            (Some(p_idx), Some(r_idx)) => {
                // Use TransferSettler for strict double-entry accounting
                let result = crate::economy::transfer_settler::settle_company_to_company(
                    companies,
                    p_idx,
                    r_idx,
                    *rent_due,
                    country,
                );
                if result.is_ok() {
                    total_rent += rent_due;
                }
                // If transfer fails (insufficient cash), rent is not collected
                // — the tenant cannot pay. This is realistic.
            }
            _ => {
                // One or both companies not found — skip this rent payment
            }
        }
    }

    total_rent
}

/// Sign new retail leases for shopping centers (Phase 6.5, Phase R7).
///
/// # Arguments
/// * `shopping_center` - Shopping center building with profile
/// * `available_stores` - Available retail buildings seeking leases
/// * `current_turn` - Current turn number
///
/// # Returns
/// * `Vec<RetailLease>` - Newly signed leases
///
/// # Rules
/// * Stores without landlord_building_id are candidates
/// * Lease duration defaults to 12 turns
/// * Rent per sqm based on shopping center base rate
/// * Updates tenant retail profiles with landlord_id
/// * Used in R7 phase after rent accrual
pub fn sign_retail_leases(
    shopping_center: &mut CommercialBuilding,
    available_stores: &mut [CommercialBuilding],
    current_turn: u32,
) -> Vec<RetailLease> {
    let mut new_leases = Vec::new();
    
    if let Some(profile) = &mut shopping_center.shopping_center_profile {
        let base_rent_per_sqm = shopping_center.rent_per_sqm;
        let lease_duration = 12; // Default 12 turns
        
        for store in available_stores {
            if let Some(store_profile) = &mut store.retail_profile {
                // Only sign leases for stores without landlords
                if store_profile.landlord_building_id.is_none() {
                    let leased_sqm = store.retail_capacity;
                    let rent_per_sqm = base_rent_per_sqm;
                    
                    let lease = RetailLease {
                        tenant_id: store.id.clone(),
                        leased_sqm,
                        rent_per_sqm,
                        start_turn: current_turn,
                        duration_turns: lease_duration,
                    };
                    
                    // Update store profile
                    store_profile.landlord_building_id = Some(shopping_center.id.clone());
                    store_profile.leased_sqm = leased_sqm;
                    
                    // Update shopping center profile
                    profile.tenant_building_ids.push(store.id.clone());
                    
                    new_leases.push(lease);
                }
            }
        }
    }
    
    new_leases
}

/// Calculate diversity bonus for shopping centers (Phase 6.5, Phase R2).
///
/// # Arguments
/// * `shopping_center` - Shopping center building with profile
/// * `all_buildings` - All commercial buildings (for tenant profile lookup)
///
/// # Returns
/// * `f64` - Diversity bonus (0.0-1.0)
///
/// # Rules
/// * Bonus based on variety of store profiles
/// * More unique store types = higher bonus
/// * Bonus increases attractiveness for consumers
/// * Used in R2 phase when computing effective_attractiveness
pub fn calculate_diversity_bonus(
    shopping_center: &CommercialBuilding,
    all_buildings: &[CommercialBuilding],
) -> f64 {
    if let Some(profile) = &shopping_center.shopping_center_profile {
        let mut unique_profiles: BTreeSet<StoreProfile> = BTreeSet::new();

        // Phase 24C.9: Look up actual store profiles from tenant buildings
        // instead of using placeholder hardcoded values.
        for tenant_id in &profile.tenant_building_ids {
            if let Some(tenant) = all_buildings.iter().find(|b| b.id == *tenant_id) {
                if let Some(ref store_profile) = tenant.retail_profile {
                    unique_profiles.extend(store_profile.profiles.iter().copied());
                }
            }
        }

        // Bonus = unique_count / max_possible (7 store types)
        let unique_count = unique_profiles.len() as f64;
        let max_possible = 7.0; // Total StoreProfile variants

        unique_count / max_possible
    } else {
        0.0
    }
}

/// Update anchor tenant for shopping centers (Phase 6.5, Phase R7).
///
/// # Arguments
/// * `shopping_center` - Shopping center building with profile
/// * `all_buildings` - All commercial buildings (for sales lookup)
///
/// # Rules
/// * Anchor tenant is the store with highest sales volume
/// * Provides traffic boost to entire mall
/// * Used in R7 phase after B2C clearing
pub fn update_anchor_tenant(
    shopping_center: &mut CommercialBuilding,
    all_buildings: &[CommercialBuilding],
) {
    if let Some(profile) = &mut shopping_center.shopping_center_profile {
        let mut best_tenant: Option<String> = None;
        let mut best_sales: f64 = 0.0;

        // Phase 24C.9: Look up actual sales volume from tenant buildings
        // using total units sold last turn as the sales metric.
        for tenant_id in &profile.tenant_building_ids {
            if let Some(tenant) = all_buildings.iter().find(|b| b.id == *tenant_id) {
                let tenant_sales: f64 = tenant
                    .retail_profile
                    .as_ref()
                    .map(|p| p.units_sold_last_turn.values().sum())
                    .unwrap_or(0.0);
                if tenant_sales > best_sales {
                    best_sales = tenant_sales;
                    best_tenant = Some(tenant_id.clone());
                }
            }
        }

        profile.anchor_tenant = best_tenant;
    }
}
