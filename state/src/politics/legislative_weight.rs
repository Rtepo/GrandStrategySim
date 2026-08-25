//! Phase 86: Legislative weight classification for voting majorities.
//!
//! Bills are classified into three weights based on their provisions:
//! - `Ordinary`: Relative majority (>50% of present MPs)
//! - `Organic`: Absolute majority (>50% of total seats)
//! - `Constitutional`: Qualified majority (2/3 of total seats)
//!
//! The weight is derived from the highest-impact provision in the bill.
//! If a bill contains multiple provisions, the heaviest one determines
//! the voting threshold (a bill cannot be split into separate votes).

use serde::{Deserialize, Serialize};

use super::legislation::BillProvision;

/// Legislative weight of a bill, determining the voting majority required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegislativeWeight {
    /// Ordinary legislation — relative majority (>50% of present, non-abstaining MPs).
    #[default]
    Ordinary,
    /// Organic law — absolute majority (>50% of total seats).
    /// Covers structural law changes: tax rates, healthcare, education, justice.
    Organic,
    /// Constitutional law — qualified majority (2/3 of total seats).
    /// Reserved for fundamental structural changes.
    Constitutional,
}

impl LegislativeWeight {
    /// Returns the quorum fraction required for this weight.
    /// Ordinary and Organic require 50% of total seats present.
    /// Constitutional requires 2/3 of total seats present.
    pub fn quorum_fraction(&self) -> f64 {
        match self {
            LegislativeWeight::Ordinary | LegislativeWeight::Organic => 0.50,
            LegislativeWeight::Constitutional => 2.0 / 3.0,
        }
    }

    /// Returns a human-readable label for this weight.
    pub fn as_str(&self) -> &'static str {
        match self {
            LegislativeWeight::Ordinary => "Ordinary (Relative Majority)",
            LegislativeWeight::Organic => "Organic (Absolute Majority)",
            LegislativeWeight::Constitutional => "Constitutional (Qualified Majority)",
        }
    }
}

/// Derive the legislative weight from a bill's provisions.
///
/// The weight is determined by the heaviest provision in the bill:
/// - `ConstitutionalAmendment` → Constitutional
/// - Tax rate changes, healthcare, education, justice, free speech, transport,
///   sentencing, migration law changes → Organic
/// - Price controls, subsidies, deregulation, infrastructure mandates,
///   custom provisions → Ordinary
///
/// If a bill has multiple provisions, the heaviest one wins.
pub fn derive_weight_from_provisions(provisions: &[&BillProvision]) -> LegislativeWeight {
    let mut weight = LegislativeWeight::Ordinary;
    for provision in provisions {
        let p_weight = provision_weight(provision);
        if p_weight as u8 > weight as u8 {
            weight = p_weight;
        }
    }
    weight
}

/// Determine the weight of a single provision.
fn provision_weight(provision: &BillProvision) -> LegislativeWeight {
    match provision {
        // Structural law changes require Organic majority
        BillProvision::TaxRateChange { .. }
        | BillProvision::HealthcareLaw(_)
        | BillProvision::EducationLaw(_)
        | BillProvision::JusticeLaw(_)
        | BillProvision::FreeSpeechLaw(_)
        | BillProvision::TransportLaw(_)
        | BillProvision::SentencingLaw(_)
        | BillProvision::MigrationLawChange(_) => LegislativeWeight::Organic,

        // Routine adjustments use Ordinary majority
        BillProvision::PriceControl { .. }
        | BillProvision::Subsidy { .. }
        | BillProvision::Deregulation { .. }
        | BillProvision::InfrastructureMandate { .. }
        | BillProvision::Custom { .. } => LegislativeWeight::Ordinary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quorum_fraction_ordinary() {
        assert!((LegislativeWeight::Ordinary.quorum_fraction() - 0.50).abs() < 1e-6);
    }

    #[test]
    fn test_quorum_fraction_organic() {
        assert!((LegislativeWeight::Organic.quorum_fraction() - 0.50).abs() < 1e-6);
    }

    #[test]
    fn test_quorum_fraction_constitutional() {
        assert!((LegislativeWeight::Constitutional.quorum_fraction() - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_derive_weight_empty() {
        let weight = derive_weight_from_provisions(&[]);
        assert_eq!(weight, LegislativeWeight::Ordinary);
    }

    #[test]
    fn test_derive_weight_picks_heaviest() {
        use crate::politics::legislation::BillProvision;
        let ordinary = BillProvision::Subsidy {
            target: "steel".to_string(),
            amount_per_unit: 1.0,
        };
        let organic = BillProvision::TaxRateChange {
            income_tax: Some(0.2),
            vat: None,
            corporate_tax: None,
        };
        let provisions: Vec<&BillProvision> = vec![&ordinary, &organic];
        let weight = derive_weight_from_provisions(&provisions);
        assert_eq!(weight, LegislativeWeight::Organic);
    }
}
