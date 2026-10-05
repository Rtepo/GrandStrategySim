//! Temporary diagnostic: which methods does the rank round-robin actually
//! seed for each producing sector at 1926 (with the real conversion filter),
//! and which commodities end up with zero producer capacity?
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::Registries;
use std::collections::{BTreeMap, BTreeSet};

fn is_conv(name: &str) -> bool {
    sim_engine::registries::production_methods::is_decree_conversion_method(name)
}

#[test]
fn seeded_method_coverage() {
    let reg = Registries::native_only();
    let sectors = [
        "light_industry",
        "agriculture",
        "heavy_industry",
        "mining",
        "construction",
        "energy",
    ];
    for sector in sectors {
        let Some(bm) = reg.production_methods.get(sector) else {
            println!("=== {sector}: (no registry key) ===");
            continue;
        };
        let mut eligible: Vec<(&String, &sim_engine::registries::production_methods::ProductionMethod)> = bm
            .production
            .iter()
            .filter(|(n, _)| !is_conv(n))
            .filter(|(_, pm)| pm.year <= 1926)
            .filter(|(_, pm)| match &pm.required_tech {
                None => true,
                Some(t) => reg
                    .tech_tree
                    .get(t.as_str())
                    .map(|n| n.year <= 1926)
                    .unwrap_or(false),
            })
            .collect();
        eligible.sort_by(|(an, a), (bn, b)| a.year.cmp(&b.year).then(an.cmp(bn)));
        println!("=== {sector}: {} era-eligible methods ===", eligible.len());
        for (i, (name, pm)) in eligible.iter().enumerate() {
            let seeded = if i < 5 { "SEED" } else { "    " };
            let outs: Vec<String> = pm
                .outputs
                .iter()
                .map(|(c, q)| format!("{c:?}:{q:.0}"))
                .collect();
            println!("  [{i:2}] {seeded} {name} (y{}) -> {outs:?}", pm.year);
        }
    }

    // Coverage: commodities produced only by methods at idx >=5
    let mut produced_first5: BTreeSet<Commodity> = BTreeSet::new();
    let mut produced_rest: BTreeSet<Commodity> = BTreeSet::new();
    let mut consumers: BTreeMap<Commodity, usize> = BTreeMap::new();
    for sector in sectors {
        let Some(bm) = reg.production_methods.get(sector) else { continue };
        let mut eligible: Vec<_> = bm
            .production
            .iter()
            .filter(|(n, _)| !is_conv(n))
            .filter(|(_, pm)| pm.year <= 1926)
            .collect();
        eligible.sort_by(|(an, a), (bn, b)| a.year.cmp(&b.year).then(an.cmp(bn)));
        for (i, (_, pm)) in eligible.iter().enumerate() {
            for &c in pm.outputs.keys() {
                if i < 5 {
                    produced_first5.insert(c);
                } else {
                    produced_rest.insert(c);
                }
            }
            for &c in pm.inputs.keys() {
                *consumers.entry(c).or_default() += 1;
            }
        }
    }
    println!("\n=== produced ONLY by methods idx>=5 (never rank-seeded) ===");
    let mut v: Vec<_> = produced_rest
        .difference(&produced_first5)
        .copied()
        .collect();
    v.sort_by_key(|c| std::cmp::Reverse(consumers.get(c).copied().unwrap_or(0)));
    for c in v {
        println!(
            "  {c:?}  (consumed by {} era methods)",
            consumers.get(&c).copied().unwrap_or(0)
        );
    }
}
