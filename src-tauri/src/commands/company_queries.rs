use crate::state::AppState;
use serde::Serialize;
use sim_engine::registries::enums::Sector;
use sim_engine::ui::snapshot::{
    build_country_snapshot, CompanyDetail, CompanyFilter, CompanyPageResponse, PageQuery,
    RegionOption, ViewQuery,
};

#[derive(Debug, Clone, Serialize)]
pub struct SectorOption {
    pub value: String,
    pub label: String,
}

#[tauri::command]
pub async fn get_available_sectors() -> Result<Vec<SectorOption>, String> {
    let sectors = vec![
        Sector::Mining,
        Sector::Agriculture,
        Sector::HeavyIndustry,
        Sector::LightIndustry,
        Sector::ArmamentsIndustry,
        Sector::LocalServices,
        Sector::ExportServices,
        Sector::Construction,
        Sector::Energy,
        Sector::PublicServices,
        Sector::MedicalServices,
        Sector::EducationalServices,
        Sector::TransportLogistics,
        Sector::PublicAdministration,
        Sector::Banking,
        Sector::MediaAndEntertainment,
        Sector::WasteManagement,
        Sector::Hospitality,
        Sector::NGO,
        Sector::Religion,
        Sector::MaintenanceWorkshops,
        Sector::Government,
    ];
    let result = sectors
        .iter()
        .map(|s| {
            let value = serde_json::to_string(s)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let label = match s {
                Sector::Mining => "Mining",
                Sector::Agriculture => "Agriculture",
                Sector::HeavyIndustry => "Heavy Industry",
                Sector::LightIndustry => "Light Industry",
                Sector::ArmamentsIndustry => "Armaments Industry",
                Sector::LocalServices => "Local Services",
                Sector::ExportServices => "Export Services",
                Sector::Construction => "Construction",
                Sector::Energy => "Energy",
                Sector::PublicServices => "Public Services",
                Sector::MedicalServices => "Medical Services",
                Sector::EducationalServices => "Educational Services",
                Sector::TransportLogistics => "Transport & Logistics",
                Sector::PublicAdministration => "Public Administration",
                Sector::Banking => "Banking",
                Sector::MediaAndEntertainment => "Media & Entertainment",
                Sector::WasteManagement => "Waste Management",
                Sector::Hospitality => "Hospitality",
                Sector::NGO => "NGO",
                Sector::Religion => "Religion",
                Sector::MaintenanceWorkshops => "Maintenance Workshops",
                Sector::Government => "Government",
            }
            .to_string();
            SectorOption { value, label }
        })
        .collect();
    Ok(result)
}

/// Phase 54: Returns all regions for a country from the backend, for the
/// Companies tab region filter dropdown.
#[tauri::command]
pub async fn get_available_regions(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<RegionOption>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let country_ref = engine_state
            .game_state
            .countries
            .get(&country)
            .ok_or(format!("Country '{}' not found", country))?;

        let result = country_ref
            .regions
            .iter()
            .map(|r| RegionOption {
                value: r.id.clone(),
                label: if r.display_name.is_empty() {
                    r.id.clone()
                } else {
                    r.display_name.clone()
                },
            })
            .collect();
        Ok(result)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_paginated_companies(
    state: tauri::State<'_, AppState>,
    country: String,
    offset: usize,
    limit: usize,
    search: String,
    sector_filter: String,
    region_filter: Option<String>,
) -> Result<CompanyPageResponse, String> {
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
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let view = ViewQuery {
            company_page: PageQuery { offset, limit },
            company_filter: CompanyFilter {
                search,
                sector_filter,
                region_filter: region_filter.unwrap_or_default(),
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

        Ok(CompanyPageResponse {
            rows: snap.companies_page,
            total_count: snap.company_total_count,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn get_company_detail(
    state: tauri::State<'_, AppState>,
    country: String,
    company_id: String,
) -> Result<Option<CompanyDetail>, String> {
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
            .map(|e| (e.companies.as_slice(), e.buildings.as_slice()))
            .unwrap_or((&[], &[]));

        let view = ViewQuery {
            company_detail_id: Some(company_id),
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

        Ok(snap.company_detail)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
