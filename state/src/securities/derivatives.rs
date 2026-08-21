//! Derivatives module for advanced financial instruments.
//!
//! This module implements Phase D.5 derivative structures:
//! - Credit Default Swaps (CDS)
//! - Futures Contracts
//! - Clearing methods (OTC vs CCP)

use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Reference entity for CDS contracts.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum ReferenceEntity {
    /// Corporate entity.

    Company { 
        /// Company identifier.
        company_id: String 
    },
    /// Bank entity.

    Bank { 
        /// Bank identifier.
        bank_id: String 
    },
    /// Sovereign country.

    Country { 
        /// Country identifier.
        country_id: String 
    },
}

impl Default for ReferenceEntity {
    fn default() -> Self {
        ReferenceEntity::Company { company_id: String::new() }
    }
}

/// Credit Default Swap - Insurance against default.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct CreditDefaultSwap {
    /// CDS ID.
    #[serde(default)]
    pub id: String,
    
    /// Protection buyer (pays premium).
    #[serde(default)]
    pub protection_buyer_id: String,
    
    /// Protection seller (receives premium, pays on default).
    #[serde(default)]
    pub protection_seller_id: String,
    
    /// Reference entity (what we're insuring against).
    #[serde(default)]
    pub reference_entity: ReferenceEntity,
    
    /// Notional value (exposure amount).
    #[serde(default)]
    pub notional: f64,
    
    /// Premium rate (annualized, e.g., 0.02 for 2%).
    #[serde(default)]
    pub premium_rate: f64,
    
    /// Clearing method (OTC or CCP).
    #[serde(default)]
    pub clearing_method: ClearingMethod,
    
    /// Current mark-to-market value.
    #[serde(default)]
    pub market_value: f64,
    
    /// Any additional CDS fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Underlying asset for Futures contracts.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum FuturesUnderlying {
    /// Physical commodity (e.g., Oil, Wheat).

    Commodity { 
        /// Commodity identifier.
        commodity_id: String 
    },
    /// Interest rate (XIBOR).

    InterestRate { 
        /// Benchmark rate identifier.
        benchmark: String 
    },
}

impl Default for FuturesUnderlying {
    fn default() -> Self {
        FuturesUnderlying::Commodity { commodity_id: String::new() }
    }
}

/// Futures position type.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]

pub enum FuturesPosition {
    /// Long position (buyer - profits from price increase).

    Long,
    /// Short position (seller - profits from price decrease).

    Short,
}

impl Default for FuturesPosition {
    fn default() -> Self {
        FuturesPosition::Long
    }
}

/// Clearing method for derivatives.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]

pub enum ClearingMethod {
    /// Over-The-Counter: Direct P2P, no margin enforcement.

    OTC,
    /// Central Counterparty: Strict margin enforcement.

    CCP,
}

impl Default for ClearingMethod {
    fn default() -> Self {
        ClearingMethod::OTC
    }
}

/// Futures Contract - Obligation to buy/sell at future price.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct FuturesContract {
    /// Futures ID.
    #[serde(default)]
    pub id: String,
    
    /// Long position (buyer) or Short position (seller).
    #[serde(default)]
    pub position: FuturesPosition,
    
    /// Owner of the position.
    #[serde(default)]
    pub owner_id: String,
    
    /// Counterparty (for OTC) or CCP (for cleared).
    #[serde(default)]
    pub counterparty_id: String,
    
    /// Underlying asset.
    #[serde(default)]
    pub underlying: FuturesUnderlying,
    
    /// Contract size (units of underlying).
    #[serde(default)]
    pub contract_size: f64,
    
    /// Strike price (agreed future price).
    #[serde(default)]
    pub strike_price: f64,
    
    /// Current market price of underlying.
    #[serde(default)]
    pub current_price: f64,
    
    /// Maturity turn.
    #[serde(default)]
    pub maturity_turn: u32,
    
    /// Clearing method (OTC or CCP).
    #[serde(default)]
    pub clearing_method: ClearingMethod,
    
    /// Unrealized P&L.
    #[serde(default)]
    pub unrealized_pnl: f64,
    
    /// Any additional futures fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl FuturesContract {
    /// Calculate unrealized P&L based on current market price.
    ///
    /// # Rules
    /// - Long: P&L = (current_price - strike_price) * contract_size
    /// - Short: P&L = (strike_price - current_price) * contract_size
    pub fn calculate_unrealized_pnl(&self) -> f64 {
        match self.position {
            FuturesPosition::Long => {
                (self.current_price - self.strike_price) * self.contract_size
            }
            FuturesPosition::Short => {
                (self.strike_price - self.current_price) * self.contract_size
            }
        }
    }
}

/// Process CDS premium payments: buyer pays seller.
///
/// # Arguments
/// * `cds_contracts` - Mutable slice of all CDS contracts
/// * `companies` - Mutable slice of all companies (for buyer debit and seller credit)
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Premium = notional * premium_rate (per turn)
/// * Protection buyer is DEBITED (brokerage cash -= premium)
/// * Protection seller is CREDITED (brokerage cash += premium)
/// * If buyer cannot pay, CDS is cancelled (no free insurance)
/// * OTC contracts: direct P2P, no CCP margin check
/// * CCP-cleared contracts: margin handled by CCP module
pub fn process_cds_premiums(
    cds_contracts: &mut [CreditDefaultSwap],
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) {
    for cds in cds_contracts.iter_mut() {
        let premium = cds.notional * cds.premium_rate;
        if premium <= 0.0 {
            continue;
        }

        let buyer_id = cds.protection_buyer_id.clone();
        let seller_id = cds.protection_seller_id.clone();

        // Debit buyer
        let mut paid = false;
        if let Some(buyer) = companies.iter_mut().find(|c| c.id == buyer_id) {
            if let Some(ref mut acct) = buyer.brokerage_account {
                if acct.cash >= premium {
                    acct.cash -= premium;
                    paid = true;
                }
            }
        }

        // Credit seller only if buyer paid
        if paid {
            if let Some(seller) = companies.iter_mut().find(|c| c.id == seller_id) {
                if let Some(ref mut acct) = seller.brokerage_account {
                    acct.cash += premium;
                }
            }
        }
    }
}

/// Process futures mark-to-market: settle unrealized P&L via CCP variation margin.
///
/// # Arguments
/// * `futures_contracts` - Mutable slice of all futures contracts
/// * `companies` - Mutable slice of all companies (for margin settlement)
/// * `central_counterparty` - Mutable CCP (for margin handling)
/// * `current_turn` - Current turn number
///
/// # Rules
/// * For each contract: calculate unrealized P&L
/// * Long profits when price rises, Short profits when price falls
/// * CCP-cleared: variation margin transferred through CCP
/// * OTC: direct P2P settlement (if counterparty can pay)
/// * Update unrealized_pnl field on contract
/// * At maturity: contract settled and removed
pub fn process_futures_mark_to_market(
    futures_contracts: &mut Vec<FuturesContract>,
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) {
    let mut matured_indices = Vec::new();

    for (idx, contract) in futures_contracts.iter_mut().enumerate() {
        let pnl = contract.calculate_unrealized_pnl();
        contract.unrealized_pnl = pnl;

        if contract.clearing_method == ClearingMethod::CCP {
            // CCP-cleared: transfer variation margin
            let owner_id = contract.owner_id.clone();
            let counterparty_id = contract.counterparty_id.clone();

            if pnl > 0.0 {
                // Owner gains, counterparty loses
                if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                    if let Some(ref mut acct) = owner.brokerage_account {
                        acct.cash += pnl;
                    }
                }
                if let Some(counterparty) = companies.iter_mut().find(|c| c.id == counterparty_id) {
                    if let Some(ref mut acct) = counterparty.brokerage_account {
                        acct.cash = (acct.cash - pnl).max(0.0);
                    }
                }
            } else if pnl < 0.0 {
                // Owner loses, counterparty gains
                let loss = -pnl;
                if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                    if let Some(ref mut acct) = owner.brokerage_account {
                        acct.cash = (acct.cash - loss).max(0.0);
                    }
                }
                if let Some(counterparty) = companies.iter_mut().find(|c| c.id == counterparty_id) {
                    if let Some(ref mut acct) = counterparty.brokerage_account {
                        acct.cash += loss;
                    }
                }
            }
        } else {
            // OTC: direct settlement (may fail if counterparty can't pay)
            let owner_id = contract.owner_id.clone();
            let counterparty_id = contract.counterparty_id.clone();

            if pnl > 0.0 {
                if let Some(counterparty) = companies.iter_mut().find(|c| c.id == counterparty_id) {
                    if let Some(ref mut acct) = counterparty.brokerage_account {
                        if acct.cash >= pnl {
                            acct.cash -= pnl;
                            if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                                if let Some(ref mut owner_acct) = owner.brokerage_account {
                                    owner_acct.cash += pnl;
                                }
                            }
                        }
                    }
                }
            } else if pnl < 0.0 {
                let loss = -pnl;
                if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                    if let Some(ref mut acct) = owner.brokerage_account {
                        if acct.cash >= loss {
                            acct.cash -= loss;
                            if let Some(counterparty) = companies.iter_mut().find(|c| c.id == counterparty_id) {
                                if let Some(ref mut cp_acct) = counterparty.brokerage_account {
                                    cp_acct.cash += loss;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check maturity
        if current_turn >= contract.maturity_turn {
            matured_indices.push(idx);
        }
    }

    // Remove matured contracts
    for idx in matured_indices.into_iter().rev() {
        futures_contracts.remove(idx);
    }
}
