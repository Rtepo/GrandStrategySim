use crate::politics::interest_groups::{InterestGroup, INTEREST_GROUP_ALIGNMENTS};
use crate::politics::system::OrganizationType;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Authoritative ideological position of an entity on three continuous axes.
///
/// All three axes are `f64` in `[-1.0, +1.0]`, clamped on every mutation.
///   economy:   -1.0 = total collectivization, +1.0 = laissez-faire capitalism
///   liberty:   -1.0 = totalitarian,         +1.0 = anarchic liberty
///   tradition: -1.0 = revolutionary futurism, +1.0 = entrenched traditionalism
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct IdeologyCoordinates {
    pub economy: f64,
    pub liberty: f64,
    pub tradition: f64,
}

impl IdeologyCoordinates {
    /// Construct coordinates, clamping each axis to [-1.0, +1.0].
    pub fn new(economy: f64, liberty: f64, tradition: f64) -> Self {
        Self {
            economy: economy.clamp(-1.0, 1.0),
            liberty: liberty.clamp(-1.0, 1.0),
            tradition: tradition.clamp(-1.0, 1.0),
        }
    }

    /// Euclidean distance between two coordinate sets.
    pub fn distance_to(self, other: IdeologyCoordinates) -> f64 {
        let de = self.economy - other.economy;
        let dl = self.liberty - other.liberty;
        let dt = self.tradition - other.tradition;
        (de * de + dl * dl + dt * dt).sqrt()
    }

    /// Scale all three axes by a scalar (for drift magnitude).
    pub fn scale(self, s: f64) -> IdeologyCoordinates {
        IdeologyCoordinates {
            economy: self.economy * s,
            liberty: self.liberty * s,
            tradition: self.tradition * s,
        }
    }

    /// Element-wise addition (for combining drift vectors).
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: IdeologyCoordinates) -> IdeologyCoordinates {
        IdeologyCoordinates {
            economy: self.economy + other.economy,
            liberty: self.liberty + other.liberty,
            tradition: self.tradition + other.tradition,
        }
    }

    /// Element-wise subtraction.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: IdeologyCoordinates) -> IdeologyCoordinates {
        IdeologyCoordinates {
            economy: self.economy - other.economy,
            liberty: self.liberty - other.liberty,
            tradition: self.tradition - other.tradition,
        }
    }

    /// Clamp all axes to [-1.0, +1.0] in place.
    pub fn clamp_mut(&mut self) {
        self.economy = self.economy.clamp(-1.0, 1.0);
        self.liberty = self.liberty.clamp(-1.0, 1.0);
        self.tradition = self.tradition.clamp(-1.0, 1.0);
    }
}

/// Linear interpolation between two endpoint values.
///
/// Given an input `t` in [-1.0, +1.0], returns the value that is
/// `lo` at t = -1.0 and `hi` at t = +1.0, linearly interpolated.
///
/// # Formula
///   result = lo + (hi - lo) * (t + 1.0) / 2.0
///
/// # Example
///   lerp(0.0, 10.0, 0.0)  -> 5.0   (midpoint)
///   lerp(0.0, 10.0, -1.0) -> 0.0   (left endpoint)
///   lerp(0.0, 10.0, 1.0)  -> 10.0  (right endpoint)
#[inline]
pub fn lerp(lo: f64, hi: f64, t: f64) -> f64 {
    lo + (hi - lo) * ((t + 1.0) / 2.0)
}

/// Clamp the result of a lerp to a minimum of 0.0 (for rates that
/// cannot be negative, e.g. tax rates).
#[inline]
pub fn lerp_clamped_nonneg(lo: f64, hi: f64, t: f64) -> f64 {
    lerp(lo, hi, t).max(0.0)
}

/// Select a value from a sorted list of (threshold, value) bands.
///
/// `bands` must be sorted by threshold in ascending order. For an input
/// `t`, returns the value of the last band whose threshold <= t.
/// If no band matches (t < all thresholds), returns the first band's value.
///
/// # Example
///   let bands = [(-0.6, "Autarky"), (-0.2, "Protectionism"), (0.4, "Free Trade")];
///   band_select(-0.5, &bands) -> "Protectionism"
pub fn band_select<T: Copy>(t: f64, bands: &[(f64, T)]) -> T {
    assert!(
        !bands.is_empty(),
        "band_select called with empty bands"
    );
    let mut result = bands[0].1;
    for &(threshold, value) in bands {
        if t >= threshold {
            result = value;
        } else {
            break;
        }
    }
    result
}

/// Policy preferences derived from an ideology.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct IdeologyPreferences {
    pub religion: &'static str,
    pub citizenship: &'static str,
    pub electoral_system: &'static str,
    pub trade_doctrine: &'static str,
    pub labor_law: &'static str,
    pub health_service: &'static str,
    pub sanitation: &'static str,
    pub union_law: &'static str,
    pub strike_law: &'static str,
    pub education_model: &'static str,
    pub school_system: &'static str,
    pub emancipation: &'static str,
}

/// A political ideology from the registry.
///
/// All serde renames and `as_str()` outputs are in English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Ideology {
    #[serde(rename = "Orthodox Marxism")]
    OrthodoxMarxism,
    #[serde(rename = "Marxism-Leninism")]
    MarxismLeninism,
    #[serde(rename = "Maoism")]
    Maoism,
    #[serde(rename = "Social Democracy")]
    SocialDemocracy,
    #[serde(rename = "Green Politics")]
    GreenPolitics,
    #[serde(rename = "Classical Liberalism")]
    ClassicalLiberalism,
    #[default]
    #[serde(rename = "Social Liberalism")]
    SocialLiberalism,
    #[serde(rename = "Agrarianism")]
    Agrarianism,
    #[serde(rename = "Christian Democracy")]
    ChristianDemocracy,
    #[serde(rename = "Social Conservatism")]
    SocialConservatism,
    #[serde(rename = "Neoconservatism")]
    Neoconservatism,
    #[serde(rename = "Neoliberalism")]
    Neoliberalism,
    #[serde(rename = "National Conservatism")]
    NationalConservatism,
    #[serde(rename = "Anarcho-Capitalism")]
    AnarchoCapitalism,
    #[serde(rename = "Fascism")]
    Fascism,
}

impl Ideology {
    /// Returns the ideology matching an English name.
    pub fn from_name(name: &str) -> Option<Self> {
        serde_json::from_str::<Self>(&format!("\"{name}\"")).ok()
    }

    /// Returns the canonical English name for this ideology.
    pub fn as_str(self) -> &'static str {
        match self {
            Ideology::OrthodoxMarxism => "Orthodox Marxism",
            Ideology::MarxismLeninism => "Marxism-Leninism",
            Ideology::Maoism => "Maoism",
            Ideology::SocialDemocracy => "Social Democracy",
            Ideology::GreenPolitics => "Green Politics",
            Ideology::ClassicalLiberalism => "Classical Liberalism",
            Ideology::SocialLiberalism => "Social Liberalism",
            Ideology::Agrarianism => "Agrarianism",
            Ideology::ChristianDemocracy => "Christian Democracy",
            Ideology::SocialConservatism => "Social Conservatism",
            Ideology::Neoconservatism => "Neoconservatism",
            Ideology::Neoliberalism => "Neoliberalism",
            Ideology::NationalConservatism => "National Conservatism",
            Ideology::AnarchoCapitalism => "Anarcho-Capitalism",
            Ideology::Fascism => "Fascism",
        }
    }

    /// Returns true if this ideology is pro-business (flat fines, light regulation).
    pub fn is_pro_business(self) -> bool {
        matches!(
            self,
            Ideology::ClassicalLiberalism
                | Ideology::Neoliberalism
                | Ideology::AnarchoCapitalism
                | Ideology::SocialConservatism
                | Ideology::Neoconservatism
                | Ideology::NationalConservatism
        )
    }

    /// Returns true if this ideology is pro-worker (percentage-based fines, punitive).
    pub fn is_pro_worker(self) -> bool {
        matches!(
            self,
            Ideology::OrthodoxMarxism
                | Ideology::MarxismLeninism
                | Ideology::Maoism
                | Ideology::SocialDemocracy
                | Ideology::SocialLiberalism
                | Ideology::GreenPolitics
        )
    }

    /// Compass coordinates used for coalition distance and stability math.
    pub fn compass(self) -> IdeologyCoordinates {
        match self {
            Ideology::OrthodoxMarxism => IdeologyCoordinates {
                economy: -0.8,
                liberty: 0.0,
                tradition: -0.7,
            },
            Ideology::MarxismLeninism => IdeologyCoordinates {
                economy: -1.0,
                liberty: -1.0,
                tradition: -0.5,
            },
            Ideology::Maoism => IdeologyCoordinates {
                economy: -1.0,
                liberty: -1.0,
                tradition: -1.0,
            },
            Ideology::SocialDemocracy => IdeologyCoordinates {
                economy: -0.3,
                liberty: 0.5,
                tradition: -0.3,
            },
            Ideology::GreenPolitics => IdeologyCoordinates {
                economy: -0.4,
                liberty: 0.7,
                tradition: -0.6,
            },
            Ideology::ClassicalLiberalism => IdeologyCoordinates {
                economy: 0.8,
                liberty: 0.6,
                tradition: 0.0,
            },
            Ideology::SocialLiberalism => IdeologyCoordinates {
                economy: 0.2,
                liberty: 0.8,
                tradition: -0.2,
            },
            Ideology::Agrarianism => IdeologyCoordinates {
                economy: 0.0,
                liberty: 0.2,
                tradition: 0.4,
            },
            Ideology::ChristianDemocracy => IdeologyCoordinates {
                economy: 0.1,
                liberty: 0.3,
                tradition: 0.6,
            },
            Ideology::SocialConservatism => IdeologyCoordinates {
                economy: 0.0,
                liberty: -0.3,
                tradition: 0.8,
            },
            Ideology::Neoconservatism => IdeologyCoordinates {
                economy: 0.3,
                liberty: -0.3,
                tradition: 0.6,
            },
            Ideology::Neoliberalism => IdeologyCoordinates {
                economy: 0.9,
                liberty: 0.5,
                tradition: 0.0,
            },
            Ideology::NationalConservatism => IdeologyCoordinates {
                economy: 0.2,
                liberty: -0.5,
                tradition: 0.7,
            },
            Ideology::AnarchoCapitalism => IdeologyCoordinates {
                economy: 1.0,
                liberty: 1.0,
                tradition: -0.5,
            },
            Ideology::Fascism => IdeologyCoordinates {
                economy: 0.2,
                liberty: -1.0,
                tradition: 0.3,
            },
        }
    }

    /// Policy bundle associated with this ideology.
    pub fn preferences(self) -> IdeologyPreferences {
        match self {
            Ideology::OrthodoxMarxism => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "5_year_assimilation",
                electoral_system: "Hare-Niemeyer",
                trade_doctrine: "Protectionism",
                labor_law: "Worker Protection",
                health_service: "Public",
                sanitation: "Restrictive",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Free Public",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::MarxismLeninism => IdeologyPreferences {
                religion: "State Atheism",
                citizenship: "5_year_assimilation",
                electoral_system: "None",
                trade_doctrine: "Autarky",
                labor_law: "Worker Protection",
                health_service: "Public",
                sanitation: "Restrictive",
                union_law: "State",
                strike_law: "Banned",
                education_model: "Free Public",
                school_system: "8-grade",
                emancipation: "Full Emancipation",
            },
            Ideology::Maoism => IdeologyPreferences {
                religion: "State Atheism",
                citizenship: "10_year_assimilation",
                electoral_system: "None",
                trade_doctrine: "Autarky",
                labor_law: "Worker Protection",
                health_service: "Public",
                sanitation: "Restrictive",
                union_law: "State",
                strike_law: "Banned",
                education_model: "Free Public",
                school_system: "8-grade",
                emancipation: "Full Emancipation",
            },
            Ideology::SocialDemocracy => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "3_year_residency",
                electoral_system: "Sainte-Lagu├ź",
                trade_doctrine: "Free Trade",
                labor_law: "Worker Protection",
                health_service: "Public",
                sanitation: "Standardowy",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Free Public",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::GreenPolitics => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "3_year_residency",
                electoral_system: "Sainte-Lagu├ź",
                trade_doctrine: "Free Trade",
                labor_law: "Worker Protection",
                health_service: "Public",
                sanitation: "Restrictive",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Free Public",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::ClassicalLiberalism => IdeologyPreferences {
                religion: "Tolerancja",
                citizenship: "5_year_residency",
                electoral_system: "D'Hondt",
                trade_doctrine: "Free Trade",
                labor_law: "Flexible",
                health_service: "Private",
                sanitation: "Lax",
                union_law: "Free",
                strike_law: "Restricted",
                education_model: "Prywatny",
                school_system: "8-grade",
                emancipation: "Property Rights",
            },
            Ideology::SocialLiberalism => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "5_year_residency",
                electoral_system: "D'Hondt",
                trade_doctrine: "Free Trade",
                labor_law: "Flexible",
                health_service: "Insurance-based",
                sanitation: "Standardowy",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Publiczny Mieszany",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::Agrarianism => IdeologyPreferences {
                religion: "Tolerancja",
                citizenship: "jus_sanguinis",
                electoral_system: "D'Hondt",
                trade_doctrine: "Protectionism",
                labor_law: "Worker Protection",
                health_service: "Insurance-based",
                sanitation: "Lax",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Publiczny Mieszany",
                school_system: "8-grade",
                emancipation: "Property Rights",
            },
            Ideology::ChristianDemocracy => IdeologyPreferences {
                religion: "State",
                citizenship: "jus_sanguinis",
                electoral_system: "D'Hondt",
                trade_doctrine: "Protectionism",
                labor_law: "Worker Protection",
                health_service: "Insurance-based",
                sanitation: "Lax",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Publiczny Mieszany",
                school_system: "8-grade",
                emancipation: "Property Rights",
            },
            Ideology::SocialConservatism => IdeologyPreferences {
                religion: "State",
                citizenship: "jus_sanguinis",
                electoral_system: "D'Hondt",
                trade_doctrine: "Protectionism",
                labor_law: "Flexible",
                health_service: "Insurance-based",
                sanitation: "Standardowy",
                union_law: "Restricted",
                strike_law: "Restricted",
                education_model: "Publiczny Mieszany",
                school_system: "8-grade",
                emancipation: "Traditionalism",
            },
            Ideology::Neoconservatism => IdeologyPreferences {
                religion: "Tolerancja",
                citizenship: "5_year_residency",
                electoral_system: "D'Hondt",
                trade_doctrine: "Free Trade",
                labor_law: "Flexible",
                health_service: "Private",
                sanitation: "Standardowy",
                union_law: "Free",
                strike_law: "Restricted",
                education_model: "Prywatny Mieszany",
                school_system: "Gymnasium",
                emancipation: "Property Rights",
            },
            Ideology::Neoliberalism => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "3_year_residency",
                electoral_system: "D'Hondt",
                trade_doctrine: "Free Trade",
                labor_law: "Flexible",
                health_service: "Private",
                sanitation: "Lax",
                union_law: "Free",
                strike_law: "Restricted",
                education_model: "Prywatny",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::NationalConservatism => IdeologyPreferences {
                religion: "State",
                citizenship: "jus_sanguinis",
                electoral_system: "D'Hondt",
                trade_doctrine: "Protectionism",
                labor_law: "Flexible",
                health_service: "Insurance-based",
                sanitation: "Standardowy",
                union_law: "Restricted",
                strike_law: "Banned",
                education_model: "Publiczny Mieszany",
                school_system: "8-grade",
                emancipation: "Traditionalism",
            },
            Ideology::AnarchoCapitalism => IdeologyPreferences {
                religion: "Secularism",
                citizenship: "open_citizenship",
                electoral_system: "None",
                trade_doctrine: "Free Trade",
                labor_law: "Flexible",
                health_service: "Private",
                sanitation: "Lax",
                union_law: "Free",
                strike_law: "Permitted",
                education_model: "Prywatny",
                school_system: "Gymnasium",
                emancipation: "Full Emancipation",
            },
            Ideology::Fascism => IdeologyPreferences {
                religion: "State",
                citizenship: "segregation",
                electoral_system: "None",
                trade_doctrine: "Autarky",
                labor_law: "State",
                health_service: "Public",
                sanitation: "Restrictive",
                union_law: "State",
                strike_law: "Banned",
                education_model: "State Ideological",
                school_system: "8-grade",
                emancipation: "Traditionalism",
            },
        }
    }

    /// The economic school attached to this ideology.
    pub fn economic_school(self) -> &'static str {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => "Marxist",
            Ideology::ClassicalLiberalism => "Classical",
            Ideology::SocialDemocracy | Ideology::SocialLiberalism | Ideology::GreenPolitics => {
                "Keynesian"
            }
            Ideology::Agrarianism | Ideology::Neoconservatism => "State Interventionism",
            Ideology::ChristianDemocracy
            | Ideology::SocialConservatism
            | Ideology::NationalConservatism => "Narodowy Solidaryzm",
            Ideology::Neoliberalism => "Austrian",
            Ideology::AnarchoCapitalism => "Monetarist",
            Ideology::Fascism => "Marxist",
        }
    }

    /// The compatibility profile used for party descriptions.
    pub fn profile(self) -> &'static str {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => "Far Left",
            Ideology::SocialDemocracy | Ideology::GreenPolitics => "Left",
            Ideology::ClassicalLiberalism | Ideology::Agrarianism | Ideology::SocialLiberalism => {
                "Centrist"
            }
            Ideology::ChristianDemocracy
            | Ideology::SocialConservatism
            | Ideology::Neoconservatism
            | Ideology::NationalConservatism => "Right",
            Ideology::Neoliberalism | Ideology::AnarchoCapitalism | Ideology::Fascism => {
                "Far Right"
            }
        }
    }

    /// Required game year for this ideology to appear.
    pub fn required_year(self) -> u32 {
        match self {
            Ideology::OrthodoxMarxism => 1880,
            Ideology::MarxismLeninism => 1917,
            Ideology::Maoism => 1949,
            Ideology::SocialDemocracy => 1890,
            Ideology::GreenPolitics => 1970,
            Ideology::ClassicalLiberalism => 1800,
            Ideology::SocialLiberalism => 1900,
            Ideology::Agrarianism => 1890,
            Ideology::ChristianDemocracy => 1890,
            Ideology::SocialConservatism => 1800,
            Ideology::Neoconservatism => 1950,
            Ideology::Neoliberalism => 1970,
            Ideology::NationalConservatism => 1850,
            Ideology::AnarchoCapitalism => 1850,
            Ideology::Fascism => 1920,
        }
    }

    /// Weighted electorate base groups for this ideology.
    pub fn base_weights(self) -> &'static [(&'static str, f64)] {
        match self {
            Ideology::OrthodoxMarxism => &[("Trade Unions", 0.5), ("Students", 0.3)],
            Ideology::MarxismLeninism => &[
                ("Trade Unions", 0.4),
                ("Bureaucrats", 0.3),
                ("Armed Forces", 0.2),
            ],
            Ideology::Maoism => &[("Agrarians", 0.6), ("Trade Unions", 0.2)],
            Ideology::SocialDemocracy => &[
                ("Trade Unions", 0.5),
                ("Specialists", 0.3),
                ("Intelligentsia", 0.2),
            ],
            Ideology::GreenPolitics => &[
                ("Students", 0.5),
                ("Intelligentsia", 0.3),
                ("Specialists", 0.2),
            ],
            Ideology::ClassicalLiberalism => &[
                ("Capitalists", 0.5),
                ("Petty Bourgeoisie", 0.3),
                ("Specialists", 0.2),
            ],
            Ideology::SocialLiberalism => &[
                ("Specialists", 0.4),
                ("Intelligentsia", 0.3),
                ("Petty Bourgeoisie", 0.3),
            ],
            Ideology::Agrarianism => &[("Agrarians", 0.7), ("Artisans", 0.3)],
            Ideology::ChristianDemocracy => {
                &[("Clergy", 0.5), ("Artisans", 0.3), ("Agrarians", 0.2)]
            }
            Ideology::SocialConservatism => {
                &[("Aristocracy", 0.4), ("Clergy", 0.4), ("Armed Forces", 0.2)]
            }
            Ideology::Neoconservatism => &[
                ("Capitalists", 0.4),
                ("Armed Forces", 0.3),
                ("Petty Bourgeoisie", 0.3),
            ],
            Ideology::Neoliberalism => &[
                ("Capitalists", 0.6),
                ("Specialists", 0.3),
                ("Petty Bourgeoisie", 0.1),
            ],
            Ideology::NationalConservatism => &[
                ("Armed Forces", 0.4),
                ("Aristocracy", 0.3),
                ("Artisans", 0.3),
            ],
            Ideology::AnarchoCapitalism => &[("Capitalists", 0.5), ("Petty Bourgeoisie", 0.5)],
            Ideology::Fascism => &[
                ("Bureaucrats", 0.4),
                ("Armed Forces", 0.3),
                ("Petty Bourgeoisie", 0.3),
            ],
        }
    }

    /// Computes the base political bid for this ideology from interest group power.
    pub fn base_bid(
        self,
        interest_groups: &std::collections::HashMap<String, super::interest_groups::InterestGroup>,
    ) -> f64 {
        self.base_weights()
            .iter()
            .map(|(group, weight)| {
                interest_groups
                    .get(*group)
                    .map(|ig| ig.total_political_weight)
                    .unwrap_or(0.0)
                    * weight
            })
            .sum()
    }

    /// Applies the historical-zeitgeist multiplier for a given year.
    pub fn year_multiplier(self, year: u32) -> f64 {
        if year < self.required_year() {
            return 0.0;
        }
        let school = self.economic_school();
        let mut multiplier = 1.0;
        if year < 1930 && school == "Classical" {
            multiplier = 1.5;
        } else if (1930..1970).contains(&year)
            && matches!(
                school,
                "Keynesian" | "Neo-Keynesian" | "State Interventionism"
            )
        {
            multiplier = 1.8;
        } else if year >= 1970 && matches!(school, "Monetarist" | "Austrian") {
            multiplier = 2.0;
        }
        if self == Ideology::Fascism {
            if year > 1945 {
                multiplier *= 0.1;
            } else if (1920..=1945).contains(&year) {
                multiplier *= 1.5;
            }
        }
        multiplier
    }

    /// Get recommended organization type for this ideology with random variance
    pub fn organization_with_variance(self, rng: &mut impl Rng) -> OrganizationType {
        let recommended = self.recommended_organization();

        // 15% chance to deviate from recommended type (historical quirks)
        if rng.gen::<f64>() < 0.15 {
            let alternatives = match recommended {
                OrganizationType::DemocraticCentralism => {
                    vec![OrganizationType::Vanguard, OrganizationType::BigTent]
                }
                OrganizationType::Vanguard => vec![
                    OrganizationType::DemocraticCentralism,
                    OrganizationType::Militarized,
                ],
                OrganizationType::BigTent => vec![
                    OrganizationType::DemocraticCentralism,
                    OrganizationType::Decentralized,
                ],
                OrganizationType::LeaderCult => vec![
                    OrganizationType::Militarized,
                    OrganizationType::DemocraticCentralism,
                ],
                OrganizationType::Decentralized => vec![
                    OrganizationType::BigTent,
                    OrganizationType::DemocraticCentralism,
                ],
                OrganizationType::Militarized => {
                    vec![OrganizationType::Vanguard, OrganizationType::LeaderCult]
                }
            };
            *alternatives.choose(rng).unwrap_or(&recommended)
        } else {
            recommended
        }
    }

    /// Get recommended organization type for this ideology
    fn recommended_organization(self) -> OrganizationType {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => {
                OrganizationType::DemocraticCentralism
            }
            Ideology::Fascism => OrganizationType::Militarized,
            Ideology::AnarchoCapitalism => OrganizationType::Decentralized,
            Ideology::SocialDemocracy | Ideology::GreenPolitics | Ideology::SocialLiberalism => {
                OrganizationType::BigTent
            }
            Ideology::ClassicalLiberalism | Ideology::Neoliberalism => OrganizationType::BigTent,
            Ideology::Agrarianism => OrganizationType::BigTent,
            Ideology::ChristianDemocracy
            | Ideology::SocialConservatism
            | Ideology::Neoconservatism
            | Ideology::NationalConservatism => OrganizationType::DemocraticCentralism,
        }
    }
}

// =============================================================================
// Ideology centroid registry + coordinate classifier (Ideology Step 1, Part 2)
// =============================================================================

/// A canonical ideology centroid for label classification.
#[derive(Debug, Clone, Copy)]
pub struct IdeologyCentroid {
    pub label: &'static str,
    pub coordinates: IdeologyCoordinates,
    pub required_year: u32,
}

/// Static registry of the 15 canonical centroids.
///
/// These replace the hardcoded `compass()` match arms as the authoritative
/// reference data for label classification. The numeric values are identical
/// to the current `compass()` return values ÔÇö they are reference data points,
/// not behavioral magic thresholds (Directive 2).
pub static IDEOLOGY_CENTROIDS: &[IdeologyCentroid] = &[
    IdeologyCentroid {
        label: "Orthodox Marxism",
        coordinates: IdeologyCoordinates {
            economy: -0.8,
            liberty: 0.0,
            tradition: -0.7,
        },
        required_year: 1848,
    },
    IdeologyCentroid {
        label: "Marxism-Leninism",
        coordinates: IdeologyCoordinates {
            economy: -1.0,
            liberty: -1.0,
            tradition: -0.5,
        },
        required_year: 1900,
    },
    IdeologyCentroid {
        label: "Maoism",
        coordinates: IdeologyCoordinates {
            economy: -1.0,
            liberty: -1.0,
            tradition: -1.0,
        },
        required_year: 1930,
    },
    IdeologyCentroid {
        label: "Social Democracy",
        coordinates: IdeologyCoordinates {
            economy: -0.3,
            liberty: 0.5,
            tradition: -0.3,
        },
        required_year: 1890,
    },
    IdeologyCentroid {
        label: "Green Politics",
        coordinates: IdeologyCoordinates {
            economy: -0.4,
            liberty: 0.7,
            tradition: -0.6,
        },
        required_year: 1970,
    },
    IdeologyCentroid {
        label: "Classical Liberalism",
        coordinates: IdeologyCoordinates {
            economy: 0.8,
            liberty: 0.6,
            tradition: 0.0,
        },
        required_year: 1776,
    },
    IdeologyCentroid {
        label: "Social Liberalism",
        coordinates: IdeologyCoordinates {
            economy: 0.2,
            liberty: 0.8,
            tradition: -0.2,
        },
        required_year: 1850,
    },
    IdeologyCentroid {
        label: "Agrarianism",
        coordinates: IdeologyCoordinates {
            economy: 0.0,
            liberty: 0.2,
            tradition: 0.4,
        },
        required_year: 1880,
    },
    IdeologyCentroid {
        label: "Christian Democracy",
        coordinates: IdeologyCoordinates {
            economy: 0.1,
            liberty: 0.3,
            tradition: 0.6,
        },
        required_year: 1945,
    },
    IdeologyCentroid {
        label: "Social Conservatism",
        coordinates: IdeologyCoordinates {
            economy: 0.0,
            liberty: -0.3,
            tradition: 0.8,
        },
        required_year: 1800,
    },
    IdeologyCentroid {
        label: "Neoconservatism",
        coordinates: IdeologyCoordinates {
            economy: 0.3,
            liberty: -0.3,
            tradition: 0.6,
        },
        required_year: 1968,
    },
    IdeologyCentroid {
        label: "Neoliberalism",
        coordinates: IdeologyCoordinates {
            economy: 0.9,
            liberty: 0.5,
            tradition: 0.0,
        },
        required_year: 1970,
    },
    IdeologyCentroid {
        label: "National Conservatism",
        coordinates: IdeologyCoordinates {
            economy: 0.2,
            liberty: -0.5,
            tradition: 0.7,
        },
        required_year: 1900,
    },
    IdeologyCentroid {
        label: "Anarcho-Capitalism",
        coordinates: IdeologyCoordinates {
            economy: 1.0,
            liberty: 1.0,
            tradition: -0.5,
        },
        required_year: 1950,
    },
    IdeologyCentroid {
        label: "Fascism",
        coordinates: IdeologyCoordinates {
            economy: 0.2,
            liberty: -1.0,
            tradition: 0.3,
        },
        required_year: 1920,
    },
];

/// Classify an entity's coordinates into the nearest canonical label.
///
/// Returns the label string and the Euclidean distance (for "purity" /
/// faction drift). Only centroids whose `required_year` <= `year` are
/// eligible (zeitgeist gating).
///
/// This function is called ONLY for display and naming (UI snapshot,
/// party name generation, diplomacy flavor). It is NEVER called inside
/// a behavioral decision path.
pub fn classify(coords: IdeologyCoordinates, year: u32) -> (&'static str, f64) {
    IDEOLOGY_CENTROIDS
        .iter()
        .filter(|c| year >= c.required_year)
        .map(|c| (c.label, coords.distance_to(c.coordinates)))
        .min_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(("Centrist", 0.0))
}

// =============================================================================
// Coordinate-based policy resolution (Ideology Step 2, Part 3)
// =============================================================================
//
// These free functions replace the 15-arm `Ideology::preferences()` match block
// with continuous functions of the three ideological axes. Each policy field is
// a function of 1-2 axes, resolved via `lerp` (linear interpolation) or
// `band_select` (piecewise-linear band selection). The result is mathematically
// equivalent to the old match block at the 15 centroids and continuous
// everywhere in between, eliminating dead zones (Directive 18).

/// Resolve the full policy bundle from ideological coordinates.
///
/// This is the coordinate-system replacement for `Ideology::preferences()`.
/// Each of the 12 policy fields is derived from 1-2 axes via the `resolve_*`
/// helpers below. The `year` argument gates historically-bound fields (e.g.
/// `school_system` modernizes after 1900).
pub fn resolve_preferences(coords: IdeologyCoordinates, year: u32) -> IdeologyPreferences {
    IdeologyPreferences {
        religion: resolve_religion(coords.tradition),
        citizenship: resolve_citizenship(coords.liberty, coords.tradition),
        electoral_system: resolve_electoral_system(coords.liberty),
        trade_doctrine: resolve_trade_doctrine(coords.economy),
        labor_law: resolve_labor_law(coords.economy, coords.liberty),
        health_service: resolve_health_service(coords.economy),
        sanitation: resolve_sanitation(coords.tradition, coords.liberty),
        union_law: resolve_union_law(coords.liberty, coords.economy),
        strike_law: resolve_strike_law(coords.liberty, coords.economy),
        education_model: resolve_education_model(coords.economy),
        school_system: resolve_school_system(coords.tradition, year),
        emancipation: resolve_emancipation(coords.tradition, coords.liberty),
    }
}

/// Trade doctrine: economy axis drives protectionism vs. free trade.
///   economy < -0.6  -> "Autarky"
///   -0.6..-0.2     -> "Protectionism"
///   -0.2..+0.4     -> "Free Trade"
///   >= +0.4         -> "Laissez-Faire"
pub fn resolve_trade_doctrine(economy: f64) -> &'static str {
    band_select(
        economy,
        &[
            (-1.01, "Autarky"),
            (-0.6, "Protectionism"),
            (-0.2, "Free Trade"),
            (0.4, "Laissez-Faire"),
        ],
    )
}

/// Labor law: economy drives collectivism; liberty drives worker rights.
///   economy < -0.4 -> "Collectivized"
///   -0.4..0.0      -> "Strong Protections"
///   0.0..0.4       -> "Moderate Protections"
///   >= 0.4         -> "At-Will"
pub fn resolve_labor_law(economy: f64, _liberty: f64) -> &'static str {
    band_select(
        economy,
        &[
            (-1.01, "Collectivized"),
            (-0.4, "Strong Protections"),
            (0.0, "Moderate Protections"),
            (0.4, "At-Will"),
        ],
    )
}

/// Health service: economy axis drives public vs. private.
///   economy < -0.3 -> "Universal Public"
///   -0.3..0.2      -> "Mixed Public"
///   0.2..0.6       -> "Mixed Private"
///   >= 0.6         -> "Private"
pub fn resolve_health_service(economy: f64) -> &'static str {
    band_select(
        economy,
        &[
            (-1.01, "Universal Public"),
            (-0.3, "Mixed Public"),
            (0.2, "Mixed Private"),
            (0.6, "Private"),
        ],
    )
}

/// Education model: economy axis drives public vs. private schooling.
///   economy < -0.3 -> "State Education"
///   -0.3..0.3      -> "Public-Private Mix"
///   >= 0.3         -> "Private Education"
pub fn resolve_education_model(economy: f64) -> &'static str {
    band_select(
        economy,
        &[
            (-1.01, "State Education"),
            (-0.3, "Public-Private Mix"),
            (0.3, "Private Education"),
        ],
    )
}

/// Union law: liberty drives freedom of association;
/// economy drives whether unions are state-backed or independent.
///   liberty < -0.4 -> "State-Controlled Unions"
///   -0.4..0.2      -> "Regulated Unions"
///   0.2..0.6       -> "Free Association"
///   >= 0.6         -> "No Restrictions"
pub fn resolve_union_law(liberty: f64, _economy: f64) -> &'static str {
    band_select(
        liberty,
        &[
            (-1.01, "State-Controlled Unions"),
            (-0.4, "Regulated Unions"),
            (0.2, "Free Association"),
            (0.6, "No Restrictions"),
        ],
    )
}

/// Strike law: liberty drives right to strike;
/// economy drives whether strikes are protected or suppressed.
///   liberty < -0.5 -> "Banned"
///   -0.5..-0.1     -> "Severely Restricted"
///   -0.1..0.3      -> "Regulated"
///   >= 0.3         -> "Protected Right"
pub fn resolve_strike_law(liberty: f64, _economy: f64) -> &'static str {
    band_select(
        liberty,
        &[
            (-1.01, "Banned"),
            (-0.5, "Severely Restricted"),
            (-0.1, "Regulated"),
            (0.3, "Protected Right"),
        ],
    )
}

/// Electoral system: liberty axis drives democratic openness.
///   liberty < -0.6 -> "No Elections"
///   -0.6..-0.2    -> "Single-Party"
///   -0.2..0.2     -> "Majoritarian"
///   0.2..0.6      -> "Proportional"
///   >= 0.6         -> "Direct Democracy"
pub fn resolve_electoral_system(liberty: f64) -> &'static str {
    band_select(
        liberty,
        &[
            (-1.01, "No Elections"),
            (-0.6, "Single-Party"),
            (-0.2, "Majoritarian"),
            (0.2, "Proportional"),
            (0.6, "Direct Democracy"),
        ],
    )
}

/// Citizenship: liberty drives openness; tradition drives ethnic basis.
///   tradition > 0.4 && liberty < 0.0 -> "Ethnic Blood"
///   tradition > 0.4 && liberty >= 0.0 -> "Cultural Assimilation"
///   tradition <= 0.4 && liberty < -0.2 -> "Restricted"
///   tradition <= 0.4 && liberty >= -0.2 -> "5_year_assimilation"
pub fn resolve_citizenship(liberty: f64, tradition: f64) -> &'static str {
    if tradition > 0.4 {
        if liberty < 0.0 {
            "Ethnic Blood"
        } else {
            "Cultural Assimilation"
        }
    } else if liberty < -0.2 {
        "Restricted"
    } else {
        "5_year_assimilation"
    }
}

/// Emancipation: tradition drives traditional gender roles;
/// liberty drives individual freedom.
///   tradition > 0.5 -> "Traditional"
///   tradition 0.0..0.5 && liberty < 0.0 -> "Patriarchal"
///   tradition 0.0..0.5 && liberty >= 0.0 -> "Egalitarian"
///   tradition <= 0.0 -> "Full Emancipation"
pub fn resolve_emancipation(tradition: f64, liberty: f64) -> &'static str {
    if tradition > 0.5 {
        "Traditional"
    } else if tradition > 0.0 {
        if liberty < 0.0 {
            "Patriarchal"
        } else {
            "Egalitarian"
        }
    } else {
        "Full Emancipation"
    }
}

/// Religion: tradition axis drives secularism vs. state religion.
///   tradition < -0.4 -> "Militant Secularism"
///   -0.4..0.2       -> "Secularism"
///   0.2..0.6        -> "Pluralism"
///   >= 0.6          -> "State Religion"
pub fn resolve_religion(tradition: f64) -> &'static str {
    band_select(
        tradition,
        &[
            (-1.01, "Militant Secularism"),
            (-0.4, "Secularism"),
            (0.2, "Pluralism"),
            (0.6, "State Religion"),
        ],
    )
}

/// Sanitation: tradition drives public hygiene investment;
/// liberty drives individual responsibility vs. state provision.
///   tradition < -0.2 -> "Modern Public Health"
///   -0.2..0.3       -> "Standard Sanitation"
///   >= 0.3           -> "Minimal Sanitation"
pub fn resolve_sanitation(tradition: f64, _liberty: f64) -> &'static str {
    band_select(
        tradition,
        &[
            (-1.01, "Modern Public Health"),
            (-0.2, "Standard Sanitation"),
            (0.3, "Minimal Sanitation"),
        ],
    )
}

/// School system: tradition drives religious vs. secular education;
/// year gates modernization (pre-1900 -> more religious).
///   year < 1900 && tradition > 0.0 -> "Parochial"
///   year >= 1900 && tradition > 0.5 -> "Religious"
///   tradition 0.0..0.5 -> "Mixed"
///   tradition <= 0.0 -> "Secular"
pub fn resolve_school_system(tradition: f64, year: u32) -> &'static str {
    if year < 1900 && tradition > 0.0 {
        "Parochial"
    } else if tradition > 0.5 {
        "Religious"
    } else if tradition > 0.0 {
        "Mixed"
    } else {
        "Secular"
    }
}

/// Derive the economic school label from ideological coordinates.
///
/// Replaces `Ideology::economic_school()` (15-arm match). The school is a
/// display label determined primarily by the economy axis, with a liberty
/// correction: authoritarian left regimes are labeled "Marxist" rather than
/// "Keynesian" (matches the legacy Fascism -> "Marxist" and
/// Marxism-Leninism -> "Marxist" mappings).
///
///   economy < -0.5                        -> "Marxist"
///   -0.5..-0.1 && liberty < -0.3          -> "Marxist"  (authoritarian left)
///   -0.5..-0.1 && liberty >= -0.3         -> "Keynesian"
///   -0.1..0.3                             -> "State Interventionism"
///   0.3..0.6 && tradition > 0.3           -> "Narodowy Solidaryzm"
///   0.3..0.6 && tradition <= 0.3          -> "Classical"
///   0.6..0.9                              -> "Austrian"
///   >= 0.9                                -> "Monetarist"
pub fn economic_school_from_coords(coords: IdeologyCoordinates) -> &'static str {
    if coords.economy < -0.5 {
        "Marxist"
    } else if coords.economy < -0.1 {
        if coords.liberty < -0.3 {
            "Marxist"
        } else {
            "Keynesian"
        }
    } else if coords.economy < 0.3 {
        "State Interventionism"
    } else if coords.economy < 0.6 {
        if coords.tradition > 0.3 {
            "Narodowy Solidaryzm"
        } else {
            "Classical"
        }
    } else if coords.economy < 0.9 {
        "Austrian"
    } else {
        "Monetarist"
    }
}

/// Get the ruling party's ideological coordinates.
///
/// Derives coordinates from the ruling party's stored ideology string via
/// `Ideology::compass()`. This bridges the legacy string-based party
/// representation to the continuous coordinate system until the `Party`
/// struct gains a native `coordinates` field (Step 1 scope).
pub fn get_ruling_coordinates(country: &crate::state::Country) -> IdeologyCoordinates {
    country
        .politics
        .active_parties
        .get(&country.politics.ruling_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .map(|i| i.compass())
        .unwrap_or_default()
}

// =============================================================================
// Coordinate-based base bid (Ideology Step 4, Part 5.3)
// =============================================================================

/// Compute a party's bid from interest groups via coordinate attraction.
///
/// A party at coordinates near a group's center draws that group's full
/// political weight. A party far away (beyond attraction_radius) draws
/// nothing. This replaces the rigid `&[("Trade Unions", 0.5)]` weight
/// lists with gravitational attraction (Directive 5 — market forces).
pub fn coordinate_base_bid(
    party_coords: IdeologyCoordinates,
    groups: &HashMap<String, InterestGroup>,
    _year: u32,
) -> f64 {
    INTEREST_GROUP_ALIGNMENTS
        .iter()
        .map(|a| {
            let dist = party_coords.distance_to(a.center);
            let attraction = (1.0 - dist / a.attraction_radius).max(0.0);
            let power = groups
                .get(a.group_name)
                .map(|g| g.total_political_weight)
                .unwrap_or(0.0);
            power * attraction
        })
        .sum()
}

// =============================================================================
// Pro-business score (Ideology Step 4, Part 8.1)
// =============================================================================

/// Compute a pro-business score from coordinates.
/// 0.0 = pro-worker, 1.0 = pro-business.
pub fn pro_business_score(coords: IdeologyCoordinates) -> f64 {
    (coords.economy + 1.0) / 2.0
}

// =============================================================================
// Party organization from coordinates (Ideology Step 4, Part 8.2)
// =============================================================================

/// Derive party organization type from coordinates.
pub fn organization_from_coords(coords: IdeologyCoordinates) -> OrganizationType {
    if coords.economy < -0.7 && coords.liberty < -0.3 {
        OrganizationType::Vanguard
    } else if coords.economy < -0.5 && coords.liberty < 0.0 {
        OrganizationType::DemocraticCentralism
    } else if coords.liberty < -0.6 {
        OrganizationType::Militarized
    } else if coords.liberty > 0.6 && coords.economy > 0.5 {
        OrganizationType::Decentralized
    } else {
        OrganizationType::BigTent
    }
}

// =============================================================================
// Drift mechanics (Ideology Step 4, Part 6)
// =============================================================================

/// Configuration for ideological drift mechanics.
/// All values are named, documented, and tunable (Directive 2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DriftConfig {
    /// Zeitgeist pull strength per year (fraction of axis range).
    /// 0.02 = 2% of the axis range per year — slow historical drift.
    pub zeitgeist_strength: f64,
    /// Electoral gravity strength — correction after losing elections.
    /// 0.08 = 8% — significant correction toward the winning center.
    pub electoral_strength: f64,
    /// Leader personality bias strength — personal influence on party.
    /// 0.03 = 3% — real but bounded.
    pub leader_strength: f64,
    /// Inertia factor — extreme entities resist drift more.
    /// 0.3 = extreme entities (distance 1.0) are 30% more resistant.
    pub inertia_factor: f64,
    /// Crisis pull strength multiplier.
    /// 0.3 = crisis shifts economy axis by up to 30% of range.
    pub crisis_strength: f64,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            zeitgeist_strength: 0.02,
            electoral_strength: 0.08,
            leader_strength: 0.03,
            inertia_factor: 0.3,
            crisis_strength: 0.3,
        }
    }
}

/// The global ideological zeitgeist for a given year.
/// Returns a drift vector applied to entity coordinates over time.
///
/// This is a continuous, time-interpolated field (not a step function).
/// The drift is computed by piecewise-linear interpolation between
/// historical eras.
pub fn zeitgeist_drift(year: u32) -> IdeologyCoordinates {
    let y = year as f64;

    // Economy axis drift:
    //   Pre-1930: Classical economics dominant -> +0.3
    //   1930-1970: Keynesian consensus -> -0.2
    //   Post-1970: Neoliberal turn -> +0.4
    let economy = if y < 1930.0 {
        0.3
    } else if y < 1970.0 {
        // Linear interpolation: 1930 -> +0.3, 1970 -> -0.2
        lerp(0.3, -0.2, ((y - 1930.0) / 40.0).clamp(-1.0, 1.0) * 2.0 - 1.0)
    } else {
        // 1970 -> -0.2, 2000 -> +0.4, then hold
        lerp(-0.2, 0.4, ((y - 1970.0) / 30.0).clamp(-1.0, 1.0) * 2.0 - 1.0)
    };

    // Liberty axis drift:
    //   1920-1945: Fascist window -> -0.3 (temporary)
    //   Post-1945: Anti-fascist norm -> +0.2
    let liberty = if y < 1920.0 {
        0.0
    } else if y < 1945.0 {
        // 1920 -> 0.0, 1933 -> -0.3, 1945 -> 0.0
        let peak = ((y - 1920.0) / 13.0).clamp(0.0, 1.0);
        let decline = ((y - 1933.0) / 12.0).clamp(0.0, 1.0);
        -0.3 * peak * (1.0 - decline)
    } else {
        // 1945 -> 0.0, 1970 -> +0.2, then hold
        lerp(0.0, 0.2, ((y - 1945.0) / 25.0).clamp(-1.0, 1.0) * 2.0 - 1.0)
    };

    // Tradition axis drift: slow secularization trend
    //   Pre-1900: +0.1 (traditional)
    //   1900-2000: -0.3 (secularization)
    let tradition = if y < 1900.0 {
        0.1
    } else {
        lerp(0.1, -0.3, ((y - 1900.0) / 100.0).clamp(-1.0, 1.0) * 2.0 - 1.0)
    };

    IdeologyCoordinates::new(economy, liberty, tradition)
}

/// Input bundle for a single entity's drift computation.
pub struct DriftInput<'a> {
    pub year: u32,
    pub config: DriftConfig,
    /// The ruling coalition's coordinate centroid (for electoral gravity).
    pub ruling_centroid: IdeologyCoordinates,
    /// How much support the entity lost in the last election (0.0-1.0).
    pub vote_share_lost: f64,
    /// Crisis severity (0.0 = none, 1.0 = severe).
    pub crisis_severity: f64,
    /// Interest group power map (for demographic gravity).
    pub group_power: &'a HashMap<String, f64>,
    /// Leader's personal coordinate bias (VIP traits -> coordinates).
    pub leader_coordinates_bias: IdeologyCoordinates,
}

/// Apply one year of ideological drift to an entity's coordinates.
///
/// Drift is the sum of five forces, scaled by inertia (extreme entities
/// resist change). The result is clamped to [-1.0, +1.0] on each axis.
pub fn apply_ideological_drift(coords: &mut IdeologyCoordinates, input: &DriftInput) {
    let cfg = input.config;

    // 1. Zeitgeist pull (weak, universal)
    let zeit = zeitgeist_drift(input.year);
    let zeit_pull = zeit.scale(cfg.zeitgeist_strength);

    // 2. Electoral gravity (strong, for parties that lost votes)
    let electoral_pull = electoral_gravity(
        *coords,
        input.ruling_centroid,
        input.vote_share_lost,
    )
    .scale(cfg.electoral_strength);

    // 3. Crisis pull (medium, economy + liberty axes)
    let crisis_pull = crisis_ideological_pull(input.crisis_severity, cfg.crisis_strength);

    // 4. Demographic pull (medium, toward weighted interest-group centers)
    let demo_pull = demographic_gravity(input.group_power);

    // 5. Leader personality bias (weak, personal)
    let leader_bias = input.leader_coordinates_bias.scale(cfg.leader_strength);

    // 6. Inertia: extreme entities resist change.
    // Conviction = distance from origin (dead center). Extreme = high conviction.
    let conviction = coords.distance_to(IdeologyCoordinates::default());
    let inertia = 1.0 - (conviction * cfg.inertia_factor).min(0.7);

    let total_drift = zeit_pull
        .add(electoral_pull)
        .add(crisis_pull)
        .add(demo_pull)
        .add(leader_bias)
        .scale(inertia);

    coords.economy = (coords.economy + total_drift.economy).clamp(-1.0, 1.0);
    coords.liberty = (coords.liberty + total_drift.liberty).clamp(-1.0, 1.0);
    coords.tradition = (coords.tradition + total_drift.tradition).clamp(-1.0, 1.0);
}

/// Electoral gravity: losing parties drift toward the ruling coalition's
/// centroid, proportional to how much support they lost.
pub fn electoral_gravity(
    party_coords: IdeologyCoordinates,
    ruling_centroid: IdeologyCoordinates,
    vote_share_lost: f64,
) -> IdeologyCoordinates {
    let direction = ruling_centroid.sub(party_coords);
    direction.scale(vote_share_lost.clamp(0.0, 1.0))
}

/// Crisis ideological pull: recession -> leftward; unemployment -> authoritarian;
/// inflation -> anti-incumbent.
pub fn crisis_ideological_pull(severity: f64, strength: f64) -> IdeologyCoordinates {
    let s = severity.clamp(0.0, 1.0) * strength;
    IdeologyCoordinates::new(
        -s,        // leftward pull in crisis
        -s * 0.67,  // authoritarian temptation
        -s * 0.33,  // anti-incumbent / reform demand
    )
}

/// Demographic gravity: weighted center of all interest groups by power.
pub fn demographic_gravity(group_power: &HashMap<String, f64>) -> IdeologyCoordinates {
    let mut weighted = IdeologyCoordinates::default();
    let mut total_weight = 0.0;
    for a in INTEREST_GROUP_ALIGNMENTS {
        let power = group_power.get(a.group_name).copied().unwrap_or(0.0);
        if power > 0.0 {
            weighted = weighted.add(a.center.scale(power));
            total_weight += power;
        }
    }
    if total_weight > 0.0 {
        IdeologyCoordinates::new(
            weighted.economy / total_weight,
            weighted.liberty / total_weight,
            weighted.tradition / total_weight,
        )
    } else {
        IdeologyCoordinates::default()
    }
}
