//! Tax-advantaged retirement account wrappers for citizen investments.
//!
//! This module implements tax-advantaged investment accounts that legally
//! exempt holders from certain taxes. Two variants are provided:
//! - `TaxFreeGrowth`: Exempt from dividend withholding tax AND capital gains tax.
//!   Analogous to a Roth IRA.
//! - `TaxDeferred`: Exempt from capital gains tax only; contributions are
//!   deductible from personal income tax (PIT). Analogous to a Traditional IRA.

use serde::{Deserialize, Serialize};

/// Variant of tax-advantaged retirement account.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum RetirementAccountVariant {
    /// Tax-free growth account (analogous to Roth IRA / IKE).
    /// Exempt from dividend withholding tax AND capital gains tax.
    /// Contributions are made with after-tax income (no PIT deduction).
    #[default]
    TaxFreeGrowth,
    /// Tax-deferred account (analogous to Traditional IRA / IKZE).
    /// Exempt from capital gains tax only. Contributions are deductible
    /// from PIT (taxable income reduced by contribution amount, clamped at zero).
    TaxDeferred,
}

/// A tax-advantaged retirement account wrapper attached to a BrokerageAccount.
///
/// When present on a brokerage account, dividends and capital gains
/// routed through that account receive the tax exemptions defined by
/// the `variant` field.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TaxAdvantagedAccount {
    /// The tax treatment variant (TaxFreeGrowth or TaxDeferred).
    #[serde(default)]
    pub variant: RetirementAccountVariant,
    /// Total contributions made this fiscal year.
    #[serde(default)]
    pub contribution_this_year: f64,
    /// Maximum contribution allowed this year (scaled by average_wage).
    #[serde(default)]
    pub contribution_limit: f64,
    /// Minimum turn before penalty-free withdrawal (age-based).
    #[serde(default)]
    pub withdrawal_turn: u32,
}

impl TaxAdvantagedAccount {
    /// Create a new TaxFreeGrowth account with a contribution limit.
    pub fn tax_free_growth(contribution_limit: f64, withdrawal_turn: u32) -> Self {
        Self {
            variant: RetirementAccountVariant::TaxFreeGrowth,
            contribution_this_year: 0.0,
            contribution_limit,
            withdrawal_turn,
        }
    }

    /// Create a new TaxDeferred account with a contribution limit.
    pub fn tax_deferred(contribution_limit: f64, withdrawal_turn: u32) -> Self {
        Self {
            variant: RetirementAccountVariant::TaxDeferred,
            contribution_this_year: 0.0,
            contribution_limit,
            withdrawal_turn,
        }
    }

    /// Returns true if this account is exempt from dividend withholding tax.
    /// Only TaxFreeGrowth is exempt; TaxDeferred pays dividend tax.
    pub fn exempt_from_dividend_tax(&self) -> bool {
        self.variant == RetirementAccountVariant::TaxFreeGrowth
    }

    /// Returns true if this account is exempt from capital gains tax.
    /// Both variants are exempt from CGT.
    pub fn exempt_from_capital_gains_tax(&self) -> bool {
        true
    }

    /// Returns true if contributions are deductible from PIT.
    /// Only TaxDeferred offers a PIT deduction.
    pub fn has_pit_deduction(&self) -> bool {
        self.variant == RetirementAccountVariant::TaxDeferred
    }

    /// Record a contribution. Returns true if accepted (within limit), false if over limit.
    pub fn record_contribution(&mut self, amount: f64) -> bool {
        if self.contribution_this_year + amount > self.contribution_limit {
            return false;
        }
        self.contribution_this_year += amount;
        true
    }

    /// Reset annual contribution tracking at fiscal year-end.
    pub fn reset_year(&mut self) {
        self.contribution_this_year = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tax_free_growth_exempt_from_dividend_tax() {
        let account = TaxAdvantagedAccount::tax_free_growth(1000.0, 100);
        assert!(account.exempt_from_dividend_tax());
        assert!(account.exempt_from_capital_gains_tax());
        assert!(!account.has_pit_deduction());
    }

    #[test]
    fn test_tax_deferred_not_exempt_from_dividend_tax() {
        let account = TaxAdvantagedAccount::tax_deferred(1000.0, 100);
        assert!(!account.exempt_from_dividend_tax());
        assert!(account.exempt_from_capital_gains_tax());
        assert!(account.has_pit_deduction());
    }

    #[test]
    fn test_contribution_within_limit() {
        let mut account = TaxAdvantagedAccount::tax_free_growth(1000.0, 100);
        assert!(account.record_contribution(500.0));
        assert!((account.contribution_this_year - 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_contribution_over_limit_rejected() {
        let mut account = TaxAdvantagedAccount::tax_free_growth(1000.0, 100);
        assert!(account.record_contribution(600.0));
        assert!(!account.record_contribution(500.0));
        assert!((account.contribution_this_year - 600.0).abs() < 1e-6);
    }

    #[test]
    fn test_reset_year() {
        let mut account = TaxAdvantagedAccount::tax_free_growth(1000.0, 100);
        account.record_contribution(500.0);
        account.reset_year();
        assert!((account.contribution_this_year - 0.0).abs() < 1e-6);
    }
}
