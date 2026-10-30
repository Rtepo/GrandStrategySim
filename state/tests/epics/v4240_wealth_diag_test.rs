//! v4.24.0 diagnostic: dump class savings dynamics per turn.
//!
//! v4.25.0 WS5: also dumps per-country per-sector employment (fulfilled FTE,
//! worker capacity, wage-weighted "GDP share" proxy matching the Sector
//! Overview aggregation) so the sector labor-balance gates are observable:
//! Construction > 25k FTE, WasteManagement > 5k FTE, LocalServices < 45% of
//! the wage-share proxy in major countries at t=40.

use sim_engine::engine::diagnostic::NoopProbe;
use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::registries::Registries;
use sim_engine::society::geography::RuralClass;
use tempfile::TempDir;

/// Print a per-sector employment table for one country (Sector Overview basis:
/// share = sector wages / total wages).
fn print_sector_table(tick: u32, cname: &str, companies: &[sim_engine::entities::Company]) {
    use std::collections::BTreeMap;
    #[allow(clippy::type_complexity)]
    let mut by_sector: BTreeMap<String, (usize, u32, u32, u32, u32, f64)> = BTreeMap::new();
    for c in companies {
        let e = by_sector
            .entry(format!("{:?}", c.sector))
            .or_insert((0, 0, 0, 0, 0, 0.0));
        e.0 += 1;
        e.1 += c.fulfilled_fte;
        e.2 += c.worker_capacity;
        e.3 += c.target_fte_demand;
        e.4 += c.physical_fte_demand;
        e.5 += c.offered_wage_per_fte * c.fulfilled_fte as f64;
    }
    let total_wages: f64 = by_sector.values().map(|v| v.5).sum();
    let total_fte: u32 = by_sector.values().map(|v| v.1).sum();
    let mut rows: Vec<_> = by_sector.into_iter().collect();
    rows.sort_by(|a, b| b.1 .5.partial_cmp(&a.1 .5).unwrap_or(std::cmp::Ordering::Equal));
    eprintln!("SECTOR-TABLE t{tick} {cname}: total_fte={total_fte} total_wages={total_wages:.0}");
    for (name, (n, fte, cap, tgt, phys, wages)) in rows {
        let share = if total_wages > 0.0 {
            wages / total_wages
        } else {
            0.0
        };
        eprintln!(
            "  {name}: companies={n} fte={fte} cap={cap} tgt={tgt} phys={phys} wage_share={share:.3} avg_wage={:.1}",
            if fte > 0 { wages / fte as f64 } else { 0.0 }
        );
    }
}

#[test]
fn v4240_wealth_diag() {
    let tmp = TempDir::new().expect("temp dir");
    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1900,
        seed: Some(42),
    };
    let GeneratedWorld { mut state, .. } =
        generate_world(tmp.path(), options, &registries).expect("worldgen");
    let mut ctx =
        InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("turn ctx");
    let mut probe = NoopProbe;

    for tick in 0..40 {
        run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
            .unwrap_or_else(|e| panic!("turn {tick} failed: {e:?}"));

        // v4.25.0 WS5: sector employment table every 10 turns + final turn.
        if tick % 10 == 9 || tick == 39 {
            for (cname, ents) in &ctx.entities {
                print_sector_table(tick, cname, &ents.companies);
            }
        }

        // Fine-grained collapse window for Dacia.
        if (5..30).contains(&tick) {
            let country = &state.countries["Dacia"];
            let mut emp_total = 0u32;
            let mut capacity_total = 0u32;
            let mut corp_cash = 0.0f64;
            let mut furloughed = 0.0f64;
            let mut energy_cash = 0.0f64;
            let mut energy_emp = 0u32;
            for c in &ctx.entities["Dacia"].companies {
                emp_total += c.prev_fulfilled_fte;
                capacity_total += c.worker_capacity;
                corp_cash += c.available_cash;
                furloughed += c.furloughed_workers_count;
                if c.sector == sim_engine::registries::enums::Sector::Energy {
                    energy_cash += c.available_cash;
                    energy_emp += c.prev_fulfilled_fte;
                }
            }
            // Mining companies: do fuel producers still staff and produce?
            let mut mine_emp = 0u32;
            let mut mine_cap = 0u32;
            let mut mine_cash = 0.0f64;
            let mut mine_furl = 0.0f64;
            let mut mine_n = 0usize;
            for c in &ctx.entities["Dacia"].companies {
                if c.sector == sim_engine::registries::enums::Sector::Mining {
                    mine_n += 1;
                    mine_emp += c.prev_fulfilled_fte;
                    mine_cap += c.worker_capacity;
                    mine_cash += c.available_cash;
                    mine_furl += c.furloughed_workers_count;
                }
            }
            eprintln!(
                "DACIA-MINE t{tick}: n={} emp={} cap={} cash={:.1}M furl={:.0}",
                mine_n, mine_emp, mine_cap, mine_cash / 1e6, mine_furl
            );
            let unemp;
            let mut avail = 0.0f64;
            let mut ll_sav = 0.0;
            let mut ar_sav = 0.0;
            let mut se_sav = 0.0;
            for region in &country.regions {
                let rc = &region.class_demographics.rural_classes;
                avail += rc.values().map(|d| d.available_fte).sum::<f64>();
                if let Some(d) = rc.get(&RuralClass::LandlessLaborer) {
                    ll_sav += d.savings;
                }
                if let Some(d) = rc.get(&RuralClass::Aristocracy) {
                    ar_sav += d.savings;
                }
                if let Some(d) = rc.get(&RuralClass::Serf) {
                    se_sav += d.savings;
                }
            }
            unemp = country.macro_indicators.labor_market.unemployment_rate;
            eprintln!(
                "DACIA t{tick}: reserves={:.0} corp_emp={} corp_cap={} corp_cash={:.2}B furl={:.0} e_cash={:.1}M e_emp={} unemp={:.2} avail_fte={:.0} ll={:.0}M ar={:.0}M se={:.0}M",
                country.budget.liquid_reserves, emp_total, capacity_total, corp_cash / 1e9, furloughed,
                energy_cash / 1e6, energy_emp, unemp, avail,
                ll_sav / 1e6, ar_sav / 1e6, se_sav / 1e6,
            );
            continue;
        }

        if tick % 20 != 19 {
            continue;
        }
        for (cname, country) in &state.countries {
            let mut a_pop = 0i64;
            let mut a_sav = 0.0;
            let mut s_pop = 0i64;
            let mut s_sav = 0.0;
            for region in &country.regions {
                let rc = &region.class_demographics.rural_classes;
                if let Some(d) = rc.get(&RuralClass::Aristocracy) {
                    a_pop += d.population;
                    a_sav += d.savings;
                }
                if let Some(d) = rc.get(&RuralClass::Serf) {
                    s_pop += d.population;
                    s_sav += d.savings;
                }
            }
            eprintln!(
                "T{tick} {cname}: aristo spc={:.2} serf spc={:.2} ratio={:.2}",
                if a_pop > 0 { a_sav / a_pop as f64 } else { 0.0 },
                if s_pop > 0 { s_sav / s_pop as f64 } else { 0.0 },
                if s_sav > 0.0 && s_pop > 0 && a_pop > 0 {
                    (a_sav / a_pop as f64) / (s_sav / s_pop as f64)
                } else {
                    f64::INFINITY
                }
            );
        }
    }

    // Final sector tables for every country (gate observability).
    for (cname, ents) in &ctx.entities {
        print_sector_table(40, cname, &ents.companies);
    }

    for (cname, country) in &state.countries {
        let mut a_pop = 0i64;
        let mut a_sav = 0.0;
        let mut s_pop = 0i64;
        let mut s_sav = 0.0;
        let mut ll_sav = 0.0;
        let mut fp_sav = 0.0;
        let mut a_debt = 0.0;
        let mut s_debt = 0.0;
        for region in &country.regions {
            let rc = &region.class_demographics.rural_classes;
            if let Some(d) = rc.get(&RuralClass::Aristocracy) {
                a_pop += d.population;
                a_sav += d.savings;
                a_debt += d.debt;
            }
            if let Some(d) = rc.get(&RuralClass::Serf) {
                s_pop += d.population;
                s_sav += d.savings;
                s_debt += d.debt;
            }
            if let Some(d) = rc.get(&RuralClass::LandlessLaborer) {
                ll_sav += d.savings;
            }
            if let Some(d) = rc.get(&RuralClass::FreePeasant) {
                fp_sav += d.savings;
            }
        }
        eprintln!(
            "FINAL {cname}: aristo pop={a_pop} sav={a_sav:.0} debt={a_debt:.0} spc={:.2} | serf pop={s_pop} sav={s_sav:.0} debt={s_debt:.0} spc={:.2} | ll={ll_sav:.0} fp={fp_sav:.0}",
            if a_pop > 0 { a_sav / a_pop as f64 } else { 0.0 },
            if s_pop > 0 { s_sav / s_pop as f64 } else { 0.0 }
        );
        let wars_in = state
            .declared_wars
            .iter()
            .filter(|w| {
                w.end_turn.is_none() && (w.aggressor == *cname || w.defender == *cname)
            })
            .count();
        let wars_total = state
            .declared_wars
            .iter()
            .filter(|w| w.aggressor == *cname || w.defender == *cname)
            .count();
        eprintln!(
            "FINAL {cname}: war_bonds_issued={:.0} liquid_reserves={:.0} wars_active={} wars_total={}",
            country.war_economy.war_bonds_issued,
            country.budget.liquid_reserves,
            wars_in,
            wars_total,
        );
        // Energy probe: why did national supply hit 0?
        if let Some(ents) = ctx.entities.get(cname) {
            let mut n_plants = 0usize;
            let mut emp = 0u32;
            let mut energy_inv = 0.0f64;
            let mut fuel_inv = 0.0f64;
            let mut plant_cash = 0.0f64;
            for b in &ents.buildings {
                if b.sector == sim_engine::registries::enums::Sector::Energy {
                    n_plants += 1;
                    emp += b.current_employment;
                    energy_inv += b
                        .inventory
                        .get(&sim_engine::registries::enums::Commodity::Energy)
                        .copied()
                        .unwrap_or(0.0);
                    for (c, q) in &b.inventory {
                        if format!("{c:?}").contains("Coal")
                            || format!("{c:?}").contains("Gas")
                            || format!("{c:?}").contains("Fuel")
                            || format!("{c:?}").contains("Oil")
                        {
                            fuel_inv += q;
                        }
                    }
                }
            }
            let mut n_eco = 0usize;
            let mut eco_capital = 0.0f64;
            let mut eco_capacity = 0u32;
            let mut eco_fulfilled = 0u32;
            let mut eco_furloughed = 0.0f64;
            let mut eco_neg = 0usize;
            for c in &ents.companies {
                if c.sector == sim_engine::registries::enums::Sector::Energy {
                    plant_cash += c.available_cash;
                    n_eco += 1;
                    eco_capital += c.company_capital;
                    eco_capacity += c.worker_capacity;
                    eco_fulfilled += c.prev_fulfilled_fte;
                    eco_furloughed += c.furloughed_workers_count;
                    if c.company_capital < 0.0 {
                        eco_neg += 1;
                    }
                }
            }
            let supply: f64 = country.power_grid_state.region_supply_mw.values().sum();
            eprintln!(
                "FINAL {cname}: ENERGY plants={} emp={} energy_inv={:.1} fuel_inv={:.1} plant_cash={:.0} supply={:.1}MW",
                n_plants, emp, energy_inv, fuel_inv, plant_cash, supply
            );
            eprintln!(
                "FINAL {cname}: ECOCORPS n={} capital={:.0} worker_cap={} fulfilled={} furloughed={:.0} neg_capital={}",
                n_eco, eco_capital, eco_capacity, eco_fulfilled, eco_furloughed, eco_neg
            );
        }
    }
}
