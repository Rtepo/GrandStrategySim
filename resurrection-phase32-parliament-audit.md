# Phase 32 — Parliament, Elections, Legislation & Executive Overhaul

**Audit & Technical Blueprint — READ-ONLY**
**Date:** 2026-08-14
**Status:** Blueprint UPDATED with all 4 user-mandated architectural corrections. Awaiting user approval. No Rust code to be written until approved.

---

## 0a. User-Mandated Architectural Corrections (4 total)

The following four strict corrections have been applied to this blueprint. They override
any conflicting language in the original draft.

### Correction 1: Pork-Barrel Accounting — No Ghost Wallets

**Rule:** Regions do not have magical "wallets". Every unit of currency must belong to a
Company, a Citizen Class, a Bank, or the Treasury. Pork-barrel spending must NOT credit
abstract regional counters.

**Implementation:** Pork-barrel (vote buying) is executed via one of two **real economic
hooks**:

1. **`MinistrySpendingAction::Subsidy`** — Targeted cash transfer to the
   `available_cash` (or `brokerage_account.cash`) of Companies located in the target
   club's stronghold regions. The Treasury is debited via
   `country.budget.liquid_reserves -= amount`, and the company is credited via
   `settle_treasury_to_company()` (existing function in `transfer_settler.rs`, line 394).
   This follows the exact pattern already used by agricultural subsidies in
   `ministries.rs` (line 707–716).

2. **`ConstructionTender`** — Publishing a new state-funded construction tender
   (e.g., promising to build a hospital) in the target club's region. The tender is
   created with `investor_id = "STATE:{region_id}"` and
   `investor_type = TenderInvestorType::State`, following the existing
   `ConstructionTender` structure in `construction/tenders.rs` (line 46). The Treasury
   encumbers the `estimated_cost` as the budget ceiling. This creates real construction
   jobs and physical infrastructure in the target region.

**Revised `PorkBarrelOffer` struct:**

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PorkBarrelOffer {
    pub target_club: String,
    pub seats_bought: u32,
    /// Execution method: direct subsidy or construction tender.
    pub method: PorkBarrelMethod,
    /// Total Treasury cost.
    pub budget_cost: f64,
    pub political_capital_cost: f64,
    pub vote_bonus: f64,
}

pub enum PorkBarrelMethod {
    /// Direct subsidy to companies in the target club's stronghold regions.
    /// Uses `settle_treasury_to_company()` for each recipient.
    CompanySubsidy {
        target_company_ids: Vec<String>,
        per_company_amount: f64,
    },
    /// State-funded construction tender in the target region.
    /// Creates a real `ConstructionTender` in the tender market.
    ConstructionProject {
        region_id: String,
        project_type: ConstructionProjectType,
        estimated_cost: f64,
    },
}
```

### Correction 2: Wealth/Census Voting Uses ClassDemographics, Not Interest Groups

**Rule:** Wealth-weighted and Census-restricted elections must directly query the
`savings` and `savings_per_capita` fields of the actual `ClassDemographics` within
`RegionalClassDemographics.rural_classes` and `urban_classes`. Interest Group wealth is
NOT used for election seat calculation.

**Implementation:** The existing class keys are (from `geography.rs` lines 1423–1459):
- Rural: `"Aristocracy"`, `"FreePeasant"`, `"LandlessLaborer"`
- Urban: `"Worker"`, `"Bourgeoisie"`

**Revised `calculate_seats_wealth_census()`:**

```rust
/// Wealth-weighted and Census-restricted elections using ClassDemographics savings.
///
/// # Arguments
/// * `parties` - Active parties with support percentages.
/// * `regions` - Country regions with class demographics.
/// * `class_group_mapping` - Maps classes to interest groups (for party backing).
/// * `suffrage` - WealthWeightedVoting or CensusRestrictedVoting.
/// * `total_seats` - Seats to allocate.
///
/// # Rules
/// * WealthWeightedVoting: Party support is multiplied by the total savings of the
///   demographic classes backing that party (via class_group_mapping). A party backed
///   by Aristocracy (high savings) gets a seat bonus; a party backed by LandlessLaborer
///   (near-zero savings) gets seats reduced.
/// * CensusRestrictedVoting: Only classes with `savings_per_capita > census_threshold`
///   (default: 100.0) are counted. A LandlessLaborer with 0 savings has 0 voting power.
///   The Bourgeoisie and Aristocracy control seat distribution based on accumulated wealth.
pub fn calculate_seats_wealth_census(
    parties: &HashMap<String, Party>,
    regions: &[Region],
    class_group_mapping: &ClassToGroupMapping,
    suffrage: SuffrageType,
    total_seats: u32,
) -> HashMap<String, u32>;
```

**Algorithm:**
1. For each region, iterate `rural_classes` and `urban_classes`.
2. For each `ClassDemographics`, look up its backing party via
   `class_group_mapping` → interest group → party `base`.
3. **Wealth-weighted:** Accumulate `party_weight[party] += cd.savings * voting_weight`.
   The party's effective support = `party.support * party_weight[party] / total_weight`.
4. **Census-restricted:** If `cd.savings_per_capita < census_threshold`, skip this class
   entirely (disenfranchised). Otherwise, accumulate `party_weight[party] += cd.population as f64`.
5. Feed the weighted support into the existing D'Hondt allocation.

### Correction 3: Strict Demographic Targeting for Parliament Payroll

**Rule:** Parliament payroll must NOT use the generic
`credit_citizen_savings_region()` (which distributes proportionally across ALL classes by
population). Instead, wages must be routed to **specific** `ClassDemographics` so they
participate in the B2C retail market correctly.

**Implementation:** Direct mutation of `ClassDemographics.savings` for specific classes
in the capital region (identified by `region.is_capital == true`, field at
`geography.rs` line 527).

**Payroll routing:**
- **MP salaries** → credited to `"Aristocracy"` (rural) or `"Bourgeoisie"` (urban) in
  the capital region. MPs are elite politicians; their wages flow to the wealthy classes
  that shop in luxury retail.
- **Staff salaries** → credited to `"Worker"` (urban) in the capital region.
  Administrative staff are working-class employees who spend in basic retail.

**Code pattern** (following the crisis_management.rs direct-mutation approach at
lines 569–579):

```rust
// Find the capital region
let capital_idx = country.regions.iter().position(|r| r.is_capital);
if let Some(cap_idx) = capital_idx {
    let region = &mut country.regions[cap_idx];
    
    // MP salaries → Bourgeoisie (urban elite)
    if let Some(bourgeoisie) = region.class_demographics.urban_classes.get_mut("Bourgeoisie") {
        bourgeoisie.savings += mp_payroll;
    }
    // Staff salaries → Worker (urban working class)
    if let Some(worker) = region.class_demographics.urban_classes.get_mut("Worker") {
        worker.savings += staff_payroll;
    }
}
```

**Treasury debit:**
```rust
let total_payroll = mp_payroll + staff_payroll;
// CRITICAL: Check if Treasury can afford it (Correction 4).
if country.budget.liquid_reserves >= total_payroll {
    country.budget.liquid_reserves -= total_payroll;
    // Credit specific classes as above.
} else {
    // Payroll fails — see Correction 4.
}
```

### Correction 4: Consequences for a Bankrupt Parliament

**Rule:** If the Treasury cannot afford the Parliament payroll
(`liquid_reserves < total_payroll`), the money is NOT printed. The payroll fails. This
has severe consequences:

1. **`building.condition` degrades rapidly** — Unmaintained parliament building
   physically deteriorates. Condition drops by `0.05` per failed payroll turn (vs normal
   degradation of ~`0.005`). At `condition < 0.3`, the parliament is non-functional
   (bills cannot be processed, agenda control drops to 0).

2. **`political_capital` takes a massive hit** — Unpaid politicians will not support the
   government. `political_capital` drops by `20.0` per failed payroll turn (from a
   baseline of ~`50.0–100.0`). If `political_capital < 10.0`, the ruling coalition cannot
   pass any legislation (pork-barrel offers fail, agenda control is ignored).

3. **Coalition stability risk** — Each failed payroll turn increases
   `PartyOrganization.factional_tension` by `0.15` for all coalition partners, raising
   the probability of splintering and government collapse.

4. **No partial payment** — If `liquid_reserves < total_payroll`, the entire payroll
   fails. No MP or staff receives wages. This is the harshest possible outcome and
   creates strong political pressure to fix the budget.

**Code pattern:**

```rust
if country.budget.liquid_reserves < total_payroll {
    // PAYROLL FAILS — no money is printed.
    let shortfall = total_payroll - country.budget.liquid_reserves;
    
    // 1. Building condition degrades rapidly.
    if let Some(parliament_building) = buildings.iter_mut().find(|b| b.sector == Sector::Government) {
        parliament_building.condition = (parliament_building.condition - 0.05).max(0.0);
    }
    
    // 2. Political capital crashes.
    country.politics.political_capital = (country.politics.political_capital - 20.0).max(0.0);
    
    // 3. Coalition factional tension rises.
    for party_id in &country.politics.coalition {
        if let Some(party) = country.politics.active_parties.get_mut(party_id) {
            party.organization.factional_tension = (party.organization.factional_tension + 0.15).min(1.0);
        }
    }
    
    // 4. Telemetry.
    messages.push(format!(
        "[PARLIAMENT BANKRUPT] Payroll failed — shortfall: {:.0}. Condition degrading, political capital collapsing.",
        shortfall
    ));
    
    // No wages credited to any class.
    return;
}
```

---

## 0. Executive Summary

Phase 31 stabilized the economy after the Phase 30 global-trade shock and gave the State
crisis-management capabilities via **executive decrees only** (bypassing the legislative
`bill_lifecycle` stub entirely). Phase 32 now replaces those stubs with a **functional
Parliament**: dynamic chambers, named VIPs, committees, mathematical debate, pork-barrel
vote buying, fast-track crisis legislation, a constitutional State of Emergency, deep
electoral math with mid-term faction splintering, a physical Parliament Building, and two
new TUI tabs.

This document is a rigorous read-only audit of the existing political engine followed by a
detailed implementation blueprint. All new logic will use **English-only code** and **typed
enums** (`Commodity`, `Sector`, `Ideology`) — no localized Polish string keys in new logic.

---

## 1. Audit of the Existing Political Engine

### 1.1 Current Parliament Representation

**File:** `state/src/politics/system.rs` (lines 734–920)

`Politics` currently stores parliament as two flat seat-count maps:

```rust
pub parliament: HashMap<String, u32>,        // party_name → seats (lower house)
pub upper_house: HashMap<String, u32>,       // party_name → seats (upper house)
```

**Problems:**
- No chamber objects — no Speaker, no Presidium, no agenda control.
- No notion of 0/1/2 chambers derived from `GovernmentForm.chambers()`.
- MPs are not tracked at all — only aggregate seat counts per party.
- No named VIPs beyond `head_of_state: Leader` and party `leader: Leader`.
- `GovernmentForm.chambers()` returns 0/1/2 but this is never used to gate legislation.

**File:** `state/src/politics/system.rs` (lines 67–75)

```rust
pub fn chambers(self) -> u32 {
    match self {
        GovernmentForm::AbsoluteMonarchy | GovernmentForm::MilitaryDictatorship => 0,
        GovernmentForm::ElectiveMonarchy | GovernmentForm::OnePartyState | GovernmentForm::Theocracy => 1,
        _ => 2,
    }
}
```

This method exists but is **never called** in the turn engine. The `process_political_year`
function in `politics/turn.rs` always calculates both lower and upper house composition
regardless of regime type.

### 1.2 Current Legislative Engine

**File:** `state/src/politics/legislation.rs` (359 lines)

`Bill` has clauses, concessions, stages (`Introduced → Committee → FloorVote →
BicameralPending → Executive → Enacted/Rejected`). `LegislativeSession` tracks active,
enacted, and rejected bills.

**File:** `state/src/politics/bill_lifecycle.rs` (389 lines)

`process_bill_lifecycle()` runs the full pipeline: committee → floor vote → bicameral →
executive. However, `process_legislation_turn()` (line 365) is a **stub**:

```rust
// Placeholder: no bills to process until legislative_session is wired into Politics
messages.push("[LEGISLATION] No active legislative session".to_string());
```

Although `Politics.legislative_session: Option<LegislativeSession>` and
`Politics.committee_system: Option<CommitteeSystem>` exist (lines 859–863), **no bills are
ever introduced**. The entire pipeline is dead code.

**File:** `state/src/politics/budget_lifecycle.rs` (~500 lines)

The budget bill lifecycle is more complete: `draft_budget_bill()`, `process_budget_amendments()`,
`process_budget_lifecycle()` (amendment → floor vote → bicameral → executive), and
`apply_budget_failure_consequence()`. Floor voting uses ideological alignment of opposition
parties with the budget's spending priorities. However, this is called from the turn engine
only for the annual budget, not for general legislation.

### 1.3 Current Committee System

**File:** `state/src/politics/committees.rs` (351 lines)

`Committee` has proportional composition mirroring parliament, a chair from the ruling
coalition, and a partisan bias. `CommitteeSystem` initializes 8 standard committees (Budget,
Health, Education, Defense, ForeignAffairs, Justice, Infrastructure, SocialAffairs).

**Problems:**
- Committee names are hardcoded in Polish: `"Komisja Budżetowa"`, etc.
- `calculate_recommendation()` returns a modifier but it is only applied in the dead
  `process_bill_lifecycle` path.
- No amendment proposal mechanism — committees only delay and recommend.

### 1.4 Current Election System

**File:** `state/src/politics/elections.rs` (389 lines)

`calculate_seats()` supports three methods: **D'Hondt** (default), **Sainte-Laguë**, and
**Hare-Niemeyer** (largest remainder). Electoral thresholds are supported.

**Missing methods:**
- **FPTP / Majoritarian** — no single-member district support.
- **Wealth/Census voting** — `SuffrageType::WealthWeightedVoting` and
  `CensusRestrictedVoting` exist in `interest_groups.rs` but are never used in seat
  calculation. Currently all elections use proportional representation regardless of
  suffrage type.

`build_coalition()` and `build_coalition_with_concessions()` form coalitions by ideological
proximity with a max distance of 1.4. `check_coalition_stability()` can collapse coalitions
when ideological spread + unrest exceed thresholds.

`calculate_upper_house_composition()` handles hereditary, appointed, indirect, and universal
election upper houses.

### 1.5 Current Crisis Management (Phase 31)

**File:** `state/src/politics/crisis_management.rs` (~1300 lines)

`execute_crisis_response()` (line 958) is the main entry point, called from
`engine/turn.rs` (line 906) **before** ministry procurement. It:
1. Detects crisis via `CrisisIndicators`
2. Gets ideology-specific `CrisisResponseProfile`
3. Applies coalition moderation (mathematical, no voting)
4. Executes fiscal response (tax adjustments + spending cuts) — **executive decree**
5. Issues crisis bonds (strict private-liquidity auction)
6. Allocates emergency subsidies — **executive decree**
7. Applies starvation mortality
8. Voluntary legalization of shadow workers
9. Gradual distress handling for bankrupt companies

**All actions are executive decrees** — none go through Parliament. This was the correct
design for Phase 31 (the legislative engine was a stub), but Phase 32 requires a more
nuanced approach: major fiscal changes should go through fast-track legislation, with
decrees reserved for minor interventions.

### 1.6 Current Emergency Powers

**File:** `state/src/government/treasury.rs` (lines 27–67)

`EmergencyPowers` enum: `Normal`, `ExciseTaxesEnabled`, `RationingEnabled`, `MartialLaw`.
Set by `check_emergency_conditions()` based on treasury deficit and commodity shortages.
This is a **fiscal** emergency system, not a **political** one.

**File:** `state/src/state/mod.rs` (lines 190–203)

```rust
pub enum EmergencyPowers { Normal, ExciseTaxesEnabled, RationingEnabled, MartialLaw }
```

There is no political State of Emergency that allows the executive to bypass Parliament.

### 1.7 Current VIP / Name Generation

**File:** `state/src/politics/generator.rs` (192 lines)

Only generates **party names** using cultural patterns (Slavic, Germanic, Latin, Middle
Eastern, Balkan). No personal name generation.

**File:** `state/src/politics/system.rs` (lines 79–107)

`Leader` has `name: String`, `gender`, `age`, `health`, `religion`, `nationality`, `views`,
`traits`, `main_trait`, `dynasty`, `base_influence`, `faction`. The `name` field is
populated during country generation but with unknown quality.

**File:** `state/src/society/cultures.rs` (line 176)

`generate_cultural_background()` picks a cultural group and nation but does not generate
personal names.

### 1.8 Current Building System

**File:** `state/src/entities/mod.rs` (lines 958–1017)

`Building` has: `id`, `name`, `owner_id`, `year_built`, `sector: Sector`, `worker_capacity`,
`current_employment`, `reserve` (cash), `active_method`, `region_id`, `condition`,
`last_production: BTreeMap<Commodity, f64>`, `last_profit`, `building_capacity`,
`cluster_info`, `fixed_assets`.

**File:** `state/src/registries/enums.rs` (lines 118–167)

`Sector` enum has 20 variants (Mining, Agriculture, HeavyIndustry, ... MaintenanceWorkshops).
There is **no Government/Parliament sector**. The closest is `PublicAdministration`.

### 1.9 Current TUI Tabs

**File:** `state/src/ui/tui/tabs.rs` (62 lines)

5 tabs: `MacroFinance` (1), `MarketLogistics` (2), `ConstructionGeology` (3),
`SocietyJustice` (4), `Sectors` (5). Hotkeys 1–5, Tab/BackTab cycling.

**File:** `state/src/ui/snapshot.rs` (~260 lines)

`CountrySnapshot` is a flat struct with macro, market, construction, society, and sector
fields. No government or parliament data.

**File:** `state/src/ui/tui/render.rs` (359 lines)

`render_tab_content()` dispatches to per-tab render functions. All return `Table<'a>`.

**File:** `state/src/ui/tui/app.rs` (line 469–473)

Hotkeys hardcoded: `KeyCode::Char('1')` → MacroFinance, etc.

---

## 2. Implementation Blueprint

### PART 1: Parliament Structure & Named VIPs

#### 2.1.1 New Module: `state/src/politics/names.rs`

A culturally-aware personal name generator for VIPs.

```rust
pub struct NamePool {
    pub first_names_male: Vec<&'static str>,
    pub first_names_female: Vec<&'static str>,
    pub surnames: Vec<&'static str>,
}

pub fn name_pool_for_culture(cultural_group: &str) -> &'static NamePool;
pub fn generate_person_name(cultural_group: &str, gender: &str, rng: &mut impl Rng) -> String;
pub fn generate_full_vip(cultural_group: &str, rng: &mut impl Rng) -> VipName;
```

**Cultural pools** (5 groups matching `generator.rs`):
- **Slavic** (`"Słowiańska"`): Polish/Czech/Slovak names (e.g., "Jan Kowalski", "Anna Wiśniewska")
- **Germanic** (`"Germańska"`): German/Nordic names
- **Latin** (`"Łacińska"`): French/Italian/Spanish names
- **Middle Eastern** (`"Bliskowschodnia"`): Arabic names
- **Balkan** (`"Bałkańska"`): Serbo-Croatian names

Each pool contains ~50 first names per gender and ~80 surnames. Names are stored as
`&'static str` for zero allocation. The function uses deterministic RNG seeded by
`country_name + role + turn` for reproducibility.

```rust
pub struct VipName {
    pub first_name: String,
    pub surname: String,
    pub full_name: String,
    pub gender: String,
}
```

#### 2.1.2 New Module: `state/src/politics/parliament.rs`

The core Parliament data structures.

```rust
/// A single legislative chamber (Lower House or Senate).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Chamber {
    /// Chamber identifier: "lower" or "upper".
    pub id: String,
    /// Display name: "Sejm", "Senate", etc.
    pub name: String,
    /// Total seat count.
    pub total_seats: u32,
    /// Seat distribution by parliamentary club (club_id → seats).
    pub seats: HashMap<String, u32>,
    /// Presidium: Speaker and Deputy Speakers.
    pub presidium: ChamberPresidium,
    /// Active bills in this chamber's queue (bill IDs).
    pub legislative_queue: Vec<String>,
    /// Recently passed/rejected bills with vote tallies (last 20).
    pub recent_votes: Vec<VoteRecord>,
}

/// The Speaker and Deputy Speakers controlling the legislative agenda.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ChamberPresidium {
    /// Speaker (Marszałek) — named VIP.
    pub speaker: NamedVip,
    /// Deputy speakers (Wicemarszałkowie).
    pub deputy_speakers: Vec<NamedVip>,
    /// Party/club of the speaker.
    pub speaker_club: String,
    /// Agenda control factor (0.0–1.0): how much the speaker controls what reaches the floor.
    pub agenda_control: f64,
}

/// A named VIP holding a political office.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct NamedVip {
    pub full_name: String,
    pub party: String,
    pub role: VipRole,
    pub ideology: String,
    pub age: u32,
}

/// Types of VIP political offices.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub enum VipRole {
    #[default]
    HeadOfState,
    PrimeMinister,
    Minister,
    Speaker,
    DeputySpeaker,
    Whip,
}

/// A parliamentary club/faction (anonymized MP seat pool).
/// Regular MPs are tracked as seat counts, not individual entities.
/// Clubs can form via mid-term splintering without creating new active_parties.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ParliamentaryClub {
    /// Club identifier (may differ from party name for splinter groups).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Parent party (if affiliated); None for independent clubs.
    pub parent_party: Option<String>,
    /// Seat count in this chamber.
    pub seats: u32,
    /// Ideology string (inherited from parent or declared at splinter).
    pub ideology: String,
    /// Club discipline (0.0–1.0).
    pub discipline: f64,
    /// Whether this club was formed by mid-term splintering.
    pub is_splinter: bool,
    /// Turn when the club was formed.
    pub formation_turn: u32,
}

/// A recorded floor vote on a bill.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct VoteRecord {
    pub bill_id: String,
    pub bill_title: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
    pub passed: bool,
    pub turn: u32,
}

/// The full Parliament for a country (0, 1, or 2 chambers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Parliament {
    /// Chambers present (0, 1, or 2 based on GovernmentForm).
    pub chambers: Vec<Chamber>,
    /// All parliamentary clubs in the lower house.
    pub clubs: Vec<ParliamentaryClub>,
    /// Named VIPs: Head of State, PM, Ministers, Speakers.
    pub vips: Vec<NamedVip>,
    /// Whether parliament is currently suspended (State of Emergency).
    pub suspended: bool,
}
```

**Integration with `Politics`:**

Add to `Politics` in `system.rs`:
```rust
#[serde(rename = "parlament_struktura", default)]
pub parliament_struct: Option<Parliament>,
```

Using `Option<Parliament>` ensures backward compatibility with existing saves. When
`None`, the engine falls back to the legacy flat `parliament: HashMap<String, u32>`.

**Chamber initialization** based on `GovernmentForm.chambers()`:
- 0 chambers: `Parliament { chambers: vec![], suspended: false }` (absolutist/dictatorial)
- 1 chamber: single `Chamber { id: "lower", ... }`
- 2 chambers: lower + upper, upper house composition from `calculate_upper_house_composition()`

**VIP generation** during elections:
- Head of State: from `politics.head_of_state` (already exists) — populate name if empty
- PM: ruling party leader — populate name if empty
- Ministers: from `ministry_config.ministries` — populate `minister_name` if empty
- Speakers: generated per chamber from the ruling coalition's senior MPs

#### 2.1.3 Chamber Presidium & Agenda Control

The Speaker's `agenda_control` (0.0–1.0) determines:
- **Bill scheduling priority**: High agenda_control → government bills fast-tracked, opposition bills delayed
- **Debate time allocation**: High agenda_control → shorter debates for government bills
- **Committee assignment influence**: Speaker can steer bills to friendly committees

Agenda control is derived from:
- Coalition seat share (majority → 0.8, minority → 0.4)
- Speaker's party discipline
- Whether the Speaker is from the ruling party

---

### PART 2: The Legislative Engine & Committees

#### 2.2.1 Committee Amendment Proposals

Extend `Committee` in `committees.rs` with amendment capability:

```rust
/// An amendment proposed during committee or floor debate.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BillAmendment {
    /// Proposing club/party.
    pub proposer: String,
    /// Target clause index.
    pub clause_index: usize,
    /// Parameter being amended (e.g., "tax_rate", "spending_cut").
    pub parameter: String,
    /// Delta value (e.g., +0.01 for +1% tax).
    pub delta: f64,
    /// Whether the amendment was accepted.
    pub accepted: bool,
    /// Ideological justification.
    pub rationale: String,
}
```

**Mathematical debate model:**
1. Bill enters committee → committee members propose amendments based on ideological
   distance from the bill. Each committee member (proportional to party seats) has a
   probability of proposing an amendment proportional to `1.0 - ideological_alignment`.
2. Amendments are accepted/rejected by majority vote within the committee (using
   `deterministic_roll` seeded by `bill_id + amendment_index + turn`).
3. Accepted amendments modify the bill's clause parameters (e.g., tax rate ±1%).
4. Bill advances to floor with amended parameters.

#### 2.2.2 Pork-Barreling & Vote Buying

> **CORRECTION 1 APPLIED:** Pork-barrel spending must NOT credit abstract regional
> wallets. It is executed via real economic hooks: `MinistrySpendingAction::Subsidy`
> (crediting company `available_cash` via `settle_treasury_to_company()`) or
> `ConstructionTender` (publishing a state-funded tender in the target region). See
> Section 0a, Correction 1 for the full rule.

Add to `bill_lifecycle.rs`:

```rust
/// Pork-barrel offer to buy opposition votes.
/// Executed via REAL economic hooks — no ghost wallets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PorkBarrelOffer {
    /// Target club/party being bribed.
    pub target_club: String,
    /// Seats being bought.
    pub seats_bought: u32,
    /// Execution method: direct company subsidy or construction tender.
    pub method: PorkBarrelMethod,
    /// Total Treasury cost.
    pub budget_cost: f64,
    /// Political capital spent.
    pub political_capital_cost: f64,
    /// Vote probability bonus per seat (0.0–1.0).
    pub vote_bonus: f64,
}

/// How pork-barrel spending is physically executed in the economy.
pub enum PorkBarrelMethod {
    /// Direct subsidy to companies in the target club's stronghold regions.
    /// Uses `settle_treasury_to_company()` for each recipient (transfer_settler.rs:394).
    /// Follows the existing agricultural subsidy pattern in ministries.rs:707–716.
    CompanySubsidy {
        target_company_ids: Vec<String>,
        per_company_amount: f64,
    },
    /// State-funded construction tender in the target region.
    /// Creates a real `ConstructionTender` (construction/tenders.rs:46) with
    /// `investor_id = "STATE:{region_id}"`, `investor_type = State`.
    /// Generates real construction jobs and physical infrastructure.
    ConstructionProject {
        region_id: String,
        project_type: ConstructionProjectType,
        estimated_cost: f64,
    },
}

/// Attempt to buy opposition votes using pork-barrel spending.
/// Returns the offers made and the total Treasury cost.
/// All offers are executed via real economic hooks (subsidies or tenders).
pub fn attempt_pork_barrel(
    bill: &Bill,
    parliament: &HashMap<String, u32>,
    coalition: &[String],
    companies: &[Company],         // For finding companies in target regions
    regions: &[Region],            // For identifying stronghold regions
    treasury_reserves: f64,
    political_capital: f64,
) -> (Vec<PorkBarrelOffer>, f64);
```

**Rules:**
- Only the ruling coalition can offer pork.
- Cost per seat bought = `GDP * 0.0001 * seats_bought` (0.01% of GDP per seat).
- Political capital is a new field on `Politics`: `political_capital: f64`, regenerated
  each turn based on ruling party support and coalition stability.
- Vote bonus is capped at +0.3 per seat (can't guarantee a yes, only increase probability).
- **Execution:** Each offer is physically executed:
  - `CompanySubsidy`: Treasury debited via `settle_treasury_to_company()`, company
    `available_cash` or `brokerage_account.cash` credited. Double-entry via bank balance
    sheet sync.
  - `ConstructionProject`: `ConstructionTender` published in the tender market with
    Treasury encumbrance. Real construction jobs created.
- Pork offers are also added as `Concession` entries on the bill (reusing the existing
  concession mechanism in `legislation.rs`) for vote-probability calculation.

#### 2.2.3 Wiring the Legislative Engine

Replace the stub `process_legislation_turn()` in `bill_lifecycle.rs` with a functional
implementation that:
1. Checks `politics.parliament_struct.suspended` — if suspended (State of Emergency), skip.
2. Iterates `legislative_session.active_bills`.
3. For each bill in `Committee` stage: check `committee_completion_turn`, run amendment
   proposals, advance to `FloorVote`.
4. For each bill in `FloorVote` stage: run `process_floor_vote()` with pork-barrel offers,
   record `VoteRecord` in the chamber's `recent_votes`.
5. For each bill in `BicameralPending`: run upper chamber vote.
6. For each bill in `Executive`: run executive review (veto/sign).
7. Enacted bills trigger `enact_law()` to mutate economic configs.

---

### PART 3: Crisis Management Revision & State of Emergency

#### 2.3.1 Split: Decrees vs Fast-Track Legislation

Revise `crisis_management.rs` to classify crisis actions:

**Executive Decrees (minor, immediate):**
- Emergency subsidies to collapsing sectors
- Voluntary legalization of shadow workers
- Gradual distress handling for bankrupt companies
- Starvation mortality application (mechanical, not political)

**Fast-Track Legislation (major, goes through Parliament):**
- Broad tax changes (PIT/CIT/VAT adjustments)
- Sovereign bond issuance authorization
- Spending cuts to ministry budgets
- Emergency appropriation reallocations

**New function in `crisis_management.rs`:**

```rust
/// Classify a crisis action as decree or fast-track.
pub enum CrisisActionType {
    Decree,
    FastTrack,
}

/// Determine action type based on severity and scope.
pub fn classify_crisis_action(
    indicators: &CrisisIndicators,
    action: &CrisisAction,
) -> CrisisActionType;
```

**Fast-track process** (new in `bill_lifecycle.rs`):
1. Crisis detected → `execute_crisis_response()` creates a `Bill` with crisis clauses.
2. Bill is introduced with `FastTrack` flag → committee delay reduced to 0–1 turns.
3. Speaker's `agenda_control` is set to 1.0 for fast-track bills (no delay tactic).
4. Floor vote occurs same turn or next turn (depending on severity).
5. If Parliament rejects → fallback to decree only if State of Emergency is active.

#### 2.3.2 State of Emergency

**New struct on `Politics`:**

```rust
/// Constitutional State of Emergency / Martial Law (political, not fiscal).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct StateOfEmergency {
    /// Whether currently active.
    pub active: bool,
    /// Turn when activated.
    pub activation_turn: u32,
    /// Reason for activation (crisis severity, war, rebellion).
    pub reason: String,
    /// Maximum duration in turns (constitutional limit, e.g., 24 turns = 1 year).
    pub max_duration: u32,
    /// Turns remaining.
    pub turns_remaining: u32,
    /// Whether Parliament is suspended (full martial law) or just fast-tracked.
    pub parliament_suspended: bool,
    /// Who authorized it (Head of State or PM).
    pub authorized_by: String,
}

impl StateOfEmergency {
    /// Check if the executive can bypass Parliament entirely.
    pub fn can_bypass_parliament(&self) -> bool {
        self.active && self.parliament_suspended
    }
    
    /// Tick down the timer; auto-expire when duration elapses.
    pub fn tick(&mut self) {
        if self.active && self.turns_remaining > 0 {
            self.turns_remaining -= 1;
            if self.turns_remaining == 0 {
                self.active = false;
                self.parliament_suspended = false;
            }
        }
    }
}
```

**Add to `Politics`:**
```rust
#[serde(rename = "stan_wyjatkowy", default)]
pub state_of_emergency: Option<StateOfEmergency>,
```

**Activation criteria** (in `execute_crisis_response`):
- Crisis severity > 0.7 (70%) AND treasury coverage < 1 month
- OR: `EmergencyPowers::MartialLaw` already active (fiscal martial law escalates to political)
- OR: Active rebellion/war on home territory

**When active with `parliament_suspended = true`:**
- `process_legislation_turn()` skips all bill processing.
- `execute_crisis_response()` executes ALL actions as decrees (reverts to Phase 31 behavior).
- `Parliament.suspended = true`.
- Telemetry records the suspension.

**When active with `parliament_suspended = false` (limited emergency):**
- All crisis bills are fast-tracked (0-turn committee delay).
- Speaker's agenda control set to 1.0.
- Opposition cannot delay.

**Auto-expiry:** `StateOfEmergency.tick()` is called each turn. When `turns_remaining`
reaches 0, the emergency lapses and Parliament resumes normal function. The ruling party
may face an approval penalty for overusing emergency powers.

---

### PART 4: Elections & Faction Splintering

#### 2.4.1 Electoral Math: New Methods

Extend `calculate_seats()` in `elections.rs` with two new methods:

**FPTP / Majoritarian:**
```rust
/// Single-member district first-past-the-post.
/// Each district's seat goes to the party with the most votes.
/// Ties broken deterministically by party name.
pub fn calculate_seats_fptp(
    parties: &HashMap<String, Party>,
    num_districts: u32,
) -> HashMap<String, u32>;
```

Districts are simulated by dividing national support into `num_districts` virtual districts
using a deterministic noise function (seeded by country name + year). Each district's winner
takes that seat. This produces majoritarian outcomes where the largest party gets a seat
bonus.

**Wealth/Census Voting:**

> **CORRECTION 2 APPLIED:** Wealth/Census voting must query `ClassDemographics.savings`
> and `savings_per_capita` directly from `RegionalClassDemographics.rural_classes` and
> `urban_classes`, NOT Interest Group wealth. See Section 0a, Correction 2 for the full
> rule.

```rust
/// Wealth-weighted and Census-restricted elections using ClassDemographics savings.
///
/// # Arguments
/// * `parties` - Active parties with support percentages.
/// * `regions` - Country regions with class demographics (rural_classes, urban_classes).
/// * `class_group_mapping` - Maps demographic classes to interest groups (for party backing).
/// * `suffrage` - WealthWeightedVoting or CensusRestrictedVoting.
/// * `total_seats` - Seats to allocate.
///
/// # Rules
/// * WealthWeightedVoting: Party support is multiplied by the total `savings` of the
///   demographic classes backing that party (via class_group_mapping → interest group →
///   party base). A party backed by Aristocracy (savings ~5000/capita) gets a seat bonus;
///   a party backed by LandlessLaborer (savings ~50/capita) gets seats reduced.
/// * CensusRestrictedVoting: Only classes with `savings_per_capita > census_threshold`
///   (default: 100.0) are counted. A LandlessLaborer with 0 savings has 0 voting power.
///   The Bourgeoisie (savings ~1000/capita) and Aristocracy (savings ~5000/capita)
///   control seat distribution based on accumulated wealth.
pub fn calculate_seats_wealth_census(
    parties: &HashMap<String, Party>,
    regions: &[Region],
    class_group_mapping: &ClassToGroupMapping,
    suffrage: SuffrageType,
    total_seats: u32,
) -> HashMap<String, u32>;
```

**Algorithm:**
1. For each region, iterate `rural_classes` and `urban_classes` (existing class keys:
   `"Aristocracy"`, `"FreePeasant"`, `"LandlessLaborer"`, `"Worker"`, `"Bourgeoisie"`).
2. For each `ClassDemographics`, look up its backing party via
   `class_group_mapping` → interest group → party `base`.
3. **Wealth-weighted:** Accumulate `party_weight[party] += cd.savings`. The party's
   effective support = `party.support * party_weight[party] / total_weight`. A party
   backed by Aristocracy (high aggregate savings) gets a seat bonus.
4. **Census-restricted:** If `cd.savings_per_capita < census_threshold` (default 100.0),
   skip this class entirely (disenfranchised). Otherwise, accumulate
   `party_weight[party] += cd.population as f64`. Only wealthy classes vote.
5. Feed the weighted support into the existing D'Hondt allocation.

**Method selection** based on `Politics.election_method` and `Constitution.suffrage_system`:
- `"D'Hondt"` / `"Sainte-Laguë"` / `"Hare-Niemeyer"` → existing proportional methods
- `"FPTP"` → new `calculate_seats_fptp`
- `"Wealth"` → `calculate_seats_wealth_census` with `WealthWeightedVoting`
- `"Census"` → `calculate_seats_wealth_census` with `CensusRestrictedVoting`

#### 2.4.2 Mid-Term Faction Splintering

**New function in `parliament.rs`:**

```rust
/// Check for mid-term faction splintering.
/// MPs defect from their club to a new or existing club based on:
/// - Ideological distance from their party's current position
/// - Unpopular government policies (low approval, high unrest)
/// - Party organization factional_tension
/// Returns a list of splinter events for telemetry.
pub fn check_faction_splintering(
    parliament: &mut Parliament,
    active_parties: &HashMap<String, Party>,
    approval_rating: f64,
    unrest: f64,
    current_turn: u32,
) -> Vec<SplinterEvent>;

/// A recorded splinter event.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SplinterEvent {
    pub source_club: String,
    pub new_club: String,
    pub seats_defected: u32,
    pub reason: String,
    pub turn: u32,
}
```

**Splintering logic:**
1. For each club in the lower house, check `PartyOrganization.factional_tension` and
   `split_risk()`.
2. If `split_risk() > 0.4` AND (`unrest > 30` OR `approval_rating < 40`):
   - Calculate defectors: `seats * split_risk() * (1.0 - discipline) * 0.5`
   - Create a new `ParliamentaryClub` with `is_splinter = true`, ideology derived from
     the parent party's ideology shifted by ±0.2 on one compass axis.
   - Move defected seats from source club to new club in the chamber's `seats` map.
3. Splinter clubs can merge with ideologically close existing clubs (distance < 0.3).
4. Splinter clubs do NOT appear in `active_parties` — they are voting blocs only.
5. At the next general election, splinter clubs with > 5% of seats may register as a
   new party in `active_parties` (if they choose to run).

**Power dynamics recalculation:**
After splintering, recompute:
- Coalition majority status (may lose majority if a coalition partner splinters)
- Speaker's agenda control
- Committee compositions (proportional to new seat distribution)

---

### PART 5: The Physical Political Economy

#### 2.5.1 Parliament Building

**Extend `Sector` enum** in `registries/enums.rs`:
```rust
/// `"government"` — Parliament, government buildings, ministerial offices.
Government,
```

**Parliament Building creation** during country generation:
- One `Building` per country with `sector: Sector::Government`, placed in the capital region.
- `name: "Parliament"` (or culturally appropriate: "Sejm", "Reichstag", etc.)
- `worker_capacity`: scales with parliament size (e.g., `total_seats * 3` — MPs + staff)
- `current_employment`: filled with politicians and administrative staff
- `reserve`: cash reserve for operating expenses (funded from Treasury)

**Per-turn operation** (new phase in `engine/turn.rs`):

1. **Wage payroll:** Parliament staff and MPs are paid from Treasury `liquid_reserves`.

   > **CORRECTION 3 APPLIED:** Payroll must NOT use the generic
   > `credit_citizen_savings_region()` (which distributes proportionally across ALL
   > classes). Wages must be routed to **specific** `ClassDemographics` in the capital
   > region. See Section 0a, Correction 3 for the full rule.
   >
   > **CORRECTION 4 APPLIED:** If Treasury cannot afford the payroll, the money is NOT
   > printed. The payroll fails with severe consequences. See Section 0a, Correction 4.

   - MP salary: `average_wage * 3.0` per MP (politicians earn above-average wages)
   - Staff salary: `average_wage * 0.8` per staff member
   - Total payroll = `(MPs * mp_salary) + (staff * staff_salary)`
   - **Treasury check:** If `liquid_reserves < total_payroll`, payroll FAILS (Correction 4):
     - `building.condition -= 0.05` (rapid degradation)
     - `political_capital -= 20.0` (massive hit)
     - All coalition partners' `factional_tension += 0.15` (splintering risk)
     - No wages credited to any class. No money printed.
     - Telemetry: `"[PARLIAMENT BANKRUPT] Payroll failed — shortfall: {shortfall}"`
   - **If Treasury CAN afford payroll:**
     - Debit: `country.budget.liquid_reserves -= total_payroll`
     - Credit MP salaries → `"Bourgeoisie"` (urban) in the capital region
       (`region.class_demographics.urban_classes.get_mut("Bourgeoisie").savings += mp_payroll`)
     - Credit staff salaries → `"Worker"` (urban) in the capital region
       (`region.class_demographics.urban_classes.get_mut("Worker").savings += staff_payroll`)
     - Capital region identified by `region.is_capital == true` (geography.rs:527)
     - Direct `ClassDemographics.savings` mutation (following crisis_management.rs:569–579 pattern)
   - This counts as `G` (government spending) in GDP.

2. **Goods consumption:** Parliament consumes physical goods to function:
   - **Paper** (`Commodity::Paper`): `total_seats * 0.5` units per turn (legislative documents)
   - **Energy** (`Commodity::Energy`): `building.condition * 100.0` units (heating, lighting)
   - **Services** (`Commodity::Services`): `total_seats * 0.2` units (administrative services)
   - Procured via B2B order book (ministry procurement path, already exists)
   - If goods are unavailable, `building.condition` degrades (parliament can't function)

3. **Maintenance:** Standard building maintenance (existing system applies).

**When Parliament is suspended (State of Emergency):**
- MP wages are not paid (savings).
- Staff wages continue (skeleton crew) — credited to `"Worker"` in capital region.
- Goods consumption reduced by 80%.
- `building.condition` degrades at 2× normal rate (deferred maintenance).

**When Parliament is bankrupt (Treasury cannot afford payroll):**
- No wages paid to any class. No money printed.
- `building.condition -= 0.05` per failed turn (rapid physical deterioration).
- `political_capital -= 20.0` per failed turn (unpaid politicians withdraw support).
- All coalition partners' `factional_tension += 0.15` per failed turn (splintering risk).
- At `condition < 0.3`: parliament non-functional (bills cannot be processed).
- At `political_capital < 10.0`: ruling coalition cannot pass any legislation.

---

### PART 6: TUI Presentation (Two New Tabs)

#### 2.6.1 Tab Enum Extension

**File:** `state/src/ui/tui/tabs.rs`

```rust
pub enum Tab {
    MacroFinance,      // 1
    MarketLogistics,   // 2
    ConstructionGeology, // 3
    SocietyJustice,    // 4
    Sectors,           // 5
    Government,        // 6 (NEW)
    Parliament,        // 7 (NEW)
}
```

Update `ALL`, `title()`, `hotkey()` ('6', '7'), `next()`, `prev()`.

#### 2.6.2 Snapshot Extensions

**File:** `state/src/ui/snapshot.rs`

```rust
/// Government tab data.
#[derive(Debug, Clone, Default)]
pub struct GovernmentSnapshot {
    pub head_of_state_name: String,
    pub head_of_state_role: String,
    pub pm_name: String,
    pub pm_party: String,
    pub pm_ideology: String,
    pub cabinet: Vec<MinisterRow>,
    pub state_of_emergency: Option<EmergencySnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct MinisterRow {
    pub ministry_name: String,
    pub minister_name: String,
    pub party: String,
    pub ideology: String,
    pub allocated_cash: f64,
    pub spent_cash: f64,
}

#[derive(Debug, Clone, Default)]
pub struct EmergencySnapshot {
    pub active: bool,
    pub reason: String,
    pub turns_remaining: u32,
    pub parliament_suspended: bool,
}

/// Parliament tab data.
#[derive(Debug, Clone, Default)]
pub struct ParliamentSnapshot {
    pub chambers: Vec<ChamberSnapshot>,
    pub clubs: Vec<ClubRow>,
    pub recent_votes: Vec<VoteRow>,
    pub legislative_queue: Vec<QueueRow>,
    pub suspended: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ChamberSnapshot {
    pub name: String,
    pub total_seats: u32,
    pub speaker_name: String,
    pub speaker_club: String,
    pub seat_distribution: Vec<(String, u32)>,
}

#[derive(Debug, Clone, Default)]
pub struct ClubRow {
    pub name: String,
    pub seats: u32,
    pub ideology: String,
    pub is_splinter: bool,
    pub discipline: f64,
}

#[derive(Debug, Clone, Default)]
pub struct VoteRow {
    pub bill_id: String,
    pub bill_title: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub passed: bool,
    pub turn: u32,
}

#[derive(Debug, Clone, Default)]
pub struct QueueRow {
    pub bill_id: String,
    pub bill_title: String,
    pub stage: String,
    pub initiator: String,
}
```

Add to `CountrySnapshot`:
```rust
pub government: GovernmentSnapshot,
pub parliament: ParliamentSnapshot,
```

#### 2.6.3 Render Functions

**File:** `state/src/ui/tui/render.rs`

```rust
pub fn render_government<'a>(snap: &CountrySnapshot) -> Table<'a>;
pub fn render_parliament<'a>(snap: &CountrySnapshot) -> Table<'a>;
```

Update `render_tab_content()`:
```rust
Tab::Government => render_government(snap),
Tab::Parliament => render_parliament(snap),
```

**Government tab layout:**
- Header: Head of State name + role, PM name + party + ideology
- Cabinet table: Ministry | Minister | Party | Ideology | Allocated | Spent
- State of Emergency panel (if active): reason, turns remaining, parliament status

**Parliament tab layout:**
- Chamber selector (if bicameral): Lower / Upper
- Speaker name + club
- Seat distribution table: Club | Seats | Ideology | Splinter? | Discipline
- Recent votes table: Bill | For | Against | Passed? | Turn
- Legislative queue: Bill | Stage | Initiator

#### 2.6.4 App Hotkey Wiring

**File:** `state/src/ui/tui/app.rs`

Add:
```rust
KeyCode::Char('6') => self.active_tab = Tab::Government,
KeyCode::Char('7') => self.active_tab = Tab::Parliament,
```

---

## 3. Implementation Steps (Ordered)

### Step 1: Create `politics/names.rs`
- Cultural name pools (5 groups, ~50 first names × 2 genders, ~80 surnames each)
- `generate_person_name()`, `generate_full_vip()`
- Deterministic seeding: `seed = country_name + role + turn`
- Tests: name non-empty, cultural consistency, determinism

### Step 2: Create `politics/parliament.rs`
- `Chamber`, `ChamberPresidium`, `NamedVip`, `VipRole`, `ParliamentaryClub`, `VoteRecord`,
  `Parliament`, `SplinterEvent`
- `initialize_parliament()` — builds chambers from `GovernmentForm.chambers()`
- `check_faction_splintering()` — mid-term defection logic
- Tests: chamber count matches government form, splinter seat reallocation, VIP generation

### Step 3: Extend `Sector` enum + Building for Parliament
- Add `Sector::Government` to `registries/enums.rs`
- Update `display_name()` and `all()` methods
- Parliament building creation in generator
- Tests: sector serialization, building with Government sector

### Step 4: Add `StateOfEmergency` to `Politics`
- New struct in `system.rs` (or `parliament.rs`)
- Add `state_of_emergency: Option<StateOfEmergency>` to `Politics`
- Add `political_capital: f64` to `Politics`
- `tick()` method for auto-expiry
- Tests: activation, suspension, auto-expiry, bypass logic

### Step 5: Revise `crisis_management.rs`
- Add `CrisisActionType` classification (decree vs fast-track)
- Split `execute_crisis_response()` into decree-only and fast-track paths
- When State of Emergency active with `parliament_suspended`: all actions are decrees
- When no emergency: major actions create fast-track bills
- Keep minor actions (subsidies, legalization, distress) as decrees always
- Tests: classification logic, fast-track bill creation, martial-law bypass

### Step 6: Extend `elections.rs`
- `calculate_seats_fptp()` — single-member district simulation
- `calculate_seats_wealth_census()` — wealth-weighted and census-restricted
  - **CORRECTION 2:** Uses `ClassDemographics.savings` and `savings_per_capita` from
    `RegionalClassDemographics.rural_classes`/`urban_classes`, NOT Interest Group wealth.
  - Class keys: `"Aristocracy"`, `"FreePeasant"`, `"LandlessLaborer"`, `"Worker"`,
    `"Bourgeoisie"` (geography.rs:1423–1459)
  - Wealth-weighted: party support × aggregate class savings
  - Census-restricted: classes with `savings_per_capita < 100.0` are disenfranchised
- Method selection based on `election_method` + `suffrage_system`
- Tests: FPTP majoritarian bonus, wealth weighting favors rich classes, census
  disenfranchisement of LandlessLaborer (0 savings → 0 voting power)

### Step 7: Extend `bill_lifecycle.rs`
- Replace `process_legislation_turn()` stub with functional implementation
- Add `BillAmendment` struct + committee amendment proposals
- Add `PorkBarrelOffer` + `PorkBarrelMethod` + `attempt_pork_barrel()`
  - **CORRECTION 1:** Pork-barrel executed via `settle_treasury_to_company()` (subsidies)
    or `ConstructionTender` (construction projects). NO ghost wallets.
  - `attempt_pork_barrel()` takes `companies` and `regions` as arguments to find real
    economic targets.
- Add fast-track path (reduced committee delay, speaker agenda control)
- Check `state_of_emergency.can_bypass_parliament()` → skip if suspended
- Record `VoteRecord` in chamber's `recent_votes`
- Tests: bill advances through stages, amendments modify parameters, pork-barrel
  subsidies credit company cash, pork-barrel tenders appear in tender market
  votes, fast-track reduces delay, suspended parliament skips legislation

### Step 8: Wire Parliament into Political Turn
- In `politics/turn.rs`: after elections, call `initialize_parliament()` to build chambers
- Generate VIPs (Head of State, PM, Ministers, Speakers) using `names.rs`
- Call `check_faction_splintering()` each turn
- Call `StateOfEmergency.tick()` each turn
- Regenerate `political_capital` each turn
- Tests: parliament initialized after election, VIPs populated, splintering triggers

### Step 9: Parliament Building Procurement & Payroll
- In `engine/turn.rs`: add parliament building operation phase
- Wage payroll from Treasury (MPs + staff)
  - **CORRECTION 3:** MP salaries → `"Bourgeoisie"` (urban) in capital region
    (`is_capital == true`). Staff salaries → `"Worker"` (urban) in capital region.
    Direct `ClassDemographics.savings` mutation, NOT `credit_citizen_savings_region()`.
  - **CORRECTION 4:** If `liquid_reserves < total_payroll`: payroll FAILS.
    `building.condition -= 0.05`, `political_capital -= 20.0`,
    coalition `factional_tension += 0.15`. No money printed. No wages credited.
- B2B procurement for Paper, Energy, Services
- Condition degradation when goods unavailable or parliament suspended
- Tests: payroll debits treasury and credits Bourgeoisie/Worker in capital,
  bankrupt parliament fails payroll and degrades condition, procurement submits orders,
  suspension reduces costs

### Step 10: TUI Tabs + Snapshot
- Add `Tab::Government` and `Tab::Parliament` to `tabs.rs`
- Add snapshot structs to `snapshot.rs` + populate in `build_country_snapshot()`
- Add render functions to `render.rs`
- Wire hotkeys '6' and '7' in `app.rs`
- Tests: snapshot population, render functions return non-empty tables

### Step 11: Comprehensive Tests
- All new modules have focused unit tests
- Integration test: full political turn with parliament, elections, splintering, crisis
- Integration test: State of Emergency activation → parliament suspension → decree-only
  crisis response → auto-expiry → parliament resumes
- Integration test: FPTP election produces different seat distribution than D'Hondt
- Integration test: pork-barrel spending buys opposition votes

### Step 12: Build + Test Verification
- `cargo build --lib` — must succeed (warnings OK)
- `cargo test --lib` — all 620 existing tests + ~40 new Phase 32 tests must pass
- No Polish string keys in new logic
- No regression in Phase 31 crisis management (when no parliament exists, decrees still work)

---

## 4. Files to Modify

| File | Change |
|------|--------|
| `state/src/politics/names.rs` | **NEW** — cultural name pools, VIP generation |
| `state/src/politics/parliament.rs` | **NEW** — Chamber, Club, VIP, splintering |
| `state/src/politics/mod.rs` | Register `names` and `parliament` modules |
| `state/src/politics/system.rs` | Add `state_of_emergency`, `political_capital`, `parliament_struct` to `Politics` |
| `state/src/politics/crisis_management.rs` | Split decrees vs fast-track; State of Emergency bypass |
| `state/src/politics/elections.rs` | Add FPTP, Wealth/Census methods |
| `state/src/politics/bill_lifecycle.rs` | Functional legislation turn, amendments, pork-barrel, fast-track |
| `state/src/politics/committees.rs` | Amendment proposal mechanism |
| `state/src/politics/turn.rs` | Wire parliament init, VIP generation, splintering, SoE tick |
| `state/src/registries/enums.rs` | Add `Sector::Government` |
| `state/src/entities/mod.rs` | (Minimal — Building already supports any Sector) |
| `state/src/engine/turn.rs` | Parliament building procurement + payroll phase |
| `state/src/engine/generator/mod.rs` | Create Parliament building during world generation |
| `state/src/ui/tui/tabs.rs` | Add `Tab::Government`, `Tab::Parliament` |
| `state/src/ui/snapshot.rs` | Add `GovernmentSnapshot`, `ParliamentSnapshot` + population |
| `state/src/ui/tui/render.rs` | Add `render_government()`, `render_parliament()` |
| `state/src/ui/tui/app.rs` | Wire hotkeys '6', '7' |

---

## 5. Risks & Considerations

### 5.1 Backward Compatibility
- All new `Politics` fields are `Option<T>` or have `#[serde(default)]` — existing saves
  will deserialize with `None`/default values.
- The legacy `parliament: HashMap<String, u32>` field is kept. The new `parliament_struct`
  is populated alongside it during elections. Both are updated in sync.
- If `parliament_struct` is `None` (old save), the engine falls back to legacy behavior.

### 5.2 Performance
- Anonymized MP seat pools (`ParliamentaryClub.seats: u32`) — no per-MP allocation.
- VIP generation only for ~10–20 named individuals per country.
- Splintering check is O(clubs) per turn — negligible.
- Name pools are `&'static str` — zero heap allocation.

### 5.3 Phase 31 Regression
- When `parliament_struct` is `None` OR `parliament.suspended == true`, crisis management
  reverts to Phase 31 decree-only behavior. This ensures the stabilized economy doesn't
  break if the new legislative path fails.
- The fast-track path is only used when Parliament is functional AND no State of Emergency
  is active.

### 5.4 Crisis Response Timing
- Fast-track legislation takes 0–1 turns (vs instant decrees). This means crisis response
  is slightly delayed when going through Parliament. The trade-off is realism: major tax
  changes require legislative approval.
- If Parliament rejects a fast-track crisis bill, the crisis worsens. This is intentional —
  it creates political consequences for legislative obstruction.

### 5.5 Splinter Club → Party Promotion
- Splinter clubs that survive to the next election and have > 5% of seats may register as
  a new party. This is handled in `regenerate_parties()` in `politics/turn.rs`:
  - Check splinter clubs in `parliament_struct.clubs` with `is_splinter == true`.
  - If seats > 5% of total, create a new `Party` in `active_parties` with the club's
    ideology.
  - Generate party name using `generator.rs` with the splinter ideology.

### 5.6 English-Only Constraint
- All new struct fields, function names, and logic use English identifiers.
- Serde rename attributes may use Polish for save-file compatibility with existing fields,
  but new fields use English serde names.
- Committee display names in `committees.rs` are currently Polish — these will be kept
  (they are display strings, not logic keys) but new committees use English display names.

### 5.7 Double-Entry Integrity

> **CORRECTIONS 1, 3, 4 APPLIED:** All political financial flows use real economic hooks.

- **Parliament payroll:** Treasury debited (`liquid_reserves -= payroll`), specific
  `ClassDemographics.savings` credited:
  - MP salaries → `"Bourgeoisie"` (urban) in capital region (direct mutation)
  - Staff salaries → `"Worker"` (urban) in capital region (direct mutation)
  - If Treasury cannot afford payroll: NO money printed, payroll fails, condition and
    political capital degrade. (Correction 4)
- **Pork-barrel spending:** Treasury debited, real economic entities credited:
  - `CompanySubsidy`: `settle_treasury_to_company()` credits company `available_cash`/
    `brokerage_account.cash` with bank balance sheet sync. (Correction 1)
  - `ConstructionProject`: `ConstructionTender` published with Treasury encumbrance,
    creating real construction jobs. (Correction 1)
  - NO abstract "regional wallets" or "earmarked spending" counters. (Correction 1)
- **Wealth/Census elections:** Query `ClassDemographics.savings` and
  `savings_per_capita` directly — no Interest Group wealth proxies. (Correction 2)
- No money is created or destroyed in any political transaction. Every credit has a
  matching debit to a real entity (Company, Citizen Class, Bank, or Treasury).

---

## 6. Verification Plan

| Check | Method |
|-------|--------|
| Build succeeds | `cargo build --lib` (warnings OK) |
| All tests pass | `cargo test --lib` (620 existing + ~40 new) |
| No Polish keys in new logic | `grep -r "Zywnosc\|Rolnictwo" state/src/politics/names.rs state/src/politics/parliament.rs` → empty |
| Crisis decrees work without parliament | Test: country with `parliament_struct = None`, crisis response executes |
| Fast-track goes through parliament | Test: country with functional parliament, crisis creates fast-track bill |
| State of Emergency bypasses parliament | Test: SoE active with `parliament_suspended`, legislation skipped |
| SoE auto-expires | Test: `turns_remaining` reaches 0, `active = false` |
| FPTP differs from D'Hondt | Test: same party support, different seat distributions |
| Splintering reallocates seats | Test: high factional_tension + low approval → seats move to new club |
| Parliament payroll credits specific classes | Test: Treasury debited, `"Bourgeoisie"` and `"Worker"` in capital region credited (Correction 3) |
| Bankrupt parliament fails payroll | Test: `liquid_reserves < payroll` → no wages, `condition -= 0.05`, `political_capital -= 20.0` (Correction 4) |
| Pork-barrel uses real economic hooks | Test: `CompanySubsidy` credits company cash via `settle_treasury_to_company()`; `ConstructionProject` creates real `ConstructionTender` (Correction 1) |
| Wealth/Census uses ClassDemographics | Test: LandlessLaborer with 0 savings → 0 voting power in census; Aristocracy savings weight seats (Correction 2) |
| TUI tabs render | Test: `render_government()` and `render_parliament()` return non-empty |
| Save compatibility | Test: deserialize old save (no `parliament_struct`) → falls back to legacy |

---

## 7. Glossary

| Term | Definition |
|------|------------|
| **Chamber** | A legislative house (Lower House / Senate). 0–2 per country. |
| **Parliamentary Club** | An anonymized seat pool representing a voting bloc. May differ from a registered party (splinter groups). |
| **Named VIP** | A politically significant individual with a generated name (Head of State, PM, Minister, Speaker). |
| **Presidium** | The Speaker + Deputy Speakers who control the legislative agenda. |
| **Fast-Track** | Expedited legislative process for crisis bills (0–1 turn committee delay). |
| **State of Emergency** | Constitutional mechanism allowing the executive to bypass or suspend Parliament. |
| **Pork-Barrel** | Earmarked budget spending used to buy opposition votes. |
| **Splinter Event** | Mid-term defection of MPs from one club to a new club. |
| **Political Capital** | A resource spent by the ruling coalition on pork-barrel and agenda control. |

---

**END OF BLUEPRINT — AWAITING USER APPROVAL**
