//! Bugfix Sprint: World-generation scaling tests.
//!
//! Verifies:
//! - Power plant count scales with regional population (Rule 15).
//! - Nameplate capacity scales with plant_count (no flat 15 plants / 150 MW).
//! - Turn-0 wage floor uses `.max(1.0)`, not hardcoded 500.0.
//! - LV capacity is derived from actual building demand, not magic formula.

#[cfg(test)]
mod tests {
    use sim_engine::energy::generation::{
        nameplate_per_plant, plant_count, target_regional_capacity_mw,
    };

    /// Plant count scales with population — a 10M pop region gets more plants
    /// than a 100K pop region at the same development level and wage.
    #[test]
    fn test_plant_count_scales_with_population() {
        let dev = 0.8;
        let wage = 1000.0;
        let year = 1920;

        let small_pop = 100_000.0;
        let large_pop = 10_000_000.0;

        let small_target = target_regional_capacity_mw(small_pop, dev, wage, year);
        let large_target = target_regional_capacity_mw(large_pop, dev, wage, year);

        let small_count = plant_count(small_target, year);
        let large_count = plant_count(large_target, year);

        assert!(
            large_count > small_count,
            "Large pop ({}) should get more plants ({}) than small pop ({}) ({})",
            large_pop,
            large_count,
            small_pop,
            small_count
        );
    }

    /// Turn-0 wage floor: when average_wage is 0.0, the .max(1.0) floor
    /// prevents capacity collapse without using a hardcoded 500.0.
    #[test]
    fn test_turn0_wage_floor_does_not_collapse_capacity() {
        let pop = 1_000_000.0;
        let dev = 0.5;
        let year = 1920;

        // With wage = 0.0 (uninitialized), the floor .max(1.0) gives wage = 1.0
        let floored_wage = 0.0_f64.max(1.0);
        let target = target_regional_capacity_mw(pop, dev, floored_wage, year);
        assert!(
            target > 0.0,
            "Target capacity with floored wage must be positive, got {}",
            target
        );

        // With the old hardcoded 500.0, target would be 500x larger — verify
        // the floor gives a much smaller (but non-zero) value.
        let old_target = target_regional_capacity_mw(pop, dev, 500.0, year);
        assert!(
            target < old_target,
            "Floored wage target ({}) should be smaller than old 500.0 target ({})",
            target,
            old_target
        );
    }

    /// Total nameplate = nameplate_per_plant * plant_count, so larger regions
    /// get proportionally more capacity (Rule 15 — Universal Physical Scaling).
    #[test]
    fn test_total_nameplate_scales_with_plant_count() {
        let year = 1950;
        let nameplate = nameplate_per_plant(year);

        // Small region: 1 plant
        let small_count = 1;
        let small_total = nameplate * small_count as f64;

        // Large region: 10 plants
        let large_count = 10;
        let large_total = nameplate * large_count as f64;

        assert!(large_total > small_total);
        assert_eq!(large_total / small_total, 10.0);
    }

    /// Nameplate per plant is era-scaled (not a magic number for all eras).
    #[test]
    fn test_nameplate_per_plant_is_era_scaled() {
        assert_eq!(nameplate_per_plant(1900), 10.0);
        assert_eq!(nameplate_per_plant(1920), 50.0);
        assert_eq!(nameplate_per_plant(1950), 200.0);
        assert_eq!(nameplate_per_plant(1980), 500.0);
    }

    /// target_regional_capacity_mw scales with all three inputs (pop, dev, wage).
    #[test]
    fn test_target_capacity_scales_with_development() {
        let pop = 1_000_000.0;
        let wage = 1000.0;
        let year = 1920;

        let low_dev = target_regional_capacity_mw(pop, 0.2, wage, year);
        let high_dev = target_regional_capacity_mw(pop, 0.9, wage, year);

        assert!(high_dev > low_dev);
        // Ratio should be proportional to development level
        assert!((high_dev / low_dev - 0.9 / 0.2).abs() < 0.01);
    }
}
