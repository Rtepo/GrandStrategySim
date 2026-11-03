//! v4.30.0 release-gate assertions (TASK-021..026 acceptance):
//!
//! - `regions_map_polygons_count(country) == owned_regions_count(country)`
//!   — world-map polygons follow current `owner_country`, not genesis
//! - `clickable_lakes_count == total_lakes_count` — 100% of water bodies
//!   render as clickable polygons (all have a name + ≥3 boundary points)
//! - `max_battle_displayed_morale_pct <= 100.0` and
//!   `peacetime_disasters_logged_count > 0`
//! - `international_migration_total_flows > 0`
//! - `banking_kpi_history_series_count == 10` and
//!   `commercial_bank_profit_history_len > 0`

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
    fn v4300_release_gates() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 12,
            start_year: StartYear::Y1900,
            seed: Some(43000),
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
        // GATE 1 — dynamic ownership: every country owns exactly the
        // polygons whose `MapRegionDto.country` points at it, summed
        // across all layout buckets (conquered polygons stay in the
        // loser's layout slot but carry the victor's owner tag).
        // ============================================================
        let map = build_world_map(&state);
        for (name, country) in state.countries.iter() {
            let polygon_count = map
                .countries
                .iter()
                .flat_map(|c| c.regions.iter())
                .filter(|r| r.country == *name)
                .count();
            assert_eq!(
                polygon_count,
                country.regions.len(),
                "regions_map_polygons_count({name}) != owned_regions_count"
            );
        }

        // ============================================================
        // GATE 2 — 100% of inland water bodies are clickable: every
        // lake/inland-sea/ocean polygon has a name and a closed polygon
        // (≥3 boundary points), which is what the SVG onClick layer
        // requires.
        // ============================================================
        let total_lakes = map
            .water_bodies
            .iter()
            .filter(|w| w.kind == "lake" || w.kind == "inland_sea")
            .count();
        let clickable_lakes = map
            .water_bodies
            .iter()
            .filter(|w| w.kind == "lake" || w.kind == "inland_sea")
            .filter(|w| !w.name.is_empty() && w.boundary.len() >= 3)
            .count();
        assert!(total_lakes > 0, "no lakes generated");
        assert_eq!(
            clickable_lakes, total_lakes,
            "clickable_lakes_count != total_lakes_count"
        );

        // ============================================================
        // GATE 3 — displayed morale never exceeds 100% (organization is
        // already 0-100 scale; the old UI multiplied it by 100 again),
        // and peacetime disasters have been logged.
        // ============================================================
        let max_morale: f64 = state
            .countries
            .values()
            .flat_map(|c| c.order_of_battle.armies.iter())
            .flat_map(|a| a.divisions.iter())
            .flat_map(|d| d.regiments.iter())
            .flat_map(|r| r.units.iter())
            .map(|u| u.stats.organization)
            .fold(0.0_f64, f64::max);
        assert!(
            max_morale <= 100.0,
            "max_battle_displayed_morale_pct = {max_morale} (>100)"
        );
        let disasters_logged: usize = state
            .countries
            .values()
            .map(|c| c.disaster_log.len())
            .sum();
        assert!(
            disasters_logged > 0,
            "peacetime_disasters_logged_count == 0"
        );

        // ============================================================
        // GATE 4 — international migration actually flows across
        // borders (was zero because residual passenger-transport stock
        // was consumed by commuters before migration ran).
        // ============================================================
        let intl_flows: usize = state
            .countries
            .values()
            .map(|c| c.last_migration_flows.len())
            .sum();
        assert!(
            intl_flows > 0,
            "international_migration_total_flows == 0"
        );

        // ============================================================
        // GATE 5 — banking history carries all 10 KPI series and a
        // per-bank financial-results history for the expanded drawer.
        // ============================================================
        let (name, hist) = state
            .banking_history
            .iter()
            .find(|(_, h)| h.has_data())
            .expect("banking history populated");
        // Ten KPI series (the `turns` axis itself is not a KPI series).
        let series_count = [
            hist.total_reserves.is_empty(),
            hist.total_deposits.is_empty(),
            hist.total_loans.is_empty(),
            hist.total_consumer_debt.is_empty(),
            hist.dspw_bank_count.is_empty(),
            hist.cb_rate.is_empty(),
            hist.m0.is_empty(),
            hist.m3.is_empty(),
            hist.cb_fx_reserves.is_empty(),
            hist.cb_gold_reserves.is_empty(),
        ]
        .iter()
        .filter(|empty| !**empty)
        .count();
        assert_eq!(
            series_count, 10,
            "banking_kpi_history_series_count({name}) = {series_count} (!=10)"
        );
        let profit_len: usize = state
            .banking_history
            .values()
            .flat_map(|h| h.bank_profits.values())
            .map(|s| s.len())
            .sum();
        assert!(
            profit_len > 0,
            "commercial_bank_profit_history_len == 0"
        );
    }
}
