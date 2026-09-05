//! Headless smoke test: run the simulation for 50 ticks without panicking.
//!
//! This is the 5th stage of the Iron CI/CD pipeline (Daemon v2.0).
//! It catches runtime panics that compile-time checks (cargo build, clippy)
//! and unit tests miss — e.g., unwrap on None, array index OOB, NaN/Inf
//! propagation, stack overflow from deep recursion.
//!
//! The test bootstraps a world via `generate_world()` + `InMemoryTurnContext`,
//! then runs `run_turn_in_memory()` 50 times. If any tick panics or returns
//! an error, the test fails and the daemon rejects the branch.

#[cfg(test)]
mod tests {
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    /// Run the simulation for 50 ticks headlessly.
    /// If any tick panics or returns an error, the test fails.
    #[test]
    fn headless_50_tick_smoke() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let data_dir = tmp.path();

        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 1,
            start_year: StartYear::Y1900,
        };

        let GeneratedWorld {
            mut state,
            ..
        } = generate_world(data_dir, options, &registries)
            .expect("world generation failed");

        let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
            .expect("failed to load turn context");

        let mut probe = NoopProbe;

        for tick in 0..50u32 {
            let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
            assert!(
                result.is_ok(),
                "Tick {} failed: {:?}",
                tick,
                result.err()
            );
        }
    }
}
