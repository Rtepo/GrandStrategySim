//! v4.24.0 WS4.3: Internal conflicts — military coup plots, Politburo
//! power struggles, and royal claimant crises.
//!
//! Lightweight but real state: plots derive their pressure from the
//! advisory council's live faction loyalties, the royal dynasty's
//! claimant list, and macro unrest; they accrue strength each turn,
//! log escalation events at strength milestones, and resolve into the
//! historical record ("Coup Crushed", "Monarch Abdicated",
//! "Junta Installed", ...) with real state effects — cooldowns,
//! loyalty resets, head-of-state changes on abdication.
//!
//! Everything is deterministic: iteration walks stable `Vec` orders and
//! every roll goes through `deterministic_roll`/`deterministic_uniform`.

use serde::{Deserialize, Serialize};

use crate::politics::advisory_council::{AdvisoryCouncil, FactionType};
use crate::politics::succession::RoyalRelation;
use crate::state::Country;

// ============================================================================
// DATA MODEL
// ============================================================================

/// Kind of internal conflict brewing against the incumbent regime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum CoupPlotKind {
    /// Disloyal military faction planning a barracks coup / junta.
    #[default]
    MilitaryCoup,
    /// Factional power struggle inside the ruling party or palace.
    PalaceStruggle,
    /// A dynastic claimant pressing a rival line to the throne.
    ClaimantCrisis,
}

/// One escalation log entry on an active plot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoupPlotEvent {
    #[serde(default)]
    pub turn: u32,
    #[serde(default)]
    pub text: String,
}

/// An active plot against the incumbent regime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoupPlot {
    /// Stable id (`PLOT-####`), unique within the country.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: CoupPlotKind,
    /// Display title (e.g. "Barracks Conspiracy").
    #[serde(default)]
    pub title: String,
    /// VIP id + resolved name of the plot's instigator.
    #[serde(default)]
    pub instigator_vip_id: String,
    #[serde(default)]
    pub instigator_name: String,
    /// Backing factions — council faction labels (and "Royal Court" for
    /// claimant crises), grouped at spawn from the live member list.
    #[serde(default)]
    pub backers: Vec<String>,
    /// Plot strength 0–100; reaching 100 triggers the resolution roll.
    #[serde(default)]
    pub strength: f64,
    #[serde(default)]
    pub started_turn: u32,
    /// Last turn the strength moved materially.
    #[serde(default)]
    pub last_change_turn: u32,
    /// Bitmask of milestones already logged (25/50/75/90) so each
    /// escalation event fires once.
    #[serde(default)]
    pub milestones_logged: u32,
    /// Escalation log — oldest first, capped at 8.
    #[serde(default)]
    pub events: Vec<CoupPlotEvent>,
}

/// A resolved plot — the historical record row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoupResolution {
    #[serde(default)]
    pub plot_id: String,
    #[serde(default)]
    pub kind: CoupPlotKind,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub instigator_name: String,
    /// Outcome label — "Coup Crushed" | "Junta Installed" |
    /// "Monarch Abdicated" | "Claimant Exiled" | "Purge Executed" |
    /// "Reforms Conceded" | "Reformists Sidelined" | "Plot Dissolved".
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub resolved_turn: u32,
    /// Strength at resolution.
    #[serde(default)]
    pub peak_strength: f64,
}

/// Internal-conflict state for one country — stored on `Politics`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InternalConflicts {
    /// Plots currently brewing (at most one per kind).
    #[serde(default)]
    pub active_plots: Vec<CoupPlot>,
    /// Historical resolutions — oldest first, capped at 16.
    #[serde(default)]
    pub resolutions: Vec<CoupResolution>,
    /// Monotone plot id counter (`PLOT-####`).
    #[serde(default)]
    pub next_plot_id: u32,
}

impl Default for InternalConflicts {
    fn default() -> Self {
        InternalConflicts {
            active_plots: Vec::new(),
            resolutions: Vec::new(),
            next_plot_id: 1,
        }
    }
}

/// Display label for a plot kind.
pub fn plot_kind_label(kind: &CoupPlotKind) -> &'static str {
    match kind {
        CoupPlotKind::MilitaryCoup => "Military Coup",
        CoupPlotKind::PalaceStruggle => "Power Struggle",
        CoupPlotKind::ClaimantCrisis => "Claimant Crisis",
    }
}

/// Derived status label for a live plot (UI badge).
pub fn plot_status(strength: f64) -> &'static str {
    if strength >= 70.0 {
        "Imminent"
    } else if strength >= 30.0 {
        "Critical"
    } else {
        "Brewing"
    }
}

// ============================================================================
// PRESSURE MODEL — every source reads real engine state
// ============================================================================

/// Pressure (0–~70) pushing a military coup plot: mean loyalty of the
/// council's military faction vs the 0.40 unrest line, plus a bonus
/// while the aggregate loyalty is already under the coup threshold.
fn military_pressure(council: &AdvisoryCouncil) -> f64 {
    let (sum, n) = council
        .members
        .iter()
        .filter(|m| m.faction_type == FactionType::Military)
        .fold((0.0, 0usize), |(s, n), m| (s + m.loyalty, n + 1));
    if n == 0 {
        return 0.0;
    }
    let mean = sum / n as f64;
    let mut pressure = (0.40 - mean).max(0.0) * 90.0;
    if council.aggregate_loyalty < council.coup_risk_threshold {
        pressure += 8.0;
    }
    pressure
}

/// Pressure pushing a Politburo/palace power struggle: mean loyalty of
/// the non-military factions vs the 0.35 line.
fn palace_pressure(council: &AdvisoryCouncil) -> f64 {
    let (sum, n) = council
        .members
        .iter()
        .filter(|m| {
            m.faction_type == FactionType::Party
                || m.faction_type == FactionType::Bureaucratic
                || m.faction_type == FactionType::Nobility
        })
        .fold((0.0, 0usize), |(s, n), m| (s + m.loyalty, n + 1));
    if n == 0 {
        return 0.0;
    }
    let mean = sum / n as f64;
    (0.35 - mean).max(0.0) * 90.0
}

/// Pressure pushing a dynastic claimant crisis: noble/court disloyalty
/// while a rival branch exists (any non-monarch, non-consort member in
/// the succession line), plus a regency bonus while the heir is a minor.
fn claimant_pressure(
    council: &AdvisoryCouncil,
    country: &Country,
) -> Option<(f64, String, String)> {
    let dynasty = country.politics.royal_dynasty.as_ref()?;
    // Claimant candidate: first non-monarch member in the succession
    // line — a real VIP id with a resolvable name.
    let claimant = dynasty
        .succession_line()
        .into_iter()
        .find(|m| {
            m.relation != RoyalRelation::Monarch && m.relation != RoyalRelation::Consort
        })?;
    let (sum, n) = council
        .members
        .iter()
        .filter(|m| {
            m.faction_type == FactionType::Nobility
                || m.faction_type == FactionType::Religious
        })
        .fold((0.0, 0usize), |(s, n), m| (s + m.loyalty, n + 1));
    let mean = if n > 0 { sum / n as f64 } else { 0.5 };
    let mut pressure = (0.35 - mean).max(0.0) * 90.0;
    // A child monarch invites rival-branch plots.
    if dynasty.regency_active {
        pressure += 10.0;
    }
    let claimant_name = country
        .politics
        .vip_registry
        .as_ref()
        .and_then(|r| r.get(&claimant.vip_id))
        .map(|v| v.full_name.clone())
        .unwrap_or_else(|| claimant.vip_id.clone());
    Some((pressure, claimant.vip_id.clone(), claimant_name))
}

// ============================================================================
// TURN PROCESSING
// ============================================================================

/// Per-turn internal-conflict processing. Called from
/// `process_advisory_council_turn` after council legislation — plots are
/// the same machinery of faction disloyalty, surfaced to the UI.
pub fn process_internal_conflicts_turn(
    country: &mut Country,
    current_turn: u32,
    messages: &mut Vec<String>,
) {
    // Compute the three pressure sources before any mutable borrow —
    // they are pure reads of council/dynasty state.
    let (mil_p, pal_p, claimant) = {
        let council = match country.politics.advisory_council.as_ref() {
            Some(c) if !c.members.is_empty() => c,
            _ => return, // no court, no court intrigue
        };
        (
            military_pressure(council),
            palace_pressure(council),
            claimant_pressure(council, country),
        )
    };

    // Walk the three plot kinds in a fixed order.
    let sources: [(CoupPlotKind, f64); 3] = [
        (CoupPlotKind::MilitaryCoup, mil_p),
        (CoupPlotKind::PalaceStruggle, pal_p),
        (
            CoupPlotKind::ClaimantCrisis,
            claimant.as_ref().map(|c| c.0).unwrap_or(0.0),
        ),
    ];

    for (kind, pressure) in sources {
        // Claimant crises need a living rival — skip when none exists.
        if kind == CoupPlotKind::ClaimantCrisis && claimant.is_none() {
            continue;
        }
        let idx = country
            .politics
            .internal_conflicts
            .active_plots
            .iter()
            .position(|p| p.kind == kind);

        match idx {
            Some(i) => {
                // Existing plot: strength drifts toward the live pressure.
                let plot = &mut country.politics.internal_conflicts.active_plots[i];
                let before = plot.strength;
                if pressure > 15.0 {
                    plot.strength = (plot.strength + (pressure - 15.0) * 0.22).min(100.0);
                } else {
                    plot.strength = (plot.strength - 4.0).max(0.0);
                }
                if (plot.strength - before).abs() > 0.5 {
                    plot.last_change_turn = current_turn;
                }

                // Milestone escalation events — fire once each.
                for (milestone, bit, text) in [
                    (
                        25.0f64,
                        1u32,
                        format!(
                            "Whispers in the corridors — {} backers coordinate",
                            plot_kind_label(&plot.kind)
                        ),
                    ),
                    (
                        50.0f64,
                        2u32,
                        "Backers pledge support; the plot arms itself".to_string(),
                    ),
                    (
                        75.0f64,
                        4u32,
                        "Security services report the conspiracy to the throne".to_string(),
                    ),
                    (
                        90.0f64,
                        8u32,
                        "The plotters set a date — resolution imminent".to_string(),
                    ),
                ] {
                    if before < milestone && plot.strength >= milestone && plot.milestones_logged & bit == 0 {
                        plot.milestones_logged |= bit;
                        plot.events.push(CoupPlotEvent {
                            turn: current_turn,
                            text,
                        });
                    }
                }
                plot.events.truncate(8);

                if plot.strength <= 0.0 {
                    // Pressure gone — the conspiracy quietly dissolves.
                    let plot = country.politics.internal_conflicts.active_plots.remove(i);
                    resolve_plot(country, plot, "Plot Dissolved", current_turn, messages);
                    continue;
                }
                if plot.strength >= 100.0 {
                    let plot = country.politics.internal_conflicts.active_plots.remove(i);
                    resolve_active_plot(country, plot, current_turn, messages);
                    continue;
                }
            }
            None => {
                // Spawn threshold — real pressure plus a deterministic
                // per-turn roll so plots appear stochastically.
                if pressure > 22.0
                    && crate::politics::bill_lifecycle::deterministic_roll(
                        &format!("coupplot:{}:{}:spawn", country.name, current_turn),
                        0.20,
                    )
                {
                    let (instigator_vip_id, instigator_name) = match kind {
                        CoupPlotKind::ClaimantCrisis => {
                            let c = claimant.as_ref().unwrap();
                            (c.1.clone(), c.2.clone())
                        }
                        _ => match pick_instigator(country, &kind) {
                            Some(pair) => pair,
                            None => continue,
                        },
                    };
                    let title = match kind {
                        CoupPlotKind::MilitaryCoup => "Barracks Conspiracy",
                        CoupPlotKind::PalaceStruggle => "Palace Power Struggle",
                        CoupPlotKind::ClaimantCrisis => "Claimant Crisis",
                    };
                    let id = format!(
                        "PLOT-{:04}",
                        country.politics.internal_conflicts.next_plot_id
                    );
                    country.politics.internal_conflicts.next_plot_id += 1;
                    let mut plot = CoupPlot {
                        id: id.clone(),
                        kind: kind.clone(),
                        title: title.to_string(),
                        instigator_vip_id,
                        instigator_name,
                        backers: backers_of(country, &kind),
                        strength: pressure.min(60.0),
                        started_turn: current_turn,
                        last_change_turn: current_turn,
                        milestones_logged: 0,
                        events: vec![CoupPlotEvent {
                            turn: current_turn,
                            text: "Plot formed — first contacts made".to_string(),
                        }],
                    };
                    plot.events.truncate(8);
                    messages.push(format!(
                        "[COUPS] {} spawned ({}, strength {:.0}).",
                        id,
                        plot_kind_label(&plot.kind),
                        plot.strength
                    ));
                    country.politics.internal_conflicts.active_plots.push(plot);
                }
            }
        }
    }
}

/// Least-loyal councilor of the plot's faction base — the instigator
/// (falls back to the least-loyal councilor overall).
fn pick_instigator(country: &Country, kind: &CoupPlotKind) -> Option<(String, String)> {
    let council = country.politics.advisory_council.as_ref()?;
    let by_loyalty = |a: &&crate::politics::advisory_council::CouncilMember,
                      b: &&crate::politics::advisory_council::CouncilMember| {
        a.loyalty
            .partial_cmp(&b.loyalty)
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    let member = council
        .members
        .iter()
        .filter(|m| match kind {
            CoupPlotKind::MilitaryCoup => m.faction_type == FactionType::Military,
            CoupPlotKind::PalaceStruggle => {
                m.faction_type == FactionType::Party
                    || m.faction_type == FactionType::Bureaucratic
                    || m.faction_type == FactionType::Nobility
            }
            CoupPlotKind::ClaimantCrisis => {
                m.faction_type == FactionType::Nobility
                    || m.faction_type == FactionType::Religious
            }
        })
        .min_by(by_loyalty)
        .or_else(|| council.members.iter().min_by(by_loyalty))?;
    let name = country
        .politics
        .vip_registry
        .as_ref()
        .and_then(|r| r.get(&member.vip_id))
        .map(|v| v.full_name.clone())
        .unwrap_or_else(|| member.vip_id.clone());
    Some((member.vip_id.clone(), name))
}

/// Backing faction labels of councilors below the 0.45 loyalty line that
/// match the plot's faction base (≤3, stable member order).
fn backers_of(country: &Country, kind: &CoupPlotKind) -> Vec<String> {
    let Some(council) = country.politics.advisory_council.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in &council.members {
        if m.loyalty >= 0.45 {
            continue;
        }
        let matches = match kind {
            CoupPlotKind::MilitaryCoup => m.faction_type == FactionType::Military,
            CoupPlotKind::PalaceStruggle => {
                m.faction_type == FactionType::Party
                    || m.faction_type == FactionType::Bureaucratic
                    || m.faction_type == FactionType::Nobility
            }
            CoupPlotKind::ClaimantCrisis => {
                m.faction_type == FactionType::Nobility
                    || m.faction_type == FactionType::Religious
            }
        };
        if matches && !out.contains(&m.faction) {
            out.push(m.faction.clone());
        }
        if out.len() >= 3 {
            break;
        }
    }
    out
}

/// Terminal resolution roll for a plot that reached full strength.
/// Outcomes apply real state effects, then land in the history record.
fn resolve_active_plot(country: &mut Country, plot: CoupPlot, turn: u32, messages: &mut Vec<String>) {
    let roll = crate::politics::bill_lifecycle::deterministic_uniform(&format!(
        "coupplot:{}:{}:resolve",
        country.name, plot.id
    ));
    match plot.kind {
        CoupPlotKind::MilitaryCoup => {
            if roll < 0.45 {
                // Junta installed — purge stabilizes the barracks.
                if let Some(council) = country.politics.advisory_council.as_mut() {
                    council.coup_cooldown_until_turn = turn + 24;
                    for m in &mut council.members {
                        if m.faction_type == FactionType::Military {
                            m.loyalty = m.loyalty.max(0.6);
                        } else {
                            m.loyalty = (m.loyalty - 0.1).max(0.05);
                        }
                    }
                    council.recalculate_loyalty();
                }
                country.macro_indicators.social_unrest =
                    (country.macro_indicators.social_unrest + 15.0).clamp(0.0, 100.0);
                resolve_plot(country, plot, "Junta Installed", turn, messages);
            } else {
                // Coup crushed — the plot's military base is cowed.
                if let Some(council) = country.politics.advisory_council.as_mut() {
                    council.coup_cooldown_until_turn = turn + 12;
                    for m in &mut council.members {
                        if plot.backers.contains(&m.faction) {
                            m.loyalty = (m.loyalty + 0.15).min(1.0);
                        }
                    }
                    council.recalculate_loyalty();
                }
                country.macro_indicators.social_unrest =
                    (country.macro_indicators.social_unrest + 5.0).clamp(0.0, 100.0);
                resolve_plot(country, plot, "Coup Crushed", turn, messages);
            }
        }
        CoupPlotKind::PalaceStruggle => {
            if roll < 0.40 {
                // Purge executed — backers cowed, instigator sidelined.
                if let Some(council) = country.politics.advisory_council.as_mut() {
                    for m in &mut council.members {
                        if plot.backers.contains(&m.faction) {
                            m.loyalty = (m.loyalty + 0.10).min(1.0);
                        }
                        if m.vip_id == plot.instigator_vip_id {
                            m.loyalty = 0.15;
                        }
                    }
                    council.recalculate_loyalty();
                }
                resolve_plot(country, plot, "Purge Executed", turn, messages);
            } else if roll < 0.75 {
                // Reforms conceded — disloyal factions bought off.
                if let Some(council) = country.politics.advisory_council.as_mut() {
                    for m in &mut council.members {
                        if plot.backers.contains(&m.faction) {
                            m.loyalty = (m.loyalty + 0.25).min(1.0);
                        }
                    }
                    council.recalculate_loyalty();
                }
                resolve_plot(country, plot, "Reforms Conceded", turn, messages);
            } else {
                resolve_plot(country, plot, "Reformists Sidelined", turn, messages);
            }
        }
        CoupPlotKind::ClaimantCrisis => {
            if roll < 0.35 {
                // The claimant wins — the monarch abdicates.
                let abdicated = abdicate_monarch(country, turn, messages);
                if abdicated {
                    country.macro_indicators.social_unrest =
                        (country.macro_indicators.social_unrest + 10.0).clamp(0.0, 100.0);
                    resolve_plot(country, plot, "Monarch Abdicated", turn, messages);
                } else {
                    // No viable heir — the claimant is bought off instead.
                    resolve_plot(country, plot, "Claimant Exiled", turn, messages);
                }
            } else {
                // Claimant exiled — court loyalty partially restored.
                if let Some(council) = country.politics.advisory_council.as_mut() {
                    for m in &mut council.members {
                        if m.faction_type == FactionType::Nobility {
                            m.loyalty = (m.loyalty + 0.10).min(1.0);
                        }
                    }
                    council.recalculate_loyalty();
                }
                resolve_plot(country, plot, "Claimant Exiled", turn, messages);
            }
        }
    }
}

/// "Monarch Abdicated" — promote the first legitimate member of the
/// succession line (excluding the abdicating monarch and consorts) to
/// the throne, mirroring `monarchy_succession`'s stamping.
/// Returns false when no viable heir exists.
fn abdicate_monarch(country: &mut Country, turn: u32, messages: &mut Vec<String>) -> bool {
    let politics = &mut country.politics;
    let Some(dynasty) = politics.royal_dynasty.as_mut() else {
        return false;
    };
    let old_monarch = dynasty.current_monarch_id.clone();
    let heir_vip_id = dynasty
        .succession_line()
        .into_iter()
        .find(|m| {
            Some(&m.vip_id) != old_monarch.as_ref()
                && m.relation != RoyalRelation::Consort
                && m.relation != RoyalRelation::Monarch
        })
        .map(|m| m.vip_id.clone());
    let Some(heir_id) = heir_vip_id else {
        return false;
    };
    // Deposed monarch keeps their seat in the tree but loses the crown.
    if let Some(m) = dynasty
        .members
        .iter_mut()
        .find(|m| Some(&m.vip_id) == old_monarch.as_ref())
    {
        m.relation = RoyalRelation::FormerMonarch; // off the throne
        m.is_heir_apparent = false;
    }
    dynasty.current_monarch_id = Some(heir_id.clone());
    dynasty.regency_active = false;
    dynasty.current_regent_id = None;
    if let Some(m) = dynasty.members.iter_mut().find(|m| m.vip_id == heir_id) {
        m.relation = RoyalRelation::Monarch;
        m.reign_start_turn = Some(turn);
        m.is_heir_apparent = false;
    }
    if let Some(registry) = politics.vip_registry.as_mut() {
        if let Some(old) = old_monarch.as_ref().and_then(|id| registry.get_mut(id)) {
            old.roles
                .retain(|r| *r != crate::politics::vip_registry::VipRoleExtended::Monarch);
        }
        if let Some(heir_vip) = registry.get_mut(&heir_id) {
            heir_vip.add_role(crate::politics::vip_registry::VipRoleExtended::Monarch);
            politics.head_of_state.name = heir_vip.full_name.clone();
            politics.head_of_state.age = heir_vip.age;
        }
    }
    messages.push(format!(
        "[COUPS] Monarch abdicated — {} accedes to the throne (T{}).",
        heir_id, turn
    ));
    true
}

/// Write a resolution into the historical record (capped, oldest first).
fn resolve_plot(
    country: &mut Country,
    plot: CoupPlot,
    outcome: &str,
    turn: u32,
    messages: &mut Vec<String>,
) {
    messages.push(format!(
        "[COUPS] {} resolved — {} ({}, peak strength {:.0}).",
        plot.id,
        outcome,
        plot_kind_label(&plot.kind),
        plot.strength
    ));
    let ledger = &mut country.politics.internal_conflicts;
    ledger.resolutions.push(CoupResolution {
        plot_id: plot.id,
        kind: plot.kind,
        title: plot.title,
        instigator_name: plot.instigator_name,
        outcome: outcome.to_string(),
        resolved_turn: turn,
        peak_strength: plot.strength,
    });
    if ledger.resolutions.len() > 16 {
        let overflow = ledger.resolutions.len() - 16;
        ledger.resolutions.drain(0..overflow);
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::advisory_council::{AdvisoryCouncil, CouncilMember, CouncilType};

    fn council_with(military_loyalty: f64, party_loyalty: f64) -> AdvisoryCouncil {
        let mut council = AdvisoryCouncil::new(CouncilType::PartyPolitburo);
        for (i, (ft, faction, loyalty)) in [
            (FactionType::Military, "Army", military_loyalty),
            (FactionType::Military, "Army", military_loyalty),
            (FactionType::Party, "Party Old Guard", party_loyalty),
            (FactionType::Party, "Reformist Wing", party_loyalty),
            (FactionType::Bureaucratic, "State Apparatus", party_loyalty),
        ]
        .into_iter()
        .enumerate()
        {
            council.members.push(CouncilMember {
                vip_id: format!("VIP-{i:03}"),
                faction: faction.to_string(),
                loyalty,
                influence: 50.0,
                faction_type: ft,
                ..Default::default()
            });
        }
        council.recalculate_loyalty();
        council
    }

    #[test]
    fn test_military_pressure_scales_with_disloyalty() {
        let loyal = council_with(0.8, 0.8);
        assert_eq!(military_pressure(&loyal), 0.0);
        let disloyal = council_with(0.1, 0.8);
        assert!(military_pressure(&disloyal) > 20.0);
    }

    #[test]
    fn test_palace_pressure() {
        let council = council_with(0.8, 0.1);
        assert!(palace_pressure(&council) > 20.0);
        let calm = council_with(0.8, 0.8);
        assert_eq!(palace_pressure(&calm), 0.0);
    }

    #[test]
    fn test_plot_status_labels() {
        assert_eq!(plot_status(10.0), "Brewing");
        assert_eq!(plot_status(50.0), "Critical");
        assert_eq!(plot_status(90.0), "Imminent");
    }
}
