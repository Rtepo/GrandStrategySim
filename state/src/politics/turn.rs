use std::collections::HashMap;

use rand::Rng;

use super::elections;
use super::generator;
use super::ideology::Ideology;
use super::interest_groups;
use super::local_council;
use super::names;
use super::rebellions;
use super::system::{Constitution, GovernmentForm, Judiciary, Leader, Party, Politics, UpperHouse};
use super::vip_registry::{Vip, VipRegistry, VipRoleExtended, assign_core_traits};
use crate::state::Country;

/// Runs one year of political processing for a country.
///
/// This is a deterministic port of `politics/core.py::process_political_year`.
/// It recalculates interest group power, regenerates party support from the
/// ideology/electorate matrix, counts down to the next election, allocates
/// parliamentary seats, forms a coalition, checks its stability, and updates
/// the national policy bundle to match the ruling ideology.
///
/// # Arguments
/// * `country` - Mutable country whose `politics` block will be updated.
/// * `companies` - Mutable reference to companies (for dues and donations)
/// * `year` - Current in-game year, used for ideology availability and
///   zeitgeist multipliers.
///
/// # Returns
/// A list of human-readable political events for the turn.
///
/// # Rules
/// * Democratic regimes hold elections when `years_to_elections` reaches 0,
///   during a budget crisis, or if a minority government faces unrest > 40.
/// * Coalitions collapse deterministically when the widest ideological gap
///   together with social unrest creates a breakdown chance above 0.5.
/// * Upper house composition is recomputed every turn from the constitution.
pub fn process_political_year(country: &mut Country, companies: &mut Vec<crate::entities::Company>, unions: &mut [crate::entities::Union], year: u32) -> Vec<String> {
    let mut messages = Vec::new();

    // Phase 48: VIP Registry — age all VIPs, degrade health, check natural deaths.
    // These are batched yearly because aging is an annual event.
    // Unnatural deaths (Assassination, Coup, etc.) are handled per-turn via
    // the `pending_unnatural_deaths` queue in `process_political_turn`.
    if let Some(ref mut registry) = country.politics.vip_registry {
        registry.age_all_vips();
        registry.degrade_health_all();
        let mut rng = rand::thread_rng();
        let deaths = registry.check_natural_deaths(&mut rng);
        for (vip_id, cause) in &deaths {
            messages.push(format!(
                "[VIP] {} died of {:?} (natural death, yearly batch).",
                vip_id, cause
            ));
        }

        // Phase 55: Process CEO succession for dead CEOs.
        // When a CEO VIP dies, find the company they managed and trigger
        // family business succession or board appointment.
        let dead_ceo_ids: Vec<String> = deaths
            .iter()
            .filter_map(|(vip_id, _)| {
                let vip = registry.get(vip_id)?;
                if vip.has_role(&crate::politics::vip_registry::VipRoleExtended::Ceo) {
                    Some(vip_id.clone())
                } else {
                    None
                }
            })
            .collect();

        for dead_ceo_id in &dead_ceo_ids {
            // Find the company managed by this CEO.
            let company_idx = companies.iter().position(|c| {
                c.ceo_vip_id.as_deref() == Some(dead_ceo_id.as_str())
            });

            if let Some(idx) = company_idx {
                let company = &mut companies[idx];

                // Try family business succession first.
                let heir_id = if let crate::entities::LegalForm::FamilyBusiness(ref fbd) = company.legal_form {
                    // Find first living heir of age (≥18).
                    fbd.heir_vip_ids.iter().find_map(|heir_id| {
                        let heir = registry.get(heir_id)?;
                        if heir.is_dead || heir.age < 18 {
                            return None;
                        }
                        // Remove Heir role, add Ceo role.
                        Some(heir_id.clone())
                    })
                } else {
                    None
                };

                if let Some(heir_id) = heir_id {
                    // Promote heir to CEO.
                    if let Some(ref mut heir) = registry.get_mut(&heir_id) {
                        heir.remove_role(&crate::politics::vip_registry::VipRoleExtended::Heir);
                        heir.add_role(crate::politics::vip_registry::VipRoleExtended::Ceo);
                    }
                    company.ceo_vip_id = Some(heir_id.clone());
                    if let crate::entities::LegalForm::FamilyBusiness(ref mut fbd) = company.legal_form {
                        fbd.successor_generation += 1;
                        fbd.succession_crisis = false;
                    }
                    messages.push(format!(
                        "[SUCCESSION] Family business {} — heir {} promoted to CEO (generation {}).",
                        company.id, heir_id,
                        if let crate::entities::LegalForm::FamilyBusiness(fbd) = &company.legal_form {
                            fbd.successor_generation
                        } else { 0 }
                    ));
                } else {
                    // No living heir — check for board to appoint external CEO.
                    let has_board = if let crate::entities::LegalForm::JointStockCompany(ref jsd) = company.legal_form {
                        !jsd.board_members.is_empty()
                    } else {
                        false
                    };

                    if has_board {
                        // Board appoints an external CEO from the VIP pool.
                        // For now, mark as succession crisis — the board will
                        // recruit externally in a future turn.
                        if let crate::entities::LegalForm::FamilyBusiness(ref mut fbd) = company.legal_form {
                            fbd.succession_crisis = true;
                        }
                        messages.push(format!(
                            "[SUCCESSION] Company {} — CEO died, no living heir. Board will appoint external CEO.",
                            company.id
                        ));
                    } else {
                        // No board, no heir — mark as succession crisis.
                        // Company will be liquidated or sold via bankruptcy in future turns.
                        if let crate::entities::LegalForm::FamilyBusiness(ref mut fbd) = company.legal_form {
                            fbd.succession_crisis = true;
                        }
                        messages.push(format!(
                            "[SUCCESSION] Family business {} — CEO died, no heir. Succession crisis triggered.",
                            company.id
                        ));
                    }
                }
            }
        }
    }

    // Phase 45: Single global VIP deduplication set.
    // Created ONCE at political-year scope, passed through ALL VIP generation
    // (form_government, initialize_parliament, build_vips, generate_speaker,
    // generate_deputy_speakers). Pre-populated with all party leader names.
    let mut used_vip_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for party in country.politics.active_parties.values() {
        if !party.leader.name.is_empty() {
            used_vip_names.insert(party.leader.name.clone());
        }
    }

    // Phase 36: Migrate legacy Polish ideology strings in existing parties to
    // English canonical names. This fixes saves where parties were created with
    // Polish strings like "Socjalliberalizm" before the Phase 35 enum migration.
    for party in country.politics.active_parties.values_mut() {
        if let Some(ideo) = Ideology::from_name(&party.ideology) {
            party.ideology = ideo.as_str().to_string();
        }
        // Translate legacy Polish profile strings (Phase 79: ideology.rs now
        // returns English directly, but old saves may still have Polish values).
        match party.profile.as_str() {
            "Skrajna Lewica" => party.profile = "Far Left".to_string(),
            "Lewica" => party.profile = "Left".to_string(),
            "Centrum" => party.profile = "Centrist".to_string(),
            "Prawica" => party.profile = "Right".to_string(),
            "Skrajna Prawica" => party.profile = "Far Right".to_string(),
            _ => {}
        }
        // Translate Polish economic school strings
        match party.economic_school.as_str() {
            "Monetarist" => party.economic_school = "Monetarist".to_string(),
            "Classical" => party.economic_school = "Classical".to_string(),
            "Keynesian" => party.economic_school = "Keynesian".to_string(),
            _ => {}
        }
    }

    // 1. Refresh interest group power from the national economy.
    // Phase 36: If class_group_mapping is empty (default), inject a fallback
    // mapping so that interest groups have nonzero power. Without this, all
    // ideology bids return 0, triggering the "Provisional Technocratic
    // Government" fallback permanently.
    if country.politics.class_group_mapping.rural_class_mapping.is_empty()
        && country.politics.class_group_mapping.urban_class_mapping.is_empty()
    {
        use interest_groups::{ClassToGroupMapping, RuralClassConfig};
        let mut mapping = ClassToGroupMapping::default();
        mapping.default_group = "Petty Bourgeoisie".to_string();
        mapping.trade_union_group = "Trade Unions".to_string();
        // Rural class mappings
        mapping.rural_class_mapping.insert("FreePeasant".to_string(), RuralClassConfig {
            interest_group: "Agrarians".to_string(),
            land_value_per_capita: 500.0,
            voting_weight: 1.0,
        });
        mapping.rural_class_mapping.insert("LandlessLaborer".to_string(), RuralClassConfig {
            interest_group: "Trade Unions".to_string(),
            land_value_per_capita: 0.0,
            voting_weight: 1.0,
        });
        mapping.rural_class_mapping.insert("Aristocracy".to_string(), RuralClassConfig {
            interest_group: "Aristocracy".to_string(),
            land_value_per_capita: 5000.0,
            voting_weight: 1.0,
        });
        // Urban class mappings
        mapping.urban_class_mapping.insert("Worker".to_string(), "Trade Unions".to_string());
        mapping.urban_class_mapping.insert("Bourgeoisie".to_string(), "Petty Bourgeoisie".to_string());
        // Education mappings
        let mut no_edu = std::collections::HashMap::new();
        no_edu.insert("Trade Unions".to_string(), 0.7);
        no_edu.insert("Agrarians".to_string(), 0.3);
        mapping.education_mapping.insert("brak".to_string(), no_edu);
        let mut basic_edu = std::collections::HashMap::new();
        basic_edu.insert("Trade Unions".to_string(), 0.5);
        basic_edu.insert("Petty Bourgeoisie".to_string(), 0.3);
        basic_edu.insert("Agrarians".to_string(), 0.2);
        mapping.education_mapping.insert("podstawowe".to_string(), basic_edu);
        let mut sec_edu = std::collections::HashMap::new();
        sec_edu.insert("Petty Bourgeoisie".to_string(), 0.4);
        sec_edu.insert("Trade Unions".to_string(), 0.3);
        sec_edu.insert("Artisans".to_string(), 0.3);
        mapping.education_mapping.insert("srednie".to_string(), sec_edu);
        let mut higher_edu = std::collections::HashMap::new();
        higher_edu.insert("Specialists".to_string(), 0.4);
        higher_edu.insert("Intelligentsia".to_string(), 0.3);
        higher_edu.insert("Petty Bourgeoisie".to_string(), 0.3);
        mapping.education_mapping.insert("wyzsze".to_string(), higher_edu);
        // Company form mappings
        mapping.company_form_mapping.insert("JointStockCompany".to_string(), "Capitalists".to_string());
        mapping.company_form_mapping.insert("SoleProprietorship".to_string(), "Petty Bourgeoisie".to_string());
        mapping.company_form_mapping.insert("StateMonopoly".to_string(), "Bureaucrats".to_string());
        mapping.company_form_mapping.insert("Cooperative".to_string(), "Trade Unions".to_string());

        country.politics.class_group_mapping = mapping;
    }

    country.politics.interest_groups = interest_groups::calculate_interest_groups_power(
        country,
        companies,
        unions,
        &country.regions,
        &country.politics.class_group_mapping,
    );

    // 2. Regenerate active parties from ideology bids.
    country.politics.active_parties = regenerate_parties(&country.politics, &country.name, year, &country.macro_indicators.cultural_group);

    // Phase 36: Election escape hatch — if the provisional government has been
    // in power for more than 4 years, force-generate real parties with non-zero
    // support. This breaks the permanent deadlock even if interest group power
    // is temporarily zero.
    if country.politics.ruling_party == "Provisional Technocratic Government"
        && country.politics.active_parties.len() == 1
    {
        // The provisional government has won again. Inject real parties with
        // non-zero support based on a default ideological distribution.
        let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
            "slavic"
        } else {
            &country.macro_indicators.cultural_group
        };
        let mut rng = rand::thread_rng();
        let fallback_ideologies = [
            (Ideology::SocialDemocracy, 25.0),
            (Ideology::SocialLiberalism, 20.0),
            (Ideology::ChristianDemocracy, 18.0),
            (Ideology::SocialConservatism, 17.0),
            (Ideology::Agrarianism, 20.0),
        ];
        for (ideo, support) in &fallback_ideologies {
            let name = generator::generate_party_name(&country.name, cultural_group, *ideo, &mut rng);
            let organization = super::system::PartyOrganization::from_ideology_with_variance(*ideo, &mut rng);
            let vip = super::names::generate_full_vip(cultural_group, &mut rng);
            let leader = super::names::vip_to_leader(vip, ideo.as_str());
            let party = Party {
                ideology: ideo.as_str().to_string(),
                profile: ideo.profile().to_string(),
                economic_school: ideo.economic_school().to_string(),
                support: *support,
                base: ideo.base_weights().iter().map(|(g, _)| g.to_string()).collect(),
                id: format!("[PRT-ESC-{}]", ideo.as_str()),
                brokerage_account: None,
                loans: Vec::new(),
                organization,
                leader,
                ..Party::default()
            };
            country.politics.active_parties.insert(name, party);
        }
        // Remove the provisional government — real parties now compete.
        country.politics.active_parties.remove("Provisional Technocratic Government");
        messages.push("[ELECTION ESCAPE] Provisional government replaced by emergency party generation.".to_string());
    }

    // Phase 34: Election safety net — if the country is democratic but has
    // fewer than 3 active parties, inject additional parties with generated
    // leaders to ensure competitive elections. This breaks the "Provisional
    // Government" lock where a single stub party wins every election.
    let form = country.politics.government_form;
    if form.is_democratic() && country.politics.active_parties.len() < 3 {
        let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
            "slavic"
        } else {
            &country.macro_indicators.cultural_group
        };
        let mut rng = rand::thread_rng();
        let fallback_ideologies = [
            Ideology::SocialDemocracy,
            Ideology::SocialLiberalism,
            Ideology::ChristianDemocracy,
            Ideology::SocialConservatism,
            Ideology::Agrarianism,
        ];
        let mut idx = country.politics.active_parties.len();
        for ideo in &fallback_ideologies {
            if country.politics.active_parties.len() >= 3 {
                break;
            }
            // Skip if this ideology is already represented.
            let ideo_str = ideo.as_str().to_string();
            if country.politics.active_parties.values().any(|p| p.ideology == ideo_str) {
                continue;
            }
            let name = generator::generate_party_name(
                &country.name,
                cultural_group,
                *ideo,
                &mut rng,
            );
            let vip = super::names::generate_full_vip(cultural_group, &mut rng);
            let leader = super::names::vip_to_leader(vip, ideo.as_str());
            let organization = super::system::PartyOrganization::from_ideology_with_variance(*ideo, &mut rng);
            let party = Party {
                ideology: ideo.as_str().to_string(),
                profile: ideo.profile().to_string(),
                economic_school: ideo.economic_school().to_string(),
                support: 10.0, // Modest initial support
                base: ideo.base_weights().iter().map(|(g, _)| g.to_string()).collect(),
                id: format!("[PRT-{}]", idx),
                leader,
                organization,
                ..Party::default()
            };
            let unique_name = if country.politics.active_parties.contains_key(&name) {
                format!("{} {}", name, idx + 1)
            } else {
                name
            };
            country.politics.active_parties.insert(unique_name, party);
            idx += 1;
        }
    }

    // 2.5. Initialize party brokerage accounts and collect dues/donations
    // Initialize party brokerage accounts (no global map needed - company accounts accessed directly)
    for party in country.politics.active_parties.values_mut() {
        if party.brokerage_account.is_none() {
            party.brokerage_account = Some(crate::securities::BrokerageAccount {
                cash: 0.0,
                fx_balances: std::collections::HashMap::new(),
                portfolio: std::collections::BTreeMap::new(),
                pending_orders: std::collections::BTreeMap::new(),
                frozen_cash: 0.0,
                is_frozen: false,
                margin_account: None,
                extra: std::collections::HashMap::new(),
            });
        }
    }

    // Collect dues and donations
    for party in country.politics.active_parties.values_mut() {
        let party_support = party.support;
        let party_base = party.base.clone();
        
        party.collect_membership_dues(
            party_support,
            &party_base,
            &country.politics.interest_groups,
            companies,
            &mut country.regions,  // CHANGED: &mut for mutable access to class savings
        );
        
        party.accept_donations(
            companies,
        );
    }

    let form = country.politics.government_form;
    let unrest = country.macro_indicators.social_unrest;

    // 3. Regime safety check for democracies.
    if form.is_democratic()
        && (country.politics.election_method == "None" || country.politics.parliament.is_empty())
    {
        messages.push("[REGIME REPAIR] Democratic mechanisms restored.".to_string());
        country.politics.election_method = "D'Hondt".to_string();
        country.politics.years_to_elections = 0;
    }

    // Phase 38: Snap election trigger — if a democratic country is still
    // ruled by the Provisional Government or has fewer than 2 real parties
    // with nonzero support, force years_to_elections to 0 so an election
    // fires THIS turn. This breaks the permanent deadlock where the
    // provisional government wins every cycle because no real parties
    // compete. The guard (years_to_elections > 0) prevents infinite
    // re-election loops when the snap election itself fails to produce
    // a valid government.
    if form.is_democratic() && country.politics.years_to_elections > 0 {
        let has_provisional = country.politics.ruling_party == "Provisional Technocratic Government";
        let real_parties = country.politics.active_parties.values()
            .filter(|p| p.support > 0.0 && p.leader.name != "Provisional Technocratic Government")
            .count();
        if has_provisional || real_parties < 2 {
            country.politics.years_to_elections = 0;
            messages.push("[SNAP ELECTION] Forced election to break provisional government deadlock.".to_string());
        }
    }

    // 4. Countdown to elections.
    if country.politics.years_to_elections > 0 {
        country.politics.years_to_elections -= 1;
    }

    let election_due = country.politics.years_to_elections == 0
        || country.politics.budget_crisis
        || (country.politics.minority_government && unrest > 40.0);

    if form.is_democratic() && election_due {
        // Hold elections.
        let method = country.politics.election_method.clone();
        let threshold = country.politics.election_threshold;
        let seats = elections::calculate_seats(&country.politics.active_parties, &method, threshold, 100);
        country.politics.parliament = seats;

        let (winner, coalition, minority, coa_id) =
            elections::build_coalition(&country.politics.parliament, &country.politics.active_parties);
        country.politics.ruling_party = winner;
        country.politics.coalition = coalition;
        country.politics.minority_government = minority;
        country.politics.coalition_id = coa_id;
        country.politics.years_to_elections = form.election_cycle();
        country.politics.budget_crisis = false;

        apply_ruling_ideology_policies(country);

        let coalition_str = if country.politics.coalition.is_empty() {
            "with a decisive majority".to_string()
        } else {
            format!("with coalition ({})", country.politics.coalition.join(", "))
        };
        let minority_str = if country.politics.minority_government {
            "forming a fragile Minority Government".to_string()
        } else {
            coalition_str
        };
        messages.push(format!(
            "[ELECTION] Government formation mandate given to {} {}",
            country.politics.ruling_party, minority_str
        ));
    }

    // 5. Coalition stability.
    if form.is_democratic() && !country.politics.coalition.is_empty() {
        let (unstable, msg) = elections::check_coalition_stability(
            &country.politics.ruling_party,
            &country.politics.coalition,
            &country.politics.active_parties,
            unrest,
        );
        if unstable {
            messages.push(msg);
            country.politics.years_to_elections = 0;
            country.politics.budget_crisis = false;
        }
    }

    // 6. Recompute upper house.
    country.politics.upper_house = elections::calculate_upper_house_composition(
        &country.politics.constitution,
        &country.politics.active_parties,
        &country.politics.interest_groups,
        &country.politics.ruling_party,
    );

    // 7. NEW: Process espionage operations
    if let Some(espionage_state) = &mut country.politics.espionage_state {
        let mut councilors_map: HashMap<String, local_council::Councilor> = HashMap::new();
        // Collect councilors from all local councils
        for region in &country.regions {
            if let Some(governance) = &region.governance {
                for councilor in &governance.council.councilors {
                    councilors_map.insert(councilor.id.clone(), councilor.clone());
                }
            }
        }
        messages.extend(espionage_state.process_operations(year, &mut councilors_map));
    }

    // 8. NEW: Process committee bills
    if let Some(legislative_session) = &mut country.politics.legislative_session {
        messages.extend(legislative_session.process_turn(year));
    }

    // 9. NEW: Check rebellion triggers and spawn proto-states
    if !country.is_rebellion {
        let trigger = rebellions::RebellionTrigger::default();
        let tax_burden = country.tax_rates.income_tax.rate; // Simplified tax burden
        let war_exhaustion: f64 = country.military_fronts
            .iter()
            .flat_map(|f| f.war_exhaustion.get(&country.name).copied())
            .sum();
        let (_spawned_rebels, rebellion_messages) = rebellions::process_rebellion_spawning(
            country,
            &trigger,
            tax_burden,
            war_exhaustion,
        );
        messages.extend(rebellion_messages);
        // Note: Spawned rebels would be added to GameState in future integration
    }

    // Phase 39: Annual SOE Dividend Collection
    // For state-owned companies (state_share >= 1.0) with positive accumulated
    // profit, extract 30% as dividend to the treasury. Strict double-entry:
    // debit company liquid cash, credit treasury. If the company lacks cash,
    // the unpaid amount is tracked as arrears (no money creation).
    let soe_dividend_msgs = collect_annual_soe_dividends(country, companies);
    messages.extend(soe_dividend_msgs);

    // Phase 39: Physical Patent Fee Collection
    // Iterate over active companies that hold licensed blueprints. Each owes
    // a licensing fee to the state (patent holder). Debit liquid cash only;
    // unpaid fees are evaded (state receives nothing from broke companies).
    let patent_msgs = collect_patent_fees(country, companies);
    messages.extend(patent_msgs);

    messages
}

/// Phase 39: Collect annual SOE dividends from state-owned companies.
/// 30% of accumulated annual profit is transferred to the treasury.
fn collect_annual_soe_dividends(
    country: &mut Country,
    companies: &mut [crate::entities::Company],
) -> Vec<String> {
    let mut messages = Vec::new();
    const DIVIDEND_RATE: f64 = 0.30; // 30% of accumulated profit

    let mut total_dividends = 0.0_f64;
    let mut total_arrears = 0.0_f64;

    for company in companies.iter_mut() {
        if company.state_share < 1.0 {
            continue;
        }
        if company.annual_profit_accumulator <= 0.0 {
            continue;
        }

        let dividend = company.annual_profit_accumulator * DIVIDEND_RATE;
        // Debit from liquid cash: available_cash or brokerage_account.cash
        let available = company.available_cash
            + company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
        let actually_paid = dividend.min(available);

        if actually_paid > 0.0 {
            // Debit company cash
            if let Some(ref mut ba) = company.brokerage_account {
                let from_brokerage = actually_paid.min(ba.cash);
                ba.cash -= from_brokerage;
                let remainder = actually_paid - from_brokerage;
                company.available_cash -= remainder;
            } else {
                company.available_cash -= actually_paid;
            }
            // Credit treasury
            country.budget.liquid_reserves += actually_paid;
            total_dividends += actually_paid;
        }

        let unpaid = dividend - actually_paid;
        if unpaid > 0.0 {
            total_arrears += unpaid;
        }

        // Reset accumulator regardless of payment success
        company.annual_profit_accumulator = 0.0;
    }

    if total_dividends > 0.0 || total_arrears > 0.0 {
        messages.push(format!(
            "[SOE DIVIDEND] Collected {:.0} to treasury, {:.0} unpaid (arrears).",
            total_dividends, total_arrears
        ));
    }

    messages
}

/// Phase 39: Collect patent licensing fees from companies using licensed blueprints.
/// Each company with licensed_blueprints owes a fee per patent. Debit liquid cash
/// only; unpaid fees are evaded (state receives nothing).
fn collect_patent_fees(
    country: &mut Country,
    companies: &mut [crate::entities::Company],
) -> Vec<String> {
    let mut messages = Vec::new();
    const FEE_PER_PATENT: f64 = 5000.0; // Flat fee per licensed blueprint

    let mut total_collected = 0.0_f64;
    let mut total_evaded = 0.0_f64;

    for company in companies.iter_mut() {
        if company.licensed_blueprints.is_empty() {
            continue;
        }
        let num_patents = company.licensed_blueprints.len() as f64;
        let fee_owed = num_patents * FEE_PER_PATENT;

        // Debit from liquid cash only
        let available = company.available_cash
            + company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
        let actually_paid = fee_owed.min(available);

        if actually_paid > 0.0 {
            if let Some(ref mut ba) = company.brokerage_account {
                let from_brokerage = actually_paid.min(ba.cash);
                ba.cash -= from_brokerage;
                let remainder = actually_paid - from_brokerage;
                company.available_cash -= remainder;
            } else {
                company.available_cash -= actually_paid;
            }
            country.budget.liquid_reserves += actually_paid;
            total_collected += actually_paid;
        }

        let evaded = fee_owed - actually_paid;
        if evaded > 0.0 {
            total_evaded += evaded;
        }
    }

    if total_collected > 0.0 || total_evaded > 0.0 {
        messages.push(format!(
            "[PATENT FEES] Collected {:.0}, evaded {:.0}.",
            total_collected, total_evaded
        ));
    }

    messages
}

/// Process the full political turn (Phase 11) — calls all political orchestrators in order.
///
/// # Arguments
/// * `country` - Mutable country
/// * `companies` - Mutable companies
/// * `unions` - Mutable unions
/// * `councilors` - Councilors for floor votes
/// * `chaos_config` - Chaos config for mass movement spawn checks
/// * `trait_registry` - Optional trait registry for leader trait modifiers
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of all diagnostic messages from all sub-phases
///
/// # Rules
/// * Phase 11a: Conservation (parks, tourism, policy expiry)
/// * Phase 11b: Mass movements (spawn, disruption, strike funds, resolution)
/// * Phase 11c: Election cycle (state machine advancement, elections)
/// * Phase 11d: Campaign spending (if campaign is active)
/// * Phase 11e: Lobbying (dues, legal lobbying, bribery)
/// * Phase 11f: Legislation (bill lifecycle, committee → floor → executive)
/// * Phase 11g: Leader traits (apply trait modifiers to economic configs)
/// * All financial flows follow strict double-entry accounting.
pub fn process_political_turn(
    country: &mut Country,
    companies: &mut Vec<crate::entities::Company>,
    unions: &mut Vec<crate::entities::union::Union>,
    councilors: &[super::local_council::Councilor],
    chaos_config: &super::chaos_config::ChaosConfig,
    trait_registry: Option<&super::traits::TraitRegistry>,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Phase 48: Drain pending unnatural deaths and trigger immediate succession.
    // This prevents the "Zombie Leader" time loop where a leader assassinated
    // on turn 5 remains in power until turn 24 (year boundary).
    // Unnatural deaths (Assassination, Coup, Suicide, Execution, Battle, Accident)
    // are queued during events and processed here at the start of each turn.
    if let Some(ref mut registry) = country.politics.vip_registry {
        let pending = registry.drain_pending_deaths();
        for death in pending {
            messages.push(format!(
                "[SUCCESSION] VIP {} died of {:?} on turn {} — triggering immediate succession.",
                death.vip_id, death.cause, death.turn
            ));
            // TODO: Call process_succession() here once the succession engine
            // is fully implemented (Section 2 of Phase 48).
            // For now, the death is recorded and the message is logged.
        }
    }

    // Phase 45: Single global VIP deduplication set for this political turn.
    // Pre-populated with all party leader names so generated VIPs never collide.
    let mut used_vip_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for party in country.politics.active_parties.values() {
        if !party.leader.name.is_empty() {
            used_vip_names.insert(party.leader.name.clone());
        }
    }

    // Phase 11a: Conservation (needs country + regions — use mem::take to avoid double borrow)
    let mut regions = std::mem::take(&mut country.regions);
    let conservation_msgs = super::conservation::process_conservation_turn(
        country,
        &mut regions,
        current_turn,
    );
    messages.extend(conservation_msgs);

    // Phase 11b: Mass movements (needs country + companies + regions + unions)
    let movement_msgs = super::mass_movements::process_mass_movements_turn(
        country,
        companies,
        &mut regions,
        unions,
        chaos_config,
        current_turn,
    );
    messages.extend(movement_msgs);

    // Phase 11d: Campaign spending (needs country + parties + companies + regions)
    let mut active_parties = std::mem::take(&mut country.politics.active_parties);
    let campaign_msgs = super::campaign::process_campaign_spending(
        country,
        &mut active_parties,
        companies,
        &mut regions,
        current_turn,
    );
    messages.extend(campaign_msgs);

    // Restore regions
    country.regions = regions;

    // Phase 11c: Election cycle (needs country + parties)
    let election_msgs = super::campaign::process_election_cycle(
        country,
        &mut active_parties,
        current_turn,
    );
    messages.extend(election_msgs);

    // Phase 32: Initialize/update Parliament struct after elections.
    // Use the country's cultural group (default to slavic if not set).
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic".to_string()
    } else {
        country.macro_indicators.cultural_group.clone()
    };
    let mut rng = rand::thread_rng();
    let parliament = super::parliament::initialize_parliament(
        &country.politics,
        &cultural_group,
        current_turn,
        &mut rng,
        &mut used_vip_names,
    );
    country.politics.parliament_struct = Some(parliament);

    // Phase 54: Assign chairperson VIPs to each parliamentary club.
    if let Some(ref mut parl) = country.politics.parliament_struct {
        if let Some(ref mut registry) = country.politics.vip_registry {
            super::parliament::assign_club_chairpersons(
                parl,
                registry,
                &cultural_group,
                &country.name,
                &mut rng,
            );
        }
    }

    // Phase 32: Check for mid-term faction splintering.
    if let Some(ref mut parl) = country.politics.parliament_struct {
        // Use ruling party support as a proxy for approval rating.
        let approval = country.politics.active_parties
            .get(&country.politics.ruling_party)
            .map(|p| p.support)
            .unwrap_or(50.0);
        // Use macro_indicators.social_unrest for unrest level.
        let unrest = country.macro_indicators.social_unrest;
        let splinter_events = super::parliament::check_faction_splintering(
            parl,
            &country.politics.active_parties,
            approval,
            unrest,
            current_turn,
        );
        for event in splinter_events {
            messages.push(format!(
                "[SPLINTER] {} seats defected from {} to {} ({})",
                event.seats_defected, event.source_club, event.new_club, event.reason
            ));
        }
    }

    // Phase 32: Tick State of Emergency if active.
    if let Some(ref mut soe) = country.politics.state_of_emergency {
        if soe.active {
            soe.tick();
            if !soe.active {
                // Restore parliament.
                if let Some(ref mut parl) = country.politics.parliament_struct {
                    parl.suspended = false;
                }
                messages.push("[STATE OF EMERGENCY] Auto-expired — Parliament resumes.".to_string());
            }
        }
    }

    // Phase 32: Regenerate political capital.
    let ruling_support = country.politics.active_parties
        .get(&country.politics.ruling_party)
        .map(|p| p.support)
        .unwrap_or(50.0);
    let coalition_stability = if country.politics.minority_government { 0.5 } else { 1.0 };
    country.politics.political_capital = 50.0 + ruling_support * 0.5 * coalition_stability;

    // Phase 11e: Lobbying (needs country + companies + parties + bills)
    let lobbying_msgs = super::lobbying::process_lobbying_turn(
        country,
        companies,
        &mut active_parties,
        &mut Vec::new(),
        current_turn,
    );
    messages.extend(lobbying_msgs);

    // Restore parties
    country.politics.active_parties = active_parties;

    // Phase 11f: Legislation (needs country + councilors + parties)
    let active_parties_ref = country.politics.active_parties.clone();
    let legislation_msgs = super::bill_lifecycle::process_legislation_turn(
        country,
        councilors,
        &active_parties_ref,
        current_turn,
    );
    messages.extend(legislation_msgs);

    // Phase 86: Advisory Council turn — loyalty drift, influence modifiers, coup checks.
    // Called after legislation so council modifiers don't affect the same turn's votes.
    let council_msgs = process_advisory_council_turn(country, current_turn);
    messages.extend(council_msgs);

    // Phase 86: Dynasty turn — marriages, births, succession order updates.
    let dynasty_msgs = process_dynasty_turn(country, current_turn);
    messages.extend(dynasty_msgs);

    // Phase 11g: Leader traits
    let trait_msgs = super::traits::process_leader_traits_turn(
        country,
        trait_registry,
    );
    messages.extend(trait_msgs);

    messages
}

/// Regenerates party support from ideology bids and the existing party roster.
fn regenerate_parties(politics: &Politics, country_name: &str, year: u32, cultural_group: &str) -> HashMap<String, Party> {
    let ig_power = &politics.interest_groups;
    let threshold = politics.election_threshold;
    let old_parties = &politics.active_parties;
    let parliament = &politics.parliament;
    let cultural_group = if cultural_group.is_empty() { "slavic" } else { cultural_group };

    let mut bids: HashMap<Ideology, f64> = HashMap::new();
    for ideo in [
        Ideology::OrthodoxMarxism,
        Ideology::MarxismLeninism,
        Ideology::Maoism,
        Ideology::SocialDemocracy,
        Ideology::GreenPolitics,
        Ideology::ClassicalLiberalism,
        Ideology::SocialLiberalism,
        Ideology::Agrarianism,
        Ideology::ChristianDemocracy,
        Ideology::SocialConservatism,
        Ideology::Neoconservatism,
        Ideology::Neoliberalism,
        Ideology::NationalConservatism,
        Ideology::AnarchoCapitalism,
        Ideology::Fascism,
    ] {
        let mut bid = ideo.base_bid(ig_power) * ideo.year_multiplier(year);
        if year < ideo.required_year() {
            bid = 0.0;
        }
        bids.insert(ideo, bid);
    }

    let mut new_parties: HashMap<String, Party> = HashMap::new();
    let mut used_ideologies: Vec<Ideology> = Vec::new();
    let mut rng = rand::thread_rng();

    // Preserve existing parties when their ideology still has a bid or they
    // already hold parliamentary seats.
    for (name, party) in old_parties {
        if let Some(ideo) = Ideology::from_name(&party.ideology) {
            if let Some(&bid) = bids.get(&ideo) {
                if bid > threshold || parliament.contains_key(name) {
                    let mut updated = party.clone();
                    updated.support = bid;
                    // Phase 34: Backfill empty leader names on preserved parties.
                    // This fixes the "Provisional Government" lock where the stub
                    // party was created with ..Party::default() (empty leader.name)
                    // and then preserved indefinitely via party.clone().
                    if updated.leader.name.is_empty() {
                        let vip = super::names::generate_full_vip(cultural_group, &mut rng);
                        updated.leader = super::names::vip_to_leader(vip, &updated.ideology);
                    }
                    new_parties.insert(name.clone(), updated);
                    used_ideologies.push(ideo);
                }
            }
        }
    }

    // Create deterministic new parties for ideologies that crossed the threshold
    // but are not represented by an existing party.
    for (ideo, bid) in bids {
        if bid > threshold && !used_ideologies.contains(&ideo) {
            // Use procedural name generator
            let name = generator::generate_party_name(country_name, cultural_group, ideo, &mut rng);
            let organization = super::system::PartyOrganization::from_ideology_with_variance(ideo, &mut rng);
            // Phase 33: Generate a named leader for the new party.
            let vip = super::names::generate_full_vip(cultural_group, &mut rng);
            let leader = super::names::vip_to_leader(vip, ideo.as_str());
            let mut party = Party {
                ideology: ideo.as_str().to_string(),
                profile: ideo.profile().to_string(),
                economic_school: ideo.economic_school().to_string(),
                support: bid,
                base: ideo.base_weights().iter().map(|(g, _)| g.to_string()).collect(),
                id: format!("[PRT-{}]", new_parties.len()),
                brokerage_account: None,  // Will be initialized during banking integration
                loans: Vec::new(),
                organization: organization.clone(),
                leader,
                ..Party::default()
            };
            // Phase 42: Clean party name generation — no numeric suffixes.
            // Redraw the full name from the expanded pool until unique (up to 20 attempts).
            let mut unique_name = name;
            let mut attempts = 0;
            while new_parties.contains_key(&unique_name) && attempts < 20 {
                unique_name = generator::generate_party_name(country_name, cultural_group, ideo, &mut rng);
                attempts += 1;
            }
            if new_parties.contains_key(&unique_name) {
                // Still colliding after 20 attempts — skip, ideology already represented.
                continue;
            }
            party.id = format!("[PRT-{}]", ideo.as_str());
            new_parties.insert(unique_name, party);
            used_ideologies.push(ideo);
        }
    }

    let total_support: f64 = new_parties.values().map(|p| p.support).sum();
    if total_support == 0.0 {
        // Phase 34: Generate a named leader for the provisional government.
        // The old stub used ..Party::default() which left leader.name empty,
        // causing the "(unnamed)" bug in the UI and locking the election cycle.
        let vip = super::names::generate_full_vip(cultural_group, &mut rng);
        let leader = super::names::vip_to_leader(vip, "Social Liberalism");
        new_parties.clear();
        new_parties.insert(
            "Provisional Technocratic Government".to_string(),
            Party {
                ideology: "Social Liberalism".to_string(),
                profile: "Centrist".to_string(),
                economic_school: "Monetarist".to_string(),
                support: 100.0,
                base: vec!["Bureaucrats".to_string(), "Specialists".to_string()],
                id: "[PRT-000]".to_string(),
                leader,
                ..Party::default()
            },
        );
    } else {
        for party in new_parties.values_mut() {
            party.support = (party.support / total_support) * 100.0;
        }
    }

    new_parties
}

/// Phase 39: Check if a snap election should be forced. Called every turn
/// (not just at year boundaries) to break provisional government deadlocks
/// immediately. Includes a cooldown to prevent infinite election loops.
pub fn check_snap_election(country: &mut Country, current_turn: u32) -> Vec<String> {
    let mut messages = Vec::new();
    let form = country.politics.government_form;

    if !form.is_democratic() {
        return messages;
    }

    // Cooldown: don't trigger snap election if one was triggered recently
    // (within 4 turns = ~2 months). This prevents infinite loops when
    // election formation fails to produce a valid government.
    const SNAP_ELECTION_COOLDOWN: u32 = 4;
    if current_turn.saturating_sub(country.politics.last_snap_election_turn) < SNAP_ELECTION_COOLDOWN {
        return messages;
    }

    let has_provisional = country.politics.ruling_party == "Provisional Technocratic Government";
    let real_parties = country.politics.active_parties.values()
        .filter(|p| p.support > 0.0 && p.leader.name != "Provisional Technocratic Government")
        .count();

    if (has_provisional || real_parties < 2) && country.politics.years_to_elections > 0 {
        country.politics.years_to_elections = 0;
        country.politics.last_snap_election_turn = current_turn;
        messages.push("[SNAP ELECTION] Forced election to break provisional government deadlock.".to_string());
    }

    messages
}

/// Phase 39: Run election if one is due. Called every turn (not just at year
/// boundaries) so snap elections take effect immediately. Returns messages.
pub fn run_election_if_due(country: &mut Country, unrest: f64, current_turn: u32) -> Vec<String> {
    let mut messages = Vec::new();
    let form = country.politics.government_form;

    if !form.is_democratic() {
        return messages;
    }

    let election_due = country.politics.years_to_elections == 0
        || country.politics.budget_crisis
        || (country.politics.minority_government && unrest > 40.0);

    if !election_due {
        return messages;
    }

    // Phase 45: Single global VIP deduplication set for this election cycle.
    // Pre-populated with all party leader names so generated VIPs never collide.
    let mut used_vip_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for party in country.politics.active_parties.values() {
        if !party.leader.name.is_empty() {
            used_vip_names.insert(party.leader.name.clone());
        }
    }

    // Hold elections.
    let method = country.politics.election_method.clone();
    let threshold = country.politics.election_threshold;
    let seats = elections::calculate_seats(&country.politics.active_parties, &method, threshold, 100);
    country.politics.parliament = seats;

    let (winner, coalition, minority, coa_id) =
        elections::build_coalition(&country.politics.parliament, &country.politics.active_parties);
    country.politics.ruling_party = winner;
    country.politics.coalition = coalition;
    country.politics.minority_government = minority;
    country.politics.coalition_id = coa_id;
    country.politics.years_to_elections = form.election_cycle();
    country.politics.budget_crisis = false;

    apply_ruling_ideology_policies(country);

    // Phase 40: Initialize/update Parliament struct after elections.
    // Previously, run_election_if_due only updated the flat parliament HashMap
    // but never rebuilt parliament_struct, leaving the UI showing "No Parliament".
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic".to_string()
    } else {
        country.macro_indicators.cultural_group.clone()
    };
    let mut rng = rand::thread_rng();
    let parliament = super::parliament::initialize_parliament(
        &country.politics,
        &cultural_group,
        current_turn,
        &mut rng,
        &mut used_vip_names,
    );
    country.politics.parliament_struct = Some(parliament);

    // Phase 54: Assign chairperson VIPs to each parliamentary club.
    if let Some(ref mut parl) = country.politics.parliament_struct {
        if let Some(ref mut registry) = country.politics.vip_registry {
            super::parliament::assign_club_chairpersons(
                parl,
                registry,
                &cultural_group,
                &country.name,
                &mut rng,
            );
        }
    }

    // Phase 40: Reform government ministries with the new coalition.
    // Form a new government after elections to update ministry composition.
    let active_parties = country.politics.active_parties.clone();
    let new_config = super::ministries::form_government(
        country,
        &country.politics.coalition,
        &active_parties,
        current_turn,
        &mut used_vip_names,
    );
    country.politics.ministry_config = Some(new_config);

    // Phase 43: Initialize the parliamentary committee system.
    // This was never called before, so committee_system was always None and
    // the Parliament tab showed no committees.
    let mut cs = super::committees::CommitteeSystem::default();
    cs.initialize_committees(
        &country.politics.parliament,
        &country.politics.coalition,
    );
    country.politics.committee_system = Some(cs);

    // Phase 40: Calculate budget needs for the new government immediately.
    super::ministries::calculate_budget_needs(country);

    let coalition_str = if country.politics.coalition.is_empty() {
        "with a decisive majority".to_string()
    } else {
        format!("with coalition ({})", country.politics.coalition.join(", "))
    };
    let minority_str = if country.politics.minority_government {
        "forming a fragile Minority Government".to_string()
    } else {
        coalition_str
    };
    messages.push(format!(
        "[ELECTION] Government formation mandate given to {} {}",
        country.politics.ruling_party, minority_str
    ));

    messages
}

pub fn apply_ruling_ideology_policies(country: &mut Country) {
    let ruling_ideology = country
        .politics
        .active_parties
        .get(&country.politics.ruling_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or_default();
    let prefs = ruling_ideology.preferences();

    country.politics.government_economic_school = ruling_ideology.economic_school().to_string();
    country.politics.trade_doctrine = prefs.trade_doctrine.to_string();
    // Phase 29: Set commodity-specific tariffs based on the trade doctrine.
    crate::politics::trade_policy::set_tariffs_from_doctrine(country);
    country.politics.labor_law = prefs.labor_law.to_string();
    country.politics.health_service = prefs.health_service.to_string();
    country.politics.sanitation_policy = prefs.sanitation.to_string();
    country.politics.union_law = prefs.union_law.to_string();
    country.politics.strike_law = prefs.strike_law.to_string();
    country.politics.education_model = prefs.education_model.to_string();
    country.politics.school_system = prefs.school_system.to_string();
    country.politics.religious_law = prefs.religion.to_string();
    country.politics.civil_rights_law = prefs.citizenship.to_string();
    country.politics.emancipation_law = prefs.emancipation.to_string();

    // Phase 39: Apply ideology-driven tax policy every turn.
    apply_ideology_tax_policy(country, ruling_ideology);
}

/// Phase 39: Sets wealth tax and capital gains tax brackets based on the
/// ruling ideology's economic school. Applied every turn to ensure
/// ideological consistency. Player agency is expressed through elections.
fn apply_ideology_tax_policy(country: &mut Country, ideology: Ideology) {
    use crate::state::tax::{TaxBracket, WealthTax, CapitalGainsTax};
    use serde_json::Map;

    let (wealth_brackets, cg_brackets): (Vec<TaxBracket>, Vec<TaxBracket>) = match ideology {
        // Socialist/Marxist: heavy wealth tax, high capital gains
        Ideology::OrthodoxMarxism | Ideology::MarxismLeninism | Ideology::Maoism => (
            vec![
                TaxBracket { threshold: 1_000_000.0, rate: 0.02, extra: Map::new() },
                TaxBracket { threshold: 10_000_000.0, rate: 0.05, extra: Map::new() },
            ],
            vec![TaxBracket { threshold: 0.0, rate: 0.30, extra: Map::new() }],
        ),
        // Social Democratic: moderate wealth tax, standard capital gains
        Ideology::SocialDemocracy | Ideology::GreenPolitics => (
            vec![
                TaxBracket { threshold: 2_000_000.0, rate: 0.01, extra: Map::new() },
                TaxBracket { threshold: 10_000_000.0, rate: 0.03, extra: Map::new() },
            ],
            vec![TaxBracket { threshold: 0.0, rate: 0.19, extra: Map::new() }],
        ),
        // Keynesian/Social Liberal: light wealth tax, standard capital gains
        Ideology::SocialLiberalism => (
            vec![
                TaxBracket { threshold: 5_000_000.0, rate: 0.005, extra: Map::new() },
            ],
            vec![TaxBracket { threshold: 0.0, rate: 0.19, extra: Map::new() }],
        ),
        // Centrist/Agrarian: baseline wealth tax, standard capital gains
        Ideology::Agrarianism | Ideology::ChristianDemocracy => (
            vec![
                TaxBracket { threshold: 5_000_000.0, rate: 0.01, extra: Map::new() },
            ],
            vec![TaxBracket { threshold: 0.0, rate: 0.19, extra: Map::new() }],
        ),
        // Classical Liberal: no wealth tax, lower capital gains
        Ideology::ClassicalLiberalism => (
            vec![],
            vec![TaxBracket { threshold: 0.0, rate: 0.15, extra: Map::new() }],
        ),
        // Conservative: no wealth tax, standard capital gains
        Ideology::SocialConservatism | Ideology::NationalConservatism | Ideology::Neoconservatism => (
            vec![],
            vec![TaxBracket { threshold: 0.0, rate: 0.19, extra: Map::new() }],
        ),
        // Neoliberal: no wealth tax, low capital gains
        Ideology::Neoliberalism => (
            vec![],
            vec![TaxBracket { threshold: 0.0, rate: 0.10, extra: Map::new() }],
        ),
        // Anarcho-Capitalist: no taxes at all
        Ideology::AnarchoCapitalism => (
            vec![],
            vec![],
        ),
        // Fascism: state-controlled, moderate wealth tax, high capital gains
        Ideology::Fascism => (
            vec![
                TaxBracket { threshold: 2_000_000.0, rate: 0.02, extra: Map::new() },
            ],
            vec![TaxBracket { threshold: 0.0, rate: 0.25, extra: Map::new() }],
        ),
    };

    country.tax_rates.wealth_tax = WealthTax {
        brackets: wealth_brackets,
        asset_types: vec!["liquid_capital".to_string(), "real_estate".to_string()],
        extra: Map::new(),
    };
    country.tax_rates.capital_gains_tax = CapitalGainsTax {
        brackets: cg_brackets,
        holding_period_modifier: 1.0,
        extra: Map::new(),
    };
}

/// Bootstraps a fresh `Turn 0` political block for a generated country.
///
/// # Rules
/// * Picks a random government form, constitution, judiciary and head of state.
/// * Runs `process_political_year` to generate parties and (for democracies) the
///   first election outcome.
/// * For non-democratic forms, installs the strongest party as the ruling party
///   and applies its ideology's policies.
/// * Fills `head_of_state` and `dynasty` for monarchies.
pub fn bootstrap_politics(country: &mut Country, companies: &mut Vec<crate::entities::Company>, year: u32, rng: &mut impl Rng) {
    let forms = [
        GovernmentForm::ParliamentaryDemocracy,
        GovernmentForm::PresidentialRepublic,
        GovernmentForm::SemiPresidentialRepublic,
        GovernmentForm::DirectorialDemocracy,
        GovernmentForm::ConstitutionalMonarchy,
        GovernmentForm::DualistMonarchy,
        GovernmentForm::ElectiveMonarchy,
        GovernmentForm::AbsoluteMonarchy,
        GovernmentForm::OnePartyState,
        GovernmentForm::MilitaryDictatorship,
        GovernmentForm::Theocracy,
    ];
    let form = forms[rng.gen_range(0..forms.len())];

    country.politics.government_form = form;
    country.politics.election_method = if form.is_democratic() {
        "D'Hondt".to_string()
    } else {
        "None".to_string()
    };
    country.politics.election_threshold = if form.is_democratic() {
        rng.gen_range(3.0..7.0) / 100.0
    } else {
        0.0
    };
    country.politics.years_to_elections = 0;
    country.politics.constitution = build_constitution(form, rng);
    country.politics.dynasty = if is_monarchy(form) {
        Some(random_dynasty(rng))
    } else {
        None
    };
    // Phase 91: Shared used_names set for key political figure uniqueness.
    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    country.politics.head_of_state = random_head_of_state(country, form, rng, &mut used_names);

    process_political_year(country, companies, &mut [], year);

    // Phase 91: Deduplicate party leader names against the Head of State.
    // Party leaders were generated inside process_political_year using
    // generate_full_vip (generic, allows duplicates). If any party leader
    // has the same name as the Head of State, regenerate their name using
    // generate_key_vip for uniqueness among key political figures.
    {
        let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
            "slavic".to_string()
        } else {
            country.macro_indicators.cultural_group.clone()
        };
        let mut rng_dedup = rand::thread_rng();
        for party in country.politics.active_parties.values_mut() {
            // Check if the leader name (without title prefix) collides.
            let leader_name = &party.leader.name;
            // Strip title prefix (e.g., "King John Smith" -> "John Smith")
            let base_name = leader_name.split_whitespace()
                .skip_while(|w| w == &"King" || w == &"Queen" || w == &"President" || w == &"Leader")
                .collect::<Vec<_>>()
                .join(" ");
            if used_names.contains(&base_name) {
                // Regenerate with a unique name
                let new_vip = super::names::generate_key_vip(&cultural_group, &mut rng_dedup, &mut used_names);
                party.leader.name = new_vip.full_name;
                party.leader.gender = new_vip.gender;
            } else {
                used_names.insert(base_name);
            }
        }
    }

    // Phase 51: Initialize VIP registry and register all key political figures.
    if country.politics.vip_registry.is_none() {
        country.politics.vip_registry = Some(VipRegistry::new());
    }
    let registry = country.politics.vip_registry.as_mut().unwrap();
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic"
    } else {
        &country.macro_indicators.cultural_group
    };

    // Register Head of State.
    let hos = &country.politics.head_of_state;
    let hos_role = if is_monarchy(form) {
        VipRoleExtended::Monarch
    } else {
        VipRoleExtended::HeadOfState
    };
    let monarch_vip_id = registry.register_new(Vip {
        full_name: hos.name.clone(),
        gender: hos.gender.clone(),
        age: hos.age,
        health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
        traits: hos.traits.clone(),
        main_trait: hos.main_trait.clone(),
        ideology: hos.views.clone(),
        religion: hos.religion.clone(),
        nationality: country.name.clone(),
        dynasty: hos.dynasty.clone(),
        roles: vec![hos_role],
        base_influence: hos.base_influence,
        faction: hos.faction.clone(),
        ..Default::default()
    });

    // Phase 91: Initialize royal_dynasty for monarchies with monarch, consort,
    // and 1-2 royal heirs. Previously, only `politics.dynasty` (a string) was
    // set, but `politics.royal_dynasty` (the RoyalDynasty struct) was never
    // initialized, so process_dynasty_turn early-returned and no consorts or
    // heirs were ever generated.
    if is_monarchy(form) {
        let dynasty_name = country.politics.dynasty.clone().unwrap_or_else(|| "Royal".to_string());
        let monarch_gender = hos.gender.clone();
        let monarch_age = hos.age;

        // Create the dynasty with the monarch as the first member.
        let mut royal_dynasty = super::succession::RoyalDynasty::new(dynasty_name.clone());
        royal_dynasty.current_monarch_id = Some(monarch_vip_id.clone());
        royal_dynasty.members.push(super::succession::RoyalFamilyMember {
            vip_id: monarch_vip_id.clone(),
            relation: super::succession::RoyalRelation::Monarch,
            birth_turn: 0, // Genesis — birth turn not tracked for initial monarch
            is_legitimate: true,
            is_heir_apparent: false,
            succession_order: 0,
            father_vip_id: None,
            mother_vip_id: None,
            spouse_vip_id: None, // Will be set below after consort generation
            children_vip_ids: Vec::new(),
            marriage_turn: None,
            death_turn: None,
            death_cause: None,
        });

        // Generate a royal consort (spouse for the monarch).
        // Phase 92: Use gender-aware key VIP generation to ensure the consort's
        // name matches their assigned gender (e.g., female consort gets a
        // female first name, not a male name).
        let consort_gender = if monarch_gender == "M" { "F" } else { "M" };
        let consort_vip_name = super::names::generate_key_vip_with_gender(cultural_group, consort_gender, rng, &mut used_names);
        let (consort_traits, consort_main_trait) = assign_core_traits(rng);
        let consort_vip_id = registry.register_new(Vip {
            full_name: consort_vip_name.full_name.clone(),
            gender: consort_gender.to_string(),
            age: 18 + rng.gen_range(0..15), // Consort aged 18-32
            health: crate::politics::vip_registry::VipHealth { physical_health: 0.9, mental_health: 0.9 },
            traits: consort_traits,
            main_trait: consort_main_trait,
            ideology: String::new(),
            religion: country.macro_indicators.religion.clone(),
            nationality: country.name.clone(),
            dynasty: Some(dynasty_name.clone()),
            roles: vec![VipRoleExtended::RoyalConsort],
            base_influence: 20,
            faction: "Royal Court".to_string(),
            ..Default::default()
        });

        // Link consort to monarch in the dynasty.
        if let Some(monarch_member) = royal_dynasty.members.iter_mut().find(|m| m.vip_id == monarch_vip_id) {
            monarch_member.spouse_vip_id = Some(consort_vip_id.clone());
            monarch_member.marriage_turn = Some(0); // Genesis marriage
        }
        royal_dynasty.members.push(super::succession::RoyalFamilyMember {
            vip_id: consort_vip_id.clone(),
            relation: super::succession::RoyalRelation::Consort,
            birth_turn: 0,
            is_legitimate: true,
            is_heir_apparent: false,
            succession_order: 999, // Consorts are not in succession line
            father_vip_id: None,
            mother_vip_id: None,
            spouse_vip_id: Some(monarch_vip_id.clone()),
            children_vip_ids: Vec::new(),
            marriage_turn: Some(0),
            death_turn: None,
            death_cause: None,
        });

        // Generate 1-2 royal heirs (children of monarch and consort).
        let num_heirs = if monarch_age >= 25 { 2 } else { 1 };
        let mut children_ids = Vec::new();
        for heir_idx in 0..num_heirs {
            // Phase 92: Select heir gender FIRST, then generate name with that
            // gender to ensure name-gender consistency.
            let heir_gender = if rng.gen::<f64>() < 0.5 { "M" } else { "F" };
            let heir_vip_name = super::names::generate_key_vip_with_gender(cultural_group, heir_gender, rng, &mut used_names);
            let (heir_traits, heir_main_trait) = assign_core_traits(rng);
            let heir_age = if monarch_age > 30 { rng.gen_range(5..25) } else { rng.gen_range(1..10) };
            let heir_vip_id = registry.register_new(Vip {
                full_name: heir_vip_name.full_name.clone(),
                gender: heir_gender.to_string(),
                age: heir_age,
                health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                traits: heir_traits,
                main_trait: heir_main_trait,
                ideology: String::new(),
                religion: country.macro_indicators.religion.clone(),
                nationality: country.name.clone(),
                dynasty: Some(dynasty_name.clone()),
                roles: vec![VipRoleExtended::RoyalHeir],
                base_influence: 15,
                faction: "Royal Court".to_string(),
                ..Default::default()
            });
            let is_heir_apparent = heir_idx == 0;
            royal_dynasty.members.push(super::succession::RoyalFamilyMember {
                vip_id: heir_vip_id.clone(),
                relation: super::succession::RoyalRelation::Child,
                birth_turn: 0,
                is_legitimate: true,
                is_heir_apparent,
                succession_order: (heir_idx + 1) as u32,
                father_vip_id: if monarch_gender == "M" { Some(monarch_vip_id.clone()) } else { None },
                mother_vip_id: if monarch_gender == "F" { Some(monarch_vip_id.clone()) } else { None },
                spouse_vip_id: None,
                children_vip_ids: Vec::new(),
                marriage_turn: None,
                death_turn: None,
                death_cause: None,
            });
            children_ids.push(heir_vip_id);
        }

        // Link children to both monarch and consort.
        if let Some(monarch_member) = royal_dynasty.members.iter_mut().find(|m| m.vip_id == monarch_vip_id) {
            monarch_member.children_vip_ids = children_ids.clone();
        }
        if let Some(consort_member) = royal_dynasty.members.iter_mut().find(|m| m.vip_id == consort_vip_id) {
            consort_member.children_vip_ids = children_ids;
        }

        // Phase 92: Extended royal dynasty — siblings, uncles/aunts, cousins.
        // This creates a realistic royal court with multiple family members
        // in the line of succession, not just monarch + consort + children.
        let mut next_succession_order = (num_heirs + 1) as u32;

        // 1. Generate 1-2 siblings of the monarch (princes/princesses).
        let num_siblings = 1 + rng.gen_range(0..2); // 1-2 siblings
        let mut sibling_ids = Vec::new();
        for _ in 0..num_siblings {
            let sibling_gender = if rng.gen::<f64>() < 0.5 { "M" } else { "F" };
            let sibling_name = super::names::generate_key_vip_with_gender(cultural_group, sibling_gender, rng, &mut used_names);
            let (sib_traits, sib_main_trait) = assign_core_traits(rng);
            let sibling_age = (monarch_age as i32 + rng.gen_range(-5..6)).max(18) as u32;
            let sibling_vip_id = registry.register_new(Vip {
                full_name: sibling_name.full_name.clone(),
                gender: sibling_gender.to_string(),
                age: sibling_age.max(18),
                health: crate::politics::vip_registry::VipHealth { physical_health: 0.9, mental_health: 0.9 },
                traits: sib_traits,
                main_trait: sib_main_trait,
                ideology: String::new(),
                religion: country.macro_indicators.religion.clone(),
                nationality: country.name.clone(),
                dynasty: Some(dynasty_name.clone()),
                roles: vec![VipRoleExtended::RoyalHeir],
                base_influence: 10,
                faction: "Royal Court".to_string(),
                ..Default::default()
            });
            royal_dynasty.members.push(super::succession::RoyalFamilyMember {
                vip_id: sibling_vip_id.clone(),
                relation: super::succession::RoyalRelation::Sibling,
                birth_turn: 0,
                is_legitimate: true,
                is_heir_apparent: false,
                succession_order: next_succession_order,
                father_vip_id: None, // Siblings share monarch's parents (not tracked)
                mother_vip_id: None,
                spouse_vip_id: None,
                children_vip_ids: Vec::new(),
                marriage_turn: None,
                death_turn: None,
                death_cause: None,
            });
            next_succession_order += 1;
            sibling_ids.push(sibling_vip_id);
        }

        // 2. Generate 1-2 uncles/aunts (older than monarch).
        let num_uncles_aunts = 1 + rng.gen_range(0..2); // 1-2 uncles/aunts
        let mut uncle_aunt_ids = Vec::new();
        for _ in 0..num_uncles_aunts {
            let ua_gender = if rng.gen::<f64>() < 0.5 { "M" } else { "F" };
            let ua_name = super::names::generate_key_vip_with_gender(cultural_group, ua_gender, rng, &mut used_names);
            let (ua_traits, ua_main_trait) = assign_core_traits(rng);
            let ua_age = monarch_age + 15 + rng.gen_range(0..11); // 15-25 years older
            let ua_vip_id = registry.register_new(Vip {
                full_name: ua_name.full_name.clone(),
                gender: ua_gender.to_string(),
                age: ua_age,
                health: crate::politics::vip_registry::VipHealth { physical_health: 0.8, mental_health: 0.8 },
                traits: ua_traits,
                main_trait: ua_main_trait,
                ideology: String::new(),
                religion: country.macro_indicators.religion.clone(),
                nationality: country.name.clone(),
                dynasty: Some(dynasty_name.clone()),
                roles: vec![],
                base_influence: 8,
                faction: "Royal Court".to_string(),
                ..Default::default()
            });
            let relation = if ua_gender == "M" {
                super::succession::RoyalRelation::Uncle
            } else {
                super::succession::RoyalRelation::Aunt
            };
            royal_dynasty.members.push(super::succession::RoyalFamilyMember {
                vip_id: ua_vip_id.clone(),
                relation,
                birth_turn: 0,
                is_legitimate: true,
                is_heir_apparent: false,
                succession_order: next_succession_order,
                father_vip_id: None,
                mother_vip_id: None,
                spouse_vip_id: None,
                children_vip_ids: Vec::new(),
                marriage_turn: None,
                death_turn: None,
                death_cause: None,
            });
            next_succession_order += 1;
            uncle_aunt_ids.push(ua_vip_id);
        }

        // 3. Generate 0-2 cousins (children of uncles/aunts).
        let num_cousins = rng.gen_range(0..3); // 0-2 cousins
        for _ in 0..num_cousins {
            let cousin_gender = if rng.gen::<f64>() < 0.5 { "M" } else { "F" };
            let cousin_name = super::names::generate_key_vip_with_gender(cultural_group, cousin_gender, rng, &mut used_names);
            let (c_traits, c_main_trait) = assign_core_traits(rng);
            let cousin_age = if monarch_age > 10 { monarch_age - 10 + rng.gen_range(0..11) } else { rng.gen_range(1..10) };
            let cousin_vip_id = registry.register_new(Vip {
                full_name: cousin_name.full_name.clone(),
                gender: cousin_gender.to_string(),
                age: cousin_age.max(1),
                health: crate::politics::vip_registry::VipHealth { physical_health: 0.9, mental_health: 0.9 },
                traits: c_traits,
                main_trait: c_main_trait,
                ideology: String::new(),
                religion: country.macro_indicators.religion.clone(),
                nationality: country.name.clone(),
                dynasty: Some(dynasty_name.clone()),
                roles: vec![],
                base_influence: 5,
                faction: "Royal Court".to_string(),
                ..Default::default()
            });
            // Link cousin to a random uncle/aunt as parent.
            let parent_id = if uncle_aunt_ids.is_empty() {
                None
            } else {
                Some(uncle_aunt_ids[rng.gen_range(0..uncle_aunt_ids.len())].clone())
            };
            royal_dynasty.members.push(super::succession::RoyalFamilyMember {
                vip_id: cousin_vip_id.clone(),
                relation: super::succession::RoyalRelation::Cousin,
                birth_turn: 0,
                is_legitimate: true,
                is_heir_apparent: false,
                succession_order: next_succession_order,
                father_vip_id: if cousin_gender == "M" { parent_id.clone() } else { None },
                mother_vip_id: if cousin_gender == "F" { parent_id.clone() } else { None },
                spouse_vip_id: None,
                children_vip_ids: Vec::new(),
                marriage_turn: None,
                death_turn: None,
                death_cause: None,
            });
            // Link cousin to uncle/aunt's children list.
            if let Some(pid) = &parent_id {
                if let Some(parent_member) = royal_dynasty.members.iter_mut().find(|m| &m.vip_id == pid) {
                    parent_member.children_vip_ids.push(cousin_vip_id);
                }
            }
            next_succession_order += 1;
        }

        country.politics.royal_dynasty = Some(royal_dynasty);
    }

    // Register party leaders as VIPs.
    for (party_id, party) in &country.politics.active_parties {
        let leader = &party.leader;
        let is_ruling = country.politics.ruling_party == *party_id;
        let roles = if is_ruling && !is_monarchy(form) {
            vec![VipRoleExtended::PrimeMinister]
        } else {
            vec![]
        };
        registry.register_new(Vip {
            full_name: leader.name.clone(),
            gender: leader.gender.clone(),
            age: leader.age,
            health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
            traits: leader.traits.clone(),
            main_trait: leader.main_trait.clone(),
            ideology: party.ideology.clone(),
            religion: leader.religion.clone(),
            nationality: country.name.clone(),
            dynasty: leader.dynasty.clone(),
            roles,
            base_influence: leader.base_influence,
            faction: leader.faction.clone(),
            ..Default::default()
        });
    }

    // For non-democratic regimes, register advisory council members.
    if !form.is_democratic() {
        let mut rng2 = rand::thread_rng();
        for _ in 0..3 {
            // Phase 91: Use generate_key_vip for advisory council (key political appointees).
            let vip_name = names::generate_key_vip(cultural_group, &mut rng2, &mut used_names);
            // Phase 53: Use diverse traits from the core pool instead of
            // hardcoded "Loyalist"/"Loyalty".
            let (traits, main_trait) = assign_core_traits(&mut rng2);
            // Phase 53: Assign a real ideology (autocracies lean authoritarian).
            let ideology = {
                let autocratic_ideologies = [
                    "National Conservatism", "Neoconservatism", "Social Conservatism",
                ];
                autocratic_ideologies[rng2.gen_range(0..autocratic_ideologies.len())].to_string()
            };
            registry.register_new(Vip {
                full_name: vip_name.full_name,
                gender: vip_name.gender,
                age: 40 + rng2.gen_range(0..30),
                health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                traits,
                main_trait,
                ideology,
                nationality: country.name.clone(),
                roles: vec![VipRoleExtended::Minister],
                base_influence: 30 + rng2.gen_range(0..30),
                ..Default::default()
            });
        }
    }

    // Phase 53: Register all ministers from the formed government.
    // Previously, ministers created by `form_government` were stored only in
    // `ministry_config.ministries[].minister_name` and never appeared in the
    // VIP registry, making them invisible in the VIP Explorer.
    if let Some(ref mc) = country.politics.ministry_config {
        let mut rng3 = rand::thread_rng();
        for ministry in &mc.ministries {
            if ministry.minister_name.is_empty() {
                continue;
            }
            // Derive ideology from the minister's party if available.
            let ideology = country
                .politics
                .active_parties
                .get(&ministry.minister_party)
                .map(|p| p.ideology.clone())
                .unwrap_or_else(|| "Centrist".to_string());
            let (traits, main_trait) = assign_core_traits(&mut rng3);
            registry.register_new(Vip {
                full_name: ministry.minister_name.clone(),
                gender: {
                    let n = names::generate_full_vip(cultural_group, &mut rng3);
                    n.gender
                },
                age: 40 + rng3.gen_range(0..30),
                health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                traits,
                main_trait,
                ideology,
                nationality: country.name.clone(),
                roles: vec![VipRoleExtended::Minister],
                base_influence: 30 + rng3.gen_range(0..30),
                ..Default::default()
            });
        }
    }

    // Phase 54: Mayor/governor naming and registration has been moved to
    // `assign_regional_heads()`, which is called from the generator AFTER
    // `generate_regional_topology` produces the regions. Previously this code
    // ran inside `bootstrap_politics` before regions existed, so
    // `country.regions` was always empty and no mayors were ever named.

    if !form.is_democratic() {
        // For non-democratic forms, install the strongest party as the regime.
        if let Some((name, _)) = country
            .politics
            .active_parties
            .iter()
            .max_by(|a, b| a.1.support.partial_cmp(&b.1.support).unwrap_or(std::cmp::Ordering::Equal))
        {
            let ruling = name.clone();
            country.politics.ruling_party = ruling;
            country.politics.coalition = Vec::new();
            country.politics.coalition_id = "authoritarian".to_string();
            country.politics.minority_government = false;
            apply_ruling_ideology_policies(country);
        }
    }
}

/// Phase 54: Name and register mayors (regional heads) and megaregion
/// governors. This must be called AFTER `generate_regional_topology` has
/// produced the regions HashMap, because the governance structures are
/// initialized during region generation.
///
/// Previously this code ran inside `bootstrap_politics` before regions
/// existed, so `country.regions` was always empty and no mayors were
/// ever named — causing the UI to show blank head names.
///
/// # Arguments
/// * `country` - Mutable country with an initialized `vip_registry`.
/// * `regions` - The generated regional topology (mutated to name heads).
/// * `megaregions` - The generated megaregion list (mutated to name governors).
/// * `rng` - Random number generator.
pub fn assign_regional_heads(
    country: &mut Country,
    regions: &mut HashMap<String, crate::society::geography::Region>,
    megaregions: &mut Vec<crate::society::geography::Megaregion>,
    rng: &mut impl Rng,
) {
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic"
    } else {
        &country.macro_indicators.cultural_group
    };
    let form = country.politics.government_form;
    let country_name = country.name.clone();
    let religion = country.macro_indicators.religion.clone();

    let registry = match country.politics.vip_registry.as_mut() {
        Some(r) => r,
        None => return,
    };

    // Name and register mayors (regional heads).
    for region in regions.values_mut() {
        if let Some(ref mut gov) = region.governance {
            if gov.head.name.is_empty() {
                let vip_name = names::generate_full_vip(cultural_group, rng);
                gov.head.name = vip_name.full_name.clone();
                gov.head.gender = vip_name.gender.clone();
                gov.head.age = 35 + rng.gen_range(0..30);
                gov.head.nationality = country_name.clone();
                gov.head.religion = religion.clone();
            }
            let (traits, main_trait) = assign_core_traits(rng);
            let ideology = if form.is_democratic() {
                "Social Liberalism".to_string()
            } else {
                "National Conservatism".to_string()
            };
            if registry.get_by_name(&gov.head.name).is_none() {
                registry.register_new(Vip {
                    full_name: gov.head.name.clone(),
                    gender: gov.head.gender.clone(),
                    age: gov.head.age,
                    health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                    traits,
                    main_trait,
                    ideology,
                    nationality: country_name.clone(),
                    roles: vec![VipRoleExtended::Mayor],
                    base_influence: 20 + rng.gen_range(0..30),
                    ..Default::default()
                });
            }
        }
    }

    // Name and register megaregion governors.
    for megaregion in megaregions.iter_mut() {
        if let Some(ref mut mg_gov) = megaregion.governance {
            if mg_gov.governor.name.is_empty() {
                let vip_name = names::generate_full_vip(cultural_group, rng);
                mg_gov.governor.name = vip_name.full_name.clone();
                mg_gov.governor.gender = vip_name.gender.clone();
                mg_gov.governor.age = 40 + rng.gen_range(0..25);
                mg_gov.governor.nationality = country_name.clone();
                mg_gov.governor.religion = religion.clone();
            }
            let (traits, main_trait) = assign_core_traits(rng);
            let ideology = if form.is_democratic() {
                "Christian Democracy".to_string()
            } else {
                "National Conservatism".to_string()
            };
            if registry.get_by_name(&mg_gov.governor.name).is_none() {
                registry.register_new(Vip {
                    full_name: mg_gov.governor.name.clone(),
                    gender: mg_gov.governor.gender.clone(),
                    age: mg_gov.governor.age,
                    health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                    traits,
                    main_trait,
                    ideology,
                    nationality: country_name.clone(),
                    roles: vec![VipRoleExtended::RegionalGovernor],
                    base_influence: 25 + rng.gen_range(0..35),
                    ..Default::default()
                });
            }
        }
    }
}

fn is_monarchy(form: GovernmentForm) -> bool {
    matches!(
        form,
        GovernmentForm::ConstitutionalMonarchy
            | GovernmentForm::DualistMonarchy
            | GovernmentForm::ElectiveMonarchy
            | GovernmentForm::AbsoluteMonarchy
    )
}

fn build_constitution(form: GovernmentForm, _rng: &mut impl Rng) -> Constitution {
    let exists = !matches!(form, GovernmentForm::MilitaryDictatorship | GovernmentForm::AbsoluteMonarchy);
    let presidential_veto = matches!(
        form,
        GovernmentForm::PresidentialRepublic
            | GovernmentForm::SemiPresidentialRepublic
            | GovernmentForm::ConstitutionalMonarchy
            | GovernmentForm::DualistMonarchy
            | GovernmentForm::ElectiveMonarchy
    );
    let upper_house = if form.chambers() >= 2 {
        Some(UpperHouse {
            name: "Upper House".to_string(),
            elections: "Indirect Elections".to_string(),
            powers: "Bill Veto".to_string(),
        })
    } else {
        None
    };
    let change_mechanism = if form.is_democratic() {
        "Qualified Majority"
    } else {
        "Ruler's Decree"
    };
    Constitution {
        exists,
        constitutional_tribunal: if exists { "Constitutional Tribunal" } else { "None" }.to_string(),
        presidential_veto,
        upper_house,
        change_mechanism: change_mechanism.to_string(),
        judiciary: build_judiciary(form),
        suffrage_system: crate::politics::interest_groups::SuffrageSystem {
            nominal_weight: 0.7,
            financial_weight: 0.3,
            suffrage_type: crate::politics::interest_groups::SuffrageType::UniversalSuffrage,
        },
        budget_failure_consequence: crate::politics::budget_lifecycle::BudgetFailureConsequence::default(),
    }
}

fn build_judiciary(form: GovernmentForm) -> Judiciary {
    let (minister, judge_selection) = if form.is_democratic() {
        ("Minister of Justice", "Competitive Nomination")
    } else {
        ("Prosecutor General", "Political Nomination")
    };
    let pardon = match form {
        GovernmentForm::PresidentialRepublic
        | GovernmentForm::SemiPresidentialRepublic
        | GovernmentForm::DirectorialDemocracy => "President",
        GovernmentForm::ConstitutionalMonarchy
        | GovernmentForm::DualistMonarchy
        | GovernmentForm::ElectiveMonarchy
        | GovernmentForm::AbsoluteMonarchy => "Monarch",
        _ => "State Council",
    };
    let military = matches!(form, GovernmentForm::MilitaryDictatorship);
    Judiciary {
        minister_and_prosecutor: minister.to_string(),
        judge_selection: judge_selection.to_string(),
        jury_trials: form.is_democratic(),
        military_courts: military,
        pardon: pardon.to_string(),
        admin_courts: true,
        extra: serde_json::Map::new(),
    }
}

fn random_dynasty(rng: &mut impl Rng) -> String {
    const DYNASTIES: &[&str] = &[
        "Habsburg", "Romanow", "Piast", "Jagiellon", "Waz", "Bourbon", "Hohenzollern",
        "Sask", "Braganza", "Savoja", "Hanower", "Oldenburg", "Bernadotte", "Glücksburg",
        "Wittelsbach", "Gryfit", "Wettin", "Otton", "Norman", "Kapetyng", "Karoling",
        "Sachsen-Coburg-Gotha", "Holsztyn-Gottorp", "Bourbon-Parma", "Wittelsbach",
    ];
    DYNASTIES[rng.gen_range(0..DYNASTIES.len())].to_string()
}

fn random_head_of_state(country: &Country, form: GovernmentForm, rng: &mut impl Rng, used_names: &mut std::collections::HashSet<String>) -> Leader {
    // Phase 49/91: Use the country's cultural group for culturally-appropriate names.
    // Phase 91: Use generate_key_vip for uniqueness among key political figures.
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic"
    } else {
        &country.macro_indicators.cultural_group
    };
    let vip_name = super::names::generate_key_vip(cultural_group, rng, used_names);
    let male = vip_name.gender == "M";
    let name = vip_name.full_name;

    let (title, faction) = if is_monarchy(form) {
        if male {
            ("King", "Royal Court")
        } else {
            ("Queen", "Royal Court")
        }
    } else if form.is_democratic() {
        if male {
            ("President", "Presidential Chancellery")
        } else {
            ("President", "Presidential Chancellery")
        }
    } else {
        ("Leader", "Council of Authority")
    };

    let dynasty = if is_monarchy(form) {
        country.politics.dynasty.clone()
    } else {
        None
    };

    // Phase 53: Use the weighted core-trait pool instead of hardcoded
    // "Charismatic"/"Diplomatic"/"Lawfulness". Monarchies bias toward
    // "Traditionalist" by prepending it but still draw the rest randomly.
    let (mut traits, main_trait) = super::vip_registry::assign_core_traits(rng);
    if is_monarchy(form) {
        // Ensure monarchs lean traditional while keeping diversity.
        if !traits.contains(&"Traditionalist".to_string()) {
            traits.insert(0, "Traditionalist".to_string());
        }
    }

    // Phase 53: Assign a real ideology string instead of "Republican"/"Conservative".
    let views = if is_monarchy(form) {
        // Monarchies lean socially conservative.
        "Social Conservatism".to_string()
    } else if form.is_democratic() {
        // Democracies get a weighted ideology pick.
        let democratic_ideologies = [
            "Social Liberalism", "Christian Democracy", "Social Democracy",
            "Classical Liberalism", "Social Conservatism", "Agrarianism",
        ];
        democratic_ideologies[rng.gen_range(0..democratic_ideologies.len())].to_string()
    } else {
        // Autocracies lean nationalist/authoritarian.
        let autocratic_ideologies = [
            "National Conservatism", "Neoconservatism", "Social Conservatism",
        ];
        autocratic_ideologies[rng.gen_range(0..autocratic_ideologies.len())].to_string()
    };

    Leader {
        name: format!("{title} {name}"),
        gender: if male { "M" } else { "F" }.to_string(),
        age: rng.gen_range(35..75),
        health: "Good".to_string(),
        days_sick: 0,
        religion: country.macro_indicators.religion.clone(),
        nationality: country.name.clone(),
        views,
        traits,
        main_trait,
        dynasty,
        base_influence: rng.gen_range(30..80),
        faction: faction.to_string(),
    }
}

// ============================================================================
// PHASE 86: ADVISORY COUNCIL TURN PROCESSING
// ============================================================================

/// Phase 86: Process advisory council for authoritarian/royal regimes.
///
/// This function is called after legislation processing in the political turn.
/// It applies per-turn loyalty drift based on macro variables, calculates
/// influence modifiers on existing Country fields, and checks for coup triggers.
///
/// # Arguments
/// * `country` - Mutable country
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages.
pub fn process_advisory_council_turn(
    country: &mut Country,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Only process if an advisory council exists.
    let council = match country.politics.advisory_council.as_mut() {
        Some(c) => c,
        None => return messages,
    };

    // Gather macro variables for loyalty drift.
    let gdp = country.macro_indicators.gdp_breakdown.official_gdp.max(1.0);
    let prev_gdp = country.macro_indicators.gdp_breakdown.previous_gdp.max(1.0);
    let gdp_growth_rate = if prev_gdp > 0.0 {
        ((gdp - prev_gdp) / prev_gdp) * 100.0
    } else {
        0.0
    };
    let inflation_rate = country.macro_indicators.inflation;
    let social_unrest = country.macro_indicators.social_unrest;
    // Military spending ratio: derived from GDP (heuristic — no stored field).
    // Uses the same formula as military/oob.rs: share scales inversely with GDP per capita.
    let gdp_per_capita = if country.budget.population > 0 {
        gdp / country.budget.population as f64
    } else {
        1000.0
    };
    let military_spending_ratio = (0.06 - gdp_per_capita * 0.000009).max(0.015).min(0.06);

    // Apply loyalty drift.
    council.apply_loyalty_drift(
        gdp_growth_rate,
        inflation_rate,
        social_unrest,
        military_spending_ratio,
    );

    // Calculate and apply influence modifiers to existing Country fields.
    let modifiers = council.calculate_influence_modifiers();

    // Apply social_unrest_delta to macro_indicators (clamped to 0–100).
    if modifiers.social_unrest_delta.abs() > 1e-6 {
        country.macro_indicators.social_unrest =
            (country.macro_indicators.social_unrest + modifiers.social_unrest_delta).clamp(0.0, 100.0);
        messages.push(format!(
            "[COUNCIL] Social unrest adjusted by {:.2} → new total: {:.1}",
            modifiers.social_unrest_delta, country.macro_indicators.social_unrest
        ));
    }

    // Apply autonomy stabilization to regions (if any).
    if modifiers.autonomy_stabilization.abs() > 1e-6 {
        for region in &mut country.regions {
            for domain in region.micro_regions.values_mut() {
                domain.autonomy_level =
                    (domain.autonomy_level - modifiers.autonomy_stabilization).clamp(0.0, 1.0);
            }
        }
    }

    // Log council status.
    messages.push(format!(
        "[COUNCIL] Aggregate loyalty: {:.3} (coup risk: {})",
        council.aggregate_loyalty,
        if council.coup_risk_active(current_turn) { "ACTIVE" } else { "inactive" }
    ));

    // Check for coup trigger.
    if council.coup_risk_active(current_turn) {
        // Deterministic coup attempt roll.
        let coup_seed = format!("coup_{}_{}", country.name, current_turn);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::Hasher;
        for b in coup_seed.bytes() {
            hasher.write_u8(b);
        }
        let hash = hasher.finish();
        // Coup success probability increases as loyalty decreases.
        let coup_success_prob = ((council.coup_risk_threshold - council.aggregate_loyalty) / council.coup_risk_threshold).clamp(0.0, 0.8);
        let roll = (hash % 1000) as f64 / 1000.0;

        if roll < coup_success_prob {
            // Coup succeeds — set cooldown and emit crisis message.
            council.coup_cooldown_until_turn = current_turn + 24;
            messages.push(format!(
                "[COUP] Advisory council coup attempt SUCCEEDED (loyalty={:.3}, roll={:.3}, threshold={:.3}). Cooldown set for 24 turns.",
                council.aggregate_loyalty, roll, coup_success_prob
            ));
            // Increase social unrest dramatically due to coup.
            country.macro_indicators.social_unrest =
                (country.macro_indicators.social_unrest + 20.0).clamp(0.0, 100.0);
        } else {
            messages.push(format!(
                "[COUP] Advisory council coup attempt FAILED (loyalty={:.3}, roll={:.3}, threshold={:.3}).",
                council.aggregate_loyalty, roll, coup_success_prob
            ));
        }
    }

    messages
}

// ============================================================================
// PHASE 86: DYNASTY TURN PROCESSING
// ============================================================================

/// Phase 86: Process royal dynasty per turn — marriages, births, succession updates.
///
/// Called after advisory council processing in the political turn.
/// Only processes countries with an active royal dynasty (monarchies).
///
/// # Arguments
/// * `country` - Mutable country
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages.
pub fn process_dynasty_turn(
    country: &mut Country,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Only process if a royal dynasty exists.
    if country.politics.royal_dynasty.is_none() {
        return messages;
    }

    // Get culture for name generation.
    let culture = country.macro_indicators.culture.clone();
    let dynasty_id = country
        .politics
        .royal_dynasty
        .as_ref()
        .map(|d| d.dynasty_name.clone())
        .unwrap_or_default();

    // Process marriages and births via the succession module.
    let dynasty_msgs = super::succession::process_dynasty_turn(
        &mut country.politics.royal_dynasty,
        &mut country.politics.vip_registry,
        &culture,
        &dynasty_id,
        current_turn,
    );
    messages.extend(dynasty_msgs);

    messages
}
