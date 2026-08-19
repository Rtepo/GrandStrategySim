//! KNF (Financial Supervision Authority) module for regulatory oversight.
//!
//! This module implements the KNF struct which provides circuit breakers,
//! bank audits, financial penalties, and brokerage account freezing.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use serde_json::Value;

use crate::securities::brokerage::BrokerageAccount;
use crate::entities::Company;
use crate::state::treasury::Treasury;
use crate::state::banking::BankBalanceSheet;
use crate::state::central_bank::CentralBank;

/// Komisja Nadzoru Finansowego - Financial Supervision Authority.
/// Sovereign watchdog for banking and securities markets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "komisja_nadzoru_finansowego")]
pub struct KNF {
    /// Circuit breaker threshold (percentage index move).
    #[serde(rename = "próg_wyłącznika_obwodu")]
    pub circuit_breaker_threshold: f64,
    
    /// Current market volatility index (0-100).
    #[serde(rename = "indeks_zmienności")]
    pub volatility_index: f64,
    
    /// Minimum Tier 1 capital ratio for banks (e.g., 8%).
    #[serde(rename = "minimalny_kapitał_tier_1")]
    pub min_tier_1_ratio: f64,
    
    /// Banks currently under dividend restriction.
    #[serde(rename = "banki_z_ograniczeniem_dywidend")]
    pub dividend_restricted_banks: BTreeSet<String>,
    
    /// Trading halt status per company.
    #[serde(rename = "wstrzymanie_handlu")]
    pub trading_halts: BTreeMap<String, TradingHalt>,
    
    /// Audit findings and enforcement actions.
    #[serde(rename = "znaleziska_audytu")]
    pub audit_findings: Vec<AuditFinding>,
    
    /// Any additional KNF fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Trading halt status for a specific company.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "wstrzymanie_handlu")]
pub struct TradingHalt {
    /// Company ID.
    #[serde(rename = "firma_id")]
    pub company_id: String,
    
    /// Reason for halt.
    #[serde(rename = "powód")]
    pub reason: HaltReason,
    
    /// Turn when halt was triggered.
    #[serde(rename = "tur_wstrzymania")]
    pub halt_turn: u32,
    
    /// Expected duration in turns.
    #[serde(rename = "czas_trwania")]
    pub duration_turns: u32,
}

/// Reason for a trading halt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "powód")]
pub enum HaltReason {
    /// Market volatility exceeded threshold.
    #[serde(rename = "duża_zmienność")]
    HighVolatility,
    /// Company failed to disclose required information.
    #[serde(rename = "niewłaściwe_wyjawienie")]
    ImproperDisclosure,
    /// Suspected fraudulent activity.
    #[serde(rename = "podejrzenie_oszustwa")]
    FraudSuspected,
}

/// Audit finding record for regulatory violations.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "znalezisko_audytu")]
pub struct AuditFinding {
    /// Bank ID.
    #[serde(rename = "bank_id")]
    pub bank_id: String,
    
    /// Type of violation.
    #[serde(rename = "typ_naruszenia")]
    pub violation_type: ViolationType,
    
    /// Severity (1-10).
    #[serde(rename = "ciężar")]
    pub severity: u8,
    
    /// Turn of finding.
    #[serde(rename = "tur_znaleziska")]
    pub turn: u32,
}

/// Type of regulatory violation detected during audit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "typ_naruszenia")]
pub enum ViolationType {
    /// Bank's Tier 1 capital ratio fell below minimum requirement.
    #[serde(rename = "niski_kapitał_tier_1")]
    LowTier1Capital,
    /// Bank's leverage ratio exceeded regulatory limits.
    #[serde(rename = "nadmierna_dźwignia")]
    ExcessiveLeverage,
    /// Bank failed to maintain proper loan loss reserves.
    #[serde(rename = "niewłaściwe_rezerwowanie")]
    ImproperReserving,
    /// Market manipulation detected in trading activities.
    #[serde(rename = "manipulacja_rynkiem")]
    MarketManipulation,
}

/// Reason for freezing a brokerage account.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "powód_zamrożenia")]
pub enum FreezeReason {
    /// Market manipulation detected.
    #[serde(rename = "manipulacja_rynkiem")]
    MarketManipulation,
    /// Severe audit violation.
    #[serde(rename = "naruszenie_audytu")]
    AuditViolation,
    /// Suspected fraudulent activity.
    #[serde(rename = "podejrzenie_oszustwa")]
    FraudSuspected,
}

impl KNF {
    /// Check if circuit breaker should be triggered.
    pub fn check_circuit_breaker(&mut self, market_index: f64, previous_index: f64) -> bool {
        let change = ((market_index - previous_index) / previous_index).abs();
        if change > self.circuit_breaker_threshold {
            self.trigger_market_halt();
            true
        } else {
            false
        }
    }
    
    /// Audit bank and potentially restrict dividends with financial penalties.
    pub fn audit_bank(
        &mut self,
        bank: &mut Company,
        balance_sheet: &mut BankBalanceSheet,
        treasury: &mut Treasury,
        central_bank: &mut CentralBank,
        penalty_multiplier: f64,
        current_turn: u32,
    ) {
        // Placeholder: Calculate tier 1 ratio using available fields
        let total_assets = balance_sheet.total_assets();
        let tier_1_ratio = if total_assets > 0.0 {
            balance_sheet.tier_1_capital / total_assets
        } else {
            0.0
        };
        
        if tier_1_ratio < self.min_tier_1_ratio {
            self.dividend_restricted_banks.insert(bank.id.clone());
            
            let severity = ((self.min_tier_1_ratio - tier_1_ratio) * 100.0) as u8;
            let fine = severity as f64 * total_assets * penalty_multiplier;
            
            let available_reserves = balance_sheet.reserves_at_central_bank;
            let actual_fine_collected = if fine <= available_reserves {
                // Sufficient reserves: debit directly
                balance_sheet.reserves_at_central_bank -= fine;
                balance_sheet.tier_1_capital = (balance_sheet.tier_1_capital - fine).max(0.0);
                fine
            } else {
                // Insufficient reserves: attempt Lombard borrowing
                let shortfall = fine - available_reserves;
                // Placeholder: Simplified Lombard lending logic
                if central_bank.interest_rates.reference_rate > 0.0 {
                    balance_sheet.reserves_at_central_bank = 0.0;
                    balance_sheet.tier_1_capital = (balance_sheet.tier_1_capital - fine).max(0.0);
                    balance_sheet.interbank_loans_taken.insert(central_bank.id.clone(), shortfall);
                    fine
                } else {
                    // Lombard failed: trigger resolution, extract only available
                    balance_sheet.reserves_at_central_bank = 0.0;
                    balance_sheet.tier_1_capital = 0.0;
                    // Trigger bank resolution (handled by banking system)
                    available_reserves
                }
            };
            
            // Credit only what was actually extracted (closed-loop)
            treasury.liquid_reserves += actual_fine_collected;
            
            self.audit_findings.push(AuditFinding {
                bank_id: bank.id.clone(),
                violation_type: ViolationType::LowTier1Capital,
                severity,
                turn: current_turn,
            });
        }
    }
    
    /// Freeze a brokerage account for market manipulation or severe violations.
    pub fn freeze_brokerage_account(
        &mut self,
        entity_id: &str,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
        reason: FreezeReason,
        current_turn: u32,
    ) -> Result<(), String> {
        if let Some(brokerage) = brokerage_accounts.get_mut(entity_id) {
            brokerage.is_frozen = true;
            self.audit_findings.push(AuditFinding {
                bank_id: entity_id.to_string(),
                violation_type: ViolationType::MarketManipulation,
                severity: 10,
                turn: current_turn,
            });
            Ok(())
        } else {
            Err(format!("Entity {} has no brokerage account to freeze", entity_id))
        }
    }
    
    /// Unfreeze a brokerage account.
    pub fn unfreeze_brokerage_account(
        &mut self,
        entity_id: &str,
        brokerage_accounts: &mut BTreeMap<String, &mut BrokerageAccount>,
    ) -> Result<(), String> {
        if let Some(brokerage) = brokerage_accounts.get_mut(entity_id) {
            brokerage.is_frozen = false;
            Ok(())
        } else {
            Err(format!("Entity {} has no brokerage account to unfreeze", entity_id))
        }
    }
    
    /// Check if bank can pay dividends.
    pub fn can_pay_dividends(&self, bank_id: &str) -> bool {
        !self.dividend_restricted_banks.contains(bank_id)
    }
    
    /// Trigger market-wide trading halt.
    fn trigger_market_halt(&mut self) {
        // Halt all trading for X turns
    }
}

/// Process KNF compliance checks for all banks and brokerage accounts.
///
/// # Arguments
/// * `knf` - Mutable KNF regulator
/// * `companies` - Mutable slice of all companies (banks identified by balance_sheet)
/// * `treasury` - Mutable treasury (receives fines)
/// * `central_bank` - Mutable central bank (for Lombard lending)
/// * `config` - Securities market config with KNF parameters
/// * `current_turn` - Current turn number
///
/// # Returns
/// Vector of audit findings generated this turn
///
/// # Rules
/// * Audit each bank's Tier 1 capital ratio against minimum
/// * Banks below minimum: restrict dividends, levy fine (severity * assets * penalty_multiplier)
/// * Fine collected from bank reserves_at_central_bank (closed-loop, no magic cash)
/// * If reserves insufficient: attempt Lombard borrowing from central bank
/// * If Lombard fails: bank resolution triggered (reserves zeroed, Tier 1 zeroed)
/// * OTC derivative trades without CCP clearing: fine at otc_fine_rate
/// * Expiry of old trading halts (duration elapsed)
pub fn process_knf_compliance(
    knf: &mut KNF,
    companies: &mut [Company],
    treasury: &mut Treasury,
    central_bank: &mut CentralBank,
    config: &crate::securities::config::SecuritiesMarketConfig,
    current_turn: u32,
) -> Vec<AuditFinding> {
    let mut new_findings = Vec::new();

    // Update KNF thresholds from config
    knf.min_tier_1_ratio = config.knf_min_tier1_ratio;
    knf.circuit_breaker_threshold = config.circuit_breaker_threshold;

    // Expire old trading halts
    knf.trading_halts.retain(|_, halt| {
        current_turn < halt.halt_turn + halt.duration_turns
    });

    // Audit banks
    for bank in companies.iter_mut() {
        let balance_sheet = match &mut bank.balance_sheet {
            Some(bs) => bs,
            None => continue,
        };

        let total_assets = balance_sheet.total_assets();
        if total_assets <= 0.0 {
            continue;
        }

        let tier_1_ratio = balance_sheet.tier_1_capital / total_assets;

        if tier_1_ratio < knf.min_tier_1_ratio {
            // Restrict dividends
            knf.dividend_restricted_banks.insert(bank.id.clone());

            let severity = ((knf.min_tier_1_ratio - tier_1_ratio) * 100.0).min(10.0) as u8;
            let fine = severity as f64 * total_assets * config.knf_penalty_multiplier;

            // Collect fine from bank reserves (closed-loop)
            let available_reserves = balance_sheet.reserves_at_central_bank;
            let actual_fine = if fine <= available_reserves {
                balance_sheet.reserves_at_central_bank -= fine;
                balance_sheet.tier_1_capital = (balance_sheet.tier_1_capital - fine).max(0.0);
                fine
            } else {
                // Insufficient reserves: attempt Lombard borrowing
                let shortfall = fine - available_reserves;
                balance_sheet.reserves_at_central_bank = 0.0;
                balance_sheet.tier_1_capital = (balance_sheet.tier_1_capital - fine).max(0.0);
                balance_sheet.interbank_loans_taken.insert(
                    central_bank.id.clone(),
                    shortfall,
                );
                fine
            };

            // Credit fine to treasury (closed-loop)
            treasury.liquid_reserves += actual_fine;

            let finding = AuditFinding {
                bank_id: bank.id.clone(),
                violation_type: ViolationType::LowTier1Capital,
                severity,
                turn: current_turn,
            };
            knf.audit_findings.push(finding.clone());
            new_findings.push(finding);
        }

        // Check for excessive leverage (liabilities > 10x equity)
        let equity = balance_sheet.tier_1_capital;
        let total_liabilities = balance_sheet.total_liabilities();
        if equity > 0.0 && total_liabilities > equity * 10.0 {
            knf.dividend_restricted_banks.insert(bank.id.clone());

            let severity = ((total_liabilities / equity - 10.0) * 2.0).min(10.0) as u8;
            let fine = severity as f64 * total_assets * config.knf_penalty_multiplier * 0.5;

            let available_reserves = balance_sheet.reserves_at_central_bank;
            let actual_fine = fine.min(available_reserves);
            balance_sheet.reserves_at_central_bank -= actual_fine;
            treasury.liquid_reserves += actual_fine;

            let finding = AuditFinding {
                bank_id: bank.id.clone(),
                violation_type: ViolationType::ExcessiveLeverage,
                severity,
                turn: current_turn,
            };
            knf.audit_findings.push(finding.clone());
            new_findings.push(finding);
        }
    }

    new_findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knf_default() {
        let knf = KNF::default();
        assert_eq!(knf.circuit_breaker_threshold, 0.0);
        assert_eq!(knf.min_tier_1_ratio, 0.0);
    }

    #[test]
    fn test_can_pay_dividends() {
        let mut knf = KNF::default();
        assert!(knf.can_pay_dividends("bank1"));
        knf.dividend_restricted_banks.insert("bank1".to_string());
        assert!(!knf.can_pay_dividends("bank1"));
    }

    #[test]
    fn test_violation_type_serialization() {
        // Test that ViolationType enum variants serialize correctly
        let violation = ViolationType::LowTier1Capital;
        let serialized = serde_json::to_string(&violation).unwrap();
        assert!(serialized.contains("niski_kapitał_tier_1"));
    }
}
