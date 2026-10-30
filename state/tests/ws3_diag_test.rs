//! TEMPORARY WS3 diagnostic — measure arbitration exposure magnitudes vs GDP.
//! Delete before commit.

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use sim_engine::society::cadastre::ArbitrationStatus;
    use tempfile::TempDir;

    #[test]
    fn ws3_arbitration_magnitude_diag() {
        let tmp = TempDir::new().expect("tmp");
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1900,
            seed: Some(42),
        };
        let GeneratedWorld {
            mut state, ..
        } = generate_world(tmp.path(), options, &registries).expect("gen");
        let mut ctx = InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("ctx");
        let mut probe = NoopProbe;

        for (name, c) in &state.countries {
            let gdp = c.macro_indicators.gdp_breakdown.official_gdp;
            let total_land: f64 = c.cadastre.parcels.values().map(|p| p.current_value).sum();
            let max_p = c
                .cadastre
                .parcels
                .values()
                .map(|p| p.current_value)
                .fold(0.0_f64, f64::max);
            let n = c.cadastre.parcels.len();
            eprintln!(
                "WORLDGEN {name}: gdp={gdp:.0} total_land={total_land:.0} parcels={n} max_parcel={max_p:.0} treasury={:.0}",
                c.budget.liquid_reserves
            );
        }

        for tick in 0..40 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .unwrap_or_else(|e| panic!("turn {tick} failed: {e:?}"));
            if tick % 10 == 9 || tick == 0 {
                for (name, c) in &state.countries {
                    let court = &c.arbitration_court;
                    let mut pend = 0;
                    let mut med = 0;
                    let mut inc = 0;
                    let mut ruled = 0;
                    let mut settled = 0;
                    let mut dism = 0;
                    let mut max_claim = 0.0_f64;
                    let mut sum_claim = 0.0_f64;
                    for case in court.cases.values() {
                        match case.status {
                            ArbitrationStatus::Pending => pend += 1,
                            ArbitrationStatus::InMediation => med += 1,
                            ArbitrationStatus::InCourt => inc += 1,
                            ArbitrationStatus::RuledForPlaintiff => ruled += 1,
                            ArbitrationStatus::Settled => settled += 1,
                            ArbitrationStatus::Dismissed => dism += 1,
                            _ => {}
                        }
                        max_claim = max_claim.max(case.compensation_claimed);
                        sum_claim += case.compensation_claimed;
                    }
                    eprintln!(
                        "T{} {name}: cases={} pend={} med={} inc={} ruled={} settled={} dism={} exposure={:.0} owed={:.0} maxclaim={:.0} sumclaim={:.0} gdp={:.0}",
                        tick,
                        court.cases.len(),
                        pend,
                        med,
                        inc,
                        ruled,
                        settled,
                        dism,
                        court.unresolved_liabilities(),
                        court.total_compensation_owed,
                        max_claim,
                        sum_claim,
                        c.macro_indicators.gdp_breakdown.official_gdp
                    );
                }
            }
        }
    }
}
