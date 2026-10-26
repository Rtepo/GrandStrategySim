//! v4.22.0 Release Gate — Y1900 scenario assertions.
//!
//! Runs the 1900-scenario world for 120 turns and asserts the mandate's
//! release gates (section 5 of the v4.22.0 mandate):
//!
//! 1. `turn_10` observes January 1901 (calendar rolls over in January,
//!    not August — the WS1 offset=14 fix).
//! 2. Anachronistic goods are gated: `demand_volume(Software) == 0.0` and
//!    `demand_volume(Radio) == 0.0` while the calendar is in 1900.
//! 3. `supply_volume(Fertilizers) > 0.0` — chemical plants seeded at Turn 0.
//! 4. Peacetime homefront morale sits naturally above 50 and fewer than
//!    15% of classes are low-morale strikers (the 848-classes-24.0 fix).
//! 5. Fog of war calibrated: at most 2 countries hide their government.
//! 6. Society/Press DTOs are populated (`age_cohorts`, `articles`).
//!
//! # Run
//! ```bash
//! cargo test --release --features epic-tests v4220_release_gate
//! ```

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::enums::Commodity;
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    const TURNS: u32 = 120;

    #[test]
    fn v4220_release_gate_1900() {
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

        // -- Gate 1: calendar rollover -------------------------------------
        // The crucible samples calendar state BEFORE each run_turn_inner.
        // global_turn == 10 at that point ⇒ Jan 1901 (h = 10 + 14 = 24).
        let mut turn10_year = 0u32;
        let mut turn10_month = 0u32;
        // Demand/supply sampled at turn 0 (1900 world, before any turns).
        let mut demand_1900_software = 1.0f64;
        let mut demand_1900_radio = 1.0f64;
        let mut supply_fertilizers = 0.0f64;

        for tick in 0..TURNS {
            if tick == 0 {
                demand_1900_software = ctx
                    .market
                    .demand_volume
                    .get(&Commodity::Software)
                    .copied()
                    .unwrap_or(0.0);
                demand_1900_radio = ctx
                    .market
                    .demand_volume
                    .get(&Commodity::Radio)
                    .copied()
                    .unwrap_or(0.0);
            }
            if tick == 10 {
                turn10_year = state.calendar.current_year;
                turn10_month = state.calendar.current_month;
            }
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .unwrap_or_else(|e| panic!("turn {tick} failed: {e:?}"));
            // Fertilizer supply is asserted cumulatively — any turn with
            // positive sell-order volume proves active production.
            let fv = ctx
                .market
                .supply_volume
                .get(&Commodity::Fertilizers)
                .copied()
                .unwrap_or(0.0);
            if fv > supply_fertilizers {
                supply_fertilizers = fv;
            }
        }

        assert_eq!(
            (turn10_year, turn10_month),
            (1901, 1),
            "turn 10 must observe January 1901 (got {turn10_year}-{turn10_month:02})"
        );
        assert_eq!(
            demand_1900_software, 0.0,
            "Software demand in 1900 must be zero (uninvented)"
        );
        assert_eq!(
            demand_1900_radio, 0.0,
            "Radio demand in 1900 must be zero (uninvented)"
        );
        assert!(
            supply_fertilizers > 0.0,
            "Fertilizers must have positive market supply in the 1900 scenario"
        );

        // -- Gate 4: peacetime morale --------------------------------------
        let dash = sim_engine::ui::snapshot::build_military_dashboard(&state);
        assert!(
            !dash.war_morale.is_empty(),
            "war_morale rows must be populated"
        );
        let avg_morale = dash
            .war_morale
            .iter()
            .map(|m| m.war_morale)
            .sum::<f64>()
            / dash.war_morale.len() as f64;
        let low_striking = dash
            .war_morale
            .iter()
            .filter(|m| m.war_morale < 30.0 && m.strikes_active)
            .count() as f64
            / dash.war_morale.len() as f64;
        assert!(
            avg_morale > 50.0,
            "peacetime avg homefront morale must exceed 50 (got {avg_morale:.1})"
        );
        assert!(
            low_striking < 0.15,
            "low-morale striking classes share must stay under 15% (got {low_striking:.2})"
        );

        // -- Gate 5: fog of war ---------------------------------------------
        let player = state.countries.keys().next().unwrap().clone();
        let rows = sim_engine::ui::snapshot::build_foreign_country_rows(
            &state,
            &player,
            &ctx.diplomacy,
        );
        let known_gov = rows.iter().filter(|r| r.government_known).count();
        assert!(
            known_gov >= rows.len().saturating_sub(2),
            "at most 2 foreign governments may be unknown ({known_gov}/{} known)",
            rows.len()
        );

        // -- Gate 6: society + press DTOs -----------------------------------
        for (cname, country) in &state.countries {
            let society = sim_engine::ui::snapshot_society::build_society_snapshot(country);
            assert!(
                !society.age_cohorts.is_empty(),
                "{cname}: society age_pyramid must not be empty"
            );
            let ents = &ctx.entities[cname];
            let press = sim_engine::ui::snapshot_press::build_press_snapshot(
                country,
                &ents.companies,
                &ents.unions,
                &state.treaty_registry,
                state.calendar.global_turn,
                state.calendar.current_year,
            );
            assert!(
                press.total_articles > 0,
                "{cname}: press articles must not be empty"
            );
        }
    }
}
