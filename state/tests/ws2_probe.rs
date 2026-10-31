//! TEMPORARY ws2 probe — inspect seed-42 1900 world diplomacy/adjacency/tension.
//! Delete before committing.

use sim_engine::engine::{generate_world, GenerateOptions, StartYear};
use sim_engine::military::war_bridge::compute_country_adjacency;
use sim_engine::registries::Registries;
use tempfile::TempDir;

#[test]
fn ws2_probe_diplomacy() {
    let tmp = TempDir::new().unwrap();
    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1900,
        seed: Some(42),
    };
    let w = generate_world(tmp.path(), options, &registries).expect("worldgen failed");
    let state = &w.state;

    eprintln!("=== COUNTRIES ===");
    for (name, c) in &state.countries {
        eprintln!(
            "  {} cg={} gdp={:.0} pop_regions={} oob_manpower={} armies={} vip_reg={}",
            name,
            c.macro_indicators.cultural_group,
            c.budget.gdp,
            c.regions.len(),
            c.order_of_battle.total_manpower(),
            c.order_of_battle.armies.len(),
            c.politics.vip_registry.as_ref().map(|r| r.vips.len()).unwrap_or(0),
        );
    }

    eprintln!("=== ADJACENCY ===");
    let adj = compute_country_adjacency(state);
    for (a, b) in &adj {
        eprintln!("  {} <-> {}", a, b);
    }

    eprintln!("=== DIPLOMACY MATRIX ===");
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (c1, partners) in &w.diplomacy {
        for (c2, rel) in partners {
            if c1 < c2 {
                let back = w.diplomacy[c2][c1].relations;
                let avg = (rel.relations + back) / 2;
                let adjacent = adj.iter().any(|(a, b)| (a == c1 && b == c2) || (a == c2 && b == c1));
                eprintln!(
                    "  {} -> {} : {} (back {}) avg {} adj={}",
                    c1, c2, rel.relations, back, avg, adjacent
                );
                pairs.push((c1.clone(), c2.clone()));
            }
        }
    }

    eprintln!("=== TENSIONS ===");
    for (k, t) in &state.bilateral_tensions {
        eprintln!("  {} level={}", k, t.level);
    }
    eprintln!("declared_wars={}", state.declared_wars.len());
    eprintln!("war_cfg threshold={} decay={} per_prov={} max_wars={} cooldown={} adj_req={}",
        state.war_declaration_config.war_tension_threshold,
        state.war_declaration_config.tension_decay_rate,
        state.war_declaration_config.tension_per_provocation,
        state.war_declaration_config.max_simultaneous_wars,
        state.war_declaration_config.war_recidivism_cooldown_turns,
        state.war_declaration_config.require_border_adjacency,
    );
}
