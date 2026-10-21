//! V418-2 fast-suite regression guard: aggregate CPI-deflated (real)
//! consumption must not move more than 15% in a single turn once the
//! genesis ramp has cleared (transitions ending at turn >= 12, same window
//! as the epic crucible's consumption-smoothing gate). Nominal revenue
//! carries ±5%/turn retail price noise that is not a smoothing break.
//! Prints per-country monetized C,
//! class spend capacity, class savings, wages paid, bond maturities, and
//! retail VWAP traces so a failure is immediately diagnosable.
//!
//! Historical causes this guards: retail price stickiness gaps (markup
//! factors flipping in one turn), wholesale-vs-retail demand valuation,
//! and synchronized savings-bond maturity walls pulsing household savings.
#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::entities::LegalForm;
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    #[test]
    fn c_spike_probe() {
        let tmp = TempDir::new().expect("tmp");
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(tmp.path(), options, &registries).expect("worldgen failed");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("ctx load failed");
        let mut probe = NoopProbe;
        let mut prev_sold: std::collections::BTreeMap<(String, sim_engine::registries::enums::Commodity), f64> =
            std::collections::BTreeMap::new();
        let mut prev_agg_c_real = 0.0f64;
        let mut worst: (f64, u32) = (0.0, 0);

        for t in 0..96u32 {
            if t >= 10 {
                // Wholesale + retail VWAP trace — does B2B or B2C price spike
                // before the measured-C swing?
                use sim_engine::registries::enums::Commodity as Cm;
                let vw: String = [Cm::Clothing, Cm::Cereal, Cm::Furniture, Cm::Vegetable, Cm::Fruit]
                    .iter()
                    .map(|cm| {
                        format!(
                            "{:?}={:.0}",
                            cm,
                            state
                                .market_history
                                .vwap_per_commodity
                                .get(cm)
                                .copied()
                                .unwrap_or(0.0)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("  VWAP t={}: {}", t, vw);
                if t >= 12 {
                    // Realized B2C price per commodity (revenue-weighted).
                    let rv: String = state
                        .market_history
                        .retail_vwap_per_commodity
                        .iter()
                        .map(|(cm, p)| format!("{:?}={:.0}", cm, p))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!("  RVWAP t={}: {}", t, rv);
                }
            }
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .expect("turn failed");
            {
                // Real (CPI-deflated) aggregate consumption — the V418-2
                // invariant is household real-consumption smoothing; nominal
                // revenue carries ±5%/turn retail price noise.
                let mut agg = 0.0f64;
                let mut agg_real = 0.0f64;
                for c in state.countries.values() {
                    let nom = c.macro_indicators.gdp_breakdown.consumption;
                    agg += nom;
                    let idx = c.macro_indicators.inflation_indices.cpi_index.max(1e-6);
                    agg_real += nom / idx;
                }
                if prev_agg_c_real > 0.0 {
                    let d = (agg_real - prev_agg_c_real) / prev_agg_c_real * 100.0;
                    if t >= 12 && d.abs() > worst.0.abs() {
                        worst = (d, t);
                    }
                    eprintln!("AGGDC t={} aggC={:.1}M aggCr={:.1}M dCr={:+.1}%{}", t, agg / 1e6, agg_real / 1e6, d,
                        if t >= 12 && d.abs() > 15.0 { "  <<< VIOLATION" } else { "" });
                }
                prev_agg_c_real = agg_real;
            }
            if t >= 12 {
                let mut names: Vec<&String> = state.countries.keys().collect();
                names.sort();
                for n in names {
                    let c = &state.countries[n];
                    let measured_c = c.macro_indicators.gdp_breakdown.consumption;
                    let imputed = c.macro_indicators.gdp_breakdown.imputed_consumption;
                    let mut cap = 0.0f64;
                    let mut savings = 0.0f64;
                    for r in &c.regions {
                        for d in r
                            .class_demographics
                            .rural_classes
                            .values()
                            .chain(r.class_demographics.urban_classes.values())
                        {
                            cap += sim_engine::economy::trade::retail::class_spend_capacity(
                                d,
                                c.macro_indicators.average_wage,
                            );
                            savings += d.savings;
                        }
                    }
                    let ents = ctx.entities.get(n.as_str());
                    let wages: f64 = ents
                        .map(|e| e.companies.iter().map(|x| x.wages_paid_this_turn).sum())
                        .unwrap_or(0.0);
                    let guilds = ents
                        .map(|e| {
                            e.companies
                                .iter()
                                .filter(|x| matches!(x.legal_form, LegalForm::Guild(_)))
                                .count()
                        })
                        .unwrap_or(0);
                    let (active_mv, mean_dis) = {
                        let act: Vec<_> = c
                            .politics
                            .mass_movements
                            .iter()
                            .filter(|m| {
                                m.status
                                    == sim_engine::politics::mass_movements::MassMovementStatus::Active
                            })
                            .collect();
                        let dis: f64 = ents
                            .map(|e| {
                                e.companies.iter().map(|x| x.temporary_disruption_modifier).sum::<f64>()
                                    / e.companies.len().max(1) as f64
                            })
                            .unwrap_or(0.0);
                        (act.len(), dis)
                    };
                    eprintln!(
                        "CSPIKE t={} c={} measC={:.1}M imp={:.1}M cap={:.1}M sav={:.1}M wages={:.1}M mv={}/{} dis={:.1}% guilds={}",
                        t,
                        n,
                        measured_c / 1e6,
                        imputed / 1e6,
                        cap / 1e6,
                        savings / 1e6,
                        wages / 1e6,
                        active_mv,
                        c.politics.mass_movements.len(),
                        mean_dis * 100.0,
                        guilds
                    );
                    // Retail savings-bond stock: count, face value, and how much
                    // of it is scheduled to pay out (annual coupon or maturity)
                    // on the NEXT debt-service pass — the t24 wave hypothesis.
                    {
                        let (mut nb, mut fv, mut due) = (0usize, 0f64, 0f64);
                        for b in &c.debt_market.retail_bonds {
                            nb += 1;
                            fv += b.face_value;
                            let coupon_due = ((t + 1).saturating_sub(b.issue_turn))
                                % 24
                                == 0;
                            let matures = b.turns_remaining <= 1;
                            if coupon_due {
                                due += b.face_value * b.interest_rate;
                            }
                            if matures {
                                due += b.face_value;
                            }
                        }
                        eprintln!(
                            "  BONDS t={} {}: n={} fv={:.1}M due_next={:.1}M",
                            t, n, nb, fv / 1e6, due / 1e6
                        );
                    }
                    // Per-commodity sold deltas — which commodity legs moved.
                    if let Some(e) = ents {
                        let mut sold_now: std::collections::BTreeMap<sim_engine::registries::enums::Commodity, f64> =
                            std::collections::BTreeMap::new();
                        for store in &e.commercial_buildings {
                            if let Some(p) = &store.retail_profile {
                                for (cm, q) in &p.units_sold_last_turn {
                                    *sold_now.entry(*cm).or_insert(0.0) += q;
                                }
                            }
                        }
                        let mut deltas: Vec<(f64, sim_engine::registries::enums::Commodity)> = sold_now
                            .iter()
                            .map(|(cm, q)| {
                                (
                                    *q - prev_sold
                                        .get(&(n.to_string(), *cm))
                                        .copied()
                                        .unwrap_or(0.0),
                                    *cm,
                                )
                            })
                            .collect();
                        deltas.sort_by(|a, b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
                        let top: String = deltas
                            .iter()
                            .take(4)
                            .map(|(d, cm)| format!("{:?}:{:+.0}", cm, d))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let total_units: f64 = sold_now.values().sum();
                        eprintln!(
                            "  SOLD t={} {}: {} | units={:.0} implied_px={:.1}",
                            t,
                            n,
                            top,
                            total_units,
                            if total_units > 0.0 {
                                measured_c / total_units
                            } else {
                                0.0
                            }
                        );
                        for (cm, q) in sold_now {
                            prev_sold.insert((n.to_string(), cm), q);
                        }
                    }
                    // Per-class savings deltas around the t24 year boundary —
                    // find which class absorbs the +430M lump.
                    if n.as_str() == "Dacia" && (22..=27).contains(&t) {
                        for r in &c.regions {
                            for (k, d) in &r.class_demographics.rural_classes {
                                eprintln!(
                                    "    CLS t={} {} rural {:?}: sav={:.1}M pop={} smi={:.0} fte={:.0}",
                                    t, r.id, k, d.savings / 1e6, d.population, d.smoothed_income, d.allocated_fte
                                );
                            }
                            for (k, d) in &r.class_demographics.urban_classes {
                                eprintln!(
                                    "    CLS t={} {} urban {:?}: sav={:.1}M pop={} smi={:.0} fte={:.0}",
                                    t, r.id, k, d.savings / 1e6, d.population, d.smoothed_income, d.allocated_fte
                                );
                            }
                        }
                    }
                    // Per-commodity units for Dacia — the mix-shift leg.
                    if n.as_str() == "Dacia" && t >= 13 {
                        if let Some(e) = ents {
                            let mut sold_map: std::collections::BTreeMap<String, f64> =
                                std::collections::BTreeMap::new();
                            for store in &e.commercial_buildings {
                                if let Some(p) = &store.retail_profile {
                                    for (cm, q) in &p.units_sold_last_turn {
                                        *sold_map
                                            .entry(format!("{:?}", cm))
                                            .or_insert(0.0) += q;
                                    }
                                }
                            }
                            let s: String = sold_map
                                .iter()
                                .map(|(k, v)| {
                                    let px = state
                                        .market_history
                                        .retail_vwap_per_commodity
                                        .iter()
                                        .find(|(cm, _)| format!("{:?}", cm) == *k)
                                        .map(|(_, p)| *p)
                                        .unwrap_or(0.0);
                                    format!("{}={:.0}@{:.0}", k, v, px)
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            eprintln!("    MIX t={} Dacia: {}", t, s);
                        }
                    }
                }
            }
        }

        // Hard gate: the same ±15% window the epic crucible asserts, on the
        // real (CPI-deflated) series.
        assert!(
            worst.0.abs() <= 15.0,
            "V418-2 regression: real |dC/C| {:.1}% at turn {} exceeds 15%",
            worst.0,
            worst.1
        );
    }
}
