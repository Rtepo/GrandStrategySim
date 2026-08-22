//! Government form registry.
//!
//! Faithful port of `GOVERNMENT_FORMS` from `politics/system/forms.py`
//! (11 forms). Drives regime classification, election cycles, and revolution
//! thresholds.

use crate::registries::enums::RegimeType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Static definition of a form of government.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernmentForm {
    /// Broad regime classification (`"typ"`).

    pub regime_type: RegimeType,

    /// Length of an election cycle in years (`"election_cycle"`); `999` denotes
    /// no regular elections.

    pub election_cycle: u32,

    /// Social-unrest level at which revolution triggers (`"prog_rewolucji"`).

    pub revolution_threshold: f64,

    /// Number of legislative chambers (`"izby"`).

    pub chambers: u32,

    /// Title of the head of government (`"szef_rzadu"`).

    pub head_of_government: String,

    /// Title of the head of state (`"glowa_panstwa"`).

    pub head_of_state: String,

    /// Available ideological/structural subtypes (`"podtypy"`).
    #[serde(default)]
    pub subtypes: Vec<String>,
}

/// Convenience constructor for a [`GovernmentForm`] to keep the registry
/// builder terse.
fn form(
    regime_type: RegimeType,
    election_cycle: u32,
    revolution_threshold: f64,
    chambers: u32,
    head_of_government: &str,
    head_of_state: &str,
    subtypes: &[&str],
) -> GovernmentForm {
    GovernmentForm {
        regime_type,
        election_cycle,
        revolution_threshold,
        chambers,
        head_of_government: head_of_government.to_string(),
        head_of_state: head_of_state.to_string(),
        subtypes: subtypes.iter().map(|s| s.to_string()).collect(),
    }
}

/// Builds the government-form registry.
///
/// # Returns
/// A map of `form name -> `[`GovernmentForm`]`, natively encoding
/// `GOVERNMENT_FORMS` from the Python source.
///
/// # Rules
/// * Five democratic forms and six authoritarian forms are defined.
/// * Autocracies use `election_cycle == 999` to signal no regular elections.
pub fn government_forms() -> HashMap<String, GovernmentForm> {
    use RegimeType::{Autocracy, Democracy};
    HashMap::from([
        (
            "Demokracja Parlamentarna".to_string(),
            form(Democracy, 4, 85.0, 2, "Premier", "Prezydent", &[]),
        ),
        (
            "Republika Prezydencka".to_string(),
            form(Democracy, 5, 80.0, 2, "None", "Prezydent", &[]),
        ),
        (
            "Semi-Presidential Republic".to_string(),
            form(Democracy, 5, 82.0, 2, "Premier", "Prezydent", &[]),
        ),
        (
            "Demokracja Dyrektorialna".to_string(),
            form(Democracy, 4, 88.0, 2, "None", "Board of Directors", &[]),
        ),
        (
            "Monarchia Konstytucyjna".to_string(),
            form(Democracy, 4, 88.0, 2, "Premier", "Monarcha", &[]),
        ),
        (
            "Monarchia Dualistyczna".to_string(),
            form(Autocracy, 5, 75.0, 2, "Kanclerz", "Monarcha", &[]),
        ),
        (
            "Monarchia Elekcyjna".to_string(),
            form(
                Autocracy,
                999,
                70.0,
                1,
                "Kanclerz",
                "Monarcha Elekcyjny",
                &["Elekcja Arystokratyczna", "Elekcja Powszechna"],
            ),
        ),
        (
            "Monarchia Absolutna".to_string(),
            form(Autocracy, 999, 60.0, 0, "None", "Monarcha", &[]),
        ),
        (
            "Single-Party State".to_string(),
            form(
                Autocracy,
                999,
                65.0,
                1,
                "Premier",
                "Sekretarz Generalny",
                &["Corporate State", "People's Republic", "Authoritarian Democracy"],
            ),
        ),
        (
            "Dyktatura Wojskowa".to_string(),
            form(Autocracy, 999, 55.0, 0, "None", "General", &[]),
        ),
        (
            "Teokracja".to_string(),
            form(
                Autocracy,
                999,
                70.0,
                1,
                "None",
                "High Priest",
                &["Teokracja Klasyczna", "Republika Religijna"],
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_forms_defined() {
        assert_eq!(government_forms().len(), 11);
    }

    #[test]
    fn parliamentary_democracy_is_democratic() {
        let forms = government_forms();
        let f = &forms["Demokracja Parlamentarna"];
        assert!(f.regime_type.is_democratic());
        assert_eq!(f.election_cycle, 4);
        assert_eq!(f.revolution_threshold, 85.0);
    }

    #[test]
    fn military_dictatorship_is_autocratic() {
        let forms = government_forms();
        let f = &forms["Dyktatura Wojskowa"];
        assert!(!f.regime_type.is_democratic());
        assert_eq!(f.election_cycle, 999);
        assert_eq!(f.chambers, 0);
    }

    #[test]
    fn subtypes_are_captured() {
        let forms = government_forms();
        assert_eq!(forms["Teokracja"].subtypes.len(), 2);
    }
}
