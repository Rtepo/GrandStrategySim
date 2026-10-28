//! v4.24.0 WS1 regression test — investment funds must deploy capital.
//!
//! Release gate: average fund cash share below 0.35 (i.e. ~65%+ invested
//! across equities, covered bonds, MBS tranches and treasury paper).
//!
//! Root cause fixed in v4.24: `Company::eps` was a per-turn profit flow
//! compared against ANNUAL P/E thresholds (`pe_or_pb <= 25`). Every listed
//! name screened "overvalued" (P/E read ~24x high), every active mandate's
//! ranked candidate list came out empty, no bids ever rested on the book,
//! and ~90% of fund AUM sat in brokerage cash.
//!
//! This test runs a generated world for a year of turns (enough for the
//! underwriter float asks, mandate screens and matching engine to deploy
//! the subscription inflow) and asserts the average cash share clears the
//! release gate. `frozen_cash` — resting bid pledges — counts as cash for
//! gate purposes: it is un-deployed capital by definition.

#[cfg(test)]
mod tests {
    use sim_engine::engine::diagnostic::NoopProbe;
    use sim_engine::engine::turn::run_turn_inner;
    use sim_engine::engine::turn_context::InMemoryTurnContext;
    use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
    use sim_engine::entities::Company;
    use sim_engine::registries::Registries;
    use sim_engine::state::Country;
    use tempfile::TempDir;

    /// Release gate: average cash share of fund AUM.
    const MAX_AVG_CASH_SHARE: f64 = 0.35;
    /// One year of turns — enough for mandates to fill their equity sleeves
    /// and the fixed-income leg to absorb the residual.
    const TURNS: u32 = 24;

    /// Mark-to-market AUM for a fund, mirroring the command layer's
    /// `fund_holdings_value` accounting (equities at the issuer's mark,
    /// covered bonds at principal, MBS tranches at outstanding balance,
    /// treasury ledger holdings at face value) plus both cash legs.
    fn fund_aum_and_cash(fund: &Company, entities: &[Company], country: &Country) -> (f64, f64) {
        let acct = match fund.brokerage_account.as_ref() {
            Some(a) => a,
            None => return (0.0, 0.0),
        };
        let mut invested = 0.0;
        for (inst, lots) in &acct.portfolio {
            let qty: u64 = lots.iter().map(|l| l.quantity).sum();
            if qty == 0 {
                continue;
            }
            if let Some(cid) = inst.strip_prefix("EQUITY:") {
                if let Some(c) = entities.iter().find(|c| c.id == cid) {
                    invested += c.share_price * qty as f64;
                }
            } else if let Some(bond_id) = inst.strip_prefix("BOND:") {
                if let Some(b) = country
                    .covered_bonds_issued
                    .iter()
                    .find(|b| b.id == bond_id)
                {
                    invested += b.principal * qty as f64;
                }
            } else if inst.starts_with("MBS:") || inst.starts_with("mbs:") {
                let rest = inst
                    .strip_prefix("MBS:")
                    .or_else(|| inst.strip_prefix("mbs:"))
                    .unwrap_or(inst);
                let mut parts = rest.rsplitn(2, ':');
                let priority_str = parts.next().unwrap_or("");
                let mbs_id = parts.next().unwrap_or(rest);
                if let Some(mbs) = country
                    .mbs_pool
                    .iter()
                    .find(|m| m.id.eq_ignore_ascii_case(mbs_id))
                {
                    if let Some(t) = mbs.tranches.iter().find(|t| {
                        format!("{:?}", t.priority).to_lowercase() == priority_str
                    }) {
                        invested += t.outstanding_balance * qty as f64;
                    }
                }
            }
        }
        if let Some(ledger) = fund.fund_ledger.as_ref() {
            invested += ledger
                .bond_holdings
                .iter()
                .filter(|h| !h.redeemed)
                .map(|h| h.face_value)
                .sum::<f64>();
        }
        let cash = acct.cash + acct.frozen_cash;
        (cash + invested, cash)
    }

    #[test]
    fn funds_deploy_capital_below_cash_gate() {
        let tmp = TempDir::new().expect("failed to create temp dir");
        let registries = Registries::native_only();
        let options = GenerateOptions {
            country_count: 4,
            start_year: StartYear::Y1950,
            seed: Some(0x5eed_4242),
        };
        let GeneratedWorld { mut state, .. } =
            generate_world(tmp.path(), options, &registries).expect("world generation failed");
        let mut ctx = InMemoryTurnContext::load_from_disk(tmp.path(), &mut state)
            .expect("failed to load turn context");
        let mut probe = NoopProbe;

        for tick in 0..TURNS {
            let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
            assert!(result.is_ok(), "turn {tick} failed: {:?}", result.err());
        }

        let mut fund_count = 0usize;
        let mut cash_share_sum = 0.0f64;
        let mut diagnostics: Vec<String> = Vec::new();

        let mut names: Vec<String> = state.countries.keys().cloned().collect();
        names.sort();
        for name in &names {
            let country = &state.countries[name];
            let Some(entities) = ctx.entities.get(name) else {
                continue;
            };
            for fund in entities
                .companies
                .iter()
                .filter(|c| c.fund_type.is_some() && c.fund_ledger.is_some())
            {
                let (aum, cash) = fund_aum_and_cash(fund, &entities.companies, country);
                if aum <= 0.0 {
                    continue;
                }
                fund_count += 1;
                let share = cash / aum;
                cash_share_sum += share;
                let profile = fund
                    .fund_ledger
                    .as_ref()
                    .map(|l| format!("{:?}", l.investment_mandate.risk_profile))
                    .unwrap_or_else(|| "?".to_string());
                diagnostics.push(format!(
                    "{name} {} [{profile}] aum={aum:.0} cash_share={share:.2}",
                    fund.id
                ));
            }
        }

        diagnostics.sort();
        for line in &diagnostics {
            eprintln!("FUNDDBG {line}");
        }

        assert!(fund_count > 0, "generated world must contain funds");
        let avg = cash_share_sum / fund_count as f64;
        assert!(
            avg < MAX_AVG_CASH_SHARE,
            "average fund cash share {avg:.3} exceeds gate {MAX_AVG_CASH_SHARE} \
             across {fund_count} funds"
        );
    }
}
