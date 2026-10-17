//! v4.18 WS2 determinism regression: the same world snapshot run twice in
//! one process must produce identical state fingerprints at every phase
//! seam and after every turn.
//!
//! The probe records `fingerprint_tasks` at each checkpoint inside
//! `run_turn_inner`; `state_fingerprint` covers the post-turn `GameState` +
//! `InMemoryTurnContext`. HashMap `RandomState` seeds differ between the two
//! runs' freshly-created maps even inside one process, so any iteration-
//! order dependence shows up here without needing a second binary run.

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::FingerprintProbe;
    use sim_engine::engine::turn::{run_turn_inner, state_fingerprint};
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::registries::Registries;
    use tempfile::TempDir;

    // v4.18 WS2: 4 turns × 2 cloned runs. Every order-sensitive divergence
    // found during bisection (order-book sequencing, public-works seller
    // sums, wage clearing) fired at turn 0–1 phase seams — a short window
    // catches them while staying well inside the 600 s nextest watchdog on
    // fresh staging checkouts (~140 s wall on a dev box).
    const TURNS: u32 = 4;

    fn run_scenario(
        state: &mut sim_engine::state::GameState,
        ctx: &mut InMemoryTurnContext,
        registries: &Registries,
    ) -> (FingerprintProbe, Vec<u64>) {
        let mut probe = FingerprintProbe::default();
        let mut post_turn = Vec::new();
        for _ in 0..TURNS {
            run_turn_inner(state, registries, ctx, &mut probe).expect("turn failed");
            post_turn.push(state_fingerprint(state, ctx));
        }
        (probe, post_turn)
    }

    #[test]
    fn same_snapshot_same_fingerprints() {
        let registries = Registries::native_only();
        let tmp = TempDir::new().expect("temp dir");
        let options = GenerateOptions {
            country_count: 2,
            start_year: StartYear::Y1925,
            seed: Some(42),
        };
        let GeneratedWorld {
            state: mut state_a, ..
        } = generate_world(tmp.path(), options, &registries).expect("world generation failed");
        let mut ctx_a =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state_a).expect("ctx load failed");

        // Clone the post-generation snapshot — the only airtight counterfactual
        // (generate_world itself consumes unseeded streams on repeated calls).
        let mut state_b = state_a.clone();
        let mut ctx_b = ctx_a.clone();

        let (probe_a, post_a) = run_scenario(&mut state_a, &mut ctx_a, &registries);
        let (probe_b, post_b) = run_scenario(&mut state_b, &mut ctx_b, &registries);

        if let Some((idx, fa, fb)) =
            FingerprintProbe::first_divergence(&probe_a.fingerprints, &probe_b.fingerprints)
        {
            // Print a window around the divergence for bisection context.
            let lo = idx.saturating_sub(3);
            eprintln!("DIVERGENCE at checkpoint index {}", idx);
            for (i, (a, b)) in probe_a.fingerprints[lo..]
                .iter()
                .zip(probe_b.fingerprints[lo..].iter())
                .enumerate()
            {
                eprintln!(
                    "  [{}] a=({}, {}, {:016x})  b=({}, {}, {:016x}){}",
                    lo + i,
                    a.0,
                    a.1,
                    a.2,
                    b.0,
                    b.1,
                    b.2,
                    if lo + i == idx { "  <-- FIRST DIVERGENCE" } else { "" }
                );
            }
            panic!(
                "nondeterminism: first divergent phase is '{}' at turn {} \
                 (fp {:016x} vs {:016x})",
                fa.1, fa.0, fa.2, fb.2
            );
        }

        assert_eq!(
            post_a, post_b,
            "post-turn state_fingerprint diverged across identical runs"
        );
    }
}
