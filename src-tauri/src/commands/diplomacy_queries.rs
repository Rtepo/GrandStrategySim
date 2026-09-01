use crate::state::AppState;
use sim_engine::international::sanctions::{Sanction, SanctionType};
use sim_engine::international::treaties::{Treaty, TreatyClause};
use sim_engine::politics::vip_registry::DiplomaticPostType;
use sim_engine::state::diplomatic_actions::DiplomaticAction;
use sim_engine::ui::snapshot::{
    DiplomacySnapshot, ForeignCountryRow, InternationalOrgRow, SanctionRow, TreatyRow,
};

/// Phase 66: Get the diplomacy snapshot for the player's country.
/// Returns bilateral relations, posted diplomats, and foreign intelligence.
#[tauri::command]
pub async fn get_diplomacy_snapshot(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<DiplomacySnapshot, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let snapshot = sim_engine::ui::snapshot::build_diplomacy_snapshot(
            &engine_state.game_state,
            &country,
            &engine_state.turn_context.diplomacy,
        );
        Ok(snapshot)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Get foreign country rows with Fog of War applied.
/// The player country gets full data; all others get filtered by intel level.
#[tauri::command]
pub async fn get_foreign_countries(
    state: tauri::State<'_, AppState>,
    player_country: String,
) -> Result<Vec<ForeignCountryRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let rows = sim_engine::ui::snapshot::build_foreign_country_rows(
            &engine_state.game_state,
            &player_country,
            &engine_state.turn_context.diplomacy,
        );
        Ok(rows)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Assign a diplomat to a foreign post.
/// Debits the home country's liquid_reserves (diplomat_assignment_cost).
#[tauri::command]
pub async fn assign_diplomat(
    state: tauri::State<'_, AppState>,
    vip_id: String,
    home_country: String,
    host_country: String,
    post_type: String,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let pt = match post_type.as_str() {
            "Ambassador" => DiplomaticPostType::Ambassador,
            "Consul" => DiplomaticPostType::Consul,
            "Spy" => DiplomaticPostType::Spy,
            "MilitaryAttache" => DiplomaticPostType::MilitaryAttache,
            _ => return Err(format!("Unknown post type: {}", post_type)),
        };

        let current_turn = engine_state.game_state.calendar.global_turn;
        engine_state
            .game_state
            .pending_diplomatic_actions
            .push(DiplomaticAction::AssignDiplomat {
                vip_id,
                home_country,
                host_country,
                post_type: pt,
                assigned_turn: current_turn,
            });

        // Drain immediately for responsive UI
        let config = engine_state.game_state.diplomatic_config.clone();
        sim_engine::state::diplomatic_actions::drain_diplomatic_actions(
            &mut engine_state.game_state,
            &config,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Recall a diplomat from their foreign post.
#[tauri::command]
pub async fn recall_diplomat(
    state: tauri::State<'_, AppState>,
    vip_id: String,
    home_country: String,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        engine_state
            .game_state
            .pending_diplomatic_actions
            .push(DiplomaticAction::RecallDiplomat {
                vip_id,
                home_country,
            });

        let config = engine_state.game_state.diplomatic_config.clone();
        sim_engine::state::diplomatic_actions::drain_diplomatic_actions(
            &mut engine_state.game_state,
            &config,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Expel a diplomat from the player's country (Persona non grata).
#[tauri::command]
pub async fn expel_diplomat(
    state: tauri::State<'_, AppState>,
    host_country: String,
    home_country: String,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        engine_state
            .game_state
            .pending_diplomatic_actions
            .push(DiplomaticAction::ExpelDiplomat {
                home_country,
                host_country,
            });

        let config = engine_state.game_state.diplomatic_config.clone();
        sim_engine::state::diplomatic_actions::drain_diplomatic_actions(
            &mut engine_state.game_state,
            &config,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Send economic aid to another country.
/// Debits sender's liquid_reserves, credits receiver's liquid_reserves.
#[tauri::command]
pub async fn send_economic_aid(
    state: tauri::State<'_, AppState>,
    from_country: String,
    to_country: String,
    amount: f64,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        engine_state.game_state.pending_diplomatic_actions.push(
            DiplomaticAction::SendEconomicAid {
                from_country,
                to_country,
                amount,
            },
        );

        let config = engine_state.game_state.diplomatic_config.clone();
        sim_engine::state::diplomatic_actions::drain_diplomatic_actions(
            &mut engine_state.game_state,
            &config,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 66: Initiate a border provocation against another country.
/// Damages relations and may freeze them.
#[tauri::command]
pub async fn border_provocation(
    state: tauri::State<'_, AppState>,
    from_country: String,
    to_country: String,
    intensity: f64,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        engine_state.game_state.pending_diplomatic_actions.push(
            DiplomaticAction::BorderProvocation {
                from_country,
                to_country,
                intensity,
            },
        );

        let config = engine_state.game_state.diplomatic_config.clone();
        sim_engine::state::diplomatic_actions::drain_diplomatic_actions(
            &mut engine_state.game_state,
            &config,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 67: Get all active and pending treaties involving a country.
#[tauri::command]
pub async fn get_active_treaties(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<TreatyRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let treaties: Vec<TreatyRow> = engine_state
            .game_state
            .treaty_registry
            .treaties_for_country(&country)
            .into_iter()
            .map(|t| TreatyRow {
                id: t.id.clone(),
                name: t.name.clone(),
                status: t.status.as_str().to_string(),
                participants: t.participants.clone(),
                clauses: t.clauses.iter().map(|c| c.as_str().to_string()).collect(),
                negotiation_progress: t.negotiation_progress,
                diplomatic_capacity_cost: t.diplomatic_capacity_cost,
                initiated_turn: t.initiated_turn,
                signed_turn: t.signed_turn,
                duration_turns: t.duration_turns,
                initiator: t.initiator.clone(),
            })
            .collect();
        Ok(treaties)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 67: Propose a new treaty between countries.
#[tauri::command]
pub async fn propose_treaty(
    state: tauri::State<'_, AppState>,
    name: String,
    participants: Vec<String>,
    clause_labels: Vec<String>,
    initiator: String,
    duration_turns: u32,
) -> Result<String, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let clauses: Vec<TreatyClause> = clause_labels
            .into_iter()
            .map(|label| match label.as_str() {
                "CustomsUnion" => TreatyClause::CustomsUnion,
                "SchengenFreeMovement" => TreatyClause::SchengenFreeMovement,
                "FinancialMarketIntegration" => TreatyClause::FinancialMarketIntegration,
                "MutualDefense" => TreatyClause::MutualDefense,
                "TradePreference" => TreatyClause::TradePreference,
                other => TreatyClause::ResourceAccess {
                    commodity: other.to_string(),
                },
            })
            .collect();

        let current_turn = engine_state.game_state.calendar.global_turn;
        let treaty_id = engine_state.game_state.treaty_registry.next_treaty_id();
        let treaty = Treaty::new(
            treaty_id.clone(),
            name,
            participants,
            clauses,
            current_turn,
            duration_turns,
            initiator,
        );
        engine_state
            .game_state
            .treaty_registry
            .treaties
            .push(treaty);
        Ok(treaty_id)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 67: Sign a treaty (move from Proposed/Negotiating to Active).
#[tauri::command]
pub async fn sign_treaty(
    state: tauri::State<'_, AppState>,
    treaty_id: String,
) -> Result<bool, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let current_turn = engine_state.game_state.calendar.global_turn;
        let config = engine_state.game_state.treaty_config.clone();
        let result =
            engine_state
                .game_state
                .treaty_registry
                .sign_treaty(&treaty_id, current_turn, &config);
        Ok(result)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 67: Abrogate a treaty unilaterally — triggers reputation penalty.
#[tauri::command]
pub async fn abrogate_treaty(
    state: tauri::State<'_, AppState>,
    treaty_id: String,
    abrogating_country: String,
) -> Result<bool, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let abrogated = engine_state
            .game_state
            .treaty_registry
            .abrogate_treaty(&treaty_id);
        if let Some(treaty) = abrogated {
            let rep_config = engine_state.game_state.reputation_config.clone();
            let current_turn = engine_state.game_state.calendar.global_turn;
            if let Some(country) = engine_state
                .game_state
                .countries
                .get_mut(&abrogating_country)
            {
                country.global_reputation.apply_violation(
                    sim_engine::international::reputation::TreatyViolation {
                        treaty_id: treaty.id.clone(),
                        turn: current_turn,
                        severity: 1.0,
                        description: format!("Unilateral abrogation of treaty '{}'", treaty.name),
                    },
                    &rep_config,
                );
            }
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 68: Get all international organizations a country belongs to.
#[tauri::command]
pub async fn get_organizations(
    state: tauri::State<'_, AppState>,
    country: String,
) -> Result<Vec<InternationalOrgRow>, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let engine_guard = state_clone.engine.blocking_read();
        let engine_state = engine_guard.as_ref().ok_or("No game loaded")?;

        let current_turn = engine_state.game_state.calendar.global_turn;
        let orgs: Vec<InternationalOrgRow> = engine_state
            .game_state
            .international_organizations
            .orgs_for_country(&country)
            .into_iter()
            .map(|org| {
                let org_sanctions: Vec<SanctionRow> = engine_state
                    .game_state
                    .active_sanctions
                    .sanctions
                    .iter()
                    .filter(|s| s.sanctioning_org == org.id && s.is_active_at(current_turn))
                    .map(|s| SanctionRow {
                        id: s.id.clone(),
                        target_country: s.target_country.clone(),
                        sanctioning_org: org.name.clone(),
                        sanction_type: s.sanction_type.as_str().to_string(),
                        enacted_turn: s.enacted_turn,
                        duration_turns: s.duration_turns,
                        reason: s.reason.clone(),
                        is_active: s.is_active_at(current_turn),
                    })
                    .collect();
                InternationalOrgRow {
                    id: org.id.clone(),
                    name: org.name.clone(),
                    integration_level: org.integration_level.as_str().to_string(),
                    voting_mechanism: org.voting_mechanism.as_str().to_string(),
                    member_states: org.member_states.clone(),
                    directive_count: org.directives.len(),
                    founded_turn: org.founded_turn,
                    sanctions: org_sanctions,
                }
            })
            .collect();
        Ok(orgs)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 68: Propose a sanction against a country via organization vote.
#[tauri::command]
pub async fn propose_sanction(
    state: tauri::State<'_, AppState>,
    org_id: String,
    target_country: String,
    sanction_type_label: String,
    reason: String,
    duration_turns: u32,
) -> Result<String, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let sanction_type = match sanction_type_label.as_str() {
            "TradeEmbargo" => SanctionType::TradeEmbargo,
            "AssetFreeze" => SanctionType::AssetFreeze,
            "FinancialIsolation" => SanctionType::FinancialIsolation,
            "FullEmbargo" => SanctionType::FullEmbargo,
            _ => SanctionType::TradeEmbargo,
        };

        let current_turn = engine_state.game_state.calendar.global_turn;
        let sanction_id = engine_state.game_state.active_sanctions.next_sanction_id();
        let duration = if duration_turns > 0 {
            duration_turns
        } else {
            engine_state
                .game_state
                .sanction_config
                .default_duration_turns
        };

        let sanction = Sanction::new(
            sanction_id.clone(),
            target_country,
            org_id,
            sanction_type,
            current_turn,
            duration,
            reason,
        );
        engine_state
            .game_state
            .active_sanctions
            .enact_sanction(sanction);
        Ok(sanction_id)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Phase 68: Lift a sanction by ID.
#[tauri::command]
pub async fn lift_sanction(
    state: tauri::State<'_, AppState>,
    sanction_id: String,
) -> Result<bool, String> {
    let state_clone = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut engine_guard = state_clone.engine.blocking_write();
        let engine_state = engine_guard.as_mut().ok_or("No game loaded")?;

        let result = engine_state
            .game_state
            .active_sanctions
            .lift_sanction(&sanction_id);
        Ok(result)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
