use crate::politics::system::OrganizationType;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A three-dimensional ideological compass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct IdeologyCompass {
    pub economy: f64,
    pub liberty: f64,
    pub tradition: f64,
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
    pub fn compass(self) -> IdeologyCompass {
        match self {
            Ideology::OrthodoxMarxism => IdeologyCompass {
                economy: -0.8,
                liberty: 0.0,
                tradition: -0.7,
            },
            Ideology::MarxismLeninism => IdeologyCompass {
                economy: -1.0,
                liberty: -1.0,
                tradition: -0.5,
            },
            Ideology::Maoism => IdeologyCompass {
                economy: -1.0,
                liberty: -1.0,
                tradition: -1.0,
            },
            Ideology::SocialDemocracy => IdeologyCompass {
                economy: -0.3,
                liberty: 0.5,
                tradition: -0.3,
            },
            Ideology::GreenPolitics => IdeologyCompass {
                economy: -0.4,
                liberty: 0.7,
                tradition: -0.6,
            },
            Ideology::ClassicalLiberalism => IdeologyCompass {
                economy: 0.8,
                liberty: 0.6,
                tradition: 0.0,
            },
            Ideology::SocialLiberalism => IdeologyCompass {
                economy: 0.2,
                liberty: 0.8,
                tradition: -0.2,
            },
            Ideology::Agrarianism => IdeologyCompass {
                economy: 0.0,
                liberty: 0.2,
                tradition: 0.4,
            },
            Ideology::ChristianDemocracy => IdeologyCompass {
                economy: 0.1,
                liberty: 0.3,
                tradition: 0.6,
            },
            Ideology::SocialConservatism => IdeologyCompass {
                economy: 0.0,
                liberty: -0.3,
                tradition: 0.8,
            },
            Ideology::Neoconservatism => IdeologyCompass {
                economy: 0.3,
                liberty: -0.3,
                tradition: 0.6,
            },
            Ideology::Neoliberalism => IdeologyCompass {
                economy: 0.9,
                liberty: 0.5,
                tradition: 0.0,
            },
            Ideology::NationalConservatism => IdeologyCompass {
                economy: 0.2,
                liberty: -0.5,
                tradition: 0.7,
            },
            Ideology::AnarchoCapitalism => IdeologyCompass {
                economy: 1.0,
                liberty: 1.0,
                tradition: -0.5,
            },
            Ideology::Fascism => IdeologyCompass {
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
                electoral_system: "Sainte-Laguë",
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
                electoral_system: "Sainte-Laguë",
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
