//! Espionage system for uncovering corrupt politicians and conducting covert operations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Active espionage operation
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct EspionageOperation {
    /// Operation ID
    #[serde(default)]
    pub id: String,

    /// Target councilor
    #[serde(default)]
    pub target_councilor_id: String,

    /// Budget allocated to operation
    #[serde(default)]
    pub budget: f64,

    /// Operation type
    pub operation_type: EspionageType,

    /// Turn when operation completes
    pub completion_turn: u32,

    /// Success probability (0-1)
    #[serde(default)]
    pub success_probability: f64,
}

/// Type of espionage operation
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EspionageType {
    #[default]
    /// Surveillance to uncover corrupt politicians
    Surveillance,
    /// Direct bribery attempt
    Bribery,
    /// Blackmail using existing material
    Blackmail,
    /// Phase E.10: Industrial espionage — steal a competitor's patented technology.
    IndustrialEspionage,
    /// Phase E.10: Reverse-engineer a product to replicate a patented technology.
    ReverseEngineering,
}

/// State's active espionage operations
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct EspionageState {
    /// Active operations by ID
    #[serde(default)]
    pub active_operations: HashMap<String, EspionageOperation>,

    /// Total espionage budget allocated this turn
    #[serde(default)]
    pub espionage_budget: f64,

    /// Number of successful operations
    #[serde(default)]
    pub successful_operations: u32,

    /// Number of failed operations
    #[serde(default)]
    pub failed_operations: u32,
}

impl EspionageState {
    /// Create a new espionage operation
    ///
    /// # Arguments
    /// * `id` - Unique operation ID
    /// * `target_councilor_id` - ID of target councilor
    /// * `budget` - Budget allocated to operation
    /// * `operation_type` - Type of operation
    /// * `current_turn` - Current game turn
    /// * `target_corruption_level` - Corruption level of target (0-1)
    ///
    /// # Returns
    /// New EspionageOperation with calculated success probability and completion turn
    ///
    /// # Rules
    /// * Surveillance costs 10-50 budget, takes 1-2 turns
    /// * Success probability = budget/100 + target's corruption_level
    /// * On success: reveals Corrupt trait, generates blackmail material
    pub fn create_operation(
        id: String,
        target_councilor_id: String,
        budget: f64,
        operation_type: EspionageType,
        current_turn: u32,
        target_corruption_level: f64,
    ) -> EspionageOperation {
        let (completion_turn, base_success) = match operation_type {
            EspionageType::Surveillance => {
                // 1-2 turns for surveillance
                let turns = if rand::random::<f64>() < 0.5 { 1 } else { 2 };
                (current_turn + turns, budget / 100.0)
            }
            EspionageType::Bribery => {
                // Immediate, but lower base success
                (current_turn, budget / 150.0)
            }
            EspionageType::Blackmail => {
                // Immediate, requires existing material
                (current_turn, 0.8) // High success if material exists
            }
            EspionageType::IndustrialEspionage | EspionageType::ReverseEngineering => {
                // E.10: Corporate IP theft — handled by corporate strategy, not
                // political espionage. These variants exist on the enum for
                // unified serialization but are not created here.
                (current_turn, 0.0)
            }
        };

        let success_probability = (base_success + target_corruption_level).min(1.0);

        EspionageOperation {
            id,
            target_councilor_id,
            budget,
            operation_type,
            completion_turn,
            success_probability,
        }
    }

    /// Process espionage operations for the current turn
    ///
    /// # Arguments
    /// * `current_turn` - Current game turn
    /// * `councilors` - Mutable reference to councilors (to update traits if successful)
    ///
    /// # Returns
    /// Vector of operation result messages
    ///
    /// # Rules
    /// * Operations complete when completion_turn == current_turn
    /// * Success determined by success_probability vs random roll
    /// * Successful surveillance reveals Corrupt trait and generates blackmail material
    /// * Successful bribery/blackmail sways councilor vote
    pub fn process_operations(
        &mut self,
        current_turn: u32,
        councilors: &mut HashMap<String, crate::politics::local_council::Councilor>,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        let mut completed_operations = Vec::new();

        for (id, operation) in &self.active_operations {
            if operation.completion_turn == current_turn {
                completed_operations.push(id.clone());

                let success = rand::random::<f64>() < operation.success_probability;

                if success {
                    self.successful_operations += 1;
                    messages.extend(self.handle_successful_operation(operation, councilors));
                } else {
                    self.failed_operations += 1;
                    messages.push(format!(
                        "[ESPIONAGE] Operation {} failed against councilor {}",
                        operation.id, operation.target_councilor_id
                    ));
                }
            }
        }

        // Remove completed operations
        for id in completed_operations {
            self.active_operations.remove(&id);
        }

        messages
    }

    /// Handle a successful espionage operation
    fn handle_successful_operation(
        &self,
        operation: &EspionageOperation,
        councilors: &mut HashMap<String, crate::politics::local_council::Councilor>,
    ) -> Vec<String> {
        let mut messages = Vec::new();

        if let Some(councilor) = councilors.get_mut(&operation.target_councilor_id) {
            match operation.operation_type {
                EspionageType::Surveillance => {
                    // Reveal Corrupt trait if present
                    if councilor.hidden_trait
                        == crate::politics::local_council::CouncilorTrait::Corrupt
                    {
                        councilor.trait_revealed = true;
                        councilor.blackmail_material = Some(format!(
                            "Compromising material from operation {}",
                            operation.id
                        ));
                        messages.push(format!(
                            "[ESPIONAGE] Councilor {} corruption exposed (compromising material)",
                            councilor.name
                        ));
                    } else {
                        messages.push(format!(
                            "[ESPIONAGE] Surveillance of councilor {} revealed no corruption",
                            councilor.name
                        ));
                    }
                }
                EspionageType::Bribery => {
                    messages.push(format!(
                        "[ESPIONAGE] Successfully bribed councilor {} for {} of budget",
                        councilor.name, operation.budget
                    ));
                }
                EspionageType::Blackmail => {
                    if councilor.blackmail_material.is_some() {
                        messages.push(format!(
                            "[ESPIONAGE] Successfully blackmailed councilor {}",
                            councilor.name
                        ));
                    } else {
                        messages.push(format!(
                            "[ESPIONAGE] Blackmail attempt on councilor {} failed (no material)",
                            councilor.name
                        ));
                    }
                }
                EspionageType::IndustrialEspionage | EspionageType::ReverseEngineering => {
                    // E.10: Corporate IP theft — not applicable to councilor operations.
                }
            }
        }

        messages
    }

    /// Add an operation to the active operations
    pub fn add_operation(&mut self, operation: EspionageOperation) {
        self.active_operations
            .insert(operation.id.clone(), operation);
    }
}
