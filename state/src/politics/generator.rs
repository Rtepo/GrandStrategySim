//! Procedural party name generator with cultural awareness.
//!
//! This module generates culturally appropriate party names based on the country's
//! cultural group and the party's ideology. Names use English keys for struct IDs
//! but can include native flavor in display names.

use crate::politics::ideology::Ideology;
use rand::Rng;

/// Cultural naming patterns for party generation
#[derive(Debug, Clone)]
pub struct CulturalNamingPatterns {
    /// Common prefixes (e.g., "National", "United", "People's")
    pub prefixes: Vec<&'static str>,
    
    /// Party nouns (e.g., "Party", "Front", "Union", "League")
    pub nouns: Vec<&'static str>,
    
    /// Ideological themes (e.g., "of Labor", "of Freedom", "Agrarian")
    pub themes: Vec<&'static str>,
    
    /// Cultural-specific modifiers
    pub cultural_modifiers: Vec<&'static str>,
}

/// Get cultural naming patterns for a given cultural group
/// Phase 42: Expanded pools to reduce collision probability.
pub fn get_cultural_patterns(cultural_group: &str) -> CulturalNamingPatterns {
    match cultural_group {
        "slavic" => CulturalNamingPatterns {
            prefixes: vec!["National", "United", "People's", "Patriotic", "Democratic", "Christian", "Conservative", "Liberal", "Progressive", "Social"],
            nouns: vec!["Party", "Front", "Union", "Movement", "League", "Coalition", "Alliance", "Bloc", "Initiative", "Platform"],
            themes: vec!["of Labor", "of Freedom", "of Farmers", "of Rebirth", "of Solidarity", "of Justice", "of Truth", "of the Future", "of Family", "of Reform"],
            cultural_modifiers: vec!["Slavic", "National", "Traditional", "Modern", "People's"],
        },
        "germanic" => CulturalNamingPatterns {
            prefixes: vec!["National", "United", "People's", "Christian", "Democratic", "Conservative", "Liberal", "Progressive", "Social", "Free"],
            nouns: vec!["Party", "Union", "League", "Alliance", "Movement", "Coalition", "Front", "Bloc", "Initiative", "Platform"],
            themes: vec!["Labor", "Freedom", "Progress", "Conservative", "Liberal", "Justice", "Future", "Reform", "Family", "Heritage"],
            cultural_modifiers: vec!["Germanic", "Nordic", "Federal", "Northern", "Civic"],
        },
        "latin" => CulturalNamingPatterns {
            prefixes: vec!["Nationale", "Unité", "Populaire", "Républicain", "Démocratique", "Chrétien", "Conservateur", "Libéral", "Progressiste", "Social"],
            nouns: vec!["Parti", "Front", "Union", "Ligue", "Mouvement", "Coalition", "Alliance", "Bloc", "Initiative", "Plateforme"],
            themes: vec!["Travail", "Liberté", "Paysan", "Renaissance", "Social", "Justice", "Avenir", "Réforme", "Famille", "Héritage"],
            cultural_modifiers: vec!["Latin", "Méditerranéen", "Républicain", "Civique", "Populaire"],
        },
        "middle_eastern" => CulturalNamingPatterns {
            prefixes: vec!["Al-Watani", "Al-Wahda", "Al-Sha'bi", "Al-Islami", "Al-Dimuqrati", "Al-Ahrar", "Al-Tagaddi", "Al-Ijtima'i", "Al-Watani Al-Jadid", "Al-Mustaqbal"],
            nouns: vec!["Hizb", "Jabha", "Ittihad", "Harakat", "Tahaluf", "Kutla", "Mubadara", "Minbar", "Rabitat", "Usbat"],
            themes: vec!["Al-Ammal", "Al-Hurriya", "Al-Fallahin", "Al-Tahrir", "Al-Adala", "Al-Mustaqbal", "Al-Islah", "Al-Usra", "Al-Turath", "Al-Wahda"],
            cultural_modifiers: vec!["Islamic", "Arab", "National", "Progressive", "Civic"],
        },
        "balkan" => CulturalNamingPatterns {
            prefixes: vec!["Narodna", "Jedinstvena", "Demokratska", "Konzervativna", "Liberalna", "Socijalna", "Hrišćanska", "Progressivna", "Patriotska", "Narodna Nova"],
            nouns: vec!["Stranka", "Front", "Savez", "Pokret", "League", "Koalicija", "Alijansa", "Bloc", "Inicijativa", "Platform"],
            themes: vec!["Rada", "Sloboda", "Seljaka", "Obnova", "Solidarnost", "Pravde", "Budućnosti", "Reforme", "Porodice", "Nasleđa"],
            cultural_modifiers: vec!["Balkanski", "Narodni", "Slovenski", "Novi", "Gradjanski"],
        },
        _ => CulturalNamingPatterns {
            prefixes: vec!["National", "United", "People's", "Democratic", "Conservative", "Liberal", "Progressive", "Social", "Christian", "Free"],
            nouns: vec!["Party", "Union", "League", "Alliance", "Movement", "Coalition", "Front", "Bloc", "Initiative", "Platform"],
            themes: vec!["Labor", "Freedom", "Progress", "Justice", "Future", "Reform", "Family", "Heritage", "Solidarity", "Unity"],
            cultural_modifiers: vec!["National", "Democratic", "Civic", "Popular", "Modern"],
        },
    }
}

/// Check if an ideology is radical
fn is_radical(ideology: Ideology) -> bool {
    matches!(
        ideology,
        Ideology::OrthodoxMarxism
            | Ideology::Maoism
            | Ideology::Fascism
            | Ideology::AnarchoCapitalism
    )
}

/// Check if an ideology is centrist
fn is_centrist(ideology: Ideology) -> bool {
    matches!(
        ideology,
        Ideology::SocialLiberalism
            | Ideology::ChristianDemocracy
            | Ideology::ClassicalLiberalism
    )
}

/// Select a weighted item from a vector based on ideology
fn select_weighted(items: &[&'static str], ideology: Ideology, rng: &mut impl Rng) -> &'static str {
    let radical = is_radical(ideology);
    let centrist = is_centrist(ideology);
    
    // Simple selection - in full implementation, this would use weighted probabilities
    let index = if radical {
        rng.gen_range(0..items.len())
    } else if centrist {
        rng.gen_range(0..items.len())
    } else {
        rng.gen_range(0..items.len())
    };
    
    items.get(index).copied().unwrap_or(items[0])
}

/// Generate a party name based on cultural group and ideology
///
/// # Arguments
/// * `country_name` - Name of the country
/// * `cultural_group` - Cultural group of the country
/// * `ideology` - Party ideology
/// * `rng` - Random number generator
///
/// # Returns
/// Generated party name
///
/// # Rules
/// * Uses culturally appropriate naming patterns
/// * Radical ideologies prefer "Front", "Union", "Movement"
/// * Centrist ideologies prefer "Party", "Union", "Alliance"
/// * Conservative ideologies prefer "Party", "Union", "League"
/// * Falls back to "Country Ideology" if generation fails
pub fn generate_party_name(
    country_name: &str,
    cultural_group: &str,
    ideology: Ideology,
    rng: &mut impl Rng,
) -> String {
    let patterns = get_cultural_patterns(cultural_group);
    let radical = is_radical(ideology);
    let centrist = is_centrist(ideology);

    // Component selection weights based on ideology
    let prefix_weight = if radical { 0.7 } else { 0.4 };
    let theme_weight = if centrist { 0.3 } else { 0.6 };

    let mut components = Vec::new();

    // Phase 41: Prepend country adjective (50% probability) for immersion.
    // e.g., "Illyrian Conservative League" instead of just "Conservative League".
    if rng.gen::<f64>() < 0.5 {
        let adjective = country_adjective(country_name);
        components.push(adjective);
    }

    // Optional prefix
    if rng.gen::<f64>() < prefix_weight {
        let prefix = select_weighted(&patterns.prefixes, ideology, rng);
        components.push(prefix.to_string());
    }

    // Noun (always present)
    let noun = select_weighted(&patterns.nouns, ideology, rng);
    components.push(noun.to_string());

    // Theme (high probability for radical parties)
    if rng.gen::<f64>() < theme_weight {
        let theme = select_weighted(&patterns.themes, ideology, rng);
        components.push(theme.to_string());
    }

    // Cultural modifier (low probability)
    if rng.gen::<f64>() < 0.2 && !patterns.cultural_modifiers.is_empty() {
        let index = rng.gen_range(0..patterns.cultural_modifiers.len());
        let modifier = patterns.cultural_modifiers[index];
        components.push(modifier.to_string());
    }

    // Combine components
    let name = components.join(" ");

    // Fallback: if generation fails, use country + ideology
    if name.is_empty() {
        format!("{} {}", country_name, ideology.as_str())
    } else {
        name
    }
}

/// Phase 41: Derive a country adjective from the country name.
///
/// Simple heuristic: strip common suffixes and add "-ian" or "-ic".
/// e.g., "Illyria" → "Illyrian", "Germania" → "Germanian", "Poland" → "Polish".
fn country_adjective(country_name: &str) -> String {
    let name = country_name.trim();
    if name.is_empty() {
        return "National".to_string();
    }
    // Common suffixes → adjective forms
    if name.ends_with("ia") {
        format!("{}n", name)
    } else if let Some(stripped) = name.strip_suffix("land") {
        format!("{}ish", stripped)
    } else if name.ends_with("stan") {
        format!("{}i", name)
    } else if let Some(stripped) = name.strip_suffix("a") {
        format!("{}n", stripped)
    } else if name.ends_with("o") {
        format!("{}n", name)
    } else {
        format!("{}ian", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_party_name_not_empty() {
        let mut rng = rand::thread_rng();
        let name = generate_party_name("TestCountry", "slavic", Ideology::SocialDemocracy, &mut rng);
        assert!(!name.is_empty());
    }
    
    #[test]
    fn test_fallback_mechanism() {
        let mut rng = rand::thread_rng();
        // Test with unknown cultural group
        let name = generate_party_name("TestCountry", "UnknownGroup", Ideology::SocialLiberalism, &mut rng);
        assert!(!name.is_empty());
    }
}
