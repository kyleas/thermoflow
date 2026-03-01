//! Generic curve source architecture for arbitrary x-y plotting.
//!
//! This module provides the framework for plotting arbitrary curves beyond
//! time-series histories, including component characteristics, actuator responses,
//! and future fluid property sweeps.

use serde::{Deserialize, Serialize};

/// Unique identifier for a curve source.
#[allow(dead_code)]
pub type CurveSourceId = String;

/// Generic x-y curve data.
#[derive(Debug, Clone)]
pub struct CurveData {
    /// X-axis values
    pub x_values: Vec<f64>,
    /// Y-axis values (same length as x_values)
    pub y_values: Vec<f64>,
    /// Human-readable label for legend
    pub label: String,
}

/// Axis label and units.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct AxisLabel {
    pub name: String,
    pub units: Option<String>,
}

impl AxisLabel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            units: None,
        }
    }

    pub fn with_units(name: impl Into<String>, units: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            units: Some(units.into()),
        }
    }

    #[allow(dead_code)]
    pub fn display(&self) -> String {
        if let Some(ref units) = self.units {
            format!("{} ({})", self.name, units)
        } else {
            self.name.clone()
        }
    }
}

/// Generic curve source that can generate arbitrary x-y data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CurveSource {
    /// Valve characteristic curve (position vs CdA or other property)
    ValveCharacteristic {
        /// ID of the valve component
        component_id: String,
        /// Which characteristic to plot
        characteristic: ValveCharacteristicKind,
        /// Number of sample points
        #[serde(default = "default_sample_count")]
        sample_count: usize,
    },
    /// Actuator step response curve
    ActuatorResponse {
        /// Actuator parameters
        tau_s: f64,
        rate_limit_per_s: f64,
        /// Initial position
        #[serde(default)]
        initial_position: f64,
        /// Command step value
        #[serde(default = "default_step_command")]
        command: f64,
        /// Simulation duration
        #[serde(default = "default_response_duration")]
        duration_s: f64,
        /// Number of time steps
        #[serde(default = "default_sample_count")]
        sample_count: usize,
    },
    /// Placeholder for future fluid property sweep curves
    #[allow(dead_code)]
    FluidPropertySweep {
        /// Property on x-axis (e.g., "temperature", "pressure")
        x_property: String,
        /// Property on y-axis (e.g., "density", "enthalpy")
        y_property: String,
        /// Sweep parameters (specific implementation TBD)
        parameters: FluidSweepParameters,
    },
}

/// Valve characteristic options.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValveCharacteristicKind {
    /// Effective CdA vs position
    EffectiveArea,
    /// Opening factor vs position (for debugging valve laws)
    OpeningFactor,
}

/// Placeholder for future fluid sweep parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FluidSweepParameters {
    // Future: range, fixed properties, etc.
    #[allow(dead_code)]
    placeholder: String,
}

fn default_sample_count() -> usize {
    100
}

fn default_step_command() -> f64 {
    1.0
}

fn default_response_duration() -> f64 {
    5.0
}

impl CurveSource {
    /// Get a descriptive label for this curve source.
    #[allow(dead_code)]
    pub fn label(&self) -> String {
        match self {
            CurveSource::ValveCharacteristic {
                component_id,
                characteristic,
                ..
            } => {
                let char_name = match characteristic {
                    ValveCharacteristicKind::EffectiveArea => "CdA",
                    ValveCharacteristicKind::OpeningFactor => "Opening Factor",
                };
                format!("{} ({})", component_id, char_name)
            }
            CurveSource::ActuatorResponse {
                tau_s,
                rate_limit_per_s,
                ..
            } => {
                format!("Actuator (τ={:.2}s, rate={:.2}/s)", tau_s, rate_limit_per_s)
            }
            CurveSource::FluidPropertySweep {
                x_property,
                y_property,
                ..
            } => {
                format!("{} vs {}", y_property, x_property)
            }
        }
    }

    /// Get the x-axis label for this curve.
    #[allow(dead_code)]
    pub fn x_axis_label(&self) -> AxisLabel {
        match self {
            CurveSource::ValveCharacteristic { .. } => AxisLabel::with_units("Position", "0-1"),
            CurveSource::ActuatorResponse { .. } => AxisLabel::with_units("Time", "s"),
            CurveSource::FluidPropertySweep { x_property, .. } => {
                AxisLabel::new(x_property.clone())
            }
        }
    }

    /// Get the y-axis label for this curve.
    #[allow(dead_code)]
    pub fn y_axis_label(&self) -> AxisLabel {
        match self {
            CurveSource::ValveCharacteristic { characteristic, .. } => match characteristic {
                ValveCharacteristicKind::EffectiveArea => AxisLabel::with_units("CdA", "m²"),
                ValveCharacteristicKind::OpeningFactor => AxisLabel::with_units("Factor", "0-1"),
            },
            CurveSource::ActuatorResponse { .. } => AxisLabel::with_units("Position", "0-1"),
            CurveSource::FluidPropertySweep { y_property, .. } => {
                AxisLabel::new(y_property.clone())
            }
        }
    }
}

/// Runtime representation of curve series selection for a plot.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ArbitraryCurveSelection {
    /// List of arbitrary curve sources to plot
    pub curves: Vec<CurveSource>,
}

impl ArbitraryCurveSelection {
    /// Create empty selection.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a curve to the selection.
    #[allow(dead_code)]
    pub fn add_curve(&mut self, source: CurveSource) {
        self.curves.push(source);
    }

    /// Remove a curve by index.
    #[allow(dead_code)]
    pub fn remove_curve(&mut self, index: usize) {
        if index < self.curves.len() {
            self.curves.remove(index);
        }
    }

    /// Clear all curves.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.curves.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valve_characteristic_label() {
        let source = CurveSource::ValveCharacteristic {
            component_id: "v12".to_string(),
            characteristic: ValveCharacteristicKind::EffectiveArea,
            sample_count: 100,
        };
        assert_eq!(source.label(), "v12 (CdA)");
        assert_eq!(source.x_axis_label().display(), "Position (0-1)");
        assert_eq!(source.y_axis_label().display(), "CdA (m²)");
    }

    #[test]
    fn actuator_response_label() {
        let source = CurveSource::ActuatorResponse {
            tau_s: 0.5,
            rate_limit_per_s: 2.0,
            initial_position: 0.0,
            command: 1.0,
            duration_s: 5.0,
            sample_count: 100,
        };
        assert!(source.label().contains("Actuator"));
        assert!(source.label().contains("0.50"));
        assert_eq!(source.x_axis_label().name, "Time");
        assert_eq!(source.y_axis_label().name, "Position");
    }

    #[test]
    fn curve_selection_operations() {
        let mut selection = ArbitraryCurveSelection::new();
        assert_eq!(selection.curves.len(), 0);

        let curve = CurveSource::ValveCharacteristic {
            component_id: "v1".to_string(),
            characteristic: ValveCharacteristicKind::EffectiveArea,
            sample_count: 50,
        };
        selection.add_curve(curve);
        assert_eq!(selection.curves.len(), 1);

        selection.remove_curve(0);
        assert_eq!(selection.curves.len(), 0);
    }
}
