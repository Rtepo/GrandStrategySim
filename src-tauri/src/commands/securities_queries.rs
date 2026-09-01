use crate::state::AppState;
use sim_engine::ui::snapshot::{
    CapitalGainsTaxRow, CapitalGainsTaxSummary, CommoditySpotRow, FundDetail, FundRow,
    KnfFindingRow, ListedCompanyDetail, ListedCompanyPageResponse, ListedCompanyRow,
    MarketIndexSnapshot, SectorIndexSnapshot, StockExchangeResponse, TradeRow,
};

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
    sector_filter: String,
) -> Result<ListedCompanyPageResponse, String> {
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

                // Compute top holdings from portfolio.
                let top_holdings: Vec<(String, f64)> = if let Some(ref acct) = c.brokerage_account {
                    let mut holdings: Vec<(String, f64)> = acct
                        .portfolio
                        .iter()
                        .map(|(inst, lots)| {
                            let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                            (inst.clone(), qty as f64)
                        })
                        .filter(|(_, qty)| *qty > 0.0)
                        .collect();
                    holdings
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    holdings.into_iter().take(10).collect()
                } else {
                    Vec::new()
                };

                let total_aum = if let Some(ref acct) = c.brokerage_account {
                    acct.cash
                        + top_holdings
                            .iter()
                            .map(|(_, w)| *w * c.share_price)
                            .sum::<f64>()
                } else {
                    0.0
                };

                let fund_type_str = match c.fund_type {
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
                };

                FundRow {
                    fund_id: c.id.clone(),
                    name: c.name.clone(),
                    fund_type: fund_type_str.to_string(),
                    nav_per_share: ledger.nav_per_share,
                    total_aum,
                    manager_name,
                    manager_vip_id,
                    manager_trait,
                    shares_outstanding: ledger.shares_outstanding,
                    ytd_return: 0.0, // TODO: compute from NAV history
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

        let portfolio_holdings: Vec<(String, u64, f64)> =
            if let Some(ref acct) = fund.brokerage_account {
                acct.portfolio
                    .iter()
                    .map(|(inst, lots)| {
                        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                        let avg_cost =
                            lots.iter().map(|l| l.cost_basis).sum::<f64>() / (qty as f64).max(1.0);
                        (inst.clone(), qty, avg_cost)
                    })
                    .filter(|(_, qty, _)| *qty > 0)
                    .collect()
            } else {
                Vec::new()
            };

        let top_holdings: Vec<(String, f64)> = portfolio_holdings
            .iter()
            .map(|(inst, qty, _)| (inst.clone(), *qty as f64))
            .take(10)
            .collect();

        let total_aum = if let Some(ref acct) = fund.brokerage_account {
            acct.cash
        } else {
            0.0
        };

        Ok(Some(FundDetail {
            fund_id: fund.id.clone(),
            name: fund.name.clone(),
            fund_type: format!("{:?}", fund.fund_type),
            nav_per_share: ledger.nav_per_share,
            total_aum,
            manager_name,
            manager_vip_id,
            manager_trait,
            shares_outstanding: ledger.shares_outstanding,
            ytd_return: 0.0,
            leverage_ratio: ledger.leverage_ratio,
            management_fee: ledger.management_fee,
            performance_fee: ledger.performance_fee,
            top_holdings,
            portfolio_holdings,
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

        let rows: Vec<CapitalGainsTaxRow> = cgt
            .accruals
            .iter()
            .map(|(entity_id, accrual)| {
                let tax_owed = if accrual.realized_gains > accrual.realized_losses {
                    (accrual.realized_gains - accrual.realized_losses) * cgt.securities_cgt_rate
                } else {
                    0.0
                };

                CapitalGainsTaxRow {
                    entity_id: entity_id.clone(),
                    realized_gains: accrual.realized_gains,
                    realized_losses: accrual.realized_losses,
                    tax_owed,
                    carried_forward_losses: accrual.carried_forward_losses,
                }
            })
            .collect();

        Ok(CapitalGainsTaxSummary {
            rows,
            total_tax_collected: cgt.tax_collected_this_year,
            annual_tax_history: cgt.annual_tax_history.clone(),
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
