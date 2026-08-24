use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::registries::enums::Sector;
use crate::entities::Company;
use crate::society::geography::Region;
use crate::politics::system::Party;
use crate::state::treasury::Treasury;

/// Election campaign state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ElectionState {
    #[default]

    Idle,
    

    PreCampaign {
        turns_until_start: u32,
        registration_deadline: u32,
    },
    

    ActiveCampaign {
        turns_remaining: u32,
        current_turn: u32,
    },
    

    ElectionDay,
    

    PostElectionResolution {
        turn: u32,
    },
}

/// Electoral Commission (PKW) - regulatory body monitoring campaigns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ElectoralCommission {
    /// Legal spending limit per party (scaled to GDP)
    #[serde(default)]
    pub spending_limit: f64,
    
    /// Current audit status
    #[serde(default)]
    pub audit_status: AuditStatus,
    
    /// Parties currently under investigation
    #[serde(default)]
    pub parties_under_investigation: Vec<String>,
    
    /// Fines imposed this campaign cycle
    #[serde(default)]
    pub fines_imposed: HashMap<String, f64>,
    
    /// Outstanding debts owed by parties (receivable assets for state)
    #[serde(default)]
    pub outstanding_party_debts: HashMap<String, PartyDebt>,
    
    /// Commission budget (for enforcement)
    #[serde(default)]
    pub commission_budget: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PartyDebt {
    /// Total debt owed to Electoral Commission
    #[serde(default)]
    pub amount: f64,
    
    /// Turn when debt was incurred
    #[serde(default)]
    pub incurrence_turn: u32,
    
    /// Demographic classes liable for debt (member liability)
    #[serde(default)]
    pub liable_classes: Vec<String>,
    
    /// Whether asset liquidation has been triggered
    #[serde(default)]
    pub asset_liquidation_triggered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AuditStatus {
    #[default]

    None,
    

    InProgress {
        target_party: String,
        turns_remaining: u32,
    },
    

    Complete {
        target_party: String,
        findings: AuditFindings,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub enum AuditFindings {

    #[default]
    Clean,
    

    Overspending {
        amount: f64,
        penalty_multiplier: f64,
    },
    

    IllegalFinancing {
        black_money_detected: f64,
        severity: CorruptionSeverity,
    },
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum CorruptionSeverity {
    #[default]

    Low,
    

    Medium,
    

    High,
    

    Catastrophic,
}

/// Campaign action options that parties can execute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CampaignAction {

    NationalAdCampaign {
        cost: f64,
        support_boost: f64,
        mobilization_boost: f64,
        duration_turns: u32,
    },
    

    RegionalRally {
        target_region: String,
        cost: f64,
        support_boost: f64,
        mobilization_boost: f64,
    },
    

    TelevisionCampaign {
        cost: f64,
        support_boost: f64,
        reach_factor: f64,
    },
    

    DigitalCampaign {
        cost: f64,
        support_boost: f64,
        youth_targeting: bool,
    },
    

    CorporateDonors {
        cost: f64,
        support_boost: f64,
        risk_factor: f64,
    },
}

impl Default for CampaignAction {
    fn default() -> Self {
        CampaignAction::NationalAdCampaign {
            cost: 0.0,
            support_boost: 0.0,
            mobilization_boost: 0.0,
            duration_turns: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CampaignExecution {

    pub party_id: String,
    

    pub action: CampaignAction,
    

    pub execution_turn: u32,
    

    pub is_black_money: bool,
    

    pub transaction_recorded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BlackMoneyPool {
    /// Illicit funds not recorded in official treasury
    #[serde(default)]
    pub illicit_funds: f64,
    
    /// Source of black money (for scandal context)
    #[serde(default)]
    pub source: BlackMoneySource,
    
    /// Risk factor for discovery (0-1)
    #[serde(default)]
    pub discovery_risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub enum BlackMoneySource {

    #[default]
    None,
    

    CorporateLobbying {
        company_id: String,
        amount: f64,
    },
    

    OrganizedCrime {
        syndicate_id: String,
        amount: f64,
    },
    

    MoneyLaundering {
        shell_company_id: String,
        amount: f64,
    },
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CampaignError {

    InsufficientFunds,
    

    CompanyNotFound,
    

    TransactionError,
}

/// Calculate legal spending limit per party (scaled to GDP).
pub fn calculate_spending_limit(gdp: f64, population: f64) -> f64 {
    // Base limit: 0.5% of GDP per party
    let base_limit = gdp * 0.005;
    
    // Adjustment for population density (larger countries get proportionally less per capita)
    let population_factor = (population / 1_000_000.0).sqrt();
    
    base_limit / population_factor
}

/// Resolve scandal with receivable asset tracking + subvention garnishing + member liability.
pub fn resolve_scandal(
    party: &mut Party,
    commission: &mut ElectoralCommission,
    treasury: &mut Treasury,
    current_turn: u32,
    severity: CorruptionSeverity,
) -> Vec<String> {
    let mut messages = Vec::new();
    let party_cash = party.liquid_funds();
    
    match severity {
        CorruptionSeverity::Low => {
            let fine = party_cash * 0.5;
            let collected = fine.min(party_cash);
            
            party.brokerage_account.as_mut().unwrap().cash -= collected;
            treasury.liquid_reserves += collected;
            commission.fines_imposed.insert(party.id.clone(), fine);
            
            if collected < fine {
                let unpaid = fine - collected;
                commission.outstanding_party_debts.insert(
                    party.id.clone(),
                    PartyDebt {
                        amount: unpaid,
                        incurrence_turn: current_turn,
                        liable_classes: party.base.clone(),
                        asset_liquidation_triggered: false,
                    }
                );
                messages.push(format!("[SCANDAL] Party {} penalized. Collected: {:.2}, Unpaid: {:.2} (party debt)", party.id, collected, unpaid));
            } else {
                messages.push(format!("[SKANDAL] Partia {} ukarana za nielegalne finansowanie. Kara: {:.2}", party.id, fine));
            }
            
            party.support *= 0.9;
        }
        
        CorruptionSeverity::Medium => {
            let fine = party_cash * 0.8;
            let collected = fine.min(party_cash);
            
            party.brokerage_account.as_mut().unwrap().cash -= collected;
            treasury.liquid_reserves += collected;
            commission.fines_imposed.insert(party.id.clone(), fine);
            
            if collected < fine {
                let unpaid = fine - collected;
                commission.outstanding_party_debts.insert(
                    party.id.clone(),
                    PartyDebt {
                        amount: unpaid,
                        incurrence_turn: current_turn,
                        liable_classes: party.base.clone(),
                        asset_liquidation_triggered: false,
                    }
                );
                messages.push(format!("[SCANDAL] Party {} doomed. Collected: {:.2}, Unpaid: {:.2} (party debt)", party.id, collected, unpaid));
            } else {
                messages.push(format!("[SCANDAL] Party {} engulfed in corruption scandal. Fine: {:.2}", party.id, fine));
            }
            
            party.support *= 0.75;
            party.organization.cohesion *= 0.8;
        }
        
        CorruptionSeverity::High => {
            let fine = party_cash * 1.5;
            let collected = fine.min(party_cash);
            
            party.brokerage_account.as_mut().unwrap().cash -= collected;
            treasury.liquid_reserves += collected;
            commission.fines_imposed.insert(party.id.clone(), fine);
            
            if collected < fine {
                let unpaid = fine - collected;
                commission.outstanding_party_debts.insert(
                    party.id.clone(),
                    PartyDebt {
                        amount: unpaid,
                        incurrence_turn: current_turn,
                        liable_classes: party.base.clone(),
                        asset_liquidation_triggered: false,
                    }
                );
                messages.push(format!("[SCANDAL] Party {} severely penalized. Collected: {:.2}, Unpaid: {:.2} (party debt)", party.id, collected, unpaid));
            } else {
                messages.push(format!("[SCANDAL] Party {} severely penalized for systemic corruption. Fine: {:.2}", party.id, fine));
            }
            
            party.support *= 0.5;
            party.organization.cohesion *= 0.5;
        }
        
        CorruptionSeverity::Catastrophic => {
            let collected = party_cash;
            
            party.brokerage_account.as_mut().unwrap().cash = 0.0;
            treasury.liquid_reserves += collected;
            commission.fines_imposed.insert(party.id.clone(), party_cash);
            
            party.support = 0.0;
            party.organization.cohesion = 0.0;
            messages.push(format!("[CATASTROPHE] Party {} dissolved for rampant corruption. Confiscated: {:.2}. Leader arrested.", party.id, collected));
        }
    }
    
    messages
}

/// Garnish party subventions until debt cleared.
pub fn garnish_party_subventions(
    party: &mut Party,
    commission: &mut ElectoralCommission,
    treasury: &mut Treasury,
    annual_subvention: f64,
) -> f64 {
    if let Some(debt) = commission.outstanding_party_debts.get_mut(&party.id) {
        if debt.amount > 0.0 {
            let garnished = annual_subvention.min(debt.amount);
            debt.amount -= garnished;
            treasury.liquid_reserves += garnished;
            
            if debt.amount <= 0.01 {
                commission.outstanding_party_debts.remove(&party.id);
            }
            
            return garnished;
        }
    }
    
    0.0
}

/// Process party debt liquidation with member liability (double-entry sealed).
pub fn process_party_debt_liquidation(
    party: &mut Party,
    commission: &mut ElectoralCommission,
    treasury: &mut Treasury,
    regions: &mut [Region],
    _current_turn: u32,
    annual_subvention_threshold: f64,
) -> Vec<String> {
    let mut messages = Vec::new();
    
    if let Some(debt) = commission.outstanding_party_debts.get_mut(&party.id) {
        if debt.amount > annual_subvention_threshold && !debt.asset_liquidation_triggered {
            debt.asset_liquidation_triggered = true;
            
            let asset_value = party.liquid_funds() * 0.2;
            party.brokerage_account.as_mut().unwrap().cash -= asset_value;
            
            treasury.liquid_reserves += asset_value;
            debt.amount -= asset_value;
            
            messages.push(format!("[LIQUIDATION] Party {} assets liquidated. Value: {:.2}", party.id, asset_value));
            
            if debt.amount > 0.0 {
                let remaining_debt = debt.amount;
                let total_liable_population = debt.liable_classes.iter()
                    .flat_map(|class_name| {
                        regions.iter()
                            .flat_map(|r| {
                                r.class_demographics.rural_classes.get(class_name)
                                    .into_iter()
                                    .chain(r.class_demographics.urban_classes.get(class_name))
                            })
                            .map(|c| c.population as f64)
                    })
                    .sum::<f64>()
                    .max(1.0);
                
                let mut total_extracted = 0.0;
                
                for class_name in &debt.liable_classes {
                    for region in regions.iter_mut() {
                        if let Some(class_data) = region.class_demographics.rural_classes.get_mut(class_name) {
                            let class_share = (class_data.population as f64 / total_liable_population) * remaining_debt;
                            let extracted = class_share.min(class_data.savings);
                            class_data.savings -= extracted;
                            treasury.liquid_reserves += extracted;
                            total_extracted += extracted;
                        }
                        if let Some(class_data) = region.class_demographics.urban_classes.get_mut(class_name) {
                            let class_share = (class_data.population as f64 / total_liable_population) * remaining_debt;
                            let extracted = class_share.min(class_data.savings);
                            class_data.savings -= extracted;
                            treasury.liquid_reserves += extracted;
                            total_extracted += extracted;
                        }
                    }
                }
                
                let uncollectible = remaining_debt - total_extracted;
                
                if uncollectible > 0.01 {
                    messages.push(format!("[LOSS] Uncollectible party {} debt written off as loss: {:.2}", party.id, uncollectible));
                }
                
                messages.push(format!("[ACCOUNTABILITY] Party {} members paid: {:.2}, Loss: {:.2}", party.id, total_extracted, uncollectible));
                debt.amount = 0.0;
                commission.outstanding_party_debts.remove(&party.id);
            }
        }
    }
    
    messages
}

/// Execute national ad campaign with Ultimate Macroeconomic Sink.
pub fn execute_national_ad_campaign(
    party: &mut Party,
    companies: &mut Vec<Company>,
    regions: &mut [Region],
    cost: f64,
) -> Result<(), CampaignError> {
    party.brokerage_account.as_mut().unwrap().cash -= cost;
    
    let media_share = cost * 0.7;
    let media_count = companies.iter()
        .filter(|c| c.sector == Sector::MediaAndEntertainment)
        .count();
    
    if media_count > 0 {
        let per_company = media_share / media_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::MediaAndEntertainment {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let local_services_count = companies.iter()
            .filter(|c| c.sector == Sector::LocalServices)
            .count();
        
        if local_services_count > 0 {
            let per_company = media_share / local_services_count as f64;
            for company in companies.iter_mut() {
                if company.sector == Sector::LocalServices {
                    if let Some(ref mut account) = company.brokerage_account {
                        account.cash += per_company;
                    }
                }
            }
        } else {
            let total_population = regions.iter()
                .flat_map(|r| {
                    r.class_demographics.rural_classes.values()
                        .chain(r.class_demographics.urban_classes.values())
                })
                .map(|c| c.population as f64)
                .sum::<f64>()
                .max(1.0);
            
            for region in regions.iter_mut() {
                for class_data in region.class_demographics.rural_classes.values_mut() {
                    let per_capita = media_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
                for class_data in region.class_demographics.urban_classes.values_mut() {
                    let per_capita = media_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
            }
        }
    }
    
    let local_services_share = cost * 0.2;
    let local_services_count = companies.iter()
        .filter(|c| c.sector == Sector::LocalServices)
        .count();
    
    if local_services_count > 0 {
        let per_company = local_services_share / local_services_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::LocalServices {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let total_population = regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for region in regions.iter_mut() {
            for class_data in region.class_demographics.rural_classes.values_mut() {
                let per_capita = local_services_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
            for class_data in region.class_demographics.urban_classes.values_mut() {
                let per_capita = local_services_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
        }
    }
    
    let agency_share = cost * 0.1;
    if local_services_count > 0 {
        let per_company = agency_share / local_services_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::LocalServices {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let total_population = regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for region in regions.iter_mut() {
            for class_data in region.class_demographics.rural_classes.values_mut() {
                let per_capita = agency_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
            for class_data in region.class_demographics.urban_classes.values_mut() {
                let per_capita = agency_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
        }
    }
    
    Ok(())
}

/// Execute regional rally with Ultimate Macroeconomic Sink.
pub fn execute_regional_rally(
    party: &mut Party,
    region: &mut Region,
    companies: &mut Vec<Company>,
    cost: f64,
) -> Result<(), CampaignError> {
    party.brokerage_account.as_mut().unwrap().cash -= cost;
    
    let local_share = cost * 0.5;
    let regional_local_count = companies.iter()
        .filter(|c| c.region_id == region.id && c.sector == Sector::LocalServices)
        .count();
    
    if regional_local_count > 0 {
        let per_company = local_share / regional_local_count as f64;
        for company in companies.iter_mut() {
            if company.region_id == region.id && company.sector == Sector::LocalServices {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let fallback_to_demographics = local_share;
        let total_population = region.class_demographics.rural_classes.values()
            .chain(region.class_demographics.urban_classes.values())
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for class_data in region.class_demographics.rural_classes.values_mut() {
            let per_capita = fallback_to_demographics / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
        for class_data in region.class_demographics.urban_classes.values_mut() {
            let per_capita = fallback_to_demographics / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
    }
    
    let class_share = cost * 0.3;
    let total_population = region.class_demographics.rural_classes.values()
        .chain(region.class_demographics.urban_classes.values())
        .map(|c| c.population as f64)
        .sum::<f64>()
        .max(1.0);
    
    for class_data in region.class_demographics.rural_classes.values_mut() {
        let per_capita = class_share / total_population;
        class_data.savings += per_capita * class_data.population as f64;
    }
    for class_data in region.class_demographics.urban_classes.values_mut() {
        let per_capita = class_share / total_population;
        class_data.savings += per_capita * class_data.population as f64;
    }
    
    let construction_share = cost * 0.2;
    let regional_construction_count = companies.iter()
        .filter(|c| c.region_id == region.id && c.sector == Sector::Construction)
        .count();
    
    if regional_construction_count > 0 {
        let per_company = construction_share / regional_construction_count as f64;
        for company in companies.iter_mut() {
            if company.region_id == region.id && company.sector == Sector::Construction {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let fallback_to_demographics = construction_share;
        for class_data in region.class_demographics.rural_classes.values_mut() {
            let per_capita = fallback_to_demographics / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
        for class_data in region.class_demographics.urban_classes.values_mut() {
            let per_capita = fallback_to_demographics / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
    }
    
    Ok(())
}

/// Execute television campaign with Ultimate Macroeconomic Sink.
pub fn execute_television_campaign(
    party: &mut Party,
    companies: &mut Vec<Company>,
    regions: &mut [Region],
    cost: f64,
) -> Result<(), CampaignError> {
    party.brokerage_account.as_mut().unwrap().cash -= cost;
    
    let broadcasting_share = cost * 0.7;
    let media_count = companies.iter()
        .filter(|c| c.sector == Sector::MediaAndEntertainment)
        .count();
    
    if media_count > 0 {
        let per_company = broadcasting_share / media_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::MediaAndEntertainment {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let export_count = companies.iter()
            .filter(|c| c.sector == Sector::ExportServices)
            .count();
        
        if export_count > 0 {
            let per_company = broadcasting_share / export_count as f64;
            for company in companies.iter_mut() {
                if company.sector == Sector::ExportServices {
                    if let Some(ref mut account) = company.brokerage_account {
                        account.cash += per_company;
                    }
                }
            }
        } else {
            let total_population = regions.iter()
                .flat_map(|r| {
                    r.class_demographics.rural_classes.values()
                        .chain(r.class_demographics.urban_classes.values())
                })
                .map(|c| c.population as f64)
                .sum::<f64>()
                .max(1.0);
            
            for region in regions.iter_mut() {
                for class_data in region.class_demographics.rural_classes.values_mut() {
                    let per_capita = broadcasting_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
                for class_data in region.class_demographics.urban_classes.values_mut() {
                    let per_capita = broadcasting_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
            }
        }
    }
    
    let production_share = cost * 0.2;
    if media_count > 0 {
        let per_company = production_share / media_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::MediaAndEntertainment {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let total_population = regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for region in regions.iter_mut() {
            for class_data in region.class_demographics.rural_classes.values_mut() {
                let per_capita = production_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
            for class_data in region.class_demographics.urban_classes.values_mut() {
                let per_capita = production_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
        }
    }
    
    let talent_share = cost * 0.1;
    let total_population = regions.iter()
        .flat_map(|r| {
            r.class_demographics.rural_classes.values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.population as f64)
        .sum::<f64>()
        .max(1.0);
    
    for region in regions.iter_mut() {
        for class_data in region.class_demographics.rural_classes.values_mut() {
            let per_capita = talent_share / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
        for class_data in region.class_demographics.urban_classes.values_mut() {
            let per_capita = talent_share / total_population;
            class_data.savings += per_capita * class_data.population as f64;
        }
    }
    
    Ok(())
}

/// Execute digital campaign with Ultimate Macroeconomic Sink.
pub fn execute_digital_campaign(
    party: &mut Party,
    companies: &mut Vec<Company>,
    regions: &mut [Region],
    cost: f64,
) -> Result<(), CampaignError> {
    party.brokerage_account.as_mut().unwrap().cash -= cost;
    
    let tech_share = cost * 0.6;
    let media_count = companies.iter()
        .filter(|c| c.sector == Sector::MediaAndEntertainment)
        .count();
    
    if media_count > 0 {
        let per_company = tech_share / media_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::MediaAndEntertainment {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let export_count = companies.iter()
            .filter(|c| c.sector == Sector::ExportServices)
            .count();
        
        if export_count > 0 {
            let per_company = tech_share / export_count as f64;
            for company in companies.iter_mut() {
                if company.sector == Sector::ExportServices {
                    if let Some(ref mut account) = company.brokerage_account {
                        account.cash += per_company;
                    }
                }
            }
        } else {
            let total_population = regions.iter()
                .flat_map(|r| {
                    r.class_demographics.rural_classes.values()
                        .chain(r.class_demographics.urban_classes.values())
                })
                .map(|c| c.population as f64)
                .sum::<f64>()
                .max(1.0);
            
            for region in regions.iter_mut() {
                for class_data in region.class_demographics.rural_classes.values_mut() {
                    let per_capita = tech_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
                for class_data in region.class_demographics.urban_classes.values_mut() {
                    let per_capita = tech_share / total_population;
                    class_data.savings += per_capita * class_data.population as f64;
                }
            }
        }
    }
    
    let social_share = cost * 0.3;
    if media_count > 0 {
        let per_company = social_share / media_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::MediaAndEntertainment {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let total_population = regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for region in regions.iter_mut() {
            for class_data in region.class_demographics.rural_classes.values_mut() {
                let per_capita = social_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
            for class_data in region.class_demographics.urban_classes.values_mut() {
                let per_capita = social_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
        }
    }
    
    let marketing_share = cost * 0.1;
    let local_count = companies.iter()
        .filter(|c| c.sector == Sector::LocalServices)
        .count();
    
    if local_count > 0 {
        let per_company = marketing_share / local_count as f64;
        for company in companies.iter_mut() {
            if company.sector == Sector::LocalServices {
                if let Some(ref mut account) = company.brokerage_account {
                    account.cash += per_company;
                }
            }
        }
    } else {
        let total_population = regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);
        
        for region in regions.iter_mut() {
            for class_data in region.class_demographics.rural_classes.values_mut() {
                let per_capita = marketing_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
            for class_data in region.class_demographics.urban_classes.values_mut() {
                let per_capita = marketing_share / total_population;
                class_data.savings += per_capita * class_data.population as f64;
            }
        }
    }
    
    Ok(())
}

/// Generate corporate lobbying black money with transactional origination.
pub fn generate_corporate_lobbying_black_money(
    party: &mut Party,
    companies: &mut Vec<Company>,
    company_id: &str,
    amount: f64,
) -> Result<(), CampaignError> {
    let company = companies.iter_mut()
        .find(|c| c.id == company_id)
        .ok_or(CampaignError::CompanyNotFound)?;
    
    let company_cash = company.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    if company_cash < amount {
        return Err(CampaignError::InsufficientFunds);
    }
    
    company.brokerage_account.as_mut().unwrap().cash -= amount;
    
    if party.black_money_pool.is_none() {
        party.black_money_pool = Some(BlackMoneyPool {
            illicit_funds: 0.0,
            source: BlackMoneySource::None,
            discovery_risk: 0.0,
        });
    }
    
    let pool = party.black_money_pool.as_mut().unwrap();
    pool.illicit_funds += amount;
    pool.source = BlackMoneySource::CorporateLobbying {
        company_id: company_id.to_string(),
        amount,
    };
    
    pool.discovery_risk = (amount / company_cash).min(0.8);
    
    Ok(())
}

/// Generate organized crime black money with transactional origination.
pub fn generate_organized_crime_black_money(
    party: &mut Party,
    region: &mut Region,
    syndicate_id: &str,
    amount: f64,
) -> Result<(), CampaignError> {
    let total_savings = region.class_demographics.rural_classes.values()
        .map(|c| c.savings)
        .sum::<f64>();
    
    if total_savings < amount {
        return Err(CampaignError::InsufficientFunds);
    }
    
    let deduction_ratio = amount / total_savings;
    for class_data in region.class_demographics.rural_classes.values_mut() {
        class_data.savings *= 1.0 - deduction_ratio ;
    }
    
    if party.black_money_pool.is_none() {
        party.black_money_pool = Some(BlackMoneyPool {
            illicit_funds: 0.0,
            source: BlackMoneySource::None,
            discovery_risk: 0.0,
        });
    }
    
    let pool = party.black_money_pool.as_mut().unwrap();
    pool.illicit_funds += amount;
    pool.source = BlackMoneySource::OrganizedCrime {
        syndicate_id: syndicate_id.to_string(),
        amount,
    };
    
    pool.discovery_risk = 0.6;
    
    Ok(())
}

/// Generate money laundering black money with transactional origination.
pub fn generate_money_laundering_black_money(
    party: &mut Party,
    companies: &mut Vec<Company>,
    shell_company_id: &str,
    amount: f64,
) -> Result<(), CampaignError> {
    let company = companies.iter_mut()
        .find(|c| c.id == shell_company_id)
        .ok_or(CampaignError::CompanyNotFound)?;
    
    let company_cash = company.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    if company_cash < amount {
        return Err(CampaignError::InsufficientFunds);
    }
    
    company.brokerage_account.as_mut().unwrap().cash -= amount;
    
    if party.black_money_pool.is_none() {
        party.black_money_pool = Some(BlackMoneyPool {
            illicit_funds: 0.0,
            source: BlackMoneySource::None,
            discovery_risk: 0.0,
        });
    }
    
    let pool = party.black_money_pool.as_mut().unwrap();
    pool.illicit_funds += amount;
    pool.source = BlackMoneySource::MoneyLaundering {
        shell_company_id: shell_company_id.to_string(),
        amount,
    };
    
    pool.discovery_risk = 0.4;
    
    Ok(())
}

/// Process election cycle — advance state machine, trigger campaigns and elections.
///
/// # Arguments
/// * `country` - Mutable country (for politics.election_state)
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Advances ElectionState: Idle → PreCampaign → ActiveCampaign → ElectionDay → PostElectionResolution → Idle
/// * On ElectionDay: seats are recalculated, coalition formed, ruling party set
/// * On PostElectionResolution: committee system initialized
pub fn process_election_cycle(
    country: &mut crate::state::Country,
    parties: &mut HashMap<String, Party>,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    match &country.politics.election_state {
        ElectionState::Idle => {
            if current_turn > 0 && current_turn.is_multiple_of(12) {
                country.politics.election_state = ElectionState::PreCampaign {
                    turns_until_start: country.politics.campaign_duration_turns.max(1),
                    registration_deadline: current_turn + 2,
                };
                messages.push("[ELECTION] Pre-campaign period begins".to_string());
            }
        }
        ElectionState::PreCampaign { turns_until_start, registration_deadline } => {
            let mut turns = *turns_until_start;
            if turns <= 1 {
                country.politics.election_state = ElectionState::ActiveCampaign {
                    turns_remaining: country.politics.campaign_duration_turns.max(1),
                    current_turn,
                };
                messages.push("[ELECTION] Campaign is now active".to_string());
            } else {
                turns -= 1;
                country.politics.election_state = ElectionState::PreCampaign {
                    turns_until_start: turns,
                    registration_deadline: *registration_deadline,
                };
            }
        }
        ElectionState::ActiveCampaign { turns_remaining, current_turn: _ } => {
            let mut remaining = *turns_remaining;
            if remaining <= 1 {
                country.politics.election_state = ElectionState::ElectionDay;
                messages.push("[ELECTION] Election day!".to_string());
            } else {
                remaining -= 1;
                country.politics.election_state = ElectionState::ActiveCampaign {
                    turns_remaining: remaining,
                    current_turn,
                };
            }
        }
        ElectionState::ElectionDay => {
            // Calculate seats from party support
            let party_list: Vec<(String, f64)> = parties.iter()
                .map(|(id, p)| (id.clone(), p.support))
                .collect();

            if !party_list.is_empty() {
                let seats = crate::politics::elections::calculate_seats(
                    parties,
                    &country.politics.election_method,
                    country.politics.election_threshold,
                    460,
                );
                messages.push(format!("[ELECTION] Seats allocated: {} parties", seats.len()));

                // Form coalition (simplified: largest party rules)
                if let Some((winner_id, _)) = party_list.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) {
                    country.politics.ruling_party = winner_id.clone();
                    messages.push(format!("[ELECTION] Ruling party: {}", winner_id));
                }
            }

            country.politics.election_state = ElectionState::PostElectionResolution { turn: current_turn };
        }
        ElectionState::PostElectionResolution { turn: _ } => {
            country.politics.election_state = ElectionState::Idle;
            messages.push("[ELECTION] Post-election resolution complete".to_string());
        }
    }

    messages
}

/// Process campaign spending — execute campaign actions for active campaign state.
///
/// # Arguments
/// * `country` - Mutable country
/// * `parties` - Mutable parties
/// * `companies` - Mutable companies
/// * `regions` - Mutable regions
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Only executes during ActiveCampaign state.
/// * Each party with sufficient funds runs a national ad campaign.
/// * Double-entry: Party brokerage cash → Media companies + Local services + Citizen savings.
pub fn process_campaign_spending(
    country: &mut crate::state::Country,
    parties: &mut HashMap<String, Party>,
    companies: &mut Vec<Company>,
    regions: &mut [Region],
    _current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    if !matches!(country.politics.election_state, ElectionState::ActiveCampaign { .. }) {
        return messages;
    }

    for (party_id, party) in parties.iter_mut() {
        let party_cash = party.brokerage_account.as_ref().map(|a| a.cash).unwrap_or(0.0);
        if party_cash < 1000.0 {
            continue;
        }

        let spend_amount = party_cash * 0.1; // Spend 10% of war chest per turn
        if let Err(e) = execute_national_ad_campaign(party, companies, regions, spend_amount) {
            messages.push(format!("[CAMPAIGN] {} campaign error: {:?}", party_id, e));
        } else {
            messages.push(format!("[CAMPAIGN] {} spent {:.0} on ads", party_id, spend_amount));
        }
    }

    messages
}
