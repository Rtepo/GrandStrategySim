//! Parliamentary committee system for bill review and recommendation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parliamentary committee for reviewing legislation
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Committee {
    /// Committee ID
    #[serde(default)]
    pub id: String,

    /// Committee name
    #[serde(default)]
    pub name: String,

    /// Committee type

    pub committee_type: CommitteeType,

    /// Members by party
    #[serde(default)]
    pub members: HashMap<String, u32>,

    /// Chair party
    #[serde(default)]
    pub chair: String,

    /// Phase 48: Chair VIP ID (references the global VIP registry).
    /// When None, the chair is identified by party name only (legacy behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chair_vip_id: Option<String>,

    /// Partisan bias (-1.0 to 1.0, negative = opposition, positive = government)
    #[serde(default)]
    pub partisan_bias: f64,

    /// Bills currently under review (bill IDs)
    #[serde(default)]
    pub bills_under_review: Vec<String>,
}

/// Type of parliamentary committee
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommitteeType {
    #[default]
    Budget,
    Health,
    Education,
    Defense,
    ForeignAffairs,
    Justice,
    Infrastructure,
    SocialAffairs,
}

impl Committee {
    /// Create a new committee
    /// 
    /// # Arguments
    /// * `id` - Unique committee identifier
    /// * `name` - Committee name
    /// * `committee_type` - Type of committee
    /// * `parliament` - Current parliamentary seat distribution
    /// * `ruling_coalition` - Parties in ruling coalition
    /// 
    /// # Returns
    /// New Committee with composition mirroring parliament
    /// 
    /// # Rules
    /// * Committee composition mirrors parliament proportions exactly
    /// * Ruling coalition always secures the Chairmanship
    /// * Committee type influences partisan bias
    pub fn new(
        id: String,
        name: String,
        committee_type: CommitteeType,
        parliament: &HashMap<String, u32>,
        ruling_coalition: &[String],
    ) -> Self {
        // Committee composition mirrors parliament proportions exactly
        let total_seats: u32 = parliament.values().sum();
        let committee_size = (total_seats as f64 * 0.15) as u32; // 15% of parliament
        let committee_size = committee_size.max(5).min(20); // 5-20 members
        
        let mut members = HashMap::new();
        let mut allocated = 0;
        
        for (party, seats) in parliament {
            let proportion = *seats as f64 / total_seats as f64;
            let committee_seats = (proportion * committee_size as f64) as u32;
            if committee_seats > 0 {
                members.insert(party.clone(), committee_seats);
                allocated += committee_seats;
            }
        }
        
        // Distribute remaining seats to largest party
        if allocated < committee_size {
            if let Some((largest_party, _)) = parliament.iter().max_by_key(|(_, s)| *s) {
                *members.get_mut(largest_party).unwrap_or(&mut 0) += committee_size - allocated;
            }
        }
        
        // Ruling coalition always secures the Chairmanship
        let chair = ruling_coalition
            .first()
            .unwrap_or(&"Independents".to_string())
            .clone();
        
        // Calculate partisan bias based on committee type
        let partisan_bias = Self::calculate_partisan_bias(&committee_type, parliament, ruling_coalition);
        
        Committee {
            id,
            name,
            committee_type,
            members,
            chair,
            chair_vip_id: None,
            partisan_bias,
            bills_under_review: Vec::new(),
        }
    }
    
    /// Calculate partisan bias for a committee type
    /// 
    /// # Arguments
    /// * `committee_type` - Type of committee
    /// * `parliament` - Current parliamentary seat distribution
    /// * `ruling_coalition` - Parties in ruling coalition
    /// 
    /// # Returns
    /// Partisan bias (-1.0 to 1.0)
    /// 
    /// # Rules
    /// * Defense committee favors military interest groups (pro-government bias)
    /// * Budget committee favors fiscal conservatives (variable bias)
    /// * Social affairs favors progressive parties (pro-opposition bias)
    fn calculate_partisan_bias(
        committee_type: &CommitteeType,
        parliament: &HashMap<String, u32>,
        ruling_coalition: &[String],
    ) -> f64 {
        let total_seats: u32 = parliament.values().sum();
        let coalition_seats: u32 = ruling_coalition.iter()
            .filter_map(|p| parliament.get(p))
            .sum();
        
        let coalition_share = coalition_seats as f64 / total_seats as f64;
        
        match committee_type {
            CommitteeType::Defense => {
                // Defense committee favors government (military interest groups)
                0.3 + (coalition_share - 0.5) * 0.4
            }
            CommitteeType::Budget => {
                // Budget committee favors fiscal conservatives (variable)
                (coalition_share - 0.5) * 0.6
            }
            CommitteeType::SocialAffairs => {
                // Social affairs favors progressive parties (often opposition)
                -0.2 + (coalition_share - 0.5) * 0.3
            }
            CommitteeType::Health | CommitteeType::Education => {
                // Health and education committees have moderate bias
                (coalition_share - 0.5) * 0.2
            }
            CommitteeType::ForeignAffairs => {
                // Foreign affairs favors executive (pro-government)
                0.2 + (coalition_share - 0.5) * 0.3
            }
            CommitteeType::Justice => {
                // Justice committee favors conservatives (variable)
                (coalition_share - 0.5) * 0.4
            }
            CommitteeType::Infrastructure => {
                // Infrastructure committee has minimal bias
                (coalition_share - 0.5) * 0.1
            }
        }
    }
    
    /// Assign a bill to this committee for review
    /// 
    /// # Arguments
    /// * `bill_id` - ID of bill to assign
    /// 
    /// # Rules
    /// * Bill must match committee type or be general legislation
    pub fn assign_bill(&mut self, bill_id: String) {
        if !self.bills_under_review.contains(&bill_id) {
            self.bills_under_review.push(bill_id);
        }
    }
    
    /// Calculate committee recommendation modifier for a bill
    /// 
    /// # Arguments
    /// * `bill_ideology` - Ideological vector of the bill
    /// * `initiator_party` - Party that initiated the bill
    /// * `is_ruling_party` - Whether initiator is in ruling coalition
    /// 
    /// # Returns
    /// Recommendation modifier (-0.3 to +0.3)
    /// 
    /// # Rules
    /// * Recommendation modifier = partisan_bias * 0.3
    /// * Pro-government bias helps ruling party bills
    /// * Anti-government bias helps opposition bills
    pub fn calculate_recommendation(
        &self,
        _bill_ideology: &crate::politics::ideology::IdeologyCompass,
        _initiator_party: &str,
        is_ruling_party: bool,
    ) -> f64 {
        let base_modifier = self.partisan_bias * 0.3;
        
        // If initiator is ruling party, pro-government bias helps
        if is_ruling_party {
            base_modifier.abs().min(0.3)
        } else {
            // If initiator is opposition, anti-government bias helps
            -base_modifier.abs().max(-0.3)
        }
    }
    
    /// Calculate committee delay for a bill
    /// 
    /// # Arguments
    /// * `bill_complexity` - Complexity score of the bill (0-10)
    /// 
    /// # Returns
    /// Number of turns for committee review (1-3)
    /// 
    /// # Rules
    /// * 1 turn for minor bills (complexity 0-3)
    /// * 2 turns for moderate bills (complexity 4-7)
    /// * 3 turns for massive reforms (complexity 8-10)
    pub fn calculate_delay(&self, bill_complexity: u32) -> u32 {
        match bill_complexity {
            0..=3 => 1,
            4..=7 => 2,
            8..=10 => 3,
            _ => 2,
        }
    }
    
    /// Remove a bill from committee review
    /// 
    /// # Arguments
    /// * `bill_id` - ID of bill to remove
    pub fn remove_bill(&mut self, bill_id: &str) {
        self.bills_under_review.retain(|id| id != bill_id);
    }
}

/// Collection of all parliamentary committees
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CommitteeSystem {
    /// Active committees by ID
    #[serde(default)]
    pub committees: HashMap<String, Committee>,
    
    /// Committee assignments for bill types
    #[serde(default)]
    pub bill_type_assignments: HashMap<String, String>,
}

impl CommitteeSystem {
    /// Create a new committee system
    /// 
    /// # Returns
    /// New CommitteeSystem with standard committees
    pub fn new() -> Self {
        let mut system = CommitteeSystem {
            committees: HashMap::new(),
            bill_type_assignments: HashMap::new(),
        };
        
        // Set up standard bill type assignments
        system.bill_type_assignments.insert("budget".to_string(), "budget_committee".to_string());
        system.bill_type_assignments.insert("health".to_string(), "health_committee".to_string());
        system.bill_type_assignments.insert("education".to_string(), "education_committee".to_string());
        system.bill_type_assignments.insert("defense".to_string(), "defense_committee".to_string());
        system.bill_type_assignments.insert("foreign".to_string(), "foreign_affairs_committee".to_string());
        system.bill_type_assignments.insert("justice".to_string(), "justice_committee".to_string());
        system.bill_type_assignments.insert("infrastructure".to_string(), "infrastructure_committee".to_string());
        system.bill_type_assignments.insert("social".to_string(), "social_affairs_committee".to_string());
        
        system
    }
    
    /// Initialize all standard committees
    /// 
    /// # Arguments
    /// * `parliament` - Current parliamentary seat distribution
    /// * `ruling_coalition` - Parties in ruling coalition
    pub fn initialize_committees(
        &mut self,
        parliament: &HashMap<String, u32>,
        ruling_coalition: &[String],
    ) {
        self.committees = HashMap::new();
        
        let committee_types = vec![
            (CommitteeType::Budget, "Budget Committee".to_string()),
            (CommitteeType::Health, "Health Committee".to_string()),
            (CommitteeType::Education, "Education Committee".to_string()),
            (CommitteeType::Defense, "Defense Committee".to_string()),
            (CommitteeType::ForeignAffairs, "Foreign Affairs Committee".to_string()),
            (CommitteeType::Justice, "Justice Committee".to_string()),
            (CommitteeType::Infrastructure, "Infrastructure Committee".to_string()),
            (CommitteeType::SocialAffairs, "Social Affairs Committee".to_string()),
        ];
        
        for (committee_type, name) in committee_types {
            let id = format!("{}_committee", format!("{:?}", committee_type).to_lowercase());
            let committee = Committee::new(id.clone(), name, committee_type, parliament, ruling_coalition);
            self.committees.insert(id, committee);
        }
    }
    
    /// Get appropriate committee for a bill type
    /// 
    /// # Arguments
    /// * `bill_type` - Type of bill (budget, health, etc.)
    /// 
    /// # Returns
    /// Committee ID if found, None otherwise
    pub fn get_committee_for_bill(&self, bill_type: &str) -> Option<&String> {
        self.bill_type_assignments.get(bill_type)
    }
    
    /// Get committee by ID
    /// 
    /// # Arguments
    /// * `committee_id` - ID of committee
    /// 
    /// # Returns
    /// Committee reference if found, None otherwise
    pub fn get_committee(&self, committee_id: &str) -> Option<&Committee> {
        self.committees.get(committee_id)
    }
    
    /// Get mutable committee by ID
    /// 
    /// # Arguments
    /// * `committee_id` - ID of committee
    /// 
    /// # Returns
    /// Mutable committee reference if found, None otherwise
    pub fn get_committee_mut(&mut self, committee_id: &str) -> Option<&mut Committee> {
        self.committees.get_mut(committee_id)
    }
}

// ============================================================================
// PHASE 48: COMMITTEE CHAIR MUTATION POWER
// ============================================================================

use crate::politics::ideology::IdeologyCompass;
use crate::politics::legislation::{Bill, BillProvision, Clause};
use crate::politics::vip_registry::Vip;

/// Committee chair action on a bill under review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChairAction {
    /// Pass the bill through unchanged.
    Pass,
    /// Dilute a provision (reduce tax change, lower subsidy amount).
    Dilute { clause_index: usize, dilution_factor: f64 },
    /// Poison a bill by adding an unpopular rider.
    PoisonRider { rider: BillProvision, description: String },
    /// Strip a provision entirely.
    Strip { clause_index: usize },
    /// Block the bill in committee (delays indefinitely).
    Block,
    /// Fast-track the bill (skip normal delay).
    FastTrack,
}

/// Determine what a committee chair does with a bill.
///
/// Uses a strict deterministic scoring matrix. No speculation.
///
/// ## Algorithm
///
/// 1. Calculate `ideological_distance` = Manhattan distance between the bill's
///    ideological vector and the chair's party ideology vector.
/// 2. Calculate `alignment_score` = 1.0 - (ideological_distance / 2.0).
/// 3. Apply trait modifiers to `alignment_score`.
/// 4. Clamp to [0.0, 1.0].
/// 5. Apply party discipline factor.
/// 6. Resolve into `ChairAction` using fixed thresholds.
/// 7. Apply Ambitious trait override.
/// 8. Apply Corrupt trait override.
pub fn determine_chair_action(
    chair: &Vip,
    bill: &Bill,
    _committee: &Committee,
    is_ruling_party_bill: bool,
    chair_party_discipline: f64,
    chair_party_ideology: &IdeologyCompass,
) -> ChairAction {
    // Step 1: ideological distance (Manhattan).
    let bill_ideology = bill.calculate_ideological_impact();
    let ideological_distance = (bill_ideology.economy - chair_party_ideology.economy).abs()
        + (bill_ideology.liberty - chair_party_ideology.liberty).abs()
        + (bill_ideology.tradition - chair_party_ideology.tradition).abs();

    // Step 2: alignment score.
    let mut alignment = 1.0 - (ideological_distance / 2.0);

    // Step 3: trait modifiers.
    let has = |trait_id: &str| chair.has_trait(trait_id);

    if has("Loyal") && is_ruling_party_bill {
        alignment += 0.20;
    }
    if has("Ambitious") && !is_ruling_party_bill {
        alignment -= 0.15;
    }
    if has("Corrupt") {
        let concession_targeted = bill.concessions.iter()
            .any(|c| c.target == chair.faction);
        if concession_targeted {
            alignment += 0.25;
        }
    }
    if has("Conservative") && bill_ideology.economy > 0.3 {
        alignment -= 0.20;
    }
    if has("Populist") {
        let has_elite_provision = bill.core_clauses.iter().any(|c| {
            c.provision.as_ref().map(|p| p.favors_elites()).unwrap_or(false)
        });
        if has_elite_provision {
            alignment -= 0.20;
        }
    }
    if has("Reformer") && bill_ideology.economy > 0.0 {
        alignment += 0.15;
    }
    if has("Cruel") && !is_ruling_party_bill {
        alignment -= 0.10;
    }
    if has("Paranoid") {
        // Check if bill reduces internal security (FreeSpeechLaw with Full level).
        let reduces_security = bill.core_clauses.iter().any(|c| {
            matches!(c.provision, Some(BillProvision::FreeSpeechLaw(_)))
        });
        if reduces_security {
            alignment -= 0.25;
        }
    }

    // Step 4: clamp.
    alignment = alignment.clamp(0.0, 1.0);

    // Step 5: party discipline.
    alignment += (chair_party_discipline - 0.5) * 0.2;
    alignment = alignment.clamp(0.0, 1.0);

    // Step 6: resolve into action via fixed thresholds.
    let action = if alignment >= 0.85 {
        if is_ruling_party_bill { ChairAction::FastTrack }
        else { ChairAction::Pass }
    } else if alignment >= 0.60 {
        ChairAction::Pass
    } else if alignment >= 0.40 {
        let idx = most_distant_clause(&bill.core_clauses, chair_party_ideology);
        ChairAction::Dilute { clause_index: idx, dilution_factor: 0.5 }
    } else if alignment >= 0.20 {
        let idx = most_distant_clause(&bill.core_clauses, chair_party_ideology);
        ChairAction::Strip { clause_index: idx }
    } else if alignment >= 0.10 {
        let rider = build_unpopular_rider(&chair.faction);
        ChairAction::PoisonRider {
            rider,
            description: format!("Rider opposed by {}", chair.faction),
        }
    } else {
        ChairAction::Block
    };

    // Step 7: Ambitious override.
    if has("Ambitious") && alignment < 0.50 && is_ruling_party_bill
        && !matches!(action, ChairAction::PoisonRider { .. } | ChairAction::Block) {
            let rider = build_unpopular_rider(&chair.faction);
            return ChairAction::PoisonRider {
                rider,
                description: "Ambitious chair poisons government bill".to_string(),
            };
        }

    // Step 8: Corrupt override.
    if has("Corrupt") && alignment < 0.50 {
        let concession_targeted = bill.concessions.iter()
            .any(|c| c.target == chair.faction);
        if !concession_targeted && matches!(action, ChairAction::Block) {
            let idx = most_distant_clause(&bill.core_clauses, chair_party_ideology);
            return ChairAction::Strip { clause_index: idx };
        }
    }

    action
}

/// Find the clause index with the highest ideological distance from the chair.
fn most_distant_clause(clauses: &[Clause], ideology: &IdeologyCompass) -> usize {
    clauses.iter().enumerate()
        .max_by(|(_, a), (_, b)| {
            let da = (a.ideological_vector.economy - ideology.economy).abs()
                + (a.ideological_vector.liberty - ideology.liberty).abs()
                + (a.ideological_vector.tradition - ideology.tradition).abs();
            let db = (b.ideological_vector.economy - ideology.economy).abs()
                + (b.ideological_vector.liberty - ideology.liberty).abs()
                + (b.ideological_vector.tradition - ideology.tradition).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Build an unpopular rider provision for a given faction.
/// Uses a fixed faction-to-opposition lookup table.
fn build_unpopular_rider(faction: &str) -> BillProvision {
    match faction {
        "Capitalists" | "Petty Bourgeoisie" => BillProvision::TaxRateChange {
            income_tax: Some(0.45),
            vat: None,
            corporate_tax: Some(0.35),
        },
        "Trade Unions" | "Robotnicy" => BillProvision::Deregulation {
            sector: "HeavyIndustry".to_string(),
            scope: "labor_protections".to_string(),
        },
        "Agrarians" | "Peasants" => BillProvision::TaxRateChange {
            income_tax: None,
            vat: Some(0.25),
            corporate_tax: None,
        },
        "Intelligentsia" => BillProvision::Deregulation {
            sector: "Education".to_string(),
            scope: "academic_tenure".to_string(),
        },
        _ => BillProvision::TaxRateChange {
            income_tax: Some(0.30),
            vat: Some(0.20),
            corporate_tax: None,
        },
    }
}

#[cfg(test)]
mod phase48_tests {
    use super::*;
    use crate::politics::legislation::{Clause, LegislativeStage};
    use crate::politics::vip_registry::Vip;

    fn make_test_bill(economy: f64, liberty: f64, tradition: f64) -> Bill {
        Bill {
            id: "TEST-001".to_string(),
            title: "Test Bill".to_string(),
            initiator: "RulingParty".to_string(),
            core_clauses: vec![Clause {
                description: "Test clause".to_string(),
                ideological_vector: IdeologyCompass { economy, liberty, tradition },
                budget_impact: 100.0,
                provision: None,
                sunset_turn: None,
                mutated: false,
                mutation_notes: Vec::new(),
            }],
            concessions: Vec::new(),
            stage: LegislativeStage::Committee,
            committee: None,
            committee_modifier: 0.0,
            introduction_turn: 0,
            committee_completion_turn: None,
            weight: crate::politics::legislative_weight::LegislativeWeight::Ordinary,
        }
    }

    fn make_test_chair(traits: Vec<String>, faction: String) -> Vip {
        Vip {
            id: "VIP-CHAIR".to_string(),
            full_name: "Chair Person".to_string(),
            traits,
            faction,
            ..Default::default()
        }
    }

    fn centrist_ideology() -> IdeologyCompass {
        IdeologyCompass { economy: 0.0, liberty: 0.0, tradition: 0.0 }
    }

    #[test]
    fn test_high_alignment_ruling_bill_fasttrack() {
        let chair = make_test_chair(vec!["Loyal".to_string()], "Royal Court".to_string());
        let bill = make_test_bill(0.0, 0.0, 0.0); // Perfect alignment.
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, true, 0.8, &centrist_ideology());
        assert!(matches!(action, ChairAction::FastTrack), "Loyal chair + aligned ruling bill should FastTrack");
    }

    #[test]
    fn test_high_alignment_opposition_bill_pass() {
        let chair = make_test_chair(vec![], "Royal Court".to_string());
        let bill = make_test_bill(0.0, 0.0, 0.0);
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, false, 0.5, &centrist_ideology());
        assert!(matches!(action, ChairAction::Pass), "Aligned opposition bill should Pass");
    }

    #[test]
    fn test_low_alignment_block() {
        let chair = make_test_chair(vec![], "Royal Court".to_string());
        // Bill is far from chair's ideology.
        let bill = make_test_bill(1.0, 1.0, 1.0);
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, false, 0.5, &centrist_ideology());
        assert!(matches!(action, ChairAction::Block), "Very low alignment should Block");
    }

    #[test]
    fn test_ambitious_override_poisons_government_bill() {
        let chair = make_test_chair(vec!["Ambitious".to_string()], "Royal Court".to_string());
        // Bill is moderately misaligned (alignment = 1.0 - 1.2/2.0 = 0.4).
        let bill = make_test_bill(0.6, 0.6, 0.0);
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, true, 0.5, &centrist_ideology());
        // Ambitious chair should poison the government bill (alignment 0.4 < 0.50).
        assert!(matches!(action, ChairAction::PoisonRider { .. }), "Ambitious chair should poison government bill");
    }

    #[test]
    fn test_corrupt_override_block_to_strip() {
        let chair = make_test_chair(vec!["Corrupt".to_string()], "Royal Court".to_string());
        // Bill is far from chair, no concessions targeting chair's faction.
        let bill = make_test_bill(1.0, 1.0, 1.0);
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, false, 0.5, &centrist_ideology());
        // Corrupt chair without concessions should Strip instead of Block.
        assert!(matches!(action, ChairAction::Strip { .. }), "Corrupt chair should Strip instead of Block");
    }

    #[test]
    fn test_populist_penalizes_elite_provisions() {
        let chair = make_test_chair(vec!["Populist".to_string()], "Royal Court".to_string());
        let mut bill = make_test_bill(0.0, 0.0, 0.0);
        // Add an elite-favoring provision.
        bill.core_clauses[0].provision = Some(BillProvision::Subsidy {
            target: "HeavyIndustry".to_string(),
            amount_per_unit: 5.0,
        });
        let committee = Committee::default();
        let action = determine_chair_action(&chair, &bill, &committee, false, 0.5, &centrist_ideology());
        // Populist should penalize elite provisions, lowering alignment.
        // With 0.0 ideology + Populist -0.20, alignment ~0.8 - 0.2 = 0.6 → Pass.
        // But let's verify it's not FastTrack (which requires >= 0.85).
        assert!(matches!(action, ChairAction::Pass), "Populist should not fast-track elite bill");
    }

    #[test]
    fn test_build_unpopular_rider_capitalists() {
        let rider = build_unpopular_rider("Capitalists");
        match rider {
            BillProvision::TaxRateChange { income_tax, corporate_tax, .. } => {
                assert!(income_tax.is_some(), "Capitalist rider should raise income tax");
                assert!(corporate_tax.is_some(), "Capitalist rider should raise corporate tax");
            }
            _ => panic!("Should be TaxRateChange for capitalists"),
        }
    }

    #[test]
    fn test_build_unpopular_rider_unions() {
        let rider = build_unpopular_rider("Trade Unions");
        match rider {
            BillProvision::Deregulation { sector, .. } => {
                assert_eq!(sector, "HeavyIndustry");
            }
            _ => panic!("Should be Deregulation for unions"),
        }
    }

    #[test]
    fn test_build_unpopular_rider_default() {
        let rider = build_unpopular_rider("UnknownFaction");
        match rider {
            BillProvision::TaxRateChange { income_tax, vat, .. } => {
                assert!(income_tax.is_some());
                assert!(vat.is_some());
            }
            _ => panic!("Default rider should be TaxRateChange"),
        }
    }

    #[test]
    fn test_most_distant_clause() {
        let ideology = IdeologyCompass { economy: 0.0, liberty: 0.0, tradition: 0.0 };
        let clauses = vec![
            Clause {
                description: "Close".to_string(),
                ideological_vector: IdeologyCompass { economy: 0.1, liberty: 0.0, tradition: 0.0 },
                ..Default::default()
            },
            Clause {
                description: "Far".to_string(),
                ideological_vector: IdeologyCompass { economy: 0.9, liberty: 0.9, tradition: 0.0 },
                ..Default::default()
            },
        ];
        let idx = most_distant_clause(&clauses, &ideology);
        assert_eq!(idx, 1, "Should find the most distant clause");
    }

    #[test]
    fn test_committee_has_chair_vip_id_field() {
        let committee = Committee::default();
        assert!(committee.chair_vip_id.is_none(), "Default chair_vip_id should be None");
    }
}
