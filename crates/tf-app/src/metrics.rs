//! Control loop performance metrics analysis.
//!
//! Computes standard control metrics (rise time, settling time, overshoot, steady-state error)
//! from persisted control block time-series data.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// Standard control loop performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopMetrics {
    /// Time for output to reach 10% of final value (seconds)
    pub rise_time_10_s: Option<f64>,
    /// Time for output to reach 90% of final value (seconds)
    pub rise_time_90_s: Option<f64>,
    /// Time for output to settle within ±2% band and stay there (seconds)
    pub settling_time_2pct_s: Option<f64>,
    /// Peak overshoot in percent of final value
    pub overshoot_pct: Option<f64>,
    /// Final steady-state error (measured - setpoint)
    pub steady_state_error: Option<f64>,
    /// Maximum controller output value witnessed
    pub max_controller_output: Option<f64>,
    /// Maximum actuator position witnessed
    pub max_actuator_position: Option<f64>,
    /// Percentage of time actuator was at upper limit (if computable)
    pub saturation_pct_upper: Option<f64>,
    /// Percentage of time controller output was at upper limit (if computable)
    pub controller_saturation_pct_upper: Option<f64>,
}

impl LoopMetrics {
    /// Returns true if at least some metrics were computed
    pub fn has_data(&self) -> bool {
        self.rise_time_10_s.is_some()
            || self.rise_time_90_s.is_some()
            || self.settling_time_2pct_s.is_some()
            || self.overshoot_pct.is_some()
            || self.steady_state_error.is_some()
    }
}

/// Compute metrics for a control loop given measured variable and setpoint time series.
///
/// # Arguments
/// * `measured_series` - (time, value) pairs for measured variable
/// * `setpoint_series` - (time, value) pairs for setpoint
/// * `controller_output_series` - Optional (time, value) pairs for controller output
/// * `actuator_series` - Optional (time, value) pairs for actuator position
///
/// # Returns
/// Populated LoopMetrics with fields that could be computed; others None.
pub fn compute_loop_metrics(
    measured_series: &[(f64, f64)],
    setpoint_series: &[(f64, f64)],
    controller_output_series: Option<&[(f64, f64)]>,
    actuator_series: Option<&[(f64, f64)]>,
) -> AppResult<LoopMetrics> {
    if measured_series.is_empty() || setpoint_series.is_empty() {
        return Ok(LoopMetrics::default());
    }

    let mut metrics = LoopMetrics::default();

    // Get initial and final setpoint values
    let initial_setpoint = setpoint_series
        .first()
        .map(|(_, v)| v)
        .copied()
        .unwrap_or(0.0);
    let final_setpoint = setpoint_series
        .last()
        .map(|(_, v)| v)
        .copied()
        .unwrap_or(0.0);

    // Get initial and final measured values
    let initial_measured = measured_series
        .first()
        .map(|(_, v)| v)
        .copied()
        .unwrap_or(0.0);
    let final_measured = measured_series
        .last()
        .map(|(_, v)| v)
        .copied()
        .unwrap_or(0.0);

    // Steady-state error
    metrics.steady_state_error = Some(final_measured - final_setpoint);

    // Rise time and overshoot: check if there's a measurable change in the response
    // This can happen if setpoint changes OR if measured value changes (response to disturbance/setpoint)
    let measured_change = (final_measured - initial_measured).abs();
    let setpoint_change = (final_setpoint - initial_setpoint).abs();
    let has_dynamic_change = measured_change > 1e-6 || setpoint_change > 1e-6;

    if has_dynamic_change && final_measured.abs() > 1e-6 {
        // Use the final measured value as the target (where system settles)
        metrics.rise_time_10_s =
            compute_time_to_percentage(measured_series, initial_measured, final_measured, 0.1);
        metrics.rise_time_90_s =
            compute_time_to_percentage(measured_series, initial_measured, final_measured, 0.9);

        // Overshoot (peak above final value)
        let peak = measured_series
            .iter()
            .map(|(_, v)| v)
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        if final_measured.abs() > 1e-6 {
            let overshoot = ((peak - final_measured) / final_measured.abs()) * 100.0;
            if overshoot > 0.0 {
                metrics.overshoot_pct = Some(overshoot);
            }
        }

        // Settling time (within 2% band)
        metrics.settling_time_2pct_s = compute_settling_time(measured_series, final_measured, 0.02);
    }

    // Controller output stats
    if let Some(ctrl_series) = controller_output_series {
        if let Some((_, max_val)) = ctrl_series
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            metrics.max_controller_output = Some(*max_val);
        }

        // Detect saturation: assume bounds are [0.0, 1.0] for normalized outputs
        metrics.controller_saturation_pct_upper = Some(compute_saturation_pct(ctrl_series, 0.99));
    }

    // Actuator position stats
    if let Some(act_series) = actuator_series {
        if let Some((_, max_val)) = act_series
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            metrics.max_actuator_position = Some(*max_val);
        }

        // Saturation (assume [0.0, 1.0] unless we see values outside)
        metrics.saturation_pct_upper = Some(compute_saturation_pct(act_series, 0.99));
    }

    Ok(metrics)
}

/// Compute time at which output reaches a percentage of the final value.
/// Percentage is 0.0..1.0 (e.g., 0.1 = 10%, 0.9 = 90%).
fn compute_time_to_percentage(
    series: &[(f64, f64)],
    initial: f64,
    final_val: f64,
    pct: f64,
) -> Option<f64> {
    let change = final_val - initial;
    if change.abs() < 1e-9 {
        return None;
    }

    let target = initial + pct * change;

    // If moving positive and measured stays below target, or vice versa
    let looking_above = change > 0.0;

    for (i, (time, val)) in series.iter().enumerate() {
        let reached = if looking_above {
            *val >= target
        } else {
            *val <= target
        };

        if i > 0 && reached {
            // Linear interpolation between last two points for finer accuracy
            let (prev_time, prev_val) = series[i - 1];
            let delta_t = time - prev_time;
            let delta_v = val - prev_val;

            if delta_v.abs() > 1e-9 {
                let frac = (target - prev_val) / delta_v;
                let interp_time = prev_time + frac * delta_t;
                return Some(interp_time);
            }
            return Some(*time);
        }
    }

    None
}

/// Compute time at which output first enters and remains in the band
/// [final_value * (1 - tolerance), final_value * (1 + tolerance)] strictly.
/// Uses strict inequalities to be conservative.
fn compute_settling_time(series: &[(f64, f64)], final_val: f64, tolerance: f64) -> Option<f64> {
    if final_val.abs() < 1e-9 {
        return None;
    }

    let lower = final_val * (1.0 - tolerance);
    let upper = final_val * (1.0 + tolerance);

    // Find first point where value enters band (strictly), then verify it stays there
    let mut entered = false;
    let mut entry_idx = 0;

    for (i, (_, val)) in series.iter().enumerate() {
        let in_band = *val > lower && *val < upper; // Strict inequalities

        if !entered && in_band {
            // First time entering the band
            entered = true;
            entry_idx = i;
        } else if entered && !in_band {
            // Left the band after having entered; reset
            entered = false;
        }
    }

    // If still in band at end, return entry time
    if entered {
        Some(series[entry_idx].0)
    } else {
        None
    }
}

/// Compute percentage of time the signal was above the threshold (saturation indicator).
fn compute_saturation_pct(series: &[(f64, f64)], threshold: f64) -> f64 {
    if series.len() < 2 {
        return 0.0;
    }

    let total_time = series.last().unwrap().0 - series.first().unwrap().0;
    if total_time <= 0.0 {
        return 0.0;
    }

    let mut saturated_time = 0.0;

    for window in series.windows(2) {
        let (t1, v1) = window[0];
        let (t2, v2) = window[1];
        let dt = t2 - t1;

        // If both values above threshold, full dt is saturated
        if v1 >= threshold && v2 >= threshold {
            saturated_time += dt;
        } else if (v1 >= threshold) != (v2 >= threshold) {
            // One above, one below: approximate with linear section
            let frac = (threshold - v1) / (v2 - v1);
            if (0.0..=1.0).contains(&frac) {
                saturated_time += dt * (1.0 - frac.abs());
            }
        }
    }

    (saturated_time / total_time) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rise_time_10_90_percent() {
        // Simple step response: 0 → 1 starting at t=1s
        let measured = vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (1.5, 0.15), // ~10% at t=1.5
            (2.0, 0.5),  // ~50%
            (2.5, 0.85), // ~90% at t~2.5
            (3.0, 0.95),
            (4.0, 1.0),
        ];
        let setpoint = vec![(0.0, 1.0), (4.0, 1.0)];

        let metrics = compute_loop_metrics(&measured, &setpoint, None, None).unwrap();

        assert!(metrics.rise_time_10_s.is_some());
        assert!(metrics.rise_time_90_s.is_some());
        let t10 = metrics.rise_time_10_s.unwrap();
        let t90 = metrics.rise_time_90_s.unwrap();
        assert!(t10 < t90);
        assert!(t10 > 1.0 && t10 < 2.0);
        assert!(t90 > 2.0 && t90 < 3.0);
    }

    #[test]
    fn test_overshoot() {
        let measured = vec![
            (0.0, 0.0),
            (1.0, 0.5),
            (2.0, 1.2), // Overshoot to 1.2 when target is 1.0
            (3.0, 1.1),
            (4.0, 1.0),
        ];
        let setpoint = vec![(0.0, 1.0), (4.0, 1.0)];

        let metrics = compute_loop_metrics(&measured, &setpoint, None, None).unwrap();

        assert!(metrics.overshoot_pct.is_some());
        let overshoot = metrics.overshoot_pct.unwrap();
        assert!(overshoot > 19.0 && overshoot < 21.0); // ~20%
    }

    #[test]
    fn test_settling_time() {
        let measured = vec![
            (0.0, 0.0),
            (1.0, 0.8),
            (2.0, 1.05),  // Outside ±2% band at 1.0
            (3.0, 1.02),  // On boundary (technically outside with strict inequality)
            (4.0, 1.01),  // Inside: 0.98 < 1.01 < 1.02
            (5.0, 1.002), // Inside and stays
            (6.0, 1.001),
            (7.0, 1.0005),
        ];
        let setpoint = vec![(0.0, 1.0), (7.0, 1.0)];

        let metrics = compute_loop_metrics(&measured, &setpoint, None, None).unwrap();

        assert!(metrics.settling_time_2pct_s.is_some());
        let settle = metrics.settling_time_2pct_s.unwrap();
        // First time value enters the band; depends on floating point comparisons
        assert!(
            (3.0..=4.0).contains(&settle),
            "settling_time {} outside expected range",
            settle
        );
    }

    #[test]
    fn test_steady_state_error() {
        let measured = vec![(0.0, 95.0), (1.0, 98.0), (2.0, 99.0), (3.0, 99.5)];
        let setpoint = vec![(0.0, 100.0), (3.0, 100.0)];

        let metrics = compute_loop_metrics(&measured, &setpoint, None, None).unwrap();

        assert!(metrics.steady_state_error.is_some());
        let err = metrics.steady_state_error.unwrap();
        assert!((err - (-0.5)).abs() < 0.01); // Final measured - final setpoint
    }

    #[test]
    fn test_controller_saturation() {
        let measured = vec![(0.0, 0.0), (1.0, 0.5)];
        let setpoint = vec![(0.0, 1.0), (1.0, 1.0)];
        let controller = vec![
            (0.0, 0.8),
            (0.2, 0.95),
            (0.4, 0.99), // At saturation
            (0.6, 0.99),
            (0.8, 0.5),
            (1.0, 0.3),
        ];

        let metrics = compute_loop_metrics(&measured, &setpoint, Some(&controller), None).unwrap();

        assert!(metrics.controller_saturation_pct_upper.is_some());
        let sat = metrics.controller_saturation_pct_upper.unwrap();
        assert!(sat > 0.0); // Some saturation detected
    }

    #[test]
    fn test_empty_series() {
        let empty: Vec<(f64, f64)> = vec![];
        let setpoint = vec![(0.0, 1.0), (1.0, 1.0)];

        let metrics = compute_loop_metrics(&empty, &setpoint, None, None).unwrap();

        assert!(!metrics.has_data());
    }

    #[test]
    fn test_no_step_change() {
        // Constant setpoint with no disturbance
        let measured = vec![(0.0, 100.0), (1.0, 100.0), (2.0, 100.0)];
        let setpoint = vec![(0.0, 100.0), (2.0, 100.0)];

        let metrics = compute_loop_metrics(&measured, &setpoint, None, None).unwrap();

        // Rise time and overshoot should not be computed for constant signal
        assert!(metrics.rise_time_10_s.is_none());
        assert!(metrics.overshoot_pct.is_none());
        // But steady-state error should be
        assert!(metrics.steady_state_error.is_some());
    }
}
