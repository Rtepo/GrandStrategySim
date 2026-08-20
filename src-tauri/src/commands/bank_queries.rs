use crate::state::AppState;
use sim_engine::ui::snapshot::{
    build_country_snapshot, BankPageResponse, BankingAggregates, BankingHistoryResponse, ViewQuery, PageQuery,
};

/// Phase 54: Returns rolling banking history for sparkline tooltips.
#[tauri::command]
pub async fn get_banking_history(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<BankingHistoryResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let history = engine_state
            .game_state
            .banking_history
            .get(&country)
            .cloned()
            .unwrap_or_default();

        Ok(BankingHistoryResponse {
            turns: history.turns,
            total_reserves: history.total_reserves,
            total_deposits: history.total_deposits,
            total_loans: history.total_loans,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_paginated_banks(
    state: tauri::State<'_, AppState>,
    country: String,
    offset: usize,
    limit: usize,
) -> Result<BankPageResponse, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let view = ViewQuery {
            bank_page: PageQuery { offset, limit },
            ..Default::default()
        };

        let snap = build_country_snapshot(
            country_ref,
            &engine_state.game_state.market_history,
            &engine_state.turn_context.market,
            entities.1,
            entities.0,
            &view,
        );

        Ok(BankPageResponse {
            rows: snap.banks_page,
            total_count: snap.bank_total_count,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_banking_aggregates(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<BankingAggregates, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard
            .as_ref()
            .ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let entities = engine_state
            .turn_context
            .entities
            .get(&country)
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let view = ViewQuery::default();
        let snap = build_country_snapshot(
            country_ref,
            &engine_state.game_state.market_history,
            &engine_state.turn_context.market,
            entities.1,
            entities.0,
            &view,
        );

        let finance = &snap.finance;
        Ok(BankingAggregates {
            total_bank_reserves: finance.total_bank_reserves,
            total_bank_deposits: finance.total_bank_deposits,
            total_bank_loans: finance.total_bank_loans,
            total_consumer_debt: finance.total_consumer_debt,
            dspw_bank_count: finance.dspw_bank_count,
            central_bank_rate: snap.central_bank_rate,
            m0: snap.money_supply.m0,
            m3: snap.money_supply.m3,
            cb_fx_reserves_total: finance.cb_fx_reserves_total,
            cb_gold_reserves: finance.cb_gold_reserves,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
