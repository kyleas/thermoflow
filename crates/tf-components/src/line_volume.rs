//! Line volume element with finite storage and optional flow resistance.
//!
//! LineVolume represents a short fluid line or manifold with meaningful volume.
//! It provides:
//! - Finite fluid storage (mass and energy) in transient simulations
//! - Optional flow resistance (via embedded orifice-like conductance)
//! - Improved transient robustness by buffering rapid pressure changes
//!
//! # Use Cases
//! - Short feed lines or manifolds between control volumes and restrictions
//! - Small plenums or buffer volumes
//! - Any connection where the fluid volume is not negligible compared to attached CVs
//!
//! # Steady-State Behavior
//! In steady-state, the LineVolume acts as a connection with optional pressure drop.
//! No storage accumulation occurs (dM/dt = 0, dU/dt = 0).
//!
//! # Transient Behavior
//! In transient mode, mass and energy are integrated using control volume dynamics,
//! and the thermodynamic state is computed from stored (M, U, V, composition).

use crate::common::{EPSILON_PRESSURE, check_finite};
use crate::error::ComponentResult;
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{Area, MassRate, Volume};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Line volume element with finite storage and optional flow resistance.
///
/// This component provides physically meaningful fluid storage between components,
/// improving transient robustness by providing a lumped-parameter buffer.
#[derive(Debug, Clone)]
pub struct LineVolume {
    name: String,
    /// Internal fluid volume (m³)
    pub volume: Volume,
    /// Optional flow resistance: discharge coefficient (0.0 = no resistance, typical 0.6-0.95)
    pub cd: f64,
    /// Optional flow resistance: effective flow area (used if cd > 0)
    pub area: Option<Area>,
}

impl LineVolume {
    /// Create a new line volume with no flow resistance (lossless).
    ///
    /// This is useful for simple buffering where pressure drop is negligible.
    pub fn new_lossless(name: String, volume: Volume) -> Self {
        Self {
            name,
            volume,
            cd: 0.0,
            area: None,
        }
    }

    /// Create a new line volume with flow resistance.
    ///
    /// Flow resistance is modeled as an embedded orifice with given cd and area.
    /// This is useful when the line has non-negligible pressure drop.
    pub fn new_with_resistance(name: String, volume: Volume, cd: f64, area: Area) -> Self {
        Self {
            name,
            volume,
            cd,
            area: Some(area),
        }
    }

    /// Compute mass flow with resistance (if configured).
    fn mdot_with_resistance(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
    ) -> ComponentResult<MassRate> {
        let area = self.area.ok_or(crate::error::ComponentError::InvalidArg {
            what: "LineVolume: resistance area not configured",
        })?;

        let p_in = ports.inlet.pressure().value;
        let p_out = ports.outlet.pressure().value;
        let dp = p_in - p_out;

        // Small pressure difference => zero flow
        if dp.abs() < EPSILON_PRESSURE {
            return Ok(tf_core::units::kgps(0.0));
        }

        // Use upstream density (incompressible approximation for now)
        let state_up = if dp > 0.0 { ports.inlet } else { ports.outlet };
        let rho = fluid.rho(state_up)?.value;

        check_finite(rho, "density")?;

        // Orifice-like flow: mdot = sign(dp) * Cd * A * sqrt(2 * rho * |dp|)
        let sign = dp.signum();
        let mdot = sign * self.cd * area.value * (2.0 * rho * dp.abs()).sqrt();

        check_finite(mdot, "mass flow rate")?;

        Ok(tf_core::units::kgps(mdot))
    }

    /// Compute mass flow without resistance (lossless).
    ///
    /// In lossless mode, flow is determined purely by upstream/downstream boundary
    /// conditions. For steady-state solving, this means the solver must determine
    /// the flow that satisfies mass balance. We return a nominal value based on
    /// the pressure gradient.
    fn mdot_lossless(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
    ) -> ComponentResult<MassRate> {
        // For lossless line volume, we model it as having very low resistance
        // This is a heuristic that helps the solver converge
        let p_in = ports.inlet.pressure().value;
        let p_out = ports.outlet.pressure().value;
        let dp = p_in - p_out;

        if dp.abs() < EPSILON_PRESSURE {
            return Ok(tf_core::units::kgps(0.0));
        }

        // Use a very large effective area to represent low resistance
        // mdot = sign(dp) * A_large * sqrt(2 * rho * |dp|)
        let state_up = if dp > 0.0 { ports.inlet } else { ports.outlet };
        let rho = fluid.rho(state_up)?.value;

        check_finite(rho, "density")?;

        let sign = dp.signum();
        // Large conductance (equivalent to Cd=1, large area)
        let a_equiv = 1.0; // m² equivalent for lossless
        let mdot = sign * a_equiv * (2.0 * rho * dp.abs()).sqrt();

        check_finite(mdot, "mass flow rate")?;

        Ok(tf_core::units::kgps(mdot))
    }
}

impl TwoPortComponent for LineVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate> {
        if self.cd > 0.0 && self.area.is_some() {
            self.mdot_with_resistance(fluid, ports)
        } else {
            self.mdot_lossless(fluid, ports)
        }
    }

    fn outlet_enthalpy(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        // LineVolume is isenthalpic (no heat transfer, no work)
        // Outlet enthalpy equals inlet enthalpy
        Ok(fluid.h(ports.inlet)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::Volume;
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};
    use uom::si::area::square_meter;
    use uom::si::pressure::pascal;
    use uom::si::thermodynamic_temperature::kelvin;
    use uom::si::volume::cubic_meter;

    fn area(v: f64) -> Area {
        Area::new::<square_meter>(v)
    }

    #[test]
    fn test_lossless_line_volume_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(200_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(150_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp,
            )
            .unwrap();

        let line = LineVolume::new_lossless("test_line".into(), Volume::new::<cubic_meter>(0.01));

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = line.mdot(&model, ports).unwrap();
        assert!(
            mdot.value > 0.0,
            "Flow should be positive with pressure gradient"
        );
    }

    #[test]
    fn test_line_volume_with_resistance() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(200_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(150_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp,
            )
            .unwrap();

        let line = LineVolume::new_with_resistance(
            "test_line".into(),
            Volume::new::<cubic_meter>(0.01),
            0.7,
            area(0.001),
        );

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = line.mdot(&model, ports).unwrap();
        assert!(
            mdot.value > 0.0,
            "Flow should be positive with pressure gradient"
        );
        assert!(mdot.value < 1.0, "Flow should be reasonable magnitude");
    }

    #[test]
    fn test_line_volume_isenthalpic() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(200_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: tf_core::units::Pressure::new::<pascal>(150_000.0),
                    t: tf_core::units::Temperature::new::<kelvin>(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let line = LineVolume::new_lossless("test_line".into(), Volume::new::<cubic_meter>(0.01));

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let h_in = model.h(&state_in).unwrap();
        let h_out = line
            .outlet_enthalpy(&model, ports, tf_core::units::kgps(0.1))
            .unwrap();
        assert!(
            (h_out - h_in).abs() < 1.0,
            "Outlet enthalpy should equal inlet enthalpy (isenthalpic)"
        );
    }
}
