//! v4.22 WS4/WS6: Society, organizations, GDP-composition, and trade-treemap
//! snapshot DTOs.
//!
//! These types extend the legacy `snapshot.rs` records without touching its
//! generated `api.ts` exports. Every number is sourced from real simulation
//! state — where a mechanic does not exist the field documents the gap rather
//! than fabricating a value.
//!
//! Data sources:
//! * GDP composition: `MacroData::gdp_breakdown` (national) and the per-turn
//!   `CountryEntities::gdp_flows_by_region` expenditure legs written back by
//!   the turn loop (regional). The 25-sample
//!   `MacroData::gdp_composition_history` ring buffer (v4.22, appended by the
//!   post-turn `advance_turn` hook) supplies the last-24-turns history.
//! * Trade treemap: `GlobalMarket::supply_volume` / `demand_volume` (real
//!   order-book flows this turn) priced at `MarketHistory::vwap_per_commodity`
//!   (last settled VWAP). Exact settled quantities are not persisted, so the
//!   cleared-volume bound `min(supply, demand)` is emitted explicitly.
//! * Organizations: `Union` entities, `Company` union affiliation/FTE,
//!   `LegalForm::Guild` membership, `Politics::mass_movements`, and the
//!   per-class `political_sentiment` distribution.
//! * Society: `MacroData::demographics`, `health_statistics`,
//!   `education_statistics`, and per-region `ClassDemographics` aggregates.

use crate::economy::market::market_history::MarketHistory;
use crate::economy::market::GlobalMarket;
use crate::economy::telemetry::RegionalGdpAccumulator;
use crate::society::geography::{ClassDemographics, Region};
use crate::state::Country;
use std::collections::{BTreeMap, HashMap};

// ============================================================================
// WS4.1 — GDP COMPOSITION
// ============================================================================

/// Expenditure-side GDP breakdown for one accounting unit (nation or region).
/// All values are absolute currency units — the frontend derives shares.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GdpExpenditureBreakdown {
    /// C — monetized final household consumption (B2C retail clearing).
    pub consumption: f64,
    /// Imputed consumption — in-kind/subsistence flows valued at VWAP
    /// (tracked separately from `consumption` since v4.19).
    pub imputed_consumption: f64,
    /// G — government spending (ministry procurement + subsidies + wages).
    pub government_spending: f64,
    /// I — gross investment (fixed-asset purchases + construction spend).
    pub investment: f64,
    /// NX — net exports (exports − imports).
    pub net_exports: f64,
    /// Official GDP = C + imputed-C + G + I + NX.
    pub official_gdp: f64,
}

/// Per-region expenditure composition for the Region Overview.
///
/// The `*_leg` fields are the real per-region expenditure legs accumulated
/// by `GdpAccumulator::regional` during the last turn. `reconciled_gdp` is
/// `Region::gdp` after `reconcile_regional_gdp` distributed the national
/// residual (modeled trade balance + unattributed legs) — it need not equal
/// the leg sum; `residual` carries the difference so the account reconciles
/// visibly.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionalGdpCompositionRow {
    /// Region id (`Region::id`).
    pub region_id: String,
    /// Display name (`Region::display_name`, falls back to id upstream).
    pub region_name: String,
    /// Region population.
    pub population: i64,
    /// C leg — B2C retail revenue attributed to this region's households.
    pub consumption: f64,
    /// Imputed subsistence consumption leg.
    pub imputed_consumption: f64,
    /// G leg — ministry spending attributed to this region.
    pub government_spending: f64,
    /// I leg — construction/fixed-asset spend attributed to this region.
    pub investment: f64,
    /// NX leg — physically settled foreign-sweep legs attributed here.
    pub net_exports: f64,
    /// Sum of the attributed legs (C + imputed-C + G + I + NX).
    pub attributed_gdp: f64,
    /// Final `Region::gdp` after national-residual reconciliation
    /// (`Σ regions.gdp == national GDP` by construction).
    pub reconciled_gdp: f64,
    /// `reconciled_gdp − attributed_gdp` — the residual placed on this
    /// region (∝ measured gross cross-border engagement, or capital seat).
    pub residual: f64,
}

/// One turn of GDP composition history from the
/// `MacroData::gdp_composition_history` ring buffer (up to 25 samples ≈
/// last 24 turns + current; empty on pre-v4.22 saves).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GdpCompositionHistoryPoint {
    /// Global turn of the sample.
    pub turn: u32,
    /// Sim year of the sample.
    pub year: u32,
    /// Monetized household consumption.
    pub consumption: f64,
    /// Imputed subsistence consumption.
    pub imputed_consumption: f64,
    /// Government spending.
    pub government_spending: f64,
    /// Gross investment.
    pub investment: f64,
    /// Net exports.
    pub net_exports: f64,
    /// Official GDP.
    pub official_gdp: f64,
    /// Shadow GDP (parallel economy, outside official GDP).
    pub shadow_gdp: f64,
}

/// National + per-region GDP composition with a ~24-turn history.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GdpCompositionDto {
    /// Country name.
    pub country: String,
    /// National expenditure breakdown (last computed turn).
    pub national: GdpExpenditureBreakdown,
    /// Shadow GDP — off-the-books wages + bribes (parallel economy,
    /// not part of official GDP).
    pub shadow_gdp: f64,
    /// Per-region composition rows in `country.regions` order.
    pub regions: Vec<RegionalGdpCompositionRow>,
    /// Composition history — newest sample last.
    pub history: Vec<GdpCompositionHistoryPoint>,
}

/// Build the GDP composition DTO for a country.
///
/// * `regional_flows` — `CountryEntities::gdp_flows_by_region` from the turn
///   context (empty map on fresh saves / pre-first-turn → rows report the
///   reconciled `Region::gdp` with zero attributed legs, an honest gap).
pub fn build_gdp_composition(
    country: &Country,
    regional_flows: &HashMap<String, RegionalGdpAccumulator>,
) -> GdpCompositionDto {
    let g = &country.macro_indicators.gdp_breakdown;
    let national = GdpExpenditureBreakdown {
        consumption: g.consumption,
        imputed_consumption: g.imputed_consumption,
        government_spending: g.government_spending,
        investment: g.investment,
        net_exports: g.net_exports,
        official_gdp: g.official_gdp,
    };

    let regions: Vec<RegionalGdpCompositionRow> = country
        .regions
        .iter()
        .map(|r| {
            let legs = regional_flows.get(&r.id).cloned().unwrap_or_default();
            let attributed = legs.consumption
                + legs.imputed_consumption
                + legs.government_spending
                + legs.investment
                + legs.net_exports;
            RegionalGdpCompositionRow {
                region_id: r.id.clone(),
                region_name: if r.display_name.is_empty() {
                    r.id.clone()
                } else {
                    r.display_name.clone()
                },
                population: r.population,
                consumption: legs.consumption,
                imputed_consumption: legs.imputed_consumption,
                government_spending: legs.government_spending,
                investment: legs.investment,
                net_exports: legs.net_exports,
                attributed_gdp: attributed,
                reconciled_gdp: r.gdp,
                residual: r.gdp - attributed,
            }
        })
        .collect();

    let history: Vec<GdpCompositionHistoryPoint> = country
        .macro_indicators
        .gdp_composition_history
        .iter()
        .map(|s| GdpCompositionHistoryPoint {
            turn: s.turn,
            year: s.year,
            consumption: s.consumption,
            imputed_consumption: s.imputed_consumption,
            government_spending: s.government_spending,
            investment: s.investment,
            net_exports: s.net_exports,
            official_gdp: s.official_gdp,
            shadow_gdp: s.shadow_gdp,
        })
        .collect();

    GdpCompositionDto {
        country: country.name.clone(),
        national,
        shadow_gdp: g.shadow_gdp,
        regions,
        history,
    }
}

// ============================================================================
// WS4.2 — TRADE TREEMAP
// ============================================================================

/// One commodity's market-value share for the trade treemap.
///
/// `value` is a cleared-volume bound — `min(supply_volume, demand_volume)`
/// (the most goods that could cross the book this turn) priced at the last
/// settled VWAP. Exact settled quantities are not persisted on state, so
/// the bound and its inputs are emitted explicitly rather than estimated
/// silently.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TradeTreemapEntry {
    /// Commodity display name (serde key, e.g. "Steel").
    pub commodity: String,
    /// VAT-category grouping: "agriculture" | "industry" | "services".
    pub category: String,
    /// Cleared-volume bound: `min(supply_volume, demand_volume)` in units.
    pub cleared_volume: f64,
    /// Reference price — VWAP → last trade → global base fallback chain.
    pub reference_price: f64,
    /// Market value: `cleared_volume × reference_price`.
    pub value: f64,
    /// Share of total treemap value (0.0–1.0).
    pub share: f64,
    /// Total sell-order volume this turn (B2B supply side).
    pub supply_volume: f64,
    /// Total buy-order volume this turn (B2C + B2B demand side).
    pub demand_volume: f64,
    /// Net trade (imports − exports, physical units; positive = importer).
    pub net_trade: f64,
}

/// Per-commodity market/trade value shares grouped by VAT category.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TradeTreemapDto {
    /// Sum of all entry values.
    pub total_value: f64,
    /// Entries sorted by value descending (ties broken by commodity name —
    /// deterministic). Only commodities with nonzero order flow appear.
    pub entries: Vec<TradeTreemapEntry>,
}

/// Build the trade treemap from real market state.
///
/// Iterates the union of `supply_volume`/`demand_volume` keys (real order
/// flow this turn), computes the cleared-volume bound per commodity, and
/// prices it at the settled-VWAP fallback chain. Commodities whose bound is
/// zero (one-sided book) are omitted — the treemap shows value that could
/// actually cross.
pub fn build_trade_treemap(market: &GlobalMarket, history: &MarketHistory) -> TradeTreemapDto {
    // Deterministic commodity set: union of the two order-flow maps.
    let mut commodities: std::collections::BTreeSet<crate::registries::enums::Commodity> =
        market.supply_volume.keys().copied().collect();
    commodities.extend(market.demand_volume.keys().copied());

    let mut entries: Vec<TradeTreemapEntry> = Vec::new();
    for commodity in commodities {
        let supply = market.supply_volume.get(&commodity).copied().unwrap_or(0.0);
        let demand = market.demand_volume.get(&commodity).copied().unwrap_or(0.0);
        let cleared = supply.min(demand);
        if cleared <= 0.0 {
            continue;
        }
        let price =
            crate::economy::market::market_history::get_reference_price(&commodity, history)
                .unwrap_or(0.0);
        if price <= 0.0 {
            continue;
        }
        entries.push(TradeTreemapEntry {
            commodity: commodity.to_string(),
            category: commodity.vat_category().to_string(),
            cleared_volume: cleared,
            reference_price: price,
            value: cleared * price,
            share: 0.0,
            supply_volume: supply,
            demand_volume: demand,
            net_trade: market.net_trade.get(&commodity).copied().unwrap_or(0.0),
        });
    }

    let total: f64 = entries.iter().map(|e| e.value).sum();
    if total > 0.0 {
        for e in &mut entries {
            e.share = e.value / total;
        }
    }
    entries.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.commodity.cmp(&b.commodity))
    });

    TradeTreemapDto {
        total_value: total,
        entries,
    }
}

// ============================================================================
// WS6.1 — ORGANIZATIONS SUMMARY
// ============================================================================

/// Headline aggregates for the Organizations page.
///
/// Union membership counts the *workers* at union-affiliated companies (the
/// v4.19 WS5 definition — `fulfilled_fte`, not firm count). The
/// radicalization index is the population-weighted `radicals` share of
/// `political_sentiment` aggregated over every class demographic.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct OrganizationsSummaryDto {
    /// Country name.
    pub country: String,
    /// Number of registered trade unions.
    pub union_count: usize,
    /// Total workers employed at union-affiliated companies.
    pub union_member_workers: u64,
    /// Total employed workforce (`labor_market.employed_total`).
    pub employed_total: f64,
    /// Unionization rate: `union_member_workers / employed_total` (0 when
    /// employed_total is 0).
    pub unionization_rate: f64,
    /// Unions currently on strike.
    pub active_strikes: usize,
    /// Workers at companies covered by striking unions.
    pub striking_workers: u64,
    /// Number of craft-guild companies.
    pub guild_count: usize,
    /// Total member workshops across all guilds.
    pub guild_member_workshops: usize,
    /// FTE allocated to guild workshops this turn (real labor allocation).
    pub guild_worker_fte: f64,
    /// Active/forming/negotiating mass movements.
    pub active_movements: usize,
    /// Total participants across active mass movements.
    pub movement_participants: i64,
    /// Population-weighted radical share of political sentiment (0.0–1.0).
    pub radicalization_index: f64,
    /// Population-weighted loyalist share (0.0–1.0).
    pub loyalist_share: f64,
    /// Population-weighted undecided share (0.0–1.0).
    pub undecided_share: f64,
}

/// Iterate all class demographics (rural + urban) of a country.
fn all_class_demographics(country: &Country) -> impl Iterator<Item = &ClassDemographics> {
    country.regions.iter().flat_map(|r| {
        r.class_demographics
            .rural_classes
            .values()
            .chain(r.class_demographics.urban_classes.values())
    })
}

/// Count workers employed by companies affiliated with a union — the same
/// union-membership resolution used by `build_organizations_snapshot`
/// (`company.union_id` or `union.company_ids` membership).
fn union_member_workers(
    union: &crate::entities::union::Union,
    companies: &[crate::entities::Company],
) -> u64 {
    companies
        .iter()
        .filter(|c| {
            c.union_id.as_deref() == Some(union.id.as_str()) || union.company_ids.contains(&c.id)
        })
        .map(|c| c.fulfilled_fte as u64)
        .sum()
}

/// Build the organizations summary for one country.
///
/// * `companies` / `unions` — the country's entity slices from the turn
///   context (`CountryEntities`).
pub fn build_organizations_summary(
    country: &Country,
    companies: &[crate::entities::Company],
    unions: &[crate::entities::union::Union],
) -> OrganizationsSummaryDto {
    let mut union_member_workers_total: u64 = 0;
    let mut active_strikes: usize = 0;
    let mut striking_workers: u64 = 0;
    for union in unions {
        let members = union_member_workers(union, companies);
        union_member_workers_total += members;
        if union.on_strike {
            active_strikes += 1;
            striking_workers += members;
        }
    }

    let mut guild_count: usize = 0;
    let mut guild_member_workshops: usize = 0;
    for company in companies {
        if let crate::entities::LegalForm::Guild(data) = &company.legal_form {
            guild_count += 1;
            guild_member_workshops += data.member_workshop_ids.len();
        }
    }

    let mut guild_worker_fte: f64 = 0.0;
    let mut radical_weighted: f64 = 0.0;
    let mut loyalist_weighted: f64 = 0.0;
    let mut undecided_weighted: f64 = 0.0;
    let mut sentiment_population: f64 = 0.0;
    for demo in all_class_demographics(country) {
        guild_worker_fte += demo.guild_fte_allocated;
        let pop = demo.population.max(0) as f64;
        radical_weighted += pop * demo.political_sentiment.radicals;
        loyalist_weighted += pop * demo.political_sentiment.loyalists;
        undecided_weighted += pop * demo.political_sentiment.undecided;
        sentiment_population += pop;
    }

    let mut active_movements: usize = 0;
    let mut movement_participants: i64 = 0;
    for movement in &country.politics.mass_movements {
        use crate::politics::mass_movements::MassMovementStatus::*;
        if matches!(
            movement.status,
            Forming | Active | Negotiating
        ) {
            active_movements += 1;
            movement_participants += movement.participant_count;
        }
    }

    let employed_total = country.macro_indicators.labor_market.employed_total;
    OrganizationsSummaryDto {
        country: country.name.clone(),
        union_count: unions.len(),
        union_member_workers: union_member_workers_total,
        employed_total,
        unionization_rate: if employed_total > 0.0 {
            union_member_workers_total as f64 / employed_total
        } else {
            0.0
        },
        active_strikes,
        striking_workers,
        guild_count,
        guild_member_workshops,
        guild_worker_fte,
        active_movements,
        movement_participants,
        radicalization_index: if sentiment_population > 0.0 {
            radical_weighted / sentiment_population
        } else {
            0.0
        },
        loyalist_share: if sentiment_population > 0.0 {
            loyalist_weighted / sentiment_population
        } else {
            0.0
        },
        undecided_share: if sentiment_population > 0.0 {
            undecided_weighted / sentiment_population
        } else {
            0.0
        },
    }
}

// ============================================================================
// WS6.2 — SOCIETY SNAPSHOT
// ============================================================================

/// One age cohort row of the demographic structure.
///
/// The sim tracks three real cohorts — children / adults / elderly
/// (`Demographics::age_groups`, shares updated every turn by the
/// demographic model). Finer granularity (5-year brackets) does not exist
/// in engine state, so the honest three-cohort structure is emitted; the
/// frontend renders it as the population pyramid's coarse form.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct AgeCohortDto {
    /// Cohort label ("children" | "adults" | "elderly").
    pub label: String,
    /// Share of total population (0.0–1.0).
    pub share: f64,
    /// Absolute population (share × population).
    pub population: f64,
}

/// One education-level row of the national education distribution.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct EducationLevelDto {
    /// Level label ("none" | "basic" | "secondary:<spec>" | "higher:<spec>").
    pub level: String,
    /// Share of the adult population (0.0–1.0).
    pub share: f64,
}

/// Generic composition entry (ethnicity or religion share).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompositionEntryDto {
    /// Group name (real key from `ethnic_composition`/`religious_composition`,
    /// or the per-class `culture`/`religion` field).
    pub name: String,
    /// Share (0.0–1.0).
    pub share: f64,
}

/// Per-class standard-of-living row, aggregated nationally across regions.
/// All fields are population-weighted aggregates of the real per-region
/// `ClassDemographics` records.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClassLivingRow {
    /// Class name (RuralClass/UrbanClass identifier, e.g. "FreePeasant").
    pub class_name: String,
    /// Total population of this class nationwide.
    pub population: i64,
    /// Population-weighted savings per capita.
    pub savings_per_capita: f64,
    /// Modal economic status across regions (the status of the largest
    /// regional population share — deterministic).
    pub economic_status: String,
    /// Population-weighted mental health (0.0–100.0).
    pub mental_health: f64,
    /// Population-weighted radical sentiment share (0.0–1.0).
    pub radical_share: f64,
    /// Population-weighted loyalist sentiment share (0.0–1.0).
    pub loyalist_share: f64,
    /// Population-weighted undecided sentiment share (0.0–1.0).
    pub undecided_share: f64,
    /// Largest regional religion label for this class (real per-class
    /// `religion` field; empty when unset).
    pub religion: String,
    /// Largest regional culture label for this class (real per-class
    /// `culture` field; empty when unset).
    pub culture: String,
    /// Total available FTE offered by this class.
    pub available_fte: f64,
    /// Total FTE allocated to companies this turn.
    pub allocated_fte: f64,
}

/// Per-country demographic snapshot — all fields sourced from real state.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SocietySnapshotDto {
    /// Country name.
    pub country: String,
    /// Total population (`budget.population`).
    pub population: u64,
    /// Age structure — the real children/adults/elderly cohorts.
    pub age_cohorts: Vec<AgeCohortDto>,
    /// Male share of population (0.0–1.0).
    pub male_share: f64,
    /// Female share of population (0.0–1.0).
    pub female_share: f64,
    /// Mean age (`demographics.average_age`).
    pub average_age: f64,
    /// Median age (`demographics.median_age`).
    pub median_age: f64,
    /// Education distribution (none/basic + specialization rows).
    pub education: Vec<EducationLevelDto>,
    /// Ethnic composition (`demographics.ethnic_composition` shares).
    pub ethnicities: Vec<CompositionEntryDto>,
    /// Religious composition (`demographics.religious_composition` shares).
    pub religions: Vec<CompositionEntryDto>,
    /// Annual birth rate percent (`demographics.birth_rate`).
    pub birth_rate: f64,
    /// Annual mortality rate percent (`demographics.death_rate`).
    pub death_rate: f64,
    /// Net migration rate (fraction of population per year).
    pub net_migration_rate: f64,
    /// Births in the last turn.
    pub last_births: f64,
    /// Deaths in the last turn (natural + workplace).
    pub last_deaths: f64,
    /// Net migration in the last turn.
    pub last_migration: f64,
    /// Life expectancy (`health_statistics.average_lifespan`, computed each
    /// turn from health infrastructure/quality).
    pub life_expectancy: f64,
    /// Literacy rate (`education_statistics.literacy_rate`).
    pub literacy_rate: f64,
    /// Higher-education rate (`education_statistics.higher_education_rate`).
    pub higher_education_rate: f64,
    /// Urban population share (`demographics.city_urban`).
    pub urban_share: f64,
    /// Rural population share (`demographics.rural`).
    pub rural_share: f64,
    /// Nomadic population share (`demographics.nomads`).
    pub nomad_share: f64,
    /// Per-class standard-of-living rows (rural + urban classes).
    pub classes: Vec<ClassLivingRow>,
}

/// Build the society snapshot for a country — aggregates the real regional
/// class demographics and national demographic indicators.
pub fn build_society_snapshot(country: &Country) -> SocietySnapshotDto {
    let demo = &country.macro_indicators.demographics;
    let population = country.budget.population;

    let age_cohorts = vec![
        AgeCohortDto {
            label: "children".to_string(),
            share: demo.age_groups.children,
            population: demo.age_groups.children * population as f64,
        },
        AgeCohortDto {
            label: "adults".to_string(),
            share: demo.age_groups.adults,
            population: demo.age_groups.adults * population as f64,
        },
        AgeCohortDto {
            label: "elderly".to_string(),
            share: demo.age_groups.elderly,
            population: demo.age_groups.elderly * population as f64,
        },
    ];

    let mut education = vec![
        EducationLevelDto {
            level: "none".to_string(),
            share: demo.education.none,
        },
        EducationLevelDto {
            level: "basic".to_string(),
            share: demo.education.basic,
        },
    ];
    // BTreeMap — deterministic specialization order.
    for (spec, share) in &demo.education.secondary {
        education.push(EducationLevelDto {
            level: format!("secondary:{spec}"),
            share: *share,
        });
    }
    for (spec, share) in &demo.education.higher {
        education.push(EducationLevelDto {
            level: format!("higher:{spec}"),
            share: *share,
        });
    }

    let ethnicities = demo
        .ethnic_composition
        .iter()
        .map(|(name, share)| CompositionEntryDto {
            name: name.clone(),
            share: *share,
        })
        .collect();
    let religions = demo
        .religious_composition
        .iter()
        .map(|(name, share)| CompositionEntryDto {
            name: name.clone(),
            share: *share,
        })
        .collect();

    // Aggregate class rows across all regions, keyed by class name.
    // BTreeMap keeps the output order deterministic.
    #[derive(Default)]
    struct ClassAcc {
        population: i64,
        savings_pc_weighted: f64,
        mental_health_weighted: f64,
        radical_weighted: f64,
        loyalist_weighted: f64,
        undecided_weighted: f64,
        available_fte: f64,
        allocated_fte: f64,
        // Modal economic status: population per status.
        status_pop: BTreeMap<String, i64>,
        // Modal religion/culture labels: population per label.
        religion_pop: BTreeMap<String, i64>,
        culture_pop: BTreeMap<String, i64>,
    }
    let mut classes: BTreeMap<String, ClassAcc> = BTreeMap::new();
    for region in &country.regions {
        for (class_name, demo) in region
            .class_demographics
            .rural_classes
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .chain(
                region
                    .class_demographics
                    .urban_classes
                    .iter()
                    .map(|(k, v)| (k.as_str(), v)),
            )
        {
            let acc = classes.entry(class_name.to_string()).or_default();
            let pop = demo.population.max(0);
            acc.population += demo.population;
            let pop_f = pop as f64;
            acc.savings_pc_weighted += pop_f * demo.savings_per_capita;
            acc.mental_health_weighted += pop_f * demo.mental_health;
            acc.radical_weighted += pop_f * demo.political_sentiment.radicals;
            acc.loyalist_weighted += pop_f * demo.political_sentiment.loyalists;
            acc.undecided_weighted += pop_f * demo.political_sentiment.undecided;
            acc.available_fte += demo.available_fte;
            acc.allocated_fte += demo.allocated_fte;
            *acc.status_pop
                .entry(format!("{:?}", demo.economic_status))
                .or_default() += demo.population;
            if !demo.religion.is_empty() {
                *acc.religion_pop.entry(demo.religion.clone()).or_default() += demo.population;
            }
            if !demo.culture.is_empty() {
                *acc.culture_pop.entry(demo.culture.clone()).or_default() += demo.population;
            }
        }
    }

    // Deterministic modal picker: max population, ties broken by label.
    fn modal_label(map: &BTreeMap<String, i64>) -> String {
        map.iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(k, _)| k.clone())
            .unwrap_or_default()
    }

    let class_rows: Vec<ClassLivingRow> = classes
        .into_iter()
        .map(|(name, acc)| {
            let pop_f = acc.population.max(0) as f64;
            ClassLivingRow {
                class_name: name,
                population: acc.population,
                savings_per_capita: if pop_f > 0.0 {
                    acc.savings_pc_weighted / pop_f
                } else {
                    0.0
                },
                economic_status: modal_label(&acc.status_pop),
                mental_health: if pop_f > 0.0 {
                    acc.mental_health_weighted / pop_f
                } else {
                    0.0
                },
                radical_share: if pop_f > 0.0 {
                    acc.radical_weighted / pop_f
                } else {
                    0.0
                },
                loyalist_share: if pop_f > 0.0 {
                    acc.loyalist_weighted / pop_f
                } else {
                    0.0
                },
                undecided_share: if pop_f > 0.0 {
                    acc.undecided_weighted / pop_f
                } else {
                    0.0
                },
                religion: modal_label(&acc.religion_pop),
                culture: modal_label(&acc.culture_pop),
                available_fte: acc.available_fte,
                allocated_fte: acc.allocated_fte,
            }
        })
        .collect();

    SocietySnapshotDto {
        country: country.name.clone(),
        population,
        age_cohorts,
        male_share: demo.gender.male,
        female_share: demo.gender.female,
        average_age: demo.average_age,
        median_age: demo.median_age,
        education,
        ethnicities,
        religions,
        birth_rate: demo.birth_rate,
        death_rate: demo.death_rate,
        net_migration_rate: demo.net_migration,
        last_births: demo.last_births,
        last_deaths: demo.last_deaths,
        last_migration: demo.last_migration,
        life_expectancy: country.macro_indicators.health_statistics.average_lifespan,
        literacy_rate: country
            .macro_indicators
            .education_statistics
            .literacy_rate,
        higher_education_rate: country
            .macro_indicators
            .education_statistics
            .higher_education_rate,
        urban_share: demo.city_urban,
        rural_share: demo.rural,
        nomad_share: demo.nomads,
        classes: class_rows,
    }
}

#[allow(dead_code)]
fn _unused(_r: &Region) {}
