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
//! v4.25 WS4 editorial rules: each front page carries at most
//! [`FRONT_PAGE_SIZE`] DISTINCT multi-paragraph analytical articles,
//! strictly one article per `NewsKind` category, every headline unique,
//! and no raw `KRS-…` company-registry id anywhere in print (real names
//! resolved through [`press_company_label`]). `economy`/`stock_mover`/
//! `earnings`/`dividend` articles embed real sparkline series for the
//! frontend mini-charts.
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

/// v4.25 WS4: Articles per paper front page — a tight editorial selection
/// of 3–4 DISTINCT analytical articles per issue, strictly one article per
/// event category (never two `budget_bill` or two `strike` stories in the
/// same paper). The 8-card grid previously let one topic dominate the page
/// ("Budget Session Adjourns Without Settlement" 8× in one issue).
const FRONT_PAGE_SIZE: usize = 4;
/// Only events from the last N turns are eligible for front pages
/// (royal/council/history logs can stretch back further).
const NEWS_WINDOW_TURNS: u32 = 96;
/// v4.24 WS1: hard bound on `politics.history` entries scanned per snapshot.
/// The ledger grows unboundedly (a `budget_bill` record is appended every
/// budget session plus every floor/veto vote); scanning it whole made
/// snapshot latency grow with game length — perceived by the Press tab as an
/// infinite "Loading…". Entries are appended in chronological order, so the
/// newest entries sit at the tail — scan backwards and stop after this many.
/// 8192 covers ~8000 turns of budget records; anything older is outside the
/// 96-turn news window anyway.
const MAX_HISTORY_SCAN: usize = 8192;

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
    /// "mass_movement" | "treaty" | "war" | "economy" | "election_scheduled";
    /// business section adds "stock_mover" | "earnings" | "dividend" |
    /// "investment".
    pub event_kind: String,
    /// Deterministic event reference for frontend linking/dedup
    /// (e.g. "floor_vote:BILL-12:187", "royal_birth:VIP-91").
    pub event_ref: String,
    /// Global turn the underlying event happened (or its live state).
    pub turn: u32,
    /// v4.25 WS4: real time-series points for the embedded mini-chart —
    /// GDP history on `economy` leads, share-price history on
    /// `stock_mover`, reported revenue on `earnings`. Empty when the
    /// underlying state has no series (or the kind carries none).
    #[serde(default)]
    pub sparkline: Vec<f64>,
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
    /// v4.23 WS7: Financial & Business Press — stock movers, earnings,
    /// dividends and major investments sourced from real company/market
    /// state (one shared business section, not per-paper selection).
    /// `serde(default)` tolerates snapshots built before this section.
    #[serde(default)]
    pub business_articles: Vec<PressArticleDto>,
    /// Total articles emitted across all papers.
    pub total_articles: usize,
}

// ============================================================================
// EVENT COLLECTION (real sources only)
// ============================================================================

/// Internal event kind — also the DTO `event_kind` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// v4.25 WS4: supplementary real context rendered in the analytical
    /// paragraph (movement status/demands, etc.) — kept out of `object`
    /// so headlines stay short and enum-free.
    detail: String,
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
    // Scan the tail of the ledger (newest entries first) and stop after
    // MAX_HISTORY_SCAN — the log is appended chronologically and only the
    // NEWS_WINDOW_TURNS tail is newsworthy; this keeps worst-case latency
    // constant regardless of save length. Iteration order does not affect
    // output: dedup collapses identical refs/idents and the final event sort
    // is fully deterministic.
    for entry in country.politics.history.iter().rev().take(MAX_HISTORY_SCAN) {
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
                    object: humanize(
                        obj.get("bill_type")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    ),
                    votes_for: num("votes_for"),
                    votes_against: num("votes_against"),
                    abstentions: num("abstentions"),
                    positive: obj
                        .get("passed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    magnitude: 0.0,
                    detail: String::new(),
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
                    detail: String::new(),
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
                    detail: String::new(),
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
                detail: String::new(),
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
                detail: String::new(),
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
                    humanize(&format!("{:?}", marriage.political_significance))
                } else {
                    foreign
                },
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: true,
                magnitude: 0.0,
                detail: String::new(),
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
            object: union.sector.display_name(),
            votes_for: 0.0,
            votes_against: 0.0,
            abstentions: 0.0,
            positive: true,
            magnitude: workers as f64,
            detail: String::new(),
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
        // v4.25 WS4: `object` carries only the region display name — the
        // "(Active status, demands: …)" parenthetical previously leaked
        // into headlines ("The Streets Speak: riot in X (Active status,
        // demands: …)"). Status, demands and the population share move to
        // `detail` for the analytical paragraph.
        let region_pop = country
            .regions
            .iter()
            .find(|r| r.id == movement.region_id)
            .map(|r| r.population)
            .unwrap_or(0);
        let pop_share = if region_pop > 0 {
            movement.participant_count as f64 / region_pop as f64 * 100.0
        } else {
            0.0
        };
        events.push(NewsEvent {
            kind: NewsKind::MassMovement,
            turn: movement.start_turn,
            event_ref: format!("movement:{}", movement.id),
            subject: mtype.to_string(),
            object: region_name(country, &movement.region_id),
            votes_for: 0.0,
            votes_against: 0.0,
            abstentions: 0.0,
            positive: false,
            magnitude: movement.participant_count as f64,
            detail: format!(
                "The gathering is in {} status; its stated demands are {}. \
                 Roughly {:.1}% of the region's population is involved.",
                humanize(&format!("{:?}", movement.status)).to_lowercase(),
                if movement.demands.is_empty() {
                    "unspecified".to_string()
                } else {
                    movement.demands.join("; ")
                },
                pop_share
            ),
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
                            detail: String::new(),
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
                    detail: String::new(),
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
                            detail: String::new(),
                        });
                    }
                }
            }
            // v4.25.0 WS7: Rejected is a negotiation outcome — no treaty
            // event to report (mirrors Proposed/Negotiating).
            Proposed | Negotiating | Rejected => {}
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
            detail: String::new(),
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
                    "GDP {}, CPI {:.1}, unemployment {:.1}%",
                    fmt_money(md.gdp_breakdown.official_gdp),
                    md.inflation_indices.cpi_index,
                    md.labor_market.unemployment_rate
                ),
                votes_for: 0.0,
                votes_against: 0.0,
                abstentions: 0.0,
                positive: rising && !cpi_shock,
                magnitude: gdp_tot.unwrap_or(0.0),
                detail: String::new(),
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
                detail: String::new(),
            });
        }
    }

    // v4.23 WS7: dedup the event pool — one real event must never emit two
    // articles, whether the duplicate surfaced across papers, across turns,
    // or inside a single front page. Two keys collapse duplicates:
    //   1. `event_ref` — the deterministic event id; the history log can
    //      double-write an event (both chambers voting the same bill in one
    //      turn, a repeated `budget_bill` record) producing identical refs.
    //   2. (kind, subject, object, turn) — the template + subject identity,
    //      so stale double-emissions of one vote (same council resolution
    //      title in the same region on the same turn) still collapse even
    //      when persisted log entries lack a stable id.
    // The first occurrence wins; collection order is deterministic.
    {
        let mut seen_refs = std::collections::HashSet::new();
        let mut seen_idents = std::collections::HashSet::new();
        events.retain(|e| {
            seen_refs.insert(e.event_ref.clone())
                && seen_idents.insert((
                    e.kind,
                    e.subject.clone(),
                    e.object.clone(),
                    e.turn,
                ))
        });
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

/// v4.23 WS7: Compact money formatter for interpolated article figures —
/// raw engine magnitudes like `749732294` must never reach the page.
/// `$749.73M` / `$1.24B` / `$812.50K` / `-$3.10M`; sub-thousand values keep
/// cents so small treasuries still read as money.
pub(crate) fn fmt_money(v: f64) -> String {
    if !v.is_finite() {
        return "$0.00".to_string();
    }
    // Collapse negative-zero / sub-cent dust — a formatting artifact.
    let v = if v.abs() < 0.005 { 0.0 } else { v };
    let (abs, sign) = (v.abs(), if v < 0.0 { "-" } else { "" });
    if abs >= 1e9 {
        format!("{sign}${:.2}B", abs / 1e9)
    } else if abs >= 1e6 {
        format!("{sign}${:.2}M", abs / 1e6)
    } else if abs >= 1e3 {
        format!("{sign}${:.2}K", abs / 1e3)
    } else {
        format!("{sign}${abs:.2}")
    }
}

/// v4.23 WS7: Thousands-separated count formatter for interpolated
/// participant/worker figures ("roughly 50,000 participants").
fn fmt_count(v: f64) -> String {
    let n = v.round() as i64;
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// v4.23 WS7: Humanize a debug-style token for display — `{:?}` enum
/// variants (`HeavyIndustry` → "Heavy Industry"), snake_case ids
/// (`tax_reform` → "Tax Reform") and kebab-case. Multi-word results are
/// Title Cased; single words keep their capitalization.
fn humanize(raw: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in raw.trim().chars() {
        if c == '_' || c == '-' || c == ' ' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        } else if c.is_ascii_uppercase()
            && !cur.is_empty()
            && cur
                .chars()
                .last()
                .map(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                .unwrap_or(false)
        {
            // CamelCase boundary: lower/digit → Upper.
            words.push(std::mem::take(&mut cur));
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    if words.is_empty() {
        return String::new();
    }
    words
        .iter()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// v4.25 WS4: Remove any residual `KRS-…` company-registry token from
/// print text — the last-resort guard behind the release gate's
/// `!raw_text_contains("KRS-")` assertion. Tokens are `[A-Za-z0-9-]`
/// runs; an immediately enclosing `[…]`/`(…)` pair is consumed with the
/// token so no empty brackets remain. Whitespace/newlines are preserved.
fn strip_krs_tokens(s: &str) -> String {
    if !s.contains("KRS-") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut drop = vec![false; chars.len()];
    let is_tok = |c: char| c.is_ascii_alphanumeric() || c == '-';
    let mut i = 0;
    while i + 4 <= chars.len() {
        if chars[i] == 'K' && chars[i + 1] == 'R' && chars[i + 2] == 'S' && chars[i + 3] == '-' {
            let mut a = i;
            while a > 0 && is_tok(chars[a - 1]) {
                a -= 1;
            }
            let mut b = i + 4;
            while b < chars.len() && is_tok(chars[b]) {
                b += 1;
            }
            // Consume the brackets that would be left empty.
            if a > 0 && b < chars.len() {
                let (open, close) = (chars[a - 1], chars[b]);
                if (open == '[' && close == ']') || (open == '(' && close == ')') {
                    a -= 1;
                    b += 1;
                }
            }
            for d in drop.iter_mut().take(b).skip(a) {
                *d = true;
            }
            i = b;
        } else {
            i += 1;
        }
    }
    let mut out: String = chars
        .iter()
        .zip(drop.iter())
        .filter(|(_, d)| !**d)
        .map(|(c, _)| c)
        .collect();
    // Tidy removal artifacts without touching paragraph breaks: collapse
    // doubled spaces first so "(  )" becomes "( )" before the sweep.
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    for pat in ["( )", "[ ]", " .", " ,", " ;"] {
        while out.contains(pat) {
            out = out.replace(pat, if pat.starts_with(' ') { &pat[1..] } else { "" });
        }
    }
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out.trim_matches(|c| c == ' ' || c == ';' || c == ',').to_string()
}

/// v4.25 WS4: print-safe company display name. Companies carry a real
/// `name` — but older generators baked the registry id into it
/// ("Retail Co KRS-ANA-0890 (Anatolia-Region1)"). Strip `KRS-…` tokens
/// and resolve the region-id suffix to the region display name, falling
/// back to `Sector (Region)` when nothing human-readable remains.
fn press_company_label(country: &Country, c: &crate::entities::Company) -> String {
    let raw = if c.name.is_empty() {
        c.id.as_str()
    } else {
        c.name.as_str()
    };
    let mut out = String::with_capacity(raw.len());
    for tok in raw.split_whitespace() {
        let core = tok.trim_matches(|ch| matches!(ch, '[' | ']' | '(' | ')' | ',' | ';'));
        if core.starts_with("KRS-") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(tok);
    }
    // Resolve "(Anatolia-Region1)" → "(Anatolia)" when the parenthesized
    // tail is exactly the company's region id.
    if !c.region_id.is_empty() {
        let pat = format!("({})", c.region_id);
        if out.contains(&pat) {
            let disp = region_name(country, &c.region_id);
            out = out.replace(&pat, &format!("({disp})"));
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        format!(
            "{} ({})",
            c.sector.display_name(),
            region_name(country, &c.region_id)
        )
    } else {
        out
    }
}

/// v4.25 WS4: real series for the embedded mini-chart — GDP history for
/// `economy` leads; other kinds carry no chart.
fn sparkline_for(e: &NewsEvent, country: &Country) -> Vec<f64> {
    match e.kind {
        NewsKind::Economy => country
            .macro_indicators
            .telemetry_history
            .samples
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|s| s.official_gdp)
            .collect(),
        _ => Vec::new(),
    }
}

/// v4.25 WS4: ordinal helper for analytical copy ("3rd consecutive").
fn ordinal(n: usize) -> String {
    match n % 100 {
        11..=13 => format!("{n}th"),
        _ => match n % 10 {
            1 => format!("{n}st"),
            2 => format!("{n}nd"),
            3 => format!("{n}rd"),
            _ => format!("{n}th"),
        },
    }
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

/// v4.25 WS4: the analytical second paragraph — the real context behind
/// the fact (vote margins, treasury cover, movement scale, GDP streak).
/// Paper-agnostic: slant lives in the headline and the closing flavor
/// line; analysis reports only measured state.
fn analysis_for(e: &NewsEvent, country: &Country) -> String {
    match e.kind {
        NewsKind::FloorVote | NewsKind::VetoOverride => {
            let cast = e.votes_for + e.votes_against;
            if cast <= 0.0 {
                return String::new();
            }
            let share = e.votes_for / cast * 100.0;
            if e.positive {
                let verdict = if share >= 60.0 {
                    "a comfortable mandate for the government's agenda"
                } else if share >= 50.0 {
                    "a workable, if fragile, majority"
                } else {
                    "a plurality — not a majority — of votes cast"
                };
                format!("The measure carried with {:.0}% of votes cast, {}.", share, verdict)
            } else {
                let verdict = if share < 40.0 {
                    "opposition was overwhelming"
                } else {
                    "it fell agonizingly short of the line"
                };
                format!("Only {:.0}% of votes cast backed the measure — {}.", share, verdict)
            }
        }
        NewsKind::BudgetBill => {
            let gdp = country.budget.gdp.max(1.0);
            let cover = (country.budget.liquid_reserves / gdp * 100.0).clamp(0.0, 999.9);
            format!(
                "Treasury reserves stand at {} — roughly {:.1}% of measured GDP, {}.",
                fmt_money(country.budget.liquid_reserves),
                cover,
                if cover < 1.0 {
                    "a thin cushion for the commitments ahead"
                } else {
                    "some room, on paper, for the program"
                }
            )
        }
        NewsKind::CouncilResolution => format!(
            "The decision is binding on the {} region {}.",
            e.object,
            if e.positive { "as adopted" } else { "as rejected" }
        ),
        NewsKind::RoyalBirth => {
            if e.positive {
                "The child enters the line of succession.".to_string()
            } else {
                "The birth is recorded outside the line of succession.".to_string()
            }
        }
        NewsKind::RoyalMarriage => {
            if e.object.is_empty() {
                "Court observers read the match as a domestic arrangement.".to_string()
            } else {
                format!(
                    "Court observers read the match as carrying {} significance.",
                    e.object
                )
            }
        }
        NewsKind::Strike => format!(
            "The stoppage idles roughly {} workers across the {} sector.",
            fmt_count(e.magnitude),
            e.object
        ),
        NewsKind::MassMovement => e.detail.clone(),
        NewsKind::Treaty => format!(
            "The instrument runs to {} clauses among {}.",
            e.magnitude as u64, e.object
        ),
        NewsKind::War => format!(
            "The front with {} remains active; no armistice is on record.",
            e.object
        ),
        NewsKind::Economy => {
            let samples = &country.macro_indicators.telemetry_history.samples;
            let mut streak = 0usize;
            let mut falling = false;
            for w in samples.windows(2).rev() {
                let (prev, cur) = (w[0].official_gdp, w[1].official_gdp);
                if cur == prev {
                    break;
                }
                let dir = cur < prev;
                if streak == 0 {
                    falling = dir;
                } else if dir != falling {
                    break;
                }
                streak += 1;
            }
            if streak >= 2 {
                if falling {
                    format!(
                        "It is the {} consecutive turn of contraction in official output.",
                        ordinal(streak)
                    )
                } else {
                    format!(
                        "It is the {} consecutive turn of expansion in official output.",
                        ordinal(streak)
                    )
                }
            } else {
                "The print breaks the economy's recent trajectory.".to_string()
            }
        }
        NewsKind::ElectionScheduled => format!(
            "With {} turns to run, {} begins the campaign as the incumbent.",
            e.magnitude as u64,
            if country.politics.ruling_party.is_empty() {
                "the caretaker cabinet"
            } else {
                country.politics.ruling_party.as_str()
            }
        ),
    }
}

/// Slant-flavored body — paragraph one reports the real figures,
/// paragraph two the analysis and the editorial line.
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
             Treasury reserves stand at {}.",
            if e.positive { "enacted" } else { "left unenacted" },
            fmt_money(country.budget.liquid_reserves)
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
            "{} ({}) remains on strike — roughly {} workers are out.",
            e.subject,
            e.object,
            fmt_count(e.magnitude)
        ),
        NewsKind::MassMovement => format!(
            "A {} involving roughly {} participants is reported in {}.",
            e.subject,
            fmt_count(e.magnitude),
            e.object
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

    // Paragraph two: measured analysis plus the paper's editorial line.
    let second = format!("{} {}", analysis_for(e, country), flavor)
        .trim()
        .to_string();
    if second.is_empty() {
        facts
    } else {
        format!("{facts}\n\n{second}")
    }
}

// ============================================================================
// FINANCIAL & BUSINESS PRESS (v4.23 WS7)
// ============================================================================
//
// A separate business-desk section — stock movers, earnings, dividends and
// major corporate investments — every item sourced from real company/market
// state only:
//
// * `Company.price_history` — day-over-day share-price change (the same
//   series `securities/exchange.rs` appends each turn, bounded to 60).
// * `Company.financial_history` — latest reported `revenue`/`net_profit`/
//   `year` record written by `corporate/manager.rs::process_company`.
// * `Company.aggregated_stats.total_dividends` + `dividend_yield` — real
//   routed distributions.
// * `Company.action_ledger.action_records` — major board actions ("Expand",
//   "Ipo", "DesignBlueprint", "GeologicalSurvey") recorded with their turn;
//   `pending_expansion.investment` supplies the committed amount while the
//   expansion is still in flight.
//
// Neutral wire voice (no ideological slant — a shared business section),
// deterministic ordering inside each category.

/// Minimum |day-over-day % move| for a stock to make the movers column.
const BUSINESS_MOVER_MIN_PCT: f64 = 3.0;
/// Editorial caps per business category.
const BUSINESS_MAX_MOVERS: usize = 3;
const BUSINESS_MAX_EARNINGS: usize = 3;
const BUSINESS_MAX_DIVIDENDS: usize = 2;
const BUSINESS_MAX_INVESTMENTS: usize = 2;

/// Collect the business-desk article set for a country (one section shared
/// by all front pages — not per-paper editorial selection).
fn collect_business_articles(
    country: &Country,
    companies: &[crate::entities::Company],
    current_turn: u32,
) -> Vec<PressArticleDto> {
    let window_start = current_turn.saturating_sub(NEWS_WINDOW_TURNS);
    let mut articles: Vec<PressArticleDto> = Vec::new();

    // v4.25 WS4: print-safe names via `press_company_label` — raw `KRS-…`
    // registry ids never reach the page (older saves baked the id into
    // `name`), and the deterministic `id`/`event_ref` strip the prefix too
    // so even machine fields satisfy the no-`KRS-` gate.
    let ref_id = |c: &crate::entities::Company| c.id.replace("KRS-", "");
    let mk = |kind: &str,
              ref_id: &str,
              headline: String,
              body: String,
              turn: u32,
              sparkline: Vec<f64>|
     -> PressArticleDto {
        PressArticleDto {
            id: format!("biz:{kind}:{ref_id}"),
            paper_id: "business".to_string(),
            headline: strip_krs_tokens(&headline),
            body: strip_krs_tokens(&body),
            event_kind: kind.to_string(),
            event_ref: format!("biz_{kind}:{ref_id}"),
            turn,
            sparkline,
        }
    };

    // -- Stock movers: real day-over-day share-price change -----------------
    // (|pct|, company, signed pct, last price)
    let mut movers: Vec<(f64, &crate::entities::Company, f64, f64)> = Vec::new();
    for c in companies {
        let h = &c.price_history;
        if h.len() < 2 {
            continue;
        }
        let (prev, last) = (h[h.len() - 2], h[h.len() - 1]);
        if prev.abs() < 1e-9 {
            continue;
        }
        let pct = (last - prev) / prev * 100.0;
        if pct.abs() >= BUSINESS_MOVER_MIN_PCT {
            movers.push((pct.abs(), c, pct, last));
        }
    }
    movers.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    for (_, c, pct, last) in movers.into_iter().take(BUSINESS_MAX_MOVERS) {
        let name = press_company_label(country, c);
        let cap = c.shares_count as f64 * last;
        let direction = if pct >= 0.0 { "Surges" } else { "Slides" };
        let pe = if c.pe_ratio > 0.0 {
            format!("; P/E {:.1}", c.pe_ratio)
        } else {
            String::new()
        };
        // v4.25 WS4: real share-price series for the embedded mini-chart.
        let sparkline: Vec<f64> = c
            .price_history
            .iter()
            .rev()
            .take(16)
            .rev()
            .copied()
            .collect();
        articles.push(mk(
            "stock_mover",
            &ref_id(c),
            format!("{name} {direction} {pct:+.1}% on the Exchange"),
            format!(
                "Shares of {name} ({}) closed at {}{}; market capitalization {}.",
                c.sector.display_name(),
                fmt_money(last),
                pe,
                fmt_money(cap)
            ),
            current_turn,
            sparkline,
        ));
    }

    // -- Earnings: latest reported accounts ---------------------------------
    // (|revenue|, company, revenue, net_profit, report_year)
    let mut earners: Vec<(f64, &crate::entities::Company, f64, f64, f64)> = Vec::new();
    for c in companies {
        let Some(rec) = c.financial_history.last() else {
            continue;
        };
        let Some(map) = rec.as_object() else {
            continue;
        };
        let num = |k: &str| map.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let (revenue, profit, year) = (num("revenue"), num("net_profit"), num("year"));
        if revenue.abs() < 1e-9 && profit.abs() < 1e-9 {
            continue;
        }
        earners.push((revenue.abs(), c, revenue, profit, year));
    }
    earners.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    for (_, c, revenue, profit, year) in earners.into_iter().take(BUSINESS_MAX_EARNINGS) {
        let name = press_company_label(country, c);
        let result = if profit >= 0.0 { "Profit" } else { "Loss" };
        // v4.25 WS4: real reported-revenue series for the mini-chart.
        let sparkline: Vec<f64> = c
            .financial_history
            .iter()
            .rev()
            .take(8)
            .rev()
            .filter_map(|rec| {
                rec.as_object()
                    .and_then(|m| m.get("revenue"))
                    .and_then(Value::as_f64)
            })
            .collect();
        articles.push(mk(
            "earnings",
            &ref_id(c),
            format!(
                "{name} Posts {} {result} on {} Revenue",
                fmt_money(profit.abs()),
                fmt_money(revenue)
            ),
            format!(
                "Reported accounts for year {:.0}: revenue {}, net profit {}. \
                 {name} ({}) employs {} workers.",
                year,
                fmt_money(revenue),
                fmt_money(profit),
                c.sector.display_name(),
                fmt_count(c.fulfilled_fte as f64)
            ),
            current_turn,
            sparkline,
        ));
    }

    // -- Dividends: real routed distributions --------------------------------
    let mut payers: Vec<(f64, &crate::entities::Company)> = companies
        .iter()
        .filter(|c| c.aggregated_stats.total_dividends > 1e-9)
        .map(|c| (c.aggregated_stats.total_dividends, c))
        .collect();
    payers.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    for (total, c) in payers.into_iter().take(BUSINESS_MAX_DIVIDENDS) {
        let name = press_company_label(country, c);
        let cap = c.shares_count as f64 * c.share_price;
        // v4.25 WS4: `dividend_yield` is a lifetime payout ratio
        // (cumulative distributions ÷ market cap) that drifts far past
        // 100% — it is now capped at a realistic 25% annual yield at the
        // source; clamp here as well so pre-cap saves still print sanely.
        let yield_pct = (c.dividend_yield.clamp(0.0, 0.25)) * 100.0;
        // Real share-price series for the embedded mini-chart.
        let sparkline: Vec<f64> = c
            .price_history
            .iter()
            .rev()
            .take(16)
            .rev()
            .copied()
            .collect();
        articles.push(mk(
            "dividend",
            &ref_id(c),
            format!("{name} Distributes {} in Dividends", fmt_money(total)),
            format!(
                "Distributions to shareholders total {} ({:.2}% yield); \
                 market capitalization {}.",
                fmt_money(total),
                yield_pct,
                fmt_money(cap)
            ),
            current_turn,
            sparkline,
        ));
    }

    // -- Major investments: board decisions on the corporate action ledger ---
    // (action turn, company, action label, committed amount when serialized)
    const INVESTMENT_ACTIONS: [(&str, &str); 4] = [
        ("Expand", "expansion program"),
        ("Ipo", "public share offering"),
        ("DesignBlueprint", "blueprint design program"),
        ("GeologicalSurvey", "geological survey"),
    ];
    let mut invest: Vec<(u32, &crate::entities::Company, &'static str, f64)> = Vec::new();
    for c in companies {
        let mut best: Option<(u32, &'static str)> = None;
        for (action, label) in INVESTMENT_ACTIONS {
            if let Some(recs) = c.action_ledger.action_records.get(action) {
                if let Some(&(turn, _)) = recs.iter().max_by_key(|(t, _)| *t) {
                    if turn >= window_start && best.map(|(bt, _)| turn > bt).unwrap_or(true) {
                        best = Some((turn, label));
                    }
                }
            }
        }
        if let Some((turn, label)) = best {
            // Committed capital is serialized only while an expansion is in
            // flight (`pending_expansion` is consumed on project creation).
            let amount = c
                .pending_expansion
                .as_ref()
                .map(|p| p.investment)
                .unwrap_or(0.0);
            invest.push((turn, c, label, amount));
        }
    }
    invest.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    for (turn, c, label, amount) in invest.into_iter().take(BUSINESS_MAX_INVESTMENTS) {
        let name = press_company_label(country, c);
        let commitment = if amount > 0.0 {
            format!(" committing {}", fmt_money(amount))
        } else {
            String::new()
        };
        articles.push(mk(
            "investment",
            &ref_id(c),
            format!("{name} Green-Lights {}", humanize(label)),
            format!(
                "The corporate ledger records a {label} decision in turn \
                 {turn}{commitment}. {name} operates in {}.",
                c.sector.display_name()
            ),
            turn,
            Vec::new(),
        ));
    }

    articles
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

        // v4.25 WS4: strict editorial dedup — at most FRONT_PAGE_SIZE
        // articles, at most ONE article per topic category (never two
        // `budget_bill` or two `strike` stories), and every printed
        // headline unique. Scores still decide which categories lead.
        let mut articles: Vec<PressArticleDto> = Vec::new();
        let mut used_kinds = std::collections::HashSet::new();
        let mut used_headlines = std::collections::HashSet::new();
        for (_, e) in scored {
            if articles.len() >= FRONT_PAGE_SIZE {
                break;
            }
            if !used_kinds.insert(e.kind) {
                continue;
            }
            let headline = strip_krs_tokens(&headline_for(paper_id, e));
            if !used_headlines.insert(headline.clone()) {
                continue;
            }
            articles.push(PressArticleDto {
                id: format!("{paper_id}:{}", e.event_ref),
                paper_id: paper_id.to_string(),
                headline,
                body: strip_krs_tokens(&body_for(paper_id, e, country)),
                event_kind: e.kind.as_str().to_string(),
                event_ref: e.event_ref.clone(),
                turn: e.turn,
                sparkline: sparkline_for(e, country),
            });
        }
        total_articles += articles.len();
        papers.push(PressPaperDto {
            paper_id: paper_id.to_string(),
            name: name.clone(),
            ideological_slant: slant.to_string(),
            status: status_for(paper_id).to_string(),
            articles,
        });
    }

    // -- Financial & Business Press (real company/market state) -------------
    let business_articles = collect_business_articles(country, companies, current_turn);

    PressSnapshotDto {
        country: country.name.clone(),
        current_turn,
        year,
        media,
        papers,
        business_articles,
        total_articles,
    }
}
