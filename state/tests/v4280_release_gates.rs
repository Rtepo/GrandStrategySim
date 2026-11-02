//! v4.28.0 release-gate assertions (TASK-011..015 acceptance):
//!
//! - `region_vertices_turn_0 == region_vertices_after_annexation`
//!   (immutable geometry — annexation repaints, never re-lays-out)
//! - `zero_pop_countries_have_zero_gdp_and_annexed_status`
//! - `megaregions_are_100_percent_connected_components`
//! - `parcel_map_has_industrial_parcels && residential_color != water_color`
//! - `ministry_social_unrest_pct < 60.0 && inspectorate_capacity > 0.0
//!    && school_seats > 0.0`
//! - `max_single_turn_cb_rate_change_bps <= 150 &&
//!    !cb_fx_reserves_contain("EUR")`

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use sim_engine::ui::snapshot_map::build_world_map;
    use tempfile::TempDir;

    #[test]
    fn v4280_release_gates() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 6,
            start_year: StartYear::Y1900,
            seed: Some(42800),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("load turn context");

        // Warm up a few turns so capacity pools / KPIs / MPC meetings resolve.
        let mut probe = NoopProbe;
        for _ in 0..8u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .expect("turn");
        }

        // ============================================================
        // GATE 1 — Immutable region geometry across annexation.
        // Snapshot every region polygon at "turn 0", execute a war-style
        // cession (the same mutation war_bridge performs), rebuild the
        // map, assert every polygon + centroid is identical.
        // ============================================================
        let map_before = build_world_map(&state);
        let geom_before: Vec<(String, Vec<(i64, i64)>, (i64, i64))> = map_before
            .countries
            .iter()
            .flat_map(|c| c.regions.iter())
            .map(|r| {
                (
                    r.id.clone(),
                    r.polygon
                        .iter()
                        .map(|p| (p.x.to_bits() as i64, p.y.to_bits() as i64))
                        .collect::<Vec<_>>(),
                    (r.centroid.x.to_bits() as i64, r.centroid.y.to_bits() as i64),
                )
            })
            .collect();

        // Simulate territorial cession: pick the two largest countries,
        // move one region loser→victor exactly as war_bridge does.
        let mut sizes: Vec<(String, usize)> = state
            .countries
            .iter()
            .map(|(n, c)| (n.clone(), c.regions.len()))
            .collect();
        sizes.sort_by_key(|b| std::cmp::Reverse(b.1));
        let victor = sizes[0].0.clone();
        let loser = sizes[1].0.clone();
        let ceded_id = {
            let dc = state.countries.get_mut(&loser).expect("loser");
            let mut region = dc.regions.pop().expect("region");
            region.owner_country = victor.clone();
            region.is_capital = false;
            let id = region.id.clone();
            state
                .countries
                .get_mut(&victor)
                .expect("victor")
                .regions
                .push(region);
            id
        };

        let map_after = build_world_map(&state);
        let geom_after: Vec<(String, Vec<(i64, i64)>, (i64, i64))> = map_after
            .countries
            .iter()
            .flat_map(|c| c.regions.iter())
            .map(|r| {
                (
                    r.id.clone(),
                    r.polygon
                        .iter()
                        .map(|p| (p.x.to_bits() as i64, p.y.to_bits() as i64))
                        .collect::<Vec<_>>(),
                    (r.centroid.x.to_bits() as i64, r.centroid.y.to_bits() as i64),
                )
            })
            .collect();
        assert_eq!(
            geom_before, geom_after,
            "region_vertices_turn_0 == region_vertices_turn_60: a cession \
             moved polygon geometry ({} ceded)",
            ceded_id
        );

        // The ceded region must still appear on the map (painted for the
        // victor via owner_color_index) — not a void.
        let ceded = geom_after.iter().find(|(id, _, _)| *id == ceded_id);
        assert!(ceded.is_some(), "ceded region vanished from the map");

        // ============================================================
        // GATE 2 — Zombie-state elimination: strip ALL regions from the
        // loser, run a turn, assert capitulation + economic shutdown.
        // ============================================================
        {
            let dc = state.countries.get_mut(&loser).expect("loser2");
            let mut ac = dc.regions.drain(..).collect::<Vec<_>>();
            for r in ac.iter_mut() {
                r.owner_country = victor.clone();
                r.is_capital = false;
            }
            state
                .countries
                .get_mut(&victor)
                .expect("victor2")
                .regions
                .extend(ac);
        }
        for _ in 0..3u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .expect("turn2");
        }
        let zombie = state.countries.get(&loser).expect("zombie");
        assert!(
            zombie.annexed_by.is_some(),
            "zero_pop_countries_have_zero_gdp_and_annexed_status: {} owns 0 \
             regions but annexed_by is {:?}",
            loser,
            zombie.annexed_by
        );
        assert!(
            zombie.capitulated_turn.is_some(),
            "capitulated_turn not stamped for {loser}"
        );
        assert!(
            zombie.budget.liquid_reserves.abs() < 1.0,
            "zombie treasury not drained: {}",
            zombie.budget.liquid_reserves
        );
        assert_eq!(
            zombie.budget.gdp, 0.0,
            "zombie GDP tick not halted: {}",
            zombie.budget.gdp
        );

        // ============================================================
        // GATE 3 — Megaregions are 100% connected components on the same
        // k-NN geometric adjacency the generator grouped them with.
        // ============================================================
        use sim_engine::society::geography::{
            flood_fill_clusters, knn_adjacency, map_region_sort_key, megaregion_sites,
        };
        for (cname, country) in state.countries.iter() {
            // Megaregions partition the GENESIS region set — current
            // `country.regions` drifts under annexation, so rebuild the
            // id list from the stored megaregion union.
            let mut rids: Vec<String> = country
                .megaregions
                .iter()
                .flat_map(|m| m.regions.iter().cloned())
                .collect();
            rids.sort_by_key(|a| map_region_sort_key(a));
            if rids.len() <= 3 {
                continue; // single-megaregion countries are trivially contiguous
            }
            let sites = megaregion_sites(&rids);
            let adj = knn_adjacency(&sites, 3);
            let idx_of: std::collections::HashMap<&String, usize> =
                rids.iter().enumerate().map(|(i, r)| (r, i)).collect();
            for meg in &country.megaregions {
                // BFS within the megaregion's region set on the k-NN graph.
                let member: std::collections::HashSet<usize> = meg
                    .regions
                    .iter()
                    .filter_map(|r| idx_of.get(r).copied())
                    .collect();
                if member.is_empty() {
                    continue;
                }
                let mut seen = std::collections::HashSet::new();
                let mut stack = vec![*member.iter().next().unwrap()];
                while let Some(i) = stack.pop() {
                    if !seen.insert(i) {
                        continue;
                    }
                    for &j in adj.get(&i).into_iter().flatten() {
                        if member.contains(&j) {
                            stack.push(j);
                        }
                    }
                }
                assert_eq!(
                    seen.len(),
                    member.len(),
                    "megaregions_are_100_percent_connected_components: {} \
                     megaregion {} has disconnected regions",
                    cname,
                    meg.id
                );
            }
            // Sanity: the flood-fill helper itself must reproduce the
            // stored megaregion partition. v4.29.0 (TASK-017): countries
            // that gained or lost regions through treaty cession have an
            // intentionally resynced partition — the victor's megaregions
            // absorb ceded regions — so genesis flood-fill no longer
            // reproduces the stored count BY DESIGN. Only untouched
            // countries are held to the strict equality.
            // Touched iff the country's genesis region set (all regions
            // worldwide whose id carries this country's owner-prefix)
            // differs from its current holdings — covers both gains and
            // losses, since ceded regions move between country lists.
            let genesis: std::collections::BTreeSet<&str> = state
                .countries
                .values()
                .flat_map(|c| c.regions.iter())
                .filter_map(|r| {
                    r.id.rsplit_once("-Region").and_then(|(owner, _)| {
                        (owner == *cname).then_some(r.id.as_str())
                    })
                })
                .collect();
            let current: std::collections::BTreeSet<&str> = country
                .regions
                .iter()
                .map(|r| r.id.as_str())
                .collect();
            let touched_by_cession = genesis != current;
            if !touched_by_cession {
                let regen = flood_fill_clusters(&rids, &sites, &adj, 4);
                assert_eq!(
                    regen.len(),
                    country.megaregions.len(),
                    "{cname}: stored megaregion count drifted from generator"
                );
            }
        }

        // ============================================================
        // GATE 4 — Parcel map has industrial parcels; residential color
        // differs from water color.
        // ============================================================
        let mut has_industrial = false;
        let mut has_residential = false;
        let mut has_water = false;
        'outer: for cname in state.countries.keys() {
            let country = state.countries.get(cname).unwrap();
            for r in country.regions.iter().take(6) {
                if let Some(pmap) =
                    sim_engine::ui::snapshot_map::build_region_parcel_map(country, &r.id)
                {
                    for p in &pmap.parcels {
                        match p.use_class.as_str() {
                            "industrial" | "resource_mines" => has_industrial = true,
                            "residential" => has_residential = true,
                            "water" => has_water = true,
                            _ => {}
                        }
                    }
                }
                if has_industrial && has_residential && has_water {
                    break 'outer;
                }
            }
        }
        if !has_industrial {
            // Diagnostic: dump the use_class/zoning distribution the map
            // actually produced before failing.
            for (cname, _) in state.countries.iter().take(2) {
                let country = state.countries.get(cname).unwrap();
                for r in country.regions.iter().take(3) {
                    match sim_engine::ui::snapshot_map::build_region_parcel_map(
                        country, &r.id,
                    ) {
                        Some(pmap) => {
                            let mut dist: std::collections::BTreeMap<String, usize> =
                                Default::default();
                            for p in &pmap.parcels {
                                *dist
                                    .entry(format!("{}|{}", p.use_class, p.zoning))
                                    .or_default() += 1;
                            }
                            eprintln!("[PARCEL-DIAG] {} {} n={} {:?}", cname, r.id, pmap.parcels.len(), dist);
                        }
                        None => eprintln!("[PARCEL-DIAG] {} {} → None", cname, r.id),
                    }
                }
            }
        }
        assert!(
            has_industrial,
            "parcel_map_has_industrial_parcels: no Industrial use_class in \
             sampled regions"
        );
        // residential_color != water_color — the constants live in the
        // shared RegionParcelGrid.tsx component; read the source so the
        // gate can't silently pass if someone re-couples the hues.
        let grid_src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/components/map/RegionParcelGrid.tsx"
        ))
        .expect("RegionParcelGrid.tsx readable from the state crate");
        let pick = |key: &str| -> String {
            let pat = format!("{key}:");
            let start = grid_src.find(&pat).expect("color key") + pat.len();
            grid_src[start..]
                .trim_start_matches([' ', '"'])
                .chars()
                .take_while(|&c| c != '"')
                .collect()
        };
        let residential = pick("residential");
        let water = pick("water");
        assert_ne!(
            residential, water,
            "residential_color != water_color: both are {residential}"
        );

        // ============================================================
        // GATE 5 — Ministry KPIs: unrest < 60%, inspectorate capacity > 0,
        // school seats > 0.
        // ============================================================
        for cname in state.countries.keys() {
            let country = state.countries.get(cname).unwrap();
            if country.annexed_by.is_some() {
                continue;
            }
            let entities = ctx.entities.get(cname);
            let (companies, buildings) = entities
                .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
                .unwrap_or((&[], &[]));
            let snap = sim_engine::ui::snapshot::build_country_snapshot(
                country,
                &state.market_history,
                &ctx.market,
                buildings,
                companies,
                &sim_engine::ui::snapshot::ViewQuery::default(),
                state.calendar.current_year,
            );
            let mut unrest = None;
            let mut inspectorate = None;
            let mut seats = None;
            for m in &snap.government.cabinet {
                for k in &m.policy_kpis {
                    let key = k.label.to_lowercase();
                    if key.contains("unrest") {
                        unrest = Some(k.value);
                    }
                    if key.contains("inspectorate") && key.contains("capacity") {
                        inspectorate = Some(k.value);
                    }
                    if key.contains("seat") {
                        seats = Some(k.value);
                    }
                }
            }
            if let Some(u) = unrest {
                assert!(
                    u < 60.0,
                    "ministry_social_unrest_pct < 60.0: {cname} unrest = {u}"
                );
            }
            // Interior ministry must show capacity; education must show
            // seats — when those ministries exist.
            for m in &snap.government.cabinet {
                let name = m.ministry_name.to_lowercase();
                if name.contains("interior") {
                    assert!(
                        inspectorate.unwrap_or(0.0) > 0.0,
                        "inspectorate_capacity > 0.0: {cname} interior KPI \
                         missing or zero"
                    );
                }
                if name.contains("education") && seats.is_some() {
                    assert!(
                        seats.unwrap_or(0.0) > 0.0,
                        "school_seats > 0.0: {cname} education seats = \
                         {:?}",
                        seats
                    );
                }
            }
        }

        // ============================================================
        // GATE 6 — CB rate smoothing (≤150 bps/meeting) + no real-world
        // FX currencies + realistic gold tonnage.
        // ============================================================
        for (cname, country) in state.countries.iter() {
            // Annexed ledger shells are deliberately drained — their
            // reserves live with the annexer, not here.
            if country.annexed_by.is_some() {
                continue;
            }
            for key in country.central_bank.fx_reserves.keys() {
                assert!(
                    !matches!(key.as_str(), "EUR" | "USD" | "GBP" | "JPY" | "CHF"),
                    "cb_fx_reserves real-world currency {key} in {cname}"
                );
            }
            let gold = country.central_bank.physical_gold_reserves;
            assert!(
                (50.0..=2500.0).contains(&gold),
                "{cname} gold reserves {gold}t outside 50–2500t"
            );
            // Track per-decision bps jumps recorded so far.
            if let Some(rpp) = &country.central_bank.rpp {
                for d in &rpp.mpc_decisions {
                    assert!(
                        d.bps_change.abs() <= 150.0,
                        "max_single_turn_cb_rate_change_bps <= 150: {cname} \
                         moved {}bps at T{}",
                        d.bps_change,
                        d.turn
                    );
                }
            }
        }
        // Also gate live: run 12 more turns tracking raw rate deltas.
        let mut prev_rates: std::collections::HashMap<String, f64> = state
            .countries
            .iter()
            .map(|(n, c)| {
                (n.clone(), c.central_bank.interest_rates.reference_rate)
            })
            .collect();
        for _ in 0..12u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                .expect("turn3");
            for (n, c) in state.countries.iter() {
                let now = c.central_bank.interest_rates.reference_rate;
                let prev = prev_rates.insert(n.clone(), now).unwrap_or(now);
                let bps = (now - prev).abs() * 10_000.0;
                assert!(
                    bps <= 150.0,
                    "{n} rate jumped {bps:.0}bps in one turn (smoothing cap)"
                );
            }
        }
    }
}
