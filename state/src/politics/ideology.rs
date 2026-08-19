use serde::{Deserialize, Serialize};
use rand::Rng;
use rand::seq::SliceRandom;
use crate::politics::system::OrganizationType;

/// Phase 35: Maps old Polish ideology names to new English names for
/// backward compatibility with saves created before Phase 35.
fn polish_to_english(name: &str) -> Option<&'static str> {
    match name {
        "Ortodoksyjny Marksizm" => Some("Orthodox Marxism"),
        "Marksizm-Leninizm" => Some("Marxism-Leninism"),
        "Maoizm" => Some("Maoism"),
        "Socjaldemokracja" => Some("Social Democracy"),
        "Zielona Polityka" => Some("Green Politics"),
        "Klasyczny Liberalizm" => Some("Classical Liberalism"),
        "Socjalliberalizm" => Some("Social Liberalism"),
        "Agraryzm" => Some("Agrarianism"),
        "Chrześcijańska Demokracja" => Some("Christian Democracy"),
        "Konserwatyzm Społeczny" => Some("Social Conservatism"),
        "Neokonserwatyzm" => Some("Neoconservatism"),
        "Neoliberalizm" => Some("Neoliberalism"),
        "Konserwatyzm Narodowy" => Some("National Conservatism"),
        "Anarchokapitalizm" => Some("Anarcho-Capitalism"),
        "Faszyzm" => Some("Fascism"),
        _ => None,
    }
}

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

/// A political ideology from the Python `IDEOLOGIES` registry.
///
/// Phase 35: All serde renames and `as_str()` outputs are now in English.
/// A `polish_to_english` fallback in `from_name` ensures old saves with
/// Polish ideology names still load correctly.
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
    /// Returns the ideology matching an English (or legacy Polish) name.
    /// Phase 35: Tries English serde rename first, then falls back to
    /// Polish-to-English mapping for backward compatibility with old saves.
    pub fn from_name(name: &str) -> Option<Self> {
        // Try English serde rename first
        if let Ok(ideology) = serde_json::from_str::<Self>(&format!("\"{name}\"")) {
            return Some(ideology);
        }
        // Phase 35: Polish fallback for old saves
        polish_to_english(name).and_then(|en| serde_json::from_str(&format!("\"{en}\"")).ok())
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
            Ideology::OrthodoxMarxism => IdeologyCompass { economy: -0.8, liberty: 0.0, tradition: -0.7 },
            Ideology::MarxismLeninism => IdeologyCompass { economy: -1.0, liberty: -1.0, tradition: -0.5 },
            Ideology::Maoism => IdeologyCompass { economy: -1.0, liberty: -1.0, tradition: -1.0 },
            Ideology::SocialDemocracy => IdeologyCompass { economy: -0.3, liberty: 0.5, tradition: -0.3 },
            Ideology::GreenPolitics => IdeologyCompass { economy: -0.4, liberty: 0.7, tradition: -0.6 },
            Ideology::ClassicalLiberalism => IdeologyCompass { economy: 0.8, liberty: 0.6, tradition: 0.0 },
            Ideology::SocialLiberalism => IdeologyCompass { economy: 0.2, liberty: 0.8, tradition: -0.2 },
            Ideology::Agrarianism => IdeologyCompass { economy: 0.0, liberty: 0.2, tradition: 0.4 },
            Ideology::ChristianDemocracy => IdeologyCompass { economy: 0.1, liberty: 0.3, tradition: 0.6 },
            Ideology::SocialConservatism => IdeologyCompass { economy: 0.0, liberty: -0.3, tradition: 0.8 },
            Ideology::Neoconservatism => IdeologyCompass { economy: 0.3, liberty: -0.3, tradition: 0.6 },
            Ideology::Neoliberalism => IdeologyCompass { economy: 0.9, liberty: 0.5, tradition: 0.0 },
            Ideology::NationalConservatism => IdeologyCompass { economy: 0.2, liberty: -0.5, tradition: 0.7 },
            Ideology::AnarchoCapitalism => IdeologyCompass { economy: 1.0, liberty: 1.0, tradition: -0.5 },
            Ideology::Fascism => IdeologyCompass { economy: 0.2, liberty: -1.0, tradition: 0.3 },
        }
    }

    /// Policy bundle associated with this ideology.
    pub fn preferences(self) -> IdeologyPreferences {
        match self {
            Ideology::OrthodoxMarxism => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Asymilacja 5 lat", electoral_system: "Hare-Niemeyer",
                trade_doctrine: "Protekcjonizm", labor_law: "Ochrona Pracowników", health_service: "Publiczna",
                sanitation: "Restrykcyjny", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Bezpłatny", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::MarxismLeninism => IdeologyPreferences {
                religion: "Państwowy Ateizm", citizenship: "Asymilacja 5 lat", electoral_system: "Brak",
                trade_doctrine: "Autarkia", labor_law: "Ochrona Pracowników", health_service: "Publiczna",
                sanitation: "Restrykcyjny", union_law: "Państwowe", strike_law: "Zakazane",
                education_model: "Publiczny Bezpłatny", school_system: "8-klasowy", emancipation: "Pełna Emancypacja",
            },
            Ideology::Maoism => IdeologyPreferences {
                religion: "Państwowy Ateizm", citizenship: "Asymilacja 10 lat", electoral_system: "Brak",
                trade_doctrine: "Autarkia", labor_law: "Ochrona Pracowników", health_service: "Publiczna",
                sanitation: "Restrykcyjny", union_law: "Państwowe", strike_law: "Zakazane",
                education_model: "Publiczny Bezpłatny", school_system: "8-klasowy", emancipation: "Pełna Emancypacja",
            },
            Ideology::SocialDemocracy => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Ziemia 3 lata", electoral_system: "Sainte-Laguë",
                trade_doctrine: "Wolny Handel", labor_law: "Ochrona Pracowników", health_service: "Publiczna",
                sanitation: "Standardowy", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Bezpłatny", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::GreenPolitics => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Ziemia 3 lata", electoral_system: "Sainte-Laguë",
                trade_doctrine: "Wolny Handel", labor_law: "Ochrona Pracowników", health_service: "Publiczna",
                sanitation: "Restrykcyjny", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Bezpłatny", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::ClassicalLiberalism => IdeologyPreferences {
                religion: "Tolerancja", citizenship: "Ziemia 5 lat", electoral_system: "D'Hondt",
                trade_doctrine: "Wolny Handel", labor_law: "Elastyczne", health_service: "Prywatna",
                sanitation: "Luźny", union_law: "Wolne", strike_law: "Ograniczone",
                education_model: "Prywatny", school_system: "8-klasowy", emancipation: "Prawa Majątkowe",
            },
            Ideology::SocialLiberalism => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Ziemia 5 lat", electoral_system: "D'Hondt",
                trade_doctrine: "Wolny Handel", labor_law: "Elastyczne", health_service: "Składkowa",
                sanitation: "Standardowy", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Mieszany", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::Agrarianism => IdeologyPreferences {
                religion: "Tolerancja", citizenship: "Krew", electoral_system: "D'Hondt",
                trade_doctrine: "Protekcjonizm", labor_law: "Ochrona Pracowników", health_service: "Składkowa",
                sanitation: "Luźny", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Mieszany", school_system: "8-klasowy", emancipation: "Prawa Majątkowe",
            },
            Ideology::ChristianDemocracy => IdeologyPreferences {
                religion: "Państwowa", citizenship: "Krew", electoral_system: "D'Hondt",
                trade_doctrine: "Protekcjonizm", labor_law: "Ochrona Pracowników", health_service: "Składkowa",
                sanitation: "Luźny", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Publiczny Mieszany", school_system: "8-klasowy", emancipation: "Prawa Majątkowe",
            },
            Ideology::SocialConservatism => IdeologyPreferences {
                religion: "Państwowa", citizenship: "Krew", electoral_system: "D'Hondt",
                trade_doctrine: "Protekcjonizm", labor_law: "Elastyczne", health_service: "Składkowa",
                sanitation: "Standardowy", union_law: "Ograniczone", strike_law: "Ograniczone",
                education_model: "Publiczny Mieszany", school_system: "8-klasowy", emancipation: "Tradycjonalizm",
            },
            Ideology::Neoconservatism => IdeologyPreferences {
                religion: "Tolerancja", citizenship: "Ziemia 5 lat", electoral_system: "D'Hondt",
                trade_doctrine: "Wolny Handel", labor_law: "Elastyczne", health_service: "Prywatna",
                sanitation: "Standardowy", union_law: "Wolne", strike_law: "Ograniczone",
                education_model: "Prywatny Mieszany", school_system: "Gimnazjalny", emancipation: "Prawa Majątkowe",
            },
            Ideology::Neoliberalism => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Ziemia 3 lata", electoral_system: "D'Hondt",
                trade_doctrine: "Wolny Handel", labor_law: "Elastyczne", health_service: "Prywatna",
                sanitation: "Luźny", union_law: "Wolne", strike_law: "Ograniczone",
                education_model: "Prywatny", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::NationalConservatism => IdeologyPreferences {
                religion: "Państwowa", citizenship: "Krew", electoral_system: "D'Hondt",
                trade_doctrine: "Protekcjonizm", labor_law: "Elastyczne", health_service: "Składkowa",
                sanitation: "Standardowy", union_law: "Ograniczone", strike_law: "Zakazane",
                education_model: "Publiczny Mieszany", school_system: "8-klasowy", emancipation: "Tradycjonalizm",
            },
            Ideology::AnarchoCapitalism => IdeologyPreferences {
                religion: "Laicyzm", citizenship: "Brak", electoral_system: "Brak",
                trade_doctrine: "Wolny Handel", labor_law: "Elastyczne", health_service: "Prywatna",
                sanitation: "Luźny", union_law: "Wolne", strike_law: "Dozwolone",
                education_model: "Prywatny", school_system: "Gimnazjalny", emancipation: "Pełna Emancypacja",
            },
            Ideology::Fascism => IdeologyPreferences {
                religion: "Państwowa", citizenship: "Krew", electoral_system: "Brak",
                trade_doctrine: "Autarkia", labor_law: "Państwowe", health_service: "Publiczna",
                sanitation: "Restrykcyjny", union_law: "Państwowe", strike_law: "Zakazane",
                education_model: "Państwowy Ideologiczny", school_system: "8-klasowy", emancipation: "Tradycjonalizm",
            },
        }
    }

    /// The economic school attached to this ideology.
    pub fn economic_school(self) -> &'static str {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => "Marksistowska",
            Ideology::ClassicalLiberalism => "Klasyczna",
            Ideology::SocialDemocracy | Ideology::SocialLiberalism | Ideology::GreenPolitics => "Keynesowska",
            Ideology::Agrarianism | Ideology::Neoconservatism => "Interwencjonizm Państwowy",
            Ideology::ChristianDemocracy | Ideology::SocialConservatism | Ideology::NationalConservatism => "Narodowy Solidaryzm",
            Ideology::Neoliberalism => "Austriacka",
            Ideology::AnarchoCapitalism => "Monetarystyczna",
            Ideology::Fascism => "Marksistowska",
        }
    }

    /// The compatibility profile used for party descriptions.
    pub fn profile(self) -> &'static str {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => "Skrajna Lewica",
            Ideology::SocialDemocracy | Ideology::GreenPolitics => "Lewica",
            Ideology::ClassicalLiberalism | Ideology::Agrarianism | Ideology::SocialLiberalism => "Centrum",
            Ideology::ChristianDemocracy | Ideology::SocialConservatism | Ideology::Neoconservatism | Ideology::NationalConservatism => "Prawica",
            Ideology::Neoliberalism | Ideology::AnarchoCapitalism | Ideology::Fascism => "Skrajna Prawica",
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
            Ideology::OrthodoxMarxism => &[("Związki Zawodowe", 0.5), ("Studenci", 0.3)],
            Ideology::MarxismLeninism => &[("Związki Zawodowe", 0.4), ("Biurokraci", 0.3), ("Siły Zbrojne", 0.2)],
            Ideology::Maoism => &[("Agrykolanie", 0.6), ("Związki Zawodowe", 0.2)],
            Ideology::SocialDemocracy => &[("Związki Zawodowe", 0.5), ("Specjaliści", 0.3), ("Inteligencja", 0.2)],
            Ideology::GreenPolitics => &[("Studenci", 0.5), ("Inteligencja", 0.3), ("Specjaliści", 0.2)],
            Ideology::ClassicalLiberalism => &[("Kapitaliści", 0.5), ("Drobna Burżuazja", 0.3), ("Specjaliści", 0.2)],
            Ideology::SocialLiberalism => &[("Specjaliści", 0.4), ("Inteligencja", 0.3), ("Drobna Burżuazja", 0.3)],
            Ideology::Agrarianism => &[("Agrykolanie", 0.7), ("Rzemieślnicy", 0.3)],
            Ideology::ChristianDemocracy => &[("Duchowieństwo", 0.5), ("Rzemieślnicy", 0.3), ("Agrykolanie", 0.2)],
            Ideology::SocialConservatism => &[("Arystokracja", 0.4), ("Duchowieństwo", 0.4), ("Siły Zbrojne", 0.2)],
            Ideology::Neoconservatism => &[("Kapitaliści", 0.4), ("Siły Zbrojne", 0.3), ("Drobna Burżuazja", 0.3)],
            Ideology::Neoliberalism => &[("Kapitaliści", 0.6), ("Specjaliści", 0.3), ("Drobna Burżuazja", 0.1)],
            Ideology::NationalConservatism => &[("Siły Zbrojne", 0.4), ("Arystokracja", 0.3), ("Rzemieślnicy", 0.3)],
            Ideology::AnarchoCapitalism => &[("Kapitaliści", 0.5), ("Drobna Burżuazja", 0.5)],
            Ideology::Fascism => &[("Biurokraci", 0.4), ("Siły Zbrojne", 0.3), ("Drobna Burżuazja", 0.3)],
        }
    }

    /// Computes the base political bid for this ideology from interest group power.
    pub fn base_bid(self, interest_groups: &std::collections::HashMap<String, super::interest_groups::InterestGroup>) -> f64 {
        self.base_weights()
            .iter()
            .map(|(group, weight)| interest_groups.get(*group).map(|ig| ig.total_political_weight).unwrap_or(0.0) * weight)
            .sum()
    }

    /// Applies the historical-zeitgeist multiplier for a given year.
    pub fn year_multiplier(self, year: u32) -> f64 {
        if year < self.required_year() {
            return 0.0;
        }
        let school = self.economic_school();
        let mut multiplier = 1.0;
        if year < 1930 && school == "Klasyczna" {
            multiplier = 1.5;
        } else if (1930..1970).contains(&year) && matches!(school, "Keynesowska" | "Neo-Keynesowska" | "Interwencjonizm Państwowy") {
            multiplier = 1.8;
        } else if year >= 1970 && matches!(school, "Monetarystyczna" | "Austriacka") {
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
                OrganizationType::DemocraticCentralism => vec![OrganizationType::Vanguard, OrganizationType::BigTent],
                OrganizationType::Vanguard => vec![OrganizationType::DemocraticCentralism, OrganizationType::Militarized],
                OrganizationType::BigTent => vec![OrganizationType::DemocraticCentralism, OrganizationType::Decentralized],
                OrganizationType::LeaderCult => vec![OrganizationType::Militarized, OrganizationType::DemocraticCentralism],
                OrganizationType::Decentralized => vec![OrganizationType::BigTent, OrganizationType::DemocraticCentralism],
                OrganizationType::Militarized => vec![OrganizationType::Vanguard, OrganizationType::LeaderCult],
            };
            *alternatives.choose(rng).unwrap_or(&recommended)
        } else {
            recommended
        }
    }

    /// Get recommended organization type for this ideology
    fn recommended_organization(self) -> OrganizationType {
        match self {
            Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => OrganizationType::DemocraticCentralism,
            Ideology::Fascism => OrganizationType::Militarized,
            Ideology::AnarchoCapitalism => OrganizationType::Decentralized,
            Ideology::SocialDemocracy | Ideology::GreenPolitics | Ideology::SocialLiberalism => OrganizationType::BigTent,
            Ideology::ClassicalLiberalism | Ideology::Neoliberalism => OrganizationType::BigTent,
            Ideology::Agrarianism => OrganizationType::BigTent,
            Ideology::ChristianDemocracy | Ideology::SocialConservatism | Ideology::Neoconservatism | Ideology::NationalConservatism => OrganizationType::DemocraticCentralism,
        }
    }
}
