//! Phase 22D: Private construction inspection.
//!
//! Because state inspectorates may be out-of-range, understaffed, or corrupt,
//! the Investor can hire a `PrivateInspector`. The private inspector always
//! detects the true `structural_defect` and `ohs_coverage_ratio` — the investor
//! pays for thoroughness. The fee routes through `TransferSettler`.

use crate::construction::fraud::MaterialSubstitution;
use serde::{Deserialize, Serialize};

/// A private inspection engagement hired by the investor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PrivateInspection {
    /// Unique inspection ID.
    pub id: String,
    /// Tender ID (for reference).
    pub tender_id: String,
    /// Project ID being inspected.
    pub project_id: String,
    /// Investor company ID (or "STATE:...").
    pub investor_id: String,
    /// Fee paid from investor cash via `settle_transfer`.
    pub fee: f64,
    /// Turn the inspection was hired.
    pub hired_turn: u32,
    /// Inspection report (None until conducted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<InspectionReport>,
}

/// A private inspection report — always accurate (no corruption).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InspectionReport {
    /// Structural defect measured (0.0–1.0).
    pub defects_found: f64,
    /// True if OHS coverage is below compliance.
    pub ohs_violations_found: bool,
    /// OHS coverage ratio measured (0.0–1.0).
    pub ohs_coverage_measured: f64,
    /// Material substitutions detected.
    pub fraud_detected: Vec<MaterialSubstitution>,
    /// Turn the inspection was conducted.
    pub inspected_turn: u32,
}

/// Default private inspection fee multiplier — fee = avg_wage × this value.
/// D.4.3: Replaced hardcoded 25,000.0 with a dynamic multiplier (Rule 2).
pub const DEFAULT_INSPECTION_FEE_WAGE_MULTIPLIER: f64 = 25.0;

/// Defect threshold above which the investor should file a lawsuit.
pub const LAWSUIT_DEFECT_THRESHOLD: f64 = 0.15;

/// Compute the dynamic private inspection fee based on average_wage.
/// D.4.3: Scale fee by average_wage for inflation-proofing (Rule 2).
pub fn default_inspection_fee(avg_wage: f64) -> f64 {
    avg_wage * DEFAULT_INSPECTION_FEE_WAGE_MULTIPLIER
}

/// Hire a private inspector for a project.
///
/// # Arguments
/// * `project_id` - The construction project to inspect.
/// * `investor_id` - The investor hiring the inspector.
/// * `tender_id` - The associated tender (for reference).
/// * `current_turn` - Current turn number.
/// * `avg_wage` - Current average wage for dynamic fee calculation.
///
/// # Returns
/// A new `PrivateInspection` with status "hired, report pending".
pub fn hire_private_inspector(
    project_id: String,
    investor_id: String,
    tender_id: String,
    current_turn: u32,
    avg_wage: f64,
) -> PrivateInspection {
    PrivateInspection {
        id: format!("pinspect_{}_{}", project_id, current_turn),
        tender_id,
        project_id,
        investor_id,
        fee: default_inspection_fee(avg_wage),
        hired_turn: current_turn,
        report: None,
    }
}

/// Conduct the private inspection — always detects true defect and OHS.
///
/// # Arguments
/// * `inspection` - The hired inspection (mutated: report attached).
/// * `structural_defect` - The project's true structural defect.
/// * `ohs_coverage_ratio` - The project's true OHS coverage.
/// * `fraud_history` - Known material substitutions on the project.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `true` if the report found actionable defects (lawsuit-worthy).
pub fn conduct_private_inspection(
    inspection: &mut PrivateInspection,
    structural_defect: f64,
    ohs_coverage_ratio: f64,
    fraud_history: Vec<MaterialSubstitution>,
    current_turn: u32,
) -> bool {
    let ohs_violations = ohs_coverage_ratio < 1.0;
    let report = InspectionReport {
        defects_found: structural_defect,
        ohs_violations_found: ohs_violations,
        ohs_coverage_measured: ohs_coverage_ratio,
        fraud_detected: fraud_history,
        inspected_turn: current_turn,
    };

    let actionable = structural_defect > LAWSUIT_DEFECT_THRESHOLD || ohs_violations;
    inspection.report = Some(report);
    actionable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::fraud::MaterialSubstitution;
    use crate::registries::enums::Commodity;

    #[test]
    fn test_hire_private_inspector() {
        let insp = hire_private_inspector(
            "proj_1".to_string(),
            "inv_1".to_string(),
            "tender_1".to_string(),
            10,
            1000.0,
        );
        assert_eq!(insp.fee, default_inspection_fee(1000.0));
        assert!(insp.report.is_none());
    }

    #[test]
    fn test_conduct_inspection_detects_defects() {
        let mut insp = hire_private_inspector(
            "proj_1".to_string(),
            "inv_1".to_string(),
            "tender_1".to_string(),
            10,
            1000.0,
        );
        let fraud = vec![MaterialSubstitution {
            original_commodity: Commodity::Steel,
            substitute_commodity: Commodity::Timber,
            quantity_substituted: 300.0,
            cash_retained: 180_000.0,
            defect_added: 0.36,
        }];
        let actionable = conduct_private_inspection(&mut insp, 0.5, 0.3, fraud, 12);
        assert!(actionable);
        let report = insp.report.as_ref().unwrap();
        assert!((report.defects_found - 0.5).abs() < 0.01);
        assert!(report.ohs_violations_found);
        assert_eq!(report.fraud_detected.len(), 1);
    }

    #[test]
    fn test_conduct_inspection_clean_project() {
        let mut insp = hire_private_inspector(
            "proj_1".to_string(),
            "inv_1".to_string(),
            "tender_1".to_string(),
            10,
            1000.0,
        );
        let actionable = conduct_private_inspection(&mut insp, 0.0, 1.0, Vec::new(), 12);
        assert!(!actionable);
        let report = insp.report.as_ref().unwrap();
        assert!((report.defects_found - 0.0).abs() < 0.01);
        assert!(!report.ohs_violations_found);
    }
}
