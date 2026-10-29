//! v4.24.0 WS1 regression test — Press/Newspapers tab infinite-loading fix.
//!
//! The Press page refetches `get_press_snapshot` on every turn
//! (`queryClient.invalidateQueries()` after `advanceTurn`). If
//! `build_press_snapshot` panics inside the command's `spawn_blocking`, the
//! invoke rejects; if it is merely slow, the page sits on "Loading…" each
//! turn. Both failure modes trapped the tab on "Loading…" forever because the
//! page rendered that label whenever query data was `undefined`.
//!
//! This test proves the builder is fast and total on a real generated world:
//! every country's snapshot must complete under a hard latency budget and
//! serialize to JSON without error, every turn — including early turns with
//! an empty event pool and later turns once parliamentary history, council
//! resolutions, union strikes and company price history populate the pool.

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{
        generate_world, GenerateOptions, GeneratedWorld, StartYear,
    };
    use sim_engine::registries::Registries;
    use sim_engine::ui::snapshot_press::build_press_snapshot;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Per-snapshot latency budget. The builder is a pure read over already
    /// materialized state; a generated world must answer in well under
    /// 100 ms even in a debug build.
    const MAX_SNAPSHOT: Duration = Duration::from_millis(100);

    /// Turns to run — enough for the lazy entity loader to seed companies,
    /// budget_bill history entries to accumulate, and price_history to exist.
    const TURNS: u32 = 12;

    #[test]
    fn press_snapshot_builds_under_100ms_each_turn() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1950,
            seed: Some(0x5eed_4240),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(tmp.path(), options, &registries)
                .expect("world generation failed");
        let mut ctx = InMemoryTurnContext::load_from_disk(tmp.path(), &mut state)
            .expect("failed to load turn context");
        let mut probe = NoopProbe;

        for tick in 0..TURNS {
            let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
            assert!(result.is_ok(), "turn {tick} failed: {:?}", result.err());

            let turn = state.calendar.global_turn;
            let year = state.calendar.current_year;
            let mut names: Vec<String> = state.countries.keys().cloned().collect();
            names.sort();
            for name in &names {
                let country = &state.countries[name];
                let (companies, unions) = ctx
                    .entities
                    .get(name)
                    .map(|e| (e.companies.as_slice(), e.unions.as_slice()))
                    .unwrap_or((&[], &[]));

                let t0 = Instant::now();
                let snap = build_press_snapshot(
                    country,
                    companies,
                    unions,
                    &state.treaty_registry,
                    turn,
                    year,
                );
                let elapsed = t0.elapsed();

                // A serialization failure rejects the Tauri invoke — the
                // page used to render "Loading…" forever in that case.
                serde_json::to_string(&snap)
                    .expect("press snapshot must serialize to JSON");

                assert!(
                    elapsed < MAX_SNAPSHOT,
                    "build_press_snapshot took {elapsed:?} for {name} at turn \
                     {turn} (budget {MAX_SNAPSHOT:?})"
                );
            }
        }
    }

    /// The event dedup added in v4.23 must be idempotent: identical event refs
    /// or idents in the source logs must never emit two articles on one front
    /// page. Synthesize the pathological case — a duplicated floor_vote entry
    /// in `politics.history` — and assert the page still dedups.
    #[test]
    fn press_snapshot_dedups_duplicate_history_entries() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1950,
            seed: Some(0x5eed_4241),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(tmp.path(), options, &registries)
                .expect("world generation failed");

        // Inject a duplicated floor_vote pair (same bill, same turn — the
        // double-write case the v4.23 dedup guards).
        let dup = serde_json::json!({
            "event": "floor_vote",
            "bill_id": "BILL-DUP",
            "title": "Duplicate Test Act",
            "bill_type": "economic",
            "votes_for": 10.0,
            "votes_against": 3.0,
            "abstentions": 1.0,
            "passed": true,
            "turn": 0,
        });
        for country in state.countries.values_mut() {
            country.politics.history.push(dup.clone());
            country.politics.history.push(dup.clone());
        }

        let turn = state.calendar.global_turn;
        let year = state.calendar.current_year;
        let mut names: Vec<String> = state.countries.keys().cloned().collect();
        names.sort();
        for name in &names {
            let country = &state.countries[name];
            let snap = build_press_snapshot(
                country,
                &[],
                &[],
                &state.treaty_registry,
                turn,
                year,
            );
            for paper in &snap.papers {
                let mut refs = std::collections::HashSet::new();
                for a in &paper.articles {
                    assert!(
                        refs.insert(&a.event_ref),
                        "duplicate article {} on paper {}",
                        a.event_ref,
                        paper.paper_id
                    );
                }
            }
        }
    }
}
