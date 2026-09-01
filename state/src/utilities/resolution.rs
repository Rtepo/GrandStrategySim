//! Strategic Resolution for bankrupt utility companies — Phase 8.4.
//!
//! When a utility company (Sector::Energy) goes bankrupt, it cannot simply be
//! liquidated — energy is critical infrastructure. Instead, a Strategic Resolution
//! process is triggered: the State takes over as a Bridge Institution, covers
//! operational losses from the Treasury, and forces restructuring.

use crate::entities::Company;
use crate::registries::enums::Sector;

/// Configuration for the Strategic Resolution process.
#[derive(Debug, Clone)]
pub struct StrategicResolution {
    /// Company ID under resolution.
    pub company_id: String,
    /// Turns remaining in receivership.
    pub turns_in_receivership: u32,
    /// Maximum turns before nationalization.
    pub max_turns_before_nationalization: u32,
    /// Cumulative Treasury subsidies paid during resolution.
    pub cumulative_subsidies: f64,
}

impl StrategicResolution {
    /// Maximum turns a utility company can stay in receivership before nationalization.
    pub const DEFAULT_MAX_TURNS: u32 = 10;

    /// Create a new Strategic Resolution for a bankrupt utility company.
    pub fn new(company_id: String) -> Self {
        Self {
            company_id,
            turns_in_receivership: 0,
            max_turns_before_nationalization: Self::DEFAULT_MAX_TURNS,
            cumulative_subsidies: 0.0,
        }
    }

    /// Check if a company should trigger Strategic Resolution instead of liquidation.
    ///
    /// # Arguments
    /// * `company` - The company to check.
    ///
    /// # Returns
    /// * `true` if the company is in `Sector::Energy` and bankrupt (negative available_cash).
    pub fn should_trigger(company: &Company) -> bool {
        company.sector == Sector::Energy
            && company.available_cash < 0.0
            && !company.is_in_receivership
    }

    /// Process one turn of Strategic Resolution for a company in receivership.
    ///
    /// # Arguments
    /// * `company` - Mutable company in receivership.
    /// * `treasury_reserves` - Mutable treasury liquid reserves (subsidies deducted).
    ///
    /// # Returns
    /// * `true` if the company has recovered (positive OCF), `false` if still in resolution.
    pub fn process_turn(&mut self, company: &mut Company, treasury_reserves: &mut f64) -> bool {
        self.turns_in_receivership += 1;

        // Bridge Institution: Treasury covers operational losses
        let operating_loss = (-company.available_cash).max(0.0);
        if operating_loss > 0.0 {
            let subsidy = operating_loss.min(*treasury_reserves);
            *treasury_reserves -= subsidy;
            company.available_cash += subsidy;
            self.cumulative_subsidies += subsidy;
        }

        // Recovery check: positive available_cash means viable again
        if company.available_cash > 0.0 {
            company.is_in_receivership = false;
            return true;
        }

        // Nationalization check: too many turns in receivership
        if self.turns_in_receivership >= self.max_turns_before_nationalization {
            // Nationalize: state takes ownership, ensure minimal output
            company.is_in_receivership = false;
            // Mark for privatization queue (future: BankruptcyAuctionPool.privatization_queue)
            return true;
        }

        false
    }
}
