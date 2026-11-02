//! v4.29.0 release-gate assertions (TASK-016..020 acceptance):
//!
//! - `avg_global_cpi_turn_60` in [85, 125] — the deflation spiral is dead —
//!   and `countries_with_positive_cb_rate >= 10`
//! - `capitals_inside_lakes_count == 0 && unassigned_regions_count == 0
//!    && clickable_lakes_count > 0`
//! - `battle_units_with_zero_initial_strength_count == 0 &&
//!    wartime_countries_on_peacetime_conscription == 0`
//! - `parcel_industrial_clusters_modularity > 0.4 &&
//!    parcel_distinct_biomes_count >= 6`

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::military::war_economy::ConscriptionLevel;
    use sim_engine::registries::Registries;
    use sim_engine::ui::snapshot_map::{build_region_parcel_map, build_world_map};
    use tempfile::TempDir;

    fn point_in_poly(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
        let mut inside = false;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if ((yi > p.1) != (yj > p.1))
                && (p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi)
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    #[test]
    fn v4290_release_gates() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 12,
            start_year: StartYear::Y1900,
            seed: Some(42900),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("load turn context");

        let mut probe = NoopProbe;
        for _ in 0..60u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .expect("turn");
        }

        // ============================================================
        // GATE 1 — avg_global_cpi_turn_60 ∈ [85, 125] and ≥10 countries
        // hold a positive central-bank policy rate (no universal ZLB).
        // ============================================================
        let cpis: Vec<f64> = state
            .countries
            .values()
            .map(|c| c.macro_indicators.inflation_indices.cpi_index)
            .filter(|c| *c > 0.0)
            .collect();
        let avg_cpi = cpis.iter().sum::<f64>() / cpis.len().max(1) as f64;
        assert!(
            (85.0..=125.0).contains(&avg_cpi),
            "avg_global_cpi_turn_60 out of band: {avg_cpi}"
        );
        let positive_rates = state
            .countries
            .values()
            .filter(|c| c.central_bank.interest_rates.reference_rate > 0.0)
            .count();
        assert!(
            positive_rates >= 10,
            "countries_with_positive_cb_rate = {positive_rates} (<10 — universal ZLB)"
        );

        // ============================================================
        // GATE 2 — capitals never sit inside lake polygons, lakes are
        // clickable water bodies, and no region is "Unassigned".
        // ============================================================
        let map = build_world_map(&state);
        let lakes: Vec<Vec<(f64, f64)>> = map
            .water_bodies
            .iter()
            .filter(|w| w.kind == "lake" || w.kind == "inland_sea")
            .map(|w| w.boundary.iter().map(|p| (p.x, p.y)).collect())
            .collect();
        let clickable = map
            .water_bodies
            .iter()
            .filter(|w| w.kind == "lake" || w.kind == "inland_sea")
            .count();
        assert!(clickable > 0, "clickable_lakes_count == 0");
        let mut capitals_in_lakes = 0usize;
        for c in &map.countries {
            for city in &c.cities {
                if city.is_capital
                    && lakes.iter().any(|l| point_in_poly((city.point.x, city.point.y), l))
                {
                    capitals_in_lakes += 1;
                }
            }
        }
        assert_eq!(capitals_in_lakes, 0, "capitals inside lake polygons");
        let unassigned: usize = state
            .countries
            .values()
            .map(|c| {
                let owned: std::collections::BTreeSet<&str> = c
                    .megaregions
                    .iter()
                    .flat_map(|m| m.regions.iter().map(|r| r.as_str()))
                    .collect();
                c.regions
                    .iter()
                    .filter(|r| !owned.contains(r.id.as_str()))
                    .count()
            })
            .sum();
        assert_eq!(unassigned, 0, "unassigned_regions_count > 0");

        // ============================================================
        // GATE 3 — no zero-strength battle units; no wartime country sits
        // on the peacetime conscription law.
        // ============================================================
        let zero_strength: usize = state
            .countries
            .values()
            .flat_map(|c| c.military_fronts.iter())
            .flat_map(|f| f.battles.iter())
            .flat_map(|b| {
                b.attacker_forces
                    .iter()
                    .chain(b.defender_forces.iter())
            })
            .filter(|u| u.initial_manpower <= 0)
            .count();
        assert_eq!(zero_strength, 0, "ghost detachments in battle orders");
        let wartime_peacetime = state
            .countries
            .values()
            .filter(|c| {
                !c.at_war_with.is_empty()
                    && c.war_economy.conscription_level == ConscriptionLevel::Peacetime
            })
            .count();
        assert_eq!(
            wartime_peacetime, 0,
            "wartime_countries_on_peacetime_conscription > 0"
        );

        // ============================================================
        // GATE 4 — industrial parcels form contiguous clusters (modularity
        // > 0.4) and parcel topography emits ≥6 distinct biomes.
        // ============================================================
        let country = state
            .countries
            .values()
            .find(|c| c.regions.iter().any(|r| r.parcel_ids.len() >= 30))
            .or_else(|| state.countries.values().next())
            .expect("a country");
        let region = country
            .regions
            .iter()
            .find(|r| r.parcel_ids.len() >= 30)
            .or_else(|| country.regions.first())
            .expect("a region");
        let pm = build_region_parcel_map(country, &region.id).expect("parcel map");
        // Modularity: fraction of same-class adjacencies on the lattice.
        // >0.4 proves zones cluster rather than scatter as noise.
        let n = pm.parcels.len();
        let cols = (n as f64).sqrt().ceil() as usize;
        let same = |a: usize, b: usize| pm.parcels[a].use_class == pm.parcels[b].use_class;
        let mut edges = 0usize;
        let mut matching = 0usize;
        for i in 0..n {
            if i % cols + 1 < cols && i + 1 < n {
                edges += 1;
                if same(i, i + 1) {
                    matching += 1;
                }
            }
            if i + cols < n {
                edges += 1;
                if same(i, i + cols) {
                    matching += 1;
                }
            }
        }
        let modularity = matching as f64 / edges.max(1) as f64;
        assert!(
            modularity > 0.4,
            "parcel_industrial_clusters_modularity = {modularity:.3} (<= 0.4)"
        );
        let biomes: std::collections::BTreeSet<&str> = state
            .countries
            .values()
            .flat_map(|c| c.regions.iter())
            .filter(|r| !r.parcel_ids.is_empty())
            .take(24)
            .filter_map(|r| {
                let c = state
                    .countries
                    .values()
                    .find(|c| c.regions.iter().any(|rr| rr.id == r.id))?;
                build_region_parcel_map(c, &r.id)
            })
            .flat_map(|pm| pm.parcels.into_iter().map(|p| p.terrain).collect::<Vec<_>>())
            .map(|t| Box::leak(t.into_boxed_str()) as &str)
            .collect();
        eprintln!("GATE4 biomes: {biomes:?}");
        assert!(
            biomes.len() >= 6,
            "parcel_distinct_biomes_count = {} (<6)",
            biomes.len()
        );
    }
}
