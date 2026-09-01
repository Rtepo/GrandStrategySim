//! Corporate simulation — expansion, restructuring and bankruptcy.
//!
//! This module ports the post-production corporate step from the Python
//! `corporate/manager.py` and `corporate/restructuring.py` families.  It
//! operates on the typed [`crate::entities::Company`] and [`crate::entities::Building`] entities after the
//! production cycle has produced their `last_profit` values.

pub mod bankruptcy;
pub mod bounded_rationality;
pub mod capital_intensity;
pub mod development;
pub mod lifecycle;
pub mod manager;
pub mod market_behavior;
pub mod mergers;
pub mod strategy;
pub mod unions;

pub use bankruptcy::{BankruptcyAuctionPool, RestructuringPlan, Syndic};
pub use bounded_rationality::{
    apply_estimation_error, determine_information_quality, try_upgrade_to_predictive,
    InformationQuality,
};
pub use capital_intensity::{
    minimum_capital_for_sector, sector_capital_intensity, CapitalIntensity,
};
pub use development::{
    publish_developer_tenders, publish_gas_station_tenders, MarketOpportunity, PropertyDeveloper,
};
pub use lifecycle::CompanyLifecycle;
pub use manager::{
    apply_seasonal_furlough_all, process_companies, process_company, process_furlough_attrition,
    process_furlough_reinstatement, set_wage_offers,
};
pub use market_behavior::{evaluate_market_behavior, MarketBehaviorModifiers};
pub use strategy::{
    calculate_administrative_overhead, try_apply_ipo, CorporateAction, CorporateDecisionCtx,
    CorporateStrategy, FinanceSource, IpoStrategy,
};
pub use unions::process_unions;
