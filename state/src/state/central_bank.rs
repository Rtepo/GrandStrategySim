//! Central Bank (Bank Centralny) and Monetary Policy Council (RPP) structures.
//!
//! This module defines the supreme monetary authority for Stage D, including
//! institutional independence models, macroeconomic mandates, and the full suite
//! of interest rate tools.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use uuid;

// ============================================================================
// INSTITUTIONAL INDEPENDENCE MODELS
// ============================================================================

/// Political dependency model of the Central Bank.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CentralBankIndependence {
    /// Federal (Strictly Independent): Governor elected by regional branch presidents
    /// (chosen by local JST councils). State government has zero control. Cannot be dismissed.
    Federal,
    /// Central Independent: Governor appointed by Head of State/Parliament for fixed term.
    /// Regional directors appointed by Governor.
    CentralIndependent,
    /// Dependent (Ministerial): Governor acts like a minister, can be dismissed at any time
    /// by Head of State/Prime Minister, forced to print money/lower rates for political goals.
    Dependent,
}

impl Default for CentralBankIndependence {
    fn default() -> Self {
        CentralBankIndependence::CentralIndependent
    }
}

// ============================================================================
// MACROECONOMIC MANDATES
// ============================================================================

/// Macroeconomic mandate/goals of the Central Bank.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MonetaryMandate {
    /// Price stability is the supreme goal.
    Inflationary,
    /// Economic growth/stock market health prioritized (inflation secondary).
    Market,
    /// Balances both price stability and growth.
    Mixed,
}

impl Default for MonetaryMandate {
    fn default() -> Self {
        MonetaryMandate::Mixed
    }
}

// ============================================================================
// MONETARY POLICY COUNCIL (RPP) INTEREST RATES
// ============================================================================

/// The 5 distinct interest rates controlled by the Monetary Policy Council (RPP).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RppInterestRates {
    /// Reference Rate - primary policy rate.
    #[serde(default)]
    pub reference_rate: f64,
    /// Lombard Rate - rate for borrowing against collateral.
    #[serde(default)]
    pub lombard_rate: f64,
    /// Deposit Rate - rate for deposits at central bank.
    #[serde(default)]
    pub deposit_rate: f64,
    /// Rediscount Rate for Bills of Exchange.
    #[serde(default)]
    pub rediscount_rate: f64,
    /// Discount Rate for Bills of Exchange.
    #[serde(default)]
    pub discount_rate: f64,
    /// Any additional RPP rate fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

// ============================================================================
// MONETARY POLICY COUNCIL (RPP) STRUCTURE
// ============================================================================

/// Monetary Policy Council (Monetary Policy Council) - sets interest rates.
/// In independent models, this is a separate political body. In dependent models,
/// the CB handles rate decisions directly but the council structure exists for record-keeping.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MonetaryPolicyCouncil {
    /// Last RPP meeting turn number.
    #[serde(default)]
    pub last_meeting_turn: u32,
    /// Next scheduled meeting turn.
    #[serde(default)]
    pub next_meeting_turn: u32,
    /// RPP decision log (rate changes, rationale).
    #[serde(default)]
    pub decision_log: Vec<String>,
    /// Any additional RPP fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

// ============================================================================
// CENTRAL BANK STRUCTURE
// ============================================================================

/// Central Bank (Bank Centralny) - supreme monetary authority.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CentralBank {
    /// Central Bank identifier (e.g., "BC_ILIRIA").
    #[serde(default)]
    pub id: String,
    /// Display name (e.g., "Bank Centralny Ilirii").
    #[serde(default)]
    pub name: String,
    /// Institutional independence model.
    #[serde(default)]
    pub independence_model: CentralBankIndependence,
    /// Macroeconomic mandate/goals.
    #[serde(default)]
    pub mandate: MonetaryMandate,
    /// Current Governor identifier.
    #[serde(default)]
    pub governor_id: String,
    /// Governor appointment turn (for term tracking).
    #[serde(default)]
    pub governor_appointment_turn: u32,
    /// Governor term length in turns (0 = indefinite).
    #[serde(default)]
    pub governor_term_length: u32,
    /// Regional branch directors (for Federal model).
    #[serde(default)]
    pub regional_directors: Vec<String>,
    /// The 5 interest rates (mandatory field - rates exist regardless of independence model).
    #[serde(default)]
    pub interest_rates: RppInterestRates,
    /// Monetary Policy Council (optional political body for independent models).
    /// In dependent models, CB handles rate decisions directly but this may exist for record-keeping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpp: Option<MonetaryPolicyCouncil>,
    /// Reserve Requirement Ratio - CB exclusive tool.
    #[serde(default)]
    pub reserve_requirement_ratio: f64,
    /// Foreign Exchange Reserves - tangible asset ledger tracking foreign currencies held.
    /// Key = foreign currency code (e.g., "USD", "EUR"), Value = amount held.
    #[serde(default)]
    pub fx_reserves: HashMap<String, f64>,
    /// Phase E.1: Physical gold reserves held by the Central Bank.
    /// Used for currency interventions and gold standard backing.
    #[serde(default)]
    pub physical_gold_reserves: f64,
    /// Total liquidity injected into the banking system via Lombard loans and emergency lending.
    /// Used for tracking monetary base expansion.
    #[serde(default)]
    pub liquidity_injected: f64,
    /// Government bonds held by CB acquired via secondary market OMO purchases.
    /// When CB buys bonds from banks, it credits their reserves and takes the bonds.
    /// When CB sells bonds to banks, it debits their reserves and gives the bonds back.
    #[serde(default)]
    pub omo_bond_holdings: f64,
    /// Target interbank rate (XIBOR) the CB steers towards via OMO.
    /// Set by Taylor Rule when independent, or politically when dependent.
    #[serde(default)]
    pub omo_target_rate: f64,
    /// Last turn OMO was executed.
    #[serde(default)]
    pub omo_last_operation_turn: u32,
    /// Net amount of last OMO operation (positive = purchase/injection, negative = sale/absorption).
    #[serde(default)]
    pub omo_last_operation_amount: f64,
    /// Total interest paid to banks on deposit facility balances (cumulative).
    #[serde(default)]
    pub deposit_facility_interest_paid: f64,
    /// Total interest received from banks on Lombard facility loans (cumulative).
    #[serde(default)]
    pub lombard_facility_interest_received: f64,
    /// Last CB communication/message.
    #[serde(default)]
    pub last_message: String,
    /// Phase 36: Target inflation rate (e.g., 0.02 for 2%). Serialized, adjustable.
    /// Used by the Taylor Rule in `update_reference_rate`.
    #[serde(default = "default_target_inflation")]
    pub target_inflation: f64,
    /// Phase 36: Potential/long-run GDP growth rate (e.g., 0.02 for 2%). Serialized, adjustable.
    /// Used by the Taylor Rule in `update_reference_rate`.
    #[serde(default = "default_potential_growth")]
    pub potential_growth: f64,
    /// Phase 36: Neutral real interest rate (e.g., 0.02 for 2%). Serialized, adjustable.
    /// Used by the Taylor Rule in `update_reference_rate`.
    #[serde(default = "default_neutral_rate")]
    pub neutral_rate: f64,
    /// Any additional CB fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Phase 36: Default target inflation rate (2%).
fn default_target_inflation() -> f64 { 0.02 }
/// Phase 36: Default potential GDP growth rate (2%).
fn default_potential_growth() -> f64 { 0.02 }
/// Phase 36: Default neutral real interest rate (2%).
fn default_neutral_rate() -> f64 { 0.02 }

/// Phase 36: Manual Default implementation for CentralBank to ensure the
/// derived defaults match the serde defaults. Without this, the derived
/// Default would give 0.0 for target_inflation/potential_growth/neutral_rate,
/// while serde deserialization would use the default functions (0.02),
/// causing round-trip serialization tests to fail.
impl Default for CentralBank {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            independence_model: CentralBankIndependence::default(),
            mandate: MonetaryMandate::default(),
            governor_id: String::new(),
            governor_appointment_turn: 0,
            governor_term_length: 0,
            regional_directors: Vec::new(),
            interest_rates: RppInterestRates::default(),
            rpp: None,
            reserve_requirement_ratio: 0.0,
            fx_reserves: std::collections::HashMap::new(),
            physical_gold_reserves: 0.0,
            liquidity_injected: 0.0,
            omo_bond_holdings: 0.0,
            omo_target_rate: 0.0,
            omo_last_operation_turn: 0,
            omo_last_operation_amount: 0.0,
            deposit_facility_interest_paid: 0.0,
            lombard_facility_interest_received: 0.0,
            last_message: String::new(),
            target_inflation: default_target_inflation(),
            potential_growth: default_potential_growth(),
            neutral_rate: default_neutral_rate(),
            extra: Map::new(),
        }
    }
}

impl CentralBank {
    /// Calculates M0 (Monetary Base) - cash in circulation + bank reserves at central bank.
    /// This is dynamically computed from actual ledgers to guarantee double-entry accuracy.
    ///
    /// # Arguments
    /// * `cash_in_circulation` - Total physical cash circulating in the economy
    /// * `bank_reserves` - Total reserves held by commercial banks at the central bank
    ///
    /// # Returns
    /// M0 = cash_in_circulation + bank_reserves
    pub fn calculate_m0(&self, cash_in_circulation: f64, bank_reserves: f64) -> f64 {
        cash_in_circulation + bank_reserves
    }

    /// Calculates M3 (Broad Money) - M0 + demand deposits + time deposits + other liquid assets.
    /// This is dynamically computed from actual ledgers to guarantee double-entry accuracy.
    ///
    /// # Arguments
    /// * `m0` - Monetary base (from calculate_m0)
    /// * `demand_deposits` - Total demand deposits in commercial banks
    /// * `time_deposits` - Total time deposits in commercial banks
    /// * `other_liquid_assets` - Other liquid assets (money market funds, etc.)
    ///
    /// # Returns
    /// M3 = M0 + demand_deposits + time_deposits + other_liquid_assets
    pub fn calculate_m3(&self, m0: f64, demand_deposits: f64, time_deposits: f64, other_liquid_assets: f64) -> f64 {
        m0 + demand_deposits + time_deposits + other_liquid_assets
    }

    /// Calculates the money multiplier (M3 / M0).
    /// Indicates credit creation efficiency of the banking system.
    ///
    /// # Arguments
    /// * `m0` - Monetary base
    /// * `m3` - Broad money
    ///
    /// # Returns
    /// Money multiplier = M3 / M0 (returns 0.0 if M0 is zero to avoid division by zero)
    pub fn calculate_money_multiplier(&self, m0: f64, m3: f64) -> f64 {
        if m0 > 0.0 {
            m3 / m0
        } else {
            0.0
        }
    }

    /// Updates the reference rate based on a Taylor Rule using configurable
    /// targets stored on the CentralBank struct.
    ///
    /// Phase 36: Replaced fixed-step adjustment logic with a Taylor Rule:
    ///   reference_rate = neutral_rate + 1.5 * (inflation - target_inflation)
    ///                          + 0.5 * (gdp_growth - potential_growth)
    ///
    /// # Arguments
    /// * `current_inflation` - Current inflation rate (e.g., 0.05 for 5%)
    /// * `gdp_growth` - Current GDP growth rate (e.g., 0.02 for 2%)
    /// * `current_turn` - Current turn number for RPP meeting scheduling
    ///
    /// # Rules
    /// * Uses `self.target_inflation`, `self.potential_growth`, `self.neutral_rate`
    ///   (serialized fields, NOT hardcoded magic numbers).
    /// * Inflationary mandate: weighs inflation gap 2.0× (aggressive price stability)
    /// * Market mandate: weighs growth gap 1.5× (growth-oriented)
    /// * Mixed mandate: standard Taylor weights (1.5× inflation, 0.5× growth)
    /// * Floor at 0%, cap at 20%
    /// * Smoothing: 30% weight on previous rate to avoid excessive volatility
    pub fn update_reference_rate(&mut self, current_inflation: f64, gdp_growth: f64, current_turn: u32) {
        let target_inflation = self.target_inflation;
        let potential_growth = self.potential_growth;
        let neutral_rate = self.neutral_rate;

        let inflation_gap = current_inflation - target_inflation;
        let growth_gap = gdp_growth - potential_growth;

        // Taylor Rule with mandate-specific weights
        let (inflation_weight, growth_weight) = match self.mandate {
            MonetaryMandate::Inflationary => (2.0, 0.5),  // Aggressive on inflation
            MonetaryMandate::Market => (0.5, 1.5),        // Growth-oriented
            MonetaryMandate::Mixed => (1.5, 0.5),         // Standard Taylor
        };

        let taylor_rate = neutral_rate
            + inflation_weight * inflation_gap
            + growth_weight * growth_gap;

        // Smoothing: 70% new Taylor rate, 30% previous rate
        let prev_rate = self.interest_rates.reference_rate;
        let smoothed_rate = 0.7 * taylor_rate + 0.3 * prev_rate;

        // Phase 40: Allow NIRP (Negative Interest Rate Policy) during severe deflation.
        // Floor at -2% (-0.02) to allow meaningful negative rates while preventing
        // absurd deep-negative territory. Cap at 20%.
        let new_reference_rate = smoothed_rate.max(-0.02).min(0.20);
        let rate_adjustment = new_reference_rate - prev_rate;

        self.interest_rates.reference_rate = new_reference_rate;

        // Update other rates to maintain hierarchy
        self.update_rate_hierarchy();

        // Log decision if RPP exists
        if let Some(ref mut rpp) = self.rpp {
            rpp.last_meeting_turn = current_turn;
            rpp.next_meeting_turn = current_turn + 12; // Monthly meetings
            let rationale = format!(
                "Turn {}: Taylor Rule set rate to {:.2}% (adjustment: {:+.2} bps). Mandate: {:?}. Inflation: {:.2}%, Target: {:.2}%, Growth: {:.2}%, Potential: {:.2}%, Neutral: {:.2}%",
                current_turn,
                new_reference_rate * 100.0,
                rate_adjustment * 100.0,
                self.mandate,
                current_inflation * 100.0,
                target_inflation * 100.0,
                gdp_growth * 100.0,
                potential_growth * 100.0,
                neutral_rate * 100.0
            );
            rpp.decision_log.push(rationale);
        }
    }

    /// Updates the rate hierarchy to maintain proper spread between rates.
    /// Lombard > Reference > Rediscount > Discount > Deposit
    ///
    /// # Rules
    /// * Lombard rate: Typically 100-200 bps above reference rate
    /// * Deposit rate: Typically 100-200 bps below reference rate
    /// * Rediscount and discount: Between reference and deposit/lombard
    fn update_rate_hierarchy(&mut self) {
        let reference = self.interest_rates.reference_rate;
        self.interest_rates.lombard_rate = (reference + 0.015).min(0.25); // +150 bps, cap at 25%
        self.interest_rates.rediscount_rate = (reference + 0.005).min(0.25); // +50 bps, cap at 25%
        // Phase 40: Allow negative discount and deposit rates (NIRP).
        self.interest_rates.discount_rate = (reference - 0.0075).max(-0.025); // -75 bps, floor -2.5%
        self.interest_rates.deposit_rate = (reference - 0.015).max(-0.03); // -150 bps, floor -3%
        // The reference rate is the CB's target for the interbank rate (XIBOR).
        // OMO operations will steer XIBOR towards this target physically.
        self.omo_target_rate = reference;
    }

    /// Phase 16A: Accrue interest on a bank's deposit facility balance.
    /// Banks physically park excess reserves at the CB and earn the deposit rate.
    /// This is a hard floor for the interbank rate — no bank lends below this rate.
    ///
    /// # Arguments
    /// * `balance` - Current deposit facility balance of the bank
    ///
    /// # Returns
    /// Interest amount to be credited to the bank's reserves.
    /// Also updates cumulative `deposit_facility_interest_paid` on the CB.
    pub fn accrue_deposit_facility_interest(&mut self, balance: f64) -> f64 {
        if balance <= 0.0 {
            return 0.0;
        }
        let interest = balance * self.interest_rates.deposit_rate;
        self.deposit_facility_interest_paid += interest;
        interest
    }

    /// Phase 16A: Accrue interest on a bank's Lombard facility loan.
    /// Banks physically borrow reserves from the CB at the Lombard (penalty) rate.
    /// This is a hard ceiling for the interbank rate — no bank borrows above this rate.
    ///
    /// # Arguments
    /// * `loan_amount` - Current Lombard loan balance of the bank
    ///
    /// # Returns
    /// Interest amount to be debited from the bank's reserves.
    /// Also updates cumulative `lombard_facility_interest_received` on the CB.
    pub fn accrue_lombard_facility_interest(&mut self, loan_amount: f64) -> f64 {
        if loan_amount <= 0.0 {
            return 0.0;
        }
        let interest = loan_amount * self.interest_rates.lombard_rate;
        self.lombard_facility_interest_received += interest;
        interest
    }

    /// Phase 16A: Decide and execute OMO to steer XIBOR toward target rate.
    ///
    /// The CB compares the current XIBOR to its target rate. If XIBOR is above target,
    /// the CB buys bonds from banks (injecting reserves, increasing liquidity, pushing
    /// XIBOR down). If XIBOR is below target, the CB sells bonds to banks (absorbing
    /// reserves, decreasing liquidity, pushing XIBOR up).
    ///
    /// # Arguments
    /// * `current_xibor` - Current interbank rate after clearing
    /// * `total_bank_reserves` - Total reserves at central bank across all banks
    /// * `total_bank_bonds` - Total government bonds held by commercial banks
    /// * `current_turn` - Current turn number
    ///
    /// # Returns
    /// Net OMO amount: positive = CB bought bonds (injected reserves),
    /// negative = CB sold bonds (absorbed reserves).
    pub fn execute_omo(
        &mut self,
        current_xibor: f64,
        total_bank_reserves: f64,
        total_bank_bonds: f64,
        current_turn: u32,
    ) -> f64 {
        let target = self.omo_target_rate;
        let rate_gap = current_xibor - target;

        // If XIBOR is within 5 bps of target, no action needed
        if rate_gap.abs() < 0.0005 {
            self.omo_last_operation_turn = current_turn;
            self.omo_last_operation_amount = 0.0;
            return 0.0;
        }

        // Calculate operation size proportional to the rate gap and total reserves.
        // Larger gap -> larger operation. Scale by 10% of total reserves per 100 bps gap.
        let intensity = (rate_gap.abs() / 0.01).min(5.0); // Cap at 5x (500 bps gap)
        let max_operation = total_bank_reserves * 0.10 * intensity;

        if rate_gap > 0.0 {
            // XIBOR too high -> CB buys bonds from banks, injects reserves
            // Limited by how many bonds banks actually hold
            let buy_amount = max_operation.min(total_bank_bonds);
            self.omo_bond_holdings += buy_amount;
            self.omo_last_operation_turn = current_turn;
            self.omo_last_operation_amount = buy_amount;
            buy_amount
        } else {
            // XIBOR too low -> CB sells bonds to banks, absorbs reserves
            // Limited by how many bonds the CB holds
            let sell_amount = max_operation.min(self.omo_bond_holdings);
            self.omo_bond_holdings -= sell_amount;
            self.omo_last_operation_turn = current_turn;
            self.omo_last_operation_amount = -sell_amount;
            -sell_amount
        }
    }

    /// Phase E.1: Calculate gold coverage ratio (gold value / M0).
    /// 
    /// # Arguments
    /// * `m0` - Monetary base (cash in circulation + bank reserves)
    /// * `gold_price_in_ieu` - Current gold price in IEU
    /// * `currency_rate` - Domestic currency exchange rate vs IEU
    /// 
    /// # Returns
    /// Gold coverage ratio (how much of M0 is backed by gold)
    /// 
    /// # Rules
    /// - Higher ratio = stronger gold backing
    /// - Used for gold standard assessment
    pub fn calculate_gold_coverage(&self, m0: f64, gold_price_in_ieu: f64, currency_rate: f64) -> f64 {
        if m0 > 0.0 && currency_rate > 0.0 {
            let gold_value_in_currency = self.physical_gold_reserves * gold_price_in_ieu / currency_rate;
            gold_value_in_currency / m0
        } else {
            0.0
        }
    }
    
    /// Phase E.1: Buy gold from the global market (interacts with GlobalGoldExchange via execute_cb_trade).
    /// 
    /// # Arguments
    /// * `gold_amount` - Amount of gold to buy
    /// * `gold_exchange` - Global Gold Exchange (for trade execution)
    /// * `currencies` - Global currency registry (for IEU conversion)
    /// * `vaults` - Global vault registry (for physical gold storage)
    /// * `payment_currency` - Currency used for payment (e.g., "USD")
    /// * `cb_id` - Central Bank entity ID (for vault access)
    /// 
    /// # Returns
    /// Result with success or error (insufficient fx reserves, trade execution failure)
    /// 
    /// # Rules
    /// - CB cannot materialize gold - must buy from GlobalGoldExchange
    /// - Calls gold_exchange.execute_cb_trade() (bypasses brokerage_accounts requirement)
    /// - Payment currency must be in fx_reserves
    /// - Physical gold delivered to CB's vault
    pub fn buy_gold(
        &mut self,
        gold_amount: f64,
        gold_exchange: &mut crate::state::gold::GlobalGoldExchange,
        currencies: &HashMap<String, crate::state::Currency>,
        vaults: &mut std::collections::BTreeMap<String, f64>,
        payment_currency: &str,
        cb_id: &str,
        current_turn: u32,
    ) -> Result<(), String> {
        // Create gold buy order
        let gold_order = crate::state::gold::GoldOrder {
            id: format!("CB-BUY-{}", uuid::Uuid::new_v4()),
            entity_id: cb_id.to_string(),
            order_type: crate::state::forex::ForexOrderType::Buy,
            gold_amount,
            payment_currency: payment_currency.to_string(),
            limit_price_in_ieu: None,
            expiry_turn: 1,
            extra: serde_json::Map::new(),
        };
        
        // Execute trade via GlobalGoldExchange::execute_cb_trade (no brokerage_accounts)
        let _trade = gold_exchange.execute_cb_trade(
            gold_order,
            currencies,
            vaults,
            &mut self.fx_reserves,
            &mut self.physical_gold_reserves,
            cb_id,
            current_turn,
        )?;

        // Double-entry and vault updates handled by execute_cb_trade
        Ok(())
    }

    /// Phase E.1: Sell gold to the global market (interacts with GlobalGoldExchange via execute_cb_trade).
    /// 
    /// # Arguments
    /// * `gold_amount` - Amount of gold to sell
    /// * `gold_exchange` - Global Gold Exchange (for trade execution)
    /// * `currencies` - Global currency registry (for IEU conversion)
    /// * `vaults` - Global vault registry (for physical gold storage)
    /// * `target_currency` - Currency to receive (e.g., "USD")
    /// * `cb_id` - Central Bank entity ID (for vault access)
    /// 
    /// # Returns
    /// Result with success or error (insufficient gold reserves, trade execution failure)
    /// 
    /// # Rules
    /// - CB cannot materialize fiat - must sell to GlobalGoldExchange
    /// - Calls gold_exchange.execute_cb_trade() (bypasses brokerage_accounts requirement)
    /// - Used to defend currency peg during speculative attacks
    /// - Physical gold debited from CB's vault, fiat credited to fx_reserves
    pub fn sell_gold(
        &mut self,
        gold_amount: f64,
        gold_exchange: &mut crate::state::gold::GlobalGoldExchange,
        currencies: &HashMap<String, crate::state::Currency>,
        vaults: &mut std::collections::BTreeMap<String, f64>,
        target_currency: &str,
        cb_id: &str,
        current_turn: u32,
    ) -> Result<(), String> {
        // Create gold sell order
        let gold_order = crate::state::gold::GoldOrder {
            id: format!("CB-SELL-{}", uuid::Uuid::new_v4()),
            entity_id: cb_id.to_string(),
            order_type: crate::state::forex::ForexOrderType::Sell,
            gold_amount,
            payment_currency: target_currency.to_string(),
            limit_price_in_ieu: None,
            expiry_turn: 1,
            extra: serde_json::Map::new(),
        };
        
        // Execute trade via GlobalGoldExchange::execute_cb_trade (no brokerage_accounts)
        let _trade = gold_exchange.execute_cb_trade(
            gold_order,
            currencies,
            vaults,
            &mut self.fx_reserves,
            &mut self.physical_gold_reserves,
            cb_id,
            current_turn,
        )?;

        // Double-entry and vault updates handled by execute_cb_trade
        Ok(())
    }

    /// Checks if the central bank can change its mandate.
    /// Independent banks require supermajority or head of state decree.
    ///
    /// # Arguments
    /// * `parliamentary_support` - Proportion of parliament supporting the change (0.0-1.0)
    /// * `head_of_state_decree` - Whether head of state has issued a decree
    ///
    /// # Returns
    /// true if mandate change is permitted, false otherwise
    ///
    /// # Rules
    /// * Federal/Central Independent: Requires 2/3 parliamentary supermajority OR head of state decree
    /// * Dependent: Can change at any time (government control)
    pub fn can_change_mandate(&self, parliamentary_support: f64, head_of_state_decree: bool) -> bool {
        match self.independence_model {
            CentralBankIndependence::Federal | CentralBankIndependence::CentralIndependent => {
                parliamentary_support >= 2.0 / 3.0 || head_of_state_decree
            }
            CentralBankIndependence::Dependent => {
                true // Government can change at will
            }
        }
    }

    /// Changes the monetary mandate if permitted.
    ///
    /// # Arguments
    /// * `new_mandate` - The new mandate to adopt
    /// * `parliamentary_support` - Proportion of parliament supporting the change (0.0-1.0)
    /// * `head_of_state_decree` - Whether head of state has issued a decree
    ///
    /// # Returns
    /// true if mandate was changed, false if not permitted
    ///
    /// # Rules
    /// * Dependent banks must always have Mixed mandate (enforced)
    pub fn change_mandate(&mut self, new_mandate: MonetaryMandate, parliamentary_support: f64, head_of_state_decree: bool) -> bool {
        // Dependent banks cannot have pure Inflationary or Market mandate
        if self.independence_model == CentralBankIndependence::Dependent {
            if new_mandate != MonetaryMandate::Mixed {
                return false;
            }
        }

        if self.can_change_mandate(parliamentary_support, head_of_state_decree) {
            self.mandate = new_mandate;
            true
        } else {
            false
        }
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_central_bank_independence_default() {
        let independence = CentralBankIndependence::default();
        assert_eq!(independence, CentralBankIndependence::CentralIndependent);
    }

    #[test]
    fn test_monetary_mandate_default() {
        let mandate = MonetaryMandate::default();
        assert_eq!(mandate, MonetaryMandate::Mixed);
    }

    #[test]
    fn test_rpp_interest_rates_default() {
        let rates = RppInterestRates::default();
        assert!((rates.reference_rate - 0.0).abs() < 1e-9);
        assert!((rates.lombard_rate - 0.0).abs() < 1e-9);
        assert!((rates.deposit_rate - 0.0).abs() < 1e-9);
        assert!((rates.rediscount_rate - 0.0).abs() < 1e-9);
        assert!((rates.discount_rate - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_central_bank_default() {
        let cb = CentralBank::default();
        assert_eq!(cb.id, "");
        assert_eq!(cb.name, "");
        assert_eq!(cb.independence_model, CentralBankIndependence::CentralIndependent);
        assert_eq!(cb.mandate, MonetaryMandate::Mixed);
        assert_eq!(cb.governor_id, "");
        assert_eq!(cb.governor_appointment_turn, 0);
        assert_eq!(cb.governor_term_length, 0);
        assert!(cb.regional_directors.is_empty());
        assert!(cb.rpp.is_none());
        assert!((cb.reserve_requirement_ratio - 0.0).abs() < 1e-9);
        assert!(cb.fx_reserves.is_empty());
        assert_eq!(cb.last_message, "");
    }

    #[test]
    fn test_calculate_m0() {
        let cb = CentralBank::default();
        let m0 = cb.calculate_m0(1_000_000.0, 500_000.0);
        assert!((m0 - 1_500_000.0).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_m3() {
        let cb = CentralBank::default();
        let m0 = 1_500_000.0;
        let m3 = cb.calculate_m3(m0, 5_000_000.0, 2_000_000.0, 1_000_000.0);
        assert!((m3 - 9_500_000.0).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_money_multiplier() {
        let cb = CentralBank::default();
        let multiplier = cb.calculate_money_multiplier(1_500_000.0, 9_500_000.0);
        assert!((multiplier - 6.333333333333333).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_money_multiplier_zero_m0() {
        let cb = CentralBank::default();
        let multiplier = cb.calculate_money_multiplier(0.0, 9_500_000.0);
        assert!((multiplier - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_federal_independence() {
        let cb = CentralBank {
            independence_model: CentralBankIndependence::Federal,
            ..Default::default()
        };
        assert_eq!(cb.independence_model, CentralBankIndependence::Federal);
    }

    #[test]
    fn test_dependent_independence() {
        let cb = CentralBank {
            independence_model: CentralBankIndependence::Dependent,
            mandate: MonetaryMandate::Mixed, // Dependent banks always have Mixed mandate
            ..Default::default()
        };
        assert_eq!(cb.independence_model, CentralBankIndependence::Dependent);
        assert_eq!(cb.mandate, MonetaryMandate::Mixed);
    }

    #[test]
    fn test_inflationary_mandate() {
        let cb = CentralBank {
            mandate: MonetaryMandate::Inflationary,
            ..Default::default()
        };
        assert_eq!(cb.mandate, MonetaryMandate::Inflationary);
    }

    #[test]
    fn test_market_mandate() {
        let cb = CentralBank {
            mandate: MonetaryMandate::Market,
            ..Default::default()
        };
        assert_eq!(cb.mandate, MonetaryMandate::Market);
    }

    #[test]
    fn test_fx_reserves_tracking() {
        let mut cb = CentralBank::default();
        cb.fx_reserves.insert("USD".to_string(), 1_000_000.0);
        cb.fx_reserves.insert("EUR".to_string(), 500_000.0);
        assert_eq!(cb.fx_reserves.len(), 2);
        assert!((cb.fx_reserves.get("USD").unwrap() - 1_000_000.0).abs() < 1e-9);
        assert!((cb.fx_reserves.get("EUR").unwrap() - 500_000.0).abs() < 1e-9);
    }

    #[test]
    fn test_update_reference_rate_inflationary_mandate() {
        let mut cb = CentralBank {
            mandate: MonetaryMandate::Inflationary,
            interest_rates: RppInterestRates {
                reference_rate: 0.05,
                ..Default::default()
            },
            rpp: Some(MonetaryPolicyCouncil::default()),
            ..Default::default()
        };

        // High inflation should raise rates (Taylor Rule: inflation 5% vs target 2%)
        cb.update_reference_rate(0.05, 0.03, 10);
        assert!(cb.interest_rates.reference_rate > 0.05); // Rate increased
        assert_eq!(cb.rpp.as_ref().unwrap().decision_log.len(), 1);
    }

    #[test]
    fn test_update_reference_rate_market_mandate() {
        let mut cb = CentralBank {
            mandate: MonetaryMandate::Market,
            interest_rates: RppInterestRates {
                reference_rate: 0.05,
                ..Default::default()
            },
            rpp: Some(MonetaryPolicyCouncil::default()),
            ..Default::default()
        };

        // Weak growth should lower rates (growth 0.5% vs potential 2%)
        cb.update_reference_rate(0.02, 0.005, 10);
        assert!(cb.interest_rates.reference_rate < 0.05); // Rate decreased
    }

    #[test]
    fn test_update_reference_rate_mixed_mandate() {
        let mut cb = CentralBank {
            mandate: MonetaryMandate::Mixed,
            interest_rates: RppInterestRates {
                reference_rate: 0.05,
                ..Default::default()
            },
            rpp: Some(MonetaryPolicyCouncil::default()),
            ..Default::default()
        };

        // Moderate inflation gap with weak growth
        cb.update_reference_rate(0.03, 0.01, 10);
        // Should lower rates due to weak growth
        assert!(cb.interest_rates.reference_rate < 0.05);
    }

    #[test]
    fn test_rate_hierarchy_maintenance() {
        let mut cb = CentralBank {
            interest_rates: RppInterestRates {
                reference_rate: 0.05,
                ..Default::default()
            },
            ..Default::default()
        };

        cb.update_reference_rate(0.03, 0.02, 10);

        // Verify hierarchy: Lombard > Rediscount > Reference > Discount > Deposit
        assert!(cb.interest_rates.lombard_rate > cb.interest_rates.rediscount_rate);
        assert!(cb.interest_rates.rediscount_rate > cb.interest_rates.reference_rate);
        assert!(cb.interest_rates.reference_rate > cb.interest_rates.discount_rate);
        assert!(cb.interest_rates.discount_rate >= cb.interest_rates.deposit_rate);
    }

    #[test]
    fn test_can_change_mandate_independent() {
        let cb = CentralBank {
            independence_model: CentralBankIndependence::CentralIndependent,
            ..Default::default()
        };

        // Requires supermajority or decree
        assert!(!cb.can_change_mandate(0.5, false)); // 50% support, no decree
        assert!(cb.can_change_mandate(0.67, false)); // 67% support
        assert!(cb.can_change_mandate(0.5, true)); // Decree overrides
    }

    #[test]
    fn test_can_change_mandate_dependent() {
        let cb = CentralBank {
            independence_model: CentralBankIndependence::Dependent,
            ..Default::default()
        };

        // Can change at any time
        assert!(cb.can_change_mandate(0.0, false));
        assert!(cb.can_change_mandate(0.5, false));
    }

    #[test]
    fn test_change_mandate_independent_success() {
        let mut cb = CentralBank {
            independence_model: CentralBankIndependence::CentralIndependent,
            mandate: MonetaryMandate::Mixed,
            ..Default::default()
        };

        let changed = cb.change_mandate(MonetaryMandate::Inflationary, 0.67, false);
        assert!(changed);
        assert_eq!(cb.mandate, MonetaryMandate::Inflationary);
    }

    #[test]
    fn test_change_mandate_independent_failure() {
        let mut cb = CentralBank {
            independence_model: CentralBankIndependence::CentralIndependent,
            mandate: MonetaryMandate::Mixed,
            ..Default::default()
        };

        let changed = cb.change_mandate(MonetaryMandate::Inflationary, 0.5, false);
        assert!(!changed);
        assert_eq!(cb.mandate, MonetaryMandate::Mixed); // Unchanged
    }

    #[test]
    fn test_change_mandate_dependent_restriction() {
        let mut cb = CentralBank {
            independence_model: CentralBankIndependence::Dependent,
            mandate: MonetaryMandate::Mixed,
            ..Default::default()
        };

        // Dependent banks cannot have pure Inflationary mandate
        let changed = cb.change_mandate(MonetaryMandate::Inflationary, 1.0, false);
        assert!(!changed);
        assert_eq!(cb.mandate, MonetaryMandate::Mixed);
    }

    #[test]
    fn test_change_mandate_dependent_mixed_allowed() {
        let mut cb = CentralBank {
            independence_model: CentralBankIndependence::Dependent,
            mandate: MonetaryMandate::Mixed,
            ..Default::default()
        };

        // Can change to Mixed (no-op but allowed)
        let changed = cb.change_mandate(MonetaryMandate::Mixed, 0.0, false);
        assert!(changed);
    }

    #[test]
    fn test_rate_floor_and_cap() {
        let mut cb = CentralBank {
            interest_rates: RppInterestRates {
                reference_rate: 0.15,
                ..Default::default()
            },
            mandate: MonetaryMandate::Inflationary,
            ..Default::default()
        };

        // Try to push rate above 20%
        cb.update_reference_rate(0.10, 0.05, 10);
        assert!(cb.interest_rates.reference_rate <= 0.20);

        // Phase 40: NIRP allows rates below 0% (floor is now -2%).
        // Try to push rate below 0% with deflation.
        cb.interest_rates.reference_rate = 0.01;
        cb.update_reference_rate(-0.05, 0.0, 10);
        // Rate should be negative but not below -2%
        assert!(cb.interest_rates.reference_rate < 0.0);
        assert!(cb.interest_rates.reference_rate >= -0.02);
    }
}
