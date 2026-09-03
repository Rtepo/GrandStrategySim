//! State concession tracking for privatized state monopolies.
//!
//! When a state monopoly is privatized via `LawType::StateMonopolyPrivatization`,
//! the privatized company receives usufruct rights on state land but must pay
//! recurring concession royalties to the Treasury. This module tracks those
//! obligations and processes royalty payments each turn.
//!
//! ## Double-Entry Rules
//!
//! - Royalties are debited from the privatized company's cash via `settle_transfer`
//! - Royalties are credited to `country.budget.liquid_reserves` (Treasury)
//! - If a company cannot pay, usufruct is revoked on all linked parcels
//! - If a company enters bankruptcy, the concession is revoked (re-nationalization)

use crate::economy::transfer_settler::{settle_transfer, TransferRecipient};
use crate::entities::Company;
use crate::entities::legal_form::{
    LegalForm, LegalFormTransition, LegalTransition, TransitionContext,
};
use crate::politics::laws::StateMonopolyPrivatizationLaw;
use crate::society::cadastre::{index_to_parcel_id, parcel_id_to_index};
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// An active state concession granted to a privatized company.
///
/// Tracks the recurring royalty obligation for the right to operate
/// on state-owned land (forests, waters, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateConcession {
    /// Unique concession ID
    pub concession_id: String,
    /// Company ID of the privatized entity holding the concession
    pub holder_company_id: String,
    /// Sector covered by this concession (e.g., "Forestry", "Waters")
    pub sector: String,
    /// Annual royalty rate as a fraction of gross revenue (0.0–1.0)
    pub royalty_rate: f64,
    /// Cadastre parcel IDs where the holder has usufruct rights
    pub parcel_ids: Vec<u32>,
    /// Turn the concession was granted
    pub granted_turn: u32,
    /// Whether the concession is currently active
    pub active: bool,
    /// Total royalties paid to date
    #[serde(default)]
    pub total_royalties_paid: f64,
    /// Number of consecutive turns the holder failed to pay royalties
    #[serde(default)]
    pub consecutive_payment_failures: u32,
}

/// Registry of all active and historical state concessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StateConcessionRegistry {
    /// All concessions (active and revoked)
    pub concessions: Vec<StateConcession>,
}

impl StateConcessionRegistry {
    /// Add a new concession to the registry.
    pub fn add(&mut self, concession: StateConcession) {
        self.concessions.push(concession);
    }

    /// Get all active concessions for a specific company.
    pub fn active_for_company(&self, company_id: &str) -> Vec<&StateConcession> {
        self.concessions
            .iter()
            .filter(|c| c.holder_company_id == company_id && c.active)
            .collect()
    }

    /// Get all active concessions.
    pub fn active(&self) -> Vec<&StateConcession> {
        self.concessions.iter().filter(|c| c.active).collect()
    }

    /// Get mutable access to all active concessions.
    pub fn active_mut(&mut self) -> Vec<&mut StateConcession> {
        self.concessions.iter_mut().filter(|c| c.active).collect()
    }

    /// Revoke a concession by company ID (e.g., on bankruptcy).
    /// Returns the parcel IDs that were under this concession.
    pub fn revoke_for_company(&mut self, company_id: &str) -> Vec<u32> {
        let mut revoked_parcels = Vec::new();
        for c in self.concessions.iter_mut() {
            if c.holder_company_id == company_id && c.active {
                c.active = false;
                revoked_parcels.extend(c.parcel_ids.clone());
            }
        }
        revoked_parcels
    }

    /// Revoke a specific concession by ID.
    pub fn revoke(&mut self, concession_id: &str) -> Option<Vec<u32>> {
        for c in self.concessions.iter_mut() {
            if c.concession_id == concession_id && c.active {
                c.active = false;
                return Some(c.parcel_ids.clone());
            }
        }
        None
    }
}

/// Process pending privatization decrees from the politics layer.
///
/// For each pending decree:
/// 1. Find the StateMonopoly company matching `target_sector`
/// 2. Transition its legal form to JointStockCompany
/// 3. Set `usufruct_holder` on all managed cadastre parcels (land stays State-owned)
/// 4. Route IPO proceeds to Treasury via `settle_transfer`
/// 5. Register a `StateConcession` for recurring royalty tracking
///
/// # Arguments
/// * `country` - Mutable country (for cadastre, concessions, treasury, politics)
/// * `companies` - Mutable companies slice (for legal form transition and IPO payment)
/// * `current_turn` - Current turn number
///
/// # Returns
/// A vector of human-readable messages describing the privatizations executed.
pub fn process_pending_privatizations(
    country: &mut Country,
    companies: &mut [Company],
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    if country.politics.pending_privatizations.is_empty() {
        return messages;
    }

    // Drain the pending privatizations to avoid double-processing
    let pending: Vec<StateMonopolyPrivatizationLaw> =
        std::mem::take(&mut country.politics.pending_privatizations);

    for law in pending {
        // 1. Find the StateMonopoly company for the target sector
        let monopoly_idx = companies.iter().position(|c| {
            matches!(
                &c.legal_form,
                LegalForm::StateMonopoly(data) if data.controlled_sector == law.target_sector
            )
        });

        let monopoly_idx = match monopoly_idx {
            Some(idx) => idx,
            None => {
                messages.push(format!(
                    "Privatization failed: no StateMonopoly found for sector '{}'",
                    law.target_sector
                ));
                continue;
            }
        };

        // 2. Record the company ID and state_share before transition
        let company_id = companies[monopoly_idx].id.clone();
        let old_state_share = companies[monopoly_idx].state_share;

        // 3. Transition the legal form to JointStockCompany
        // Build a minimal TransitionContext — the transition is state-mandated,
        // not company-initiated, so we use minimal economic context.
        // Clone the company to avoid borrow conflicts with the mutable take below.
        let company_snapshot = companies[monopoly_idx].clone();
        let dummy_market_signal = crate::economy::market::MarketSignal::default();
        let ctx = TransitionContext {
            company: &company_snapshot,
            sector_pmi: 50.0,
            stock_confidence: 60.0,
            market_signal: &dummy_market_signal,
            private_capital_pool: 0.0,
            bank_credit_rate: 0.05,
            average_wage: country.macro_indicators.average_wage.max(1.0),
        };

        let old_legal_form = std::mem::take(&mut companies[monopoly_idx].legal_form);
        match old_legal_form.try_transition(LegalTransition::StateMonopolyToJointStockCompany, &ctx)
        {
            Ok(new_form) => {
                companies[monopoly_idx].legal_form = new_form;
                // Reduce state_share by the privatization fraction
                companies[monopoly_idx].state_share =
                    old_state_share * (1.0 - law.privatization_fraction);
            }
            Err(e) => {
                // Restore the old legal form on failure
                companies[monopoly_idx].legal_form =
                    LegalForm::StateMonopoly(crate::entities::legal_form::StateMonopolyData {
                        controlled_sector: law.target_sector.clone(),
                        ..Default::default()
                    });
                messages.push(format!(
                    "Privatization failed for sector '{}': {}",
                    law.target_sector, e.reason
                ));
                continue;
            }
        }

        // 4. Set usufruct_holder on all state forest/water parcels managed
        //    by this monopoly. Land ownership stays with the State (TREASURY).
        let mut parcel_ids: Vec<u32> = Vec::new();
        let land_use_tag = match law.target_sector.as_str() {
            "Forestry" => "forest_district",
            "Waters" => "state_waters",
            _ => "",
        };
        if !land_use_tag.is_empty() {
            for (parcel_id, parcel) in country.cadastre.parcels.iter_mut() {
                if parcel.owner_type == crate::society::cadastre::ParcelOwnerType::State
                    && parcel.land_use_tag == land_use_tag
                {
                    parcel.usufruct_holder = Some(company_id.clone());
                    parcel_ids.push(parcel_id_to_index(parcel_id));
                }
            }
        }

        // 5. Route IPO proceeds to Treasury via settle_transfer
        // IPO proceeds = shares_issued * ipo_price_per_share * privatization_fraction
        let ipo_proceeds =
            law.shares_issued as f64 * law.ipo_price_per_share * law.privatization_fraction;
        if ipo_proceeds > 0.0 {
            let _ = settle_transfer(
                companies,
                monopoly_idx,
                ipo_proceeds,
                &TransferRecipient::Treasury,
                country,
            );
        }

        // 6. Register the concession for recurring royalty tracking
        let concession = StateConcession {
            concession_id: format!("CONC-{}-{}", law.target_sector, current_turn),
            holder_company_id: company_id.clone(),
            sector: law.target_sector.clone(),
            royalty_rate: law.concession_royalty_rate,
            parcel_ids,
            granted_turn: current_turn,
            active: true,
            total_royalties_paid: 0.0,
            consecutive_payment_failures: 0,
        };
        country.state_concessions.add(concession);

        messages.push(format!(
            "Privatized state monopoly '{}': sector={}, fraction={:.2}, IPO proceeds={:.2}, royalty_rate={:.3}, parcels={}",
            company_id, law.target_sector, law.privatization_fraction,
            ipo_proceeds, law.concession_royalty_rate,
            country.state_concessions.active_for_company(&company_id).len()
        ));
    }

    messages
}

/// Process concession royalty payments for one turn.
///
/// For each active concession:
/// 1. Compute the royalty = royalty_rate * company's gross revenue
/// 2. Debit the company and credit the Treasury via `settle_transfer`
/// 3. If the company cannot pay, increment failure counter
/// 4. If failures exceed threshold, revoke usufruct and the concession
///
/// # Arguments
/// * `country` - Mutable country (for treasury and concession registry)
/// * `companies` - Mutable companies slice (for royalty debit)
/// * `current_turn` - Current turn number
///
/// # Returns
/// A vector of human-readable messages about royalty payments and revocations.
pub fn process_concession_royalties(
    country: &mut Country,
    companies: &mut [Company],
    _current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Collect concession data to avoid borrowing issues
    let concessions_data: Vec<(String, String, f64, Vec<u32>)> = country
        .state_concessions
        .active()
        .into_iter()
        .map(|c| {
            (
                c.concession_id.clone(),
                c.holder_company_id.clone(),
                c.royalty_rate,
                c.parcel_ids.clone(),
            )
        })
        .collect();

    for (concession_id, company_id, royalty_rate, _parcel_ids) in concessions_data {
        // Find the company
        let company_idx = companies.iter().position(|c| c.id == company_id);
        let company_idx = match company_idx {
            Some(idx) => idx,
            None => {
                // Company no longer exists — revoke concession
                let revoked = country.state_concessions.revoke(&concession_id);
                if let Some(parcels) = revoked {
                    for pid in &parcels {
                        if let Some(parcel) =
                            country.cadastre.parcels.get_mut(index_to_parcel_id(*pid))
                        {
                            parcel.usufruct_holder = None;
                        }
                    }
                    messages.push(format!(
                        "Concession {} revoked: company {} no longer exists (parcels reverted)",
                        concession_id, company_id
                    ));
                }
                continue;
            }
        };

        // Check if company is liquidated
        if companies[company_idx].is_liquidated {
            let revoked = country.state_concessions.revoke(&concession_id);
            if let Some(parcels) = revoked {
                for pid in &parcels {
                    if let Some(parcel) =
                        country.cadastre.parcels.get_mut(index_to_parcel_id(*pid))
                    {
                        parcel.usufruct_holder = None;
                    }
                }
                messages.push(format!(
                    "Concession {} revoked: company {} liquidated (parcels reverted)",
                    concession_id, company_id
                ));
            }
            continue;
        }

        // Compute the royalty payment. The royalty rate is applied to the
        // company's available cash (which represents their operating revenue
        // surplus for this turn). This is a liquidity-based concession fee.
        let available = companies[company_idx]
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(companies[company_idx].available_cash);

        // Royalty = royalty_rate * available_cash (concession fee on operating revenue)
        let royalty = royalty_rate * available.max(0.0);

        if royalty < 0.01 {
            continue;
        }

        if available >= royalty {
            // Company can pay — route to Treasury via settle_transfer
            let _ = settle_transfer(
                companies,
                company_idx,
                royalty,
                &TransferRecipient::Treasury,
                country,
            );
            // Update concession tracking
            for c in country.state_concessions.concessions.iter_mut() {
                if c.concession_id == concession_id {
                    c.total_royalties_paid += royalty;
                    c.consecutive_payment_failures = 0;
                    break;
                }
            }
            messages.push(format!(
                "Royalty payment: {} paid {:.2} to Treasury (concession {})",
                company_id, royalty, concession_id
            ));
        } else {
            // Company cannot pay — increment failure counter
            for c in country.state_concessions.concessions.iter_mut() {
                if c.concession_id == concession_id {
                    c.consecutive_payment_failures += 1;
                    break;
                }
            }
            // Check if we should revoke (3 consecutive failures)
            let should_revoke = country
                .state_concessions
                .concessions
                .iter()
                .find(|c| c.concession_id == concession_id)
                .map(|c| c.consecutive_payment_failures >= 3)
                .unwrap_or(false);

            if should_revoke {
                let revoked = country.state_concessions.revoke(&concession_id);
                if let Some(parcels) = revoked {
                    for pid in &parcels {
                        if let Some(parcel) = country
                            .cadastre
                            .parcels
                            .get_mut(index_to_parcel_id(*pid))
                        {
                            parcel.usufruct_holder = None;
                        }
                    }
                    messages.push(format!(
                        "Concession {} REVOKED: {} failed to pay royalties 3 turns (parcels reverted to State)",
                        concession_id, company_id
                    ));
                }
            } else {
                messages.push(format!(
                    "Royalty default: {} cannot pay {:.2} (concession {})",
                    company_id, royalty, concession_id
                ));
            }
        }
    }

    messages
}

/// Revoke all concessions for a company that has entered bankruptcy/liquidation.
///
/// # Arguments
/// * `country` - Mutable country (for cadastre and concession registry)
/// * `company_id` - ID of the company being liquidated
///
/// # Returns
/// The number of concessions revoked.
pub fn revoke_concessions_for_liquidated_company(
    country: &mut Country,
    company_id: &str,
) -> usize {
    let revoked_parcels = country.state_concessions.revoke_for_company(company_id);
    for pid in &revoked_parcels {
        if let Some(parcel) =
            country.cadastre.parcels.get_mut(index_to_parcel_id(*pid))
        {
            parcel.usufruct_holder = None;
        }
    }
    // Count how many were revoked
    revoked_parcels.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concession_registry_add_and_query() {
        let mut registry = StateConcessionRegistry::default();
        registry.add(StateConcession {
            concession_id: "CONC-1".to_string(),
            holder_company_id: "CORP-1".to_string(),
            sector: "Forestry".to_string(),
            royalty_rate: 0.05,
            parcel_ids: vec![1, 2, 3],
            granted_turn: 10,
            active: true,
            total_royalties_paid: 0.0,
            consecutive_payment_failures: 0,
        });
        assert_eq!(registry.active().len(), 1);
        let for_company = registry.active_for_company("CORP-1");
        assert_eq!(for_company.len(), 1);
        assert_eq!(for_company[0].sector, "Forestry");
    }

    #[test]
    fn test_concession_revocation_returns_parcels() {
        let mut registry = StateConcessionRegistry::default();
        registry.add(StateConcession {
            concession_id: "CONC-1".to_string(),
            holder_company_id: "CORP-1".to_string(),
            sector: "Forestry".to_string(),
            royalty_rate: 0.05,
            parcel_ids: vec![1, 2, 3],
            granted_turn: 10,
            active: true,
            total_royalties_paid: 0.0,
            consecutive_payment_failures: 0,
        });
        let revoked = registry.revoke_for_company("CORP-1");
        assert_eq!(revoked, vec![1, 2, 3]);
        assert_eq!(registry.active().len(), 0);
    }

    #[test]
    fn test_revoke_specific_concession() {
        let mut registry = StateConcessionRegistry::default();
        registry.add(StateConcession {
            concession_id: "CONC-1".to_string(),
            holder_company_id: "CORP-1".to_string(),
            sector: "Forestry".to_string(),
            royalty_rate: 0.05,
            parcel_ids: vec![1, 2],
            granted_turn: 10,
            active: true,
            total_royalties_paid: 0.0,
            consecutive_payment_failures: 0,
        });
        let revoked = registry.revoke("CONC-1");
        assert!(revoked.is_some());
        assert_eq!(revoked.unwrap(), vec![1, 2]);
        assert_eq!(registry.active().len(), 0);
    }
}
