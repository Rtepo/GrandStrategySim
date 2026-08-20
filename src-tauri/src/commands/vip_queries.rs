use crate::state::AppState;
use sim_engine::ui::snapshot::{
    build_country_snapshot, VipPageResponse, VipDossier, ViewQuery, PageQuery, VipFilter,
    RoleOption,
};
use sim_engine::politics::vip_registry::VipRoleExtended;

#[tauri::command]
pub async fn get_paginated_vips(
    state: tauri::State<'_, AppState>,
    country: String,
    offset: usize,
    limit: usize,
    search: String,
    show_dead: bool,
    role_filter: Option<String>,
) -> Result<VipPageResponse, String> {
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
            vip_page: PageQuery { offset, limit },
            vip_filter: VipFilter {
                search,
                show_dead,
                role_filter: role_filter.unwrap_or_default(),
            },
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

        Ok(VipPageResponse {
            rows: snap.vips_page,
            total_count: snap.vip_total_count,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Phase 54: Returns all valid VIP roles from the authoritative Rust enum.
/// The frontend uses this to populate the role filter dropdown dynamically.
#[tauri::command]
pub async fn get_available_roles() -> Result<Vec<RoleOption>, String> {
    let roles = VipRoleExtended::all();
    let result = roles
        .iter()
        .map(|r| RoleOption {
            value: r.as_str().to_string(),
            label: r.as_str().to_string(),
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn get_vip_dossier(
    state: tauri::State<'_, AppState>,
    country: String,
    vip_id: String,
) -> Result<Option<VipDossier>, String> {
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
            vip_dossier_id: Some(vip_id),
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

        Ok(snap.vip_dossier)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
