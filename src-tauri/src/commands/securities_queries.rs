use crate::state::AppState;
use sim_engine::ui::snapshot::{
    CapitalGainsTaxRow, CapitalGainsTaxSummary, CommoditySpotRow, FundDetail, FundHoldingRow,
    FundRow, KnfFindingRow, ListedCompanyDetail, ListedCompanyPageResponse, ListedCompanyRow,
    MarketIndexSnapshot, SectorIndexSnapshot, StockExchangeResponse, TradeRow,
};

/// Display label for a fund's type — avoids leaking Rust `Option` debug
/// formatting (`Some(ClosedEndInvestmentFund)`) into the UI.
fn fund_type_label(fund_type: &Option<sim_engine::securities::FundType>) -> &'static str {
    match fund_type {
        Some(sim_engine::securities::FundType::OpenEndInvestmentFund) => {
            "Open-End Investment Fund"
        }
        Some(sim_engine::securities::FundType::ClosedEndInvestmentFund) => {
            "Closed-End Investment Fund"
        }
        Some(sim_engine::securities::FundType::HedgeFund) => "Hedge Fund",
        Some(sim_engine::securities::FundType::ExchangeTradedFund) => "ETF",
        Some(sim_engine::securities::FundType::MutualFund) => "Mutual Fund",
        None => "Unknown",
    }
}

/// Market value of a fund's brokerage portfolio holdings: equities at the
/// issuer's current share price, covered bonds at principal per unit, MBS
/// tranche units at outstanding balance, plus the fund ledger's unredeemed
/// sovereign-bond holdings at face value.
fn fund_holdings_value(
    acct: &sim_engine::securities::BrokerageAccount,
    ledger: &sim_engine::securities::FundLedger,
    entities: &[sim_engine::entities::Company],
    country_ref: &sim_engine::state::Country,
) -> f64 {
    let mut total = 0.0;
    for (inst, lots) in &acct.portfolio {
        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
        if qty == 0 {
            continue;
        }
        if let Some(company_id) = inst.strip_prefix("EQUITY:") {
            if let Some(c) = entities.iter().find(|c| c.id == company_id) {
                total += c.share_price * qty as f64;
            }
        } else if let Some(bond_id) = inst.strip_prefix("BOND:") {
            if let Some(b) = country_ref
                .covered_bonds_issued
                .iter()
                .find(|b| b.id == bond_id)
            {
                total += b.principal * qty as f64;
            }
        } else if inst.starts_with("MBS:") || inst.starts_with("mbs:") {
            // v4.24 WS1: securitize_loans emits lowercase `mbs:` ids — the
            // old uppercase-only prefix silently dropped MBS lots from AUM.
            let parts: Vec<&str> = inst.splitn(3, ':').collect();
            if parts.len() == 3 {
                if let Some(mbs) = country_ref
                    .mbs_pool
                    .iter()
                    .find(|m| m.id.eq_ignore_ascii_case(parts[1]))
                {
                    if let Some(t) = mbs.tranches.iter().find(|t| {
                        format!("{:?}", t.priority).to_lowercase() == parts[2]
                    }) {
                        total += t.outstanding_balance * qty as f64;
                    }
                }
            }
        }
    }
    total
        + ledger
            .bond_holdings
            .iter()
            .filter(|h| !h.redeemed)
            .map(|h| h.face_value)
            .sum::<f64>()
}

/// v4.18 WS4: per-unit market value of a portfolio instrument, so the
/// holdings table can show mark-to-market value and weight, not just raw
/// share counts.
fn instrument_market_price(
    inst: &str,
    entities: &[sim_engine::entities::Company],
    country_ref: &sim_engine::state::Country,
) -> f64 {
    if let Some(company_id) = inst.strip_prefix("EQUITY:") {
        entities
            .iter()
            .find(|c| c.id == company_id)
            .map(|c| c.share_price)
            .unwrap_or(0.0)
    } else if let Some(bond_id) = inst.strip_prefix("BOND:") {
        country_ref
            .covered_bonds_issued
            .iter()
            .find(|b| b.id == bond_id)
            .map(|b| b.principal)
            .unwrap_or(0.0)
    } else if inst.starts_with("MBS:") || inst.starts_with("mbs:") {
        let rest = &inst[4..];
        let mut parts = rest.rsplitn(2, ':');
        let priority_str = parts.next().unwrap_or("");
        let mbs_id = parts.next().unwrap_or(rest);
        country_ref
            .mbs_pool
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(mbs_id))
            .and_then(|mbs| {
                mbs.tranches
                    .iter()
                    .find(|t| format!("{:?}", t.priority).to_lowercase() == priority_str)
                    .map(|t| t.outstanding_balance)
            })
            .unwrap_or(0.0)
    } else {
        0.0
    }
}

/// v4.18 WS4: wrap the engine resolver — thin adapter so the command layer
/// doesn't rebuild the country lookup tables on every row.
fn resolve_instrument(
    inst: &str,
    entities: &[sim_engine::entities::Company],
    country_ref: &sim_engine::state::Country,
) -> (String, Option<String>) {
    sim_engine::securities::exchange::instrument_display_name(
        inst,
        entities,
        &country_ref.covered_bonds_issued,
        &country_ref.mbs_pool,
        &country_ref.debt_market.outstanding_securities,
    )
}

/// v4.23 WS4: asset-class bucket for the fund drill-down's outer ring —
/// derived from the instrument-id convention the exchange itself uses.
fn instrument_asset_class(
    inst: &str,
    country_ref: &sim_engine::state::Country,
) -> &'static str {
    if inst.starts_with("EQUITY:") {
        "Equities"
    } else if inst.starts_with("BOND:") {
        "Covered Bonds"
    } else if inst.starts_with("MBS:") || inst.starts_with("mbs:") {
        "Mortgage-Backed Securities"
    } else if country_ref
        .debt_market
        .outstanding_securities
        .iter()
        .any(|s| s.id == inst)
    {
        "Government Bonds"
    } else {
        "Other Securities"
    }
}

/// v4.23 WS4: sector label for the instrument's issuer — the drill-down's
/// middle ring. Equities/covered bonds resolve to the issuer company's
/// sector; MBS tranches map to Real Estate; treasuries to Government.
fn instrument_sector(
    inst: &str,
    entities: &[sim_engine::entities::Company],
    country_ref: &sim_engine::state::Country,
) -> String {
    if let Some(cid) = inst.strip_prefix("EQUITY:") {
        entities
            .iter()
            .find(|c| c.id == cid)
            .map(|c| format!("{:?}", c.sector))
            .unwrap_or_else(|| "Unlisted".to_string())
    } else if let Some(bond_id) = inst.strip_prefix("BOND:") {
        country_ref
            .covered_bonds_issued
            .iter()
            .find(|b| b.id == bond_id)
            .and_then(|b| {
                entities
                    .iter()
                    .find(|c| c.id == b.issuer_id)
                    .map(|c| format!("{:?}", c.sector))
            })
            .unwrap_or_else(|| "Banking".to_string())
    } else if inst.starts_with("MBS:") || inst.starts_with("mbs:") {
        "Real Estate".to_string()
    } else {
        "Government".to_string()
    }
}

/// v4.18 WS4: pretty-print a PascalCase class key ("FreePeasant" →
/// "Free Peasant") for CGT entity display names.
fn prettify_pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// v4.18 WS4: resolve a CGT entity id to a display name:
/// * `{region}:{rural|urban}:{Class}` → `"Class — Region (rural)"`
///   e.g. "Aristocracy — Lower Venedia (rural)".
/// * company/fund ids → the company `name`.
/// * VIP ids → the VIP `full_name`.
/// * `TREASURY*` → "State Treasury".
/// * anything else → the raw id (never fabricated).
fn resolve_cgt_entity_name(
    entity_id: &str,
    country_ref: &sim_engine::state::Country,
    entities: &[sim_engine::entities::Company],
) -> String {
    // Household-class investor id: {region_id}:{rural|urban}:{ClassKey}
    if let Some((region_id, is_rural, class_key)) =
        sim_engine::securities::exchange::parse_retail_investor_id(entity_id)
    {
        let region_label = country_ref
            .regions
            .iter()
            .find(|r| r.id == region_id)
            .map(|r| {
                if r.display_name.is_empty() {
                    r.id.clone()
                } else {
                    r.display_name.clone()
                }
            })
            .unwrap_or_else(|| region_id.to_string());
        let domain = if is_rural { "rural" } else { "urban" };
        return format!(
            "{} — {} ({})",
            prettify_pascal_case(class_key),
            region_label,
            domain
        );
    }

    if entity_id == "TREASURY" || entity_id.starts_with("TREASURY:") {
        return "State Treasury".to_string();
    }

    if let Some(c) = entities.iter().find(|c| c.id == entity_id) {
        return c.name.clone();
    }

    if let Some(registry) = country_ref.politics.vip_registry.as_ref() {
        if let Some(vip) = registry.get(entity_id) {
            return vip.full_name.clone();
        }
    }

    entity_id.to_string()
}

/// Phase 56: Get the full stock exchange snapshot for a country.
#[tauri::command]
pub async fn get_stock_exchange(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<StockExchangeResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let exchange = &country_ref.stock_exchange;
        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let registry = country_ref.politics.vip_registry.as_ref();
        let current_turn = engine_state.game_state.calendar.global_turn;

        // Build main index snapshot.
        let mi = &exchange.market_index;
        let change_pct = if mi.main_index_history.len() >= 2 {
            let prev = mi.main_index_history[mi.main_index_history.len() - 2];
            if prev > 0.0 {
                (mi.main_index_value - prev) / prev * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        let main_index = MarketIndexSnapshot {
            value: mi.main_index_value,
            change_pct,
            history: mi.main_index_history.clone(),
            total_market_cap: mi.total_market_cap,
            total_volume: mi.total_volume,
            advancing: mi.advancing,
            declining: mi.declining,
            volatility: mi.volatility,
        };

        // Build sector indices.
        let sector_indices: Vec<SectorIndexSnapshot> = mi
            .sector_indices
            .iter()
            .map(|(sector, value)| SectorIndexSnapshot {
                sector: sector.clone(),
                value: *value,
                history: mi
                    .sector_index_history
                    .get(sector)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();

        // Build listed company rows.
        let listed_companies: Vec<ListedCompanyRow> = entities
            .iter()
            .filter(|c| c.legal_form.is_listed() && c.shares_count > 0)
            .map(|c| {
                let instrument_id = format!("EQUITY:{}", c.id);
                let volume = exchange.get_turn_volume(&instrument_id, current_turn);
                let spread = exchange.get_spread(&instrument_id);
                let change_pct = if c.open_price > 0.0 {
                    (c.close_price - c.open_price) / c.open_price * 100.0
                } else {
                    0.0
                };
                let market_cap = c.share_price * c.shares_count as f64;

                let (ceo_name, ceo_vip_id) = c
                    .ceo_vip_id
                    .as_ref()
                    .and_then(|id| {
                        registry
                            .and_then(|r| r.get(id))
                            .map(|vip| (Some(vip.full_name.clone()), Some(id.clone())))
                    })
                    .unwrap_or((None, None));

                ListedCompanyRow {
                    company_id: c.id.clone(),
                    name: c.name.clone(),
                    sector: format!("{:?}", c.sector),
                    share_price: c.share_price,
                    change_pct,
                    market_cap,
                    pe_ratio: c.pe_ratio,
                    dividend_yield: c.dividend_yield,
                    volume,
                    open_price: c.open_price,
                    close_price: c.close_price,
                    spread,
                    ceo_name,
                    ceo_vip_id,
                }
            })
            .collect();

        // Build recent trades (last 50).
        let recent_trades: Vec<TradeRow> = exchange
            .trade_history
            .iter()
            .rev()
            .take(50)
            .map(|t| TradeRow {
                instrument_id: t.instrument_id.clone(),
                buyer_id: t.buyer_id.clone(),
                seller_id: t.seller_id.clone(),
                quantity: t.quantity,
                price: t.price,
                turn: t.turn,
            })
            .collect();

        // Build commodity spot rows.
        let commodity_spot: Vec<CommoditySpotRow> = exchange
            .commodity_spot
            .spot_prices
            .iter()
            .map(|(commodity_id, price)| CommoditySpotRow {
                commodity_id: commodity_id.clone(),
                spot_price: *price,
                open_interest: exchange
                    .commodity_spot
                    .open_interest
                    .get(commodity_id)
                    .copied()
                    .unwrap_or(0),
                history: exchange
                    .commodity_spot
                    .spot_history
                    .get(commodity_id)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();

        Ok(StockExchangeResponse {
            main_index,
            sector_indices,
            listed_companies,
            recent_trades,
            trading_halted: exchange.circuit_breaker.is_halted,
            commodity_spot,
            commodity_futures: Vec::new(), // Phase 57: futures rows
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 56: Get paginated listed companies for a country.
#[tauri::command]
pub async fn get_listed_companies(
    state: tauri::State<'_, AppState>,
    country: String,
    offset: usize,
    limit: usize,
    sector_filter: Option<String>,
) -> Result<ListedCompanyPageResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        // The frontend sends `undefined` (→ null) for "all sectors"; a
        // required-String parameter would fail arg deserialization and the
        // Listed Companies table renders empty despite live listings.
        let sector_filter = sector_filter.unwrap_or_default();
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let exchange = &country_ref.stock_exchange;
        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let registry = country_ref.politics.vip_registry.as_ref();
        let current_turn = engine_state.game_state.calendar.global_turn;

        let mut rows: Vec<ListedCompanyRow> = entities
            .iter()
            .filter(|c| c.legal_form.is_listed() && c.shares_count > 0)
            .filter(|c| {
                if sector_filter.is_empty() {
                    true
                } else {
                    format!("{:?}", c.sector) == sector_filter
                }
            })
            .map(|c| {
                let instrument_id = format!("EQUITY:{}", c.id);
                let volume = exchange.get_turn_volume(&instrument_id, current_turn);
                let spread = exchange.get_spread(&instrument_id);
                let change_pct = if c.open_price > 0.0 {
                    (c.close_price - c.open_price) / c.open_price * 100.0
                } else {
                    0.0
                };
                let market_cap = c.share_price * c.shares_count as f64;

                let (ceo_name, ceo_vip_id) = c
                    .ceo_vip_id
                    .as_ref()
                    .and_then(|id| {
                        registry
                            .and_then(|r| r.get(id))
                            .map(|vip| (Some(vip.full_name.clone()), Some(id.clone())))
                    })
                    .unwrap_or((None, None));

                ListedCompanyRow {
                    company_id: c.id.clone(),
                    name: c.name.clone(),
                    sector: format!("{:?}", c.sector),
                    share_price: c.share_price,
                    change_pct,
                    market_cap,
                    pe_ratio: c.pe_ratio,
                    dividend_yield: c.dividend_yield,
                    volume,
                    open_price: c.open_price,
                    close_price: c.close_price,
                    spread,
                    ceo_name,
                    ceo_vip_id,
                }
            })
            .collect();

        let total_count = rows.len();
        let end = (offset + limit).min(total_count);
        if offset < total_count {
            rows = rows[offset..end].to_vec();
        } else {
            rows.clear();
        }

        Ok(ListedCompanyPageResponse { rows, total_count })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 56: Get detailed market view for a single listed company.
#[tauri::command]
pub async fn get_company_market_detail(
    state: tauri::State<'_, AppState>,
    country: String,
    company_id: String,
) -> Result<Option<ListedCompanyDetail>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let exchange = &country_ref.stock_exchange;
        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let company = match entities.iter().find(|c| c.id == company_id) {
            Some(c) => c,
            None => return Ok(None),
        };

        if !company.legal_form.is_listed() {
            return Ok(None);
        }

        let current_turn = engine_state.game_state.calendar.global_turn;
        let instrument_id = format!("EQUITY:{}", company.id);
        let volume = exchange.get_turn_volume(&instrument_id, current_turn);
        let spread = exchange.get_spread(&instrument_id);
        let market_cap = company.share_price * company.shares_count as f64;

        let recent_trades: Vec<TradeRow> = exchange
            .trade_history
            .iter()
            .rev()
            .filter(|t| t.instrument_id == instrument_id)
            .take(20)
            .map(|t| TradeRow {
                instrument_id: t.instrument_id.clone(),
                buyer_id: t.buyer_id.clone(),
                seller_id: t.seller_id.clone(),
                quantity: t.quantity,
                price: t.price,
                turn: t.turn,
            })
            .collect();

        Ok(Some(ListedCompanyDetail {
            company_id: company.id.clone(),
            name: company.name.clone(),
            sector: format!("{:?}", company.sector),
            share_price: company.share_price,
            market_cap,
            pe_ratio: company.pe_ratio,
            dividend_yield: company.dividend_yield,
            eps: company.eps,
            open_price: company.open_price,
            close_price: company.close_price,
            spread,
            volume,
            shares_count: company.shares_count,
            free_float: company.legal_form.free_float(),
            recent_trades,
        }))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

// ============================================================================
// PHASE 57: FUND & KNF COMMANDS
// ============================================================================

/// Phase 57: Get all investment funds for a country.
#[tauri::command]
pub async fn get_funds(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<FundRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let registry = country_ref.politics.vip_registry.as_ref();

        let funds: Vec<FundRow> = entities
            .iter()
            .filter(|c| c.fund_type.is_some() && c.fund_ledger.is_some())
            .map(|c| {
                let ledger = c.fund_ledger.as_ref().unwrap();
                let (manager_name, manager_vip_id, manager_trait) =
                    if let Some(ref mid) = ledger.fund_manager_vip_id {
                        if let Some(registry) = registry {
                            if let Some(vip) = registry.get(mid) {
                                (vip.full_name.clone(), mid.clone(), vip.main_trait.clone())
                            } else {
                                ("Unknown".to_string(), mid.clone(), "".to_string())
                            }
                        } else {
                            ("Unknown".to_string(), mid.clone(), "".to_string())
                        }
                    } else {
                        ("Unknown".to_string(), "".to_string(), "".to_string())
                    };

                // v4.18 WS4: top holdings carry RESOLVED display names and
                // market values — the raw instrument ids stay available via
                // the detail view's `instrument_id` field.
                let top_holdings: Vec<(String, f64)> = if let Some(ref acct) = c.brokerage_account {
                    let mut holdings: Vec<(String, f64)> = acct
                        .portfolio
                        .iter()
                        .map(|(inst, lots)| {
                            let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                            let mv = instrument_market_price(inst, entities, country_ref)
                                * qty as f64;
                            (inst.clone(), qty, mv)
                        })
                        .filter(|(_, qty, _)| *qty > 0)
                        .map(|(inst, _qty, mv)| {
                            let (name, _ticker) =
                                resolve_instrument(&inst, entities, country_ref);
                            (name, mv)
                        })
                        .collect();
                    holdings
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    holdings.into_iter().take(10).collect()
                } else {
                    Vec::new()
                };

                // Real AUM = cash + frozen cash + holdings marked to market —
                // the old code priced every holding at the FUND's own
                // share_price, producing a wrong (often near-zero) AUM.
                let total_aum = if let Some(ref acct) = c.brokerage_account {
                    acct.cash
                        + acct.frozen_cash
                        + fund_holdings_value(acct, ledger, entities, country_ref)
                } else {
                    0.0
                };

                let fund_type_str = fund_type_label(&c.fund_type);

                FundRow {
                    fund_id: c.id.clone(),
                    name: c.name.clone(),
                    fund_type: fund_type_str.to_string(),
                    risk_profile: ledger.investment_mandate.risk_profile.label().to_string(),
                    nav_per_share: ledger.nav_per_share,
                    total_aum,
                    manager_name,
                    manager_vip_id,
                    manager_trait,
                    shares_outstanding: ledger.shares_outstanding,
                    // v4.19 WS3: real NAV-history return — `year_start_nav`
                    // is restriken every fiscal-year boundary (24 turns).
                    ytd_return: if ledger.year_start_nav > 0.0 {
                        ledger.nav_per_share / ledger.year_start_nav - 1.0
                    } else {
                        0.0
                    },
                    top_holdings,
                }
            })
            .collect();

        Ok(funds)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 57: Get detailed fund information.
#[tauri::command]
pub async fn get_fund_detail(
    state: tauri::State<'_, AppState>,
    country: String,
    fund_id: String,
) -> Result<Option<FundDetail>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let registry = country_ref.politics.vip_registry.as_ref();

        let fund = match entities.iter().find(|c| c.id == fund_id) {
            Some(c) => c,
            None => return Ok(None),
        };

        if fund.fund_type.is_none() || fund.fund_ledger.is_none() {
            return Ok(None);
        }

        let ledger = fund.fund_ledger.as_ref().unwrap();
        // v4.19 WS3: manager resolution order — ledger VIP id → registry,
        // ceo_vip_id → registry, persisted ledger name — so a fund always
        // shows its actual manager instead of falling to "Unknown".
        let resolve_vip = |vip_id: &str| -> Option<(String, String)> {
            registry
                .and_then(|r| r.get(vip_id).or_else(|| r.get_by_name(vip_id)))
                .map(|v| (v.full_name.clone(), v.main_trait.clone()))
        };
        let (manager_name, manager_vip_id, manager_trait) = ledger
            .fund_manager_vip_id
            .as_ref()
            .and_then(|mid| resolve_vip(mid).map(|(n, t)| (n, mid.clone(), t)))
            .or_else(|| {
                fund.ceo_vip_id
                    .as_ref()
                    .and_then(|cid| resolve_vip(cid).map(|(n, t)| (n, cid.clone(), t)))
            })
            .unwrap_or_else(|| {
                (
                    if ledger.fund_manager_name.is_empty() {
                        "Unassigned".to_string()
                    } else {
                        ledger.fund_manager_name.clone()
                    },
                    ledger
                        .fund_manager_vip_id
                        .clone()
                        .or_else(|| fund.ceo_vip_id.clone())
                        .unwrap_or_default(),
                    "".to_string(),
                )
            });

        // v4.18 WS4: resolved holdings — each row carries the display name
        // and listing code so the UI never renders raw `EQUITY:`/`BOND:` ids.
        // Treasury-bond ledger holdings are surfaced too (they are part of
        // AUM but live in `fund_ledger.bond_holdings`, not the brokerage
        // portfolio).
        let mut portfolio_holdings: Vec<FundHoldingRow> =
            if let Some(ref acct) = fund.brokerage_account {
                acct.portfolio
                    .iter()
                    .map(|(inst, lots)| {
                        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                        // Quantity-weighted average cost basis (the old
                        // unweighted mean of per-lot prices overstated cost
                        // for lumpy books).
                        let avg_cost = lots
                            .iter()
                            .map(|l| l.cost_basis * l.quantity as f64)
                            .sum::<f64>()
                            / (qty as f64).max(1.0);
                        let market_value =
                            instrument_market_price(inst, entities, country_ref) * qty as f64;
                        (inst.clone(), qty, avg_cost, market_value)
                    })
                    .filter(|(_, qty, _, _)| *qty > 0)
                    .map(|(inst, qty, avg_cost, market_value)| {
                        let (display_name, ticker) =
                            resolve_instrument(&inst, entities, country_ref);
                        let asset_class = instrument_asset_class(&inst, country_ref);
                        let sector = instrument_sector(&inst, entities, country_ref);
                        FundHoldingRow {
                            instrument_id: inst,
                            display_name,
                            ticker,
                            quantity: qty,
                            avg_cost,
                            market_value,
                            weight_pct: 0.0, // filled below once AUM is known
                            asset_class: asset_class.to_string(),
                            sector,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };

        // Treasury-bond ledger holdings as resolved rows.
        for holding in ledger.bond_holdings.iter().filter(|h| !h.redeemed) {
            let (display_name, ticker) =
                resolve_instrument(&holding.security_id, entities, country_ref);
            portfolio_holdings.push(FundHoldingRow {
                instrument_id: holding.security_id.clone(),
                display_name,
                ticker,
                quantity: 1,
                avg_cost: holding.purchase_price,
                market_value: holding.face_value,
                weight_pct: 0.0,
                asset_class: instrument_asset_class(&holding.security_id, country_ref)
                    .to_string(),
                sector: instrument_sector(&holding.security_id, entities, country_ref),
            });
        }

        // Real AUM = cash + frozen cash + holdings marked to market (was
        // cash-only, which showed ~0 for fully invested funds and made the
        // detail view inconsistent with the engine's own NAV math).
        let total_aum = if let Some(ref acct) = fund.brokerage_account {
            acct.cash + acct.frozen_cash + fund_holdings_value(acct, ledger, entities, country_ref)
        } else {
            0.0
        };
        if total_aum > 0.0 {
            for h in portfolio_holdings.iter_mut() {
                h.weight_pct = h.market_value / total_aum;
            }
        }

        let mut top_holdings: Vec<(String, f64)> = portfolio_holdings
            .iter()
            .map(|h| (h.display_name.clone(), h.market_value))
            .collect();
        top_holdings
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_holdings.truncate(10);

        Ok(Some(FundDetail {
            fund_id: fund.id.clone(),
            name: fund.name.clone(),
            fund_type: fund_type_label(&fund.fund_type).to_string(),
            risk_profile: ledger.investment_mandate.risk_profile.label().to_string(),
            nav_per_share: ledger.nav_per_share,
            total_aum,
            manager_name,
            manager_vip_id,
            manager_trait,
            shares_outstanding: ledger.shares_outstanding,
            // v4.19 WS3: real NAV-history return — the ledger restrikes
            // `year_start_nav` every fiscal-year boundary (24 turns).
            ytd_return: if ledger.year_start_nav > 0.0 {
                ledger.nav_per_share / ledger.year_start_nav - 1.0
            } else {
                0.0
            },
            leverage_ratio: ledger.leverage_ratio,
            management_fee: ledger.management_fee,
            performance_fee: ledger.performance_fee,
            top_holdings,
            portfolio_holdings,
            // v4.23 WS4: cash legs of AUM — shown as a "Cash &
            // Equivalents" leaf in the allocation drill-down.
            cash_value: fund
                .brokerage_account
                .as_ref()
                .map(|a| a.cash)
                .unwrap_or(0.0),
            frozen_cash: fund
                .brokerage_account
                .as_ref()
                .map(|a| a.frozen_cash)
                .unwrap_or(0.0),
        }))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 57: Get KNF audit findings.
#[tauri::command]
pub async fn get_knf_findings(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<KnfFindingRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let knf = &country_ref.knf;
        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let findings: Vec<KnfFindingRow> = knf
            .audit_findings
            .iter()
            .rev()
            .take(100) // Last 100 findings
            .map(|f| {
                let entity_name = entities
                    .iter()
                    .find(|c| c.id == f.bank_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| f.bank_id.clone());

                let (violation_type, description) = match f.violation_type {
                    sim_engine::securities::knf::ViolationType::LowTier1Capital => (
                        "Low Tier 1 Capital",
                        "Bank's Tier 1 capital ratio fell below minimum requirement",
                    ),
                    sim_engine::securities::knf::ViolationType::ExcessiveLeverage => (
                        "Excessive Leverage",
                        "Bank's leverage ratio exceeded regulatory limits",
                    ),
                    sim_engine::securities::knf::ViolationType::ImproperReserving => (
                        "Improper Reserving",
                        "Bank failed to maintain proper loan loss reserves",
                    ),
                    sim_engine::securities::knf::ViolationType::MarketManipulation => (
                        "Market Manipulation",
                        "Market manipulation detected in trading activities",
                    ),
                    sim_engine::securities::knf::ViolationType::AccountingFraud => (
                        "Accounting Fraud",
                        "Profit diversion by corrupt CEO/manager detected",
                    ),
                    sim_engine::securities::knf::ViolationType::FundLeverageExceeded => (
                        "Fund Leverage Exceeded",
                        "Fund leverage exceeded regulatory limits",
                    ),
                    sim_engine::securities::knf::ViolationType::InsiderTrading => (
                        "Insider Trading",
                        "Fund manager traded on companies where they're CEO or board member",
                    ),
                };

                KnfFindingRow {
                    entity_id: f.bank_id.clone(),
                    entity_name,
                    violation_type: violation_type.to_string(),
                    severity: f.severity,
                    turn: f.turn,
                    description: description.to_string(),
                    penalty: f.severity as f64 * 100_000.0, // Estimated penalty
                }
            })
            .collect();

        Ok(findings)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 57: Get capital gains tax summary.
#[tauri::command]
pub async fn get_capital_gains_summary(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<CapitalGainsTaxSummary, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let cgt = &country_ref.capital_gains_tax;
        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        // v4.18 WS4: the settled table reads the REAL per-entity ledger —
        // `last_year_settlements` is populated by `settle_year_end` and the
        // turn loop fills `tax_paid` with the cash actually debited, so the
        // Tax column sums exactly to `total_tax_collected`.
        let rows: Vec<CapitalGainsTaxRow> = cgt
            .last_year_settlements
            .iter()
            .map(|(entity_id, s)| CapitalGainsTaxRow {
                entity_id: entity_id.clone(),
                entity_name: resolve_cgt_entity_name(entity_id, country_ref, entities),
                realized_gains: s.realized_gains,
                realized_losses: s.realized_losses,
                tax_owed: s.tax_paid,
                carried_forward_losses: s.losses_carried_forward,
            })
            .collect();

        // Current fiscal year's unsettled accruals — the tax shown is the
        // accrued estimate at the configured rate (not yet collected).
        let pending_rows: Vec<CapitalGainsTaxRow> = cgt
            .accruals
            .iter()
            .filter(|(_, accrual)| {
                accrual.realized_gains > 0.0
                    || accrual.realized_losses > 0.0
                    || accrual.carried_forward_losses > 0.0
            })
            .map(|(entity_id, accrual)| {
                let tax_owed = if accrual.realized_gains > accrual.realized_losses {
                    (accrual.realized_gains - accrual.realized_losses) * cgt.securities_cgt_rate
                } else {
                    0.0
                };

                CapitalGainsTaxRow {
                    entity_id: entity_id.clone(),
                    entity_name: resolve_cgt_entity_name(entity_id, country_ref, entities),
                    realized_gains: accrual.realized_gains,
                    realized_losses: accrual.realized_losses,
                    tax_owed,
                    carried_forward_losses: accrual.carried_forward_losses,
                }
            })
            .collect();

        Ok(CapitalGainsTaxSummary {
            rows,
            pending_rows,
            total_tax_collected: cgt.tax_collected_this_year,
            annual_tax_history: cgt.annual_tax_history.clone(),
            annual_tax_history_years: cgt.annual_tax_history_years.clone(),
            last_settled_year: cgt.last_settled_year,
            securities_cgt_rate: cgt.securities_cgt_rate,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
