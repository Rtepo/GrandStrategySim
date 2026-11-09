//! v4.32.0 diagnostic: 240-turn headless run tracking per-region population
//! to localize the exponential 9.24B overflow + the post-5-year crash.

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::{ClassPopProbe, NoopProbe};
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use std::io::Write;
    use tempfile::TempDir;

    /// Phase attribution: run 40 turns with `ClassPopProbe` and dump the
    /// phases that injected population. Cheap enough to iterate quickly.
    #[test]
    fn v4320_population_phase_attribution_40_turns() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 12,
            start_year: StartYear::Y1900,
            seed: Some(43200),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(data_dir, &mut state).expect("load turn context");
        let mut probe = ClassPopProbe::default();
        for _ in 0..40u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe).expect("turn");
        }
        let mut agg: std::collections::BTreeMap<String, i64> = Default::default();
        for (t, phase, delta) in &probe.deltas {
            eprintln!("POPΔ t={t} [{phase}]: {delta:+}");
            *agg.entry(phase.clone()).or_default() += *delta;
        }
        eprintln!("=== POPΔ REGION ATTRIBUTION (last 25) ===");
        for (t, phase, regions) in probe.region_deltas.iter().rev().take(25).rev() {
            eprintln!("POPREG t={t} [{phase}]: {}", regions.join(" "));
        }
        eprintln!("=== POPΔ AGGREGATE BY PHASE ===");
        for (phase, total) in &agg {
            eprintln!("  {phase}: {total:+}");
        }
    }

    #[test]
    fn v4320_population_diagnostic_240_turns() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 12,
            start_year: StartYear::Y1900,
            seed: Some(43200),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(data_dir, &mut state).expect("load turn context");
        let mut probe = NoopProbe;

        let out_path = std::path::PathBuf::from("tests/diagnostic_output/v4320_pop_trace.txt");
        if let Some(p) = out_path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let mut out = std::fs::File::create(&out_path).expect("trace file");

        for t in 0..240u32 {
            let world_pop: i64 = state
                .countries
                .values()
                .flat_map(|c| c.regions.iter().map(|r| r.population))
                .sum();
            let (max_region, max_pop, max_owner) = state
                .countries
                .values()
                .flat_map(|c| c.regions.iter().map(move |r| (r, c.name.clone())))
                .map(|(r, n)| (r.id.clone(), r.population, n))
                .max_by_key(|(_, p, _)| *p)
                .unwrap_or_default();
            let class_sum: i64 = state
                .countries
                .values()
                .flat_map(|c| {
                    c.regions.iter().flat_map(|r| {
                        r.class_demographics
                            .rural_classes
                            .values()
                            .chain(r.class_demographics.urban_classes.values())
                            .map(|d| d.population)
                            .collect::<Vec<_>>()
                    })
                })
                .sum();
            writeln!(
                out,
                "T{t:>3} world_pop={world_pop:>15} class_sum={class_sum:>15} max={max_pop:>15} @ {max_region} ({max_owner})"
            )
            .ok();
            eprintln!(
                "T{t:>3} world_pop={world_pop:>15} class_sum={class_sum:>15} max={max_pop:>15} @ {max_region} ({max_owner})"
            );
            if t % 10 == 0 || t > 90 {
                for (cname, c) in state.countries.iter() {
                    let cp: i64 = c.regions.iter().map(|r| r.population).sum();
                    let d = &c.macro_indicators.demographics;
                    eprintln!(
                        "    {cname:>16}: pop={cp:>13} budget_pop={:>13} birth={:.3} death={:.3} net_mig={:.4} last_births={:.0}",
                        c.budget.population, d.birth_rate, d.death_rate, d.net_migration, d.last_births
                    );
                }
                if let Some(bactria) = state.countries.get("Bactria") {
                    if let Some(r) = bactria.regions.iter().max_by_key(|r| r.population) {
                        for (k, d) in r.class_demographics.rural_classes.iter() {
                            eprintln!("      rural {k:?}: pop={} fte={:.0}", d.population, d.available_fte);
                        }
                        for (k, d) in r.class_demographics.urban_classes.iter() {
                            eprintln!("      urban {k:?}: pop={} fte={:.0}", d.population, d.available_fte);
                        }
                    }
                }
            }
            // v4.32.0 release gate: the 9.24B overflow stays dead — no
            // region may breach 200M at/after turn 120.
            if t + 1 == 120 {
                assert!(
                    max_pop < 200_000_000,
                    "max region population {max_pop} @ {max_region} breaches 200M at turn 120"
                );
            }
            assert!(
                max_pop < 200_000_000 || t + 1 < 120,
                "max region population {max_pop} @ {max_region} exploded before turn {}",
                t + 1
            );
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .unwrap_or_else(|e| panic!("turn {t} failed: {e:?}"));
        }
        // 240 turns completed without panic — release gate satisfied.
    }
}
