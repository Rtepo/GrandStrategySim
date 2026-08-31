//! Phase 32: Culturally-aware personal name generator for political VIPs.
//!
//! Generates culturally appropriate first names and surnames for Heads of State,
//! Prime Ministers, Ministers, and Chamber Speakers based on the country's
//! dominant cultural group.
//!
//! All name pools are `&'static str` for zero heap allocation. Generation is
//! deterministic when seeded by `country_name + role + turn`.

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

/// A culturally-grouped pool of first names and surnames.
pub struct NamePool {
    /// Male first names.
    pub first_names_male: &'static [&'static str],
    /// Female first names.
    pub first_names_female: &'static [&'static str],
    /// Surnames (gender-neutral unless noted).
    pub surnames: &'static [&'static str],
}

// ============================================================================
// CULTURAL NAME POOLS
// ============================================================================

/// Slavic name pool (Polish/Czech/Slovak).
static SLAVIC_MALE: &[&str] = &[
    "Jan", "Piotr", "Paweł", "Andrzej", "Tomasz", "Krzysztof", "Stanisław",
    "Władysław", "Kazimierz", "Bolesław", "Mieczysław", "Zdzisław", "Bronisław",
    "Wojciech", "Mariusz", "Grzegorz", "Maciej", "Jarosław", "Lech", "Zbigniew",
    "Henryk", "Jerzy", "Ryszard", "Tadeusz", "Franciszek", "Józef", "Antoni",
    "Stefan", "Aleksander", "Michał", "Jakub", "Filip", "Kacper", "Szymon",
    "Bartosz", "Adam", "Łukasz", "Mateusz", "Dawid", "Kamil", "Rafał",
    "Przemysław", "Sławomir", "Czesław", "Radosław", "Mirosław", "Bogusław",
    "Leszek", "Konrad", "Artur",
];

static SLAVIC_FEMALE: &[&str] = &[
    "Anna", "Maria", "Katarzyna", "Małgorzata", "Agnieszka", "Barbara",
    "Krystyna", "Ewa", "Elżbieta", "Zofia", "Jadwiga", "Halina", "Danuta",
    "Teresa", "Joanna", "Helena", "Irena", "Stanisława", "Władysława",
    "Kazimiera", "Wanda", "Grażyna", "Bożena", "Krystyna", "Urszula",
    "Magdalena", "Justyna", "Beata", "Dorota", "Monika", "Alicja", "Natalia",
    "Aleksandra", "Karolina", "Paulina", "Sylwia", "Marta", "Olga", "Iwona",
    "Lidia", "Ewa", "Renata", "Małgorzata", "Lucyna", "Wiesława", "Mirosława",
    "Bogusława", "Leokadia", "Czesława", "Sabina",
];

static SLAVIC_SURNAMES: &[&str] = &[
    "Kowalski", "Nowak", "Wiśniewski", "Wójcik", "Kowalczyk", "Kamiński",
    "Lewandowski", "Zieliński", "Szymański", "Woźniak", "Dąbrowski", "Kozłowski",
    "Mazur", "Krawczyk", "Piotrowski", "Grabowski", "Nowicki", "Pawłowski",
    "Michalski", "Adamczyk", "Dudek", "Zając", "Wieczorek", "Jabłoński",
    "Król", "Sikora", "Pawlak", "Majchrzak", "Walczak", "Kruk", "Kaczmarek",
    "Baran", "Sowa", "Wilk", "Lis", "Stępień", "Malinowski", "Jankowski",
    "Werner", "Sobczak", "Krupa", "Górski", "Brzeziński", "Makowski", "Zieliński",
    "Cieślak", "Domański", "Tomaszewski", "Ratajczak", "Witkowski",
    "Kaczyński", "Miller", "Tusk", "Kwaśniewski", "Wałęsa", "Jaruzelski",
    "Gierek", "Gomułka", "Bierut", "Mikołajczyk", "Pużak", "Cywinski",
    "Morawiecki", "Kaczyński", "Budka", "Hołownia", "Czarzasty", "Kosiniak",
];

/// Germanic name pool (German/Nordic).
static GERMANIC_MALE: &[&str] = &[
    "Hans", "Klaus", "Wolfgang", "Jürgen", "Karl", "Heinrich", "Friedrich",
    "Wilhelm", "Otto", "Ludwig", "Franz", "Josef", "Stefan", "Andreas",
    "Thomas", "Michael", "Peter", "Martin", "Christian", "Ulrich", "Erik",
    "Magnus", "Sven", "Lars", "Bjorn", "Olaf", "Nils", "Gustav", "Axel",
    "Henrik", "Matthias", "Georg", "Richard", "Eberhard", "Reinhard",
    "Manfred", "Günter", "Helmut", "Gerhard", "Walter", "Bernhard", "Konrad",
    "Rudolf", "Albrecht", "Dietrich", "Hermann", "Siegfried", "Oskar",
    "Viktor", "Theodor",
];

static GERMANIC_FEMALE: &[&str] = &[
    "Helga", "Ingrid", "Greta", "Anna", "Emma", "Hildegard", "Edelgard",
    "Brunhilde", "Sieglinde", "Ursula", "Gertrud", "Hannelore", "Karin",
    "Brigitte", "Christa", "Renate", "Marlene", "Erica", "Frauke", "Lena",
    "Astrid", "Birgit", "Kirsten", "Mette", "Hanne", "Sigrid", "Thora",
    "Freya", "Idun", "Saga", "Gudrun", "Hilda", "Mathilde", "Adelheid",
    "Bertha", "Clotilde", "Gerda", "Hertha", "Irmgard", "Liselotte",
    "Margarete", "Ottilie", "Roswitha", "Walburga", "Wilhelmine", "Anneliese",
    "Erika", "Friederike", "Henriette", "Josepha",
];

static GERMANIC_SURNAMES: &[&str] = &[
    "Schmidt", "Müller", "Schneider", "Fischer", "Weber", "Meyer", "Wagner",
    "Becker", "Schulz", "Hoffmann", "Schäfer", "Koch", "Bauer", "Richter",
    "Klein", "Wolf", "Schröder", "Neumann", "Schwarz", "Zimmermann", "Braun",
    "Krüger", "Hofmann", "Hartmann", "Lange", "Werner", "Krause", "Lehmann",
    "Köhler", "Herrmann", "Konig", "Walter", "Maier", "Schmitt", "Friedrich",
    "Keller", "Vogel", "Frank", "Berger", "Winkler", "Roth", "Beck", "Schubert",
    "Engel", "Bach", "Luther", "Bismarck", "Brandt", "Adenauer", "Erhard",
    "Kohl", "Schröder", "Merkel", "Schäuble", "Lafontaine", "Genscher",
    "Strauß", "Seehofer", "Lindner", "Habeck", "Baerbock", "Söder",
];

/// Latin name pool (French/Italian/Spanish).
static LATIN_MALE: &[&str] = &[
    "Jean", "Pierre", "François", "Philippe", "Henri", "Louis", "Charles",
    "Jacques", "Antoine", "Nicolas", "Michel", "André", "Bernard", "Gérard",
    "Patrick", "Daniel", "Christophe", "Stéphane", "Olivier", "Thierry",
    "Giovanni", "Marco", "Antonio", "Giuseppe", "Francesco", "Luigi", "Carlo",
    "Andrea", "Lorenzo", "Matteo", "Roberto", "Alessandro", "Stefano", "Paolo",
    "Juan", "Carlos", "José", "Antonio", "Manuel", "Francisco", "Javier",
    "Miguel", "Alejandro", "Ricardo", "Pedro", "Diego", "Fernando", "Pablo",
    "Rafael", "Eduardo",
];

static LATIN_FEMALE: &[&str] = &[
    "Marie", "Jeanne", "Marguerite", "Catherine", "Élisabeth", "Anne",
    "Françoise", "Isabelle", "Sylvie", "Nathalie", "Valérie", "Christine",
    "Brigitte", "Martine", "Sophie", "Claire", "Hélène", "Julie", "Camille",
    "Giulia", "Sofia", "Anna", "Maria", "Elena", "Francesca", "Valentina",
    "Alessandra", "Chiara", "Lucia", "Beatrice", "Marta", "Gianna", "Paola",
    "Carmen", "María", "Isabel", "Elena", "Cristina", "Ana", "Laura",
    "Patricia", "Teresa", "Rosa", "Beatriz", "Francisca", "Mercedes", "Pilar",
    "Dolores", "Concepción", "Victoria",
];

static LATIN_SURNAMES: &[&str] = &[
    "Martin", "Bernard", "Dubois", "Thomas", "Robert", "Richard", "Petit",
    "Durand", "Leroy", "Moreau", "Simon", "Laurent", "Lefebvre", "Michel",
    "Garcia", "García", "Fernández", "González", "Rodríguez", "López",
    "Martínez", "Sánchez", "Pérez", "Gómez", "Ruiz", "Hernández", "Díaz",
    "Rossi", "Russo", "Ferrari", "Esposito", "Bianchi", "Romano", "Colombo",
    "Ricci", "Marino", "Greco", "Bruno", "Gallo", "Conti", "De Luca",
    "Costa", "Mancini", "Rizzo", "Lombardi", "Moretti", "Barbieri", "Fontana",
    "Santoro", "Rinaldi", "Caruso", "Ferrara", "Galli",
    "Mitterrand", "Chirac", "Sarkozy", "Hollande", "Macron", "Le Pen",
    "Berlusconi", "Prodi", "Draghi", "Conte", "Meloni", "Renzi",
    "Zapatero", "Rajoy", "Sánchez", "Aznar", "Felipe", "Juan Carlos",
];

/// Middle Eastern name pool (Arabic).
static MIDEAST_MALE: &[&str] = &[
    "Ahmed", "Mohammed", "Ali", "Hassan", "Hussein", "Omar", "Khalid",
    "Ibrahim", "Yusuf", "Abdullah", "Abdul", "Rahman", "Karim", "Tariq",
    "Salem", "Farid", "Nabil", "Rashid", "Mansour", "Jamal", "Faisal",
    "Bassam", "Ghazi", "Sami", "Wael", "Hadi", "Najib", "Salim", "Adel",
    "Munir", "Sabri", "Latif", "Anwar", "Saad", "Hamid", "Rami", "Bilal",
    "Ziad", "Fouad", "Galal", "Mamdouh", "Nasser", "Sherif", "Tahsin",
    "Wahid", "Yasin", "Zahir", "Amin", "Fuad", "Raouf",
];

static MIDEAST_FEMALE: &[&str] = &[
    "Fatima", "Aisha", "Zainab", "Maryam", "Khadija", "Hala", "Layla",
    "Nour", "Salma", "Rana", "Hana", "Amal", "Dina", "Lina", "Mona",
    "Nadia", "Rima", "Sara", "Yara", "Farah", "Ghada", "Huda", "Intisar",
    "Jamila", "Karima", "Latifa", "Maha", "Nisreen", "Rasha", "Siham",
    "Taghrid", "Wafa", "Yasmin", "Zahra", "Amira", "Badriya", "Dalia",
    "Eman", "Fadia", "Hiba", "Iman", "Kawthar", "Maysa", "Nabila", "Ruba",
    "Samar", "Wijdan", "Yara", "Zina", "Amani",
];

static MIDEAST_SURNAMES: &[&str] = &[
    "Al-Saud", "Al-Assad", "Al-Hashimi", "Al-Rashid", "Al-Nasser", "Al-Farouq",
    "Hussein", "Saleh", "Ahmad", "Mahmoud", "Ibrahim", "Khalil", "Mansour",
    "Said", "Shamir", "Tahir", "Yassin", "Zaid", "Abbas", "Arafat", "Assad",
    "Bakr", "Darwish", "Fahd", "Ghassan", "Habib", "Idris", "Jaber", "Khoury",
    "Mansour", "Najjar", "Qasim", "Rahimi", "Sabbagh", "Touma", "Wahhab",
    "Younis", "Zahra", "Abdulrahman", "Al-Maliki", "Al-Jabouri", "Al-Douri",
    "Al-Tikriti", "Al-Bakr", "Al-Naqib", "Al-Hashimi", "Al-Samarrai",
    "Al-Dulaimi", "Al-Kubaisi", "Al-Mosuli", "Al-Baghdadi",
    "Nasser", "Sadat", "Mubarak", "Sisi", "Erdogan", "Assad", "Khomeini",
    "Khamenei", "Rafsanjani", "Rouhani", "Raisi", "Khalifa", "Hamad",
];

/// Balkan name pool (Serbo-Croatian).
static BALKAN_MALE: &[&str] = &[
    "Milan", "Dragan", "Goran", "Zoran", "Slobodan", "Radovan", "Milan",
    "Vojislav", "Boris", "Branislav", "Milorad", "Mihajlo", "Nikola",
    "Petar", "Stefan", "Aleksandar", "Filip", "Marko", "Tomislav", "Ivan",
    "Josip", "Ante", "Mate", "Ivo", "Franjo", "Stjepan", "Vladimir",
    "Dražen", "Zvonimir", "Krešimir", "Damir", "Dario", "Domagoj", "Borna",
    "Bogdan", "Bojan", "Dejan", "Darko", "Nebojša", "Saša", "Predrag",
    "Vladan", "Mladen", "Rajko", "Siniša", "Slavko", "Veselin", "Žarko",
    "Milutin", "Tihomir",
];

static BALKAN_FEMALE: &[&str] = &[
    "Milena", "Dragana", "Gordana", "Zorana", "Slobodanka", "Radovanka",
    "Vojislava", "Borisava", "Branislava", "Milorada", "Mihajla", "Nikola",
    "Petra", "Stefana", "Aleksandra", "Filipa", "Marka", "Tomislava",
    "Ivana", "Josipa", "Antea", "Matica", "Iva", "Franjo", "Stjepana",
    "Vladimira", "Dražena", "Zvonimira", "Krešimira", "Damira", "Daria",
    "Domagoja", "Borna", "Bogdana", "Bojana", "Dejana", "Darka",
    "Nebojša", "Saša", "Predraga", "Vladana", "Mladena", "Rajka",
    "Siniša", "Slavka", "Veselina", "Žarka", "Milutina", "Tihomira",
    "Ana", "Marija",
];

static BALKAN_SURNAMES: &[&str] = &[
    "Petrović", "Nikolić", "Jovanović", "Stojanović", "Ilić", "Marković",
    "Đorđević", "Stanković", "Ivanović", "Popović", "Filipović", "Milovanović",
    "Kovač", "Kovačević", "Marić", "Vuković", "Radović", "Milosevic",
    "Tadić", "Kostić", "Vasić", "Lukić", "Radosavljević", "Simić",
    "Tanasković", "Bojović", "Savić", "Zarić", "Obradović", "Pavlović",
    "Horvat", "Kovačić", "Babić", "Marinković", "Novaković", "Vidović",
    "Knežević", "Rajić", "Šuker", "Karamarko", "Sanader", "Milanović",
    "Plenković", "Grabar-Kitarović", "Josipović", "Mesić", "Tuđman",
    "Šešelj", "Dačić", "Vučić", "Nikolić", "Tadić", "Dinkić",
];

/// Static name pool constants for each cultural group.
static SLAVIC_POOL: NamePool = NamePool {
    first_names_male: SLAVIC_MALE,
    first_names_female: SLAVIC_FEMALE,
    surnames: SLAVIC_SURNAMES,
};

static GERMANIC_POOL: NamePool = NamePool {
    first_names_male: GERMANIC_MALE,
    first_names_female: GERMANIC_FEMALE,
    surnames: GERMANIC_SURNAMES,
};

static LATIN_POOL: NamePool = NamePool {
    first_names_male: LATIN_MALE,
    first_names_female: LATIN_FEMALE,
    surnames: LATIN_SURNAMES,
};

static MIDEAST_POOL: NamePool = NamePool {
    first_names_male: MIDEAST_MALE,
    first_names_female: MIDEAST_FEMALE,
    surnames: MIDEAST_SURNAMES,
};

static BALKAN_POOL: NamePool = NamePool {
    first_names_male: BALKAN_MALE,
    first_names_female: BALKAN_FEMALE,
    surnames: BALKAN_SURNAMES,
};

/// Get the name pool for a cultural group.
/// Uses strictly lowercase English keys: "slavic", "germanic", "latin",
/// "middle_eastern", "balkan". Unknown keys default to the Slavic pool.
pub fn name_pool_for_culture(cultural_group: &str) -> &'static NamePool {
    match cultural_group {
        "slavic" => &SLAVIC_POOL,
        "germanic" => &GERMANIC_POOL,
        "latin" => &LATIN_POOL,
        "middle_eastern" => &MIDEAST_POOL,
        "balkan" => &BALKAN_POOL,
        _ => &SLAVIC_POOL,
    }
}

/// A generated VIP name with gender.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VipName {
    pub first_name: String,
    pub surname: String,
    pub full_name: String,
    pub gender: String,
}

/// Generate a person's full name from a cultural group.
///
/// # Arguments
/// * `cultural_group` - Cultural group key (e.g., "slavic", "germanic")
/// * `gender` - "M" for male, "F" for female, other defaults to male
/// * `rng` - Random number generator
///
/// # Returns
/// A `VipName` with first name, surname, and full name.
pub fn generate_person_name(cultural_group: &str, gender: &str, rng: &mut impl Rng) -> VipName {
    let pool = name_pool_for_culture(cultural_group);

    let (first_names, gender_str) = if gender == "F" || gender == "Female" {
        (pool.first_names_female, "F")
    } else {
        (pool.first_names_male, "M")
    };

    let first_name = first_names
        .choose(rng)
        .copied()
        .unwrap_or("Jan")
        .to_string();
    let surname = pool
        .surnames
        .choose(rng)
        .copied()
        .unwrap_or("Kowalski")
        .to_string();

    let full_name = format!("{} {}", first_name, surname);

    VipName {
        first_name,
        surname,
        full_name,
        gender: gender_str.to_string(),
    }
}

/// Generate a full VIP with name and age.
///
/// # Arguments
/// * `cultural_group` - Cultural group key
/// * `rng` - Random number generator
///
/// # Returns
/// A `VipName` with a random age (35–75 for politicians).
pub fn generate_full_vip(cultural_group: &str, rng: &mut impl Rng) -> VipName {
    let gender = if rng.gen::<f64>() < 0.7 {
        "M" // 70% male politicians (historical realism)
    } else {
        "F"
    };
    generate_person_name(cultural_group, gender, rng)
}

/// Phase 41: Generate a unique VIP name with HashSet deduplication.
///
/// Tries up to 20 times to generate a name not in `used_names`.
/// With 50+ names per pool and 5000+ combinations, collision probability
/// after 20 redraws is mathematically negligible.
/// NO numeric suffixes or "Jr." fallbacks — these break immersion.
pub fn generate_unique_vip(
    cultural_group: &str,
    rng: &mut impl Rng,
    used_names: &mut std::collections::HashSet<String>,
) -> VipName {
    for _ in 0..20 {
        let vip = generate_full_vip(cultural_group, rng);
        if !used_names.contains(&vip.full_name) {
            used_names.insert(vip.full_name.clone());
            return vip;
        }
    }
    // With 50+ names per pool, this branch is mathematically unreachable.
    // If it somehow fires, just return the last drawn name (no numeric suffix).
    let vip = generate_full_vip(cultural_group, rng);
    used_names.insert(vip.full_name.clone());
    vip
}

/// Phase 91: Generate a unique VIP name for KEY POLITICAL FIGURES only.
///
/// This is a hardened version of `generate_unique_vip` with:
/// - 50 iterations (up from 20) for more retries before exhaustion.
/// - Hard force-break on exhaustion: returns a duplicate rather than hanging.
/// - Clear documentation that duplicate-on-exhaust is acceptable.
///
/// # When to use this vs `generate_full_vip`
/// - **Key Political Figures** (Head of State, PM, Royal Consort, Royal Heirs,
///   Party Leaders, Advisory Council): Use `generate_key_vip` with a shared
///   `used_names` set. ~10-20 per country, well within the ~2500 combination
///   pool.
/// - **Generic VIPs** (CEOs, mayors, board members, ministers): Use
///   `generate_full_vip` directly. Duplicates are permitted and realistic
///   (e.g., "John Smith" exists multiple times in the real world).
///
/// # Exhaustion behavior
/// If the cultural name pool is exhausted for key figures (extremely unlikely
/// with only 10-20 key figures per country), a duplicate name is returned.
/// This is acceptable; the simulation continues rather than hanging.
pub fn generate_key_vip(
    cultural_group: &str,
    rng: &mut impl Rng,
    used_names: &mut std::collections::HashSet<String>,
) -> VipName {
    for _ in 0..50 {
        let vip = generate_full_vip(cultural_group, rng);
        if !used_names.contains(&vip.full_name) {
            used_names.insert(vip.full_name.clone());
            return vip;
        }
    }
    // Pool exhausted — return a duplicate rather than hanging.
    // Stability is more important than avoiding a duplicate name.
    let vip = generate_full_vip(cultural_group, rng);
    used_names.insert(vip.full_name.clone());
    vip
}

/// Phase 33: Convert a generated VipName into a Leader struct.
///
/// Populates name, gender, age, and sensible defaults for views/traits
/// based on the ideology. Used when assigning party leaders.
/// Phase 53: Uses `assign_core_traits` for diverse trait assignment and
/// `Ideology::from_name` for proper English ideology names (was hardcoded).
pub fn vip_to_leader(vip: VipName, ideology: &str) -> crate::politics::system::Leader {
    use crate::politics::system::Leader;

    // Phase 53: Map ideology string to the canonical English ideology name
    // via the Ideology enum. Falls back to the raw input if unrecognized.
    let views = crate::politics::ideology::Ideology::from_name(ideology)
        .map(|i| i.as_str().to_string())
        .unwrap_or_else(|| ideology.to_string());

    // Phase 53: Use the weighted core-trait pool instead of hardcoded
    // "Charismatic"/"Diplomatic"/"Lawfulness".
    let mut rng = rand::thread_rng();
    let (traits, main_trait) = crate::politics::vip_registry::assign_core_traits(&mut rng);

    Leader {
        name: vip.full_name,
        gender: vip.gender,
        age: 45 + rng.gen_range(0..25),
        health: "Good".to_string(),
        days_sick: 0,
        religion: String::new(),
        nationality: String::new(),
        views,
        traits,
        main_trait,
        dynasty: None,
        base_influence: 40 + rng.gen_range(0..40),
        faction: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_name_not_empty() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("slavic", "M", &mut rng);
        assert!(!name.first_name.is_empty());
        assert!(!name.surname.is_empty());
        assert!(!name.full_name.is_empty());
        assert!(name.full_name.contains(&name.first_name));
        assert!(name.full_name.contains(&name.surname));
    }

    #[test]
    fn test_female_name_from_female_pool() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("slavic", "F", &mut rng);
        assert_eq!(name.gender, "F");
        assert!(SLAVIC_FEMALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_male_name_from_male_pool() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("slavic", "M", &mut rng);
        assert_eq!(name.gender, "M");
        assert!(SLAVIC_MALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_germanic_culture() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("germanic", "M", &mut rng);
        assert!(GERMANIC_MALE.contains(&name.first_name.as_str()));
        assert!(GERMANIC_SURNAMES.contains(&name.surname.as_str()));
    }

    #[test]
    fn test_latin_culture() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("latin", "M", &mut rng);
        assert!(LATIN_MALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_mideast_culture() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("middle_eastern", "M", &mut rng);
        assert!(MIDEAST_MALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_balkan_culture() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("balkan", "M", &mut rng);
        assert!(BALKAN_MALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_unknown_culture_falls_back_to_slavic() {
        let mut rng = rand::thread_rng();
        let name = generate_person_name("UnknownGroup", "M", &mut rng);
        assert!(SLAVIC_MALE.contains(&name.first_name.as_str()));
    }

    #[test]
    fn test_generate_full_vip_has_name() {
        let mut rng = rand::thread_rng();
        let vip = generate_full_vip("slavic", &mut rng);
        assert!(!vip.full_name.is_empty());
        assert!(vip.gender == "M" || vip.gender == "F");
    }

    #[test]
    fn test_name_pool_for_each_culture() {
        let cultures = ["slavic", "germanic", "latin", "middle_eastern", "balkan"];
        for culture in &cultures {
            let pool = name_pool_for_culture(culture);
            assert!(!pool.first_names_male.is_empty());
            assert!(!pool.first_names_female.is_empty());
            assert!(!pool.surnames.is_empty());
        }
    }

    // Phase 33: vip_to_leader tests.

    #[test]
    fn test_vip_to_leader_creates_named_leader() {
        let mut rng = rand::thread_rng();
        let vip = generate_full_vip("slavic", &mut rng);
        let leader = vip_to_leader(vip, "Socjalliberalizm");
        assert!(!leader.name.is_empty(), "Leader name should not be empty");
        assert!(!leader.views.is_empty(), "Leader views should not be empty");
        assert!(!leader.traits.is_empty(), "Leader should have traits");
    }

    #[test]
    fn test_vip_to_leader_ideology_mapping() {
        // Phase 53: Updated to use proper English ideology names via
        // Ideology::from_name instead of the old simplified categories.
        let vip = VipName {
            first_name: "Jan".to_string(),
            surname: "Kowalski".to_string(),
            full_name: "Jan Kowalski".to_string(),
            gender: "M".to_string(),
        };
        let leader = vip_to_leader(vip, "Orthodox Marxism");
        assert_eq!(leader.views, "Orthodox Marxism");
        let leader2 = vip_to_leader(VipName {
            first_name: "John".to_string(),
            surname: "Smith".to_string(),
            full_name: "John Smith".to_string(),
            gender: "M".to_string(),
        }, "Social Liberalism");
        assert_eq!(leader2.views, "Social Liberalism");
        // Phase 53: Verify traits are now diverse (not hardcoded).
        assert!(!leader2.traits.is_empty(), "Leader should have traits");
        assert!(!leader2.main_trait.is_empty(), "Leader should have a main trait");
        assert!(
            leader2.traits != vec!["Charismatic".to_string(), "Diplomatic".to_string()],
            "Traits should not be the old hardcoded pair"
        );
    }
}
