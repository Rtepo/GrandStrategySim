//! Phase 18S: Sports and recreation facilities — seasonality, health impacts,
//! and funding models.
//!
//! ## Facility Types
//!
//! - **OpenAirField**: Minimal Steel + Timber frame, grass pitch. Capacity
//!   scales by pitch_area_m2. Closes in winter (climate_vulnerability = 1.0).
//! - **IndoorHall**: Steel + Concrete + Bricks structure. Capacity scales by
//!   floor_area_m2. Operates year-round (climate_vulnerability = 0.0).
//! - **Stadium**: Steel + Concrete + Glass + Lighting (Energy input). Capacity
//!   scales by seat_count. Operates year-round with high CAPEX amortization.
//!
//! ## Seasonality Mechanics
//!
//! Open-air facilities lose efficiency in winter and extreme heat based on
//! ACTUAL regional weather data (WeatherState), not a global turn counter.
//! Indoor facilities are unaffected by weather.
//!
//! ## Health Impacts
//!
//! Sports facility capacity improves population health via a per-capita
//! physical access term in life expectancy calculations.
//!
//! ## Funding Models
//!
//! - **Public**: Local government ownership, free at point of use via 100%
//!   buyer_subsidy. Accessible to citizens with ZERO savings.
//! - **Private**: Commercial B2C fee collection. Naturally gated by savings.
//! - **Subsidized**: Government subsidy + citizen co-payment.
//! - **Privatization**: Transfer from public to private ownership.

use crate::economy::weather::WeatherState;
use crate::state::Season;
use serde::{Deserialize, Serialize};

/// Phase 18S: Sports facility classification (open-air vs indoor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SportsFacilityClass {
    /// Open-air facility (field) — affected by seasonality
    #[default]
    OpenAir,
    /// Indoor facility (hall, stadium) — operates year-round
    Indoor,
}

/// Phase 18S: Funding model for sports facilities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SportsFundingModel {
    /// Local government ownership — debits municipal budget, free at point of
    /// use via 100% buyer_subsidy. Accessible to zero-savings citizens.
    #[default]
    Public,
    /// Private commercial — charges fees via B2C market clearing.
    /// Citizens with insufficient savings are turned away (unmet demand).
    Private,
    /// Subsidized — government pays a fraction, citizens pay the remainder.
    Subsidized {
        /// Government subsidy fraction (0.0-1.0)
        government_subsidy_rate: f64,
        /// Citizen co-payment fraction (0.0-1.0)
        citizen_copayment_rate: f64,
    },
    /// Privatization transfer — public facility being sold to private owner.
    /// Transitions from Public to Private over a defined period.
    PrivatizationTransfer {
        /// Turn when privatization started
        start_turn: u32,
        /// Transition period in turns
        transition_period: u32,
    },
}

/// Phase 18S: Sports facility configuration and state.
///
/// Each facility has a complete lifecycle: construction, operation,
/// privatization, closure/demolition (Rule 4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SportsFacility {
    /// Unique facility ID
    pub id: String,
    /// Facility name
    pub name: String,
    /// Facility class (open-air or indoor)
    pub facility_class: SportsFacilityClass,
    /// Region where facility is located
    pub region_id: String,
    /// Total capacity (visitor-slots per turn)
    pub total_capacity: f64,
    /// Current utilization (0.0-1.0, clamped — Rule 20)
    #[serde(default)]
    pub utilization: f64,
    /// Funding model
    pub funding_model: SportsFundingModel,
    /// Owner entity ID (government or private company)
    pub owner_id: String,
    /// Physical size metric (pitch_area_m2 for open-air, floor_area_m2 for
    /// indoor, seat_count for stadium) — drives capacity and OPEX scaling
    /// (Rule 15: no flat rates)
    pub size_metric: f64,
    /// Current season efficiency multiplier (0.0-1.0, computed each turn from
    /// actual weather data)
    #[serde(default)]
    pub current_season_efficiency: f64,
    /// Phase 18S: Last turn visitor count
    #[serde(default)]
    pub last_turn_visitors: f64,
    /// Phase 18S: Last turn revenue collected
    #[serde(default)]
    pub last_turn_revenue: f64,
    /// Phase 18S: Facility lifecycle state
    #[serde(default)]
    pub lifecycle_state: SportsFacilityLifecycle,
}

/// Phase 18S: Facility lifecycle state machine (Rule 4 — complete lifecycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SportsFacilityLifecycle {
    /// Under construction — CAPEX being disbursed, BOM procured
    #[default]
    Constructing,
    /// Operational — producing SportsCapacity, consuming OPEX
    Operational,
    /// Being privatized — ownership transfer in progress
    Privatizing,
    /// Closed — no output, awaiting demolition or sale
    Closed,
    /// Demolished — land released to cadastre, salvage credited
    Demolished,
}

impl SportsFacility {
    /// Create a new sports facility.
    pub fn new(
        id: String,
        name: String,
        facility_class: SportsFacilityClass,
        region_id: String,
        total_capacity: f64,
        funding_model: SportsFundingModel,
        owner_id: String,
        size_metric: f64,
    ) -> Self {
        Self {
            id,
            name,
            facility_class,
            region_id,
            total_capacity,
            utilization: 0.0,
            funding_model,
            owner_id,
            size_metric,
            current_season_efficiency: 1.0,
            last_turn_visitors: 0.0,
            last_turn_revenue: 0.0,
            lifecycle_state: SportsFacilityLifecycle::Operational,
        }
    }

    /// Phase 18S: Compute seasonality efficiency multiplier for this facility
    /// based on ACTUAL regional weather data.
    ///
    /// Open-air facilities close in winter or during EarlyFrost events.
    /// They lose efficiency during heatwaves in summer.
    /// Indoor facilities operate at full efficiency year-round.
    ///
    /// # Arguments
    /// * `season` - Current season
    /// * `weather_state` - Country weather state with active events
    pub fn compute_season_efficiency(
        &self,
        season: Season,
        weather_state: &WeatherState,
    ) -> f64 {
        match self.facility_class {
            SportsFacilityClass::Indoor => 1.0,
            SportsFacilityClass::OpenAir => {
                let has_early_frost = weather_state.active_events.iter().any(|e| {
                    e.event_type == crate::economy::weather::WeatherEventType::EarlyFrost
                        && e.affected_regions.iter().any(|r| r == &self.region_id)
                });
                let has_heatwave = weather_state.active_events.iter().any(|e| {
                    e.event_type == crate::economy::weather::WeatherEventType::Heatwave
                        && e.affected_regions.iter().any(|r| r == &self.region_id)
                });

                if season == Season::Winter || has_early_frost {
                    return 0.0; // Closed
                }
                if season == Season::Summer && has_heatwave {
                    return 0.3; // Reduced capacity
                }
                1.0
            }
        }
    }

    /// Phase 18S: Compute effective capacity after seasonality adjustment.
    pub fn effective_capacity(&self) -> f64 {
        self.total_capacity * self.current_season_efficiency
    }

    /// Phase 18S: Process the facility for one turn.
    ///
    /// Returns (visitors, sports_capacity_produced).
    pub fn process_turn(
        &mut self,
        season: Season,
        weather_state: &WeatherState,
    ) -> (f64, f64) {
        if self.lifecycle_state != SportsFacilityLifecycle::Operational {
            self.last_turn_visitors = 0.0;
            self.last_turn_revenue = 0.0;
            return (0.0, 0.0);
        }

        self.current_season_efficiency =
            self.compute_season_efficiency(season, weather_state);

        let effective_cap = self.effective_capacity();

        // Utilization: public facilities have high utilization (pro-rata by
        // population, free at point of use). Private facilities are gated by
        // affordability (B2C clearing handles this).
        let utilization = match self.funding_model {
            SportsFundingModel::Public => 0.9 * self.current_season_efficiency,
            SportsFundingModel::Private => 0.6 * self.current_season_efficiency,
            SportsFundingModel::Subsidized { .. } => 0.75 * self.current_season_efficiency,
            SportsFundingModel::PrivatizationTransfer { .. } => {
                0.7 * self.current_season_efficiency
            }
        };
        self.utilization = utilization.clamp(0.0, 1.0); // Rule 20

        let visitors = effective_cap * self.utilization;
        self.last_turn_visitors = visitors;

        let sports_capacity = visitors; // 1 unit of SportsCapacity per visitor

        (visitors, sports_capacity)
    }

    /// Phase 18S: Close the facility (lifecycle transition).
    pub fn close(&mut self) {
        self.lifecycle_state = SportsFacilityLifecycle::Closed;
        self.utilization = 0.0;
        self.last_turn_visitors = 0.0;
    }

    /// Phase 18S: Demolish the facility (lifecycle transition).
    /// Land is released to cadastre, salvage value credited to owner.
    pub fn demolish(&mut self) {
        self.lifecycle_state = SportsFacilityLifecycle::Demolished;
        self.utilization = 0.0;
        self.total_capacity = 0.0;
        self.last_turn_visitors = 0.0;
        self.last_turn_revenue = 0.0;
    }

    /// Phase 18S: Begin privatization transfer.
    pub fn begin_privatization(&mut self, start_turn: u32, transition_period: u32) {
        self.lifecycle_state = SportsFacilityLifecycle::Privatizing;
        self.funding_model = SportsFundingModel::PrivatizationTransfer {
            start_turn,
            transition_period,
        };
    }

    /// Phase 18S: Complete privatization transfer.
    pub fn complete_privatization(&mut self, new_owner_id: String) {
        self.funding_model = SportsFundingModel::Private;
        self.owner_id = new_owner_id;
        self.lifecycle_state = SportsFacilityLifecycle::Operational;
    }
}

/// Phase 18S: Compute aggregate sports capacity per capita for a country.
///
/// This feeds into the life expectancy calculation as a per-capita physical
/// access term (NOT a demographic abstraction).
///
/// # Arguments
/// * `facilities` - All sports facilities in the country
/// * `population` - Total population
///
/// # Returns
/// Sports capacity per capita (0.0 = no access, clamped at 1.0 — Rule 20)
pub fn compute_sports_capacity_per_capita(
    facilities: &[SportsFacility],
    population: f64,
) -> f64 {
    if population <= 0.0 {
        return 0.0;
    }
    let total_capacity: f64 = facilities
        .iter()
        .filter(|f| f.lifecycle_state == SportsFacilityLifecycle::Operational)
        .map(|f| f.effective_capacity() * f.utilization)
        .sum();
    (total_capacity / population).min(1.0) // Rule 20: clamp at 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_air_winter_closes() {
        let facility = SportsFacility::new(
            "f1".to_string(),
            "Test Field".to_string(),
            SportsFacilityClass::OpenAir,
            "reg1".to_string(),
            1000.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            5000.0,
        );
        let weather = WeatherState::default();
        let eff = facility.compute_season_efficiency(Season::Winter, &weather);
        assert_eq!(eff, 0.0, "Open-air facilities must close in winter");
    }

    #[test]
    fn test_indoor_year_round() {
        let facility = SportsFacility::new(
            "f2".to_string(),
            "Test Hall".to_string(),
            SportsFacilityClass::Indoor,
            "reg1".to_string(),
            500.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            2000.0,
        );
        let weather = WeatherState::default();
        let winter_eff = facility.compute_season_efficiency(Season::Winter, &weather);
        let summer_eff = facility.compute_season_efficiency(Season::Summer, &weather);
        assert_eq!(
            winter_eff, 1.0,
            "Indoor facilities operate at full capacity in winter"
        );
        assert_eq!(
            summer_eff, 1.0,
            "Indoor facilities operate at full capacity in summer"
        );
    }

    #[test]
    fn test_early_frost_closes_open_air() {
        let facility = SportsFacility::new(
            "f3".to_string(),
            "Test Field".to_string(),
            SportsFacilityClass::OpenAir,
            "reg1".to_string(),
            1000.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            5000.0,
        );
        let weather = WeatherState {
            active_events: vec![crate::economy::weather::WeatherEvent {
                event_type: crate::economy::weather::WeatherEventType::EarlyFrost,
                severity: 0.8,
                affected_regions: vec!["reg1".to_string()],
                remaining_turns: 3,
                start_turn: 10,
                extra: Default::default(),
            }],
            last_event_turn: 10,
            seed: 42,
            extra: Default::default(),
        };
        let eff = facility.compute_season_efficiency(Season::Autumn, &weather);
        assert_eq!(eff, 0.0, "EarlyFrost must close open-air facilities");
    }

    #[test]
    fn test_heatwave_reduces_open_air() {
        let facility = SportsFacility::new(
            "f4".to_string(),
            "Test Field".to_string(),
            SportsFacilityClass::OpenAir,
            "reg1".to_string(),
            1000.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            5000.0,
        );
        let weather = WeatherState {
            active_events: vec![crate::economy::weather::WeatherEvent {
                event_type: crate::economy::weather::WeatherEventType::Heatwave,
                severity: 0.9,
                affected_regions: vec!["reg1".to_string()],
                remaining_turns: 2,
                start_turn: 20,
                extra: Default::default(),
            }],
            last_event_turn: 20,
            seed: 42,
            extra: Default::default(),
        };
        let eff = facility.compute_season_efficiency(Season::Summer, &weather);
        assert_eq!(
            eff, 0.3,
            "Heatwave in summer must reduce open-air to 30%"
        );
    }

    #[test]
    fn test_utilization_clamped() {
        let mut facility = SportsFacility::new(
            "f5".to_string(),
            "Test Pool".to_string(),
            SportsFacilityClass::Indoor,
            "reg1".to_string(),
            100.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            500.0,
        );
        let weather = WeatherState::default();
        facility.process_turn(Season::Summer, &weather);
        assert!(
            facility.utilization >= 0.0 && facility.utilization <= 1.0,
            "Utilization must be clamped [0.0, 1.0]"
        );
    }

    #[test]
    fn test_sports_capacity_per_capita() {
        let facilities = vec![
            SportsFacility::new(
                "f1".to_string(),
                "Stadium".to_string(),
                SportsFacilityClass::Indoor,
                "reg1".to_string(),
                1000.0,
                SportsFundingModel::Public,
                "STATE".to_string(),
                10000.0,
            ),
            SportsFacility::new(
                "f2".to_string(),
                "Hall".to_string(),
                SportsFacilityClass::Indoor,
                "reg1".to_string(),
                500.0,
                SportsFundingModel::Public,
                "STATE".to_string(),
                2000.0,
            ),
        ];
        let cap = compute_sports_capacity_per_capita(&facilities, 10000.0);
        assert!(cap >= 0.0 && cap <= 1.0, "Capacity per capita clamped");
    }

    #[test]
    fn test_lifecycle_close() {
        let mut facility = SportsFacility::new(
            "f6".to_string(),
            "Test".to_string(),
            SportsFacilityClass::Indoor,
            "reg1".to_string(),
            100.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            500.0,
        );
        facility.close();
        assert_eq!(facility.lifecycle_state, SportsFacilityLifecycle::Closed);
        assert_eq!(facility.utilization, 0.0);
    }

    #[test]
    fn test_lifecycle_demolish() {
        let mut facility = SportsFacility::new(
            "f7".to_string(),
            "Test".to_string(),
            SportsFacilityClass::Indoor,
            "reg1".to_string(),
            100.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            500.0,
        );
        facility.demolish();
        assert_eq!(
            facility.lifecycle_state,
            SportsFacilityLifecycle::Demolished
        );
        assert_eq!(facility.total_capacity, 0.0);
    }

    #[test]
    fn test_privatization_transfer() {
        let mut facility = SportsFacility::new(
            "f8".to_string(),
            "Test".to_string(),
            SportsFacilityClass::Indoor,
            "reg1".to_string(),
            100.0,
            SportsFundingModel::Public,
            "STATE".to_string(),
            500.0,
        );
        facility.begin_privatization(10, 5);
        assert_eq!(
            facility.lifecycle_state,
            SportsFacilityLifecycle::Privatizing
        );
        facility.complete_privatization("COMP1".to_string());
        assert_eq!(facility.funding_model, SportsFundingModel::Private);
        assert_eq!(facility.owner_id, "COMP1");
        assert_eq!(
            facility.lifecycle_state,
            SportsFacilityLifecycle::Operational
        );
    }
}
