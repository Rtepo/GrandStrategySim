use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use crate::politics::interest_groups::{SuffrageSystem, InterestGroup, ClassToGroupMapping};
use crate::securities::BrokerageAccount;
use crate::state::banking::{Borrower, Loan};

/// Government form as stored in `polityka.ustrój`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum GovernmentForm {
    #[default]
    #[serde(rename = "Demokracja Parlamentarna")]
    ParliamentaryDemocracy,
    #[serde(rename = "Republika Prezydencka")]
    PresidentialRepublic,
    #[serde(rename = "Republika Półprezydencka")]
    SemiPresidentialRepublic,
    #[serde(rename = "Demokracja Dyrektorialna")]
    DirectorialDemocracy,
    #[serde(rename = "Monarchia Konstytucyjna")]
    ConstitutionalMonarchy,
    #[serde(rename = "Monarchia Dualistyczna")]
    DualistMonarchy,
    #[serde(rename = "Monarchia Elekcyjna")]
    ElectiveMonarchy,
    #[serde(rename = "Monarchia Absolutna")]
    AbsoluteMonarchy,
    #[serde(rename = "Państwo Jednopartyjne")]
    OnePartyState,
    #[serde(rename = "Dyktatura Wojskowa")]
    MilitaryDictatorship,
    #[serde(rename = "Teokracja")]
    Theocracy,
}

impl GovernmentForm {
    /// Returns `true` for the five democratic forms.
    pub fn is_democratic(self) -> bool {
        matches!(
            self,
            GovernmentForm::ParliamentaryDemocracy
                | GovernmentForm::PresidentialRepublic
                | GovernmentForm::SemiPresidentialRepublic
                | GovernmentForm::DirectorialDemocracy
                | GovernmentForm::ConstitutionalMonarchy
        )
    }

    /// Election cycle in years (999 for autocratic / non-elected).
    pub fn election_cycle(self) -> u32 {
        match self {
            GovernmentForm::ParliamentaryDemocracy
            | GovernmentForm::DirectorialDemocracy
            | GovernmentForm::ConstitutionalMonarchy => 4,
            GovernmentForm::PresidentialRepublic
            | GovernmentForm::SemiPresidentialRepublic
            | GovernmentForm::DualistMonarchy
            | GovernmentForm::ElectiveMonarchy => 5,
            GovernmentForm::AbsoluteMonarchy
            | GovernmentForm::OnePartyState
            | GovernmentForm::MilitaryDictatorship
            | GovernmentForm::Theocracy => 999,
        }
    }

    /// Number of legislative chambers the form has by default.
    pub fn chambers(self) -> u32 {
        match self {
            GovernmentForm::AbsoluteMonarchy | GovernmentForm::MilitaryDictatorship => 0,
            GovernmentForm::ElectiveMonarchy
            | GovernmentForm::OnePartyState
            | GovernmentForm::Theocracy => 1,
            _ => 2,
        }
    }
}

/// A single political leader.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Leader {
    #[serde(default, rename = "imie")]
    pub name: String,
    #[serde(default, rename = "plec")]
    pub gender: String,
    #[serde(default, rename = "wiek")]
    pub age: u32,
    #[serde(default, rename = "stan_zdrowia")]
    pub health: String,
    #[serde(default, rename = "dni_choroby")]
    pub days_sick: u32,
    #[serde(default, rename = "religia")]
    pub religion: String,
    #[serde(default, rename = "narodowosc")]
    pub nationality: String,
    #[serde(default, rename = "poglady")]
    pub views: String,
    #[serde(default, rename = "cechy")]
    pub traits: Vec<String>,
    #[serde(default, rename = "cecha")]
    pub main_trait: String,
    #[serde(default, rename = "dynastia", skip_serializing_if = "Option::is_none")]
    pub dynasty: Option<String>,
    #[serde(default, rename = "wplywy_bazowe")]
    pub base_influence: u32,
    #[serde(default, rename = "frakcja")]
    pub faction: String,
}

/// A political party.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Party {
    #[serde(default, rename = "ideologia")]
    pub ideology: String,
    #[serde(default, rename = "profil")]
    pub profile: String,
    #[serde(default, rename = "szkola_ekonomiczna")]
    pub economic_school: String,
    #[serde(default, rename = "poparcie")]
    pub support: f64,
    #[serde(default, rename = "lider")]
    pub leader: Leader,
    #[serde(default, rename = "baza")]
    pub base: Vec<String>,
    #[serde(default, rename = "id")]
    pub id: String,
    
    // NEW: Brokerage account for double-entry banking integration
    #[serde(rename = "rachunek_maklerski", default)]
    pub brokerage_account: Option<BrokerageAccount>,
    
    // NEW: Outstanding loans (vector of Loan objects from banking system)
    #[serde(rename = "kredyty", default)]
    pub loans: Vec<Loan>,
    
    // NEW: Internal organization
    #[serde(rename = "organizacja", default)]
    pub organization: PartyOrganization,
    
    // PHASE 3: Black money pool for corruption mechanics
    #[serde(rename = "czarne_pieniądze", default)]
    pub black_money_pool: Option<super::campaign::BlackMoneyPool>,
    
    // PHASE 3: Campaign spending tracked this cycle
    #[serde(rename = "wydatki_kampanijne", default)]
    pub campaign_spending: f64,
    
    // PHASE 4: Annual donations tracked (revenue tracker)
    #[serde(rename = "datki_roczne", default)]
    pub annual_donations: f64,
}

impl Borrower for Party {
    fn id(&self) -> &str {
        &self.id
    }
    
    fn liquid_capital(&self) -> f64 {
        // Parties use brokerage cash as working capital
        self.liquid_funds()
    }
    
    fn fixed_capital(&self) -> f64 {
        // Parties have no illiquid physical assets (real estate, machinery)
        // Returns 0.0 - parties qualify for Working Capital loans based on liquidity
        0.0
    }
    
    fn liabilities(&self) -> f64 {
        self.total_debt()
    }
    
    fn computed_liquid_capital(&self) -> f64 {
        self.liquid_funds()
    }
}

impl Party {
    /// Get current liquid funds from brokerage account
    pub fn liquid_funds(&self) -> f64 {
        self.brokerage_account.as_ref().map(|a| a.cash).unwrap_or(0.0)
    }
    
    /// Get total outstanding debt
    pub fn total_debt(&self) -> f64 {
        self.loans.iter().map(|l| l.outstanding_balance).sum()
    }
    
    /// Collect membership dues from interest group members
    /// 
    /// # Arguments
    /// * `party_support` - Party support percentage (0-100)
    /// * `base_interest_groups` - Interest groups backing this party
    /// * `interest_groups` - Interest groups with bifurcated power metrics
    /// * `companies` - Mutable reference to companies (for corporate dues)
    /// * `regions` - Mutable reference to regions (for demographic class cash reserves)
    ///
    /// # Returns
    /// Total dues collected (transactional transfer, not created)
    ///
    /// # Rules
    /// * Dues are transferred FROM company/demographic cash reserves TO party brokerage account
    /// * No money is created - this is a strict transfer
    /// * Wealthier interest groups pay higher dues per member
    /// * Demographic classes can only pay if they have sufficient savings
    /// * Company accounts accessed directly from Company objects (no global map lookup)
    pub fn collect_membership_dues(
        &mut self,
        party_support: f64,
        base_interest_groups: &[String],
        interest_groups: &HashMap<String, InterestGroup>,
        companies: &mut Vec<crate::entities::Company>,
        regions: &mut [crate::society::geography::Region],
    ) -> f64 {
        let mut total_collected = 0.0;
        
        // Ensure party has brokerage account
        if self.brokerage_account.is_none() {
            self.brokerage_account = Some(BrokerageAccount {
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
        
        let party_account = self.brokerage_account.as_mut().unwrap();
        
        // Collect dues from companies (Kapitaliści, Drobna Burżuazja)
        for group in base_interest_groups {
            if let Some(ig) = interest_groups.get(group) {
                let dues_per_entity = match group.as_str() {
                    "Kapitaliści" => 1000.0 * (ig.total_political_weight / 100.0) * (party_support / 100.0),
                    "Drobna Burżuazja" => 200.0 * (ig.total_political_weight / 100.0) * (party_support / 100.0),
                    _ => continue, // Skip non-corporate groups here
                };
                
                // Transfer from company operational cash (NOT brokerage account — that's for securities)
                for company in companies.iter_mut() {
                    if company.available_cash >= dues_per_entity {
                        company.available_cash -= dues_per_entity;
                        party_account.cash += dues_per_entity;
                        total_collected += dues_per_entity;
                    }
                }
            }
        }
        
        // Collect dues from demographic classes (stored in RegionalClassDemographics.savings)
        // Mutable access allows direct deduction from class savings
        for region in regions.iter_mut() {
            for (class_key, class_demographics) in region.class_demographics.rural_classes.iter_mut() {
                let dues_per_capita = match class_key.as_str() {
                    "Aristocracy" => 500.0 * (party_support / 100.0),
                    "FreePeasant" => 50.0 * (party_support / 100.0),
                    "LandlessLaborer" => 20.0 * (party_support / 100.0),
                    _ => continue,
                };
                
                let class_dues = dues_per_capita * class_demographics.population as f64;
                
                // Check if class has sufficient savings
                if class_demographics.savings >= class_dues {
                    // Transactional transfer: deduct from class savings
                    class_demographics.savings -= class_dues;
                    
                    // Update per-capita savings
                    if class_demographics.population > 0 {
                        class_demographics.savings_per_capita = class_demographics.savings / class_demographics.population as f64;
                    }
                    
                    // Credit to party brokerage account
                    party_account.cash += class_dues;
                    total_collected += class_dues;
                }
                // If insufficient savings, no dues collected (class cannot pay)
            }
        }
        
        total_collected
    }
    
    /// Accept donations from wealthy supporters
    /// 
    /// # Arguments
    /// * `companies` - Mutable reference to companies (for corporate donations)
    /// 
    /// # Returns
    /// Total donations collected (transactional transfer)
    /// 
    /// # Rules
    /// * Only parties with "Kapitaliści" base receive corporate donations
    /// * Donations are transferred FROM company brokerage accounts TO party brokerage account
    /// * Company accounts accessed directly from Company objects (no global map lookup)
    pub fn accept_donations(
        &mut self,
        companies: &mut Vec<crate::entities::Company>,
    ) -> f64 {
        if !self.base.contains(&"Kapitaliści".to_string()) {
            return 0.0;
        }
        
        let party_account = self.brokerage_account.as_mut().unwrap();
        let mut total_donations = 0.0;
        
        // Wealthy companies donate based on their operational cash
        for company in companies.iter_mut() {
            let donation_amount = company.available_cash * 0.01; // 1% of company operational cash
            
            if company.available_cash >= donation_amount && donation_amount > 100.0 {
                company.available_cash -= donation_amount;
                party_account.cash += donation_amount;
                total_donations += donation_amount;
            }
        }
        
        total_donations
    }
    
    /// Take loan from a bank (double-entry compliant)
    /// 
    /// # Arguments
    /// * `bank_balance_sheet` - Mutable reference to bank's balance sheet
    /// * `bank_id` - ID of the lending bank
    /// * `bank_margin` - Bank's margin over XIBOR
    /// * `principal` - Loan principal amount
    /// * `loan_type` - Type of loan (WorkingCapital, Investment, Consolidation)
    /// * `term_turns` - Loan term in turns
    /// * `central_bank` - Reference to central bank
    /// * `xibor` - Current XIBOR rate
    /// 
    /// # Returns
    /// Result with Loan object or error
    /// 
    /// # Rules
    /// * Creates a standard Loan object via banking system's issue_loan()
    /// * Party implements Borrower trait - no dummy wrapper needed
    /// * Increases bank's assets (loans_issued) and party's liabilities
    /// * Principal is credited to party's brokerage account
    /// * Strict double-entry: money is created by bank, not by party
    pub fn take_bank_loan(
        &mut self,
        bank_balance_sheet: &mut crate::state::banking::BankBalanceSheet,
        bank_id: &str,
        bank_margin: f64,
        principal: f64,
        loan_type: crate::state::banking::LoanType,
        term_turns: u32,
        central_bank: &crate::state::CentralBank,
        xibor: f64,
    ) -> Result<crate::state::banking::LoanResult, String> {
        // Direct call with self as Borrower - Party implements Borrower trait
        let loan_result = crate::state::banking::issue_loan(
            bank_balance_sheet,
            bank_id,
            bank_margin,
            self,  // Party implements Borrower - no dummy wrapper
            &self.id,
            principal,
            loan_type,
            term_turns,
            central_bank,
            xibor,
        )?;
        
        // Credit principal to party's brokerage account
        if let Some(ref mut party_account) = self.brokerage_account {
            party_account.cash += loan_result.principal_amount;
        }
        
        // Store loan in party's loan vector
        self.loans.push(loan_result.loan.clone());
        
        Ok(loan_result)
    }
    
    /// Make expenditure (transactional transfer)
    /// 
    /// # Arguments
    /// * `amount` - Amount to spend
    /// * `recipient_brokerage_account` - Mutable reference to recipient's brokerage account
    /// 
    /// # Returns
    /// Result indicating success or insufficient funds
    /// 
    /// # Rules
    /// * Money is transferred FROM party brokerage account TO recipient
    /// * No money is created or destroyed in this transfer
    /// * Recipient account passed directly to avoid global map lookups
    pub fn spend(
        &mut self,
        amount: f64,
        recipient_brokerage_account: &mut BrokerageAccount,
    ) -> Result<(), String> {
        let party_account = self.brokerage_account.as_mut()
            .ok_or("Party has no brokerage account")?;
        
        if party_account.cash < amount {
            return Err("Insufficient funds".to_string());
        }
        
        party_account.cash -= amount;
        recipient_brokerage_account.cash += amount;
        
        Ok(())
    }
}

/// Internal organizational structure of a political party
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationType {
    /// Hierarchical, top-down decision making (Marxist-Leninist parties)
    #[default]
    DemocraticCentralism,
    
    /// Elite vanguard leading the masses (radical revolutionary parties)
    Vanguard,
    
    /// Broad coalition of factions (centrist, catch-all parties)
    BigTent,
    
    /// Personality cult around the leader (authoritarian parties)
    LeaderCult,
    
    /// Decentralized, bottom-up decision making (anarchist, libertarian)
    Decentralized,
    
    /// Military or paramilitary structure (fascist, nationalist)
    Militarized,
}

impl OrganizationType {
    /// Base cohesion for this organization type
    pub fn base_cohesion(self) -> f64 {
        match self {
            OrganizationType::DemocraticCentralism => 0.8,
            OrganizationType::Vanguard => 0.9,
            OrganizationType::BigTent => 0.4,
            OrganizationType::LeaderCult => 0.7,
            OrganizationType::Decentralized => 0.3,
            OrganizationType::Militarized => 0.85,
        }
    }
    
    /// Base discipline for this organization type
    pub fn base_discipline(self) -> f64 {
        match self {
            OrganizationType::DemocraticCentralism => 0.85,
            OrganizationType::Vanguard => 0.95,
            OrganizationType::BigTent => 0.5,
            OrganizationType::LeaderCult => 0.9,
            OrganizationType::Decentralized => 0.2,
            OrganizationType::Militarized => 0.95,
        }
    }
    
    /// Default faction count for this organization type
    pub fn default_faction_count(self) -> u32 {
        match self {
            OrganizationType::DemocraticCentralism => 2,
            OrganizationType::Vanguard => 1,
            OrganizationType::BigTent => 4,
            OrganizationType::LeaderCult => 1,
            OrganizationType::Decentralized => 5,
            OrganizationType::Militarized => 2,
        }
    }
}

/// Internal party organization metrics
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PartyOrganization {
    /// Organizational structure type
    #[serde(rename = "typ_organizacji", default)]
    pub organization_type: OrganizationType,
    
    /// Party cohesion (0.0-1.0): How unified the party is internally
    #[serde(rename = "spójność", default)]
    pub cohesion: f64,
    
    /// Party discipline (0.0-1.0): How strictly party line is enforced
    #[serde(rename = "dyscyplina", default)]
    pub discipline: f64,
    
    /// Number of internal factions
    #[serde(rename = "frakcje", default)]
    pub faction_count: u32,
    
    /// Internal factional tension (0.0-1.0): Risk of split
    #[serde(rename = "napięcie_frakcyjne", default)]
    pub factional_tension: f64,
    
    /// Leadership stability (0.0-1.0): Risk of leadership challenge
    #[serde(rename = "stabilność_przywództwa", default)]
    pub leadership_stability: f64,
}

impl PartyOrganization {
    /// Initialize organization based on ideology with random variance
    pub fn from_ideology_with_variance(ideology: crate::politics::ideology::Ideology, rng: &mut impl rand::Rng) -> Self {
        let org_type = ideology.organization_with_variance(rng);
        PartyOrganization {
            organization_type: org_type,
            cohesion: org_type.base_cohesion(),
            discipline: org_type.base_discipline(),
            faction_count: org_type.default_faction_count(),
            factional_tension: 0.0,
            leadership_stability: 0.8,
        }
    }
    
    /// Update organization dynamics annually
    pub fn update_dynamics(&mut self, party_support: f64, party_liquid_funds: f64) {
        // Low support increases factional tension
        if party_support < 5.0 {
            self.factional_tension += 0.1;
        }
        
        // Empty treasury increases leadership instability
        if party_liquid_funds < 1000.0 {
            self.leadership_stability -= 0.15;
        }
        
        // High factional tension reduces cohesion
        if self.factional_tension > 0.7 {
            self.cohesion -= 0.1;
        }
        
        // Clamp values
        self.cohesion = self.cohesion.clamp(0.0, 1.0);
        self.discipline = self.discipline.clamp(0.0, 1.0);
        self.factional_tension = self.factional_tension.clamp(0.0, 1.0);
        self.leadership_stability = self.leadership_stability.clamp(0.0, 1.0);
    }
    
    /// Check for party split risk
    pub fn split_risk(&self) -> f64 {
        if self.factional_tension > 0.8 && self.cohesion < 0.3 {
            0.8  // High risk
        } else if self.factional_tension > 0.6 {
            0.4  // Moderate risk
        } else {
            0.0  // Low risk
        }
    }
}

/// Upper house / senate / house of lords described in a constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct UpperHouse {
    #[serde(default, rename = "nazwa")]
    pub name: String,
    #[serde(default, rename = "wybory")]
    pub elections: String,
    #[serde(default, rename = "uprawnienia")]
    pub powers: String,
}

/// Judiciary branch described in a constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Judiciary {
    #[serde(default, rename = "minister_sprawiedliwosci_i_prokurator_generalny")]
    pub minister_and_prosecutor: String,
    #[serde(default, rename = "wybor_sedziow")]
    pub judge_selection: String,
    #[serde(default, rename = "lawy_przysieglych")]
    pub jury_trials: bool,
    #[serde(default, rename = "sady_wojskowe")]
    pub military_courts: bool,
    #[serde(default, rename = "prawo_laski")]
    pub pardon: String,
    #[serde(default, rename = "specjalne_sady_administracyjne")]
    pub admin_courts: bool,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// A constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Constitution {
    #[serde(default, rename = "istnieje")]
    pub exists: bool,
    #[serde(default, rename = "trybunal_konstytucyjny")]
    pub constitutional_tribunal: String,
    #[serde(default, rename = "weto_prezydenckie")]
    pub presidential_veto: bool,
    #[serde(default, rename = "izba_wyzsza", skip_serializing_if = "Option::is_none")]
    pub upper_house: Option<UpperHouse>,
    #[serde(default, rename = "zmiana_konstytucji")]
    pub change_mechanism: String,
    #[serde(default, rename = "sadownictwo")]
    pub judiciary: Judiciary,
    #[serde(default, rename = "system_suffrage")]
    pub suffrage_system: SuffrageSystem,
    /// Phase 8: Consequence when budget bill fails.
    #[serde(default, rename = "skutek_kryzysu_budzetowego")]
    pub budget_failure_consequence: super::budget_lifecycle::BudgetFailureConsequence,
}

/// Justice system runtime state (Phase 14).
///
/// Tracks the national justice and security coverage, frozen company cash
/// A cohort of prisoners sharing the same demographic origin and sentence length.
/// Used for tracking time served and applying rehabilitation effects on release.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PrisonerCohort {
    /// Demographic class this cohort originated from (e.g., "robotnicy", "chłopi").
    #[serde(rename = "klasa_pochodzenia", default)]
    pub origin_class_id: String,
    /// Whether from urban (true) or rural (false) demographics.
    #[serde(rename = "miejskie", default)]
    pub origin_is_urban: bool,
    /// Region ID where they were arrested.
    #[serde(rename = "region_pochodzenia", default)]
    pub origin_region_id: String,
    /// Turns remaining until release.
    #[serde(rename = "pozostały_wyrok", default)]
    pub sentence_remaining: u32,
    /// Number of prisoners in this cohort.
    #[serde(rename = "liczba", default)]
    pub count: i64,
    /// Health status at intake (for rehabilitation comparison).
    #[serde(rename = "zdrowie_przyjęcie", default)]
    pub intake_health: crate::society::geography::HealthStatus,
    /// Prison type when sentenced (affects rehabilitation outcome).
    #[serde(rename = "typ_więzienia_wyrok", default)]
    pub sentenced_under: crate::politics::laws::PrisonType,
    /// Phase 18B: Crime severity category for this cohort.
    #[serde(default)]
    pub crime_category: crate::economy::sentencing::CrimeCategory,
    /// Phase 18B: Sentence outcome (imprisonment, death penalty, community service, etc.).
    #[serde(default)]
    pub sentence_outcome: crate::economy::sentencing::SentenceOutcome,
    /// Phase 18B: Legal status of the prisoner (for legal dualism).
    #[serde(default)]
    pub legal_status: crate::economy::legal_status::LegalStatus,
}

/// Security level assessment for a single prison building.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PrisonSecurityLevel {
    /// Building ID of the prison.
    #[serde(rename = "id_budynku", default)]
    pub building_id: String,
    /// Security score 0.0–1.0 (1.0 = maximum security).
    #[serde(rename = "poziom_bezpieczeństwa", default)]
    pub security_score: f64,
    /// Number of guards (fulfilled FTE at this building).
    #[serde(rename = "obsada_strażników", default)]
    pub guard_fte: f64,
    /// Target guard FTE (worker_capacity).
    #[serde(rename = "docelowa_obsada", default)]
    pub target_guard_fte: f64,
    /// Building condition (0.0–1.0).
    #[serde(rename = "stan_budynku", default)]
    pub condition: f64,
}

/// Intelligence state tracking domestic surveillance and counterintelligence.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct IntelligenceState {
    /// Total intelligence capacity produced this turn.
    #[serde(rename = "pojemność_wywiadu", default)]
    pub total_capacity: f64,
    /// Dissidents tracked (0.0–1.0 fraction of radical population).
    #[serde(rename = "pokrycie_inwigilacji", default)]
    pub surveillance_coverage: f64,
    /// Active counterintelligence operations.
    #[serde(rename = "aktywne_operacje", default)]
    pub active_operations: u32,
    /// Infiltration level of mass movements (0.0–1.0).
    #[serde(rename = "infiltracja_ruchów", default)]
    pub movement_infiltration: f64,
    /// Phase 18C: Number of terrorist attacks prevented this turn.
    #[serde(rename = "odparte_zamachy", default)]
    pub attacks_prevented: u32,
    /// Phase 18C: Number of terrorist attacks that succeeded this turn.
    #[serde(rename = "udane_zamachy", default)]
    pub attacks_succeeded: u32,
}

/// from unresolved court disputes, and prison labor statistics.
/// Updated each turn by `process_justice_turn` and `process_prison_labor_turn`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct JusticeSystemState {
    /// Total justice capacity produced by courthouses this turn.
    #[serde(rename = "pojemność_sprawiedliwości", default)]
    pub total_justice_capacity: f64,
    /// Total security capacity produced by police stations this turn.
    #[serde(rename = "pojemność_bezpieczeństwa", default)]
    pub total_security_capacity: f64,
    /// Dynamic justice demand (scales with poverty, unemployment, unrest, health).
    #[serde(rename = "popyt_sprawiedliwości", default)]
    pub justice_demand: f64,
    /// Dynamic security demand (scales with poverty, unemployment, unrest, health).
    #[serde(rename = "popyt_bezpieczeństwa", default)]
    pub security_demand: f64,
    /// Justice coverage ratio (capacity / demand, 0.0–1.0+).
    #[serde(rename = "pokrycie_sprawiedliwości", default)]
    pub justice_coverage: f64,
    /// Security coverage ratio (capacity / demand, 0.0–1.0+).
    #[serde(rename = "pokrycie_bezpieczeństwa", default)]
    pub security_coverage: f64,
    /// Frozen company cash from unresolved court disputes.
    /// Maps company ID → frozen amount. Reclaimed on bankruptcy.
    #[serde(rename = "zamrożona_gotówka", default)]
    pub frozen_company_cash: HashMap<String, f64>,
    /// Total active prisoners across all prison buildings.
    #[serde(rename = "aktywni_więźniowie", default)]
    pub active_prisoners: i64,
    /// People held in isolation camps (removed from workforce).
    #[serde(rename = "odosobnieni", default)]
    pub isolated_population: i64,
    /// FTEs injected into the labor market from private labor camps.
    #[serde(rename = "fte_więźniów", default)]
    pub prison_labor_allocated_fte: f64,
    /// Phase 14.5: Prisoner cohorts with sentence tracking.
    #[serde(rename = "kohorty_więźniów", default)]
    pub prisoner_cohorts: Vec<PrisonerCohort>,
    /// Phase 14.5: Per-prison security level assessments.
    #[serde(rename = "poziomy_bezpieczeństwa_więzień", default)]
    pub prison_security_levels: Vec<PrisonSecurityLevel>,
    /// Phase 14.5: Total fines collected this turn.
    #[serde(rename = "zebrane_kary", default)]
    pub fines_collected: f64,
}

/// The whole political subsystem for one country.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Politics {
    #[serde(default, rename = "ustrój")]
    pub government_form: GovernmentForm,
    #[serde(default, rename = "konstytucja")]
    pub constitution: Constitution,
    #[serde(default, rename = "partia_rządząca")]
    pub ruling_party: String,
    #[serde(default, rename = "koalicja")]
    pub coalition: Vec<String>,
    #[serde(default, rename = "id_koalicji")]
    pub coalition_id: String,
    #[serde(default, rename = "rzad_mniejszosciowy")]
    pub minority_government: bool,
    #[serde(default, rename = "lata_do_wyborów")]
    pub years_to_elections: u32,
    #[serde(default, rename = "aktywne_partie")]
    pub active_parties: HashMap<String, Party>,
    #[serde(default, rename = "parlament")]
    pub parliament: HashMap<String, u32>,
    #[serde(default, rename = "sklad_izba_wyzsza")]
    pub upper_house: HashMap<String, u32>,
    #[serde(default, rename = "rada_koronna")]
    pub royal_council: Map<String, Value>,
    #[serde(default, rename = "lojalnosc_rady")]
    pub council_loyalty: Map<String, Value>,
    #[serde(default, rename = "poglady_monarchy")]
    pub monarchy_views: String,
    #[serde(default, rename = "dynastia", skip_serializing_if = "Option::is_none")]
    pub dynasty: Option<String>,
    #[serde(default, rename = "glowa_panstwa")]
    pub head_of_state: Leader,
    #[serde(default, rename = "rodzina_krolewska")]
    pub royal_family: Map<String, Value>,
    #[serde(default, rename = "krolowa_matka", skip_serializing_if = "Option::is_none")]
    pub queen_mother: Option<Value>,
    #[serde(default, rename = "elita_wladzy")]
    pub power_elite: Vec<Value>,
    #[serde(default, rename = "nastepca_tronu", skip_serializing_if = "Option::is_none")]
    pub heir: Option<Value>,
    #[serde(default, rename = "regencja")]
    pub regency: bool,
    #[serde(default, rename = "regent", skip_serializing_if = "Option::is_none")]
    pub regent: Option<Value>,
    #[serde(default, rename = "grupy_interesów")]
    pub interest_groups: HashMap<String, InterestGroup>,
    #[serde(default, rename = "mapowanie_klas")]
    pub class_group_mapping: ClassToGroupMapping,
    #[serde(default, rename = "ruchy_masowe")]
    pub mass_movements: Vec<crate::politics::mass_movements::MassMovement>,
    // PHASE 3: Election campaign state machine
    #[serde(rename = "stan_kampanii", default)]
    pub election_state: super::campaign::ElectionState,
    // PHASE 3: Electoral Commission (PKW)
    #[serde(rename = "komisja_wyborcza", default)]
    pub electoral_commission: super::campaign::ElectoralCommission,
    // PHASE 3: Campaign duration in turns
    #[serde(rename = "długość_kampanii", default)]
    pub campaign_duration_turns: u32,
    // PHASE 3: Executed campaign actions this cycle
    #[serde(rename = "wykonane_akcje", default)]
    pub campaign_executions: Vec<super::campaign::CampaignExecution>,
    // PHASE 4: Lobbying groups (institutional intermediaries)
    #[serde(rename = "grupy_lobbistyczne", default)]
    pub lobbying_groups: Vec<super::lobbying::LobbyingGroup>,
    // PHASE 4: Special economic zones
    #[serde(rename = "strefy_ekonomiczne", default)]
    pub special_economic_zones: Vec<crate::state::SpecialEconomicZone>,
    #[serde(default, rename = "historia_wladzy")]
    pub history: Vec<Value>,
    #[serde(default, rename = "prog_wyborczy")]
    pub election_threshold: f64,
    #[serde(default, rename = "ordynacja_wyborcza")]
    pub election_method: String,
    #[serde(default, rename = "twarda_reka")]
    pub iron_fist: u32,
    #[serde(default, rename = "kryzys_budzetowy")]
    pub budget_crisis: bool,
    #[serde(default, rename = "szkola_ekonomiczna_rzadu")]
    pub government_economic_school: String,
    #[serde(default, rename = "doktryna_handlowa")]
    pub trade_doctrine: String,
    #[serde(default, rename = "prawo_wyznaniowe")]
    pub religious_law: String,
    #[serde(default, rename = "polityka_migracyjna")]
    pub migration_policy: String,
    #[serde(default, rename = "prawo_obywatelskie")]
    pub civil_rights_law: String,
    #[serde(default, rename = "prawo_emancypacji")]
    pub emancipation_law: String,
    #[serde(default, rename = "prawo_pracy")]
    pub labor_law: String,
    #[serde(default, rename = "prawo_zwiazkowe")]
    pub union_law: String,
    #[serde(default, rename = "prawo_strajkowe")]
    pub strike_law: String,
    #[serde(default, rename = "sluzba_zdrowia")]
    pub health_service: String,
    #[serde(default, rename = "sanepid")]
    pub sanitation_policy: String,
    #[serde(default, rename = "model_edukacji")]
    pub education_model: String,
    #[serde(default, rename = "ustroj_szkolny")]
    pub school_system: String,
    /// Healthcare law configuration (new capacity-based model)
    #[serde(rename = "prawo_zdrowotne", default)]
    pub healthcare_law: Option<crate::politics::laws::HealthcareLaw>,
    /// Education law configuration (new capacity-based model)
    #[serde(rename = "prawo_edukacyjne", default)]
    pub education_law: Option<crate::politics::laws::EducationLaw>,
    /// Justice law configuration (Phase 14).
    #[serde(rename = "prawo_sprawiedliwości", default)]
    pub justice_law: Option<crate::politics::laws::JusticeLaw>,
    /// Prison labor law configuration (Phase 14).
    #[serde(rename = "prawo_więzienne", default)]
    pub prison_labor_law: Option<crate::politics::laws::PrisonLaborLaw>,
    /// Justice system runtime state (Phase 14).
    #[serde(rename = "stan_sprawiedliwości", default)]
    pub justice_state: Option<crate::politics::system::JusticeSystemState>,
    /// Phase 14.5: Domestic intelligence state for surveillance and repression.
    #[serde(rename = "stan_wywiadu", default)]
    pub intelligence_state: Option<crate::politics::system::IntelligenceState>,
    /// Espionage state for covert operations
    #[serde(rename = "stan_szpiegowski", default)]
    pub espionage_state: Option<crate::politics::espionage::EspionageState>,
    /// Legislative session for bill processing
    #[serde(rename = "sesja_legislacyjna", default)]
    pub legislative_session: Option<crate::politics::legislation::LegislativeSession>,
    /// Committee system for bill review
    #[serde(rename = "system_komisji", default)]
    pub committee_system: Option<crate::politics::committees::CommitteeSystem>,
    #[serde(default, rename = "agencja_pracy_aktywnej")]
    pub active_labour_agency: bool,
    #[serde(default, rename = "ustawa_jadrowa")]
    pub nuclear_law: bool,
    #[serde(default, rename = "tarcza_energetyczna")]
    pub energy_shield: bool,
    #[serde(default, rename = "knf")]
    pub knf: Value,
    /// Phase 8: Ministry configuration (government portfolios).
    #[serde(rename = "rząd_ministrów", default)]
    pub ministry_config: Option<super::ministries::MinistryConfig>,
    /// Phase 15B: Migration law configuration.
    #[serde(rename = "prawo_migracyjne", default)]
    pub migration_law: Option<crate::politics::laws::MigrationLaw>,
    /// Phase 15B: Border enforcement runtime state.
    #[serde(rename = "stan_graniczny", default)]
    pub border_state: Option<crate::politics::laws::BorderState>,
    /// Phase 15B: Customs runtime state.
    #[serde(rename = "stan_celny", default)]
    pub customs_state: Option<crate::politics::laws::CustomsState>,
    /// Phase 15C: Inspectorate runtime state.
    #[serde(rename = "stan_inspekcji", default)]
    pub inspectorate_state: Option<crate::politics::laws::InspectorateState>,
    /// Phase 17C: Structured religious law configuration.
    #[serde(rename = "ustawa_religijna", default)]
    pub religious_law_struct: Option<crate::politics::laws::ReligiousLaw>,
    /// Phase 18A: Shadow economy runtime state.
    #[serde(rename = "stan_gospodarki_cieniowej", default)]
    pub shadow_economy_state: Option<crate::economy::legal_status::ShadowEconomyState>,
    /// Phase 18A: Amnesty / legalization program configuration.
    #[serde(rename = "ustawa_amnestia", default)]
    pub amnesty_law: Option<crate::economy::legal_status::AmnestyLaw>,
    /// Phase 18B: Sentencing law configuration (dynamic sentencing, legal dualism).
    #[serde(rename = "ustawa_wyrokowanie", default)]
    pub sentencing_law: Option<crate::economy::sentencing::SentencingLaw>,
    /// Phase 18B: Administrative court state (blocks illegal state actions).
    #[serde(rename = "sąd_administracyjny", default)]
    pub administrative_court: Option<crate::economy::sentencing::AdministrativeCourtState>,
    /// Phase 18B: Ombudsman (RPO) state (monitors rights violations).
    #[serde(rename = "rzecznik_praw", default)]
    pub ombudsman: Option<crate::economy::sentencing::OmbudsmanState>,
    /// Phase 18C: Media state (tracks information production and state media share).
    #[serde(rename = "stan Mediów", default)]
    pub media_state: Option<crate::economy::propaganda::MediaState>,
    /// Phase 18C: Propaganda campaign configuration.
    #[serde(rename = "ustawa_propaganda", default)]
    pub propaganda_config: Option<crate::economy::propaganda::PropagandaConfig>,
    /// Phase 18C: Free speech / assembly / press freedom law.
    #[serde(rename = "ustawa_wolność_słowa", default)]
    pub free_speech_law: Option<crate::politics::free_speech::FreeSpeechLaw>,
    /// Phase 23C: Transport ownership / subsidy law — affects passenger
    /// transport pricing and commuter affordability.
    #[serde(rename = "ustawa_transport", default)]
    pub transport_law: Option<crate::economy::commuting::TransportLaw>,
    /// Phase 32: Structured Parliament (chambers, clubs, VIPs).
    /// When None, the engine falls back to the legacy flat `parliament` HashMap.
    #[serde(rename = "parlament_struktura", default)]
    pub parliament_struct: Option<crate::politics::parliament::Parliament>,
    /// Phase 32: Constitutional State of Emergency (political, not fiscal).
    /// Distinct from the fiscal `EmergencyPowers` enum on `Country`.
    #[serde(rename = "stan_wyjatkowy", default)]
    pub state_of_emergency: Option<crate::politics::parliament::StateOfEmergency>,
    /// Phase 32: Political capital — spent by the ruling coalition on pork-barrel
    /// offers and agenda control. Regenerated each turn based on ruling party
    /// support and coalition stability.
    #[serde(rename = "kapital_polityczny", default)]
    pub political_capital: f64,
    /// Phase 39: Last turn a snap election was triggered. Used for cooldown
    /// to prevent infinite election loops when election formation fails.
    #[serde(default, rename = "ostatnie_wyborows_snap")]
    pub last_snap_election_turn: u32,
    /// Phase 48: Global VIP registry — tracks all power holders with age,
    /// health, incapacity, traits, and death. See `politics/vip_registry.rs`.
    #[serde(default, rename = "rejestr_vip")]
    pub vip_registry: Option<crate::politics::vip_registry::VipRegistry>,
    /// Phase 48: Active sunset provisions — enacted bill provisions that will
    /// expire at a future turn. See `politics/legislation.rs`.
    #[serde(default, rename = "postanowienia_z_terminem")]
    pub active_sunset_provisions: Vec<crate::politics::legislation::SunsetProvision>,
    /// Phase 48: Active unfunded mandates imposed by the central government
    /// on regional JSTs. See `politics/local_legislation.rs`.
    #[serde(default, rename = "aktywne_mandaty")]
    pub active_mandates: Vec<crate::politics::local_legislation::UnfundedMandate>,
    /// Phase 48: Advisory council for authoritarian/royal regimes.
    /// See `politics/advisory_council.rs`.
    #[serde(default, rename = "rada_doradcza")]
    pub advisory_council: Option<crate::politics::advisory_council::AdvisoryCouncil>,
    /// Phase 48: Royal dynasty tracking for monarchies.
    /// See `politics/succession.rs`.
    #[serde(default, rename = "dynastia_krolewska")]
    pub royal_dynasty: Option<crate::politics::succession::RoyalDynasty>,
    /// Phase 65: State structure (Unitary/Federation/Totalitarian/AutonomousRepublic).
    /// Controls tax retention rates and regional law authority.
    #[serde(default, rename = "ustrój_państwa")]
    pub state_structure: super::state_structure::StateStructure,
    /// Phase 65: State structure configuration with tax retention rates.
    #[serde(default, rename = "konfiguracja_ustróju")]
    pub state_structure_config: super::state_structure::StateStructureConfig,
    /// Phase 65: Regional laws enacted by Federation/AutonomousRepublic regions.
    #[serde(default, rename = "prawa_regionalne")]
    pub regional_laws: Vec<super::state_structure::RegionalLaw>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Politics {
    /// Migrate legacy string fields to new enum-based structures
    pub fn migrate_legacy_fields(&mut self) {
        // String fields are kept for backward compatibility
        // New enum fields are optional and can be initialized from strings when needed
    }
}

/// Fiscal transfer configuration (from national Tax & Administrative Law)
/// 
/// # CRITICAL: Transfer Mathematics Must Sum to 100%
/// The three shares (local_retention + megaregion_share + central_share) must
/// sum to 1.0 (100%). This is enforced at configuration time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FiscalTransferConfig {
    /// Percentage of regional revenue retained locally
    #[serde(rename = "udzial_lokalny", default)]
    pub local_retention: f64, // 0.0-1.0
    
    /// Percentage transferred to Megaregion (if applicable)
    #[serde(rename = "udzial_megaregionu", default)]
    pub megaregion_share: f64, // 0.0-1.0
    
    /// Percentage transferred to Central Budget
    #[serde(rename = "udzial_centralny", default)]
    pub central_share: f64, // 0.0-1.0
    
    /// Minimum local retention (cannot go below this)
    #[serde(rename = "minimum_lokalne", default)]
    pub minimum_local_retention: f64,
}

impl FiscalTransferConfig {
    /// Validate that shares sum to 100%
    pub fn validate(&self) -> bool {
        let total = self.local_retention + self.megaregion_share + self.central_share;
        (total - 1.0).abs() < 0.001
    }
    
    /// Calculate upward transfers from regional revenue
    /// 
    /// # Arguments
    /// * `regional_revenue` - Total regional tax revenue
    /// * `has_megaregion` - Whether region belongs to a Megaregion
    /// 
    /// # Returns
    /// (local_retained, megaregion_transfer, central_transfer)
    /// 
    /// # CRITICAL: No Double Dipping
    /// Region splits revenue exactly once according to config.
    /// Megaregion keeps 100% of its transfer - no second upward transfer.
    pub fn calculate_transfers(
        &self,
        regional_revenue: f64,
        has_megaregion: bool,
    ) -> (f64, f64, f64) {
        let local_retained = regional_revenue * self.local_retention.max(self.minimum_local_retention);
        
        if has_megaregion {
            let megaregion_transfer = regional_revenue * self.megaregion_share;
            let central_transfer = regional_revenue * self.central_share;
            (local_retained, megaregion_transfer, central_transfer)
        } else {
            // Skip Megaregion layer - flow directly to Central
            let central_transfer = regional_revenue * (self.megaregion_share + self.central_share);
            (local_retained, 0.0, central_transfer)
        }
    }
}
