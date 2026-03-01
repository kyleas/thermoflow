//! Fluid property sweep generation.
//!
//! Supports parametric sweeps across temperature, pressure, and other properties.
//! Used to generate plots, comparison tables, and sensitivity analyses.

use crate::units::Quantity;
use std::fmt;

/// Type of sweep progression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SweepType {
    /// Uniformly spaced points
    Linear,
    /// Logarithmically spaced points
    Logarithmic,
}

/// Definition of a single parameter sweep.
///
/// Stores the original user-specified bounds and generates intermediate points
/// as needed by the UI or backend.
#[derive(Debug, Clone)]
pub struct SweepDefinition {
    /// Quantity being swept (Temperature, Pressure, etc.)
    pub quantity: Quantity,
    /// Start value in canonical SI units
    pub start_si: f64,
    /// User input for start (preserved for re-editing)
    pub start_raw: String,
    /// End value in canonical SI units
    pub end_si: f64,
    /// User input for end (preserved for re-editing)
    pub end_raw: String,
    /// Number of points to generate
    pub num_points: usize,
    /// Spacing type
    pub sweep_type: SweepType,
}

impl SweepDefinition {
    /// Create a sweep from user text inputs.
    pub fn from_text(
        start_raw: impl Into<String>,
        end_raw: impl Into<String>,
        quantity: Quantity,
        num_points: usize,
        sweep_type: SweepType,
    ) -> Result<Self, String> {
        let start_text = start_raw.into();
        let end_text = end_raw.into();

        let start_si = crate::parse_quantity(&start_text, quantity)
            .map_err(|e| format!("Start value error: {}", e))?;
        let end_si = crate::parse_quantity(&end_text, quantity)
            .map_err(|e| format!("End value error: {}", e))?;

        if num_points < 2 {
            return Err("Sweep must have at least 2 points".to_string());
        }

        if (start_si - end_si).abs() < 1e-12 {
            return Err("Start and end values must be different".to_string());
        }

        Ok(SweepDefinition {
            quantity,
            start_si,
            start_raw: start_text,
            end_si,
            end_raw: end_text,
            num_points,
            sweep_type,
        })
    }

    /// Generate all points in the sweep.
    pub fn generate_points(&self) -> Vec<f64> {
        match self.sweep_type {
            SweepType::Linear => self.generate_linear(),
            SweepType::Logarithmic => self.generate_logarithmic(),
        }
    }

    fn generate_linear(&self) -> Vec<f64> {
        if self.num_points <= 1 {
            return vec![self.start_si];
        }

        let mut points = Vec::with_capacity(self.num_points);
        let delta = (self.end_si - self.start_si) / (self.num_points - 1) as f64;

        for i in 0..self.num_points {
            points.push(self.start_si + i as f64 * delta);
        }

        // Ensure exact endpoint
        points[self.num_points - 1] = self.end_si;
        points
    }

    fn generate_logarithmic(&self) -> Vec<f64> {
        if self.num_points <= 1 {
            return vec![self.start_si];
        }

        // For logarithmic sweep, both start and end must be positive
        if self.start_si <= 0.0 || self.end_si <= 0.0 {
            return self.generate_linear(); // Fall back to linear if signs don't match
        }

        let mut points = Vec::with_capacity(self.num_points);
        let log_start = self.start_si.ln();
        let log_end = self.end_si.ln();
        let log_delta = (log_end - log_start) / (self.num_points - 1) as f64;

        for i in 0..self.num_points {
            let log_val = log_start + i as f64 * log_delta;
            points.push(log_val.exp());
        }

        // Ensure exact endpoint
        points[self.num_points - 1] = self.end_si;
        points
    }
}

impl fmt::Display for SweepType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Linear => write!(f, "Linear"),
            Self::Logarithmic => write!(f, "Logarithmic"),
        }
    }
}

impl fmt::Display for SweepDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Sweep {} from {} to {} ({} points, {})",
            self.quantity, self.start_raw, self.end_raw, self.num_points, self.sweep_type
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_sweep_generation() {
        let sweep = SweepDefinition {
            quantity: Quantity::Temperature,
            start_si: 300.0,
            start_raw: "300K".to_string(),
            end_si: 400.0,
            end_raw: "400K".to_string(),
            num_points: 5,
            sweep_type: SweepType::Linear,
        };

        let points = sweep.generate_points();
        assert_eq!(points.len(), 5);
        assert!((points[0] - 300.0).abs() < 1e-9);
        assert!((points[2] - 350.0).abs() < 1e-9);
        assert!((points[4] - 400.0).abs() < 1e-9);
    }

    #[test]
    fn logarithmic_sweep_generation() {
        let sweep = SweepDefinition {
            quantity: Quantity::Pressure,
            start_si: 1e5,
            start_raw: "1bar".to_string(),
            end_si: 1e6,
            end_raw: "10bar".to_string(),
            num_points: 3,
            sweep_type: SweepType::Logarithmic,
        };

        let points = sweep.generate_points();
        assert_eq!(points.len(), 3);
        assert!((points[0] - 1e5).abs() < 1e-9);
        assert!((points[2] - 1e6).abs() < 1e-9);
        // Log scale: sqrt(1e5 * 1e6) ≈ 3.16e5
        let expected_mid = (1e5_f64 * 1e6_f64).sqrt();
        assert!((points[1] - expected_mid).abs() / expected_mid < 1e-6);
    }

    #[test]
    fn single_point_sweep() {
        let sweep = SweepDefinition {
            quantity: Quantity::Temperature,
            start_si: 300.0,
            start_raw: "300K".to_string(),
            end_si: 300.0,
            end_raw: "300K".to_string(),
            num_points: 1,
            sweep_type: SweepType::Linear,
        };

        let points = sweep.generate_points();
        assert_eq!(points.len(), 1);
        assert!((points[0] - 300.0).abs() < 1e-9);
    }

    #[test]
    fn sweep_from_text() {
        let sweep = SweepDefinition::from_text("300K", "400K", Quantity::Temperature, 5, SweepType::Linear).unwrap();
        assert_eq!(sweep.num_points, 5);
        assert!((sweep.start_si - 300.0).abs() < 1e-9);
        assert!((sweep.end_si - 400.0).abs() < 1e-9);
    }

    #[test]
    fn reject_invalid_point_count() {
        let result = SweepDefinition::from_text("300K", "400K", Quantity::Temperature, 1, SweepType::Linear);
        assert!(result.is_err());
    }

    #[test]
    fn reject_identical_bounds() {
        let result = SweepDefinition::from_text("300K", "300K", Quantity::Temperature, 5, SweepType::Linear);
        assert!(result.is_err());
    }
}
