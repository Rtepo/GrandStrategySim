//! v4.21 WS3: Region/megaregion snapshot DTOs that extend the legacy
//! `snapshot.rs` records without touching its generated `api.ts` exports.
//!
//! These types are serialized as EXTRA fields on `RegionDetail` /
//! `MegaregionDetail` — the Tauri IPC bridge tolerates extra JSON keys
//! (the hand-maintained `src/types/api.ts` contract stays unchanged and
//! the richer shapes are declared in `src/types/regions.ts`).

use crate::politics::local_council::LocalCouncil;
use crate::politics::local_legislation::{MandateFundingDecision, UnfundedMandate};
use crate::ui::snapshot::MandateSummary;

/// A sitting regional councilor (or, in the megaregion assembly view, a
/// member of a member-region council) for the expandable faction summary.
/// `TS` is derived (not `export`ed) only so it can nest inside the
/// `#[derive(TS)]` parent DTOs; `api.ts` is not modified by this module.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
pub struct CouncilorRow {
    pub name: String,
    pub faction: String,
    /// Councilor persistent id (VIP identifier where registered).
    pub vip_id: String,
    /// Owning region id — populated for megaregion aggregation so the UI
    /// can group assembly members by their home council.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub region_id: String,
    /// v4.23 WS7: Party affiliation (empty when unaffiliated).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub party: String,
    /// v4.23 WS7: Represented social class (curial seat origin — real
    /// `Councilor.represented_class`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub represented_class: String,
    /// v4.23 WS7: Voting-behavior trait, emitted ONLY when revealed —
    /// `hidden_trait` stays hidden while `trait_revealed` is false so the
    /// UI never leaks unrevealed councilor internals.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub trait_label: String,
    /// v4.23 WS7: Political influence (0–100, `Councilor.political_influence`).
    pub influence: f64,
    /// v4.23 WS7: Years in office.
    pub years_in_office: u32,
}

/// Flatten a `LocalCouncil`'s sitting councilors into UI rows, tagged
/// with the owning region id (empty for a single-region view).
pub fn council_member_rows(council: &LocalCouncil, region_id: &str) -> Vec<CouncilorRow> {
    council
        .councilors
        .iter()
        .map(|c| CouncilorRow {
            name: c.name.clone(),
            faction: c.faction.to_string(),
            vip_id: c.id.clone(),
            region_id: region_id.to_string(),
            party: c.party.clone(),
            represented_class: c.represented_class.clone(),
            trait_label: if c.trait_revealed {
                format!("{:?}", c.hidden_trait)
            } else {
                String::new()
            },
            influence: c.political_influence,
            years_in_office: c.years_in_office,
        })
        .collect()
}

/// Aggregate faction seat counts across councils (member regions for a
/// megaregion). Returns the canonical (Populares, Moderates, Optimates)
/// triple so the UI can merge it with `RegionDetail.council_factions`.
pub fn aggregate_faction_seats<'a>(
    councils: impl Iterator<Item = &'a LocalCouncil>,
) -> Vec<(String, u32)> {
    let (mut pop, mut mods, mut opts) = (0u32, 0u32, 0u32);
    for c in councils {
        let fd = &c.faction_distribution;
        pop += fd.populares_count;
        mods += fd.moderates_count;
        opts += fd.optimates_count;
    }
    vec![
        ("Populares".to_string(), pop),
        ("Moderates".to_string(), mods),
        ("Optimates".to_string(), opts),
    ]
}

/// Human-readable funding source for the mandate's composition:
/// `Central Grant` when the Treasury covers the whole requirement,
/// `Regional Treasury` when the JST covers it all, or both legs when
/// split-funded. Deterministic from mandate cash parameters — never
/// fabricated.
fn mandate_funding_source(m: &UnfundedMandate) -> &'static str {
    let has_central = m.central_funding > 1e-9 || m.central_disbursed > 1e-9;
    let has_regional = m.funding_gap > 1e-9 || m.regional_disbursed > 1e-9;
    match (has_central, has_regional) {
        (true, true) => "Central Grant + Regional Treasury",
        (true, false) => "Central Grant",
        (false, true) => "Regional Treasury",
        (false, false) => "Regional Treasury",
    }
}

/// Map a stored `MandateFundingDecision` to a display label for the UI
/// (`Pending`/`Refused`/`RaisePropertyTax`/…). Kept here so both
/// `RegionDetail` and `MegaregionDetail` mandate rows share one labeler.
fn mandate_mechanism_label(m: &UnfundedMandate) -> String {
    match &m.council_decision {
        MandateFundingDecision::Pending => "Pending".to_string(),
        MandateFundingDecision::Refused => "Refused".to_string(),
        MandateFundingDecision::RaisePropertyTax { new_rate } => {
            format!("Raise Property Tax ({:.2})", new_rate)
        }
        MandateFundingDecision::RaiseLocalFees { fee_multiplier } => {
            format!("Raise Local Fees (x{:.2})", fee_multiplier)
        }
        MandateFundingDecision::IssueBonds {
            principal,
            interest_rate,
        } => format!(
            "Issue Bonds ({} @ {:.1}%)",
            principal,
            interest_rate * 100.0
        ),
        MandateFundingDecision::CutExpenditures { cut_amount } => {
            format!("Cut Expenditures ({:.0})", cut_amount)
        }
    }
}

/// Build the UI `MandateSummary` for one stored `UnfundedMandate`,
/// including v4.21 scope/effects, per-turn disbursement progress, and the
/// explicit Central Grant / Regional Treasury funding-source label.
pub fn build_mandate_summary(m: &UnfundedMandate) -> MandateSummary {
    let required = m.required_spending_per_region.max(0.0);
    MandateSummary {
        description: m.description.clone(),
        required_spending: required,
        central_funding: m.central_funding,
        funding_gap: m.funding_gap,
        status: mandate_mechanism_label(m),
        region_id: m.region_id.clone(),
        source_bill_id: m.source_bill_id.clone(),
        imposed_turn: m.imposed_turn,
        scope: m.scope.clone(),
        effects: m.effects.clone(),
        disbursed: m.disbursed,
        central_disbursed: m.central_disbursed,
        regional_disbursed: m.regional_disbursed,
        funding_source: mandate_funding_source(m).to_string(),
        progress: if required > 0.0 {
            (m.disbursed / required).clamp(0.0, 1.0)
        } else {
            0.0
        },
        per_turn_disbursement: required
            / crate::politics::local_legislation::MANDATE_DISBURSE_TURNS,
    }
}
