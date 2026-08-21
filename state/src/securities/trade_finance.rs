//! Trade finance module for Bills of Lading and working capital loans.
//!
//! This module implements BillOfLading for maritime cargo collateral and
//! WorkingCapitalLoan for short-term financing backed by trade documents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use serde_json::Value;

use crate::registries::enums::Commodity;

/// Bill of Lading - tradable receipt for maritime cargo in transit.
/// Acts as collateral for short-term working capital loans.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct BillOfLading {
    /// Unique bill ID.

    pub id: String,
    
    /// Physical shipment ID this bill represents.

    pub shipment_id: String,
    
    /// Owner of the bill (current holder).

    pub owner_id: String,
    
    /// Commodity type being shipped.

    pub commodity: Commodity,
    
    /// Quantity of commodity.

    pub quantity: f64,
    
    /// Declared value of cargo.

    pub declared_value: f64,
    
    /// Port of origin.

    pub port_of_origin: String,
    
    /// Port of destination.

    pub port_of_destination: String,
    
    /// Expected arrival turn.

    pub expected_arrival_turn: u32,
    
    /// Current status.

    pub status: BillStatus,
    
    /// Collateral value (for loan purposes).

    pub collateral_value: f64,
    
    /// Any additional bill fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

impl Default for BillOfLading {
    fn default() -> Self {
        BillOfLading {
            id: String::new(),
            shipment_id: String::new(),
            owner_id: String::new(),
            commodity: Commodity::Agd,
            quantity: 0.0,
            declared_value: 0.0,
            port_of_origin: String::new(),
            port_of_destination: String::new(),
            expected_arrival_turn: 0,
            status: BillStatus::InTransit,
            collateral_value: 0.0,
            extra: HashMap::new(),
        }
    }
}

/// Status of a Bill of Lading document.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum BillStatus {
    /// Cargo is currently in transit.

    InTransit,
    /// Cargo has been delivered to destination.

    Delivered,
    /// Bill is currently pledged as collateral for a loan.

    PledgedAsCollateral,
    /// Bill has expired (past maturity).

    Expired,
}

/// Short-term working capital loan backed by Bill of Lading.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct WorkingCapitalLoan {
    /// Loan ID.

    pub id: String,
    
    /// Borrower ID.

    pub borrower_id: String,
    
    /// Lender ID (bank).

    pub lender_id: String,
    
    /// Principal amount.

    pub principal: f64,
    
    /// Interest rate.

    pub interest_rate: f64,
    
    /// Collateral Bill of Lading ID.

    pub collateral_bill_id: String,
    
    /// Maturity turn (must be before cargo arrival).

    pub maturity_turn: u32,
    
    /// Any additional loan fields.
    #[serde(flatten, default)]
    pub extra: HashMap<String, Value>,
}

/// Placeholder Shipment struct for compilation.
/// In production, this would be imported from the maritime module.
#[derive(Debug, Clone)]
pub struct Shipment {
    /// Unique shipment identifier.
    pub id: String,
    /// ID of the entity sending the shipment.
    pub sender_id: String,
    /// Type of commodity being shipped.
    pub commodity: Commodity,
    /// Quantity of commodity.
    pub quantity: f64,
    /// Declared value of the cargo.
    pub declared_value: f64,
    /// Port of origin.
    pub origin_port: String,
    /// Port of destination.
    pub destination_port: String,
    /// Expected turn of arrival.
    pub expected_arrival_turn: u32,
}

impl BillOfLading {
    /// Create bill from physical shipment with dynamic LTV.
    pub fn from_shipment(shipment: &Shipment, standard_trade_ltv: f64) -> Self {
        BillOfLading {
            id: format!("BOL-{}", shipment.id),
            shipment_id: shipment.id.clone(),
            owner_id: shipment.sender_id.clone(),
            commodity: shipment.commodity.clone(),
            quantity: shipment.quantity,
            declared_value: shipment.declared_value,
            port_of_origin: shipment.origin_port.clone(),
            port_of_destination: shipment.destination_port.clone(),
            expected_arrival_turn: shipment.expected_arrival_turn,
            status: BillStatus::InTransit,
            collateral_value: shipment.declared_value * standard_trade_ltv,
            extra: HashMap::new(),
        }
    }
    
    /// Check if bill is valid collateral (in transit, not delivered).
    pub fn is_valid_collateral(&self, current_turn: u32) -> bool {
        matches!(self.status, BillStatus::InTransit | BillStatus::PledgedAsCollateral)
            && current_turn < self.expected_arrival_turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bill_of_lading_from_shipment() {
        let shipment = Shipment {
            id: "SHIP-001".to_string(),
            sender_id: "COMP-001".to_string(),
            commodity: Commodity::Steel,
            quantity: 1000.0,
            declared_value: 50000.0,
            origin_port: "Gdansk".to_string(),
            destination_port: "Rotterdam".to_string(),
            expected_arrival_turn: 100,
        };
        
        let bol = BillOfLading::from_shipment(&shipment, 0.8);
        assert_eq!(bol.id, "BOL-SHIP-001");
        assert_eq!(bol.collateral_value, 40000.0);
        assert!(matches!(bol.status, BillStatus::InTransit));
    }

    #[test]
    fn test_is_valid_collateral() {
        let shipment = Shipment {
            id: "SHIP-002".to_string(),
            sender_id: "COMP-002".to_string(),
            commodity: Commodity::Steel,
            quantity: 500.0,
            declared_value: 25000.0,
            origin_port: "Gdansk".to_string(),
            destination_port: "Hamburg".to_string(),
            expected_arrival_turn: 100,
        };
        
        let mut bol = BillOfLading::from_shipment(&shipment, 0.8);
        assert!(bol.is_valid_collateral(50));
        
        bol.status = BillStatus::Delivered;
        assert!(!bol.is_valid_collateral(50));
        
        bol.status = BillStatus::InTransit;
        assert!(!bol.is_valid_collateral(150));
    }
}

impl Default for BillStatus {
    fn default() -> Self {
        BillStatus::InTransit
    }
}

/// Process bills of lading for the current turn.
///
/// # Arguments
/// * `bills` - Mutable slice of all bills of lading
/// * `companies` - Mutable slice of all companies (for delivery settlement)
/// * `working_capital_loans` - Mutable slice of working capital loans
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Bills past expected_arrival_turn: mark as Delivered
/// * On delivery: cargo value credited to owner's brokerage cash
/// * Working capital loans at maturity: repay from borrower cash to lender
/// * If borrower cannot repay: bill collateral seized by lender
/// * NO MAGIC CASH: all flows are between existing entities
pub fn process_bills_of_lading(
    bills: &mut [BillOfLading],
    companies: &mut [crate::entities::Company],
    working_capital_loans: &mut Vec<WorkingCapitalLoan>,
    current_turn: u32,
) {
    // Process deliveries
    for bill in bills.iter_mut() {
        if bill.status == BillStatus::InTransit && current_turn >= bill.expected_arrival_turn {
            bill.status = BillStatus::Delivered;

            // Credit cargo value to owner
            let owner_id = bill.owner_id.clone();
            let cargo_value = bill.declared_value;
            if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                if let Some(ref mut acct) = owner.brokerage_account {
                    acct.cash += cargo_value;
                }
            }
        }

        // Expire old pledged bills past maturity
        if bill.status == BillStatus::PledgedAsCollateral && current_turn >= bill.expected_arrival_turn + 10 {
            bill.status = BillStatus::Expired;
        }
    }

    // Process working capital loan repayments
    let mut settled_indices = Vec::new();
    for (idx, loan) in working_capital_loans.iter_mut().enumerate() {
        if current_turn < loan.maturity_turn {
            continue;
        }

        let repayment = loan.principal * (1.0 + loan.interest_rate);
        let borrower_id = loan.borrower_id.clone();
        let lender_id = loan.lender_id.clone();

        let mut repaid = false;
        if let Some(borrower) = companies.iter_mut().find(|c| c.id == borrower_id) {
            if let Some(ref mut acct) = borrower.brokerage_account {
                if acct.cash >= repayment {
                    acct.cash -= repayment;
                    repaid = true;
                }
            }
        }

        if repaid {
            if let Some(lender) = companies.iter_mut().find(|c| c.id == lender_id) {
                if let Some(ref mut acct) = lender.brokerage_account {
                    acct.cash += repayment;
                }
            }
            settled_indices.push(idx);
        } else {
            // Borrower cannot repay: lender seizes collateral bill
            let collateral_bill_id = loan.collateral_bill_id.clone();
            if let Some(bill) = bills.iter_mut().find(|b| b.id == collateral_bill_id) {
                bill.owner_id = lender_id.clone();
                bill.status = BillStatus::Delivered;
                // Lender gets cargo value
                let cargo_value = bill.declared_value;
                if let Some(lender) = companies.iter_mut().find(|c| c.id == lender_id) {
                    if let Some(ref mut acct) = lender.brokerage_account {
                        acct.cash += cargo_value;
                    }
                }
            }
            settled_indices.push(idx);
        }
    }

    // Remove settled loans
    for idx in settled_indices.into_iter().rev() {
        working_capital_loans.remove(idx);
    }
}
