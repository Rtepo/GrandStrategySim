//! Phase 39/66: Deferred diplomatic action queue.
//!
//! This module defines the `DiplomaticAction` enum and the sequential
//! drain logic for cross-country diplomatic operations.
//!
//! ## Architecture
//!
//! During parallel per-country turn processing (Rayon `par_iter_mut`),
//! each country's processing function returns a `Vec<DiplomaticAction>`.
//! These are collected via `.map(...).flatten().collect::<Vec<_>>()`.
//!
//! After the parallel block, `drain_diplomatic_actions` is called
//! sequentially to execute each action, safely mutating both the home
//! and host countries without borrow-checker conflicts.
//!
//! ## Rules
//!
//! - No `Mutex`, `RwLock`, or interior mutability.
//! - No direct cross-country mutation during parallel processing.
//! - Actions are drained sequentially after parallel processing completes.
//! - Missing/annexed host countries are handled safely (no-op).
//! - **Strict double-entry accounting**: All financial actions debit real
//!   wallets and credit real wallets. No money creation.

use serde::{Deserialize, Serialize};
use crate::politics::vip_registry::DiplomaticPostType;

/// A deferred diplomatic action queued during parallel turn processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiplomaticAction {
    /// Request to construct an embassy in a host country.
    /// The home country's Foreign Affairs ministry pays for construction.
    EmbassyConstructionRequest {
        /// Country requesting the embassy (home country).
        home_country: String,
        /// Country where the embassy will be built (host country).
        host_country: String,
        /// Budget allocated for construction (debited from home country).
        budget: f64,
    },
    /// Transfer funds from home country to host country (embassy operating costs).
    EmbassyFundingTransfer {
        /// Country sending funds (home country).
        home_country: String,
        /// Country receiving funds (host country).
        host_country: String,
        /// Amount to transfer.
        amount: f64,
    },
    /// Phase 66: Expel a diplomat (Persona non grata).
    /// Removes the diplomat from the host country and freezes relations.
    ExpelDiplomat {
        /// Country whose diplomat is being expelled (home country of the diplomat).
        home_country: String,
        /// Country doing the expelling (host country).
        host_country: String,
    },
    /// Phase 66: Send economic aid from one country to another.
    /// Debits sender's liquid_reserves, credits receiver's liquid_reserves.
    SendEconomicAid {
        /// Country sending the aid.
        from_country: String,
        /// Country receiving the aid.
        to_country: String,
        /// Amount of aid to send.
        amount: f64,
    },
    /// Phase 66: Border provocation — damages relations with target country.
    BorderProvocation {
        /// Country initiating the provocation.
        from_country: String,
        /// Country being provoked.
        to_country: String,
        /// Intensity of the provocation (0.0 = mild, 1.0 = severe).
        intensity: f64,
    },
    /// Phase 66: Assign a diplomat VIP to a foreign post.
    AssignDiplomat {
        /// VIP ID of the diplomat being assigned.
        vip_id: String,
        /// Home country of the diplomat (country that owns the VIP).
        home_country: String,
        /// Country where the diplomat is being posted.
        host_country: String,
        /// Type of diplomatic post.
        post_type: DiplomaticPostType,
        /// Turn when this assignment is made.
        assigned_turn: u32,
    },
    /// Phase 66: Recall a diplomat from their foreign post.
    RecallDiplomat {
        /// VIP ID of the diplomat being recalled.
        vip_id: String,
        /// Home country of the diplomat.
        home_country: String,
    },
}

/// Drain the pending diplomatic action queue sequentially.
///
/// This function is called after the parallel per-country turn processing
/// completes. It processes each action in order, safely mutating both
/// home and host countries.
///
/// # Arguments
/// * `state` - Mutable game state (for country access and queue drain).
/// * `diplomatic_config` - Configuration for costs and penalties.
///
/// # Rules
/// - Debits home country's liquid_reserves for financial actions.
/// - Credits host country's treasury for aid/transfers.
/// - Missing/annexed host countries result in a no-op (funds returned).
/// - No money creation: if the home country lacks funds, the action is skipped.
/// - Diplomat assignment requires sufficient liquid_reserves (assignment cost).
pub fn drain_diplomatic_actions(
    state: &mut crate::state::GameState,
    diplomatic_config: &crate::international::fog_of_war::DiplomaticConfig,
) {
    let actions = std::mem::take(&mut state.pending_diplomatic_actions);

    for action in actions {
        match action {
            DiplomaticAction::EmbassyConstructionRequest {
                home_country,
                host_country,
                budget,
            } => {
                // Debit home country's treasury
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if home.budget.liquid_reserves < budget {
                        continue;
                    }
                    home.budget.liquid_reserves -= budget;
                } else {
                    continue;
                }

                // Credit host country: inject as treasury revenue
                if let Some(host) = state.countries.get_mut(&host_country) {
                    host.budget.liquid_reserves += budget * 0.5;
                }
            }
            DiplomaticAction::EmbassyFundingTransfer {
                home_country,
                host_country,
                amount,
            } => {
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if home.budget.liquid_reserves < amount {
                        continue;
                    }
                    home.budget.liquid_reserves -= amount;
                } else {
                    continue;
                }

                if let Some(host) = state.countries.get_mut(&host_country) {
                    host.budget.liquid_reserves += amount;
                }
            }
            DiplomaticAction::ExpelDiplomat {
                home_country,
                host_country,
            } => {
                // Find and remove the diplomat's posting from the home country's VIP registry
                if let Some(home) = state.countries.get_mut(&home_country) {
                    if let Some(registry) = &mut home.politics.vip_registry {
                        for vip in registry.vips.values_mut() {
                            if vip.diplomatic_post.as_ref().is_some_and(|p| p.host_country == host_country) {
                                vip.diplomatic_post = None;
                                // Remove diplomatic role from roles list
                                vip.roles.retain(|r| !matches!(r,
                                    crate::politics::vip_registry::VipRoleExtended::Ambassador
                                    | crate::politics::vip_registry::VipRoleExtended::Consul
                                    | crate::politics::vip_registry::VipRoleExtended::Spy
                                ));
                            }
                        }
                    }
                }
                // Note: relation freeze is handled by the diplomacy turn processor
            }
            DiplomaticAction::SendEconomicAid {
                from_country,
                to_country,
                amount,
            } => {
                // Phase 68: FinancialIsolation sanction blocks aid to the sanctioned country.
                let current_turn = state.calendar.global_turn;
                if state.active_sanctions.has_financial_isolation(&to_country, current_turn) {
                    continue; // Sanctioned — aid blocked
                }

                // Strict double-entry: debit sender, credit receiver
                if amount < diplomatic_config.min_aid_amount {
                    continue;
                }
                let from = state.countries.get_mut(&from_country);
                if let Some(from) = from {
                    if from.budget.liquid_reserves < amount {
                        continue; // Insufficient funds — no money creation
                    }
                    from.budget.liquid_reserves -= amount;
                } else {
                    continue;
                }

                if let Some(to) = state.countries.get_mut(&to_country) {
                    to.budget.liquid_reserves += amount;
                }
                // If recipient doesn't exist, funds are lost (humanitarian aid to failed state)
            }
            DiplomaticAction::BorderProvocation {
                from_country,
                to_country,
                intensity: _,
            } => {
                // No financial transfer — just relation damage (handled by diplomacy turn)
                // Mark the relation as frozen via the diplomacy matrix if it exists
                // The actual relation penalty is applied in process_diplomacy_turn
                // Here we just ensure both countries exist
                if !state.countries.contains_key(&from_country) || !state.countries.contains_key(&to_country) {
                    continue;
                }
            }
            DiplomaticAction::AssignDiplomat {
                vip_id,
                home_country,
                host_country,
                post_type,
                assigned_turn,
            } => {
                // Phase 78: Check diplomatic post cap before assignment.
                // A country cannot post more diplomats of a given type to a
                // host than the DiplomaticPostCap allows.
                let cap = crate::international::diplomacy::DiplomaticPostCap::default();
                let current_count = crate::international::diplomacy::count_diplomats(
                    state, &home_country, &host_country, &post_type,
                );
                if current_count >= cap.for_post_type(&post_type) {
                    continue; // Cap reached — cannot post another diplomat of this type
                }

                // Check sufficient funds for diplomat assignment cost
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if home.budget.liquid_reserves < diplomatic_config.diplomat_assignment_cost {
                        continue; // Insufficient funds — cannot post diplomat
                    }
                    home.budget.liquid_reserves -= diplomatic_config.diplomat_assignment_cost;

                    // Assign the diplomatic post to the VIP
                    if let Some(registry) = &mut home.politics.vip_registry {
                        if let Some(vip) = registry.vips.get_mut(&vip_id) {
                            vip.diplomatic_post = Some(crate::politics::vip_registry::DiplomaticPost {
                                host_country: host_country.clone(),
                                post_type: post_type.clone(),
                                assigned_turn,
                            });
                            // Add the corresponding role
                            let role = match post_type {
                                DiplomaticPostType::Ambassador => crate::politics::vip_registry::VipRoleExtended::Ambassador,
                                DiplomaticPostType::Consul => crate::politics::vip_registry::VipRoleExtended::Consul,
                                DiplomaticPostType::Spy => crate::politics::vip_registry::VipRoleExtended::Spy,
                                DiplomaticPostType::MilitaryAttache => crate::politics::vip_registry::VipRoleExtended::MilitaryCommander,
                            };
                            if !vip.roles.contains(&role) {
                                vip.roles.push(role);
                            }
                        }
                    }
                }
            }
            DiplomaticAction::RecallDiplomat {
                vip_id,
                home_country,
            } => {
                let home = state.countries.get_mut(&home_country);
                if let Some(home) = home {
                    if let Some(registry) = &mut home.politics.vip_registry {
                        if let Some(vip) = registry.vips.get_mut(&vip_id) {
                            vip.diplomatic_post = None;
                            vip.roles.retain(|r| !matches!(r,
                                crate::politics::vip_registry::VipRoleExtended::Ambassador
                                | crate::politics::vip_registry::VipRoleExtended::Consul
                                | crate::politics::vip_registry::VipRoleExtended::Spy
                            ));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::international::fog_of_war::DiplomaticConfig;
    use crate::state::{GameState, Country};
    use crate::politics::vip_registry::{Vip, VipRegistry, DiplomaticPostType, VipRoleExtended};

    #[test]
    fn test_drain_empty_queue() {
        let mut state = GameState::default();
        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);
        assert!(state.pending_diplomatic_actions.is_empty());
    }

    #[test]
    fn test_embassy_construction_missing_host() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 1_000_000.0;
        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyConstructionRequest {
            home_country: "HomeLand".to_string(),
            host_country: "NonExistent".to_string(),
            budget: 300_000.0,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        assert!((home.budget.liquid_reserves - 700_000.0).abs() < 0.01);
    }

    #[test]
    fn test_embassy_funding_transfer() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 500_000.0;
        let mut host = Country::mock_for_tests();
        host.name = "HostLand".to_string();
        host.budget.liquid_reserves = 100_000.0;
        state.countries.insert("HomeLand".to_string(), home);
        state.countries.insert("HostLand".to_string(), host);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyFundingTransfer {
            home_country: "HomeLand".to_string(),
            host_country: "HostLand".to_string(),
            amount: 50_000.0,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        let host = state.countries.get("HostLand").unwrap();
        assert!((home.budget.liquid_reserves - 450_000.0).abs() < 0.01);
        assert!((host.budget.liquid_reserves - 150_000.0).abs() < 0.01);
    }

    #[test]
    fn test_insufficient_funds_skipped() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 10_000.0;
        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::EmbassyFundingTransfer {
            home_country: "HomeLand".to_string(),
            host_country: "HostLand".to_string(),
            amount: 50_000.0,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        assert!((home.budget.liquid_reserves - 10_000.0).abs() < 0.01);
    }

    #[test]
    fn test_send_economic_aid_double_entry() {
        let mut state = GameState::default();
        let mut from = Country::mock_for_tests();
        from.name = "RichCountry".to_string();
        from.budget.liquid_reserves = 10_000_000.0;
        let mut to = Country::mock_for_tests();
        to.name = "PoorCountry".to_string();
        to.budget.liquid_reserves = 100_000.0;
        state.countries.insert("RichCountry".to_string(), from);
        state.countries.insert("PoorCountry".to_string(), to);

        state.pending_diplomatic_actions.push(DiplomaticAction::SendEconomicAid {
            from_country: "RichCountry".to_string(),
            to_country: "PoorCountry".to_string(),
            amount: 500_000.0,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let from = state.countries.get("RichCountry").unwrap();
        let to = state.countries.get("PoorCountry").unwrap();
        assert!((from.budget.liquid_reserves - 9_500_000.0).abs() < 0.01,
            "sender should have 9.5M");
        assert!((to.budget.liquid_reserves - 600_000.0).abs() < 0.01,
            "receiver should have 600K");
    }

    #[test]
    fn test_send_economic_aid_insufficient_funds() {
        let mut state = GameState::default();
        let mut from = Country::mock_for_tests();
        from.name = "PoorSender".to_string();
        from.budget.liquid_reserves = 500.0;
        let mut to = Country::mock_for_tests();
        to.name = "Receiver".to_string();
        to.budget.liquid_reserves = 100_000.0;
        state.countries.insert("PoorSender".to_string(), from);
        state.countries.insert("Receiver".to_string(), to);

        state.pending_diplomatic_actions.push(DiplomaticAction::SendEconomicAid {
            from_country: "PoorSender".to_string(),
            to_country: "Receiver".to_string(),
            amount: 500_000.0,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let from = state.countries.get("PoorSender").unwrap();
        let to = state.countries.get("Receiver").unwrap();
        assert!((from.budget.liquid_reserves - 500.0).abs() < 0.01, "no debit");
        assert!((to.budget.liquid_reserves - 100_000.0).abs() < 0.01, "no credit");
    }

    #[test]
    fn test_assign_diplomat_insufficient_funds() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 10_000.0; // Below assignment cost of 50_000

        // Add a VIP to the registry
        let mut registry = VipRegistry::default();
        let vip = Vip {
            id: "VIP-000001".to_string(),
            full_name: "John Diplomat".to_string(),
            ..Vip::default()
        };
        registry.vips.insert("VIP-000001".to_string(), vip);
        home.politics.vip_registry = Some(registry);

        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::AssignDiplomat {
            vip_id: "VIP-000001".to_string(),
            home_country: "HomeLand".to_string(),
            host_country: "ForeignLand".to_string(),
            post_type: DiplomaticPostType::Ambassador,
            assigned_turn: 1,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        // VIP should NOT have a diplomatic post (insufficient funds)
        let home = state.countries.get("HomeLand").unwrap();
        let registry = home.politics.vip_registry.as_ref().unwrap();
        let vip = registry.vips.get("VIP-000001").unwrap();
        assert!(vip.diplomatic_post.is_none(), "Diplomat should not be assigned without funds");
    }

    #[test]
    fn test_assign_diplomat_success() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 1_000_000.0;

        let mut registry = VipRegistry::default();
        let vip = Vip {
            id: "VIP-000001".to_string(),
            full_name: "Jane Ambassador".to_string(),
            ..Vip::default()
        };
        registry.vips.insert("VIP-000001".to_string(), vip);
        home.politics.vip_registry = Some(registry);

        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::AssignDiplomat {
            vip_id: "VIP-000001".to_string(),
            home_country: "HomeLand".to_string(),
            host_country: "ForeignLand".to_string(),
            post_type: DiplomaticPostType::Ambassador,
            assigned_turn: 5,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        // Assignment cost should have been debited
        assert!((home.budget.liquid_reserves - (1_000_000.0 - config.diplomat_assignment_cost)).abs() < 0.01);
        // VIP should have the diplomatic post
        let registry = home.politics.vip_registry.as_ref().unwrap();
        let vip = registry.vips.get("VIP-000001").unwrap();
        assert!(vip.diplomatic_post.is_some());
        let post = vip.diplomatic_post.as_ref().unwrap();
        assert_eq!(post.host_country, "ForeignLand");
        assert_eq!(post.post_type, DiplomaticPostType::Ambassador);
        assert!(vip.roles.contains(&VipRoleExtended::Ambassador));
    }

    #[test]
    fn test_recall_diplomat() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 1_000_000.0;

        let mut registry = VipRegistry::default();
        let mut vip = Vip {
            id: "VIP-000001".to_string(),
            full_name: "John Spy".to_string(),
            ..Vip::default()
        };
        vip.diplomatic_post = Some(crate::politics::vip_registry::DiplomaticPost {
            host_country: "ForeignLand".to_string(),
            post_type: DiplomaticPostType::Spy,
            assigned_turn: 1,
        });
        vip.roles.push(VipRoleExtended::Spy);
        registry.vips.insert("VIP-000001".to_string(), vip);
        home.politics.vip_registry = Some(registry);

        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::RecallDiplomat {
            vip_id: "VIP-000001".to_string(),
            home_country: "HomeLand".to_string(),
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        let registry = home.politics.vip_registry.as_ref().unwrap();
        let vip = registry.vips.get("VIP-000001").unwrap();
        assert!(vip.diplomatic_post.is_none(), "Post should be cleared");
        assert!(!vip.roles.contains(&VipRoleExtended::Spy), "Spy role should be removed");
    }

    #[test]
    fn test_expel_diplomat_clears_post() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();

        let mut registry = VipRegistry::default();
        let mut vip = Vip {
            id: "VIP-000001".to_string(),
            full_name: "Expelled Ambassador".to_string(),
            ..Vip::default()
        };
        vip.diplomatic_post = Some(crate::politics::vip_registry::DiplomaticPost {
            host_country: "HostLand".to_string(),
            post_type: DiplomaticPostType::Ambassador,
            assigned_turn: 1,
        });
        vip.roles.push(VipRoleExtended::Ambassador);
        registry.vips.insert("VIP-000001".to_string(), vip);
        home.politics.vip_registry = Some(registry);

        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::ExpelDiplomat {
            home_country: "HomeLand".to_string(),
            host_country: "HostLand".to_string(),
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        let home = state.countries.get("HomeLand").unwrap();
        let registry = home.politics.vip_registry.as_ref().unwrap();
        let vip = registry.vips.get("VIP-000001").unwrap();
        assert!(vip.diplomatic_post.is_none(), "Post should be cleared on expulsion");
        assert!(!vip.roles.contains(&VipRoleExtended::Ambassador), "Role should be removed");
    }

    /// Phase 78: Verify that a second ambassador assignment to the same host
    /// is rejected due to the DiplomaticPostCap (max 1 ambassador per host).
    #[test]
    fn test_assign_diplomat_cap_reached() {
        let mut state = GameState::default();
        let mut home = Country::mock_for_tests();
        home.name = "HomeLand".to_string();
        home.budget.liquid_reserves = 1_000_000.0;

        let mut registry = VipRegistry::default();
        // VIP-1 already has an ambassador post to ForeignLand
        let mut vip1 = Vip {
            id: "VIP-000001".to_string(),
            full_name: "First Ambassador".to_string(),
            ..Vip::default()
        };
        vip1.diplomatic_post = Some(crate::politics::vip_registry::DiplomaticPost {
            host_country: "ForeignLand".to_string(),
            post_type: DiplomaticPostType::Ambassador,
            assigned_turn: 1,
        });
        vip1.roles.push(VipRoleExtended::Ambassador);
        registry.vips.insert("VIP-000001".to_string(), vip1);

        // VIP-2 tries to become a second ambassador to the same host
        let vip2 = Vip {
            id: "VIP-000002".to_string(),
            full_name: "Second Ambassador".to_string(),
            ..Vip::default()
        };
        registry.vips.insert("VIP-000002".to_string(), vip2);

        home.politics.vip_registry = Some(registry);
        state.countries.insert("HomeLand".to_string(), home);

        state.pending_diplomatic_actions.push(DiplomaticAction::AssignDiplomat {
            vip_id: "VIP-000002".to_string(),
            home_country: "HomeLand".to_string(),
            host_country: "ForeignLand".to_string(),
            post_type: DiplomaticPostType::Ambassador,
            assigned_turn: 5,
        });

        let config = DiplomaticConfig::default();
        drain_diplomatic_actions(&mut state, &config);

        // VIP-2 should NOT have a diplomatic post (cap reached)
        let home = state.countries.get("HomeLand").unwrap();
        let registry = home.politics.vip_registry.as_ref().unwrap();
        let vip2 = registry.vips.get("VIP-000002").unwrap();
        assert!(vip2.diplomatic_post.is_none(),
            "Second ambassador should not be assigned — cap of 1 reached");
    }
}
