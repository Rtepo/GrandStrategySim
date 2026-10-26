use crate::politics::ideology::IdeologyCoordinates;
use crate::politics::interest_groups::{ClassToGroupMapping, InterestGroup, SuffrageSystem};
use crate::securities::BrokerageAccount;
use crate::state::banking::{Borrower, Loan};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Government form as stored in `politics.system`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum GovernmentForm {
    #[default]
    ParliamentaryDemocracy,

    PresidentialRepublic,

    SemiPresidentialRepublic,

    DirectorialDemocracy,

    ConstitutionalMonarchy,

    DualistMonarchy,

    ElectiveMonarchy,

    AbsoluteMonarchy,

    OnePartyState,

    MilitaryDictatorship,

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

    /// v4.20 T4: Human-readable regime display name (replaces `{:?}` output
    /// in UI DTO strings).
    pub fn display_name(self) -> &'static str {
        match self {
            GovernmentForm::ParliamentaryDemocracy => "Parliamentary Democracy",
            GovernmentForm::PresidentialRepublic => "Presidential Republic",
            GovernmentForm::SemiPresidentialRepublic => "Semi-Presidential Republic",
            GovernmentForm::DirectorialDemocracy => "Directorial Democracy",
            GovernmentForm::ConstitutionalMonarchy => "Constitutional Monarchy",
            GovernmentForm::DualistMonarchy => "Dualist Monarchy",
            GovernmentForm::ElectiveMonarchy => "Elective Monarchy",
            GovernmentForm::AbsoluteMonarchy => "Absolute Monarchy",
            GovernmentForm::OnePartyState => "One-Party State",
            GovernmentForm::MilitaryDictatorship => "Military Dictatorship",
            GovernmentForm::Theocracy => "Theocracy",
        }
    }

    /// v4.20 T3: Regime type key for the UI (`ParliamentSnapshot.regime_type`,
    /// `ConstitutionSnapshot.regime_type`). The frontend normalizes this key
    /// (lowercase, no separators) to decide whether the Elections tab and
    /// elective institutions exist for the regime.
    pub fn regime_type_key(self) -> &'static str {
        match self {
            GovernmentForm::ParliamentaryDemocracy
            | GovernmentForm::PresidentialRepublic
            | GovernmentForm::SemiPresidentialRepublic
            | GovernmentForm::DirectorialDemocracy
            | GovernmentForm::ConstitutionalMonarchy => "Democracy",
            GovernmentForm::DualistMonarchy => "DualistMonarchy",
            GovernmentForm::ElectiveMonarchy => "ElectiveMonarchy",
            GovernmentForm::AbsoluteMonarchy => "AbsoluteMonarchy",
            GovernmentForm::OnePartyState => "OnePartyState",
            GovernmentForm::MilitaryDictatorship => "MilitaryDictatorship",
            GovernmentForm::Theocracy => "Theocracy",
        }
    }

    /// v4.20 T3: Regime-appropriate display name for the legislature
    /// (`ParliamentSnapshot.legislature_name`).
    pub fn legislature_name(self) -> &'static str {
        match self {
            GovernmentForm::ParliamentaryDemocracy
            | GovernmentForm::PresidentialRepublic
            | GovernmentForm::SemiPresidentialRepublic
            | GovernmentForm::DirectorialDemocracy
            | GovernmentForm::ConstitutionalMonarchy => "Parliament",
            GovernmentForm::DualistMonarchy => "Imperial Diet",
            GovernmentForm::ElectiveMonarchy => "Elective Sejm",
            GovernmentForm::AbsoluteMonarchy => "Royal Council",
            GovernmentForm::OnePartyState => "People's Assembly",
            GovernmentForm::MilitaryDictatorship => "National Salvation Junta",
            GovernmentForm::Theocracy => "Supreme Religious Council",
        }
    }
}

/// v4.20 T4: Convert a Rust `Debug`-style enum token ("WealthWeightedVoting",
/// "snap_election") into a readable UI label ("Wealth Weighted Voting",
/// "Snap Election"). Used wherever a DTO previously leaked `format!("{:?}")`.
pub fn humanize_debug_token(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 8);
    let mut prev_lower_or_digit = false;
    for (i, ch) in token.chars().enumerate() {
        if ch == '_' || ch == '-' {
            out.push(' ');
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_uppercase() && prev_lower_or_digit {
            out.push(' ');
        }
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
        prev_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
    }
    out
}

/// v4.20 T2: Deterministic party visual-identity palette — 32 distinct hues.
/// `Party.color_hex` is assigned once at genesis/first sight and persisted,
/// so identical seeds produce identical hemicycle/election colors.
pub const PARTY_COLOR_PALETTE: &[&str] = &[
    "#b91c1c", "#dc2626", "#ea580c", "#d97706", "#ca8a04", "#a16207", "#65a30d",
    "#16a34a", "#059669", "#0d9488", "#0891b2", "#0284c7", "#2563eb", "#4f46e5",
    "#7c3aed", "#9333ea", "#c026d3", "#db2777", "#e11d48", "#78716c", "#57534e",
    "#92400e", "#713f12", "#3f6212", "#166534", "#134e4a", "#155e75", "#1e40af",
    "#5b21b6", "#86198f", "#9f1239", "#44403c",
];

/// v4.20 T2: Stable hash of a party id used to index `PARTY_COLOR_PALETTE`.
/// FNV-1a — deterministic across processes (no `RandomState`).
fn stable_party_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// v4.20 T2: Map an ideology display label / coordinates onto the icon key
/// vocabulary the frontend resolves (`conservative`, `liberal`, `socialist`,
/// `communist`, `green`, `nationalist`, `religious`, `monarchist`, `fascist`,
/// `military`, `agrarian`, `libertarian`, `anarchist`, `technocrat`,
/// `populist`, `centrist`, `reactionary`).
pub fn ideology_icon_key(ideology_label: &str, coords: &IdeologyCoordinates) -> String {
    let l = ideology_label.to_lowercase();
    let key = if l.contains("marxism") || l.contains("maoism") || l.contains("communist") {
        "communist"
    } else if l.contains("green") || l.contains("ecolog") {
        "green"
    } else if l.contains("agrarian") || l.contains("peasant") {
        "agrarian"
    } else if l.contains("christian") || l.contains("religious") || l.contains("theocrat") {
        "religious"
    } else if l.contains("fascis") || l.contains("nazis") {
        "fascist"
    } else if l.contains("monarch") || l.contains("royal") {
        "monarchist"
    } else if l.contains("militar") || l.contains("junta") {
        "military"
    } else if l.contains("national") || l.contains("patriot") {
        "nationalist"
    } else if l.contains("anarcho-capital") || l.contains("libertari") {
        "libertarian"
    } else if l.contains("anarch") {
        "anarchist"
    } else if l.contains("techno") {
        "technocrat"
    } else if l.contains("populi") {
        "populist"
    } else if l.contains("reaction") {
        "reactionary"
    } else if l.contains("conserv") || l.contains("traditional") {
        "conservative"
    } else if l.contains("social democr") || l.contains("socialist") || l.contains("labor") {
        "socialist"
    } else if l.contains("liberal") || l.contains("free") {
        "liberal"
    } else if l.contains("centri") || l.contains("moderate") {
        "centrist"
    } else {
        // Coordinate fallback when the label has no recognizable keyword.
        if coords.tradition > 0.5 {
            "conservative"
        } else if coords.economy < -0.4 {
            "socialist"
        } else if coords.liberty > 0.4 {
            "liberal"
        } else if coords.liberty < -0.6 {
            "fascist"
        } else {
            "centrist"
        }
    };
    key.to_string()
}

/// v4.20 T2: Fill `color_hex`/`ideology_icon` on one party if unset.
/// `used_colors` collects already-assigned hex values so sibling parties in
/// the same map never share a hemicycle color. Call order must be canonical
/// (sorted party ids) so the assignment is deterministic.
fn fill_party_identity(party_id: &str, party: &mut Party, used_colors: &mut std::collections::HashSet<String>) {
    if party.ideology_icon.is_empty() {
        party.ideology_icon = ideology_icon_key(&party.ideology, &party.coordinates);
    }
    if party.color_hex.is_empty() {
        let n = PARTY_COLOR_PALETTE.len();
        let start = (stable_party_hash(party_id) as usize) % n;
        let mut idx = start;
        while used_colors.contains(PARTY_COLOR_PALETTE[idx]) {
            idx = (idx + 1) % n;
        }
        let color = PARTY_COLOR_PALETTE[idx].to_string();
        used_colors.insert(color.clone());
        party.color_hex = color;
    } else {
        used_colors.insert(party.color_hex.clone());
    }
}

/// v4.20 T2: Deterministically assign `color_hex`/`ideology_icon` to every
/// active party that lacks them. Iterates sorted party ids — the fill order
/// never depends on HashMap iteration. Idempotent: persisted values are kept.
/// Called from `process_election_cycle` (every turn) and genesis, so parties
/// created by any path (bootstrap, fallback injection, splinter formalization)
/// acquire identity exactly once.
pub fn ensure_all_party_identities(parties: &mut HashMap<String, Party>) {
    let mut ids: Vec<String> = parties.keys().cloned().collect();
    ids.sort();
    let mut used_colors: std::collections::HashSet<String> = std::collections::HashSet::new();
    for id in ids {
        if let Some(party) = parties.get_mut(&id) {
            fill_party_identity(&id.clone(), party, &mut used_colors);
        }
    }
}

/// v4.20 T1: Renormalize party supports so they sum to exactly 100.0.
///
/// Support is stored on the percent scale (0-100 vote-share semantics —
/// `elections.rs` compares it against `election_threshold`). Campaign
/// decays and event multipliers (`party.support *= 0.9`, `= 0.0`) shrink
/// the aggregate below 100 over time; the invariant that the electorate
/// partitions fully across parties must be restored every turn or the
/// v4.20 release gate `(supports_sum - 100).abs() < 1.0` drifts red.
///
/// Proportional rescale preserves relative shares; zero-support parties
/// stay at zero; an all-zero map is left untouched (no parties active).
/// Iteration is over a sorted id vec for determinism.
pub fn normalize_party_supports(parties: &mut HashMap<String, Party>) {
    // `active_parties` is a std HashMap — its iteration order is per-process
    // random. The `total` f64 sum MUST iterate in a canonical (sorted) order:
    // floating-point addition is non-associative, so an unsorted sum yields a
    // last-ulp-different `scale` per process, which writes back into support
    // and drifts trajectories over 120 turns (same class as the v4.19
    // `total_forex_reserves` fix).
    let mut ids: Vec<String> = parties.keys().cloned().collect();
    ids.sort();
    let total: f64 = ids
        .iter()
        .map(|id| parties[id].support.max(0.0))
        .sum();
    if total <= f64::EPSILON || (total - 100.0).abs() < 1e-9 {
        return;
    }
    let scale = 100.0 / total;
    for id in ids {
        if let Some(p) = parties.get_mut(&id) {
            p.support = (p.support.max(0.0) * scale).clamp(0.0, 100.0);
        }
    }
}

/// v4.20 T2: Look up a club's visual identity — the parent party's persisted
/// color/icon when it exists, otherwise a deterministic derivation from the
/// club id and ideology (splinter clubs have no parent party entry).
pub fn club_visual_identity(
    club_id: &str,
    ideology_label: &str,
    coords: &IdeologyCoordinates,
    parties: &HashMap<String, Party>,
) -> (String, String) {
    if let Some(p) = parties.get(club_id) {
        let color = if p.color_hex.is_empty() {
            PARTY_COLOR_PALETTE[(stable_party_hash(club_id) as usize) % PARTY_COLOR_PALETTE.len()]
                .to_string()
        } else {
            p.color_hex.clone()
        };
        let icon = if p.ideology_icon.is_empty() {
            ideology_icon_key(&p.ideology, &p.coordinates)
        } else {
            p.ideology_icon.clone()
        };
        (color, icon)
    } else {
        (
            PARTY_COLOR_PALETTE
                [(stable_party_hash(club_id) as usize) % PARTY_COLOR_PALETTE.len()]
            .to_string(),
            ideology_icon_key(ideology_label, coords),
        )
    }
}

/// A single political leader.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Leader {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub gender: String,
    #[serde(default)]
    pub age: u32,
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub days_sick: u32,
    #[serde(default)]
    pub religion: String,
    #[serde(default)]
    pub nationality: String,
    #[serde(default)]
    pub views: String,
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub main_trait: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynasty: Option<String>,
    #[serde(default)]
    pub base_influence: u32,
    #[serde(default)]
    pub faction: String,
}

/// A political party.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Party {
    #[serde(default)]
    pub ideology: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub economic_school: String,
    #[serde(default)]
    pub support: f64,
    #[serde(default)]
    pub leader: Leader,
    #[serde(default)]
    pub base: Vec<String>,
    #[serde(default)]
    pub id: String,

    /// Authoritative ideological position on three continuous axes.
    /// The `ideology` string above is a derived display label from `classify()`.
    #[serde(default)]
    pub coordinates: IdeologyCoordinates,

    // NEW: Brokerage account for double-entry banking integration
    #[serde(default)]
    pub brokerage_account: Option<BrokerageAccount>,

    // NEW: Outstanding loans (vector of Loan objects from banking system)
    #[serde(default)]
    pub loans: Vec<Loan>,

    // NEW: Internal organization
    #[serde(default)]
    pub organization: PartyOrganization,

    // PHASE 3: Black money pool for corruption mechanics
    #[serde(default)]
    pub black_money_pool: Option<super::campaign::BlackMoneyPool>,

    // PHASE 3: Campaign spending tracked this cycle
    #[serde(default)]
    pub campaign_spending: f64,

    // PHASE 4: Annual donations tracked (revenue tracker)
    #[serde(default)]
    pub annual_donations: f64,

    // v4.20 T2: Persistent party visual identity — hex color shared across
    // the Elections poll table, the hemicycle, and the parliamentary club
    // rows. Assigned once by `ensure_all_party_identities`, never re-rolled.
    #[serde(default)]
    pub color_hex: String,

    // v4.20 T2: Stable ideology badge key resolved to an icon by the
    // frontend (e.g. "conservative", "socialist", "green").
    #[serde(default)]
    pub ideology_icon: String,
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
        self.brokerage_account
            .as_ref()
            .map(|a| a.cash)
            .unwrap_or(0.0)
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
                tax_advantaged_account: None,
extra: std::collections::HashMap::new(),
            });
        }

        let party_account = self.brokerage_account.as_mut().unwrap();

        // Collect dues from companies (Capitalists, Petty Bourgeoisie)
        for group in base_interest_groups {
            if let Some(ig) = interest_groups.get(group) {
                let dues_per_entity = match group.as_str() {
                    "Capitalists" => {
                        1000.0 * (ig.total_political_weight / 100.0) * (party_support / 100.0)
                    }
                    "Petty Bourgeoisie" => {
                        200.0 * (ig.total_political_weight / 100.0) * (party_support / 100.0)
                    }
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
            for (class_key, class_demographics) in
                region.class_demographics.rural_classes.iter_mut()
            {
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
                        class_demographics.savings_per_capita =
                            class_demographics.savings / class_demographics.population as f64;
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
    /// * Only parties with "Capitalists" base receive corporate donations
    /// * Donations are transferred FROM company brokerage accounts TO party brokerage account
    /// * Company accounts accessed directly from Company objects (no global map lookup)
    pub fn accept_donations(&mut self, companies: &mut Vec<crate::entities::Company>) -> f64 {
        if !self.base.contains(&"Capitalists".to_string()) {
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
        // Direct call with self as Borrower - Party implements Borrower trait.
        // The party's brokerage account is its claim at THIS bank — the
        // minted deposit lands on the lender's book, so pass Some(bank_id).
        let loan_result = crate::state::banking::issue_loan(
            bank_balance_sheet,
            bank_id,
            bank_margin,
            self, // Party implements Borrower - no dummy wrapper
            &self.id,
            Some(bank_id),
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
        let party_account = self
            .brokerage_account
            .as_mut()
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
    #[serde(default)]
    pub organization_type: OrganizationType,

    /// Party cohesion (0.0-1.0): How unified the party is internally
    #[serde(default)]
    pub cohesion: f64,

    /// Party discipline (0.0-1.0): How strictly party line is enforced
    #[serde(default)]
    pub discipline: f64,

    /// Number of internal factions
    #[serde(default)]
    pub faction_count: u32,

    /// Internal factional tension (0.0-1.0): Risk of split
    #[serde(default)]
    pub factional_tension: f64,

    /// Leadership stability (0.0-1.0): Risk of leadership challenge
    #[serde(default)]
    pub leadership_stability: f64,
}

impl PartyOrganization {
    /// Initialize organization based on ideological coordinates with random variance
    pub fn from_coordinates_with_variance(
        coords: IdeologyCoordinates,
        rng: &mut impl rand::Rng,
    ) -> Self {
        let org_type = crate::politics::ideology::organization_from_coords(coords);
        let _ = rng; // Variance applied inside organization_from_coords in future
        PartyOrganization {
            organization_type: org_type,
            cohesion: org_type.base_cohesion(),
            discipline: org_type.base_discipline(),
            faction_count: org_type.default_faction_count(),
            factional_tension: 0.0,
            leadership_stability: 0.8,
        }
    }

    /// Initialize organization based on ideology enum with random variance.
    /// Legacy method retained for backward compatibility during migration.
    pub fn from_ideology_with_variance(
        ideology: crate::politics::ideology::Ideology,
        rng: &mut impl rand::Rng,
    ) -> Self {
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
            0.8 // High risk
        } else if self.factional_tension > 0.6 {
            0.4 // Moderate risk
        } else {
            0.0 // Low risk
        }
    }
}

/// Upper house / senate / house of lords described in a constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct UpperHouse {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub elections: String,
    #[serde(default)]
    pub powers: String,
}

/// Judiciary branch described in a constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Judiciary {
    #[serde(default)]
    pub minister_and_prosecutor: String,
    #[serde(default)]
    pub judge_selection: String,
    #[serde(default)]
    pub jury_trials: bool,
    #[serde(default)]
    pub military_courts: bool,
    #[serde(default)]
    pub pardon: String,
    #[serde(default)]
    pub admin_courts: bool,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// A constitution.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Constitution {
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub constitutional_tribunal: String,
    #[serde(default)]
    pub presidential_veto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper_house: Option<UpperHouse>,
    #[serde(default)]
    pub change_mechanism: String,
    #[serde(default)]
    pub judiciary: Judiciary,
    #[serde(default, rename = "system_suffrage")]
    pub suffrage_system: SuffrageSystem,
    /// Phase 8: Consequence when budget bill fails.
    #[serde(default)]
    pub budget_failure_consequence: super::budget_lifecycle::BudgetFailureConsequence,
}

/// Justice system runtime state (Phase 14).
///
/// Tracks the national justice and security coverage, frozen company cash
/// A cohort of prisoners sharing the same demographic origin and sentence length.
/// Used for tracking time served and applying rehabilitation effects on release.
/// E.6.3: `origin_class` is now typed `DemographicClass` (replaces `origin_class_id: String`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PrisonerCohort {
    /// E.6.3: Typed demographic class this cohort originated from.
    #[serde(default)]
    pub origin_class: crate::society::geography::DemographicClass,
    /// Whether from urban (true) or rural (false) demographics.
    /// (Redundant with `origin_class.is_urban()` but kept for explicit serialization.)
    #[serde(default)]
    pub origin_is_urban: bool,
    /// Region ID where they were arrested.
    #[serde(default)]
    pub origin_region_id: String,
    /// Turns remaining until release.
    #[serde(default)]
    pub sentence_remaining: u32,
    /// Number of prisoners in this cohort.
    #[serde(default)]
    pub count: i64,
    /// Health status at intake (for rehabilitation comparison).
    #[serde(default)]
    pub intake_health: crate::society::geography::HealthStatus,
    /// Prison type when sentenced (affects rehabilitation outcome).
    #[serde(default)]
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
    #[serde(default)]
    pub building_id: String,
    /// Security score 0.0–1.0 (1.0 = maximum security).
    #[serde(default)]
    pub security_score: f64,
    /// Number of guards (fulfilled FTE at this building).
    #[serde(default)]
    pub guard_fte: f64,
    /// Target guard FTE (worker_capacity).
    #[serde(default)]
    pub target_guard_fte: f64,
    /// Building condition (0.0–1.0).
    #[serde(default)]
    pub condition: f64,
}

/// Intelligence state tracking domestic surveillance and counterintelligence.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct IntelligenceState {
    /// Total intelligence capacity produced this turn.
    #[serde(default)]
    pub total_capacity: f64,
    /// Dissidents tracked (0.0–1.0 fraction of radical population).
    #[serde(default)]
    pub surveillance_coverage: f64,
    /// Active counterintelligence operations.
    #[serde(default)]
    pub active_operations: u32,
    /// Infiltration level of mass movements (0.0–1.0).
    #[serde(default)]
    pub movement_infiltration: f64,
    /// Phase 18C: Number of terrorist attacks prevented this turn.
    #[serde(default)]
    pub attacks_prevented: u32,
    /// Phase 18C: Number of terrorist attacks that succeeded this turn.
    #[serde(default)]
    pub attacks_succeeded: u32,
}

/// from unresolved court disputes, and prison labor statistics.
/// Updated each turn by `process_justice_turn` and `process_prison_labor_turn`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct JusticeSystemState {
    /// Total justice capacity produced by courthouses this turn.
    #[serde(default)]
    pub total_justice_capacity: f64,
    /// Total security capacity produced by police stations this turn.
    #[serde(default)]
    pub total_security_capacity: f64,
    /// Dynamic justice demand (scales with poverty, unemployment, unrest, health).
    #[serde(default)]
    pub justice_demand: f64,
    /// Dynamic security demand (scales with poverty, unemployment, unrest, health).
    #[serde(default)]
    pub security_demand: f64,
    /// Justice coverage ratio (capacity / demand, 0.0–1.0+).
    #[serde(default)]
    pub justice_coverage: f64,
    /// Security coverage ratio (capacity / demand, 0.0–1.0+).
    #[serde(default)]
    pub security_coverage: f64,
    /// Frozen company cash from unresolved court disputes.
    /// Maps company ID → frozen amount. Reclaimed on bankruptcy.
    #[serde(default)]
    pub frozen_company_cash: HashMap<String, f64>,
    /// Total active prisoners across all prison buildings.
    #[serde(default)]
    pub active_prisoners: i64,
    /// People held in isolation camps (removed from workforce).
    #[serde(default)]
    pub isolated_population: i64,
    /// FTEs injected into the labor market from private labor camps.
    #[serde(default)]
    pub prison_labor_allocated_fte: f64,
    /// Phase 14.5: Prisoner cohorts with sentence tracking.
    #[serde(default)]
    pub prisoner_cohorts: Vec<PrisonerCohort>,
    /// Phase 14.5: Per-prison security level assessments.
    #[serde(default)]
    pub prison_security_levels: Vec<PrisonSecurityLevel>,
    /// Phase 14.5: Total fines collected this turn.
    #[serde(default)]
    pub fines_collected: f64,
}

/// The whole political subsystem for one country.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Politics {
    #[serde(default)]
    pub government_form: GovernmentForm,
    #[serde(default)]
    pub constitution: Constitution,
    #[serde(default)]
    pub ruling_party: String,
    #[serde(default)]
    pub coalition: Vec<String>,
    #[serde(default)]
    pub coalition_id: String,
    #[serde(default)]
    pub minority_government: bool,
    #[serde(default)]
    pub years_to_elections: u32,
    /// v4.20 T1: Absolute turn index on which the next scheduled election
    /// executes. Derived from `years_to_elections` (24 turns/year) and kept
    /// in sync with it; snap/crisis elections re-anchor this via
    /// `process_election_cycle`'s per-turn resync. 0 = not yet scheduled
    /// (pre-init) / non-elective regime.
    #[serde(default)]
    pub scheduled_election_turn: u32,
    /// v4.20 T1: Live countdown — `scheduled_election_turn - current_turn`,
    /// recomputed every political turn. Reaches 0 on the turn the election
    /// fires. Replaces the static `years_to_elections * 24` estimate for UI
    /// countdowns.
    #[serde(default)]
    pub turns_until_election: u32,
    /// v4.20: Total elections executed (scheduled + snap) — release gate.
    #[serde(default)]
    pub elections_held: u32,
    #[serde(default)]
    pub active_parties: HashMap<String, Party>,
    #[serde(default)]
    pub parliament: HashMap<String, u32>,
    #[serde(default)]
    pub upper_house: HashMap<String, u32>,
    #[serde(default)]
    pub royal_council: Map<String, Value>,
    #[serde(default)]
    pub council_loyalty: Map<String, Value>,
    #[serde(default)]
    pub monarchy_views: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynasty: Option<String>,
    #[serde(default)]
    pub head_of_state: Leader,
    #[serde(default)]
    pub royal_family: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queen_mother: Option<Value>,
    #[serde(default)]
    pub power_elite: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heir: Option<Value>,
    #[serde(default)]
    pub regency: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regent: Option<Value>,
    #[serde(default)]
    pub interest_groups: HashMap<String, InterestGroup>,
    #[serde(default)]
    pub class_group_mapping: ClassToGroupMapping,
    #[serde(default)]
    pub mass_movements: Vec<crate::politics::mass_movements::MassMovement>,
    // PHASE 3: Election campaign state machine
    #[serde(default)]
    pub election_state: super::campaign::ElectionState,
    // PHASE 3: Electoral Commission (PKW)
    #[serde(default)]
    pub electoral_commission: super::campaign::ElectoralCommission,
    // PHASE 3: Campaign duration in turns
    #[serde(default)]
    pub campaign_duration_turns: u32,
    // PHASE 3: Executed campaign actions this cycle
    #[serde(default)]
    pub campaign_executions: Vec<super::campaign::CampaignExecution>,
    // PHASE 4: Lobbying groups (institutional intermediaries)
    #[serde(default)]
    pub lobbying_groups: Vec<super::lobbying::LobbyingGroup>,
    // PHASE 4: Special economic zones
    #[serde(default)]
    pub special_economic_zones: Vec<crate::state::SpecialEconomicZone>,
    #[serde(default)]
    pub history: Vec<Value>,
    #[serde(default)]
    pub election_threshold: f64,
    #[serde(default)]
    pub election_method: String,
    #[serde(default)]
    pub iron_fist: u32,
    #[serde(default)]
    pub budget_crisis: bool,
    #[serde(default)]
    pub government_economic_school: String,
    #[serde(default)]
    pub trade_doctrine: String,
    #[serde(default)]
    pub religious_law: String,
    #[serde(default)]
    pub migration_policy: String,
    #[serde(default)]
    pub civil_rights_law: String,
    #[serde(default)]
    pub emancipation_law: String,
    #[serde(default)]
    pub labor_law: String,
    /// Phase D5/E.4: Child labor law.
    #[serde(default)]
    pub child_labor_law: Option<crate::politics::laws::ChildLaborLaw>,
    #[serde(default)]
    pub union_law: String,
    #[serde(default)]
    pub strike_law: String,
    #[serde(default)]
    pub health_service: String,
    #[serde(default)]
    pub sanitation_policy: String,
    #[serde(default)]
    pub education_model: String,
    #[serde(default)]
    pub school_system: String,
    /// Healthcare law configuration (new capacity-based model)
    #[serde(default)]
    pub healthcare_law: Option<crate::politics::laws::HealthcareLaw>,
    /// Education law configuration (new capacity-based model)
    #[serde(default)]
    pub education_law: Option<crate::politics::laws::EducationLaw>,
    /// Phase E.9.2: Previous school system.
    #[serde(default)]
    pub previous_school_system: Option<crate::politics::laws::SchoolSystem>,
    /// Justice law configuration (Phase 14).
    #[serde(default)]
    pub justice_law: Option<crate::politics::laws::JusticeLaw>,
    /// Prison labor law configuration (Phase 14).
    #[serde(default)]
    pub prison_labor_law: Option<crate::politics::laws::PrisonLaborLaw>,
    /// Justice system runtime state (Phase 14).
    #[serde(default)]
    pub justice_state: Option<crate::politics::system::JusticeSystemState>,
    /// Phase 14.5: Domestic intelligence state for surveillance and repression.
    #[serde(default)]
    pub intelligence_state: Option<crate::politics::system::IntelligenceState>,
    /// Espionage state for covert operations
    #[serde(default)]
    pub espionage_state: Option<crate::politics::espionage::EspionageState>,
    /// Legislative session for bill processing
    #[serde(default)]
    pub legislative_session: Option<crate::politics::legislation::LegislativeSession>,
    /// Committee system for bill review
    #[serde(default)]
    pub committee_system: Option<crate::politics::committees::CommitteeSystem>,
    #[serde(default)]
    pub active_labour_agency: bool,
    #[serde(default)]
    pub nuclear_law: bool,
    #[serde(default)]
    pub energy_shield: bool,
    #[serde(default)]
    pub knf: Value,
    /// Phase 8: Ministry configuration (government portfolios).
    #[serde(default)]
    pub ministry_config: Option<super::ministries::MinistryConfig>,
    /// Phase 15B: Migration law configuration.
    #[serde(default)]
    pub migration_law: Option<crate::politics::laws::MigrationLaw>,
    /// Phase 15B: Border enforcement runtime state.
    #[serde(default)]
    pub border_state: Option<crate::politics::laws::BorderState>,
    /// Phase 15B: Customs runtime state.
    #[serde(default)]
    pub customs_state: Option<crate::politics::laws::CustomsState>,
    /// Phase 15C: Inspectorate runtime state.
    #[serde(default)]
    pub inspectorate_state: Option<crate::politics::laws::InspectorateState>,
    /// Phase 17C: Structured religious law configuration.
    #[serde(default)]
    pub religious_law_struct: Option<crate::politics::laws::ReligiousLaw>,
    /// Phase 18A: Shadow economy runtime state.
    #[serde(default)]
    pub shadow_economy_state: Option<crate::economy::legal_status::ShadowEconomyState>,
    /// Phase 18A: Amnesty / legalization program configuration.
    #[serde(default)]
    pub amnesty_law: Option<crate::economy::legal_status::AmnestyLaw>,
    /// Phase 18B: Sentencing law configuration (dynamic sentencing, legal dualism).
    #[serde(default)]
    pub sentencing_law: Option<crate::economy::sentencing::SentencingLaw>,
    /// Phase 18B: Administrative court state (blocks illegal state actions).
    #[serde(default)]
    pub administrative_court: Option<crate::economy::sentencing::AdministrativeCourtState>,
    /// Phase 18B: Ombudsman (Ombudsman) state (monitors rights violations).
    #[serde(default)]
    pub ombudsman: Option<crate::economy::sentencing::OmbudsmanState>,
    /// Phase 18C: Media state (tracks information production and state media share).
    #[serde(default)]
    pub media_state: Option<crate::economy::propaganda::MediaState>,
    /// Phase 18C: Propaganda campaign configuration.
    #[serde(default)]
    pub propaganda_config: Option<crate::economy::propaganda::PropagandaConfig>,
    /// Phase 18C: Free speech / assembly / press freedom law.
    #[serde(default)]
    pub free_speech_law: Option<crate::politics::free_speech::FreeSpeechLaw>,
    /// Phase 23C: Transport ownership / subsidy law — affects passenger
    /// transport pricing and commuter affordability.
    #[serde(default)]
    pub transport_law: Option<crate::economy::commuting::TransportLaw>,
    /// Phase 32: Structured Parliament (chambers, clubs, VIPs).
    /// When None, the engine falls back to the legacy flat `parliament` HashMap.
    #[serde(default)]
    pub parliament_struct: Option<crate::politics::parliament::Parliament>,
    /// Phase 32: Constitutional State of Emergency (political, not fiscal).
    /// Distinct from the fiscal `EmergencyPowers` enum on `Country`.
    #[serde(default)]
    pub state_of_emergency: Option<crate::politics::parliament::StateOfEmergency>,
    /// Phase 32: Political capital — spent by the ruling coalition on pork-barrel
    /// offers and agenda control. Regenerated each turn based on ruling party
    /// support and coalition stability.
    #[serde(default)]
    pub political_capital: f64,
    /// Phase 39: Last turn a snap election was triggered. Used for cooldown
    /// to prevent infinite election loops when election formation fails.
    #[serde(default)]
    pub last_snap_election_turn: u32,
    /// Phase 48: Global VIP registry — tracks all power holders with age,
    /// health, incapacity, traits, and death. See `politics/vip_registry.rs`.
    #[serde(default)]
    pub vip_registry: Option<crate::politics::vip_registry::VipRegistry>,
    /// Phase 48: Active sunset provisions — enacted bill provisions that will
    /// expire at a future turn. See `politics/legislation.rs`.
    #[serde(default)]
    pub active_sunset_provisions: Vec<crate::politics::legislation::SunsetProvision>,
    /// Phase 48: Active unfunded mandates imposed by the central government
    /// on regional JSTs. See `politics/local_legislation.rs`.
    #[serde(default)]
    pub active_mandates: Vec<crate::politics::local_legislation::UnfundedMandate>,
    /// Phase 48: Advisory council for authoritarian/royal regimes.
    /// See `politics/advisory_council.rs`.
    #[serde(default)]
    pub advisory_council: Option<crate::politics::advisory_council::AdvisoryCouncil>,
    /// Phase 48: Royal dynasty tracking for monarchies.
    /// See `politics/succession.rs`.
    #[serde(default)]
    pub royal_dynasty: Option<crate::politics::succession::RoyalDynasty>,
    /// Phase 65: State structure (Unitary/Federation/Totalitarian/AutonomousRepublic).
    /// Controls tax retention rates and regional law authority.
    #[serde(default)]
    pub state_structure: super::state_structure::StateStructure,
    /// Phase 65: State structure configuration with tax retention rates.
    #[serde(default)]
    pub state_structure_config: super::state_structure::StateStructureConfig,
    /// Phase 65: Regional laws enacted by Federation/AutonomousRepublic regions.
    #[serde(default)]
    pub regional_laws: Vec<super::state_structure::RegionalLaw>,
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Politics {}

/// Fiscal transfer configuration (from national Tax & Administrative Law)
///
/// # CRITICAL: Transfer Mathematics Must Sum to 100%
/// The three shares (local_retention + megaregion_share + central_share) must
/// sum to 1.0 (100%). This is enforced at configuration time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FiscalTransferConfig {
    /// Percentage of regional revenue retained locally
    #[serde(default)]
    pub local_retention: f64, // 0.0-1.0

    /// Percentage transferred to Megaregion (if applicable)
    #[serde(default)]
    pub megaregion_share: f64, // 0.0-1.0

    /// Percentage transferred to Central Budget
    #[serde(default)]
    pub central_share: f64, // 0.0-1.0

    /// Minimum local retention (cannot go below this)
    #[serde(default)]
    pub minimum_local_retention: f64,
}

impl FiscalTransferConfig {
    /// Validate that shares sum to 100%
    pub fn validate(&self) -> bool {
        let total = self.local_retention + self.megaregion_share + self.central_share;
        (total - 1.0).abs() < 0.001
    }

    /// Phase D.1: Construct a `FiscalTransferConfig` from the country's
    /// `StateStructure` and `StateStructureConfig`.
    ///
    /// Maps the (central, region, microregion) tax shares from
    /// `StateStructureConfig` into the (local_retention, megaregion_share,
    /// central_share) format used by `process_fiscal_transfers`.
    ///
    /// The microregion share is folded into the megaregion share (microregions
    /// are sub-regional domains within a region; their share is retained
    /// locally and administered through the regional/megaregion budget).
    ///
    /// `minimum_local_retention` is set to half the region share, ensuring
    /// regions never lose more than 50% of their configured retention to
    /// upward transfers even under edge-case rounding.
    pub fn from_state_structure(
        structure: super::state_structure::StateStructure,
        config: &super::state_structure::StateStructureConfig,
    ) -> Self {
        let (central, region, micro) = config.shares_for(structure);
        let local_retention = region + micro; // Microregion share stays local
        let megaregion_share = 0.0; // Megaregion share is separate from state structure
        let central_share = central;
        let minimum_local_retention = local_retention * 0.5;

        FiscalTransferConfig {
            local_retention,
            megaregion_share,
            central_share,
            minimum_local_retention,
        }
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
        let local_retained =
            regional_revenue * self.local_retention.max(self.minimum_local_retention);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::state_structure::{StateStructure, StateStructureConfig};

    #[test]
    fn test_fiscal_transfer_config_from_unitary() {
        let config = StateStructureConfig::default();
        let ftc = FiscalTransferConfig::from_state_structure(StateStructure::Unitary, &config);
        // Unitary: central=0.80, region=0.15, micro=0.05
        // local_retention = 0.15 + 0.05 = 0.20
        assert!((ftc.local_retention - 0.20).abs() < 1e-9);
        assert!((ftc.central_share - 0.80).abs() < 1e-9);
        assert!((ftc.megaregion_share - 0.0).abs() < 1e-9);
        assert!((ftc.minimum_local_retention - 0.10).abs() < 1e-9);
        assert!(ftc.validate());
    }

    #[test]
    fn test_fiscal_transfer_config_from_federation() {
        let config = StateStructureConfig::default();
        let ftc = FiscalTransferConfig::from_state_structure(StateStructure::Federation, &config);
        // Federation: central=0.35, region=0.55, micro=0.10
        // local_retention = 0.55 + 0.10 = 0.65
        assert!((ftc.local_retention - 0.65).abs() < 1e-9);
        assert!((ftc.central_share - 0.35).abs() < 1e-9);
        assert!(ftc.validate());
    }

    #[test]
    fn test_fiscal_transfer_config_from_totalitarian() {
        let config = StateStructureConfig::default();
        let ftc = FiscalTransferConfig::from_state_structure(StateStructure::Totalitarian, &config);
        // Totalitarian: central=1.0, region=0.0, micro=0.0
        assert!((ftc.local_retention - 0.0).abs() < 1e-9);
        assert!((ftc.central_share - 1.0).abs() < 1e-9);
        assert!(ftc.validate());
    }

    #[test]
    fn test_fiscal_transfer_config_from_autonomous_republic() {
        let config = StateStructureConfig::default();
        let ftc =
            FiscalTransferConfig::from_state_structure(StateStructure::AutonomousRepublic, &config);
        // Autonomous: central=0.25, region=0.65, micro=0.10
        // local_retention = 0.65 + 0.10 = 0.75
        assert!((ftc.local_retention - 0.75).abs() < 1e-9);
        assert!((ftc.central_share - 0.25).abs() < 1e-9);
        assert!(ftc.validate());
    }

    #[test]
    fn test_fiscal_transfer_calculations_unitary() {
        let config = StateStructureConfig::default();
        let ftc = FiscalTransferConfig::from_state_structure(StateStructure::Unitary, &config);
        let (local, mega, central) = ftc.calculate_transfers(1000.0, false);
        // No megaregion: central gets megaregion_share + central_share = 0 + 0.80 = 0.80
        assert!((local - 200.0).abs() < 1e-6);
        assert!((mega - 0.0).abs() < 1e-6);
        assert!((central - 800.0).abs() < 1e-6);
    }

    #[test]
    fn test_fiscal_transfer_calculations_federation() {
        let config = StateStructureConfig::default();
        let ftc = FiscalTransferConfig::from_state_structure(StateStructure::Federation, &config);
        let (local, _mega, central) = ftc.calculate_transfers(1000.0, false);
        assert!((local - 650.0).abs() < 1e-6);
        assert!((central - 350.0).abs() < 1e-6);
    }
}

