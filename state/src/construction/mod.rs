//! Construction system for multi-turn building development.

pub mod bom;
pub mod orders;
pub mod projects;
pub mod tenders;
pub mod tender_market;
pub mod fraud;
pub mod private_inspection;
pub mod upgrade_project;

pub use projects::{ConstructionProject, ConstructionProjectType, ConstructionQueue};
pub use tenders::{
    Bid, ConstructionTender, SubcontractorAssignment, TenderInvestorType, TenderStatus, Tranche,
};
pub use bom::get_network_construction_bom;
pub use upgrade_project::UpgradeProject;
