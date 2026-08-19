//! Core mathematical utilities shared across the simulation.
//!
//! All functions here are **pure**: they take primitive inputs and return
//! results without touching global state. This makes them trivially testable
//! and is the ideal starting point for Golden-master parity verification
//! against the original Python implementation.

/// Applies a per-turn percentage decay to a quantity.
///
/// # Arguments
/// * `value` - The current quantity (e.g., military reserves).
/// * `rate` - The fractional decay rate per turn in `[0.0, 1.0]`
///   (e.g., `0.02` for 2% decay).
///
/// # Returns
/// The quantity remaining after one turn of decay: `value * (1.0 - rate)`.
///
/// # Rules
/// * The rate is clamped to `[0.0, 1.0]`; out-of-range inputs cannot amplify
///   or invert the quantity.
/// * Mirrors the Python military reserve decay (2%/turn) and readiness decay
///   (5%/turn) mechanics.
///
/// # Examples
/// ```
/// use sim_engine::math::apply_decay;
/// let remaining = apply_decay(1000.0, 0.02);
/// assert!((remaining - 980.0).abs() < 1e-9);
/// ```
pub fn apply_decay(value: f64, rate: f64) -> f64 {
    let clamped = rate.clamp(0.0, 1.0);
    value * (1.0 - clamped)
}

/// Increments a military unit's experience by a fixed per-turn gain, capped.
///
/// # Arguments
/// * `current` - Current experience level.
/// * `gain` - Experience gained this turn (e.g., `0.03` professionals,
///   `0.01` conscripts).
/// * `cap` - Maximum attainable experience (e.g., `1.0` professionals,
///   `0.6` conscripts).
///
/// # Returns
/// The new experience level, never exceeding `cap`.
///
/// # Rules
/// * Experience only ever increases (conscripts gain, never lose — fixing the
///   original Python bug).
/// * The result is clamped to `cap`.
///
/// # Examples
/// ```
/// use sim_engine::math::gain_experience;
/// let xp = gain_experience(0.58, 0.01, 0.6);
/// assert!((xp - 0.59).abs() < 1e-9);
/// let capped = gain_experience(0.595, 0.01, 0.6);
/// assert!((capped - 0.6).abs() < 1e-9);
/// ```
pub fn gain_experience(current: f64, gain: f64, cap: f64) -> f64 {
    (current + gain).min(cap)
}

/// Computes the amount siphoned from a budget by a given fraction.
///
/// # Arguments
/// * `budget` - The source budget amount.
/// * `fraction` - The fraction to siphon in `[0.0, 1.0]` (e.g., `0.10` for the
///   Deep State passive spending 10% of the black-ops budget per turn).
///
/// # Returns
/// The siphoned amount: `budget * fraction`, clamped so it never exceeds the
/// available budget and is never negative.
///
/// # Rules
/// * A negative `budget` yields `0.0` (cannot siphon from a deficit here).
/// * `fraction` is clamped to `[0.0, 1.0]`.
pub fn siphon_fraction(budget: f64, fraction: f64) -> f64 {
    if budget <= 0.0 {
        return 0.0;
    }
    budget * fraction.clamp(0.0, 1.0)
}

/// Normalizes a slice of weights so they sum to `1.0`.
///
/// # Arguments
/// * `weights` - Raw non-negative weights (e.g., sector GDP shares or energy
///   mix components before normalization).
///
/// # Returns
/// A `Vec<f64>` of the same length whose elements sum to `1.0`. If the input
/// sum is `0.0` (or the slice is empty), returns the input values unchanged as
/// a `Vec` to avoid division by zero.
///
/// # Rules
/// * Mirrors the Python pattern `{k: v / total for ...}` used for budget
///   allocations, sector shares, and the energy mix.
///
/// # Examples
/// ```
/// use sim_engine::math::normalize;
/// let n = normalize(&[1.0, 3.0]);
/// assert!((n[0] - 0.25).abs() < 1e-9);
/// assert!((n[1] - 0.75).abs() < 1e-9);
/// ```
pub fn normalize(weights: &[f64]) -> Vec<f64> {
    let total: f64 = weights.iter().sum();
    if total == 0.0 {
        return weights.to_vec();
    }
    weights.iter().map(|w| w / total).collect()
}

/// Clamps a percentage-like value to the `[0.0, 100.0]` range.
///
/// # Arguments
/// * `value` - A value expressed on a 0–100 scale (e.g., social unrest,
///   readiness, confidence).
///
/// # Returns
/// The value clamped into `[0.0, 100.0]`.
///
/// # Rules
/// * Used to keep indicators such as social unrest and military readiness
///   within their valid display bounds after arithmetic updates.
pub fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_reduces_value() {
        assert!((apply_decay(1000.0, 0.05) - 950.0).abs() < 1e-9);
    }

    #[test]
    fn decay_rate_is_clamped() {
        assert_eq!(apply_decay(100.0, 2.0), 0.0);
        assert_eq!(apply_decay(100.0, -1.0), 100.0);
    }

    #[test]
    fn experience_respects_cap() {
        assert!((gain_experience(0.99, 0.03, 1.0) - 1.0).abs() < 1e-9);
        assert!((gain_experience(0.0, 0.01, 0.6) - 0.01).abs() < 1e-9);
    }

    #[test]
    fn siphon_handles_deficit() {
        assert_eq!(siphon_fraction(-500.0, 0.1), 0.0);
        assert!((siphon_fraction(10_000.0, 0.10) - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn normalize_sums_to_one() {
        let n = normalize(&[2.0, 2.0, 4.0]);
        assert!((n.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn normalize_zero_sum_is_safe() {
        assert_eq!(normalize(&[0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn clamp_percent_bounds() {
        assert_eq!(clamp_percent(150.0), 100.0);
        assert_eq!(clamp_percent(-10.0), 0.0);
        assert_eq!(clamp_percent(42.0), 42.0);
    }
}
