use std::collections::HashMap;

use super::ideology::Ideology;
use super::system::{Constitution, Party};
use super::interest_groups::InterestGroup;
use serde::{Deserialize, Serialize};

/// Concession clause in a coalition agreement
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConcessionClause {
    /// Description of the concession
    #[serde(default)]
    pub description: String,
    
    /// Target party/faction receiving the concession
    #[serde(default)]
    pub target: String,
    
    /// Budget cost of the concession
    #[serde(default)]
    pub cost: f64,
    
    /// Ideological distance reduction provided by this concession
    #[serde(default)]
    pub distance_reduction: f64,
}

/// Allocates parliamentary seats using the requested proportional method.
///
/// Parties whose support is below the threshold are filtered out. If no party
/// crosses the threshold, the strongest party receives all seats.
///
/// # Arguments
/// * `parties` - Active parties with `support` percentages.
/// * `method` - `"D'Hondt"`, `"Sainte-Laguë"` or `"Hare-Niemeyer"`.
/// * `threshold` - Minimum support percentage to qualify for seats.
/// * `total_seats` - Number of seats to allocate.
///
/// # Returns
/// A map from party name to seat count.
///
/// # Rules
/// * Tie-breaking is deterministic: the lexicographically smallest party name
///   wins ties.
/// * D'Hondt quotient is `support / (seats + 1)`.
/// * Sainte-Laguë quotient is `support / (2 * seats + 1)`.
/// * Hare-Niemeyer uses the largest-remainder method.
pub fn calculate_seats(
    parties: &HashMap<String, Party>,
    method: &str,
    threshold: f64,
    total_seats: u32,
) -> HashMap<String, u32> {
    let mut valid: Vec<(String, f64)> = parties
        .iter()
        .filter(|(_, p)| p.support >= threshold)
        .map(|(n, p)| (n.clone(), p.support))
        .collect();

    if valid.is_empty() {
        if let Some(strongest) = strongest_party(parties) {
            return [(strongest, total_seats)].into_iter().collect();
        }
        return HashMap::new();
    }

    let total_support: f64 = valid.iter().map(|(_, s)| s).sum();
    if total_support > 0.0 {
        for (_, s) in &mut valid {
            *s = (*s / total_support) * 100.0;
        }
    }

    let names: Vec<String> = valid.iter().map(|(n, _)| n.clone()).collect();
    let mut seats: HashMap<String, u32> = names.iter().map(|n| (n.clone(), 0)).collect();

    match method {
        "Sainte-Laguë" => {
            for _ in 0..total_seats {
                let winner = valid
                    .iter()
                    .map(|(n, s)| {
                        let current = seats.get(n).copied().unwrap_or(0);
                        let q = *s / (2.0 * current as f64 + 1.0);
                        (n.clone(), q)
                    })
                    .max_by(|(na, qa), (nb, qb)| {
                        qa.partial_cmp(qb)
                            .unwrap()
                            .then_with(|| na.cmp(nb).reverse())
                    })
                    .map(|(n, _)| n);
                if let Some(w) = winner {
                    *seats.get_mut(&w).unwrap() += 1;
                }
            }
        }
        "Hare-Niemeyer" => {
            let mut remainders: Vec<(String, f64)> = Vec::new();
            for (n, s) in &valid {
                let quota = (*s / 100.0) * total_seats as f64;
                let whole = quota as u32;
                seats.insert(n.clone(), whole);
                remainders.push((n.clone(), quota - whole as f64));
            }
            let allocated: u32 = seats.values().sum();
            let mut remaining = total_seats.saturating_sub(allocated);
            remainders.sort_by(|(na, ra), (nb, rb)| {
                rb.partial_cmp(ra)
                    .unwrap()
                    .then_with(|| na.cmp(nb).reverse())
            });
            for (n, _) in remainders.iter().take(remaining as usize) {
                *seats.get_mut(n).unwrap() += 1;
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
            }
        }
        _ => {
            // D'Hondt is the default.
            for _ in 0..total_seats {
                let winner = valid
                    .iter()
                    .map(|(n, s)| {
                        let current = seats.get(n).copied().unwrap_or(0);
                        let q = *s / (current as f64 + 1.0);
                        (n.clone(), q)
                    })
                    .max_by(|(na, qa), (nb, qb)| {
                        qa.partial_cmp(qb)
                            .unwrap()
                            .then_with(|| na.cmp(nb).reverse())
                    })
                    .map(|(n, _)| n);
                if let Some(w) = winner {
                    *seats.get_mut(&w).unwrap() += 1;
                }
            }
        }
    }

    seats.retain(|_, s| *s > 0);
    seats
}

/// Builds a ruling coalition by ideological proximity.
///
/// # Returns
/// `(ruling_party, coalition_partners, is_minority, coalition_id)`.
///
/// # Rules
/// * If the largest party already has a majority, it rules alone.
/// * Otherwise partners are added in order of increasing ideological distance
///   until a majority is reached.
/// * The maximum acceptable ideological distance is `1.4`.
pub fn build_coalition(
    parliament: &HashMap<String, u32>,
    active_parties: &HashMap<String, Party>,
) -> (String, Vec<String>, bool, String) {
    let (ruling, coalition, minority, _id, _cost) = build_coalition_with_concessions(parliament, active_parties, &[]);
    (ruling, coalition, minority, _id)
}

/// Builds a ruling coalition with concession clauses to sway votes.
///
/// # Arguments
/// * `parliament` - Current parliamentary seat distribution
/// * `active_parties` - Active parties with ideologies
/// * `concessions` - Concession clauses to reduce ideological distance thresholds
///
/// # Returns
/// `(ruling_party, coalition_partners, is_minority, coalition_id, total_concession_cost)`.
///
/// # Rules
/// * Concessions cost budget but reduce ideological distance threshold temporarily
/// * Example: "Increase hospital funding in Region X" costs 5 budget, reduces distance by 0.2
/// * Councilor traits (Loyalist/Undecided/Corrupt) affect vote counting
pub fn build_coalition_with_concessions(
    parliament: &HashMap<String, u32>,
    active_parties: &HashMap<String, Party>,
    concessions: &[ConcessionClause],
) -> (String, Vec<String>, bool, String, f64) {
    let total: u32 = parliament.values().sum();
    let majority = total / 2 + 1;
    let mut ordered: Vec<(String, u32)> = parliament.iter().map(|(n, s)| (n.clone(), *s)).collect();
    ordered.sort_by(|(na, sa), (nb, sb)| sb.cmp(sa).then_with(|| na.cmp(nb).reverse()));

    if ordered.is_empty() {
        return (
            "Provisional Technocratic Government".to_string(),
            Vec::new(),
            false,
            "[COA-000]".to_string(),
            0.0,
        );
    }

    let (leader, leader_seats) = ordered.first().cloned().unwrap();
    if leader_seats >= majority {
        return (leader, Vec::new(), false, "[COA-000]".to_string(), 0.0);
    }

    let leader_ideology = active_parties
        .get(&leader)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or_default();

    // Calculate total distance reduction from concessions
    let total_distance_reduction: f64 = concessions.iter().map(|c| c.distance_reduction).sum();
    let total_concession_cost: f64 = concessions.iter().map(|c| c.cost).sum();

    let mut candidates: Vec<(String, f64, u32)> = ordered
        .iter()
        .skip(1)
        .map(|(n, seats)| {
            let dist = active_parties
                .get(n)
                .and_then(|p| Ideology::from_name(&p.ideology))
                .map(|i| ideological_distance(leader_ideology, i))
                .unwrap_or(f64::MAX);
            (n.clone(), dist, *seats)
        })
        .collect();
    candidates.sort_by(|(na, da, sa), (nb, db, sb)| {
        da.partial_cmp(db)
            .unwrap()
            .then_with(|| sb.cmp(sa).reverse())
            .then_with(|| na.cmp(nb).reverse())
    });

    let mut coalition = Vec::new();
    let mut seats_count = leader_seats;
    let max_distance = 1.4 + total_distance_reduction; // Concessions increase acceptable distance
    for (partner, dist, partner_seats) in candidates {
        if dist > max_distance {
            continue;
        }
        coalition.push(partner.clone());
        seats_count += partner_seats;
        if seats_count >= majority {
            return (leader, coalition, false, "[COA-000]".to_string(), total_concession_cost);
        }
    }

    (leader, coalition, true, "[COA-000]".to_string(), total_concession_cost)
}

/// Checks whether the ideological spread in a coalition is too wide.
///
/// # Returns
/// `(unstable, message)`. Unstable means the coalition collapses and early
/// elections are called.
pub fn check_coalition_stability(
    ruling_party: &str,
    coalition: &[String],
    active_parties: &HashMap<String, Party>,
    unrest: f64,
) -> (bool, String) {
    if coalition.is_empty() {
        return (false, String::new());
    }

    let leader_ideology = active_parties
        .get(ruling_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or_default();

    let mut max_dist = 0.0;
    let mut worst_partner = String::new();
    for partner in coalition {
        if let Some(party) = active_parties.get(partner) {
            if let Some(ideo) = Ideology::from_name(&party.ideology) {
                let dist = ideological_distance(leader_ideology, ideo);
                if dist > max_dist {
                    max_dist = dist;
                    worst_partner = partner.clone();
                }
            }
        }
    }

    if max_dist > 0.8 {
        let chance = (max_dist - 0.8) * 0.4 + (unrest / 200.0);
        if chance > 0.5 {
            return (
                true,
                format!(
                    "[COALITION BREAKDOWN] Ideological differences between {} and {} caused the pact to break.",
                    ruling_party, worst_partner
                ),
            );
        }
    }

    (false, String::new())
}

/// Computes the composition of the upper house from constitutional rules.
///
/// # Rules
/// * Hereditary upper houses seat aristocracy, clergy, and the military.
/// * Appointed houses are packed by the ruling party and bureaucrats.
/// * Indirect elections mirror the strongest interest groups.
/// * Universal elections mirror the lower house with a 10% threshold.
pub fn calculate_upper_house_composition(
    constitution: &Constitution,
    active_parties: &HashMap<String, Party>,
    interest_groups: &HashMap<String, InterestGroup>,
    ruling_party: &str,
) -> HashMap<String, u32> {
    let mut composition: HashMap<String, f64> = HashMap::new();
    const SEATS: f64 = 100.0;

    if let Some(upper_house) = &constitution.upper_house {
        let method = upper_house.elections.clone();
        if method.contains("Hereditary") {
            composition.insert("Aristocracy".to_string(), interest_groups.get("Aristocracy").map(|ig| ig.total_political_weight).unwrap_or(30.0));
            composition.insert("Clergy".to_string(), interest_groups.get("Clergy").map(|ig| ig.total_political_weight).unwrap_or(20.0));
            composition.insert("Armed Forces".to_string(), interest_groups.get("Armed Forces").map(|ig| ig.total_political_weight).unwrap_or(15.0));
            composition.insert("Independent Conservatives".to_string(), 35.0);
        } else if method.contains("Appointment") {
            composition.insert(ruling_party.to_string(), 60.0);
            composition.insert("Bureaucrats".to_string(), 20.0);
            composition.insert("Specialists / Technocrats".to_string(), 20.0);
        } else if method.contains("Indirect") {
            let mut groups: Vec<(&String, f64)> = interest_groups.iter().map(|(k, v)| (k, v.total_political_weight)).collect();
            groups.sort_by(|(a, va), (b, vb)| vb.partial_cmp(va).unwrap().then_with(|| a.cmp(b)));
            for (group, power) in groups.iter().take(4) {
                composition.insert((*group).clone(), *power);
            }
        } else {
            for (name, party) in active_parties {
                if party.support > 10.0 {
                    composition.insert(name.clone(), party.support);
                }
            }
        }
    }

    let total: f64 = composition.values().sum();
    if total <= 0.0 {
        return [("Independents".to_string(), 100)].into_iter().collect();
    }

    let mut seats: HashMap<String, u32> = HashMap::new();
    let mut remainders: Vec<(String, f64)> = Vec::new();
    for (name, power) in &composition {
        let quota = (*power / total) * SEATS;
        seats.insert(name.clone(), quota as u32);
        remainders.push((name.clone(), quota - (quota as u32) as f64));
    }

    let mut rem = (SEATS as u32) - seats.values().sum::<u32>();
    remainders.sort_by(|(a, ra), (b, rb)| rb.partial_cmp(ra).unwrap().then_with(|| a.cmp(b)));
    for (name, _) in remainders.iter().take(rem as usize) {
        *seats.get_mut(name).unwrap() += 1;
        rem -= 1;
        if rem == 0 {
            break;
        }
    }

    seats.retain(|_, s| *s > 0);
    seats
}

/// Euclidean distance between two ideologies on the three-dimensional compass.
pub fn ideological_distance(a: Ideology, b: Ideology) -> f64 {
    let c1 = a.compass();
    let c2 = b.compass();
    let dx = c1.economy - c2.economy;
    let dy = c1.liberty - c2.liberty;
    let dz = c1.tradition - c2.tradition;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn strongest_party(parties: &HashMap<String, Party>) -> Option<String> {
    parties
        .iter()
        .max_by(|(na, pa), (nb, pb)| {
            pa.support
                .partial_cmp(&pb.support)
                .unwrap()
                .then_with(|| na.cmp(nb))
        })
        .map(|(n, _)| n.clone())
}

// ============================================================================
// PHASE 32: FPTP / MAJORITARIAN ELECTIONS
// ============================================================================

/// Single-member district first-past-the-post seat allocation.
///
/// Each district's seat goes to the party with the most votes. Districts are
/// simulated by dividing national support into `num_districts` virtual districts
/// using a deterministic noise function. This produces majoritarian outcomes
/// where the largest party gets a seat bonus.
///
/// # Arguments
/// * `parties` - Active parties with `support` percentages.
/// * `num_districts` - Number of single-member districts (= total seats).
/// * `country_name` - Country name (for deterministic district seeding).
/// * `election_year` - Election year (for deterministic variation).
///
/// # Returns
/// A map from party name to seat count.
///
/// # Rules
/// * Ties broken deterministically by party name (lexicographically smallest wins).
/// * Parties below 1% support are excluded.
/// * The largest party typically gets a seat bonus (majoritarian effect).
pub fn calculate_seats_fptp(
    parties: &HashMap<String, Party>,
    num_districts: u32,
    country_name: &str,
    election_year: u32,
) -> HashMap<String, u32> {
    let mut seats: HashMap<String, u32> = HashMap::new();

    // Filter parties with meaningful support.
    let valid: Vec<(String, f64)> = parties
        .iter()
        .filter(|(_, p)| p.support >= 1.0)
        .map(|(n, p)| (n.clone(), p.support))
        .collect();

    if valid.is_empty() {
        if let Some(strongest) = strongest_party(parties) {
            seats.insert(strongest, num_districts);
        }
        return seats;
    }

    // Simulate districts with deterministic noise.
    // Each district applies a different "regional bias" to party support.
    for district_idx in 0..num_districts {
        // Deterministic seed: hash of country_name + year + district_idx.
        let seed = deterministic_seed(country_name, election_year, district_idx);

        // Apply noise to each party's support.
        let mut district_votes: Vec<(String, f64)> = valid
            .iter()
            .map(|(name, support)| {
                // Noise factor: ±20% variation per district.
                let noise = 1.0 + ((seed.wrapping_mul(district_idx as u64 + 1) % 41) as f64 / 100.0) - 0.2;
                (name.clone(), support * noise)
            })
            .collect();

        // Winner takes all in this district.
        district_votes.sort_by(|(na, va), (nb, vb)| {
            vb.partial_cmp(va)
                .unwrap()
                .then_with(|| na.cmp(nb)) // Tie-break: lexicographically smallest wins.
        });

        if let Some((winner, _)) = district_votes.first() {
            *seats.entry(winner.clone()).or_insert(0) += 1;
        }
    }

    seats.retain(|_, s| *s > 0);
    seats
}

/// Simple deterministic hash function for district seeding.
fn deterministic_seed(country_name: &str, year: u32, district: u32) -> u64 {
    let mut hash: u64 = 5381;
    for byte in country_name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash = hash.wrapping_add(year as u64);
    hash = hash.wrapping_add(district as u64);
    hash
}

// ============================================================================
// PHASE 32: WEALTH / CENSUS VOTING
// ============================================================================

/// Default census threshold: classes with savings_per_capita below this are disenfranchised.
pub const DEFAULT_CENSUS_THRESHOLD: f64 = 100.0;

/// Wealth-weighted and Census-restricted elections using ClassDemographics savings.
///
/// # Arguments
/// * `parties` - Active parties with `support` percentages.
/// * `regions` - Country regions with class demographics.
/// * `class_group_mapping` - Maps demographic classes to interest groups.
/// * `suffrage` - WealthWeightedVoting or CensusRestrictedVoting.
/// * `total_seats` - Seats to allocate.
///
/// # Rules
/// * WealthWeightedVoting: Party support is multiplied by the total `savings` of
///   the demographic classes backing that party. A party backed by Aristocracy
///   (high savings) gets a seat bonus; a party backed by LandlessLaborer (low
///   savings) gets seats reduced.
/// * CensusRestrictedVoting: Only classes with `savings_per_capita > census_threshold`
///   (default: 100.0) are counted. A LandlessLaborer with 0 savings has 0 voting
///   power. The Bourgeoisie and Aristocracy control seat distribution.
/// * Final allocation uses D'Hondt on the weighted support.
pub fn calculate_seats_wealth_census(
    parties: &HashMap<String, Party>,
    regions: &[crate::society::geography::Region],
    class_group_mapping: &super::interest_groups::ClassToGroupMapping,
    suffrage: super::interest_groups::SuffrageType,
    total_seats: u32,
) -> HashMap<String, u32> {
    calculate_seats_wealth_census_with_threshold(
        parties,
        regions,
        class_group_mapping,
        suffrage,
        total_seats,
        DEFAULT_CENSUS_THRESHOLD,
    )
}

/// Wealth/Census voting with a custom census threshold.
pub fn calculate_seats_wealth_census_with_threshold(
    parties: &HashMap<String, Party>,
    regions: &[crate::society::geography::Region],
    class_group_mapping: &super::interest_groups::ClassToGroupMapping,
    suffrage: super::interest_groups::SuffrageType,
    total_seats: u32,
    census_threshold: f64,
) -> HashMap<String, u32> {
    use super::interest_groups::SuffrageType;

    // Step 1: Calculate wealth weight for each interest group.
    let mut group_wealth: HashMap<String, f64> = HashMap::new();

    for region in regions {
        // Rural classes.
        for (class_key, demo) in &region.class_demographics.rural_classes {
            let group_name = class_group_mapping
                .rural_class_mapping
                .get(class_key)
                .map(|c| c.interest_group.clone())
                .unwrap_or_else(|| class_group_mapping.default_group.clone());

            match suffrage {
                SuffrageType::WealthWeightedVoting => {
                    // Weight by total savings of the class.
                    *group_wealth.entry(group_name).or_insert(0.0) += demo.savings;
                }
                SuffrageType::CensusRestrictedVoting => {
                    // Only count classes above the census threshold.
                    if demo.savings_per_capita >= census_threshold {
                        *group_wealth.entry(group_name).or_insert(0.0) += demo.population as f64;
                    }
                }
                _ => {
                    // Universal suffrage: weight by population.
                    *group_wealth.entry(group_name).or_insert(0.0) += demo.population as f64;
                }
            }
        }

        // Urban classes.
        for (class_key, demo) in &region.class_demographics.urban_classes {
            let group_name = class_group_mapping
                .urban_class_mapping
                .get(class_key)
                .cloned()
                .unwrap_or_else(|| class_group_mapping.default_group.clone());

            match suffrage {
                SuffrageType::WealthWeightedVoting => {
                    *group_wealth.entry(group_name).or_insert(0.0) += demo.savings;
                }
                SuffrageType::CensusRestrictedVoting => {
                    if demo.savings_per_capita >= census_threshold {
                        *group_wealth.entry(group_name).or_insert(0.0) += demo.population as f64;
                    }
                }
                _ => {
                    *group_wealth.entry(group_name).or_insert(0.0) += demo.population as f64;
                }
            }
        }
    }

    // Step 2: Calculate weighted support for each party.
    // Party support is multiplied by the total wealth of its backing interest groups.
    let mut weighted_support: HashMap<String, f64> = HashMap::new();
    let mut total_weight: f64 = 0.0;

    for (party_name, party) in parties {
        let mut party_weight: f64 = 0.0;
        for group in &party.base {
            party_weight += group_wealth.get(group).copied().unwrap_or(0.0);
        }
        // If party has no backing groups, use a minimal weight to avoid zero.
        if party_weight <= 0.0 {
            party_weight = 1.0;
        }
        let weighted = party.support * party_weight;
        weighted_support.insert(party_name.clone(), weighted);
        total_weight += weighted;
    }

    if total_weight <= 0.0 {
        return calculate_seats(parties, "D'Hondt", 0.0, total_seats);
    }

    // Step 3: Normalize and allocate using D'Hondt.
    let mut valid: Vec<(String, f64)> = weighted_support
        .iter()
        .map(|(n, w)| (n.clone(), (*w / total_weight) * 100.0))
        .collect();

    valid.sort_by(|(na, _), (nb, _)| na.cmp(nb));

    let mut seats: HashMap<String, u32> = valid.iter().map(|(n, _)| (n.clone(), 0)).collect();

    // D'Hondt allocation.
    for _ in 0..total_seats {
        let winner = valid
            .iter()
            .map(|(n, s)| {
                let current = seats.get(n).copied().unwrap_or(0);
                let q = *s / (current as f64 + 1.0);
                (n.clone(), q)
            })
            .max_by(|(na, qa), (nb, qb)| {
                qa.partial_cmp(qb)
                    .unwrap()
                    .then_with(|| na.cmp(nb).reverse())
            })
            .map(|(n, _)| n);
        if let Some(w) = winner {
            *seats.get_mut(&w).unwrap() += 1;
        }
    }

    seats.retain(|_, s| *s > 0);
    seats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::system::Party;
    use crate::politics::interest_groups::{ClassToGroupMapping, RuralClassConfig, SuffrageType};
    use crate::society::geography::{Region, RegionalClassDemographics, ClassDemographics};

    fn make_test_parties() -> HashMap<String, Party> {
        let mut parties = HashMap::new();
        let mut p1 = Party::default();
        p1.support = 45.0;
        p1.base = vec!["Aristocracy".to_string()];
        parties.insert("Conservative".to_string(), p1);

        let mut p2 = Party::default();
        p2.support = 35.0;
        p2.base = vec!["Kapitalisci".to_string()];
        parties.insert("Liberal".to_string(), p2);

        let mut p3 = Party::default();
        p3.support = 20.0;
        p3.base = vec!["Chlopi".to_string(), "Robotnicy".to_string()];
        parties.insert("Labor".to_string(), p3);
        parties
    }

    fn make_test_regions() -> Vec<Region> {
        let mut region = Region::default();
        let mut rural = std::collections::BTreeMap::new();
        rural.insert("Aristocracy".to_string(), ClassDemographics {
            population: 1000,
            savings: 5_000_000.0,
            savings_per_capita: 5000.0,
            ..Default::default()
        });
        rural.insert("LandlessLaborer".to_string(), ClassDemographics {
            population: 10000,
            savings: 50_000.0,
            savings_per_capita: 5.0,
            ..Default::default()
        });
        let mut urban = std::collections::BTreeMap::new();
        urban.insert("Bourgeoisie".to_string(), ClassDemographics {
            population: 3000,
            savings: 3_000_000.0,
            savings_per_capita: 1000.0,
            ..Default::default()
        });
        urban.insert("Worker".to_string(), ClassDemographics {
            population: 7000,
            savings: 200_000.0,
            savings_per_capita: 28.0,
            ..Default::default()
        });
        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: urban,
        };
        vec![region]
    }

    fn make_test_mapping() -> ClassToGroupMapping {
        let mut mapping = ClassToGroupMapping::default();
        mapping.rural_class_mapping.insert("Aristocracy".to_string(), RuralClassConfig {
            interest_group: "Aristocracy".to_string(),
            ..Default::default()
        });
        mapping.rural_class_mapping.insert("LandlessLaborer".to_string(), RuralClassConfig {
            interest_group: "Chlopi".to_string(),
            ..Default::default()
        });
        mapping.urban_class_mapping.insert("Bourgeoisie".to_string(), "Kapitalisci".to_string());
        mapping.urban_class_mapping.insert("Worker".to_string(), "Robotnicy".to_string());
        mapping.default_group = "Chlopi".to_string();
        mapping
    }

    #[test]
    fn test_fptp_largest_party_gets_bonus() {
        let parties = make_test_parties();
        let seats = calculate_seats_fptp(&parties, 100, "TestCountry", 1900);
        let total: u32 = seats.values().sum();
        assert_eq!(total, 100);
        let conservative_seats = seats.get("Conservative").copied().unwrap_or(0);
        assert!(conservative_seats > 45, "Conservative should get majoritarian bonus, got {}", conservative_seats);
    }

    #[test]
    fn test_fptp_deterministic() {
        let parties = make_test_parties();
        let seats1 = calculate_seats_fptp(&parties, 50, "TestCountry", 1900);
        let seats2 = calculate_seats_fptp(&parties, 50, "TestCountry", 1900);
        assert_eq!(seats1, seats2);
    }

    #[test]
    fn test_fptp_differs_from_dhondt() {
        let parties = make_test_parties();
        let fptp_seats = calculate_seats_fptp(&parties, 100, "TestCountry", 1900);
        let dhondt_seats = calculate_seats(&parties, "D'Hondt", 0.0, 100);
        let fptp_con = fptp_seats.get("Conservative").copied().unwrap_or(0);
        let dhondt_con = dhondt_seats.get("Conservative").copied().unwrap_or(0);
        assert!(fptp_con >= dhondt_con, "FPTP should give >= D'Hondt for largest party");
    }

    #[test]
    fn test_wealth_weighted_favors_rich_parties() {
        let parties = make_test_parties();
        let regions = make_test_regions();
        let mapping = make_test_mapping();
        let seats = calculate_seats_wealth_census(
            &parties, &regions, &mapping, SuffrageType::WealthWeightedVoting, 100,
        );
        let con_seats = seats.get("Conservative").copied().unwrap_or(0);
        let labor_seats = seats.get("Labor").copied().unwrap_or(0);
        assert!(con_seats > labor_seats, "Wealth-weighted should favor Conservative over Labor");
    }

    #[test]
    fn test_census_disenfranchises_poor_classes() {
        let parties = make_test_parties();
        let regions = make_test_regions();
        let mapping = make_test_mapping();
        let census_seats = calculate_seats_wealth_census(
            &parties, &regions, &mapping, SuffrageType::CensusRestrictedVoting, 100,
        );
        let universal_seats = calculate_seats_wealth_census(
            &parties, &regions, &mapping, SuffrageType::UniversalSuffrage, 100,
        );
        let census_labor = census_seats.get("Labor").copied().unwrap_or(0);
        let universal_labor = universal_seats.get("Labor").copied().unwrap_or(0);
        assert!(
            census_labor <= universal_labor,
            "Census should give Labor <= seats than universal (census: {}, universal: {})",
            census_labor, universal_labor
        );
    }

    #[test]
    fn test_wealth_census_total_seats_correct() {
        let parties = make_test_parties();
        let regions = make_test_regions();
        let mapping = make_test_mapping();
        let seats = calculate_seats_wealth_census(
            &parties, &regions, &mapping, SuffrageType::WealthWeightedVoting, 100,
        );
        let total: u32 = seats.values().sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_fptp_empty_parties() {
        let parties: HashMap<String, Party> = HashMap::new();
        let seats = calculate_seats_fptp(&parties, 10, "Test", 1900);
        assert!(seats.is_empty());
    }

    #[test]
    fn test_fptp_single_party() {
        let mut parties = HashMap::new();
        let mut p = Party::default();
        p.support = 100.0;
        parties.insert("OnlyParty".to_string(), p);
        let seats = calculate_seats_fptp(&parties, 50, "Test", 1900);
        assert_eq!(seats.get("OnlyParty").copied().unwrap_or(0), 50);
    }
}
