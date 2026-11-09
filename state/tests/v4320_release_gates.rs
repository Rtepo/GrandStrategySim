//! v4.32.0 release-gate assertions (TASK-033..037 acceptance):
//!
//! - `max_region_population_at_turn_N < 200_000_000` — the 9.24B
//!   population-overflow bug stays dead (the full 240-turn headless run
//!   lives in `tests/epics/v4320_pop_diagnostic.rs`, `--release`).
//! - `save_and_load_roundtrip_preserves_world_fingerprint`
//! - `joint_stock_companies_have_shareholders &&
//!   balance_sheet_has_uor_inventory_subcategories &&
//!   xml_export_contains("urn:schemas-microsoft-com:office:spreadsheet")`
//! - `snap_election_triggers_campaign_actions_count > 0`
//! - `military_vips_all_have_rank && battles_all_display_commander_names`

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::io::runtime_save::{
        list_save_slots, load_runtime_save, world_fingerprint, write_runtime_save,
    };
    use sim_engine::politics::campaign::{process_campaign_spending, ElectionState};
    use sim_engine::politics::check_snap_election;
    use sim_engine::politics::vip_registry::{Vip, VipRoleExtended};
    use sim_engine::registries::Registries;
    use sim_engine::ui::snapshot::{
        build_country_snapshot, oob_commanders_for, vip_military_rank, ViewQuery,
    };
    use tempfile::TempDir;

    const SEED: u64 = 43210;

    fn world() -> (
        TempDir,
        sim_engine::state::GameState,
        InMemoryTurnContext,
        std::sync::Arc<Registries>,
    ) {
        let tmp = TempDir::new().expect("tempdir");
        let registries = Registries::native_only();
        let GeneratedWorld { mut state, .. } = generate_world(
            tmp.path(),
            GenerateOptions {
                country_count: 6,
                start_year: StartYear::Y1900,
                seed: Some(SEED),
            },
            &registries,
        )
        .expect("worldgen");
        let ctx =
            InMemoryTurnContext::load_from_disk(tmp.path(), &mut state).expect("load turn context");
        (tmp, state, ctx, registries)
    }

    #[test]
    fn v4320_population_stays_bounded_under_200m() {
        let (_tmp, mut state, mut ctx, registries) = world();
        let mut probe = NoopProbe;
        for _ in 0..30u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe).expect("turn");
        }
        let max_pop = state
            .countries
            .values()
            .flat_map(|c| c.regions.iter().map(|r| r.population))
            .max()
            .unwrap_or(0);
        assert!(
            max_pop < 200_000_000,
            "max region population {max_pop} still explodes past 200M"
        );
        let world_pop: i64 = state
            .countries
            .values()
            .flat_map(|c| c.regions.iter().map(|r| r.population))
            .sum();
        assert!(world_pop < 2_000_000_000, "world pop {world_pop} exploded");
    }

    #[test]
    fn v4320_save_load_roundtrip_preserves_world_fingerprint() {
        let (_tmp, mut state, mut ctx, registries) = world();
        let mut probe = NoopProbe;
        for _ in 0..5u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe).expect("turn");
        }
        let saves = TempDir::new().expect("saves dir");
        let fp_before = world_fingerprint(&state, &ctx);
        write_runtime_save(saves.path(), "slot-1", &state, &ctx, 1_700_000_000, false)
            .expect("write save");
        // Slot listing resolves metadata.
        let slots = list_save_slots(saves.path());
        assert!(slots.iter().any(|s| s.slot == "slot-1"));
        // .gss file exists + round-trips.
        assert!(saves.path().join("slot-1.gss").exists());
        let loaded = load_runtime_save(saves.path(), "slot-1").expect("load save");
        let fp_after = world_fingerprint(&loaded.game_state, &loaded.turn_context);
        assert_eq!(
            fp_before, fp_after,
            "save/load roundtrip changed the world fingerprint"
        );
    }

    #[test]
    fn v4320_accounting_gates() {
        let (_tmp, mut state, mut ctx, registries) = world();
        let mut probe = NoopProbe;
        for _ in 0..6u32 {
            run_turn_inner(&mut state, &registries, &mut ctx, &mut probe).expect("turn");
        }

        // Find a joint-stock company anywhere in the world.
        let (country_name, company_id) = ctx
            .entities
            .iter()
            .flat_map(|(name, e)| e.companies.iter().map(move |c| (name.clone(), c)))
            .find(|(_, c)| {
                matches!(
                    c.legal_form,
                    sim_engine::entities::LegalForm::JointStockCompany(_)
                )
            })
            .map(|(n, c)| (n, c.id.clone()))
            .expect("world has at least one joint-stock company");

        let country = state.countries.get(&country_name).expect("country");
        let entities = ctx.entities.get(&country_name).expect("entities");
        let view = ViewQuery {
            company_detail_id: Some(company_id),
            ..Default::default()
        };
        let snap = build_country_snapshot(
            country,
            &state.market_history,
            &ctx.market,
            &entities.buildings,
            &entities.companies,
            &view,
            state.calendar.current_year,
        );
        let detail = snap.company_detail.expect("company detail");

        // Gate: joint_stock_companies_have_shareholders.
        assert!(
            !detail.shareholders.is_empty(),
            "joint-stock company {} has no shareholders",
            detail.id
        );
        let pct_sum: f64 = detail.shareholders.iter().map(|s| s.pct).sum();
        assert!(pct_sum > 0.0, "shareholder percentages all zero");

        // Gate: balance_sheet_has_uor_inventory_subcategories.
        let bs = &detail.balance_sheet;
        let inv_split =
            bs.inv_raw_materials + bs.inv_wip + bs.inv_finished_goods + bs.inv_trade_goods;
        assert!(
            (inv_split - bs.inventory).abs() < (bs.inventory.abs() * 0.001 + 1.0),
            "inventory subcategories {inv_split} don't reconcile to inventory {}",
            bs.inventory
        );

        // Gate: xml_export_contains("urn:schemas-microsoft-com:office:spreadsheet").
        let xml = sim_engine::ui::spreadsheet::company_balance_sheet_spreadsheetml(
            &detail,
            state.calendar.current_year,
            state.calendar.global_turn,
        );
        assert!(xml.contains("urn:schemas-microsoft-com:office:spreadsheet"));
        assert!(xml.contains("mso-application progid=\"Excel.Sheet\""));
        assert!(xml.contains("ss:Type=\"Number\""));
        assert!(xml.contains("AKTYWA") && xml.contains("PASYWA"));
        assert!(xml.contains("Materiały") && xml.contains("Półfabrykaty"));
    }

    #[test]
    fn v4320_snap_election_triggers_campaign_actions() {
        let (_tmp, mut state, _ctx, _reg) = world();
        // Force a snap trigger: provisional government + elections pending.
        let country = state
            .countries
            .values_mut()
            .find(|c| c.politics.government_form.is_democratic())
            .expect("a democratic country exists");
        country.politics.ruling_party = "Provisional Technocratic Government".to_string();
        country.politics.years_to_elections = 2;
        country.politics.election_state = ElectionState::Idle;
        country.politics.last_snap_election_turn = 0;

        let msgs = check_snap_election(country, 100);
        assert!(msgs.iter().any(|m| m.contains("SNAP")), "snap msgs: {msgs:?}");
        assert!(
            matches!(
                country.politics.election_state,
                ElectionState::ActiveCampaign { .. }
            ),
            "snap trigger must arm a 3-turn ActiveCampaign"
        );

        // The 3-turn snap campaign populates campaign_executions.
        let mut parties = std::mem::take(&mut country.politics.active_parties);
        let mut companies = Vec::new();
        let mut regions = std::mem::take(&mut country.regions);
        let before = country.politics.campaign_executions.len();
        let spend_msgs =
            process_campaign_spending(country, &mut parties, &mut companies, &mut regions, 100);
        country.politics.active_parties = parties;
        country.regions = regions;
        let actions = country
            .politics
            .campaign_executions
            .len()
            .saturating_sub(before);
        assert!(
            actions > 0,
            "snap campaign produced no campaign actions; msgs={spend_msgs:?}"
        );
    }

    #[test]
    fn v4320_military_vips_have_rank_and_battles_show_commanders() {
        // Rank resolution: every MilitaryCommander VIP gets a non-empty
        // rank, persisted or derived.
        let mut v = Vip::default();
        v.roles = vec![VipRoleExtended::MilitaryCommander];
        v.id = "VIP-000001".to_string();
        v.full_name = "Jan Kowalski".to_string();
        assert!(!vip_military_rank(&v).is_empty());
        v.military_rank = "Rear Admiral".to_string();
        assert_eq!(vip_military_rank(&v), "Rear Admiral");
        let mut civ = Vip::default();
        civ.roles = vec![VipRoleExtended::Ceo];
        assert!(vip_military_rank(&civ).is_empty());

        // Battles: the war-state battle ledger carries commander rows on
        // both sides; OOB resolution yields named commanders per side.
        let (_t, state, _c, _r) = world();
        for ws in &state.declared_wars {
            let atk = oob_commanders_for(&state, &ws.aggressor);
            let def = oob_commanders_for(&state, &ws.defender);
            for row in atk.iter().chain(def.iter()) {
                assert!(!row.name.is_empty(), "commander row missing name");
                assert!(!row.rank.is_empty(), "commander {} missing rank", row.name);
            }
        }
    }
}
