//! Phase 82: CHP (Combined Heat and Power) cogeneration logic.
//!
//! CHP retrofits existing thermal power plants to co-produce `Commodity::Heat`
//! alongside `Commodity::Energy`. When the spot market curtails electrical
//! output but district heating demand exists, the plant switches to Auxiliary
//! Boiler Mode (CORRECTION 6: CHP Winter Paradox).

use crate::energy::heating_types::ChpRetrofitMetadata;

/// Compute the total heat output from a CHP-retrofitted power plant.
///
/// This combines:
/// 1. Normal CHP heat (from dispatched electrical output)
/// 2. Auxiliary boiler heat (when electrical output is curtailed but heat demand exists)
///
/// # Arguments
/// * `chp` - The CHP retrofit metadata
/// * `electrical_dispatched` - Actual electrical output after spot market dispatch (MW)
/// * `nameplate_mw` - Plant nameplate capacity (MW)
/// * `thermal_efficiency` - Plant thermal efficiency
/// * `fuel_available` - Fuel actually consumed this turn (units)
/// * `fuel_cv` - Fuel calorific value (MJ/unit)
/// * `unmet_heat_demand` - Remaining district heating demand (GJ)
///
/// # Returns
/// Total heat output in GJ.
pub fn compute_chp_heat_output(
    chp: &ChpRetrofitMetadata,
    electrical_dispatched: f64,
    nameplate_mw: f64,
    thermal_efficiency: f64,
    fuel_available: f64,
    fuel_cv: f64,
    unmet_heat_demand: f64,
) -> f64 {
    if !chp.is_active {
        return 0.0;
    }

    // Normal CHP heat from dispatched electricity
    let heat_from_chp = chp.heat_from_electrical(electrical_dispatched, thermal_efficiency);

    // Auxiliary boiler mode: if electrical output was curtailed but heat demand exists
    let was_curtailed = electrical_dispatched < nameplate_mw;
    let remaining_demand = (unmet_heat_demand - heat_from_chp).max(0.0);

    let auxiliary_heat = if was_curtailed && remaining_demand > 0.0 {
        chp.auxiliary_heat(
            fuel_available,
            fuel_cv,
            thermal_efficiency,
            remaining_demand,
        )
    } else {
        0.0
    };

    heat_from_chp + auxiliary_heat
}

/// Compute the effective electrical output after CHP efficiency penalty.
///
/// CHP extraction reduces electrical output by `electrical_efficiency_penalty`
/// because steam is extracted before full turbine expansion.
pub fn effective_electrical_output(nameplate_mw: f64, chp: &ChpRetrofitMetadata) -> f64 {
    if !chp.is_active {
        return nameplate_mw;
    }
    nameplate_mw * (1.0 - chp.electrical_efficiency_penalty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::heating_types::ChpRetrofitMetadata;

    fn test_chp() -> ChpRetrofitMetadata {
        ChpRetrofitMetadata {
            heat_to_power_ratio: 1.0,
            electrical_efficiency_penalty: 0.08,
            is_active: true,
            auxiliary_efficiency_factor: 0.85,
        }
    }

    #[test]
    fn test_inactive_chp_no_heat() {
        let chp = ChpRetrofitMetadata {
            is_active: false,
            ..test_chp()
        };
        let heat = compute_chp_heat_output(&chp, 100.0, 100.0, 0.35, 50.0, 24.0, 200.0);
        assert_eq!(heat, 0.0);
    }

    #[test]
    fn test_normal_chp_heat_no_curtailment() {
        let chp = test_chp();
        // Full dispatch, no curtailment → no auxiliary needed
        // heat = 100 * 1.0 * 0.35 = 35.0
        let heat = compute_chp_heat_output(&chp, 100.0, 100.0, 0.35, 50.0, 24.0, 200.0);
        assert!((heat - 35.0).abs() < 1e-9);
    }

    #[test]
    fn test_curtailed_chp_with_auxiliary() {
        let chp = test_chp();
        // Electrical curtailed to 50 MW (nameplate 100)
        // heat_from_chp = 50 * 1.0 * 0.35 = 17.5
        // remaining_demand = 200 - 17.5 = 182.5
        // auxiliary: fuel=50, cv=24, aux_eff=0.35*0.85=0.2975
        // potential = 50 * 24 * 0.2975 = 357.0
        // capped at 182.5
        let heat = compute_chp_heat_output(&chp, 50.0, 100.0, 0.35, 50.0, 24.0, 200.0);
        assert!((heat - 200.0).abs() < 0.1); // Full demand met
    }

    #[test]
    fn test_curtailed_chp_auxiliary_capped_by_fuel() {
        let chp = test_chp();
        // Very little fuel, large demand
        // heat_from_chp = 10 * 1.0 * 0.35 = 3.5
        // remaining = 1000 - 3.5 = 996.5
        // auxiliary: fuel=5, cv=24, aux_eff=0.2975
        // potential = 5 * 24 * 0.2975 = 35.7
        // total = 3.5 + 35.7 = 39.2
        let heat = compute_chp_heat_output(&chp, 10.0, 100.0, 0.35, 5.0, 24.0, 1000.0);
        assert!((heat - 39.2).abs() < 0.5);
    }

    #[test]
    fn test_effective_electrical_output() {
        let chp = test_chp();
        // 100 MW * (1 - 0.08) = 92.0 MW
        let effective = effective_electrical_output(100.0, &chp);
        assert!((effective - 92.0).abs() < 1e-9);
    }

    #[test]
    fn test_effective_electrical_output_inactive() {
        let chp = ChpRetrofitMetadata {
            is_active: false,
            ..test_chp()
        };
        let effective = effective_electrical_output(100.0, &chp);
        assert!((effective - 100.0).abs() < 1e-9);
    }
}
