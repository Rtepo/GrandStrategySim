//! Phase 18C: Free speech, assembly, and press freedom laws.
//!
//! These laws control whether propaganda campaigns can be run, whether hate
//! speech is permitted, and how the Ombudsman reacts to rights violations.

use serde::{Deserialize, Serialize};

/// Level of free speech protection in the country.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FreeSpeechLevel {
    /// Full free speech — hate speech blocked, ombudsman sensitivity increased.
    #[default]
    Full,
    /// Some restrictions on speech but opposition media allowed.
    Restricted,
    /// Suppressed — propaganda campaigns enabled without political cost.
    Suppressed,
    /// Totalitarian — full state control of information, no opposition media.
    Totalitarian,
}

/// Assembly rights level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyRights {
    /// Free assembly — protests and gatherings allowed.
    #[default]
    Free,
    /// Restricted assembly — permits required, size limits.
    Restricted,
    /// Banned — no public gatherings allowed.
    Banned,
}

/// Press freedom level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PressFreedom {
    /// Independent press — private media operates freely.
    #[default]
    Independent,
    /// Mixed — both state and private media exist.
    Mixed,
    /// State-controlled — all media is state-run.
    StateControlled,
}

/// Free speech / assembly / press freedom law configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FreeSpeechLaw {
    /// Level of free speech protection.
    #[serde(default)]
    pub free_speech_level: FreeSpeechLevel,
    /// Assembly rights level.
    #[serde(default)]
    pub assembly_rights: AssemblyRights,
    /// Press freedom level.
    #[serde(default)]
    pub press_freedom: PressFreedom,
}

impl FreeSpeechLaw {
    /// Returns true if hate speech campaigns are allowed.
    ///
    /// # Rules
    /// * `Full` → hate speech blocked.
    /// * `Restricted` → hate speech blocked.
    /// * `Suppressed` → hate speech allowed.
    /// * `Totalitarian` → hate speech allowed.
    pub fn allows_hate_speech(&self) -> bool {
        matches!(
            self.free_speech_level,
            FreeSpeechLevel::Suppressed | FreeSpeechLevel::Totalitarian
        )
    }

    /// Returns true if propaganda campaigns can run without political cost.
    ///
    /// # Rules
    /// * `Suppressed` or `Totalitarian` → no political cost.
    /// * `Full` or `Restricted` → political cost applies.
    pub fn propaganda_without_cost(&self) -> bool {
        matches!(
            self.free_speech_level,
            FreeSpeechLevel::Suppressed | FreeSpeechLevel::Totalitarian
        )
    }

    /// Returns true if the press is fully state-controlled.
    pub fn is_state_controlled_press(&self) -> bool {
        self.press_freedom == PressFreedom::StateControlled
    }

    /// Returns the ombudsman sensitivity multiplier based on free speech level.
    ///
    /// # Rules
    /// * `Full` → 2.0 (doubled sensitivity).
    /// * `Restricted` → 1.0.
    /// * `Suppressed` → 0.5.
    /// * `Totalitarian` → 0.1 (ombudsman effectively neutralized).
    pub fn ombudsman_sensitivity_multiplier(&self) -> f64 {
        match self.free_speech_level {
            FreeSpeechLevel::Full => 2.0,
            FreeSpeechLevel::Restricted => 1.0,
            FreeSpeechLevel::Suppressed => 0.5,
            FreeSpeechLevel::Totalitarian => 0.1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_free_speech_blocks_hate_speech() {
        let law = FreeSpeechLaw::default(); // Full
        assert!(!law.allows_hate_speech());
        assert!(!law.propaganda_without_cost());
        assert_eq!(law.ombudsman_sensitivity_multiplier(), 2.0);
    }

    #[test]
    fn test_suppressed_allows_hate_speech() {
        let law = FreeSpeechLaw {
            free_speech_level: FreeSpeechLevel::Suppressed,
            ..Default::default()
        };
        assert!(law.allows_hate_speech());
        assert!(law.propaganda_without_cost());
        assert_eq!(law.ombudsman_sensitivity_multiplier(), 0.5);
    }

    #[test]
    fn test_totalitarian_neutralizes_ombudsman() {
        let law = FreeSpeechLaw {
            free_speech_level: FreeSpeechLevel::Totalitarian,
            press_freedom: PressFreedom::StateControlled,
            ..Default::default()
        };
        assert!(law.allows_hate_speech());
        assert!(law.is_state_controlled_press());
        assert!((law.ombudsman_sensitivity_multiplier() - 0.1).abs() < 1e-9);
    }
}
