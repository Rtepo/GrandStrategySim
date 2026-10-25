//! Divergence bisection probe: two modes.
//!
//! * `clone_twins`: clone a generated world, run two copies for N turns in
//!   one process, report the first turn where `state_fingerprint` differs.
//!   Catches rayon-schedule dependence (per-task reseeding gaps) but NOT
//!   HashMap-RandomState bugs — `HashMap::clone` preserves the hasher keys,
//!   so the twin iterates maps in the same order.
//! * `fp_dump`: single run writing one `state_fingerprint` per turn to
//!   `SES_FP_DUMP` (or stdout). Two process-level runs diffed offline reveal
//!   the first divergent turn — the only check that sees fresh per-process
//!   RandomState order. Confirmed catches: the `eligible_vips.first()` fund
//!   pick at the year-end block, and `check_natural_deaths` drawing
//!   `rng.gen` per `vips.keys()` (HashMap) element — draw↔VIP assignment
//!   varied per process and flipped `fund_manager_vip_id` lookups, which
//!   surfaced as `bids=8` vs `bids=11` inside `submit_fund_orders` at t=48.
//!
//! `SES_TURNS` (default 120) bounds the run; e.g. `SES_TURNS=27` covers the
//! first fiscal year boundary.
//!
//! Run: cargo test -p sim_engine --test run_to_run_divergence_probe -- --nocapture

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::{run_turn_inner, state_fingerprint};
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    fn turn_limit() -> u32 {
        std::env::var("SES_TURNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120)
    }

    #[test]
    #[ignore = "manual cross-process bisection tool — run with --ignored"]
    fn fp_dump() {
        let turns = turn_limit();
        let registries = Registries::native_only();
        let tmp = TempDir::new().expect("tmp");
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld {
            mut state, ..
        } = generate_world(tmp.path(), options, &registries).expect("worldgen failed");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("ctx load");
        let out_path = std::env::var("SES_FP_DUMP").ok();
        let mut lines = Vec::new();
        for turn in 0..turns {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut NoopProbe)
                .expect("turn failed");
            lines.push(format!("t={} fp={:016x}", turn, state_fingerprint(&state, &ctx)));
        }
        let text = lines.join("\n");
        if let Some(p) = out_path {
            std::fs::write(&p, &text).expect("write fp dump");
            eprintln!("wrote fp dump: {}", p);
        } else {
            eprintln!("{}", text);
        }
    }

    /// V419-6 probe: census faction domains + artisanal slack at t=0.
    /// Prints per-domain faction type, population, region-level artisanal
    /// slack (avail−alloc over FreePeasant+urban classes), and the scaled
    /// formation threshold — answers why `check_guild_formation_trigger`
    /// never fires on this world.
    #[test]
    #[ignore = "diagnostic probe — run explicitly"]
    fn guild_census() {
        let registries = Registries::native_only();
        let tmp = TempDir::new().expect("tmp");
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld {
            mut state, ..
        } = generate_world(tmp.path(), options, &registries).expect("worldgen failed");
        // country.regions is empty until regions.json is loaded into state.
        let mut ctx =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("ctx load");
        let turns = turn_limit();
        for _ in 0..turns {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut NoopProbe)
                .expect("turn failed");
        }
        let mut guilds = 0usize;
        let mut total_ws_ids = 0usize;
        for e in ctx.entities.values() {
            for c in e.companies.iter() {
                if let sim_engine::entities::LegalForm::Guild(gd) = &c.legal_form {
                    guilds += 1;
                    total_ws_ids += gd.member_workshop_ids.len();
                    if guilds <= 4 {
                        eprintln!(
                            "GUILDWS: {} members={} parcels_domain=?",
                            c.id,
                            gd.member_workshop_ids.len()
                        );
                    }
                }
            }
        }
        eprintln!(
            "GUILDCOUNT after {} turns: {} total_ws_ids={}",
            turns, guilds, total_ws_ids
        );
        for (name, country) in &state.countries {
            let mut counts: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for region in &country.regions {
                // region-level artisanal slack = unallocated FTE over
                // FreePeasant (rural) + all urban classes — mirrors the
                // guild-formation loop in turn.rs
                let slack = |d: &sim_engine::society::geography::ClassDemographics| {
                    (d.available_fte - d.allocated_fte).max(0.0)
                };
                let mut artisanal = 0.0;
                for (k, d) in region.class_demographics.rural_classes.iter() {
                    if matches!(
                        k,
                        sim_engine::society::geography::RuralClass::FreePeasant
                    ) {
                        artisanal += slack(d);
                    }
                }
                for d in region.class_demographics.urban_classes.values() {
                    artisanal += slack(d);
                }
                for micro in region.micro_regions.values() {
                    *counts
                        .entry(format!("{:?}", micro.faction_type))
                        .or_insert(0) += 1;
                    if matches!(
                        micro.faction_type,
                        sim_engine::society::geography::FactionDomainType::GuildBurgher
                    ) {
                        let threshold =
                            50.0 * (micro.population as f64 / 1000.0).max(1.0);
                        eprintln!(
                            "GBDOM: c={} region={} dom={} pop={} region_artisanal={:.0} thresh={:.0} {}",
                            name,
                            region.id,
                            micro.id,
                            micro.population,
                            artisanal,
                            threshold,
                            if artisanal > threshold { "FIRES" } else { "no" }
                        );
                    }
                }
            }
            eprintln!("DOMCENSUS[{}]: {:?}", name, counts);
        }
    }

    #[test]
    #[ignore = "diagnostic probe — run explicitly"]
    fn clone_twins() {
        let registries = Registries::native_only();
        let tmp = TempDir::new().expect("tmp");
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld {
            state: mut state_a, ..
        } = generate_world(tmp.path(), options, &registries).expect("worldgen failed");
        let mut ctx_a =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state_a).expect("ctx load");

        let mut state_b = state_a.clone();
        let mut ctx_b = ctx_a.clone();
        let turns = turn_limit();

        let mut diverged = false;
        for turn in 0..turns {
            run_turn_inner(&mut state_a, &registries, &mut ctx_a, &mut NoopProbe)
                .expect("run A turn failed");
            run_turn_inner(&mut state_b, &registries, &mut ctx_b, &mut NoopProbe)
                .expect("run B turn failed");
            let fa = state_fingerprint(&state_a, &ctx_a);
            let fb = state_fingerprint(&state_b, &ctx_b);
            eprintln!("FPX t={} a={:016x} b={:016x} {}", turn, fa, fb,
                if fa == fb { "" } else { " <-- DIVERGED" });
            if fa != fb {
                eprintln!("FIRST DIVERGENCE at turn {}", turn);
                diverged = true;
                break;
            }
        }
        eprintln!("probe complete: diverged={}", diverged);
    }

    /// v4.21 WS2 probe: household income vs GDP decomposition at horizon.
    /// For each country prints the gdp_breakdown components, total class
    /// smoothed_income (the EWMA household-income aggregate the retail
    /// pipeline reads), total class savings, population, and per-capita
    /// spending power — to localize where GDP fails to reach households.
    #[test]
    #[ignore = "diagnostic probe — run explicitly"]
    fn hh_income_probe() {
        let registries = Registries::native_only();
        let tmp = TempDir::new().expect("tmp");
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld {
            mut state, ..
        } = generate_world(tmp.path(), options, &registries).expect("worldgen failed");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("ctx load");
        let turns = turn_limit();
        for _ in 0..turns {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut NoopProbe)
                .expect("turn failed");
        }
        for (name, country) in &state.countries {
            let bd = &country.macro_indicators.gdp_breakdown;
            let mut sum_income = 0.0;
            let mut sum_savings = 0.0;
            let mut sum_pop = 0i64;
            let mut sum_capacity = 0.0;
            for region in &country.regions {
                for d in region
                    .class_demographics
                    .rural_classes
                    .values()
                    .chain(region.class_demographics.urban_classes.values())
                {
                    sum_income += d.smoothed_income.max(0.0);
                    sum_savings += d.savings.max(0.0);
                    sum_pop += d.population;
                    // Same capacity formula the retail pipeline uses.
                    sum_capacity += d.smoothed_income.max(0.0)
                        + d.savings.max(0.0) * 0.12;
                }
            }
            // Realized wage bill: offered_wage_per_fte × fulfilled_fte.
            let ents = ctx.entities.get(name);
            let wage_bill: f64 = ents
                .map(|e| {
                    e.companies
                        .iter()
                        .map(|c| c.offered_wage_per_fte.max(0.0) * c.fulfilled_fte as f64)
                        .sum()
                })
                .unwrap_or(0.0);
            let total_fte: f64 = ents
                .map(|e| {
                    e.companies
                        .iter()
                        .map(|c| c.fulfilled_fte as f64)
                        .sum()
                })
                .unwrap_or(0.0);
            // Rent flow: occupied_slots × rent_per_slot across housing stock.
            let rent_flow: f64 = ents
                .map(|e| {
                    e.housing_buildings
                        .iter()
                        .map(|h| {
                            h.primary_slots.occupied_slots as f64
                                * h.primary_slots.rent_per_slot
                        })
                        .sum()
                })
                .unwrap_or(0.0);
            eprintln!(
                "HH[{}]: gdp={:.3e} C={:.3e} impC={:.3e} I={:.3e} G={:.3e} NX={:.3e} \
                 | hh_income={:.3e} ({:.1}% gdp) savings={:.3e} pop={} \
                 pit={:.3} vat_svc={:.3} unemp={:.3} \
                 | wage_bill={:.3e} ({:.1}% gdp) fte={:.2e} avgw={:.1} \
                 rent={:.3e} ({:.1}% gdp) capacity={:.3e} spend/cap={:.2}",
                name,
                bd.official_gdp,
                bd.consumption,
                bd.imputed_consumption,
                bd.investment,
                bd.government_spending,
                bd.net_exports,
                sum_income,
                if bd.official_gdp > 0.0 {
                    sum_income / bd.official_gdp * 100.0
                } else {
                    0.0
                },
                sum_savings,
                sum_pop,
                country.tax_rates.income_tax.rate,
                country
                    .tax_rates
                    .vat
                    .get("services")
                    .map(|v| v.rate)
                    .unwrap_or(0.0),
                country.macro_indicators.labor_market.unemployment_rate,
                wage_bill,
                if bd.official_gdp > 0.0 {
                    wage_bill / bd.official_gdp * 100.0
                } else {
                    0.0
                },
                total_fte,
                if total_fte > 0.0 {
                    wage_bill / total_fte
                } else {
                    0.0
                },
                rent_flow,
                if bd.official_gdp > 0.0 {
                    rent_flow / bd.official_gdp * 100.0
                } else {
                    0.0
                },
                sum_capacity,
                if sum_capacity > 0.0 {
                    bd.consumption / sum_capacity
                } else {
                    0.0
                },
            );
        }
    }
}
