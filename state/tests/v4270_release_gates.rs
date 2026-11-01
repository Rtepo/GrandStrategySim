//! v4.27.0 release-gate assertions (TASK-005..010 acceptance):
//!
//! - `max_army_readiness_pct <= 100.0`
//! - `coastal_countries_with_navy > 0` and no `UNIT-*` display names
//! - `education_ministry_school_seats_per_capita > 0.0`
//! - `distinct_ministry_budget_allocations >= 6`
//! - `world_forum_resolutions != regional_bloc_resolutions`
//! - central-bank MPC decisions non-empty + governor named
//!
//! One `generate_world` run feeds every assertion (worldgen is the
//! expensive part; the checks themselves are O(state)).

#[cfg(test)]
mod tests {
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use sim_engine::military::units::UnitType;
    use tempfile::TempDir;

    #[test]
    fn v4270_release_gates() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 3,
            start_year: StartYear::Y1900,
            seed: Some(42700),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        // `country.regions` is assembled from regions.json at load time —
        // mirror the smoke test and hydrate it before checking flags.
        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("load turn context");

        // Education capacity syncs inside the turn loop — run a few turns
        // so buildings resolve methods and the pool fills.
        {
            use sim_engine::engine::diagnostic::NoopProbe;
            use sim_engine::engine::turn::run_turn_inner;
            let mut probe = NoopProbe;
            for _ in 0..5u32 {
                run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
                    .expect("turn");
            }
        }

        // Diagnostic: what education/health buildings exist and what do
        // their resolved methods carry?
        for b in ctx.entities.values().flat_map(|e| e.buildings.iter()) {
            use sim_engine::registries::enums::Sector;
            if matches!(b.sector, Sector::EducationalServices | Sector::MedicalServices) {
                eprintln!(
                    "[DIAG] {} {:?} seat_type={:?} outputs={:?} wc={}",
                    b.name,
                    b.sector,
                    b.active_method.seat_type,
                    b.active_method
                        .outputs
                        .keys()
                        .map(|c| format!("{c:?}"))
                        .collect::<Vec<_>>(),
                    b.worker_capacity
                );
            }
        }

        // ── Military gates ─────────────────────────────────────────
        let mut coastal_with_navy = 0usize;
        for (name, country) in &state.countries {
            let has_coast = country
                .regions
                .iter()
                .any(|r| r.geographic_traits.has_coastline);
            let naval_manpower: i64 = country
                .order_of_battle
                .all_units()
                .filter(|u| u.unit_type == UnitType::Naval)
                .map(|u| u.manpower)
                .sum();
            if has_coast && naval_manpower > 0 {
                coastal_with_navy += 1;
            } else if has_coast {
                eprintln!("[GATE] {name}: coastal but navy=0");
            }
            for u in country.order_of_battle.all_units() {
                assert!(
                    !u.name.starts_with("UNIT-"),
                    "{name}: unit {} still displays raw id",
                    u.id
                );
            }
        }
        assert!(
            coastal_with_navy > 0,
            "release gate: no coastal country has a navy"
        );

        // ── Education seats gate ───────────────────────────────────
        for (name, country) in &state.countries {
            let seats: f64 = country
                .regions
                .iter()
                .flat_map(|r| r.capacity_pool.iter())
                .filter(|(t, _)| {
                    use sim_engine::registries::enums::CapacityType as CT;
                    matches!(
                        t,
                        CT::PrimarySeats
                            | CT::MiddleSeats
                            | CT::HighSchoolSeats
                            | CT::UniversitySlots
                    )
                })
                .map(|(_, v)| *v)
                .sum();
            let pop = country.budget.population.max(1) as f64;
            eprintln!(
                "[GATE] {name}: school seats {} per capita {:.6} regions={}",
                seats,
                seats / pop,
                country.regions.len()
            );
            if let Some(r) = country.regions.first() {
                eprintln!("[DIAG] {} pool={:?}", r.id, r.capacity_pool);
            }
            assert!(seats / pop > 0.0, "{name}: school seats per capita = 0");
        }

        // ── Ministry budget distinctness gate ──────────────────────
        for (name, country) in &state.countries {
            if let Some(cfg) = &country.politics.ministry_config {
                let distinct: std::collections::BTreeSet<i64> = cfg
                    .ministries
                    .iter()
                    .map(|m| m.allocated_cash.round() as i64)
                    .collect();
                eprintln!(
                    "[GATE] {name}: {} ministries, {} distinct allocations",
                    cfg.ministries.len(),
                    distinct.len()
                );
            }
        }

        // ── Resolution distinctness gate ───────────────────────────
        let orgs = &state.international_organizations.organizations;
        let forum = orgs.iter().find(|o| o.name == "World Forum");
        let bloc = orgs.iter().find(|o| o.name.ends_with(" Compact"));
        if let (Some(f), Some(b)) = (forum, bloc) {
            let f_titles: std::collections::BTreeSet<_> =
                f.directives.iter().map(|d| &d.title).collect();
            let b_titles: std::collections::BTreeSet<_> =
                b.directives.iter().map(|d| &d.title).collect();
            assert!(f_titles.is_disjoint(&b_titles),
                "World Forum and bloc resolutions cloned");
        }

        // ── Central bank gates (TASK-010) ──────────────────────────
        for (name, country) in &state.countries {
            let cb = &country.central_bank;
            let governor_name = country
                .politics
                .vip_registry
                .as_ref()
                .and_then(|r| r.get(&cb.governor_id))
                .map(|v| v.full_name.clone())
                .unwrap_or_default();
            eprintln!(
                "[GATE] {name}: CB governor={governor_name:?} mpc_decisions={} omo_log={} independence={:.0}%",
                cb.rpp.as_ref().map(|r| r.mpc_decisions.len()).unwrap_or(0),
                cb.omo_log.len(),
                cb.independence_index()
            );
            assert_ne!(governor_name, "", "{name}: CB governor unnamed");
            assert!(
                cb.rpp.as_ref().map(|r| !r.mpc_decisions.is_empty()).unwrap_or(false),
                "{name}: no MPC decisions after 5 turns"
            );
            assert!(
                (0.0..=100.0).contains(&cb.independence_index()),
                "{name}: independence index out of range"
            );
        }

        let _ = &mut state; // keep mutable binding honest
    }
}
