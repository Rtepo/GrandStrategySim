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

    /// Generation determinism probe: two fresh `generate_world` calls must
    /// produce byte-identical canonical state. `serde_json::Value` parses
    /// objects into BTreeMap (sorted keys), so a textual diff of the two
    /// pretty-printed dumps pinpoints the exact divergent field — including
    /// fields GENESIS_FP's sorted projection cannot see (class savings,
    /// market internals, unions, securities).
    #[test]
    fn same_seed_same_world() {
        fn canon(dir: &std::path::Path, label: &str) -> String {
            let registries = Registries::native_only();
            let options = GenerateOptions {
                country_count: 4,
                start_year: StartYear::Y1925,
                seed: Some(42),
            };
            sim_engine::engine::seeded_rng::reset_draw_count();
            let GeneratedWorld {
                mut state, ..
            } = generate_world(dir, options, &registries).expect("world generation failed");
            let ctx =
                InMemoryTurnContext::load_from_disk(dir, &mut state).expect("ctx load failed");
            eprintln!(
                "DRAWCOUNT {}: {}",
                label,
                sim_engine::engine::seeded_rng::draw_count()
            );

            // Canonicalize: serialize → parse (BTreeMap-sorted keys) →
            // recursively sort every array by its elements' canonical text,
            // so only true multiset/content diffs remain.
            fn deep_canon(v: serde_json::Value) -> serde_json::Value {
                match v {
                    serde_json::Value::Object(m) => serde_json::Value::Object(
                        m.into_iter().map(|(k, x)| (k, deep_canon(x))).collect(),
                    ),
                    serde_json::Value::Array(a) => {
                        let mut items: Vec<serde_json::Value> =
                            a.into_iter().map(deep_canon).collect();
                        items.sort_by_cached_key(|x| serde_json::to_string(x).unwrap());
                        serde_json::Value::Array(items)
                    }
                    x => x,
                }
            }
            fn canon<T: serde::Serialize>(label: &str, v: &T) -> serde_json::Value {
                match serde_json::to_string(v) {
                    Ok(s) => {
                        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                        deep_canon(parsed)
                    }
                    Err(e) => {
                        eprintln!("canon '{}' not JSON-serializable: {} — skipped", label, e);
                        serde_json::Value::Null
                    }
                }
            }
            let mut bundle = serde_json::json!({
                "state": canon("state", &state),
                "market": canon("market", &ctx.market),
                "diplomacy": canon("diplomacy", &ctx.diplomacy),
                "countries": canon("countries", &state.countries),
                "regions": canon("regions", &state.countries.values().flat_map(|c| c.regions.clone()).collect::<Vec<_>>()),
            });
            let mut ents = serde_json::Map::new();
            let mut names: Vec<&String> = ctx.entities.keys().collect();
            names.sort();
            for name in names {
                let e = &ctx.entities[name];
                ents.insert(
                    name.clone(),
                    serde_json::json!({
                        "companies": canon("companies", &e.companies),
                        "buildings": canon("buildings", &e.buildings),
                        "unions": canon("unions", &e.unions),
                        "commercial_buildings": canon("commercial_buildings", &e.commercial_buildings),
                        "housing_buildings": canon("housing_buildings", &e.housing_buildings),
                    }),
                );
            }
            bundle["entities"] = serde_json::Value::Object(ents);
            let text = serde_json::to_string_pretty(&bundle).unwrap();
            let out = std::env::temp_dir().join(format!("ses_gen_{}.json", label));
            std::fs::write(&out, &text).unwrap();
            eprintln!("wrote canonical dump: {}", out.display());
            text
        }

        let tmp_a = TempDir::new().expect("temp dir a");
        let tmp_b = TempDir::new().expect("temp dir b");
        let a = canon(tmp_a.path(), "a");
        let b = canon(tmp_b.path(), "b");

        if a != b {
            let (la, lb) = (a.lines().collect::<Vec<_>>(), b.lines().collect::<Vec<_>>());
            let first = (0..la.len().max(lb.len()))
                .find(|&i| la.get(i) != lb.get(i))
                .unwrap_or(0);
            let lo = first.saturating_sub(4);
            let mut ctx_lines = String::new();
            for i in lo..(first + 6).min(la.len().max(lb.len())) {
                ctx_lines.push_str(&format!(
                    "  line {}: a={:?}  b={:?}{}\n",
                    i,
                    la.get(i).unwrap_or(&"<eof>"),
                    lb.get(i).unwrap_or(&"<eof>"),
                    if i == first { "  <-- FIRST DIFF" } else { "" }
                ));
            }
            panic!("worldgen nondeterminism at line {}:\n{}", first, ctx_lines);
        }
    }
}
