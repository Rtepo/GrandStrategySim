//! Commercial banking turn: fractional reserve credit creation, reserve limits,
//! and dynamic interest rates.
//!
//! This module ports the deterministic backbone of the Python banking turn from
//! `corporate/markets/banking/core.py`. Random events (new bank creation,
//! consortiums, certificate-of-deposit emission) are intentionally omitted;
//! only the balance-sheet, reserve, and interest-rate logic is ported.

use crate::economy::CountryTurnCtx;

/// Central-bank base rate used until the currency system is fully ported.
///
/// # Rules
/// * This is a placeholder; the real value will come from `ctx.registries` or
///   global currency state once the monetary system is migrated.
const BASE_RATE: f64 = 0.05;

/// Maximum leverage a bank is allowed before it is considered distressed.
///
/// # Rules
/// * Mirrors the `max_leverage` default in the Python banking turn; KNF regulatory
///   adjustments will be added when the regulator module is ported.
const MAX_LEVERAGE: f64 = 15.0;

/// Risk spread added to the base rate based on the bank's type.
///
/// # Arguments
/// * `bank_type` - The `type` field from the Python bank dictionary.
///
/// # Returns
/// The spread in percentage points, e.g. `0.02` for a commercial bank.
///
/// # Rules
/// * Matches the `risk_margin` logic in `corporate/markets/banking/core.py`:
///   Investment = 0.05, Cooperative = 0.01, Commercial = 0.02.
fn risk_spread(bank_type: &str) -> f64 {
    match bank_type {
        "Investment" => 0.05,
        "Cooperative" => 0.01,
        "State" => 0.015,
        _ => 0.02,
    }
}

/// Processes one economic turn for the entire commercial banking sector of a
/// country.
///
/// # Arguments
/// * `ctx` - The [`CountryTurnCtx`] holding the country whose `budget.banks`
///   will be updated.
///
/// # Rules
/// * For each bank:
///   1. `required_reserves = total_deposits * reserve_requirement_ratio`.
///   2. `liquidity` is updated as `liquid_reserves / required_reserves`.
///   3. If the bank is sufficiently reserved (`liquid_reserves >= required_reserves`),
///      `max_new_credit = max(0, total_deposits - required_reserves - issued_loans)`.
///   4. `interest_rate` is updated based on the base rate, the bank type's
///      risk spread, and liquidity tightness.
///   5. `deposit_interest_rate` is set to 80% of the loan rate.
///   6. Interest income/expense and operating costs are accrued to `own_capital`.
///   7. `issued_loans` is increased by `max_new_credit` and `last_new_credit`
///      records the amount.
///   8. `condition` is updated from the new leverage.
///
/// * The function is deterministic: the same `Country` state and registries
///   always produce the same outputs.
pub fn process_banking_system(ctx: &mut CountryTurnCtx<'_>) {
    for bank in &mut ctx.country.budget.banks {
        let required_reserves = bank.total_deposits * bank.reserve_requirement_ratio;

        // Liquidity ratio; treat zero required reserves as fully liquid.
        bank.liquidity = if required_reserves > 0.0 {
            bank.liquid_reserves / required_reserves
        } else {
            f64::INFINITY
        };

        // Tightness = 0 when liquidity >= 1, rising as liquidity falls below 1.
        let liquidity_tightness = if bank.liquidity.is_infinite() || bank.liquidity >= 1.0 {
            0.0
        } else {
            1.0 - bank.liquidity
        };

        // Dynamic interest-rate update.
        bank.interest_rate = BASE_RATE + risk_spread(&bank.bank_type) + liquidity_tightness * 0.05;
        bank.deposit_interest_rate = bank.interest_rate * 0.8;

        // Interest and operating-cost accrual.
        let loan_accrual = bank.issued_loans * bank.interest_rate;
        let deposit_accrual = bank.total_deposits * bank.deposit_interest_rate;
        let operating_costs = (bank.own_capital + bank.total_deposits) * 0.015;
        bank.own_capital += loan_accrual - deposit_accrual - operating_costs;

        // Fractional-reserve credit capacity.
        let max_new_credit = if bank.liquid_reserves >= required_reserves {
            let capacity = bank.total_deposits - required_reserves - bank.issued_loans;
            capacity.max(0.0)
        } else {
            0.0
        };

        bank.last_new_credit = max_new_credit;
        bank.issued_loans += max_new_credit;

        // Condition update based on post-credit leverage.
        let leverage = bank.issued_loans / bank.own_capital.max(1.0);
        bank.condition = if leverage < MAX_LEVERAGE {
            if leverage < MAX_LEVERAGE * 0.5 {
                "Excellent".to_string()
            } else {
                "Good".to_string()
            }
        } else {
            "Endangered".to_string()
        };
    }
}
