//! Modular ministries and autonomous AI minister spending logic.
//!
//! This module implements Pillar II of the Phase 8 blueprint: modular ministries
//! with autonomous AI ministers using deterministic ideology-based spending
//! decision trees. Ministries submit B2B procurement orders as physical buy
//! bids to the order book, with cash encumbered at submission and settled
//! during market clearing.

use crate::economy::order_book::OrderBook;
use crate::entities::Company;
use crate::politics::ideology::Ideology;
use crate::politics::system::{GovernmentForm, Party};
use crate::registries::enums::Commodity;
use crate::state::Country;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// GOVERNMENT COMPETENCY
// ============================================================================

/// A bundle of government responsibilities assigned to a single ministry.
///
/// Each competency has a `BudgetPriorities` profile and a set of valid spending
/// actions. A single ministry may bundle multiple competencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GovernmentCompetency {
    /// Heavy industry (steel, machinery, mining).
    HeavyIndustry,
    /// Light industry (textiles, consumer goods).
    LightIndustry,
    /// Agriculture and rural economy.
    Agriculture,
    /// Infrastructure (roads, bridges, utilities).
    Infrastructure,
    /// Internal security (police, intelligence).
    InternalSecurity,
    /// Foreign affairs and diplomacy.
    ForeignAffairs,
    /// National defense and armed forces.
    Defense,
    /// Education and propaganda.
    Education,
    /// Healthcare and sanitation.
    Healthcare,
    /// Social welfare and pensions.
    SocialWelfare,
    /// Justice and courts.
    Justice,
    /// Treasury — always exists, manages taxation & fiscal policy.
    /// Does NOT handle debt service (that is a central obligation).
    Treasury,
    /// Science and R&D.
    Science,
    /// Energy production and distribution.
    Energy,
    /// Transport and logistics.
    Transport,
    /// Housing and urban development.
    Housing,
    /// Culture and arts.
    Culture,
    /// Labor regulation and employment.
    Labor,
    /// Environmental protection.
    Environment,
    /// Phase 39: State assets management (SOEs, patents, state property).
    StateAssets,
}

// ============================================================================
// BUDGET PRIORITIES
// ============================================================================

/// Ideology-derived spending weights (0.0–1.0) for each policy area.
///
/// Used by the amendment negotiation logic and the autonomous minister AI
/// to determine spending priorities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BudgetPriorities {
    /// Heavy industry nationalization/subsidy preference.
    pub heavy_industry: f64,
    /// Internal security and policing preference.
    pub internal_security: f64,
    /// Education and propaganda preference.
    pub education: f64,
    /// Healthcare and public health preference.
    pub healthcare: f64,
    /// Infrastructure investment preference.
    pub infrastructure: f64,
    /// Social welfare and pensions preference.
    pub social_welfare: f64,
    /// Agriculture and rural economy preference.
    pub agriculture: f64,
    /// Armed forces and defense preference.
    pub armed_forces: f64,
    /// Free-market preference (low taxation / low spending).
    pub free_market: f64,
}

impl BudgetPriorities {
    /// Returns the priority weight for a given `GovernmentCompetency`.
    pub fn weight_for(&self, competency: GovernmentCompetency) -> f64 {
        match competency {
            GovernmentCompetency::HeavyIndustry => self.heavy_industry,
            GovernmentCompetency::LightIndustry => self.heavy_industry * 0.7,
            GovernmentCompetency::Agriculture => self.agriculture,
            GovernmentCompetency::Infrastructure => self.infrastructure,
            GovernmentCompetency::InternalSecurity => self.internal_security,
            GovernmentCompetency::ForeignAffairs => self.internal_security * 0.5,
            GovernmentCompetency::Defense => self.armed_forces,
            GovernmentCompetency::Education => self.education,
            GovernmentCompetency::Healthcare => self.healthcare,
            GovernmentCompetency::SocialWelfare => self.social_welfare,
            GovernmentCompetency::Justice => self.internal_security * 0.6,
            GovernmentCompetency::Treasury => 0.5, // always moderate
            GovernmentCompetency::Science => self.education * 0.6,
            GovernmentCompetency::Energy => self.infrastructure * 0.5,
            GovernmentCompetency::Transport => self.infrastructure * 0.7,
            GovernmentCompetency::Housing => self.social_welfare * 0.5,
            GovernmentCompetency::Culture => self.education * 0.3,
            GovernmentCompetency::Labor => self.social_welfare * 0.4,
            GovernmentCompetency::Environment => self.education * 0.2,
            GovernmentCompetency::StateAssets => self.heavy_industry * 0.4,
        }
    }
}

/// Extension trait to add `budget_priorities()` to `Ideology`.
pub trait IdeologyBudgetPriorities {
    /// Returns the `BudgetPriorities` for this ideology.
    fn budget_priorities(self) -> BudgetPriorities;
}

impl IdeologyBudgetPriorities for Ideology {
    fn budget_priorities(self) -> BudgetPriorities {
        match self {
            Ideology::OrthodoxMarxism => BudgetPriorities {
                heavy_industry: 0.9, internal_security: 0.7, education: 0.8,
                healthcare: 0.9, infrastructure: 0.7, social_welfare: 0.9,
                agriculture: 0.6, armed_forces: 0.5, free_market: 0.0,
            },
            Ideology::MarxismLeninism => BudgetPriorities {
                heavy_industry: 1.0, internal_security: 0.9, education: 0.8,
                healthcare: 0.9, infrastructure: 0.8, social_welfare: 0.8,
                agriculture: 0.7, armed_forces: 0.8, free_market: 0.0,
            },
            Ideology::Maoism => BudgetPriorities {
                heavy_industry: 0.7, internal_security: 0.9, education: 0.7,
                healthcare: 0.8, infrastructure: 0.5, social_welfare: 0.7,
                agriculture: 1.0, armed_forces: 0.7, free_market: 0.0,
            },
            Ideology::SocialDemocracy => BudgetPriorities {
                heavy_industry: 0.3, internal_security: 0.3, education: 0.8,
                healthcare: 0.9, infrastructure: 0.6, social_welfare: 0.9,
                agriculture: 0.4, armed_forces: 0.3, free_market: 0.3,
            },
            Ideology::GreenPolitics => BudgetPriorities {
                heavy_industry: 0.1, internal_security: 0.2, education: 0.7,
                healthcare: 0.7, infrastructure: 0.4, social_welfare: 0.8,
                agriculture: 0.5, armed_forces: 0.1, free_market: 0.2,
            },
            Ideology::ClassicalLiberalism => BudgetPriorities {
                heavy_industry: 0.2, internal_security: 0.3, education: 0.3,
                healthcare: 0.2, infrastructure: 0.3, social_welfare: 0.1,
                agriculture: 0.2, armed_forces: 0.3, free_market: 0.9,
            },
            Ideology::SocialLiberalism => BudgetPriorities {
                heavy_industry: 0.3, internal_security: 0.3, education: 0.7,
                healthcare: 0.6, infrastructure: 0.5, social_welfare: 0.6,
                agriculture: 0.3, armed_forces: 0.3, free_market: 0.5,
            },
            Ideology::Agrarianism => BudgetPriorities {
                heavy_industry: 0.2, internal_security: 0.3, education: 0.4,
                healthcare: 0.4, infrastructure: 0.4, social_welfare: 0.5,
                agriculture: 1.0, armed_forces: 0.3, free_market: 0.4,
            },
            Ideology::ChristianDemocracy => BudgetPriorities {
                heavy_industry: 0.3, internal_security: 0.4, education: 0.6,
                healthcare: 0.6, infrastructure: 0.5, social_welfare: 0.7,
                agriculture: 0.5, armed_forces: 0.4, free_market: 0.4,
            },
            Ideology::SocialConservatism => BudgetPriorities {
                heavy_industry: 0.3, internal_security: 0.7, education: 0.5,
                healthcare: 0.4, infrastructure: 0.4, social_welfare: 0.4,
                agriculture: 0.4, armed_forces: 0.7, free_market: 0.4,
            },
            Ideology::Neoconservatism => BudgetPriorities {
                heavy_industry: 0.4, internal_security: 0.6, education: 0.4,
                healthcare: 0.3, infrastructure: 0.4, social_welfare: 0.3,
                agriculture: 0.3, armed_forces: 0.9, free_market: 0.6,
            },
            Ideology::Neoliberalism => BudgetPriorities {
                heavy_industry: 0.2, internal_security: 0.3, education: 0.3,
                healthcare: 0.2, infrastructure: 0.3, social_welfare: 0.1,
                agriculture: 0.2, armed_forces: 0.4, free_market: 1.0,
            },
            Ideology::NationalConservatism => BudgetPriorities {
                heavy_industry: 0.4, internal_security: 0.8, education: 0.5,
                healthcare: 0.4, infrastructure: 0.4, social_welfare: 0.4,
                agriculture: 0.5, armed_forces: 0.8, free_market: 0.3,
            },
            Ideology::AnarchoCapitalism => BudgetPriorities {
                heavy_industry: 0.1, internal_security: 0.1, education: 0.1,
                healthcare: 0.1, infrastructure: 0.1, social_welfare: 0.0,
                agriculture: 0.1, armed_forces: 0.1, free_market: 1.0,
            },
            Ideology::Fascism => BudgetPriorities {
                heavy_industry: 0.8, internal_security: 1.0, education: 0.6,
                healthcare: 0.5, infrastructure: 0.7, social_welfare: 0.5,
                agriculture: 0.5, armed_forces: 1.0, free_market: 0.0,
            },
        }
    }
}

// ============================================================================
// MINISTRY STRUCTS
// ============================================================================

/// A log entry recording a single spending action taken by a ministry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MinistrySpendingAction {
    /// Physical B2B buy order submitted to the order book.
    /// Cash is encumbered at submission, settled during `match_orders`.
    B2BProcurementOrder {
        /// Commodity being procured.
        commodity: Commodity,
        /// Quantity requested.
        quantity: f64,
        /// Maximum price willing to pay.
        limit_price: f64,
    },
    /// Cash subsidy to a specific company (direct transfer, not B2B).
    Subsidy {
        /// Company receiving the subsidy.
        target_company_id: String,
        /// Amount transferred.
        amount: f64,
    },
    /// Infrastructure funding for a specific building.
    InfrastructureFunding {
        /// Building receiving the funding.
        target_building_id: String,
        /// Amount transferred to reserve fund.
        amount: f64,
    },
    /// Wages for public-service buildings (schools, hospitals).
    PublicServiceWages {
        /// Buildings whose workers are paid.
        building_ids: Vec<String>,
        /// Total wage amount.
        total_amount: f64,
    },
    /// Transfer to local government.
    TransferToLocalGov {
        /// Region receiving the transfer.
        region_id: String,
        /// Amount transferred.
        amount: f64,
    },
    /// R&D grant to a company or institution.
    RAndDGrant {
        /// Entity receiving the grant.
        target_entity: String,
        /// Grant amount.
        amount: f64,
    },
    /// Retail savings bond issuance recorded during B2C window.
    RetailSavingsBondIssuance {
        /// Total cash absorbed from citizen savings.
        total_absorbed: f64,
    },
    /// Direct cash transfer for spending not covered by other variants.
    DirectTransfer {
        /// Target description.
        target: String,
        /// Amount transferred.
        amount: f64,
    },
}

/// A single ministry headed by a minister from a specific party.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Ministry {
    /// Unique ministry ID (e.g. "MIN-001").
    pub id: String,
    /// Display name (e.g. "Ministry of Heavy Industry").
    pub name: String,
    /// Competencies bundled into this ministry.
    pub competencies: Vec<GovernmentCompetency>,
    /// Party ID of the minister heading this ministry.
    pub minister_party: String,
    /// Name of the minister (leader name).
    pub minister_name: String,
    /// Cash received from treasury allocation.
    pub allocated_cash: f64,
    /// Cumulative cash spent this turn.
    pub spent_cash: f64,
    /// Log of all spending actions taken this turn.
    pub spending_actions: Vec<MinistrySpendingAction>,
    /// Phase 35: Cash currently held by the ministry (debited from
    /// liquid_reserves at allocation). All spending debits from this field,
    /// NOT from liquid_reserves, eliminating the double-debit bug.
    #[serde(default)]
    pub ministry_cash: f64,
}

/// A ministry allocation entry in a budget bill (promised, not yet funded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MinistryAllocation {
    /// Ministry ID.
    pub ministry_id: String,
    /// Ministry display name.
    pub ministry_name: String,
    /// Competencies assigned to this ministry.
    pub competencies: Vec<GovernmentCompetency>,
    /// Promised cash allocation.
    pub allocated_cash: f64,
    /// Party ID of the minister.
    pub minister_party: String,
}

/// Configuration for the entire ministry system, stored on `Politics`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MinistryConfig {
    /// All active ministries.
    pub ministries: Vec<Ministry>,
    /// Turn when the current ministry list was formed.
    pub formation_turn: u32,
    /// Prime Minister's party ID.
    pub pm_party: String,
}

// ============================================================================
// MINISTRY FORMATION
// ============================================================================

/// Forms a government by distributing ministries among coalition partners.
///
/// # Arguments
/// * `country` - The country to form a government for.
/// * `coalition` - List of party IDs in the ruling coalition.
/// * `active_parties` - Map of all active parties by ID.
/// * `current_turn` - The current turn number.
///
/// # Returns
/// A `MinistryConfig` with ministries distributed proportional to seat count.
///
/// # Rules
/// * The PM always keeps `Treasury` and `Defense` (or `InternalSecurity` in
///   authoritarian regimes).
/// * Number of ministries scales with GDP and population:
///   `min(15, max(3, (gdp / 1e9) as usize + 3))`.
/// * Coalition partners receive portfolios proportional to their seat count.
pub fn form_government(
    country: &Country,
    coalition: &[String],
    active_parties: &HashMap<String, Party>,
    current_turn: u32,
    used_names: &mut HashSet<String>,
) -> MinistryConfig {
    let gdp = country.budget.gdp;
    let num_ministries = std::cmp::min(15, std::cmp::max(3, (gdp / 1e9) as usize + 3));
    let cg = if country.macro_indicators.cultural_group.is_empty() {
        "slavic"
    } else {
        &country.macro_indicators.cultural_group
    };

    // Define competency bundles
    let all_competencies = default_competency_bundles(num_ministries);

    // Calculate total coalition seats
    let parliament = &country.politics.parliament;
    let coalition_seats: u32 = coalition
        .iter()
        .filter_map(|pid| parliament.get(pid))
        .sum();

    // PM always keeps Treasury and Defense/InternalSecurity
    let is_autocratic = !country.politics.government_form.is_democratic();
    let pm_reserved = if is_autocratic {
        vec![GovernmentCompetency::Treasury, GovernmentCompetency::InternalSecurity]
    } else {
        vec![GovernmentCompetency::Treasury, GovernmentCompetency::Defense]
    };

    // Distribute remaining competencies among coalition partners
    let mut ministries: Vec<Ministry> = Vec::new();
    let mut competency_idx = 0;

    // Phase 49: Track which parties have already had their leader used as a minister.
    // Initialize BEFORE PM reserved ministries so the PM's party is tracked.
    let mut leader_used: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(pm_party) = coalition.first() {
        leader_used.insert(pm_party.clone());
    }

    // PM ministries (reserved competencies)
    for comp in &pm_reserved {
        let pm_party = coalition.first().cloned().unwrap_or_default();
        let minister_name = resolve_minister_name(active_parties, &pm_party, cg);
        ministries.push(Ministry {
            id: format!("MIN-{:03}", ministries.len() + 1),
            name: competency_display_name(*comp),
            competencies: vec![*comp],
            minister_party: pm_party.clone(),
            minister_name,
            allocated_cash: 0.0,
            spent_cash: 0.0,
            spending_actions: Vec::new(),
            ministry_cash: 0.0,
        });
        competency_idx += 1;
    }

    // Distribute remaining competencies proportionally
    if coalition.len() > 1 && coalition_seats > 0 {
        let remaining_competencies: Vec<GovernmentCompetency> = all_competencies
            .iter()
            .filter(|c| !pm_reserved.contains(c))
            .cloned()
            .collect();

        let cultural_group = cg;
        let mut rng = rand::thread_rng();
        for comp in &remaining_competencies {
            if competency_idx >= num_ministries {
                break;
            }
            // Pick coalition partner by proportional rotation
            let party_idx = competency_idx % coalition.len();
            let party_id = &coalition[party_idx];
            // Phase 41: Use party leader name only once per party; generate
            // unique VIP names for subsequent ministries from the same party.
            let minister_name = if leader_used.contains(party_id) {
                crate::politics::names::generate_unique_vip(cultural_group, &mut rng, &mut *used_names).full_name
            } else {
                leader_used.insert(party_id.clone());
                let name = resolve_minister_name(active_parties, party_id, cg);
                used_names.insert(name.clone());
                name
            };
            ministries.push(Ministry {
                id: format!("MIN-{:03}", ministries.len() + 1),
                name: competency_display_name(*comp),
                competencies: vec![*comp],
                minister_party: party_id.clone(),
                minister_name,
                allocated_cash: 0.0,
                spent_cash: 0.0,
                spending_actions: Vec::new(),
                ministry_cash: 0.0,
            });
            competency_idx += 1;
        }
    } else {
        // Single-party government: PM gets all portfolios, but each ministry
        // gets a UNIQUE minister name (Phase 37 fix — was cloning PM name for all).
        let pm_party = coalition.first().cloned().unwrap_or_default();
        let pm_name = resolve_minister_name(active_parties, &pm_party, cg);
        let cultural_group = cg;
        let mut rng = rand::thread_rng();
        // Phase 45: Use the global used_names set — no local HashSet.
        used_names.insert(pm_name.clone());
        for comp in all_competencies.iter() {
            if pm_reserved.contains(comp) {
                continue;
            }
            if competency_idx >= num_ministries {
                break;
            }
            // Generate a unique minister name for each ministry.
            // The PM keeps the party leader name; other ministries get generated VIPs.
            let minister_name = if ministries.is_empty() {
                pm_name.clone()
            } else {
                crate::politics::names::generate_unique_vip(cultural_group, &mut rng, &mut *used_names).full_name
            };
            ministries.push(Ministry {
                id: format!("MIN-{:03}", ministries.len() + 1),
                name: competency_display_name(*comp),
                competencies: vec![*comp],
                minister_party: pm_party.clone(),
                minister_name,
                allocated_cash: 0.0,
                spent_cash: 0.0,
                spending_actions: Vec::new(),
                ministry_cash: 0.0,
            });
            competency_idx += 1;
        }
    }

    MinistryConfig {
        ministries,
        formation_turn: current_turn,
        pm_party: coalition.first().cloned().unwrap_or_default(),
    }
}

/// Returns a default set of competency bundles for a given ministry count.
fn default_competency_bundles(count: usize) -> Vec<GovernmentCompetency> {
    let base = vec![
        GovernmentCompetency::Treasury,
        GovernmentCompetency::Defense,
        GovernmentCompetency::InternalSecurity,
        GovernmentCompetency::Education,
        GovernmentCompetency::Healthcare,
        GovernmentCompetency::Infrastructure,
        GovernmentCompetency::SocialWelfare,
        GovernmentCompetency::Agriculture,
        GovernmentCompetency::HeavyIndustry,
        GovernmentCompetency::Justice,
        GovernmentCompetency::ForeignAffairs,
        GovernmentCompetency::Science,
        GovernmentCompetency::Transport,
        GovernmentCompetency::Energy,
        GovernmentCompetency::Labor,
        GovernmentCompetency::StateAssets,
    ];
    base.into_iter().take(count).collect()
}

/// Phase 33/39: Resolve a minister name from the party leader, with a fallback
/// that generates a random VIP name if the leader name is empty or the party_id
/// is empty. This prevents the "Minister ()" bug when coalition formation
/// produces an empty party_id.
/// Phase 49: Now accepts `cultural_group` to generate culturally-appropriate fallback names.
fn resolve_minister_name(active_parties: &HashMap<String, Party>, party_id: &str, cultural_group: &str) -> String {
    let cg = if cultural_group.is_empty() { "slavic" } else { cultural_group };
    // Phase 39: If party_id is empty, generate a technocrat name.
    if party_id.is_empty() {
        let mut rng = rand::thread_rng();
        return crate::politics::names::generate_full_vip(cg, &mut rng).full_name;
    }
    let name = active_parties
        .get(party_id)
        .map(|p| p.leader.name.clone())
        .unwrap_or_default();
    if name.is_empty() {
        // Phase 39: Generate a random VIP name instead of "Minister (party_id)".
        let mut rng = rand::thread_rng();
        crate::politics::names::generate_full_vip(cg, &mut rng).full_name
    } else {
        name
    }
}

/// Returns a human-readable display name for a competency.
fn competency_display_name(comp: GovernmentCompetency) -> String {
    match comp {
        GovernmentCompetency::HeavyIndustry => "Ministry of Heavy Industry".to_string(),
        GovernmentCompetency::LightIndustry => "Ministry of Light Industry".to_string(),
        GovernmentCompetency::Agriculture => "Ministry of Agriculture".to_string(),
        GovernmentCompetency::Infrastructure => "Ministry of Infrastructure".to_string(),
        GovernmentCompetency::InternalSecurity => "Ministry of Internal Affairs".to_string(),
        GovernmentCompetency::ForeignAffairs => "Ministry of Foreign Affairs".to_string(),
        GovernmentCompetency::Defense => "Ministry of National Defense".to_string(),
        GovernmentCompetency::Education => "Ministry of Education".to_string(),
        GovernmentCompetency::Healthcare => "Ministry of Health".to_string(),
        GovernmentCompetency::SocialWelfare => "Ministry of Social Welfare".to_string(),
        GovernmentCompetency::Justice => "Ministry of Justice".to_string(),
        GovernmentCompetency::Treasury => "Ministry of Treasury".to_string(),
        GovernmentCompetency::Science => "Ministry of Science".to_string(),
        GovernmentCompetency::Energy => "Ministry of Energy".to_string(),
        GovernmentCompetency::Transport => "Ministry of Transport".to_string(),
        GovernmentCompetency::Housing => "Ministry of Housing".to_string(),
        GovernmentCompetency::Culture => "Ministry of Culture".to_string(),
        GovernmentCompetency::Labor => "Ministry of Labor".to_string(),
        GovernmentCompetency::Environment => "Ministry of Environment".to_string(),
        GovernmentCompetency::StateAssets => "Ministry of State Assets".to_string(),
    }
}

// ============================================================================
// CASH ALLOCATION
// ============================================================================

/// Phase 40: Calculate budget needs for all ministries based on GDP and
/// the ruling party's ideology weights. Sets `allocated_cash` on each
/// ministry so that `allocate_cash_to_ministries` and `draft_budget_bill`
/// see non-zero targets.
///
/// # Arguments
/// * `country` - Mutable country state.
///
/// # Rules
/// * Base budget = 15% of GDP (government spending target).
/// * Each ministry's share = sum(weight_for(comp) for comp in competencies)
///   / sum(all weights across all ministries).
/// * Minimum floor of 10,000 per ministry to ensure basic functionality
///   even in very poor economies.
/// * Does NOT debit treasury — that happens in `allocate_cash_to_ministries`.
pub fn calculate_budget_needs(country: &mut Country) {
    let Some(ref mut config) = country.politics.ministry_config else {
        return;
    };

    let gdp = country.budget.gdp.max(1.0);
    let base_budget = gdp * 0.15;

    // Get ruling party ideology for weight distribution
    let ideology = country
        .politics
        .active_parties
        .get(&country.politics.ruling_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or(Ideology::SocialLiberalism);
    let priorities = ideology.budget_priorities();

    // Compute total weight across all ministries for normalization
    let total_weight: f64 = config
        .ministries
        .iter()
        .map(|m| {
            m.competencies
                .iter()
                .map(|c| priorities.weight_for(*c))
                .sum::<f64>()
        })
        .sum();

    if total_weight <= 0.0 {
        // Fallback: equal distribution
        let per_ministry = base_budget / config.ministries.len().max(1) as f64;
        for ministry in &mut config.ministries {
            ministry.allocated_cash = per_ministry.max(10_000.0);
        }
        return;
    }

    for ministry in &mut config.ministries {
        let ministry_weight: f64 = ministry
            .competencies
            .iter()
            .map(|c| priorities.weight_for(*c))
            .sum();
        let share = ministry_weight / total_weight;
        ministry.allocated_cash = (base_budget * share).max(10_000.0);
    }
}

/// Allocates cash from `treasury.liquid_reserves` to ministries.
///
/// # Arguments
/// * `country` - Mutable country state.
///
/// # Rules
/// * Ministries are hard-capped by actual `treasury.liquid_reserves`.
/// * If the treasury cannot fully fund the promised amounts, allocations are
///   proportionally reduced to match physical cash on hand.
/// * `sum(allocated) <= liquid_reserves` — no negative cash ever.
/// * `treasury.liquid_reserves` is decremented by the total allocated.
pub fn allocate_cash_to_ministries(country: &mut Country) {
    let Some(ref mut config) = country.politics.ministry_config else {
        return;
    };

    let promised: f64 = config.ministries.iter().map(|m| m.allocated_cash).sum();
    if promised <= 0.0 {
        return;
    }

    let available = country.budget.liquid_reserves;
    let ratio = (available / promised).min(1.0);

    for ministry in &mut config.ministries {
        let allocated = ministry.allocated_cash * ratio;
        ministry.allocated_cash = allocated;
        // Phase 35: Credit the ministry's cash pocket. All spending debits
        // from this field, NOT from liquid_reserves, eliminating the
        // double-debit bug where liquid_reserves was hit at allocation AND
        // again at spending time.
        ministry.ministry_cash = allocated;
        ministry.spent_cash = 0.0;
        ministry.spending_actions.clear();
    }

    let total_allocated: f64 = config.ministries.iter().map(|m| m.allocated_cash).sum();
    country.budget.liquid_reserves -= total_allocated;
}

/// Sums the promised allocations from a ministry config.
pub fn sum_ministry_allocations(config: &Option<MinistryConfig>) -> f64 {
    config
        .as_ref()
        .map(|c| c.ministries.iter().map(|m| m.allocated_cash).sum())
        .unwrap_or(0.0)
}

// ============================================================================
// MINISTER AI — PHASE A: PRE-CLEARING STRATEGIES
// ============================================================================

/// Variant of `prepare_minister_strategies` that takes active_parties directly,
/// to avoid borrow conflicts when ministry_config is borrowed mutably from country.
pub fn prepare_minister_strategies_with_parties(
    ministry: &mut Ministry,
    active_parties: &HashMap<String, Party>,
    companies: &mut [Company],
    order_book: &mut OrderBook,
    country: &mut Country,
) -> f64 {
    if ministry.allocated_cash <= 0.0 || ministry.competencies.is_empty() {
        return 0.0;
    }

    let ideology = active_parties
        .get(&ministry.minister_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or(Ideology::SocialLiberalism);
    let priorities = ideology.budget_priorities();

    let total_weight: f64 = ministry
        .competencies
        .iter()
        .map(|c| priorities.weight_for(*c))
        .sum();

    if total_weight <= 0.0 {
        return 0.0;
    }

    let mut total_g_spending: f64 = 0.0;
    let competencies: Vec<GovernmentCompetency> = ministry.competencies.clone();
    for comp in competencies {
        let weight = priorities.weight_for(comp);
        let budget = ministry.allocated_cash * (weight / total_weight);
        if budget <= 0.0 {
            continue;
        }

        total_g_spending += execute_competency_spending_with_parties(ministry, comp, budget, companies, order_book, country);
    }
    total_g_spending
}

/// Variant of `execute_competency_spending` without the unused `country` parameter.
/// Phase 42: Returns total non-procurement spending (subsidies + infrastructure + direct transfers)
/// for GDP G-component accumulation. Healthcare/Education is excluded because it flows through
/// the State Employer wage path which already accumulates to G.
fn execute_competency_spending_with_parties(
    ministry: &mut Ministry,
    comp: GovernmentCompetency,
    budget: f64,
    companies: &mut [Company],
    order_book: &mut OrderBook,
    country: &mut Country,
) -> f64 {
    // Phase 35: Cap spending at ministry_cash (the pocket), NOT liquid_reserves.
    // allocate_cash_to_ministries already moved cash from liquid_reserves into
    // ministry_cash, so we debit from the pocket only — no double-debit.
    let available = ministry.ministry_cash.min(ministry.allocated_cash - ministry.spent_cash);
    let spend = budget.min(available);
    if spend <= 0.0 {
        return 0.0;
    }

    let mut total_g_spending: f64 = 0.0;

    match comp {
        GovernmentCompetency::HeavyIndustry
        | GovernmentCompetency::LightIndustry
        | GovernmentCompetency::Defense
        | GovernmentCompetency::InternalSecurity => {
            let commodities = match comp {
                GovernmentCompetency::HeavyIndustry => vec![
                    Commodity::Steel,
                    Commodity::IndustrialMachinery,
                ],
                GovernmentCompetency::LightIndustry => vec![
                    Commodity::Clothing,
                    Commodity::LuxuryClothing,
                ],
                GovernmentCompetency::Defense => vec![
                    Commodity::Steel,
                    Commodity::IndustrialMachinery,
                ],
                GovernmentCompetency::InternalSecurity => vec![
                    Commodity::Clothing,
                    Commodity::IndustrialMachinery,
                ],
                _ => vec![],
            };

            let per_commodity_budget = spend / commodities.len().max(1) as f64;
            for commodity in commodities {
                // Phase 28: Dynamic limit price based on reference price, not hardcoded 120.0.
                let ref_price = country.budget.extra
                    .get(&format!("{:?}", commodity))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(100.0);
                let limit_price = ref_price * 1.2; // 20% above reference price
                let quantity = per_commodity_budget / limit_price;
                if quantity > 0.0 {
                    let encumbrance = quantity * limit_price;
                    // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                    if ministry.spent_cash + encumbrance <= ministry.allocated_cash
                        && ministry.ministry_cash >= encumbrance
                    {
                        ministry.ministry_cash -= encumbrance;
                        ministry.spent_cash += encumbrance;
                        ministry.spending_actions.push(MinistrySpendingAction::B2BProcurementOrder {
                            commodity,
                            quantity,
                            limit_price,
                        });
                        order_book
                            .bids
                            .entry(commodity)
                            .or_insert_with(Vec::new)
                            .push(crate::economy::order_book::Bid {
                                buyer_id: ministry.id.clone(),
                                commodity,
                                quantity,
                                limit_price,
                                blueprint_id: None,
                                min_quality: None,
                            });
                    }
                }
            }
        }
        GovernmentCompetency::Agriculture => {
            let agri_companies: Vec<usize> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| c.sector == crate::registries::enums::Sector::Agriculture)
                .map(|(i, _)| i)
                .collect();
            if !agri_companies.is_empty() {
                let per_company = spend / agri_companies.len() as f64;
                for idx in &agri_companies {
                    let actual = per_company.min(ministry.ministry_cash);
                    if actual > 0.0 {
                        // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                        ministry.ministry_cash -= actual;
                        companies[*idx].liquid_capital += actual;
                        ministry.spent_cash += actual;
                        ministry.spending_actions.push(MinistrySpendingAction::Subsidy {
                            target_company_id: companies[*idx].id.clone(),
                            amount: actual,
                        });
                        total_g_spending += actual;
                    }
                }
            }
        }
        GovernmentCompetency::Infrastructure | GovernmentCompetency::Transport => {
            // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
            let actual = spend.min(ministry.ministry_cash);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::InfrastructureFunding {
                    target_building_id: "STATE_INFRA".to_string(),
                    amount: actual,
                });
                total_g_spending += actual;
            }
        }
        GovernmentCompetency::Healthcare | GovernmentCompetency::Education => {
            // Phase 33: Route public service wages through the State Employer
            // pseudo-company. The budget is added to ministry_public_service_pool.
            // Phase 35: Now debit ministry_cash (the pocket) since allocation
            // already moved cash from liquid_reserves into ministry_cash.
            let available = ministry.ministry_cash.min(ministry.allocated_cash - ministry.spent_cash);
            let actual = spend.min(available);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                country.ministry_public_service_pool += actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::PublicServiceWages {
                    building_ids: vec!["STATE_EMPLOYER_POOL".to_string()],
                    total_amount: actual,
                });
            }
        }
        GovernmentCompetency::SocialWelfare => {
            // Phase 13: SocialWelfare handled by dynamic SocialProgram system.
            // No-op here; cash is reserved for execute_social_welfare in turn loop.
        }
        GovernmentCompetency::Treasury
        | GovernmentCompetency::ForeignAffairs
        | GovernmentCompetency::Justice
        | GovernmentCompetency::Science
        | GovernmentCompetency::Energy
        | GovernmentCompetency::Culture
        | GovernmentCompetency::Environment
        | GovernmentCompetency::Labor
        | GovernmentCompetency::Housing
        | GovernmentCompetency::StateAssets => {
            // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
            let actual = spend.min(ministry.ministry_cash);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::DirectTransfer {
                    target: format!("{:?}", comp),
                    amount: actual,
                });
                total_g_spending += actual;
            }
        }
    }

    total_g_spending
}

/// Prepares minister spending strategies and submits B2B orders.
///
/// Called **before** B2B market clearing. The minister defines spending
/// strategies and submits orders. Direct cash transfers (subsidies, wages,
/// infrastructure funding) are also executed here.
///
/// # Arguments
/// * `ministry` - Mutable ministry executing its strategy.
/// * `country` - Country state (for tax rates, regions, etc.).
/// * `companies` - Mutable companies (for subsidies, B2B orders).
/// * `order_book` - Mutable order book for B2B bid submission.
///
/// # Rules
/// * Spending is pro-rated if `allocated_cash < sum_of_intended_spending`.
/// * B2B procurement orders encumber cash at submission time.
/// * Subsidies and direct transfers execute immediately (double-entry).
/// * Infrastructure funding happens here so buildings have reserves before Phase 7.
pub fn prepare_minister_strategies(
    ministry: &mut Ministry,
    country: &mut Country,
    companies: &mut [Company],
    order_book: &mut OrderBook,
) {
    if ministry.allocated_cash <= 0.0 || ministry.competencies.is_empty() {
        return;
    }

    // Look up minister ideology
    let ideology = country
        .politics
        .active_parties
        .get(&ministry.minister_party)
        .and_then(|p| Ideology::from_name(&p.ideology))
        .unwrap_or(Ideology::SocialLiberalism);
    let priorities = ideology.budget_priorities();

    // Calculate spending weights for each competency
    let total_weight: f64 = ministry
        .competencies
        .iter()
        .map(|c| priorities.weight_for(*c))
        .sum();

    if total_weight <= 0.0 {
        return;
    }

    // Distribute cash across competencies by weight
    let competencies: Vec<GovernmentCompetency> = ministry.competencies.clone();
    for comp in competencies {
        let weight = priorities.weight_for(comp);
        let budget = ministry.allocated_cash * (weight / total_weight);
        if budget <= 0.0 {
            continue;
        }

        execute_competency_spending(ministry, comp, budget, country, companies, order_book);
    }
}

/// Executes spending for a specific competency.
fn execute_competency_spending(
    ministry: &mut Ministry,
    comp: GovernmentCompetency,
    budget: f64,
    country: &mut Country,
    companies: &mut [Company],
    order_book: &mut OrderBook,
) {
    // Phase 35: Cap spending at ministry_cash (the pocket), NOT liquid_reserves.
    let available = ministry.ministry_cash.min(ministry.allocated_cash - ministry.spent_cash);
    let spend = budget.min(available);
    if spend <= 0.0 {
        return;
    }

    match comp {
        GovernmentCompetency::HeavyIndustry
        | GovernmentCompetency::LightIndustry
        | GovernmentCompetency::Defense
        | GovernmentCompetency::InternalSecurity => {
            // Submit B2B buy orders for relevant commodities
            let commodities = match comp {
                GovernmentCompetency::HeavyIndustry => vec![
                    Commodity::Steel,
                    Commodity::IndustrialMachinery,
                ],
                GovernmentCompetency::LightIndustry => vec![
                    Commodity::Clothing,
                    Commodity::LuxuryClothing,
                ],
                GovernmentCompetency::Defense => vec![
                    Commodity::Steel,
                    Commodity::IndustrialMachinery,
                ],
                GovernmentCompetency::InternalSecurity => vec![
                    Commodity::Clothing,
                    Commodity::IndustrialMachinery,
                ],
                _ => vec![],
            };

            let per_commodity_budget = spend / commodities.len().max(1) as f64;
            for commodity in commodities {
                // Phase 28: Dynamic limit price based on reference price, not hardcoded 120.0.
                let ref_price = country.budget.extra
                    .get(&format!("{:?}", commodity))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(100.0);
                let limit_price = ref_price * 1.2; // 20% above reference price
                let quantity = per_commodity_budget / limit_price;
                if quantity > 0.0 {
                    // Encumber cash
                    let encumbrance = quantity * limit_price;
                    // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                    if ministry.spent_cash + encumbrance <= ministry.allocated_cash
                        && ministry.ministry_cash >= encumbrance
                    {
                        ministry.ministry_cash -= encumbrance;
                        ministry.spent_cash += encumbrance;
                        ministry.spending_actions.push(MinistrySpendingAction::B2BProcurementOrder {
                            commodity,
                            quantity,
                            limit_price,
                        });
                        // Submit bid to order book
                        order_book
                            .bids
                            .entry(commodity)
                            .or_insert_with(Vec::new)
                            .push(crate::economy::order_book::Bid {
                                buyer_id: ministry.id.clone(),
                                commodity,
                                quantity,
                                limit_price,
                                blueprint_id: None,
                                min_quality: None,
                            });
                    }
                }
            }
        }
        GovernmentCompetency::Agriculture => {
            // Subsidies to agricultural companies
            let agri_companies: Vec<usize> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| c.sector == crate::registries::enums::Sector::Agriculture)
                .map(|(i, _)| i)
                .collect();
            if !agri_companies.is_empty() {
                let per_company = spend / agri_companies.len() as f64;
                for idx in &agri_companies {
                    let actual = per_company.min(ministry.ministry_cash);
                    if actual > 0.0 {
                        // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                        ministry.ministry_cash -= actual;
                        companies[*idx].liquid_capital += actual;
                        ministry.spent_cash += actual;
                        ministry.spending_actions.push(MinistrySpendingAction::Subsidy {
                            target_company_id: companies[*idx].id.clone(),
                            amount: actual,
                        });
                    }
                }
            }
        }
        GovernmentCompetency::Infrastructure | GovernmentCompetency::Transport => {
            // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
            let actual = spend.min(ministry.ministry_cash);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::InfrastructureFunding {
                    target_building_id: "STATE_INFRA".to_string(),
                    amount: actual,
                });
            }
        }
        GovernmentCompetency::Education | GovernmentCompetency::Healthcare => {
            // Phase 33: Route through State Employer pool (correction #2).
            // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
            let available = ministry.ministry_cash.min(ministry.allocated_cash - ministry.spent_cash);
            let actual = spend.min(available);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                country.ministry_public_service_pool += actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::PublicServiceWages {
                    building_ids: vec!["STATE_EMPLOYER_POOL".to_string()],
                    total_amount: actual,
                });
            }
        }
        GovernmentCompetency::SocialWelfare => {
            // Phase 13: SocialWelfare is now handled by the dynamic SocialProgram
            // system (execute_social_welfare) in the turn loop, which has access
            // to &mut Country. This arm is a no-op; cash is reserved for programs.
        }
        GovernmentCompetency::Labor => {
            // Phase 39: Labor ministry transfers cash to ministry_public_service_pool
            // to fund PublicWorksSite workers via the State Employer. This happens
            // BEFORE State Employer payroll processing. Strict double-entry:
            // debit ministry_cash, credit ministry_public_service_pool.
            let actual = spend.min(ministry.ministry_cash);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                country.ministry_public_service_pool += actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::PublicServiceWages {
                    building_ids: vec!["PUBLIC_WORKS_POOL".to_string()],
                    total_amount: actual,
                });
            }
        }
        GovernmentCompetency::Science => {
            // R&D grants to companies
            let tech_companies: Vec<usize> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.sector == crate::registries::enums::Sector::HeavyIndustry
                        || c.sector == crate::registries::enums::Sector::LightIndustry
                })
                .map(|(i, _)| i)
                .collect();
            if !tech_companies.is_empty() {
                let per_company = spend / tech_companies.len() as f64;
                for idx in &tech_companies {
                    let actual = per_company.min(ministry.ministry_cash);
                    if actual > 0.0 {
                        // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                        ministry.ministry_cash -= actual;
                        companies[*idx].liquid_capital += actual;
                        ministry.spent_cash += actual;
                        ministry.spending_actions.push(MinistrySpendingAction::RAndDGrant {
                            target_entity: companies[*idx].id.clone(),
                            amount: actual,
                        });
                    }
                }
            }
        }
        GovernmentCompetency::Treasury => {
            // Treasury ministry: discretionary fiscal operations only.
            // No debt service (handled centrally). Reserve for contingency.
            // Intentionally minimal spending.
        }
        GovernmentCompetency::Energy | GovernmentCompetency::Housing
        | GovernmentCompetency::Environment
        | GovernmentCompetency::Justice | GovernmentCompetency::ForeignAffairs => {
            // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
            let actual = spend.min(ministry.ministry_cash);
            if actual > 0.0 {
                ministry.ministry_cash -= actual;
                ministry.spent_cash += actual;
                ministry.spending_actions.push(MinistrySpendingAction::InfrastructureFunding {
                    target_building_id: format!("STATE_{:?}", comp).to_uppercase(),
                    amount: actual,
                });
            }
        }
        GovernmentCompetency::Culture => {
            // Culture competency submits maintenance B2B orders for cultural buildings
            // and heritage sites. Uses the maintenance BOM from building_condition.
            let maintenance_commodities = vec![
                Commodity::Timber,
                Commodity::Bricks,
                Commodity::Steel,
            ];
            let per_commodity_budget = spend / maintenance_commodities.len().max(1) as f64;
            for commodity in maintenance_commodities {
                let limit_price = 120.0;
                let quantity = per_commodity_budget / limit_price;
                if quantity > 0.0 {
                    let encumbrance = quantity * limit_price;
                    // Phase 35: Debit ministry_cash (the pocket), not liquid_reserves.
                    if ministry.spent_cash + encumbrance <= ministry.allocated_cash
                        && ministry.ministry_cash >= encumbrance
                    {
                        ministry.ministry_cash -= encumbrance;
                        ministry.spent_cash += encumbrance;
                        ministry.spending_actions.push(MinistrySpendingAction::B2BProcurementOrder {
                            commodity,
                            quantity,
                            limit_price,
                        });
                        order_book
                            .bids
                            .entry(commodity)
                            .or_insert_with(Vec::new)
                            .push(crate::economy::order_book::Bid {
                                buyer_id: ministry.id.clone(),
                                commodity,
                                quantity,
                                limit_price,
                                blueprint_id: None,
                                min_quality: None,
                            });
                    }
                }
            }
        }
        GovernmentCompetency::StateAssets => {
            // Phase 39: State Assets ministry subsidizes state-owned enterprises.
            // Transfer cash to SOE companies (state_share >= 1.0) to support
            // their operations. Strict double-entry: debit ministry_cash,
            // credit company liquid_capital.
            let soe_companies: Vec<usize> = companies
                .iter()
                .enumerate()
                .filter(|(_, c)| c.state_share >= 1.0)
                .map(|(i, _)| i)
                .collect();
            if !soe_companies.is_empty() {
                let per_company = spend / soe_companies.len() as f64;
                for idx in &soe_companies {
                    let actual = per_company.min(ministry.ministry_cash);
                    if actual > 0.0 {
                        ministry.ministry_cash -= actual;
                        companies[*idx].liquid_capital += actual;
                        ministry.spent_cash += actual;
                        ministry.spending_actions.push(MinistrySpendingAction::Subsidy {
                            target_company_id: companies[*idx].id.clone(),
                            amount: actual,
                        });
                    }
                }
            }
        }
    }
}

// ============================================================================
// MINISTER AI — PHASE B: POST-CLEARING RECONCILIATION
// ============================================================================

/// Reconciles ministry spending after B2B market clearing.
///
/// Called **after** `match_orders()`. Handles:
/// - Refunding unfilled bid encumbrances back to `ministry.allocated_cash`.
/// - Logging all executed trades as `MinistrySpendingAction` entries.
///
/// # Arguments
/// * `ministry` - Mutable ministry to reconcile.
/// * `order_book` - Reference to the post-clearing order book.
/// * `companies` - Mutable companies (for refund crediting).
///
/// # Rules
/// * Unfilled bid quantities are refunded at the original limit price.
/// * Refunds increase `ministry.allocated_cash` (unspent cash returns).
/// * `ministry.spent_cash` is reduced by the refund amount.
pub fn process_minister_post_clearing(
    ministry: &mut Ministry,
    order_book: &OrderBook,
    _companies: &mut [Company],
    _country: &mut Country,
) {
    // Find unfilled bids from this ministry
    for bids in order_book.bids.values() {
        for bid in bids {
            if bid.buyer_id == ministry.id && bid.quantity > 0.0 {
                // Refund unfilled quantity at original limit price
                let refund = bid.quantity * bid.limit_price;
                ministry.spent_cash -= refund;
                // Phase 35: Refund the ministry's cash pocket (not liquid_reserves,
                // since the encumbrance was debited from ministry_cash at submission).
                ministry.ministry_cash += refund;
            }
        }
    }

    // Log executed trades for this ministry
    for trade in &order_book.trades {
        if trade.buyer_id == ministry.id {
            // The trade was already settled during match_orders — cash moved
            // from encumbrance to seller. The spent_cash already reflects this.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_priorities_marxism() {
        let bp = Ideology::OrthodoxMarxism.budget_priorities();
        assert!(bp.heavy_industry > 0.8);
        assert!(bp.free_market < 0.1);
        assert!(bp.social_welfare > 0.8);
    }

    #[test]
    fn test_budget_priorities_neoliberal() {
        let bp = Ideology::Neoliberalism.budget_priorities();
        assert!(bp.free_market > 0.9);
        assert!(bp.social_welfare < 0.2);
        assert!(bp.heavy_industry < 0.3);
    }

    #[test]
    fn test_allocate_cash_pro_rated() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 500.0;
        country.politics.ministry_config = Some(MinistryConfig {
            ministries: vec![
                Ministry {
                    id: "MIN-001".into(),
                    name: "Test A".into(),
                    competencies: vec![GovernmentCompetency::Treasury],
                    minister_party: "P1".into(),
                    minister_name: "A".into(),
                    allocated_cash: 400.0,
                    spent_cash: 0.0,
                    spending_actions: vec![],
                    ministry_cash: 0.0,
                },
                Ministry {
                    id: "MIN-002".into(),
                    name: "Test B".into(),
                    competencies: vec![GovernmentCompetency::Defense],
                    minister_party: "P2".into(),
                    minister_name: "B".into(),
                    allocated_cash: 600.0,
                    spent_cash: 0.0,
                    spending_actions: vec![],
                    ministry_cash: 0.0,
                },
            ],
            formation_turn: 0,
            pm_party: "P1".into(),
        });

        allocate_cash_to_ministries(&mut country);

        let config = country.politics.ministry_config.as_ref().unwrap();
        // ratio = 500/1000 = 0.5
        assert!((config.ministries[0].allocated_cash - 200.0).abs() < 1e-6);
        assert!((config.ministries[1].allocated_cash - 300.0).abs() < 1e-6);
        assert!((country.budget.liquid_reserves - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_allocate_cash_full_funding() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 2000.0;
        country.politics.ministry_config = Some(MinistryConfig {
            ministries: vec![Ministry {
                id: "MIN-001".into(),
                name: "Test".into(),
                competencies: vec![GovernmentCompetency::Treasury],
                minister_party: "P1".into(),
                minister_name: "A".into(),
                allocated_cash: 500.0,
                spent_cash: 0.0,
                spending_actions: vec![],
                ministry_cash: 0.0,
            }],
            formation_turn: 0,
            pm_party: "P1".into(),
        });

        allocate_cash_to_ministries(&mut country);

        let config = country.politics.ministry_config.as_ref().unwrap();
        assert!((config.ministries[0].allocated_cash - 500.0).abs() < 1e-6);
        assert!((country.budget.liquid_reserves - 1500.0).abs() < 1e-6);
    }

    // Phase 33: English ministry names and minister name fallback tests.

    #[test]
    fn test_ministry_display_names_are_english() {
        assert!(competency_display_name(GovernmentCompetency::Energy).contains("Energy"));
        assert!(competency_display_name(GovernmentCompetency::Defense).contains("Defense"));
        assert!(competency_display_name(GovernmentCompetency::Healthcare).contains("Health"));
        assert!(competency_display_name(GovernmentCompetency::Education).contains("Education"));
        assert!(competency_display_name(GovernmentCompetency::Treasury).contains("Treasury"));
        // Should NOT contain Polish strings.
        assert!(!competency_display_name(GovernmentCompetency::Energy).contains("Ministerstwo"));
    }

    #[test]
    fn test_resolve_minister_name_with_empty_leader() {
        let mut parties = HashMap::new();
        let mut party = Party::default();
        party.leader.name = String::new(); // Empty name — the bug we're fixing.
        parties.insert("P1".to_string(), party);
        let name = resolve_minister_name(&parties, "P1", "slavic");
        // Phase 39: Should generate a random VIP name, not "Minister (P1)".
        assert!(!name.is_empty(), "Fallback should provide a non-empty name");
        assert!(!name.contains("Minister ()"), "Should not produce 'Minister ()'");
        assert!(!name.contains("(P1)"), "Should not contain party ID in parentheses");
    }

    #[test]
    fn test_resolve_minister_name_with_named_leader() {
        let mut parties = HashMap::new();
        let mut party = Party::default();
        party.leader.name = "Jan Kowalski".to_string();
        parties.insert("P1".to_string(), party);
        let name = resolve_minister_name(&parties, "P1", "slavic");
        assert_eq!(name, "Jan Kowalski");
    }

    #[test]
    fn test_resolve_minister_name_missing_party() {
        let parties = HashMap::new();
        let name = resolve_minister_name(&parties, "NONEXISTENT", "slavic");
        // Phase 39: Should generate a random VIP name, not "Minister (NONEXISTENT)".
        assert!(!name.is_empty(), "Should provide fallback for missing party");
        assert!(!name.contains("(NONEXISTENT)"), "Should not contain party ID in parentheses");
    }
}
