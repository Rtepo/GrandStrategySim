//! Phase 60: Tauri commands for cadastre, zoning, courts, and arbitration queries.
//!
//! Phase 68b: Role-gating has been removed for Zero-Player mode. All data is
//! visible to the observer dashboard.

use crate::state::AppState;
use sim_engine::society::cadastre::{
    self, ZoningDesignation, ParcelOwnerType,
};
use sim_engine::society::real_estate_market::generate_ministry_land_report;
use sim_engine::ui::snapshot::{
    CadastreSummaryRow, CadastreSummaryResponse,
    CadastreZoningEntry, CadastreOwnerEntry,
    ZoningPlanRow, ZoningPlansResponse,
    CourtBacklogRow, CourtBacklogResponse,
    ArbitrationCaseRow, ArbitrationCasesResponse,
    MinistryLandReportDTO, MinistryRegionalSummaryDTO,
};

/// Get cadastre summary per region (public data — visible to all players).
#[tauri::command]
pub async fn get_cadastre_summary(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<CadastreSummaryResponse, String> {
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

        let mut rows = Vec::new();
        for region in &country_ref.regions {
            if region.node_type != sim_engine::society::geography::NodeType::LandRegion {
                continue;
            }

            let region_parcels: Vec<&sim_engine::society::cadastre::ParcelChunk> =
                country_ref.cadastre.parcels.values()
                    .filter(|p| p.region_id == region.id)
                    .collect();

            if region_parcels.is_empty() {
                rows.push(CadastreSummaryRow {
                    region_id: region.id.clone(),
                    region_name: region.display_name.clone(),
                    ..Default::default()
                });
                continue;
            }

            let total_hectares: f64 = region_parcels.iter().map(|p| p.size_hectares).sum();
            let total_value: f64 = region_parcels.iter().map(|p| p.current_value).sum();
            let avg_certainty: f64 = region_parcels.iter().map(|p| p.legal_certainty).sum::<f64>()
                / region_parcels.len() as f64;
            let avg_infra: f64 = region_parcels.iter().map(|p| p.infrastructure_access).sum::<f64>()
                / region_parcels.len() as f64;

            // Zoning distribution
            let mut zoning_map: std::collections::BTreeMap<ZoningDesignation, f64> =
                std::collections::BTreeMap::new();
            for p in &region_parcels {
                *zoning_map.entry(p.zoning).or_insert(0.0) += p.size_hectares;
            }
            let zoning_distribution: Vec<CadastreZoningEntry> = zoning_map
                .iter()
                .map(|(z, h)| CadastreZoningEntry {
                    designation: format!("{:?}", z),
                    percentage: if total_hectares > 0.0 { h / total_hectares } else { 0.0 },
                })
                .collect();

            // Owner distribution
            let mut owner_map: std::collections::BTreeMap<ParcelOwnerType, f64> =
                std::collections::BTreeMap::new();
            for p in &region_parcels {
                *owner_map.entry(p.owner_type).or_insert(0.0) += p.size_hectares;
            }
            let owner_distribution: Vec<CadastreOwnerEntry> = owner_map
                .iter()
                .map(|(t, h)| CadastreOwnerEntry {
                    owner_type: format!("{:?}", t),
                    percentage: if total_hectares > 0.0 { h / total_hectares } else { 0.0 },
                })
                .collect();

            let foreign_land: f64 = region_parcels
                .iter()
                .filter(|p| p.owner_type == ParcelOwnerType::ForeignFund)
                .map(|p| p.size_hectares)
                .sum();
            let foreign_pct = if total_hectares > 0.0 { foreign_land / total_hectares } else { 0.0 };

            let border_conflicts = country_ref.border_conflicts.count_for_region(&region.id) as u32;

            rows.push(CadastreSummaryRow {
                region_id: region.id.clone(),
                region_name: region.display_name.clone(),
                total_hectares,
                total_value,
                avg_legal_certainty: avg_certainty,
                avg_infrastructure_access: avg_infra,
                zoning_distribution,
                owner_distribution,
                border_conflicts,
                foreign_ownership_pct: foreign_pct,
            });
        }

        Ok(CadastreSummaryResponse { rows })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Get zoning plans per region (public data).
#[tauri::command]
pub async fn get_zoning_plans(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<ZoningPlansResponse, String> {
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

        let mut rows = Vec::new();
        for region in &country_ref.regions {
            if region.node_type != sim_engine::society::geography::NodeType::LandRegion {
                continue;
            }
            if let Some(gov) = &region.governance {
                for plan in &gov.zoning_plans.plans {
                    let target_distribution: Vec<CadastreZoningEntry> = plan
                        .target_distribution
                        .iter()
                        .map(|(z, pct)| CadastreZoningEntry {
                            designation: format!("{:?}", z),
                            percentage: *pct,
                        })
                        .collect();

                    rows.push(ZoningPlanRow {
                        region_id: region.id.clone(),
                        region_name: region.display_name.clone(),
                        plan_id: plan.plan_id.clone(),
                        enacted_turn: plan.enacted_turn,
                        implementation_progress: plan.implementation_progress,
                        target_distribution,
                        governor_name: gov.head.name.clone(),
                        governor_trait: gov.head.main_trait.clone(),
                    });
                }
            }
        }

        Ok(ZoningPlansResponse { rows })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Get court backlog per region (public data).
/// Includes `has_crisis` flag for the pulsating red warning indicator.
#[tauri::command]
pub async fn get_court_backlog(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<CourtBacklogResponse, String> {
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

        let justice_law = country_ref.politics.justice_law.clone().unwrap_or_default();
        let court_wait = justice_law.court_wait_time_target;
        let court_status_str = match court_wait {
            sim_engine::politics::laws::CourtWaitTime::Expedited => "Expedited",
            sim_engine::politics::laws::CourtWaitTime::Normal => "Normal",
            sim_engine::politics::laws::CourtWaitTime::Backlogged => "Backlogged",
            sim_engine::politics::laws::CourtWaitTime::Paralyzed => "Paralyzed",
        };
        let is_crisis_status = matches!(
            court_wait,
            sim_engine::politics::laws::CourtWaitTime::Backlogged
                | sim_engine::politics::laws::CourtWaitTime::Paralyzed
        );

        let mut rows = Vec::new();
        let mut has_crisis = false;

        for region in &country_ref.regions {
            if region.node_type != sim_engine::society::geography::NodeType::LandRegion {
                continue;
            }

            let border_conflicts = country_ref.border_conflicts.count_for_region(&region.id) as u32;
            let court_load = country_ref.border_conflicts.court_load_for_region(&region.id);
            let arbitration_cases = country_ref.arbitration_court.pending_count() as u32;

            // A region is in crisis if court is backlogged/paralyzed AND there are
            // significant pending cases or border conflicts
            let region_crisis = is_crisis_status && (border_conflicts > 5 || court_load > 10.0);
            if region_crisis {
                has_crisis = true;
            }

            rows.push(CourtBacklogRow {
                region_id: region.id.clone(),
                region_name: region.display_name.clone(),
                pending_cases: border_conflicts + arbitration_cases,
                border_conflicts,
                arbitration_cases,
                avg_processing_turns: court_load * 2.0,
                court_status: court_status_str.to_string(),
                is_crisis: region_crisis,
            });
        }

        Ok(CourtBacklogResponse { rows, has_crisis })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Get arbitration cases (public data).
#[tauri::command]
pub async fn get_arbitration_cases(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<ArbitrationCasesResponse, String> {
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

        let rows: Vec<ArbitrationCaseRow> = country_ref
            .arbitration_court
            .cases
            .values()
            .map(|c| ArbitrationCaseRow {
                case_id: c.case_id.clone(),
                plaintiff_name: c.plaintiff_id.clone(),
                plaintiff_type: format!("{:?}", c.plaintiff_type),
                compensation_claimed: c.compensation_claimed,
                status: format!("{:?}", c.status),
                filed_turn: c.filed_turn,
                state_strength_assessment: c.state_strength_assessment,
            })
            .collect();

        let total_exposure = country_ref.arbitration_court.unresolved_liabilities();

        // Compute current state strength
        let justice_law = country_ref.politics.justice_law.clone().unwrap_or_default();
        let state_strength = cadastre::assess_state_strength(
            &justice_law,
            &justice_law.court_wait_time_target,
            country_ref.budget.liquid_reserves,
            &country_ref.arbitration_config,
        );

        Ok(ArbitrationCasesResponse {
            rows,
            total_exposure,
            state_strength,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Get ministry land report — **ROLE-GATED**.
///
/// Phase 68b: Zero-Player mode — role-gating removed. All data is visible
/// to the observer dashboard for AI debugging and verification.
#[tauri::command]
pub async fn get_ministry_land_report(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<MinistryLandReportDTO, String> {
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

        let report = generate_ministry_land_report(
            &country_ref.cadastre,
            &country_ref.border_conflicts,
            &country_ref.arbitration_court,
            &country_ref.regions,
            engine_state.game_state.calendar.global_turn,
        );

        // Convert to DTO
        let dto = MinistryLandReportDTO {
            report_turn: report.report_turn,
            total_land_value: report.total_land_value,
            total_hectares: report.total_hectares,
            foreign_ownership_pct: report.foreign_ownership_pct,
            total_border_conflicts: report.total_border_conflicts,
            total_arbitration_cases: report.total_arbitration_cases,
            total_arbitration_exposure: report.total_arbitration_exposure,
            regional_summaries: report
                .regional_summaries
                .into_iter()
                .map(|s| MinistryRegionalSummaryDTO {
                    region_id: s.region_id,
                    total_hectares: s.total_hectares,
                    total_value: s.total_value,
                    avg_legal_certainty: s.avg_legal_certainty,
                    border_conflicts: s.border_conflicts,
                    foreign_ownership_pct: s.foreign_ownership_pct,
                    court_backlog: s.court_backlog,
                })
                .collect(),
            delay_note: report.delay_note,
        };

        Ok(dto)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
