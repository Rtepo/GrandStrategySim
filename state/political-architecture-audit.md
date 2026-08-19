# Political & Social Dynamics Architecture Audit

**Date:** 2026-07-23  
**Scope:** Complete audit of political, ideological, and social systems in the Rust game engine  
**Status:** Implementation analysis complete

---

## Executive Summary

The political layer is **partially implemented** with strong foundations in ideology mapping, interest group power calculations, and legislative mechanics. However, significant gaps exist in party internal mechanics, election campaigns, and leader trait systems. The engine has excellent structural scaffolding but many systems remain dormant or abstract.

---

## 1. Political Parties & Interest Groups Formation

### Party Generation

**Location:** `src/politics/turn.rs` - `regenerate_parties()` function (lines 159-255)

**Implementation Status:** ✅ FULLY IMPLEMENTED

**Mechanics:**
- Parties are generated from ideology bids based on interest group power
- Naming convention: `{country_name} {ideology_name}` (e.g., "Krasnowia Socjaldemokracja")
- Party IDs follow pattern: `[PRT-{number}]`
- Parties persist if they hold parliamentary seats or cross the support threshold
- New parties are created deterministically for ideologies crossing the threshold
- Fallback: "Tymczasowy Rząd Technokratyczny" if no parties qualify

**Code Reference:**
```rust
let name = format!("{} {}", country_name, ideo.as_str());
let mut party = Party {
    ideology: ideo.as_str().to_string(),
    profile: ideo.profile().to_string(),
    economic_school: ideo.economic_school().to_string(),
    support: bid,
    base: ideo.base_weights().iter().map(|(g, _)| g.to_string()).collect(),
    id: "[PRT-000]".to_string(),
    ..Party::default()
};
```

### Interest Group Power Calculation

**Location:** `src/politics/interest_groups.rs` - `calculate_interest_groups_power()` function (lines 21-168)

**Implementation Status:** ✅ FULLY IMPLEMENTED

**Mechanics:**
- 13 interest groups tracked: Związki Zawodowe, Kapitaliści, Drobna Burżuazja, Agrykolanie, Inteligencja, Siły Zbrojne, Duchowieństwo, Studenci, Arystokracja, Biurokraci, Specjaliści, Rzemieślnicy, Kliki Wewnętrzne
- Power calculated from: GDP, budget shares, sector shares, education, unemployment, private capital, stock confidence, Gini coefficient
- Government form modifiers (military dictatorships, theocracies, etc.) affect specific groups
- Law modifiers (religious law, emancipation law) dynamically adjust group power
- Values normalized to sum to 100.0%

**Key Formulas:**
- Unions: `(industry * 100.0) * (1.0 - unemployment) * (1.0 + illiteracy)`
- Capitalists: `(log10(private_capital) * 2.0) + (stock_confidence * 0.2) + (export_services * 50.0)`
- Clergy: `10.0 * (1.0 + (illiteracy * 2.0)) * religious_law_multiplier`

### Interest Group Membership

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** The engine does **NOT** track individual membership in interest groups. Interest groups are abstract power calculations based on economic and demographic aggregates. There is no mechanism to determine which specific populations, entities, or characters belong to a specific interest group.

**Impact:** This is a significant abstraction gap. The system assumes aggregate economic indicators directly translate to political power without individual membership tracking.

---

## 2. Internal Party Mechanics

### Party Treasuries/Financial Mechanics

**Location:** `src/politics/system.rs` - `Party` struct (lines 106-123)

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** The `Party` struct contains **NO financial fields**. Parties have:
- `ideology: String`
- `profile: String`
- `economic_school: String`
- `support: f64`
- `leader: Leader`
- `base: Vec<String>` (interest group bases)
- `id: String`

**Missing Fields:**
- No treasury/budget
- No campaign funds
- No fundraising mechanics
- No financial operations

### Internal Party Logic

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** Parties possess **NO internal operational logic**:

**Missing Mechanics:**
- **Party Discipline:** No whipping system, no enforcement of party line votes
- **Internal Factionalism:** No internal factions within parties, no factional conflicts
- **Leadership Elections:** No internal leadership challenges, no leadership succession mechanics
- **Party Organization:** No local party branches, no party infrastructure

**Impact:** Parties are essentially static containers for ideology + support + leader. They behave as monolithic blocks without internal dynamics.

---

## 3. Lobbying & Mass Movements

### Trade Unions

**Location:** `src/entities/union.rs` and `src/corporate/unions.rs`

**Implementation Status:** ✅ FULLY IMPLEMENTED

**Mechanics:**
- Unions are first-class entities with:
  - Budget and strike fund
  - Political power (0-100)
  - Militancy (0-1)
  - Wage and safety demands
  - Company membership tracking
- Union scales: Company, Sector, Regional, National
- Strike mechanics: reduce company capacity, increase social unrest
- Union dues collection from member companies
- Member recruitment based on economic conditions

**Code Reference:**
```rust
pub struct Union {
    pub budget: f64,
    pub strike_fund: f64,
    pub political_power: f64,
    pub militancy: f64,
    pub wage_demand: f64,
    pub company_ids: BTreeSet<String>,
    // ...
}
```

### Other Lobbying Groups

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** Aside from Trade Unions, the engine implements **NO other distinct lobbying groups**:

**Missing Entities:**
- No NGOs (non-governmental organizations)
- No business associations (chambers of commerce, industry groups)
- No religious organizations (beyond abstract clergy power)
- No environmental groups
- No civil rights organizations
- No professional associations (beyond unions)

### Grassroots Mass Movements

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** The engine has **NO grassroots mass movement mechanics**:

**Missing Systems:**
- No protest movements
- No social movements (civil rights, environmental, etc.)
- No grassroots organizing
- No movement momentum tracking
- No movement-to-party conversion pathways

**Abstract Alternative:** The `mass_movements` field exists in `Politics` struct as `Vec<Value>` but is currently dormant storage only.

---

## 4. Election Campaigns

### Campaign Mechanics

**Location:** `src/politics/turn.rs` - election processing (lines 65-97)

**Implementation Status:** ❌ NOT IMPLEMENTED

**Finding:** Elections are **instant calculations** with NO campaign mechanics:

**Missing Systems:**
- **Campaign Funding:** No fundraising, no budget allocation for campaigns
- **Campaign Momentum:** No polling, no momentum tracking
- **Rallies/Events:** No campaign events, no rally mechanics
- **Campaign Turns:** No dedicated campaign phase - elections trigger instantly
- **Campaign Strategy:** No targeting of demographics, no ad spending

**Current Implementation:**
```rust
if form.is_democratic() && election_due {
    let method = country.politics.election_method.clone();
    let threshold = country.politics.election_threshold;
    let seats = elections::calculate_seats(&country.politics.active_parties, &method, threshold, 100);
    country.politics.parliament = seats;
    // Instant coalition formation
}
```

**Impact:** Elections are purely mathematical seat allocations based on party support, with no strategic campaign layer.

---

## 5. Ideologies & Economic Schools (Gap Analysis)

### Ideology Mapping

**Location:** `src/politics/ideology.rs` - `Ideology` enum and `preferences()` method (lines 29-206)

**Implementation Status:** ✅ FULLY IMPLEMENTED - NO GAPS

**Ideologies (15 total):**
1. OrthodoxMarxism (Ortodoksyjny Marksizm)
2. MarxismLeninism (Marksizm-Leninizm)
3. Maoism (Maoizm)
4. SocialDemocracy (Socjaldemokracja)
5. GreenPolitics (Zielona Polityka)
6. ClassicalLiberalism (Klasyczny Liberalizm)
7. SocialLiberalism (Socjalliberalizm)
8. Agrarianism (Agraryzm)
9. ChristianDemocracy (Chrześcijańska Demokracja)
10. SocialConservatism (Konserwatyzm Społeczny)
11. Neoconservatism (Neokonserwatyzm)
12. Neoliberalism (Neoliberalizm)
13. NationalConservatism (Konserwatyzm Narodowy)
14. AnarchoCapitalism (Anarchokapitalizm)
15. Fascism (Faszyzm)

### Policy Preferences

**Implementation Status:** ✅ FULLY IMPLEMENTED - NO GAPS

**All ideologies define stances on 12 policies:**
- `religion` (Laicyzm, Państwowa, Państwowy Ateizm, Tolerancja)
- `citizenship` (Asymilacja 5/10 lat, Ziemia 3/5 lat, Krew, Brak)
- `electoral_system` (Hare-Niemeyer, Sainte-Laguë, D'Hondt, Brak)
- `trade_doctrine` (Protekcjonizm, Wolny Handel, Autarkia)
- `labor_law` (Ochrona Pracowników, Elastyczne, Państwowe)
- `health_service` (Publiczna, Prywatna, Składkowa)
- `sanitation` (Restrykcyjny, Standardowy, Luźny)
- `union_law` (Wolne, Ograniczone, Państwowe)
- `strike_law` (Dozwolone, Ograniczone, Zakazane)
- `education_model` (Publiczny Bezpłatny, Prywatny, Publiczny Mieszany, Państwowy Ideologiczny)
- `school_system` (Gimnazjalny, 8-klasowy)
- `emancipation` (Pełna Emancypacja, Prawa Majątkowe, Tradycjonalizm)

**Gap Analysis Result:** ✅ **NO GAPS FOUND** - Every ideology has a defined stance for every policy. No `None` or undefined values.

### Economic Schools

**Location:** `src/politics/ideology.rs` - `economic_school()` method (lines 208-220)

**Implementation Status:** ✅ FULLY IMPLEMENTED

**Economic Schools (7 total):**
1. Marksistowska (OrthodoxMarxism, MarxismLeninism, Maoism, Fascism)
2. Klasyczna (ClassicalLiberalism)
3. Keynesowska (SocialDemocracy, SocialLiberalism, GreenPolitics)
4. Interwencjonizm Państwowy (Agrarianism, Neoconservatism)
5. Narodowy Solidaryzm (ChristianDemocracy, SocialConservatism, NationalConservatism)
6. Austriacka (Neoliberalism)
7. Monetarystyczna (AnarchoCapitalism)

**Historical Zeitgeist Multipliers:**
- Pre-1930: Klasyczna gets 1.5x multiplier
- 1930-1970: Keynesowska gets 1.8x multiplier
- Post-1970: Monetarystyczna and Austriacka get 2.0x multiplier
- Fascism: 1.5x multiplier 1920-1945, 0.1x multiplier post-1945

**Gap Analysis Result:** ✅ **NO GAPS FOUND** - All ideologies map to economic schools.

---

## 6. Character Traits

### Leader Traits

**Location:** `src/politics/system.rs` - `Leader` struct (lines 75-104)

**Implementation Status:** ⚠️ DORMANT (STRUCT EXISTS, NO MECHANICS)

**Struct Definition:**
```rust
pub struct Leader {
    pub traits: Vec<String>,        // e.g., "Charyzmatyczny", "Dyplomatyczny"
    pub main_trait: String,         // e.g., "Praworządność"
    // ... other fields
}
```

**Mechanical Implementation:** ❌ **NOT HOOKED INTO SIMULATION**

**Finding:** Leader traits are stored as strings but have **NO mechanical effects**:

**Missing Mechanics:**
- No stat modifications from traits
- No influence on political negotiations
- No impact on AI decision-making weights
- No effect on party popularity or stability
- No effect on international relations
- No effect on economic policy effectiveness

**Current Usage:** Traits are only set during `bootstrap_politics()` and never read or used elsewhere in the simulation.

### Councilor Traits

**Location:** `src/politics/local_council.rs` - `CouncilorTrait` enum (lines 114-127)

**Implementation Status:** ✅ FULLY IMPLEMENTED AND MECHANICALLY ACTIVE

**Traits (4 total):**
1. **Loyalist** - Always votes party line (90%+ probability)
2. **Undecided** - Can be swayed by concessions (+30% if concession offered)
3. **Corrupt** - Can be bribed (+40% if bribed) or blackmailed (+20% if blackmailed)
4. **Maverick** - Votes based on ideological alignment with randomness

**Mechanical Implementation:**
- Traits affect vote probability in `calculate_vote_probability()` (lines 376-411)
- Traits are used in floor voting in `src/politics/bill_lifecycle.rs` (lines 64-124)
- Espionage system can reveal Corrupt trait via surveillance (src/politics/espionage.rs)
- Blackmail material can be generated for corrupt councilors

**Code Reference:**
```rust
pub fn calculate_vote_probability(
    councilor: &Councilor,
    concession_offered: bool,
    ideological_alignment: f64,
    bribed: bool,
    blackmailed: bool,
) -> f64 {
    match councilor.hidden_trait {
        CouncilorTrait::Loyalist => 0.9 + rand::random::<f64>() * 0.1,
        CouncilorTrait::Undecided => {
            let mut probability: f64 = 0.5;
            if concession_offered { probability += 0.3; }
            probability += ideological_alignment * 0.2;
            probability.min(1.0)
        }
        CouncilorTrait::Corrupt => {
            let mut probability: f64 = 0.4;
            if bribed { probability += 0.4; }
            if blackmailed { probability += 0.2; }
            probability.min(1.0)
        }
        CouncilorTrait::Maverick => {
            ideological_alignment + (rand::random::<f64>() - 0.5) * 0.3
        }
    }
}
```

**Impact:** Councilor traits are the **only character trait system with actual mechanical implementation** in the engine.

---

## Summary Matrix

| System | Status | Implementation Quality |
|--------|--------|------------------------|
| Party Generation | ✅ Complete | Deterministic, ideology-based |
| Interest Group Power | ✅ Complete | Sophisticated economic modeling |
| Interest Group Membership | ❌ Missing | Abstract only, no individual tracking |
| Party Treasuries | ❌ Missing | No financial mechanics |
| Party Internal Logic | ❌ Missing | No discipline, factions, elections |
| Trade Unions | ✅ Complete | Full entity system with strikes |
| Other Lobbying Groups | ❌ Missing | No NGOs, business associations |
| Mass Movements | ❌ Missing | No grassroots movements |
| Election Campaigns | ❌ Missing | Instant calculations only |
| Ideology Mapping | ✅ Complete | All 15 ideologies, no gaps |
| Economic Schools | ✅ Complete | All 7 schools, historical multipliers |
| Leader Traits | ⚠️ Dormant | Struct exists, no mechanics |
| Councilor Traits | ✅ Complete | Full voting mechanics |

---

## Critical Gaps Requiring Development

### High Priority
1. **Leader Trait Mechanics** - Convert dormant string traits into stat modifiers and AI weights
2. **Party Internal Dynamics** - Add party discipline, factionalism, and leadership elections
3. **Election Campaigns** - Implement campaign funding, momentum, and strategic targeting

### Medium Priority
4. **Interest Group Membership** - Track individual membership for more granular power calculations
5. **Alternative Lobbying Groups** - Add NGOs, business associations, religious organizations
6. **Mass Movements** - Implement grassroots organizing and movement-to-party pathways

### Low Priority
7. **Party Treasuries** - Add financial mechanics to parties (if campaign funding is implemented)

---

## Conclusion

The political architecture has **strong foundations** with excellent ideology mapping, sophisticated interest group power calculations, and a fully functional legislative system with councilor traits. However, the engine suffers from **significant abstraction gaps** in party internal mechanics, election campaigns, and leader traits. The current implementation treats parties as monolithic blocks and elections as instant calculations, missing the strategic depth that would come from campaign mechanics and internal party dynamics.

**Overall Assessment:** 60% complete - structural scaffolding is excellent, but many systems remain dormant or abstract.
