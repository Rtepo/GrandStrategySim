//! Corporate simulation — expansion, restructuring and bankruptcy.
//!
//! This module ports the post-production corporate step from the Python
//! `corporate/manager.py` and `corporate/restructuring.py` families.  It
//! operates on the typed [`crate::entities::Company`] and [`crate::entities::Building`] entities after the
//! production cycle has produced their `last_profit` values.

pub mod lifecycle;
pub mod manager;
pub mod strategy;
pub mod unions;
pub mod development;
pub mod bounded_rationality;
pub mod capital_intensity;
pub mod bankruptcy;
pub mod market_behavior;
pub mod mergers;

pub use lifecycle::CompanyLifecycle;
pub use manager::{apply_seasonal_furlough_all, process_companies, process_company, set_wage_offers, process_furlough_reinstatement, process_furlough_attrition};
pub use strategy::{CorporateAction, CorporateDecisionCtx, CorporateStrategy, FinanceSource, IpoStrategy, try_apply_ipo, calculate_administrative_overhead};
pub use unions::process_unions;
pub use development::{PropertyDeveloper, MarketOpportunity, publish_developer_tenders, publish_gas_station_tenders};
pub use bounded_rationality::{InformationQuality, determine_information_quality, try_upgrade_to_predictive, apply_estimation_error};
pub use capital_intensity::{CapitalIntensity, sector_capital_intensity, minimum_capital_for_sector};
pub use bankruptcy::{BankruptcyAuctionPool, RestructuringPlan, Syndic};
pub use market_behavior::{MarketBehaviorModifiers, evaluate_market_behavior};
