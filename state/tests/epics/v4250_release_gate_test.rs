//! v4.25.0 Release Gate — 7-workstream crucible.
//!
//! Runs the 1900-scenario world for 120 turns, retains the critical v4.24.0
//! invariants (energy, wealth ordering, war throttle, press liveness), and
//! adds the un-skippable v4.25.0 gates:
//!
//! 12. WS1 — world map: `shared_land_borders_count > 10` and
//!     `min_region_polygon_vertices >= 16` (connected continents, fractal
//!     borders).
//! 13. WS2 — military: `oob_has_branch_breakdown &&
//!     oob_regions_have_divisions && !wars[0].battles.is_empty()`.
//! 14. WS3 — land/cadastre: `total_arbitration_exposure < gdp * 0.20` per
//!     country and `physical_gold_reserves > 0.0` somewhere (CB gold).
//! 15. WS4 — press: every paper has unique article titles and no `KRS-`
//!     anywhere in serialized output.
//! 16. WS5 — sectors: `local_services_wage_share < 0.45`,
//!     `construction_fte > 25_000`, `waste_management_fte > 5_000` in
//!     major countries (>= 60% of the largest workforce).
//! 17. WS6 — statutes: serialized display strings contain no `_`, no
//!     `: true`, no `: false`.
//! 18. WS7 — diplomacy: `intelligence_agency.active_spies >= 10` and
//!     `!operations.is_empty()` for the major country.
//!
//! # Run
//! ```bash
//! cargo test --release --features epic-tests v4250_release_gate
//! ```

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::enums::Sector;
    use sim_engine::registries::Registries;
    use std::collections::{BTreeSet, HashMap};
    use tempfile::TempDir;

    const TURNS: u32 = 120;

    /// Recursively collects every JSON string leaf — the displayed content
    /// of a DTO — so value-level assertions ignore snake_case field names.
    fn collect_string_leaves(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => {
                for x in a {
                    collect_string_leaves(x, out);
                }
            }
            serde_json::Value::Object(m) => {
                for x in m.values() {
                    collect_string_leaves(x, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn v4250_release_gate_1900() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let data_dir = tmp.path();

        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1900,
            seed: Some(42),
        };

        let GeneratedWorld {
            mut state, ..
        } = generate_world(data_dir, options, &registries)
            .expect("world generation failed");

        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("failed to load turn context");

        let mut probe = NoopProbe;
        let mut press_failed_at: Option<(u32, String)> = None;
        let mut wars_t80: Option<usize> = None;
        let mut devastation_t80: Option<f64> = None;
        let mut religions_t50: Option<BTreeSet<String>> = None;

        for tick in 0..TURNS {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .unwrap_or_else(|e| panic!("turn {tick} failed: {e:?}"));

            // Press DTO must build every turn without panic/hang (retained).
            if press_failed_at.is_none() {
                for (cname, ents) in &ctx.entities {
                    let country = &state.countries[cname];
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        sim_engine::ui::snapshot_press::build_press_snapshot(
                            country,
                            &ents.companies,
                            &ents.unions,
                            &state.treaty_registry,
                            state.calendar.global_turn,
                            state.calendar.current_year,
                        )
                    }));
                    if let Err(e) = res {
                        press_failed_at = Some((
                            tick,
                            format!("{cname}: press snapshot panicked: {e:?}"),
                        ));
                        break;
                    }
                }
            }

            if tick == 50 && religions_t50.is_none() {
                let mut rels = BTreeSet::new();
                for country in state.countries.values() {
                    for (name, share) in &country
                        .macro_indicators
                        .demographics
                        .religious_composition
                    {
                        if *share > 0.005 {
                            rels.insert(name.clone());
                        }
                    }
                }
                religions_t50 = Some(rels);
            }

            if tick == 80 && wars_t80.is_none() {
                let mut pairs = BTreeSet::new();
                for (cname, country) in &state.countries {
                    for other in &country.at_war_with {
                        let mut p = [cname.clone(), other.clone()];
                        p.sort();
                        pairs.insert((p[0].clone(), p[1].clone()));
                    }
                }
                wars_t80 = Some(pairs.len());

                let mut total = 0.0f64;
                let mut n = 0u64;
                for country in state.countries.values() {
                    for (_, parcel) in country.cadastre.parcels.iter() {
                        total += parcel.devastation_index;
                        n += 1;
                    }
                }
                devastation_t80 = Some(if n > 0 { total / n as f64 } else { 0.0 });
            }
        }

        // -- Retained v4.24 invariants ----------------------------------------
        assert!(
            press_failed_at.is_none(),
            "press DTO failed to build: {:?}",
            press_failed_at
        );
        for (cname, country) in &state.countries {
            let supply: f64 = country
                .power_grid_state
                .region_supply_mw
                .values()
                .copied()
                .sum();
            assert!(
                supply > 100.0,
                "{cname}: national generation must exceed 100 MW (got {supply:.2})"
            );
        }
        let wars = wars_t80.expect("t80 war sample missing");
        assert!(wars <= 6, "war apocalypse: {wars} war pairs at t80 (cap 6)");
        let dev = devastation_t80.expect("t80 devastation sample missing");
        assert!(dev < 0.15, "devastation {dev:.3} >= 0.15 at t80");
        let rels = religions_t50.expect("t50 religion sample missing");
        assert!(rels.len() >= 2, "monoculture wipeout: {rels:?}");

        // Pooled class wealth ordering (aristocracy > 2x serf savings/capita).
        let (mut a_sav, mut a_pop, mut s_sav, mut s_pop) = (0.0f64, 0i64, 0.0f64, 0i64);
        for (cname, country) in &state.countries {
            let _ = cname;
            let society = sim_engine::ui::snapshot_society::build_society_snapshot(country);
            for row in &society.classes {
                if row.class_name == "Aristocracy" {
                    a_sav += row.savings_per_capita * row.population as f64;
                    a_pop += row.population;
                } else if row.class_name == "Serf" {
                    s_sav += row.savings_per_capita * row.population as f64;
                    s_pop += row.population;
                }
            }
        }
        if s_pop > 0 && a_pop > 0 {
            let aristo_avg = a_sav / a_pop as f64;
            let serf_avg = s_sav / s_pop as f64;
            assert!(
                aristo_avg > serf_avg * 2.0,
                "Aristocracy spc {aristo_avg:.2} must exceed 2x Serf {serf_avg:.2}"
            );
        }

        // -- Gate 12 (WS1): connected continents + fractal borders ------------
        let world_map = sim_engine::ui::snapshot_map::build_world_map(&state);
        assert!(
            world_map.shared_land_borders_count > 10,
            "shared land borders {} <= 10 — countries still isolated islands",
            world_map.shared_land_borders_count
        );
        assert!(
            world_map.min_region_polygon_vertices >= 16,
            "min region polygon vertices {} < 16 — borders still crude 4-edge",
            world_map.min_region_polygon_vertices
        );

        // -- Gate 13 (WS2): military OOB/war depth -----------------------------
        let military = sim_engine::ui::snapshot::build_military_dashboard(&state);
        assert!(
            military.oob_has_branch_breakdown,
            "OOB lacks Land/Navy/Air branch breakdown"
        );
        assert!(
            military.oob_regions_have_divisions,
            "OOB regions lack division/regiment counts"
        );
        assert!(
            !military.wars.is_empty(),
            "no active wars recorded — war ignition broken"
        );
        assert!(
            !military.wars[0].battles.is_empty(),
            "wars[0] has no nested battles list"
        );

        // -- Gate 14 (WS3): arbitration exposure bounded + CB gold -------------
        let mut any_gold = false;
        for (cname, country) in &state.countries {
            let report = sim_engine::society::real_estate_market::generate_ministry_land_report(
                &country.cadastre,
                &country.border_conflicts,
                &country.arbitration_court,
                &country.regions,
                state.calendar.global_turn,
            );
            let gdp = country.macro_indicators.gdp_breakdown.official_gdp.max(1.0);
            assert!(
                report.total_arbitration_exposure < gdp * 0.20,
                "{cname}: arbitration exposure {:.0} >= 20% of GDP {:.0}",
                report.total_arbitration_exposure,
                gdp
            );
            if country.central_bank.physical_gold_reserves > 0.0 {
                any_gold = true;
            }
        }
        assert!(
            any_gold,
            "no central bank holds any gold — reserves never seeded"
        );

        // -- Gate 15 (WS4): press dedup + no KRS- ------------------------------
        for (cname, ents) in &ctx.entities {
            let country = &state.countries[cname];
            let press = sim_engine::ui::snapshot_press::build_press_snapshot(
                country,
                &ents.companies,
                &ents.unions,
                &state.treaty_registry,
                state.calendar.global_turn,
                state.calendar.current_year,
            );
            let press_json = serde_json::to_string(&press).unwrap_or_default();
            assert!(
                !press_json.contains("KRS-"),
                "{cname}: press output contains raw KRS- company id"
            );
            for paper in &press.papers {
                let mut titles = BTreeSet::new();
                for article in &paper.articles {
                    assert!(
                        titles.insert(article.headline.clone()),
                        "{cname}/{}: duplicate article headline {:?}",
                        paper.name,
                        article.headline
                    );
                }
            }
        }

        // -- Gate 16 (WS5): sector employment rebalance ------------------------
        // Per-sector FTE + wage share (same proxy the Sector Overview shows).
        struct SectorAgg {
            fte: u32,
            wages: f64,
        }
        let mut country_sector: HashMap<String, HashMap<Sector, SectorAgg>> = HashMap::new();
        for (cname, ents) in &ctx.entities {
            let mut per: HashMap<Sector, SectorAgg> = HashMap::new();
            for c in &ents.companies {
                let e = per.entry(c.sector).or_insert(SectorAgg { fte: 0, wages: 0.0 });
                e.fte += c.fulfilled_fte;
                e.wages += c.offered_wage_per_fte * c.fulfilled_fte as f64;
            }
            country_sector.insert(cname.clone(), per);
        }
        // Major countries: workforce >= 60% of the largest.
        let max_fte = country_sector
            .values()
            .map(|m| m.values().map(|a| a.fte).sum::<u32>())
            .max()
            .unwrap_or(0);
        assert!(max_fte > 0, "no employed workers anywhere — economy dead");
        for (cname, per) in &country_sector {
            let total_fte: u32 = per.values().map(|a| a.fte).sum();
            let total_wages: f64 = per.values().map(|a| a.wages).sum();
            if (total_fte as u64) * 100 < (max_fte as u64) * 60 {
                continue; // minor country — out of scope
            }
            let ls_wages = per.get(&Sector::LocalServices).map(|a| a.wages).unwrap_or(0.0);
            let ls_share = if total_wages > 0.0 { ls_wages / total_wages } else { 0.0 };
            let construction = per.get(&Sector::Construction).map(|a| a.fte).unwrap_or(0);
            let waste = per.get(&Sector::WasteManagement).map(|a| a.fte).unwrap_or(0);
            // Diagnostic decomposition: which constraint binds construction
            // employment — project-driven demand, payroll pacing, or seats.
            {
                let ents = &ctx.entities[cname];
                let (mut dem, mut cap, mut cash) = (0u64, 0u64, 0.0f64);
                let (mut committed, mut recv, mut credit, mut ncos) =
                    (0.0f64, 0.0f64, 0.0f64, 0usize);
                let mut projects = 0usize;
                for c in &ents.companies {
                    if c.sector != Sector::Construction { continue; }
                    ncos += 1;
                    dem += c.target_fte_demand as u64;
                    cap += c.worker_capacity as u64;
                    cash += c.available_cash.max(0.0);
                    credit += c.procurement_credit.max(0.0);
                    committed += c
                        .extra
                        .get("committed_demand_fte")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    recv += c
                        .extra
                        .get("contract_receivables")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                }
                for b in &ents.buildings {
                    if b.active_project.is_some() { projects += 1; }
                }
                eprintln!(
                    "SECTOR_DBG[{cname}]: constr_fte={construction} demand={dem} committed={committed:.0} recv={recv:.0} credit={credit:.0} seats={cap} co_cash={cash:.0} ncos={ncos} active_projects={projects} waste_fte={waste}"
                );
            }
            assert!(
                ls_share < 0.45,
                "{cname}: LocalServices wage share {ls_share:.2} >= 0.45"
            );
            assert!(
                construction > 25_000,
                "{cname}: Construction FTE {construction} <= 25_000"
            );
            assert!(
                waste > 5_000,
                "{cname}: WasteManagement FTE {waste} <= 5_000"
            );
        }

        // -- Gate 17 (WS6): statutes display strings are human-readable --------
        for (cname, country) in &state.countries {
            let dto = sim_engine::ui::snapshot::build_constitution_laws(country);
            let json = serde_json::to_value(&dto).unwrap_or(serde_json::Value::Null);
            let mut leaves = Vec::new();
            collect_string_leaves(&json, &mut leaves);
            for leaf in &leaves {
                assert!(
                    !leaf.contains('_'),
                    "{cname}: statute string leaks snake_case: {leaf:?}"
                );
                assert!(
                    !leaf.contains(": true") && !leaf.contains(": false"),
                    "{cname}: statute string leaks raw bool: {leaf:?}"
                );
            }
        }

        // -- Gate 18 (WS7): intelligence agency present & active ---------------
        // Check on the largest country (a "major power").
        let major = country_sector
            .iter()
            .max_by_key(|(_, m)| m.values().map(|a| a.fte).sum::<u32>())
            .map(|(cname, _)| cname.clone())
            .expect("no countries");
        {
            let c = &state.countries[&major];
            let mut spy_n = 0u32;
            if let Some(reg) = &c.politics.vip_registry {
                for v in reg.vips.values() {
                    if v.is_dead {
                        continue;
                    }
                    if let Some(p) = &v.diplomatic_post {
                        if p.post_type
                            == sim_engine::politics::vip_registry::DiplomaticPostType::Spy
                        {
                            spy_n += 1;
                        }
                    }
                }
            }
            eprintln!(
                "INTEL_DBG[{major}]: gdp={:.0} reserves={:.0} intel_budget={:.0} spy_vips={} pending={} ops={}",
                c.budget.gdp,
                c.budget.liquid_reserves,
                c.intelligence_budget.current_budget,
                spy_n,
                state.pending_diplomatic_actions.len(),
                state.intelligence_agencies.get(&major).map(|a| a.operations.len()).unwrap_or(0)
            );
        }
        let diplo = sim_engine::ui::snapshot::build_diplomacy_snapshot(
            &state,
            &major,
            &ctx.diplomacy,
        );
        assert!(
            diplo.intelligence_agency.active_spies >= 10,
            "{major}: active_spies {} < 10 — operatives not scaled to major power",
            diplo.intelligence_agency.active_spies
        );
        assert!(
            !diplo.intelligence_agency.operations.is_empty(),
            "{major}: intelligence operations list empty — covert ops never generated"
        );

        eprintln!("v4250 release gate: all assertions passed");
    }
}
