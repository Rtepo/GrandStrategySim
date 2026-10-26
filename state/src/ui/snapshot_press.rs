//! v4.22 WS6.3: Press — deterministic newspaper front pages generated from
//! real simulation events only.
//!
//! No article is invented: every story is a deterministic pure function of a
//! real, persisted fact. Sources:
//!
//! * `country.politics.history` — the JSON event log written by the
//!   legislature (`floor_vote`, `veto_override_vote` in
//!   `politics/bill_lifecycle.rs`, `budget_bill` in `engine/turn.rs`).
//! * `region.governance.council.resolutions` — real regional council votes
//!   (`politics/local_council.rs::CouncilResolution`, bounded 96-entry log).
//! * `country.politics.royal_dynasty.birth_history` / `marriage_history`
//!   (Phase 86 genealogy events, names resolved via `vip_registry`).
//! * `CountryEntities.unions` — unions currently `on_strike`.
//! * `country.politics.mass_movements` — Forming/Active/Negotiating
//!   movements (`start_turn`, `participant_count`, demands).
//! * `GameState.treaty_registry` — treaties the country participates in
//!   (`signed_turn`, computed expiry turn, current status).
//! * `country.at_war_with` / `is_rebellion` — live war state (reported at
//!   the current turn as an ongoing bulletin — real current state).
//! * `country.macro_indicators.telemetry_history` — ToT GDP/CPI/unemployment
//!   deltas (the same `tot_pct` math the macro dashboard uses); an economy
//!   article appears only when a real threshold is crossed.
//! * `country.macro_indicators.election_turn` — the scheduled election
//!   (forward-looking notice of a real, persisted date).
//!
//! Slant model: each event is re-phrased per paper. Facts (numbers, names,
//! outcomes) are identical across papers — only framing differs:
//! * `official` (State Gazette): government voice — bills "enacted",
//!   strikes are "disruptions", royal events celebrated, treaties triumphs.
//! * `socialist` (Workers' Tribune): labor lens — strikes/movements are
//!   struggles, budgets judged by worker welfare, royal events dismissed.
//! * `conservative` (Herald): order & tradition — royal events front-page,
//!   unrest framed as disorder, treaties as strength.
//! * `liberal` (Free Press): market & institutions — treaties/economics
//!   lead, elections covered prominently, strikes as bargaining.
//!
//! Determinism: where a template has variants, the pick is
//! `hash(paper_id | event_ref)` — stable across saves and platforms
//! (DefaultHasher with fixed keys, the same technique `politics/
//! succession.rs` uses for deterministic birth rolls). No RNG anywhere.
//!
//! Known gaps (documented, never fabricated):
//! * Election *results* are not surfaced — no persisted event records the
//!   turn a national election completed (only `election_turn` forward
//!   marker + `mandate_seats` snapshot exist), so only the scheduled
//!   election notice is emitted.
//! * Suspended/abrogated treaties carry no transition turn — they are
//!   reported at the current turn as live status.
//! * Battle outcomes/disasters have no per-turn event log on state — not
//!   reported.

use crate::politics::free_speech::{FreeSpeechLevel, PressFreedom};
use crate::politics::mass_movements::{MassMovementStatus, MassMovementType};
use crate::state::Country;
use serde_json::Value;
use std::hash::{Hash, Hasher};

/// Articles per paper front page (cap keeps history bounded ~50 total).
const FRONT_PAGE_SIZE: usize = 8;
/// Only events from the last N turns are eligible for front pages
/// (royal/council/history logs can stretch back further).
const NEWS_WINDOW_TURNS: u32 = 96;

// ============================================================================
// DTOs
// ============================================================================

/// One newspaper article — a deterministic rendering of a real event.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PressArticleDto {
    /// Stable article id: `{paper_id}:{event_ref}`.
    pub id: String,
    /// Owning paper (`gazette` | `socialist` | `conservative` | `liberal`).
    pub paper_id: String,
    /// Headline (slant-phrased; facts identical across papers).
    pub headline: String,
    /// Article body (real numbers/names + one slant-flavored line).
    pub body: String,
    /// Event category: "floor_vote" | "veto_override" | "budget_bill" |
    /// "council_resolution" | "royal_birth" | "royal_marriage" | "strike" |
    /// "mass_movement" | "treaty" | "war" | "economy" | "election_scheduled".
    pub event_kind: String,
    /// Deterministic event reference for frontend linking/dedup
    /// (e.g. "floor_vote:BILL-12:187", "royal_birth:VIP-91").
    pub event_ref: String,
    /// Global turn the underlying event happened (or its live state).
    pub turn: u32,
}

/// A newspaper front page: the paper's editorial selection of real events.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PressPaperDto {
    /// Paper id (`gazette` | `socialist` | `conservative` | `liberal`).
    pub paper_id: String,
    /// Display name (deterministic per country).
    pub name: String,
    /// Ideological slant ("official" | "socialist" | "conservative" | "liberal").
    pub ideological_slant: String,
    /// Legality under the country's real `FreeSpeechLaw`/`PressFreedom`:
    /// "legal" | "restricted" | "underground".
    pub status: String,
    /// Articles, most editorially relevant first.
    pub articles: Vec<PressArticleDto>,
}

/// Real media-freedom context from `politics.media_state` / `free_speech_law`.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PressMediaContext {
    /// State media share of information production (0.0–1.0), when tracked.
    pub state_media_share: Option<f64>,
    /// Whether a propaganda campaign is currently active.
    pub propaganda_active: bool,
    /// Last turn's information consumption ratio (0.0–1.0), when tracked.
    pub information_consumption_ratio: Option<f64>,
    /// `free_speech_law.free_speech_level` ("full" | "restricted" |
    /// "suppressed" | "totalitarian"; "full" when no law is set).
    pub free_speech_level: String,
    /// `free_speech_law.press_freedom` ("independent" | "mixed" |
    /// "state_controlled"; "independent" when no law is set).
    pub press_freedom: String,
    /// `free_speech_law.assembly_rights` ("free" | "restricted" | "banned";
    /// "free" when no law is set).
    pub assembly_rights: String,
}

/// Per-country press snapshot: four deterministic front pages + media context.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PressSnapshotDto {
    /// Country name.
    pub country: String,
    /// Global turn the snapshot was generated at.
    pub current_turn: u32,
    /// Sim year.
    pub year: u32,
    /// Real media-freedom context.
    pub media: PressMediaContext,
    /// Front pages (one per ideological paper).
    pub papers: Vec<PressPaperDto>,
    /// Total articles emitted across all papers.
    pub total_articles: usize,
}

// ============================================================================
// EVENT COLLECTION (real sources only)
// ============================================================================

/// Internal event kind — also the DTO `event_kind` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewsKind {
    FloorVote,
    VetoOverride,
    BudgetBill,
    CouncilResolution,
    RoyalBirth,
    RoyalMarriage,
    Strike,
    MassMovement,
    Treaty,
    War,
    Economy,
    ElectionScheduled,
}

impl NewsKind {
    fn as_str(&self) -> &'static str {
        match self {
            NewsKind::FloorVote => "floor_vote",
            NewsKind::VetoOverride => "veto_override",
            NewsKind::BudgetBill => "budget_bill",
            NewsKind::CouncilResolution => "council_resolution",
            NewsKind::RoyalBirth => "royal_birth",
            NewsKind::RoyalMarriage => "royal_marriage",
            NewsKind::Strike => "strike",
            NewsKind::MassMovement => "mass_movement",
            NewsKind::Treaty => "treaty",
            NewsKind::War => "war",
            NewsKind::Economy => "economy",
            NewsKind::ElectionScheduled => "election_scheduled",
        }
    }
}

/// A normalized real event ready for per-paper rendering.
#[derive(Debug, Clone)]
struct NewsEvent {
    kind: NewsKind,
    /// Turn the event happened (or current turn for live-state items).
    turn: u32,
    /// Deterministic reference (dedup + frontend linking).
    event_ref: String,
    /// Primary noun: bill title / treaty name / union name / child name /
    /// region name / movement label.
    subject: String,
    /// Secondary noun: counterparty (spouse pair, enemy list, region
    /// display name, vote-margin text).
    object: String,
    /// Votes for / adopted margin numerator.
    votes_for: f64,
    /// Votes against.
    votes_against: f64,
    /// Abstentions.
    abstentions: f64,
    /// Whether the underlying real outcome was positive
    /// (passed / adopted / enacted / legitimate birth / treaty active).
    positive: bool,
    /// Magnitude: participants, striking workers, pct change, etc.
    magnitude: f64,
}

/// Resolve a VIP id to a display name (falls back to the raw id — still
/// the real identifier, never a fabricated name).
fn vip_name(country: &Country, vip_id: &str) -> String {
    country
        .politics
        .vip_registry
        .as_ref()
        .and_then(|r| r.get(vip_id))
        .map(|v| v.full_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| vip_id.to_string())
}

/// Display name for a region id.
fn region_name(country: &Country, region_id: &str) -> String {
    country
        .regions
        .iter()
        .find(|r| r.id == region_id)
        .map(|r| {
            if r.display_name.is_empty() {
                r.id.clone()
            } else {
                r.display_name.clone()
            }
        })
        .unwrap_or_else(|| region_id.to_string())
}

/// Collect every eligible real event for a country.
///
/// * `unions` — the country's union slice (`CountryEntities::unions`).
/// * `treaties` — `GameState::treaty_registry`.
/// * `current_turn` / `current_year` — `GameState::calendar`.
fn collect_events(
    country: &Country,
    companies: &[crate::entities::Company],
    unions: &[crate::entities::union::Union],
    treaties: &crate::international::treaties::TreatyRegistry,
    current_turn: u32,
    _current_year: u32,
) -> Vec<NewsEvent> {
    let mut events: Vec<NewsEvent> = Vec::new();
    let window_start = current_turn.saturating_sub(NEWS_WINDOW_TURNS);

    // -- 1. Parliamentary JSON event log ------------------------------------
    for entry in &country.politics.history {
        let Some(obj) = entry.as_object() else { continue };
        let kind = obj.get("event").and_then(Value::as_str).unwrap_or("");
        let turn = obj
            .get("turn")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        if turn < window_start {
            continue;
        }
        let num = |key: &str| obj.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        match kind {
            "floor_vote" => {
                let bill_id = obj
                    .get("bill_id")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                events.push(NewsEvent {
                    kind: NewsKind::FloorVote,
                    turn,
                    event_ref: format!("floor_vote:{bill_id}:{turn}"),
                    subject: obj
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Untitled bill")
                        .to_string(),
                    object: obj
                        .get("bill_type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    votes_for: num("votes_for"),
                    votes_against: num("votes_against"),
                    abstentions: num("abstentions"),
                    positive: obj
                        .get("passed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    magnitude: 0.0,
                });
            }
            "veto_override_vote" => {
                let bill_id = obj
                    .get("bill_id")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                events.push(NewsEvent {
                    kind: NewsKind::VetoOverride,
                    turn,
                    event_ref: format!("veto_override:{bill_id}:{turn}"),
                    subject: obj
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Untitled bill")
                        .to_string(),
                    object: String::new(),
                    votes_for: num("votes_for"),
                    votes_against: num("votes_against"),
                    abstentions: num("abstentions"),
                    positive: obj
                        .get("passed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    magnitude: 0.0,
                });
            }
            "budget_bill" => {
                events.push(NewsEvent {
                    kind: NewsKind::BudgetBill,
                    turn,
                    event_ref: format!("budget_bill:{turn}"),
                    subject: "the national budget".to_string(),
                    object: String::new(),
                    votes_for: 0.0,
                    votes_against: 0.0,
                    abstentions: 0.0,
                    positive: obj
                        .get("enacted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    magnitude: 0.0,
                });
            }
            _ => {}
        }
    }

    // -- 2. Regional council resolutions ------------------------------------
    for region in &country.regions {
        let Some(gov) = &region.governance else { continue };
        for res in &gov.council.resolutions {
            if res.turn < window_start {
                continue;
            }
            events.push(NewsEvent {
                kind: NewsKind::CouncilResolution,
                turn: res.turn,
                event_ref: format!(
                    "council:{}:{}:{}",
                    region.id, res.title, res.turn
                ),
                subject: res.title.clone(),
                object: region_name(country, &region.id),
                votes_for: res.votes_for as f64,
                votes_against: res.votes_against as f64,
                abstentions: res.abstentions as f64,
                positive: res.status == "adopted",
                magnitude: 0.0,
            });
        }
    }

    // -- 3. Royal dynasty genealogy events ----------------------------------
    if let Some(dynasty) = &country.politics.royal_dynasty {
        for birth in &dynasty.birth_history {
            if birth.turn < window_start {
                continue;
            }
            let child = vip_name(country, &birth.child_vip_id);
            let father = vip_name(country, &birth.father_vip_id);
            let mother = vip_name(country, &birth.mother_vip_id);
            events.push(NewsEvent {
                kind: NewsKind::RoyalBirth,
                turn: birth.turn,
                event_ref: format!("royal_birth:{}", birth.child_vip_id),
                subject: child,
                object: format!("{father} and {mother}"),
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: birth.is_legitimate,
                magnitude: 0.0,
            });
        }
        for marriage in &dynasty.marriage_history {
            if marriage.turn < window_start {
                continue;
            }
            let s1 = vip_name(country, &marriage.spouse1_vip_id);
            let s2 = vip_name(country, &marriage.spouse2_vip_id);
            let foreign = marriage.foreign_dynasty.clone().unwrap_or_default();
            events.push(NewsEvent {
                kind: NewsKind::RoyalMarriage,
                turn: marriage.turn,
                event_ref: format!(
                    "royal_marriage:{}:{}:{}",
                    marriage.spouse1_vip_id, marriage.spouse2_vip_id, marriage.turn
                ),
                subject: format!("{s1} and {s2}"),
                object: if foreign.is_empty() {
                    format!("{:?}", marriage.political_significance)
                } else {
                    foreign
                },
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: true,
                magnitude: 0.0,
            });
        }
    }

    // -- 4. Strikes (live state) --------------------------------------------
    for union in unions {
        if !union.on_strike {
            continue;
        }
        // Real striking workforce: fulfilled FTE at companies covered by
        // this union (same membership resolution as the organizations
        // snapshot — `company.union_id` or `union.company_ids`).
        let workers: u64 = companies
            .iter()
            .filter(|c| {
                c.union_id.as_deref() == Some(union.id.as_str())
                    || union.company_ids.contains(&c.id)
            })
            .map(|c| c.fulfilled_fte as u64)
            .sum();
        events.push(NewsEvent {
            kind: NewsKind::Strike,
            turn: union.last_strike_turn.unwrap_or(current_turn),
            event_ref: format!("strike:{}", union.id),
            subject: if union.name.is_empty() {
                union.id.clone()
            } else {
                union.name.clone()
            },
            object: format!("{:?}", union.sector),
            votes_for: 0.0,
            votes_against: 0.0,
            abstentions: 0.0,
            positive: true,
            magnitude: workers as f64,
        });
    }

    // -- 5. Mass movements (live state) -------------------------------------
    for movement in &country.politics.mass_movements {
        use MassMovementStatus::*;
        if !matches!(movement.status, Forming | Active | Negotiating) {
            continue;
        }
        let mtype = match movement.movement_type {
            MassMovementType::IndustrialStrike => "industrial strike",
            MassMovementType::Riot => "riot",
            MassMovementType::PeacefulProtest => "protest march",
            MassMovementType::Occupation => "occupation",
            MassMovementType::Boycott => "boycott",
        };
        events.push(NewsEvent {
            kind: NewsKind::MassMovement,
            turn: movement.start_turn,
            event_ref: format!("movement:{}", movement.id),
            subject: mtype.to_string(),
            object: format!(
                "{} ({:?} status, demands: {})",
                region_name(country, &movement.region_id),
                movement.status,
                if movement.demands.is_empty() {
                    "unspecified".to_string()
                } else {
                    movement.demands.join("; ")
                }
            ),
            votes_for: 0.0,
            votes_against: 0.0,
            abstentions: 0.0,
            positive: false,
            magnitude: movement.participant_count as f64,
        });
    }

    // -- 6. Treaties ---------------------------------------------------------
    for treaty in &treaties.treaties {
        if !treaty
            .participants
            .iter()
            .any(|p| p == &country.name)
        {
            continue;
        }
        use crate::international::treaties::TreatyStatus::*;
        match treaty.status {
            Active => {
                if let Some(signed) = treaty.signed_turn {
                    if signed >= window_start {
                        events.push(NewsEvent {
                            kind: NewsKind::Treaty,
                            turn: signed,
                            event_ref: format!("treaty:{}:signed", treaty.id),
                            subject: treaty.name.clone(),
                            object: treaty.participants.join(", "),
                            votes_for: 0.0,
                            votes_against: 0.0,
                            abstentions: 0.0,
                            positive: true,
                            magnitude: treaty.clauses.len() as f64,
                        });
                    }
                }
            }
            Suspended | Abrogated => {
                // No transition turn is persisted — report the live status
                // at the current turn (real current state).
                events.push(NewsEvent {
                    kind: NewsKind::Treaty,
                    turn: current_turn,
                    event_ref: format!("treaty:{}:status", treaty.id),
                    subject: treaty.name.clone(),
                    object: treaty.status.as_str().to_string(),
                    votes_for: 0.0,
                    votes_against: 0.0,
                    abstentions: 0.0,
                    positive: false,
                    magnitude: treaty.clauses.len() as f64,
                });
            }
            Expired => {
                // Expiry turn is derivable: signed_turn + duration_turns.
                if let Some(signed) = treaty.signed_turn {
                    let expired_turn = signed + treaty.duration_turns;
                    if expired_turn >= window_start {
                        events.push(NewsEvent {
                            kind: NewsKind::Treaty,
                            turn: expired_turn,
                            event_ref: format!("treaty:{}:expired", treaty.id),
                            subject: treaty.name.clone(),
                            object: treaty.participants.join(", "),
                            votes_for: 0.0,
                            votes_against: 0.0,
                            abstentions: 0.0,
                            positive: false,
                            magnitude: treaty.clauses.len() as f64,
                        });
                    }
                }
            }
            Proposed | Negotiating => {}
        }
    }

    // -- 7. War (live state) -------------------------------------------------
    if !country.at_war_with.is_empty() {
        let mut enemies = country.at_war_with.clone();
        enemies.sort();
        let object = if country.is_rebellion {
            format!(
                "{} (war of secession from {})",
                enemies.join(", "),
                country.mother_country.clone().unwrap_or_default()
            )
        } else {
            enemies.join(", ")
        };
        events.push(NewsEvent {
            kind: NewsKind::War,
            turn: current_turn,
            event_ref: format!("war:{}", enemies.join("+")),
            subject: country.name.clone(),
            object,
            votes_for: 0.0,
            votes_against: 0.0,
            abstentions: 0.0,
            positive: false,
            magnitude: enemies.len() as f64,
        });
    }

    // -- 8. Economy (real telemetry deltas) ----------------------------------
    {
        let md = &country.macro_indicators;
        let hist = &md.telemetry_history;
        let gdp_tot = hist.tot_pct(md.gdp_breakdown.official_gdp, |s| s.official_gdp);
        let cpi_tot = hist.tot_pct(md.inflation_indices.cpi_index, |s| s.cpi_index);
        let unemp_delta = hist
            .previous_turn()
            .map(|s| md.labor_market.unemployment_rate - s.unemployment_pct);
        let gdp_shock = gdp_tot.map(|d| d.abs() > 1.5).unwrap_or(false);
        let cpi_shock = cpi_tot.map(|d| d.abs() > 1.0).unwrap_or(false);
        let unemp_shock = unemp_delta.map(|d| d.abs() > 1.0).unwrap_or(false);
        if gdp_shock || cpi_shock || unemp_shock {
            let mut parts = Vec::new();
            if let Some(d) = gdp_tot {
                if gdp_shock {
                    parts.push(format!("GDP {:+.1}%", d));
                }
            }
            if let Some(d) = cpi_tot {
                if cpi_shock {
                    parts.push(format!("consumer prices {:+.1}%", d));
                }
            }
            if let Some(d) = unemp_delta {
                if unemp_shock {
                    parts.push(format!("unemployment {:+.1}pp", d));
                }
            }
            let rising = gdp_tot.unwrap_or(0.0) > 0.0;
            events.push(NewsEvent {
                kind: NewsKind::Economy,
                turn: current_turn,
                event_ref: format!("econ:{current_turn}"),
                subject: parts.join(", "),
                object: format!(
                    "GDP {:.0}, CPI {:.1}, unemployment {:.1}%",
                    md.gdp_breakdown.official_gdp,
                    md.inflation_indices.cpi_index,
                    md.labor_market.unemployment_rate
                ),
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: rising && !cpi_shock,
                magnitude: gdp_tot.unwrap_or(0.0),
            });
        }
    }

    // -- 9. Scheduled election (real forward marker) --------------------------
    {
        let et = country.macro_indicators.election_turn;
        if et > current_turn && et - current_turn <= 48 {
            events.push(NewsEvent {
                kind: NewsKind::ElectionScheduled,
                turn: et,
                event_ref: format!("election_sched:{et}"),
                subject: "the next general election".to_string(),
                object: format!("turn {et}"),
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: true,
                magnitude: (et - current_turn) as f64,
            });
        }
    }

    // Deterministic ordering: newest first, ties by event_ref.
    events.sort_by(|a, b| {
        b.turn
            .cmp(&a.turn)
            .then_with(|| a.event_ref.cmp(&b.event_ref))
    });
    events
}

// ============================================================================
// SLANT MODEL
// ============================================================================

/// Editorial weight per (paper, kind) — which stories each paper leads with.
fn editorial_weight(paper: &str, kind: NewsKind) -> u32 {
    use NewsKind::*;
    match (paper, kind) {
        ("gazette", BudgetBill) => 5,
        ("gazette", FloorVote | VetoOverride | Treaty | War | ElectionScheduled) => 4,
        ("gazette", CouncilResolution | RoyalBirth | RoyalMarriage | Economy) => 3,
        ("gazette", MassMovement) => 2,
        ("gazette", Strike) => 1,

        ("socialist", Strike | MassMovement) => 5,
        ("socialist", Economy) => 4,
        ("socialist", FloorVote | VetoOverride | BudgetBill | War) => 3,
        ("socialist", CouncilResolution | Treaty | ElectionScheduled) => 2,
        ("socialist", RoyalBirth | RoyalMarriage) => 1,

        ("conservative", RoyalBirth | RoyalMarriage | War) => 5,
        ("conservative", VetoOverride | Treaty | ElectionScheduled) => 4,
        ("conservative", FloorVote | BudgetBill | Economy) => 3,
        ("conservative", CouncilResolution | MassMovement) => 2,
        ("conservative", Strike) => 1,

        ("liberal", Economy | Treaty | ElectionScheduled) => 5,
        ("liberal", FloorVote | War) => 4,
        ("liberal", VetoOverride | BudgetBill | Strike | MassMovement) => 3,
        ("liberal", CouncilResolution) => 2,
        ("liberal", RoyalBirth | RoyalMarriage) => 1,

        _ => 1,
    }
}

/// Deterministic variant picker — `hash(seed)` with fixed-key hasher.
fn pick(seed: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut h);
    (h.finish() as usize) % n
}

fn fmt_votes(e: &NewsEvent) -> String {
    format!(
        "{} for, {} against, {} abstentions",
        e.votes_for as u64, e.votes_against as u64, e.abstentions as u64
    )
}

/// Slant-phrased headline. All variants report the same real fact.
fn headline_for(paper: &str, e: &NewsEvent) -> String {
    let v = pick(&format!("{paper}|{}|h", e.event_ref), 2);
    use NewsKind::*;
    match (paper, e.kind) {
        ("gazette", FloorVote) => {
            if e.positive {
                if v == 0 {
                    format!("Parliament Enacts “{}”", e.subject)
                } else {
                    format!("“{}” Approved by Chamber", e.subject)
                }
            } else if v == 0 {
                format!("Chamber Declines “{}”", e.subject)
            } else {
                format!("“{}” Fails to Secure Majority", e.subject)
            }
        }
        ("gazette", VetoOverride) => {
            if e.positive {
                format!("Chamber Overrides Veto on “{}”", e.subject)
            } else {
                format!("Veto on “{}” Stands", e.subject)
            }
        }
        ("gazette", BudgetBill) => {
            if e.positive {
                "National Budget Enacted for the Coming Term".to_string()
            } else {
                "Budget Session Adjourns Without Settlement".to_string()
            }
        }
        ("gazette", CouncilResolution) => {
            if e.positive {
                format!("{} Council Adopts “{}”", e.object, e.subject)
            } else {
                format!("{} Council Rejects “{}”", e.object, e.subject)
            }
        }
        ("gazette", RoyalBirth) => format!("Court Announces the Birth of {}", e.subject),
        ("gazette", RoyalMarriage) => format!("Royal Union: {}", e.subject),
        ("gazette", Strike) => format!("Work Stoppage at {} Under Review", e.subject),
        ("gazette", MassMovement) => format!("Public Order Situation in {}", e.object),
        ("gazette", Treaty) => {
            if e.positive {
                format!("{} Signs “{}”", "the Realm", e.subject)
            } else {
                format!("Treaty Status Update: “{}” ({})", e.subject, e.object)
            }
        }
        ("gazette", War) => format!("Defense of the Realm Continues — {}", e.object),
        ("gazette", Economy) => format!("Economic Bulletin: {}", e.subject),
        ("gazette", ElectionScheduled) => {
            format!("Elections to Convene as Scheduled ({})", e.object)
        }

        ("socialist", FloorVote) => {
            if e.positive {
                format!("“{}” Passes — Who Pays the Price?", e.subject)
            } else {
                format!("Chamber Buried “{}” — Workers Take Note", e.subject)
            }
        }
        ("socialist", VetoOverride) => {
            if e.positive {
                format!("Deputies Break the Veto on “{}”", e.subject)
            } else {
                format!("Veto Strangles “{}”", e.subject)
            }
        }
        ("socialist", BudgetBill) => {
            if e.positive {
                "Budget Signed — Count the Crumbs for Workers".to_string()
            } else {
                "Budget Collapses — Ministers Count Coins, Not Need".to_string()
            }
        }
        ("socialist", CouncilResolution) => {
            if e.positive {
                format!("{} Votes “{}” — A Local Win Worth Watching", e.object, e.subject)
            } else {
                format!("{} Turns Down “{}”", e.object, e.subject)
            }
        }
        ("socialist", RoyalBirth) => format!("Another Mouth Fed by the Treasury: {}", e.subject),
        ("socialist", RoyalMarriage) => format!("Palace Celebrates While Wages Lag: {}", e.subject),
        ("socialist", Strike) => {
            if v == 0 {
                format!("{} Down Tools — Solidarity!", e.subject)
            } else {
                format!("Strike Spreads: {}", e.subject)
            }
        }
        ("socialist", MassMovement) => {
            if v == 0 {
                format!("The Streets Speak: {} in {}", e.subject, e.object)
            } else {
                format!("Workers Rise — {} in {}", e.subject, e.object)
            }
        }
        ("socialist", Treaty) => {
            if e.positive {
                format!("“{}” Signed — Whose Markets, Whose Profits?", e.subject)
            } else {
                format!("“{}” in Trouble ({})", e.subject, e.object)
            }
        }
        ("socialist", War) => format!("Workers Bleed for the Flags of {} — {}", e.subject, e.object),
        ("socialist", Economy) => {
            if e.magnitude < 0.0 {
                format!("The Ledger of the Crisis: {}", e.subject)
            } else {
                format!("Figures the Palace Won't Discuss: {}", e.subject)
            }
        }
        ("socialist", ElectionScheduled) => {
            format!("Elections Approaching ({}) — Organize!", e.object)
        }

        ("conservative", FloorVote) => {
            if e.positive {
                format!("Chamber Upholds Order, Passes “{}”", e.subject)
            } else {
                format!("“{}” Fails — Prudence or Paralysis?", e.subject)
            }
        }
        ("conservative", VetoOverride) => {
            if e.positive {
                format!("Veto Overridden on “{}” — Precedent at Risk", e.subject)
            } else {
                format!("Constitutional Guard Holds: Veto on “{}” Stands", e.subject)
            }
        }
        ("conservative", BudgetBill) => {
            if e.positive {
                "Steady Hands: Budget Enacted".to_string()
            } else {
                "Budget Deadlock Tests the Nation's Nerve".to_string()
            }
        }
        ("conservative", CouncilResolution) => {
            if e.positive {
                format!("{} Upholds Duty, Adopts “{}”", e.object, e.subject)
            } else {
                format!("{} Declines “{}”", e.object, e.subject)
            }
        }
        ("conservative", RoyalBirth) => {
            if v == 0 {
                format!("Rejoice! {} Born to the Dynasty", e.subject)
            } else {
                format!("Dynasty Secured: {}", e.subject)
            }
        }
        ("conservative", RoyalMarriage) => format!("A Union Blessed by Tradition: {}", e.subject),
        ("conservative", Strike) => format!("Disorder at {} Must Not Stand", e.subject),
        ("conservative", MassMovement) => {
            if e.magnitude > 50_000.0 {
                format!("Mob Rule Threatens: {} in {}", e.subject, e.object)
            } else {
                format!("Agitators Stir {} in {}", e.subject, e.object)
            }
        }
        ("conservative", Treaty) => {
            if e.positive {
                format!("Strength Abroad: “{}” Signed", e.subject)
            } else {
                format!("National Honor Demands Review of “{}” ({})", e.subject, e.object)
            }
        }
        ("conservative", War) => format!("Stand Firm — The Fatherland Faces {}", e.object),
        ("conservative", Economy) => {
            if e.magnitude < 0.0 {
                format!("Hard Times Demand Discipline: {}", e.subject)
            } else {
                format!("Quiet Prosperity: {}", e.subject)
            }
        }
        ("conservative", ElectionScheduled) => {
            format!("Elections Near ({}) — Duty Before Faction", e.object)
        }

        ("liberal", FloorVote) => {
            if e.positive {
                format!("“{}” Clears the Chamber — What It Means", e.subject)
            } else {
                format!("“{}” Stalls — Gridlock Costs Everyone", e.subject)
            }
        }
        ("liberal", VetoOverride) => {
            if e.positive {
                format!("Checks and Balances Tested: Override on “{}”", e.subject)
            } else {
                format!("Veto Sustained on “{}”", e.subject)
            }
        }
        ("liberal", BudgetBill) => {
            if e.positive {
                "Budget Passes — The Numbers Behind It".to_string()
            } else {
                "No Budget, No Plan: Session Ends Empty-Handed".to_string()
            }
        }
        ("liberal", CouncilResolution) => {
            if e.positive {
                format!("{} Adopts “{}” — Local Government Works", e.object, e.subject)
            } else {
                format!("{} Says No to “{}”", e.object, e.subject)
            }
        }
        ("liberal", RoyalBirth) => format!("Court Bulletin: {}", e.subject),
        ("liberal", RoyalMarriage) => format!("Court Circles: {}", e.subject),
        ("liberal", Strike) => format!("Labor Standoff at {} — Both Sides Dig In", e.subject),
        ("liberal", MassMovement) => format!("Unrest Watch: {} in {}", e.subject, e.object),
        ("liberal", Treaty) => {
            if e.positive {
                format!("Open Doors: “{}” Ratified", e.subject)
            } else {
                format!("Trade Climate Cools — “{}” {}", e.subject, e.object)
            }
        }
        ("liberal", War) => format!("Markets Brace as Conflict with {} Continues", e.object),
        ("liberal", Economy) => format!("Indicators: {}", e.subject),
        ("liberal", ElectionScheduled) => {
            format!("Campaign Season Opens — Election Set for {}", e.object)
        }

        // Fallback for any future paper id.
        (_, kind) => format!("{}: {}", kind.as_str(), e.subject),
    }
}

/// Slant-flavored body — real figures first, editorial line second.
fn body_for(paper: &str, e: &NewsEvent, country: &Country) -> String {
    let facts = match e.kind {
        NewsKind::FloorVote | NewsKind::VetoOverride => format!(
            "The chamber voted on “{}” ({}) with {}. The measure {}.",
            e.subject,
            e.object,
            fmt_votes(e),
            if e.positive { "passed" } else { "was rejected" }
        ),
        NewsKind::BudgetBill => format!(
            "The budget session for this term concluded with the national budget {}. \
             Treasury reserves stand at {:.0}.",
            if e.positive { "enacted" } else { "left unenacted" },
            country.budget.liquid_reserves
        ),
        NewsKind::CouncilResolution => format!(
            "The {} council voted on “{}” ({}) — {}.",
            e.object,
            e.subject,
            fmt_votes(e),
            if e.positive { "adopted" } else { "rejected" }
        ),
        NewsKind::RoyalBirth => format!(
            "The court registry records the birth of {} to {}, {}.",
            e.subject,
            e.object,
            if e.positive {
                "a legitimate member of the dynasty"
            } else {
                "recorded outside the line of succession"
            }
        ),
        NewsKind::RoyalMarriage => format!(
            "The court announces the marriage of {} ({}).",
            e.subject, e.object
        ),
        NewsKind::Strike => format!(
            "{} ({}) remains on strike — roughly {:.0} workers are out.",
            e.subject, e.object, e.magnitude
        ),
        NewsKind::MassMovement => format!(
            "A {} involving roughly {:.0} participants is reported in {}.",
            e.subject, e.magnitude, e.object
        ),
        NewsKind::Treaty => {
            if e.positive {
                format!(
                    "The treaty “{}” between {} entered into force ({} clauses).",
                    e.subject, e.object, e.magnitude as u64
                )
            } else {
                format!(
                    "The treaty “{}” is now recorded as {} ({} clauses).",
                    e.subject, e.object, e.magnitude as u64
                )
            }
        }
        NewsKind::War => format!(
            "{} remains in a state of war with {}.",
            e.subject, e.object
        ),
        NewsKind::Economy => format!("Latest indicators: {}. Levels: {}.", e.subject, e.object),
        NewsKind::ElectionScheduled => format!(
            "Election authorities confirm {} is scheduled for {} ({:.0} turns away).",
            e.subject, e.object, e.magnitude
        ),
    };

    let flavor = match (paper, e.kind) {
        ("gazette", NewsKind::Strike | NewsKind::MassMovement) => {
            "Authorities urge calm and a swift return to work."
        }
        ("gazette", _) => "Officials describe the development as orderly.",

        ("socialist", NewsKind::FloorVote | NewsKind::VetoOverride | NewsKind::BudgetBill) => {
            "Whether labor shares in the outcome remains to be seen."
        }
        ("socialist", NewsKind::Strike | NewsKind::MassMovement) => {
            "Our reporters stand with the workers."
        }
        ("socialist", NewsKind::RoyalBirth | NewsKind::RoyalMarriage) => {
            "The palace celebrates; the bill comes due elsewhere."
        }
        ("socialist", _) => "The bosses' papers will tell it differently.",

        ("conservative", NewsKind::Strike | NewsKind::MassMovement) => {
            "Law and order must be restored without delay."
        }
        ("conservative", NewsKind::RoyalBirth | NewsKind::RoyalMarriage) => {
            "May the dynasty endure."
        }
        ("conservative", NewsKind::Economy) => {
            "Steady stewardship, not experiments, is what the moment requires."
        }
        ("conservative", _) => "Tradition and duty remain the surest course.",

        ("liberal", NewsKind::Treaty | NewsKind::Economy) => {
            "Investors will read the fine print before cheering."
        }
        ("liberal", NewsKind::Strike | NewsKind::MassMovement) => {
            "A negotiated settlement serves every party."
        }
        ("liberal", NewsKind::War) => "Trade never fully recovers what war suspends.",
        ("liberal", _) => "Institutions, not personalities, must carry the day.",

        (_, _) => "",
    };

    if flavor.is_empty() {
        facts
    } else {
        format!("{facts} {flavor}")
    }
}

// ============================================================================
// BUILDER
// ============================================================================

/// Build the press snapshot for a country: four deterministic front pages
/// plus the real media-freedom context.
///
/// * `companies` / `unions` — `CountryEntities` slices (entity collections
///   live in the turn context, not on `Country`).
/// * `treaties` — `GameState::treaty_registry`.
/// * `current_turn` / `year` — `GameState::calendar`.
pub fn build_press_snapshot(
    country: &Country,
    companies: &[crate::entities::Company],
    unions: &[crate::entities::union::Union],
    treaties: &crate::international::treaties::TreatyRegistry,
    current_turn: u32,
    year: u32,
) -> PressSnapshotDto {
    // -- Media context (real law + runtime state) --------------------------
    let (free_speech_level, press_freedom, assembly_rights) = match &country
        .politics
        .free_speech_law
    {
        Some(law) => (
            match law.free_speech_level {
                FreeSpeechLevel::Full => "full",
                FreeSpeechLevel::Restricted => "restricted",
                FreeSpeechLevel::Suppressed => "suppressed",
                FreeSpeechLevel::Totalitarian => "totalitarian",
            },
            match law.press_freedom {
                PressFreedom::Independent => "independent",
                PressFreedom::Mixed => "mixed",
                PressFreedom::StateControlled => "state_controlled",
            },
            match law.assembly_rights {
                crate::politics::free_speech::AssemblyRights::Free => "free",
                crate::politics::free_speech::AssemblyRights::Restricted => "restricted",
                crate::politics::free_speech::AssemblyRights::Banned => "banned",
            },
        ),
        None => ("full", "independent", "free"),
    };
    let media_state = country.politics.media_state.as_ref();
    let media = PressMediaContext {
        state_media_share: media_state.map(|m| m.state_media_share),
        propaganda_active: media_state.map(|m| m.propaganda_active).unwrap_or(false),
        information_consumption_ratio: media_state.map(|m| m.last_consumption_ratio),
        free_speech_level: free_speech_level.to_string(),
        press_freedom: press_freedom.to_string(),
        assembly_rights: assembly_rights.to_string(),
    };

    // -- Event pool ---------------------------------------------------------
    let events = collect_events(country, companies, unions, treaties, current_turn, year);

    // -- Papers -------------------------------------------------------------
    let paper_specs: [(&str, &str, String); 4] = [
        (
            "gazette",
            "official",
            format!("Official Gazette of {}", country.name),
        ),
        ("socialist", "socialist", "The Workers' Tribune".to_string()),
        (
            "conservative",
            "conservative",
            format!("The {} Herald", country.name),
        ),
        (
            "liberal",
            "liberal",
            format!("The {} Free Press", country.name),
        ),
    ];

    let status_for = |paper_id: &str| -> &'static str {
        if paper_id == "gazette" {
            return "legal";
        }
        match press_freedom {
            "state_controlled" => "underground",
            "mixed" => "restricted",
            _ => "legal",
        }
    };

    let mut papers = Vec::with_capacity(paper_specs.len());
    let mut total_articles = 0usize;
    for (paper_id, slant, name) in &paper_specs {
        // Editorial selection: score = weight*1000 − age, deterministic.
        let mut scored: Vec<(i64, &NewsEvent)> = events
            .iter()
            .map(|e| {
                let age = current_turn.saturating_sub(e.turn) as i64;
                let score = editorial_weight(paper_id, e.kind) as i64 * 1000 - age;
                (score, e)
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.event_ref.cmp(&b.1.event_ref))
        });

        let articles: Vec<PressArticleDto> = scored
            .into_iter()
            .take(FRONT_PAGE_SIZE)
            .map(|(_, e)| PressArticleDto {
                id: format!("{paper_id}:{}", e.event_ref),
                paper_id: paper_id.to_string(),
                headline: headline_for(paper_id, e),
                body: body_for(paper_id, e, country),
                event_kind: e.kind.as_str().to_string(),
                event_ref: e.event_ref.clone(),
                turn: e.turn,
            })
            .collect();
        total_articles += articles.len();
        papers.push(PressPaperDto {
            paper_id: paper_id.to_string(),
            name: name.clone(),
            ideological_slant: slant.to_string(),
            status: status_for(paper_id).to_string(),
            articles,
        });
    }

    PressSnapshotDto {
        country: country.name.clone(),
        current_turn,
        year,
        media,
        papers,
        total_articles,
    }
}
