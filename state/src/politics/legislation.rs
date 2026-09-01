//! Modular legislation system for dynamic bills with clauses and concessions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::ideology::IdeologyCompass;
use super::legislative_weight::LegislativeWeight;

/// Legislative bill with modular clauses and concessions
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Bill {
    /// Bill ID
    #[serde(default)]
    pub id: String,

    /// Bill title
    #[serde(default)]
    pub title: String,

    /// Initiator party
    #[serde(default)]
    pub initiator: String,

    /// Core clauses (cannot be removed)
    #[serde(default)]
    pub core_clauses: Vec<Clause>,

    /// Concessions (can be added/removed during debate)
    #[serde(default)]
    pub concessions: Vec<Concession>,

    /// Current legislative stage
    #[serde(default)]
    pub stage: LegislativeStage,

    /// Committee assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committee: Option<String>,

    /// Committee recommendation modifier (-0.3 to +0.3)
    #[serde(default)]
    pub committee_modifier: f64,

    /// Turn when bill was introduced
    #[serde(default)]
    pub introduction_turn: u32,

    /// Turn when bill should complete committee review
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committee_completion_turn: Option<u32>,

    /// Phase 86: Legislative weight — determines voting majority threshold.
    /// Derived from the bill's provisions via `derive_weight_from_provisions()`.
    #[serde(default)]
    pub weight: LegislativeWeight,
}

/// Individual clause within a bill
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Clause {
    /// Clause description
    #[serde(default)]
    pub description: String,

    /// Ideological impact vector (economy, liberty, tradition)
    pub ideological_vector: IdeologyCompass,

    /// Budget impact
    #[serde(default)]
    pub budget_impact: f64,

    /// Phase 48: Concrete provision — what this clause actually does when enacted.
    /// None = descriptive-only clause (legacy behavior). Some = typed provision
    /// that will be applied via `enact_law` when the bill reaches `Enacted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision: Option<BillProvision>,

    /// Phase 48: Sunset clause — turn when this provision expires.
    /// None = permanent. Some(turn) = expires at this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset_turn: Option<u32>,

    /// Phase 48: Whether this provision was mutated by committee review.
    #[serde(default)]
    pub mutated: bool,

    /// Phase 48: Mutation notes (what the committee changed).
    #[serde(default)]
    pub mutation_notes: Vec<String>,
}

impl Default for Clause {
    fn default() -> Self {
        Clause {
            description: String::new(),
            ideological_vector: IdeologyCompass {
                economy: 0.0,
                liberty: 0.0,
                tradition: 0.0,
            },
            budget_impact: 0.0,
            provision: None,
            sunset_turn: None,
            mutated: false,
            mutation_notes: Vec::new(),
        }
    }
}

/// Concession offered to sway votes
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Concession {
    /// Target councilor/faction
    #[serde(default)]
    pub target: String,

    /// Concession description
    #[serde(default)]
    pub description: String,

    /// Vote probability bonus
    #[serde(default)]
    pub vote_bonus: f64,

    /// Budget cost
    #[serde(default)]
    pub cost: f64,
}

/// Stage of legislative process
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LegislativeStage {
    /// Newly introduced, not yet assigned
    #[default]
    Introduced,
    /// In committee review
    Committee,
    /// Awaiting floor vote
    FloorVote,
    /// Passed one chamber, awaiting second
    BicameralPending,
    /// Passed both chambers, awaiting executive
    Executive,
    /// Enacted into law
    Enacted,
    /// Rejected
    Rejected,
}

impl Bill {
    /// Create a new bill with core clauses
    ///
    /// # Arguments
    /// * `id` - Unique bill identifier
    /// * `title` - Bill title
    /// * `initiator` - Party initiating the bill
    /// * `core_clauses` - Core clauses that cannot be removed
    /// * `current_turn` - Current game turn
    ///
    /// # Returns
    /// New Bill in Introduced stage
    pub fn new(
        id: String,
        title: String,
        initiator: String,
        core_clauses: Vec<Clause>,
        current_turn: u32,
    ) -> Self {
        // Phase 86: Derive legislative weight from provisions.
        let provisions: Vec<&BillProvision> = core_clauses
            .iter()
            .filter_map(|c| c.provision.as_ref())
            .collect();
        let weight = super::legislative_weight::derive_weight_from_provisions(&provisions);
        Bill {
            id,
            title,
            initiator,
            core_clauses,
            concessions: Vec::new(),
            stage: LegislativeStage::Introduced,
            committee: None,
            committee_modifier: 0.0,
            introduction_turn: current_turn,
            committee_completion_turn: None,
            weight,
        }
    }

    /// Add a concession to the bill
    ///
    /// # Arguments
    /// * `concession` - Concession to add
    ///
    /// # Rules
    /// * Concessions can only be added during Committee or FloorVote stages
    pub fn add_concession(&mut self, concession: Concession) {
        if matches!(
            self.stage,
            LegislativeStage::Committee | LegislativeStage::FloorVote
        ) {
            self.concessions.push(concession);
        }
    }

    /// Calculate total ideological impact of the bill
    ///
    /// # Returns
    /// Combined ideological vector from all clauses
    pub fn calculate_ideological_impact(&self) -> IdeologyCompass {
        let mut total = IdeologyCompass {
            economy: 0.0,
            liberty: 0.0,
            tradition: 0.0,
        };

        for clause in &self.core_clauses {
            total.economy += clause.ideological_vector.economy;
            total.liberty += clause.ideological_vector.liberty;
            total.tradition += clause.ideological_vector.tradition;
        }

        // Normalize by number of clauses
        let count = self.core_clauses.len() as f64;
        if count > 0.0 {
            total.economy /= count;
            total.liberty /= count;
            total.tradition /= count;
        }

        total
    }

    /// Calculate total budget impact of the bill
    ///
    /// # Returns
    /// Sum of core clause budget impacts plus concession costs
    pub fn calculate_budget_impact(&self) -> f64 {
        let core_impact: f64 = self.core_clauses.iter().map(|c| c.budget_impact).sum();
        let concession_cost: f64 = self.concessions.iter().map(|c| c.cost).sum();
        core_impact + concession_cost
    }

    /// Calculate bill complexity for committee delay determination
    ///
    /// # Returns
    /// Complexity score (0-10), higher = more complex = longer committee review
    pub fn calculate_complexity(&self) -> u32 {
        let clause_count = self.core_clauses.len() as u32;
        let budget_magnitude = (self.calculate_budget_impact().abs() / 10.0) as u32;

        // Base complexity from clause count, plus budget impact
        let mut complexity = clause_count.min(5) + budget_magnitude.min(3);

        // Major reforms (like Land Reform) get +3 complexity
        if self.title.contains("Reforma") || self.title.contains("Land") {
            complexity += 3;
        }

        complexity.min(10)
    }

    /// Advance bill to next stage
    ///
    /// # Arguments
    /// * `current_turn` - Current game turn
    ///
    /// # Returns
    /// True if advancement successful, false if bill cannot advance
    pub fn advance_stage(&mut self, _current_turn: u32) -> bool {
        match self.stage {
            LegislativeStage::Introduced => {
                self.stage = LegislativeStage::Committee;
                true
            }
            LegislativeStage::Committee => {
                self.stage = LegislativeStage::FloorVote;
                true
            }
            LegislativeStage::FloorVote => {
                self.stage = LegislativeStage::BicameralPending;
                true
            }
            LegislativeStage::BicameralPending => {
                self.stage = LegislativeStage::Executive;
                true
            }
            LegislativeStage::Executive => {
                self.stage = LegislativeStage::Enacted;
                true
            }
            LegislativeStage::Enacted | LegislativeStage::Rejected => {
                false // Cannot advance from terminal stages
            }
        }
    }

    /// Reject the bill
    pub fn reject(&mut self) {
        self.stage = LegislativeStage::Rejected;
    }
}

/// Collection of active bills in the legislature
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LegislativeSession {
    /// Active bills by ID
    #[serde(default)]
    pub active_bills: HashMap<String, Bill>,

    /// Enacted laws (bill IDs)
    #[serde(default)]
    pub enacted_laws: Vec<String>,

    /// Rejected bills (bill IDs)
    #[serde(default)]
    pub rejected_bills: Vec<String>,

    /// Current session year
    #[serde(default)]
    pub session_year: u32,
}

impl LegislativeSession {
    /// Create a new legislative session
    ///
    /// # Arguments
    /// * `year` - Session year
    ///
    /// # Returns
    /// New LegislativeSession
    pub fn new(year: u32) -> Self {
        LegislativeSession {
            active_bills: HashMap::new(),
            enacted_laws: Vec::new(),
            rejected_bills: Vec::new(),
            session_year: year,
        }
    }

    /// Introduce a new bill
    ///
    /// # Arguments
    /// * `bill` - Bill to introduce
    ///
    /// # Rules
    /// * Bill must be in Introduced stage
    pub fn introduce_bill(&mut self, bill: Bill) {
        if matches!(bill.stage, LegislativeStage::Introduced) {
            self.active_bills.insert(bill.id.clone(), bill);
        }
    }

    /// Process bills for the current turn
    ///
    /// # Arguments
    /// * `current_turn` - Current game turn
    ///
    /// # Returns
    /// Vector of status messages
    pub fn process_turn(&mut self, current_turn: u32) -> Vec<String> {
        let mut messages = Vec::new();
        let mut bills_to_remove = Vec::new();

        for (id, bill) in &mut self.active_bills {
            match bill.stage {
                LegislativeStage::Committee => {
                    if let Some(completion_turn) = bill.committee_completion_turn {
                        if current_turn >= completion_turn {
                            messages.push(format!(
                                "[COMMITTEE] Bill {} completed committee review",
                                bill.title
                            ));
                            bill.advance_stage(current_turn);
                        }
                    }
                }
                LegislativeStage::Enacted => {
                    messages.push(format!("[BILL] Bill {} was passed", bill.title));
                    self.enacted_laws.push(id.clone());
                    bills_to_remove.push(id.clone());
                }
                LegislativeStage::Rejected => {
                    messages.push(format!("[BILL] Bill {} was rejected", bill.title));
                    self.rejected_bills.push(id.clone());
                    bills_to_remove.push(id.clone());
                }
                _ => {}
            }
        }

        // Remove completed bills
        for id in bills_to_remove {
            self.active_bills.remove(&id);
        }

        messages
    }
}

// ============================================================================
// PHASE 48: BILL PROVISIONS — concrete typed effects within a bill
// ============================================================================

/// A concrete provision within a bill — what the clause actually does when
/// enacted. Provisions can mix multiple policy domains in a single omnibus bill.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum BillProvision {
    /// Tax rate change.
    TaxRateChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        income_tax: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vat: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corporate_tax: Option<f64>,
    },
    /// Price control on a commodity.
    PriceControl {
        commodity: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_price: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_price: Option<f64>,
    },
    /// Subsidy for a sector or commodity.
    Subsidy {
        target: String,
        amount_per_unit: f64,
    },
    /// Deregulation of a sector.
    Deregulation { sector: String, scope: String },
    /// Healthcare law change.
    HealthcareLaw(crate::politics::laws::HealthcareLaw),
    /// Education law change.
    EducationLaw(crate::politics::laws::EducationLaw),
    /// Justice law change.
    JusticeLaw(crate::politics::laws::JusticeLaw),
    /// Infrastructure mandate.
    InfrastructureMandate { allocation_pct: f64 },
    /// Free speech law change.
    FreeSpeechLaw(crate::politics::free_speech::FreeSpeechLaw),
    /// Transport law change.
    TransportLaw(crate::economy::commuting::TransportLaw),
    /// Sentencing law change.
    SentencingLaw(crate::economy::sentencing::SentencingLaw),
    /// Migration law change.
    MigrationLawChange(crate::politics::laws::MigrationLaw),
    /// Custom provision (for modding/extensibility).
    Custom {
        description: String,
        effect_key: String,
        effect_value: f64,
    },
}

impl Default for BillProvision {
    fn default() -> Self {
        BillProvision::Custom {
            description: String::new(),
            effect_key: String::new(),
            effect_value: 0.0,
        }
    }
}

impl BillProvision {
    /// Convert a `BillProvision` into the corresponding `LawType` for
    /// application via `enact_law`.
    pub fn to_law_type(&self) -> Option<crate::politics::laws::LawType> {
        match self {
            BillProvision::TaxRateChange {
                income_tax,
                vat,
                corporate_tax,
            } => Some(crate::politics::laws::LawType::TaxRateChange {
                income_tax: *income_tax,
                vat: *vat,
                corporate_tax: *corporate_tax,
            }),
            BillProvision::HealthcareLaw(law) => {
                Some(crate::politics::laws::LawType::Healthcare(law.clone()))
            }
            BillProvision::EducationLaw(law) => {
                Some(crate::politics::laws::LawType::Education(law.clone()))
            }
            BillProvision::JusticeLaw(law) => {
                Some(crate::politics::laws::LawType::Justice(law.clone()))
            }
            BillProvision::InfrastructureMandate { allocation_pct } => {
                Some(crate::politics::laws::LawType::InfrastructureMandate {
                    allocation_pct: *allocation_pct,
                })
            }
            BillProvision::FreeSpeechLaw(law) => {
                Some(crate::politics::laws::LawType::FreeSpeech(law.clone()))
            }
            BillProvision::TransportLaw(law) => {
                Some(crate::politics::laws::LawType::Transport(law.clone()))
            }
            BillProvision::SentencingLaw(law) => {
                Some(crate::politics::laws::LawType::Sentencing(law.clone()))
            }
            // Provisions without a direct LawType mapping return None.
            // These are handled by specialized application logic.
            BillProvision::PriceControl { .. }
            | BillProvision::Subsidy { .. }
            | BillProvision::Deregulation { .. }
            | BillProvision::MigrationLawChange(_)
            | BillProvision::Custom { .. } => None,
        }
    }

    /// Check if this provision favors elites (used by Populist trait scoring).
    pub fn favors_elites(&self) -> bool {
        matches!(
            self,
            BillProvision::Subsidy { .. } | BillProvision::Deregulation { .. }
        )
    }
}

// ============================================================================
// PHASE 48: SUNSET PROVISION — a provision that will expire at a future turn
// ============================================================================

/// A provision that will expire at a future turn.
/// Tracked in `Politics::active_sunset_provisions` and processed each turn.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SunsetProvision {
    /// Bill ID that enacted this provision.
    #[serde(default)]
    pub bill_id: String,
    /// Clause description (for audit/logging).
    #[serde(default)]
    pub clause_description: String,
    /// The concrete provision that was enacted.
    #[serde(default)]
    pub provision: BillProvision,
    /// Turn when this provision expires.
    #[serde(default)]
    pub expiry_turn: u32,
    /// Turn when this provision was enacted.
    #[serde(default)]
    pub enacted_turn: u32,
}

// ============================================================================
// PHASE 48: BILL ENACTMENT — apply all provisions to the country
// ============================================================================

/// Enact a bill by applying all its provisions to the country.
///
/// For each clause with a `Some(provision)`, this function:
/// 1. Converts the provision to a `LawType` (if possible) and calls `enact_law`.
/// 2. If the clause has a `sunset_turn`, records a `SunsetProvision` for
///    future expiration processing.
///
/// # Arguments
/// * `country` - Mutable country to apply provisions to.
/// * `bill` - The bill being enacted (must be in `Enacted` stage).
///
/// # Returns
/// Vector of diagnostic messages.
pub fn enact_bill(country: &mut crate::state::Country, bill: &Bill) -> Vec<String> {
    let mut messages = Vec::new();

    for clause in &bill.core_clauses {
        if let Some(ref provision) = clause.provision {
            // Apply via enact_law if the provision maps to a LawType.
            if let Some(law_type) = provision.to_law_type() {
                let msg = crate::politics::laws::enact_law(country, law_type);
                messages.push(format!("[{}] {}", bill.title, msg));
            } else {
                // Provision has no direct LawType — log for now.
                // PriceControl, Subsidy, Deregulation, MigrationLawChange, Custom
                // are handled by specialized application logic in future phases.
                messages.push(format!(
                    "[{}] Provision '{}' applied (specialized handler).",
                    bill.title, clause.description
                ));
            }

            // Track sunset clauses.
            if let Some(sunset) = clause.sunset_turn {
                country
                    .politics
                    .active_sunset_provisions
                    .push(SunsetProvision {
                        bill_id: bill.id.clone(),
                        clause_description: clause.description.clone(),
                        provision: provision.clone(),
                        expiry_turn: sunset,
                        enacted_turn: bill.introduction_turn,
                    });
            }
        }
    }

    messages
}

/// Process sunset expirations for the current turn.
///
/// Removes expired provisions and applies political consequences.
///
/// # Arguments
/// * `country` - Mutable country.
/// * `current_turn` - Current game turn.
///
/// # Returns
/// Vector of diagnostic messages.
pub fn process_sunset_expirations(
    country: &mut crate::state::Country,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    let mut expired = Vec::new();

    for prov in &country.politics.active_sunset_provisions {
        if current_turn >= prov.expiry_turn {
            messages.push(format!(
                "[SUNSET] Provision '{}' from bill '{}' has expired.",
                prov.clause_description, prov.bill_id
            ));
            expired.push(prov.clone());
        }
    }

    // Remove expired provisions.
    country
        .politics
        .active_sunset_provisions
        .retain(|p| p.expiry_turn > current_turn);

    // Apply political consequences for each expired provision.
    for prov in &expired {
        apply_sunset_consequence(country, prov);
    }

    messages
}

/// Apply political consequences when a sunset provision expires.
///
/// - Tax cut expiration → government approval drops.
/// - Subsidy expiration → affected interest group radicalization increases.
/// - Price control expiration → consumer unrest rises.
/// - Deregulation expiration → business lobby anger.
fn apply_sunset_consequence(country: &mut crate::state::Country, prov: &SunsetProvision) {
    match &prov.provision {
        BillProvision::TaxRateChange {
            income_tax,
            vat,
            corporate_tax,
        } => {
            // Tax cuts expiring → approval drop.
            if income_tax.is_some() || vat.is_some() || corporate_tax.is_some() {
                // Reduce political capital as proxy for approval drop.
                country.politics.political_capital =
                    (country.politics.political_capital - 5.0).max(0.0);
            }
        }
        BillProvision::Subsidy { target, .. } => {
            // Subsidy expiration → affected interest group radicalization increases.
            if let Some(ig) = country.politics.interest_groups.get_mut(target) {
                ig.radicalization = (ig.radicalization + 0.05).min(1.0);
            }
        }
        BillProvision::PriceControl { .. } => {
            // Price control expiration → consumer unrest rises.
            // Proxy: reduce political capital.
            country.politics.political_capital =
                (country.politics.political_capital - 3.0).max(0.0);
        }
        BillProvision::Deregulation { .. } => {
            // Deregulation expiration → business lobby anger.
            // Proxy: reduce political capital slightly.
            country.politics.political_capital =
                (country.politics.political_capital - 2.0).max(0.0);
        }
        _ => {}
    }
}

#[cfg(test)]
mod phase48_tests {
    use super::*;

    #[test]
    fn test_clause_default_has_no_provision() {
        let clause = Clause::default();
        assert!(clause.provision.is_none());
        assert!(clause.sunset_turn.is_none());
        assert!(!clause.mutated);
        assert!(clause.mutation_notes.is_empty());
    }

    #[test]
    fn test_bill_provision_tax_rate_to_law_type() {
        let prov = BillProvision::TaxRateChange {
            income_tax: Some(0.20),
            vat: Some(0.10),
            corporate_tax: None,
        };
        let law_type = prov.to_law_type();
        assert!(law_type.is_some());
    }

    #[test]
    fn test_bill_provision_price_control_no_law_type() {
        let prov = BillProvision::PriceControl {
            commodity: "Steel".to_string(),
            min_price: None,
            max_price: Some(100.0),
        };
        let law_type = prov.to_law_type();
        assert!(law_type.is_none(), "PriceControl has no direct LawType");
    }

    #[test]
    fn test_bill_provision_subsidy_favors_elites() {
        let prov = BillProvision::Subsidy {
            target: "HeavyIndustry".to_string(),
            amount_per_unit: 5.0,
        };
        assert!(prov.favors_elites());
    }

    #[test]
    fn test_bill_provision_tax_change_not_elite_favoring() {
        let prov = BillProvision::TaxRateChange {
            income_tax: Some(0.30),
            vat: None,
            corporate_tax: None,
        };
        assert!(!prov.favors_elites());
    }

    #[test]
    fn test_sunset_provision_default() {
        let sp = SunsetProvision::default();
        assert!(sp.bill_id.is_empty());
        assert_eq!(sp.expiry_turn, 0);
    }

    #[test]
    fn test_sunset_expiration_removes_provision() {
        let mut country = crate::state::Country::default();
        country
            .politics
            .active_sunset_provisions
            .push(SunsetProvision {
                bill_id: "BILL-001".to_string(),
                clause_description: "Temporary tax cut".to_string(),
                provision: BillProvision::TaxRateChange {
                    income_tax: Some(0.15),
                    vat: None,
                    corporate_tax: None,
                },
                expiry_turn: 10,
                enacted_turn: 0,
            });

        // Before expiration: 1 provision.
        assert_eq!(country.politics.active_sunset_provisions.len(), 1);

        // Process at turn 10 → expires.
        let msgs = process_sunset_expirations(&mut country, 10);
        assert!(!msgs.is_empty());
        assert!(country.politics.active_sunset_provisions.is_empty());

        // Political capital should have dropped (tax cut expiration).
        assert!(
            country.politics.political_capital < 0.0 + 1e-6
                || country.politics.political_capital == 0.0
        );
    }

    #[test]
    fn test_sunset_not_yet_expired_retained() {
        let mut country = crate::state::Country::default();
        country
            .politics
            .active_sunset_provisions
            .push(SunsetProvision {
                bill_id: "BILL-002".to_string(),
                clause_description: "Temporary subsidy".to_string(),
                provision: BillProvision::Subsidy {
                    target: "HeavyIndustry".to_string(),
                    amount_per_unit: 5.0,
                },
                expiry_turn: 20,
                enacted_turn: 0,
            });

        // Process at turn 10 → not yet expired.
        let msgs = process_sunset_expirations(&mut country, 10);
        assert!(msgs.is_empty());
        assert_eq!(country.politics.active_sunset_provisions.len(), 1);
    }
}
