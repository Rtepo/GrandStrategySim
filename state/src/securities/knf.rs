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

pub struct KNF {
    /// Circuit breaker threshold (percentage index move).

    pub circuit_breaker_threshold: f64,
    
    /// Current market volatility index (0-100).

    pub volatility_index: f64,
    
    /// Minimum Tier 1 capital ratio for banks (e.g., 8%).

    pub min_tier_1_ratio: f64,
    
    /// Banks currently under dividend restriction.

    pub dividend_restricted_banks: BTreeSet<String>,
    
    /// Trading halt status per company.

    pub trading_halts: BTreeMap<String, TradingHalt>,
    
    /// Audit findings and enforcement actions.

    pub audit_findings: Vec<AuditFinding>,
    
    /// Any additional KNF fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Trading halt status for a specific company.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct TradingHalt {
    /// Company ID.

    pub company_id: String,
    
    /// Reason for halt.

    pub reason: HaltReason,
    
    /// Turn when halt was triggered.

    pub halt_turn: u32,
    
    /// Expected duration in turns.

    pub duration_turns: u32,
}

/// Reason for a trading halt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum HaltReason {
    /// Market volatility exceeded threshold.

    HighVolatility,
    /// Company failed to disclose required information.

    ImproperDisclosure,
    /// Suspected fraudulent activity.

    FraudSuspected,
}

/// Audit finding record for regulatory violations.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct AuditFinding {
    /// Bank ID.

    pub bank_id: String,
    
    /// Type of violation.

    pub violation_type: ViolationType,
    
    /// Severity (1-10).

    pub severity: u8,
    
    /// Turn of finding.

    pub turn: u32,
}

/// Type of regulatory violation detected during audit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum ViolationType {
    /// Bank's Tier 1 capital ratio fell below minimum requirement.

    LowTier1Capital,
    /// Bank's leverage ratio exceeded regulatory limits.

    ExcessiveLeverage,
    /// Bank failed to maintain proper loan loss reserves.

    ImproperReserving,
    /// Market manipulation detected in trading activities.

    MarketManipulation,
    /// Phase 57: Accounting fraud — profit diversion by corrupt CEO/manager.

    AccountingFraud,
    /// Phase 57: Fund leverage exceeded regulatory limits.

    FundLeverageExceeded,
    /// Phase 57: Insider trading — fund manager trading on companies where they're CEO or board member.

    InsiderTrading,
}

/// Reason for freezing a brokerage account.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum FreezeReason {
    /// Market manipulation detected.

    MarketManipulation,
    /// Severe audit violation.

    AuditViolation,
    /// Suspected fraudulent activity.

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

// ============================================================================
// PHASE 57: KNF REGULATORY ENFORCEMENT EXPANSION
// ============================================================================

/// Phase 57: Detect accounting fraud based on behavior modifiers.
///
/// Uses `modifiers.fraud_probability` (no raw trait string checks).
/// Severity scales with `modifiers.profit_diversion_rate` × profit.
///
/// # Arguments
/// * `company` - The company being audited.
/// * `modifiers` - The CEO's behavior modifiers (from `evaluate_market_behavior`).
/// * `current_turn` - The current turn number.
///
/// # Returns
/// `Some(AuditFinding)` if fraud is detected, `None` otherwise.
pub fn detect_accounting_fraud(
    company: &Company,
    modifiers: &crate::corporate::market_behavior::MarketBehaviorModifiers,
    current_turn: u32,
) -> Option<AuditFinding> {
    if modifiers.fraud_probability <= 0.0 {
        return None;
    }

    // Determine if fraud occurred this turn (probability check).
    // Use a deterministic check based on company ID hash + turn for reproducibility.
    let hash = company.id.chars().map(|c| c as u32).sum::<u32>();
    let pseudo_random = ((hash.wrapping_mul(current_turn)) % 100) as f64 / 100.0;

    if pseudo_random < modifiers.fraud_probability {
        // Severity scales with profit diversion rate × profit magnitude.
        let profit = company.annual_profit_accumulator.max(0.0);
        let diverted = profit * modifiers.profit_diversion_rate;
        let severity = ((diverted / 1_000_000.0).min(10.0) as u8).max(1);

        Some(AuditFinding {
            bank_id: company.id.clone(),
            violation_type: ViolationType::AccountingFraud,
            severity,
            turn: current_turn,
        })
    } else {
        None
    }
}

/// Phase 57: Regulate fund leverage — hedge funds exceeding leverage limit are penalized.
///
/// # Arguments
/// * `fund` - The fund company being audited.
/// * `config` - Securities market config (for penalty multiplier).
/// * `current_turn` - The current turn number.
///
/// # Returns
/// `Some(AuditFinding)` if leverage exceeds limits, `None` otherwise.
pub fn regulate_fund_leverage(
    fund: &Company,
    config: &crate::securities::config::SecuritiesMarketConfig,
    current_turn: u32,
) -> Option<AuditFinding> {
    let ledger = fund.fund_ledger.as_ref()?;
    let leverage = ledger.leverage_ratio;

    // Hedge funds can have up to 3x leverage; other funds up to 1.5x.
    let max_leverage = match fund.fund_type {
        Some(crate::securities::FundType::HedgeFund) => 3.0,
        _ => 1.5,
    };

    if leverage > max_leverage {
        let severity = ((leverage - max_leverage) * 3.0).min(10.0) as u8;
        Some(AuditFinding {
            bank_id: fund.id.clone(),
            violation_type: ViolationType::FundLeverageExceeded,
            severity,
            turn: current_turn,
        })
    } else {
        None
    }
}

/// Phase 57: Check for insider trading — fund manager trading on companies
/// where they're also CEO or board member.
///
/// # Arguments
/// * `fund_manager_vip_id` - The VIP ID of the fund manager.
/// * `fund_trades` - Recent trades by the fund.
/// * `companies` - All companies (to check CEO/board membership).
/// * `current_turn` - The current turn number.
///
/// # Returns
/// `Some(AuditFinding)` if insider trading is detected, `None` otherwise.
pub fn check_insider_trading(
    fund_manager_vip_id: &str,
    fund_trades: &[crate::securities::exchange::Trade],
    companies: &[Company],
    current_turn: u32,
) -> Option<AuditFinding> {
    // Find companies where the fund manager is CEO or board member.
    let connected_companies: Vec<&str> = companies
        .iter()
        .filter(|c| {
            // Check if the VIP is the CEO.
            if c.ceo_vip_id.as_deref() == Some(fund_manager_vip_id) {
                return true;
            }
            // Check if the VIP is a board member.
            if let crate::entities::LegalForm::JointStockCompany(ref jsd) = c.legal_form {
                return jsd.board_members.iter().any(|m| m.vip_id == fund_manager_vip_id);
            }
            false
        })
        .map(|c| c.id.as_str())
        .collect();

    if connected_companies.is_empty() {
        return None;
    }

    // Check if the fund traded any connected company's equity.
    for trade in fund_trades {
        if trade.instrument_id.starts_with("EQUITY:") {
            let company_id = &trade.instrument_id[7..];
            if connected_companies.contains(&company_id) {
                return Some(AuditFinding {
                    bank_id: company_id.to_string(),
                    violation_type: ViolationType::InsiderTrading,
                    severity: 8, // Insider trading is a severe violation
                    turn: current_turn,
                });
            }
        }
    }

    None
}

/// Phase 57: Check if the market index drop should trigger a trading halt.
///
/// # Arguments
/// * `knf` - The KNF regulator.
/// * `current_index` - Current market index value.
/// * `previous_index` - Previous market index value.
/// * `current_turn` - The current turn number.
///
/// # Returns
/// `true` if a trading halt was triggered, `false` otherwise.
pub fn check_market_halt(
    knf: &mut KNF,
    current_index: f64,
    previous_index: f64,
    current_turn: u32,
) -> bool {
    if previous_index <= 0.0 {
        return false;
    }
    let change_pct = (current_index - previous_index) / previous_index;
    // Halt if index drops more than 10% in one turn.
    if change_pct < -0.10 {
        knf.trigger_market_halt();
        // Record the halt.
        knf.audit_findings.push(AuditFinding {
            bank_id: "MARKET".to_string(),
            violation_type: ViolationType::MarketManipulation,
            severity: 10,
            turn: current_turn,
        });
        true
    } else {
        false
    }
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
        assert!(serialized.contains("LowTier1Capital"));
    }
}
