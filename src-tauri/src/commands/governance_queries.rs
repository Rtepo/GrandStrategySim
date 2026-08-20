use crate::state::AppState;
use sim_engine::ui::snapshot::{GovernanceDetail, BoardMemberRow};
use sim_engine::entities::LegalForm;

/// Phase 55: Get governance detail for a specific company.
/// Returns board members, succession info, and financial metrics.
#[tauri::command]
pub async fn get_governance_detail(
    state: tauri::State<'_, AppState>,
    country: String,
    company_id: String,
) -> Result<Option<GovernanceDetail>, String> {
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
            .map(|e| e.companies.as_slice())
            .unwrap_or(&[]);

        let company = match entities.iter().find(|c| c.id == company_id) {
            Some(c) => c,
            None => return Ok(None),
        };

        let registry = country_ref.politics.vip_registry.as_ref();

        // Resolve board members from VIP registry.
        let (board_members, has_board, board_independence) =
            if let LegalForm::JointStockCompany(ref jsd) = company.legal_form {
                let members: Vec<BoardMemberRow> = jsd.board_members.iter()
                    .filter_map(|seat| {
                        let vip = registry.and_then(|r| r.get(&seat.vip_id))?;
                        Some(BoardMemberRow {
                            vip_id: seat.vip_id.clone(),
                            name: vip.full_name.clone(),
                            role: match seat.role {
                                sim_engine::entities::legal_form::BoardRole::Chair => "Chair".to_string(),
                                sim_engine::entities::legal_form::BoardRole::Founder => "Founder".to_string(),
                                sim_engine::entities::legal_form::BoardRole::Independent => "Independent".to_string(),
                            },
                            loyalty_to_ceo: seat.loyalty_to_ceo,
                            appointed_turn: seat.appointed_turn,
                            age: vip.age,
                            main_trait: vip.main_trait.clone(),
                        })
                    })
                    .collect();
                (members, !jsd.board_members.is_empty(), jsd.board_independence)
            } else {
                (Vec::new(), false, 0.0)
            };

        // Resolve family business succession info.
        let (is_family_business, successor_generation, heir_count, succession_crisis, heirs) =
            if let LegalForm::FamilyBusiness(ref fbd) = company.legal_form {
                let heir_rows: Vec<BoardMemberRow> = fbd.heir_vip_ids.iter()
                    .filter_map(|heir_id| {
                        let vip = registry.and_then(|r| r.get(heir_id))?;
                        Some(BoardMemberRow {
                            vip_id: heir_id.clone(),
                            name: vip.full_name.clone(),
                            role: "Heir".to_string(),
                            loyalty_to_ceo: 0.0,
                            appointed_turn: 0,
                            age: vip.age,
                            main_trait: vip.main_trait.clone(),
                        })
                    })
                    .collect();
                (true, fbd.successor_generation, heir_rows.len() as u32, fbd.succession_crisis, heir_rows)
            } else {
                (false, 0, 0, false, Vec::new())
            };

        let market_cap = company.share_price * company.shares_count as f64;

        Ok(Some(GovernanceDetail {
            company_id: company.id.clone(),
            company_name: company.name.clone(),
            is_listed: company.legal_form.is_listed(),
            shares_count: company.shares_count,
            share_price: company.share_price,
            market_cap,
            eps: company.eps,
            pe_ratio: company.pe_ratio,
            dividend_yield: company.dividend_yield,
            open_price: company.open_price,
            close_price: company.close_price,
            board_members,
            has_board,
            board_independence,
            is_family_business,
            successor_generation,
            heir_count,
            succession_crisis,
            heirs,
        }))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
