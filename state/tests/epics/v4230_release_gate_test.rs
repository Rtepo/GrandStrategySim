//! v4.23.0 Release Gate — WS2–WS7 crucible assertions.
//!
//! Runs the 1900-scenario world for 120 turns and asserts the v4.23.0
//! release gates:
//!
//! 1. `elections_held <= 5` per country per 120 turns (snap-election spam
//!    guard — WS1 cooldown holds).
//! 2. Energy: national generation `supply_mw > 100.0` (the 1.3 MW blackout
//!    fix — plants actually dispatch).
//! 3. ExportServices companies record realized revenue > 0.0 (trade/freight
//!    margins wired into the sector).
//! 4. Society realism: `birth_rate < 8.0` (fertility 33.33 fix),
//!    `life_expectancy < 80.0` (90.8 fix), `age_cohorts.len() >= 7`
//!    (10-year pyramid cohorts), `!unionization_rate.is_nan()`.
//! 5. War bridge: declared diplomatic wars are reflected in military
//!    fronts — `declared_wars` non-empty implies `military_fronts`
//!    non-empty; no front exists for a war that was never declared.
//!
//! # Run
//! ```bash
//! cargo test --release --features epic-tests v4230_release_gate
//! ```

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::enums::Sector;
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    const TURNS: u32 = 120;

    #[test]
    fn v4230_release_gate_1900() {
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
        let mut export_services_sold = false;

        for _tick in 0..TURNS {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .unwrap_or_else(|e| panic!("turn {_tick} failed: {e:?}"));
            // Gate 3: sampled cumulatively — any settled sale by an
            // ExportServices-sector company proves the revenue wiring.
            if !export_services_sold {
                for ents in ctx.entities.values() {
                    if ents.companies.iter().any(|c| {
                        c.sector == Sector::ExportServices && c.turn_settled_sales > 0.0
                    }) {
                        export_services_sold = true;
                        break;
                    }
                }
            }
        }

        // -- Gate 1: snap-election discipline ------------------------------
        for (cname, country) in &state.countries {
            assert!(
                country.politics.elections_held <= 5,
                "{cname}: at most 5 elections per 120 turns (got {})",
                country.politics.elections_held
            );
        }

        // -- Gate 2: energy generation (blackout fix) ----------------------
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

        // -- Gate 3: ExportServices revenue --------------------------------
        assert!(
            export_services_sold,
            "no ExportServices company recorded settled revenue in 120 turns"
        );

        // -- Gate 4: society realism ----------------------------------------
        for (cname, country) in &state.countries {
            let society = sim_engine::ui::snapshot_society::build_society_snapshot(country);
            assert!(
                society.birth_rate < 8.0,
                "{cname}: birth_rate {:.2} >= 8.0 — fertility fix missing",
                society.birth_rate
            );
            assert!(
                society.life_expectancy < 80.0,
                "{cname}: life_expectancy {:.1} >= 80 — 1900-era realism fix missing",
                society.life_expectancy
            );
            assert!(
                society.age_cohorts.len() >= 7,
                "{cname}: pyramid must have >=7 ten-year cohorts (got {})",
                society.age_cohorts.len()
            );
            let ents = &ctx.entities[cname];
            let orgs = sim_engine::ui::snapshot_society::build_organizations_summary(
                country,
                &ents.companies,
                &ents.unions,
            );
            assert!(
                !orgs.unionization_rate.is_nan(),
                "{cname}: unionization_rate is NaN"
            );
        }

        // -- Gate 5: diplomatic wars ↔ military fronts ---------------------
        // Hard invariant: every front is between countries that are actually
        // at war (diplomatic_active_wars == military_active_wars).
        for (cname, country) in &state.countries {
            for front in &country.military_fronts {
                let at_war_pair = front.involved_countries.iter().any(|a| {
                    front.involved_countries.iter().any(|b| {
                        a != b
                            && state
                                .countries
                                .get(a)
                                .map(|c| c.at_war_with.contains(b))
                                .unwrap_or(false)
                    })
                });
                assert!(
                    at_war_pair,
                    "{cname}: front {:?} has no belligerent pair actually at war \
                     (diplomatic wars not bridged to military fronts)",
                    front.id
                );
            }
        }
        // If friction produced active wars, at least one front must exist.
        let any_at_war = state
            .countries
            .values()
            .any(|c| !c.at_war_with.is_empty());
        if any_at_war {
            let total_fronts: usize = state
                .countries
                .values()
                .map(|c| c.military_fronts.len())
                .sum();
            assert!(
                total_fronts > 0,
                "countries are at war but no military fronts were spawned"
            );
        }
    }
}
