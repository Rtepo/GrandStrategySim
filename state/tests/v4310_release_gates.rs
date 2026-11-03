//! v4.31.0 release-gate assertions (TASK-027..032 acceptance):
//!
//! - `rendered_map_regions_count == total_simulation_regions_count` and
//!   `capitals_covered_by_lakes_count == 0` (TASK-027)
//! - `parcel_max_aspect_ratio < 3.5` and
//!   `parcel_interior_non_border_share > 0.35` — no barcode strips (TASK-028)
//! - `banks.all(|b| net_profit == int_inc + fee - int_exp - npl - wages)`
//!   (±1.0) and `avg_bank_idle_cb_cash_share < 0.30` and
//!   `profitable_banks_share > 0.60` (TASK-030)
//! - `steppe_countries_with_nomadic_pops > 0` and
//!   `society_kpi_history_series_count == 12` (TASK-032)

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use sim_engine::society::{pastoral_intensity, SocietyKpiHistory};
    use sim_engine::ui::snapshot::build_bank_detail;
    use sim_engine::ui::snapshot_map::{
        build_region_parcel_map, build_world_map, MapPoint,
    };
    use sim_engine::ui::snapshot_society::build_society_history_dto;
    use tempfile::TempDir;

    /// Ray-casting point-in-polygon for the water-capital gate.
    fn point_in_poly(p: &MapPoint, poly: &[MapPoint]) -> bool {
        let mut inside = false;
        let n = poly.len();
        if n < 3 {
            return false;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (poly[i].x, poly[i].y);
            let (xj, yj) = (poly[j].x, poly[j].y);
            if (yi > p.y) != (yj > p.y) && p.x < (xj - xi) * (p.y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Bounding-box aspect ratio of a polygon (>= 1.0).
    fn poly_aspect(poly: &[MapPoint]) -> f64 {
        let mut minx = f64::MAX;
        let mut miny = f64::MAX;
        let mut maxx = f64::MIN;
        let mut maxy = f64::MIN;
        for p in poly {
            minx = minx.min(p.x);
            miny = miny.min(p.y);
            maxx = maxx.max(p.x);
            maxy = maxy.max(p.y);
        }
        let w = (maxx - minx).max(1e-6);
        let h = (maxy - miny).max(1e-6);
        w.max(h) / w.min(h)
    }

    #[test]
    fn v4310_release_gates() {
        let tmp = TempDir::new().expect("tempdir");
        let data_dir = tmp.path();
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 12,
            start_year: StartYear::Y1900,
            seed: Some(43100),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(data_dir, options, &registries).expect("worldgen");
        let mut ctx =
            InMemoryTurnContext::load_from_disk(data_dir, &mut state).expect("load turn context");

        let mut probe = NoopProbe;
        for _ in 0..60u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe).expect("turn");
        }

        // ============================================================
        // GATE 1 (TASK-027) — every simulation region renders a polygon,
        // and no capital/city marker sits inside a lake or inland sea.
        // ============================================================
        let map = build_world_map(&state);
        let rendered: usize = map.countries.iter().map(|c| c.regions.len()).sum();
        let simulated: usize = state.countries.values().map(|c| c.regions.len()).sum();
        assert_eq!(
            rendered, simulated,
            "rendered_map_regions_count != total_simulation_regions_count"
        );
        let mut capitals_in_lakes = 0usize;
        for layout in &map.countries {
            for city in &layout.cities {
                // Only inland water can drown a capital — the full-canvas
                // "ocean"/"sea" background legitimately contains the map.
                if map
                    .water_bodies
                    .iter()
                    .filter(|w| w.kind == "lake" || w.kind == "inland_sea")
                    .any(|w| point_in_poly(&city.point, &w.boundary))
                {
                    capitals_in_lakes += 1;
                }
            }
        }
        assert_eq!(
            capitals_in_lakes, 0,
            "capitals_covered_by_lakes_count != 0"
        );

        // ============================================================
        // GATE 2 (TASK-028) — parcels are compact 2D cells: max bounding
        // aspect < 3.5 and >35% interior (non-border) share.
        // ============================================================
        let mut max_aspect = 0.0f64;
        let mut total_parcels = 0usize;
        let mut border_parcels = 0usize;
        for country in state.countries.values() {
            for region in &country.regions {
                if let Some(pm) = build_region_parcel_map(country, &region.id) {
                    for p in &pm.parcels {
                        total_parcels += 1;
                        if p.is_border_zone {
                            border_parcels += 1;
                        }
                        max_aspect = max_aspect.max(poly_aspect(&p.polygon));
                    }
                }
            }
        }
        assert!(total_parcels > 0, "no parcels generated");
        assert!(
            max_aspect < 3.5,
            "parcel_max_aspect_ratio = {max_aspect} (>= 3.5 — barcode strips!)"
        );
        let interior_share = 1.0 - border_parcels as f64 / total_parcels as f64;
        assert!(
            interior_share > 0.35,
            "parcel_interior_non_border_share = {interior_share} (<= 0.35)"
        );

        // ============================================================
        // GATE 3 (TASK-030) — canonical income statement identity on
        // every bank, treasury pass keeps idle CB cash < 30% of assets,
        // and >60% of banks are profitable.
        // ============================================================
        let mut banks_checked = 0usize;
        let mut profitable = 0usize;
        let mut idle_share_sum = 0.0f64;
        for (name, country) in state.countries.iter() {
            let Some(ents) = ctx.entities.get(name) else {
                continue;
            };
            for c in &ents.companies {
                if c.bank_type.is_none() {
                    continue;
                }
                let Some(d) = build_bank_detail(country, &ents.companies, &c.id) else {
                    continue;
                };
                banks_checked += 1;
                let identity = (d.net_profit
                    - (d.interest_income + d.fee_income - d.interest_expense
                        - d.npl_cost
                        - d.operating_wages))
                    .abs();
                assert!(
                    identity < 1.0,
                    "bank {} net_profit identity off by {identity}",
                    c.id
                );
                if d.total_assets > 0.0 {
                    idle_share_sum += d.reserves_at_central_bank / d.total_assets;
                }
                if d.net_profit > 0.0 {
                    profitable += 1;
                }
            }
        }
        assert!(banks_checked > 0, "no banks found");
        let avg_idle = idle_share_sum / banks_checked as f64;
        assert!(
            avg_idle < 0.30,
            "avg_bank_idle_cb_cash_share = {avg_idle} (>= 0.30)"
        );
        let profit_share = profitable as f64 / banks_checked as f64;
        assert!(
            profit_share > 0.60,
            "profitable_banks_share = {profit_share} (<= 0.60)"
        );

        // ============================================================
        // GATE 4 (TASK-032) — pastoral countries carry a nonzero nomadic
        // share, and the KPI history emits exactly 12 card series.
        // ============================================================
        let steppe_with_nomads = state
            .countries
            .values()
            .filter(|c| pastoral_intensity(c) >= 0.30)
            .filter(|c| c.macro_indicators.demographics.nomads > 0.005)
            .count();
        assert!(
            steppe_with_nomads > 0,
            "steppe_countries_with_nomadic_pops == 0"
        );
        let any_name = state.countries.keys().next().expect("country").clone();
        let hist: Option<&SocietyKpiHistory> = state.society_history.get(&any_name);
        let dto = build_society_history_dto(hist);
        assert_eq!(
            dto.series.len(),
            12,
            "society_kpi_history_series_count != 12"
        );
    }
}
