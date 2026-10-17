//! Phase 55: Capital gains tax system for securities and commodities.
//!
//! This module implements per-entity capital gains tracking with year-end
//! tax-loss harvesting. Government bond coupons are taxed at source; stock
//! and commodity gains/losses are accrued throughout the fiscal year and
//! settled at year-end (every 24 turns).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Per-entity accrued capital gains/losses for the current fiscal year.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EntityGainsAccrual {
    /// Realized gains from selling securities/commodities above cost basis.
    #[serde(default)]
    pub realized_gains: f64,
    /// Realized losses from selling securities/commodities below cost basis.
    #[serde(default)]
    pub realized_losses: f64,
    /// Losses carried forward from previous fiscal years (up to 5 years).
    /// These offset current-year gains before tax is computed.
    #[serde(default)]
    pub carried_forward_losses: f64,
    /// Remaining years the carried-forward losses can be used (decrements each year).
    #[serde(default)]
    pub carry_forward_years_left: u32,
}

/// v4.18 WS4: One entity's realized position in the last completed
/// fiscal-year settlement — the real per-entity ledger the CGT summary UI
/// reads. `tax_paid` is the cash actually debited post-settlement (it can
/// be lower than `tax_assessed` when the entity's account ran dry; the
/// difference is implicit arrears, and `Σ tax_paid` equals
/// `tax_collected_this_year` exactly).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EntityCgtSettlement {
    /// Gross realized gains booked in the settled year (pre-loss-offset).
    #[serde(default)]
    pub realized_gains: f64,
    /// Gross realized losses booked in the settled year.
    #[serde(default)]
    pub realized_losses: f64,
    /// Carried-forward losses consumed by this settlement.
    #[serde(default)]
    pub carried_forward_used: f64,
    /// Net taxable gain after losses and carry-forward (≥ 0 when taxed).
    #[serde(default)]
    pub net_taxable_gain: f64,
    /// Tax assessed at the CGT rate before collection.
    #[serde(default)]
    pub tax_assessed: f64,
    /// Tax actually collected in cash (filled by `record_entity_tax_payment`).
    #[serde(default)]
    pub tax_paid: f64,
    /// Losses carried forward into the next fiscal year after settlement.
    #[serde(default)]
    pub losses_carried_forward: f64,
}

/// Centralized capital gains tax registry.
///
/// Tracks per-entity accrued gains/losses throughout the fiscal year and
/// settles tax at year-end. Government bond coupons are taxed at source
/// (not tracked here); stock and commodity trades are tracked here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapitalGainsTaxRegistry {
    /// Per-entity accruals (entity_id → accrual).
    /// Entity IDs can be company IDs, fund IDs, or VIP IDs.
    #[serde(default)]
    pub accruals: BTreeMap<String, EntityGainsAccrual>,

    /// Capital gains tax rate for securities (e.g., 0.19 for 19%).
    #[serde(default = "default_securities_cgt_rate")]
    pub securities_cgt_rate: f64,

    /// Capital gains tax rate for commodities (e.g., 0.19 for 19%).
    #[serde(default = "default_commodity_cgt_rate")]
    pub commodity_cgt_rate: f64,

    /// Total tax collected this fiscal year (for reporting).
    #[serde(default)]
    pub tax_collected_this_year: f64,

    /// History of annual tax collections (for UI charts).
    #[serde(default)]
    pub annual_tax_history: Vec<f64>,

    /// v4.18 WS4: Fiscal year of each `annual_tax_history` entry (parallel
    /// array) — the chart's X axis. Years are appended by
    /// `finalize_year_settlement` at the year-end cadence.
    #[serde(default)]
    pub annual_tax_history_years: Vec<u32>,

    /// v4.18 WS4: Per-entity ledger of the most recent completed
    /// fiscal-year settlement (entity_id → settlement record).
    #[serde(default)]
    pub last_year_settlements: BTreeMap<String, EntityCgtSettlement>,

    /// v4.18 WS4: The fiscal year `last_year_settlements` covers.
    #[serde(default)]
    pub last_settled_year: Option<u32>,
}

impl Default for CapitalGainsTaxRegistry {
    fn default() -> Self {
        CapitalGainsTaxRegistry {
            accruals: BTreeMap::new(),
            securities_cgt_rate: default_securities_cgt_rate(),
            commodity_cgt_rate: default_commodity_cgt_rate(),
            tax_collected_this_year: 0.0,
            annual_tax_history: Vec::new(),
            annual_tax_history_years: Vec::new(),
            last_year_settlements: BTreeMap::new(),
            last_settled_year: None,
        }
    }
}

fn default_securities_cgt_rate() -> f64 {
    0.19 // 19% — standard Polish capital gains tax rate (Belka's tax)
}

fn default_commodity_cgt_rate() -> f64 {
    0.19 // 19% — same rate for commodities
}

impl CapitalGainsTaxRegistry {
    /// Record a realized gain or loss for an entity.
    ///
    /// # Arguments
    /// * `entity_id` - The entity realizing the gain/loss.
    /// * `gain_or_loss` - Positive = gain, negative = loss.
    ///
    /// # Rules
    /// * Gains are added to `realized_gains`.
    /// * Losses are added to `realized_losses` (stored as positive values).
    /// * Both are used at year-end for tax-loss harvesting.
    pub fn record_realized(&mut self, entity_id: &str, gain_or_loss: f64) {
        let accrual = self.accruals.entry(entity_id.to_string()).or_default();
        if gain_or_loss > 0.0 {
            accrual.realized_gains += gain_or_loss;
        } else if gain_or_loss < 0.0 {
            accrual.realized_losses += -gain_or_loss; // Store as positive
        }
    }

    /// Settle capital gains tax for all entities at fiscal year-end.
    ///
    /// # Arguments
    /// * `treasury` - Mutable treasury to receive tax payments (credited).
    /// * `brokerage_cash` - Closure that returns the current brokerage cash
    ///   balance for an entity and allows debiting it.
    ///
    /// # Rules (per user directive)
    /// * Net capital gains = realized_gains - realized_losses - carried_forward_losses.
    /// * If net > 0: tax = net × CGT rate, debited from entity, credited to treasury.
    /// * If net < 0: remaining losses carried forward (up to 5 years).
    /// * Carried-forward losses expire after 5 years.
    /// * Resets accruals for the new fiscal year.
    pub fn settle_year_end(
        &mut self,
        treasury_credit: &mut f64,
        mut debit_entity: impl FnMut(&str, f64) -> bool,
    ) -> f64 {
        let mut total_tax_collected = 0.0;
        let cgt_rate = self.securities_cgt_rate; // Use securities rate as default

        let entity_ids: Vec<String> = self.accruals.keys().cloned().collect();
        // v4.18 WS4: per-entity settlement ledger — rebuilt every fiscal
        // year-end so the UI can show the real breakdown behind the total.
        let mut settlements: BTreeMap<String, EntityCgtSettlement> = BTreeMap::new();

        for entity_id in entity_ids {
            let accrual = match self.accruals.get_mut(&entity_id) {
                Some(a) => a,
                None => continue,
            };

            // Snapshot the year's gross book before offsets so the ledger
            // shows what the entity actually realized, not just the net.
            let gross_gains = accrual.realized_gains;
            let gross_losses = accrual.realized_losses;
            let carry_in = accrual.carried_forward_losses;

            // Apply carried-forward losses first (tax-loss harvesting).
            let net_after_carry = if accrual.carried_forward_losses > 0.0 {
                let offset = accrual.carried_forward_losses.min(accrual.realized_gains);
                accrual.realized_gains -= offset;
                accrual.carried_forward_losses -= offset;
                // Remaining carried-forward losses expire if years left is 0.
                if accrual.carry_forward_years_left > 0 {
                    accrual.carry_forward_years_left -= 1;
                }
                if accrual.carry_forward_years_left == 0 {
                    accrual.carried_forward_losses = 0.0;
                }
                accrual.realized_gains - accrual.realized_losses
            } else {
                accrual.realized_gains - accrual.realized_losses
            };

            if net_after_carry > 0.0 {
                let tax = net_after_carry * cgt_rate;
                if debit_entity(&entity_id, tax) {
                    *treasury_credit += tax;
                    total_tax_collected += tax;
                }
                settlements.insert(
                    entity_id.clone(),
                    EntityCgtSettlement {
                        realized_gains: gross_gains,
                        realized_losses: gross_losses,
                        carried_forward_used: carry_in.min(gross_gains),
                        net_taxable_gain: net_after_carry,
                        tax_assessed: tax,
                        // Actual cash collected lands in the post-pass via
                        // `record_entity_tax_payment` (0.0 = not yet debited
                        // or the entity's account ran dry).
                        tax_paid: 0.0,
                        losses_carried_forward: 0.0,
                    },
                );
                // Reset for new year (gains and losses consumed).
                accrual.realized_gains = 0.0;
                accrual.realized_losses = 0.0;
                accrual.carried_forward_losses = 0.0;
                accrual.carry_forward_years_left = 0;
            } else if net_after_carry < 0.0 {
                // Net loss — carry forward to next year (up to 5 years).
                let carry_forward = -net_after_carry;
                accrual.carried_forward_losses = carry_forward;
                accrual.carry_forward_years_left = 5;
                accrual.realized_gains = 0.0;
                accrual.realized_losses = 0.0;
                settlements.insert(
                    entity_id.clone(),
                    EntityCgtSettlement {
                        realized_gains: gross_gains,
                        realized_losses: gross_losses,
                        carried_forward_used: carry_in.min(gross_gains),
                        net_taxable_gain: 0.0,
                        tax_assessed: 0.0,
                        tax_paid: 0.0,
                        losses_carried_forward: carry_forward,
                    },
                );
            } else {
                // Break even — reset.
                accrual.realized_gains = 0.0;
                accrual.realized_losses = 0.0;
                accrual.carried_forward_losses = 0.0;
                accrual.carry_forward_years_left = 0;
                settlements.insert(
                    entity_id.clone(),
                    EntityCgtSettlement {
                        realized_gains: gross_gains,
                        realized_losses: gross_losses,
                        carried_forward_used: carry_in.min(gross_gains),
                        net_taxable_gain: 0.0,
                        tax_assessed: 0.0,
                        tax_paid: 0.0,
                        losses_carried_forward: 0.0,
                    },
                );
            }
        }

        self.last_year_settlements = settlements;

        self.tax_collected_this_year = total_tax_collected;
        self.annual_tax_history.push(total_tax_collected);
        if self.annual_tax_history.len() > 60 {
            self.annual_tax_history.remove(0);
        }

        total_tax_collected
    }

    /// v4.18 WS4: Record the cash actually collected from an entity for the
    /// settlement just computed by `settle_year_end`. The turn loop debits
    /// entity accounts AFTER settlement (borrow-ck constraint), so the
    /// assessed amount and the paid amount can diverge when an entity's
    /// balance is short — the ledger keeps both figures.
    pub fn record_entity_tax_payment(&mut self, entity_id: &str, paid: f64) {
        if let Some(settlement) = self.last_year_settlements.get_mut(entity_id) {
            settlement.tax_paid = paid;
        }
    }

    /// v4.18 WS4: Finalize the just-completed settlement with the actual
    /// cash collected and the fiscal year it covers. Corrects the history
    /// entry that `settle_year_end` pushed at the assessed figure to the
    /// genuinely-collected figure, records the settlement year on the
    /// parallel X-axis array, and stores `last_settled_year`.
    ///
    /// `Σ last_year_settlements[*].tax_paid` equals `actual_collected`,
    /// so the summary table's Tax column always reconciles to the
    /// displayed Total Tax Collected.
    pub fn finalize_year_settlement(&mut self, actual_collected: f64, settled_year: u32) {
        self.tax_collected_this_year = actual_collected;
        if let Some(last) = self.annual_tax_history.last_mut() {
            *last = actual_collected;
        } else {
            self.annual_tax_history.push(actual_collected);
        }
        self.annual_tax_history_years.push(settled_year);
        // Keep the parallel arrays aligned (bounded like the history).
        while self.annual_tax_history_years.len() > self.annual_tax_history.len() {
            self.annual_tax_history_years.remove(0);
        }
        self.last_settled_year = Some(settled_year);
    }

    /// Check if an entity has accrued gains or losses.
    pub fn has_accruals(&self, entity_id: &str) -> bool {
        if let Some(a) = self.accruals.get(entity_id) {
            a.realized_gains > 0.0 || a.realized_losses > 0.0 || a.carried_forward_losses > 0.0
        } else {
            false
        }
    }

    /// Get the accrual for an entity (if any).
    pub fn get_accrual(&self, entity_id: &str) -> Option<&EntityGainsAccrual> {
        self.accruals.get(entity_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_realized_gain() {
        let mut registry = CapitalGainsTaxRegistry::default();
        registry.record_realized("COMP-001", 1000.0);
        let accrual = registry.get_accrual("COMP-001").unwrap();
        assert!((accrual.realized_gains - 1000.0).abs() < 1e-6);
        assert!((accrual.realized_losses - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_record_realized_loss() {
        let mut registry = CapitalGainsTaxRegistry::default();
        registry.record_realized("COMP-001", -500.0);
        let accrual = registry.get_accrual("COMP-001").unwrap();
        assert!((accrual.realized_gains - 0.0).abs() < 1e-6);
        assert!((accrual.realized_losses - 500.0).abs() < 1e-6);
    }

    #[test]
    fn test_year_end_settle_with_gains() {
        let mut registry = CapitalGainsTaxRegistry::default();
        registry.record_realized("COMP-001", 1000.0);
        let mut treasury = 0.0;
        let tax = registry.settle_year_end(&mut treasury, |_, amount| {
            // Simulate successful debit
            let _ = amount;
            true
        });
        // Tax = 1000 * 0.19 = 190
        assert!((tax - 190.0).abs() < 1e-6);
        assert!((treasury - 190.0).abs() < 1e-6);
    }

    #[test]
    fn test_year_end_settle_with_losses_carried_forward() {
        let mut registry = CapitalGainsTaxRegistry::default();
        registry.record_realized("COMP-001", -500.0);
        let mut treasury = 0.0;
        let tax = registry.settle_year_end(&mut treasury, |_, _| true);
        assert!((tax - 0.0).abs() < 1e-6);
        let accrual = registry.get_accrual("COMP-001").unwrap();
        assert!((accrual.carried_forward_losses - 500.0).abs() < 1e-6);
        assert_eq!(accrual.carry_forward_years_left, 5);
    }

    #[test]
    fn test_tax_loss_harvesting() {
        let mut registry = CapitalGainsTaxRegistry::default();
        // Year 1: Loss of 500
        registry.record_realized("COMP-001", -500.0);
        let mut treasury = 0.0;
        registry.settle_year_end(&mut treasury, |_, _| true);

        // Year 2: Gain of 800 — should offset 500 from carry-forward
        registry.record_realized("COMP-001", 800.0);
        let tax = registry.settle_year_end(&mut treasury, |_, _| true);
        // Net gain = 800 - 500 = 300, tax = 300 * 0.19 = 57
        assert!((tax - 57.0).abs() < 1e-6);
    }

    #[test]
    fn test_default_cgt_rate() {
        let registry = CapitalGainsTaxRegistry::default();
        assert!((registry.securities_cgt_rate - 0.19).abs() < 1e-6);
        assert!((registry.commodity_cgt_rate - 0.19).abs() < 1e-6);
    }
}
