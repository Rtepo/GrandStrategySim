//! TEMPORARY diagnostic probe: reproduce the national energy blackout.
//! Generates a Y1900 world, runs a few turns, and dumps per-plant and
//! per-region energy telemetry so the supply/demand gap can be attributed
//! (fuel starvation vs staffing vs method vs wire bottleneck).

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::enums::{Commodity, Sector};
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    fn dump_state(
        tick: u32,
        state: &sim_engine::state::GameState,
        ctx: &InMemoryTurnContext,
        verbose: bool,
    ) {
        for (cname, ents) in &ctx.entities {
            let country = state.countries.get(cname).expect("country");
            let grid = &country.power_grid_state;
            let nat_supply: f64 = grid.region_supply_mw.values().sum();
            let nat_demand: f64 = grid.region_demand_mw.values().sum();
            let ratio = nat_supply / nat_demand.max(1e-9);
            eprintln!(
                "=== T{} [{}] nat_supply={:.2} MW nat_demand={:.2} MW ratio={:.3}",
                tick, cname, nat_supply, nat_demand, ratio
            );
            if !verbose && ratio >= 1.0 {
                continue;
            }
            for region in &country.regions {
                let s = grid.region_supply_mw.get(&region.id).copied().unwrap_or(0.0);
                let d = grid.region_demand_mw.get(&region.id).copied().unwrap_or(0.0);
                let e = grid
                    .region_effective_supply_mw
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                let cap = grid
                    .region_max_capacity_mw
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                let lv = grid
                    .region_lv_capacity
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                let shed = grid
                    .load_shed_tiers
                    .get(&region.id)
                    .copied()
                    .unwrap_or_default();
                eprintln!(
                    "  R[{}] supply={:.2} demand={:.2} eff={:.2} maxcap={:.2} lv={:.2} shed={:?}",
                    region.id, s, d, e, cap, lv, shed
                );
            }
            for b in &ents.buildings {
                if b.sector != Sector::Energy {
                    continue;
                }
                let energy_inv = b.inventory.get(&Commodity::Energy).copied().unwrap_or(0.0);
                let meta = sim_engine::energy::grid::get_plant_metadata(b);
                let fuels: Vec<String> = b
                    .inventory
                    .iter()
                    .filter(|(c, q)| c.is_fuel() && **q > 0.0)
                    .map(|(c, q)| format!("{:?}={:.0}", c, q))
                    .collect();
                eprintln!(
                    "  PLANT[{}] name={} type={:?} emp={}/{} sf={} e_inv={:.2} fuels={:?} fulfill={:.3} nameplate={:?} te={:?} out={:?} in_keys={:?}",
                    b.id,
                    b.name,
                    meta.as_ref().map(|m| m.plant_type),
                    b.current_employment,
                    b.worker_capacity,
                    b.scale_factor,
                    energy_inv,
                    fuels,
                    b.last_fulfillment_ratio,
                    meta.as_ref().map(|m| m.nameplate_capacity_mw),
                    b.active_method.thermal_efficiency,
                    b.active_method.outputs,
                    b.active_method.inputs.keys().collect::<Vec<_>>(),
                );
            }
        }
    }

    #[test]
    fn blackout_probe() {
        let tmp = TempDir::new().expect("temp dir");
        let data_dir = tmp.path();

        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 6,
            start_year: StartYear::Y1900,
            seed: Some(7),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("world generation failed");

        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("failed to load turn context");

        let mut probe = NoopProbe;

        // Turn 0 state (post-genesis, pre-simulation).
        dump_state(0, &state, &ctx, true);

        for tick in 1..=8u32 {
            let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
            assert!(result.is_ok(), "tick {} failed: {:?}", tick, result.err());
            dump_state(tick, &state, &ctx, tick >= 5);
        }
    }
}
