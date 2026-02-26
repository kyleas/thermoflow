//! Pump component model.

use crate::common::{check_finite, clamp};
use crate::error::{ComponentError, ComponentResult};
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{Area, MassRate, Power, Pressure};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Centrifugal or positive displacement pump.
///
/// Models a pump that adds pressure to a fluid stream using shaft power.
///
/// ## Model
///
/// The pump computes mass flow through a modified orifice equation where
/// an effective pressure rise is applied:
///
/// ```text
/// ΔP_eff = (P_inlet + delta_p) - P_outlet
/// mdot = sign(ΔP_eff) * Cd * A * sqrt(2 * rho * |ΔP_eff|)
/// ```
///
/// The pressure rise `delta_p` is typically set from outside based on
/// pump speed via a characteristic curve.
///
/// Energy transfer:
/// - Fluid enthalpy increases by Δh = delta_p / rho (ideal hydraulic work)
/// - Shaft power required = mdot * Δh / efficiency
/// - Inefficiency appears as additional shaft power consumption
///
/// ## Sign Conventions
///
/// - `shaft_power()` returns POSITIVE value (power consumed from shaft)
/// - Mass flow is positive when flowing inlet → outlet
#[derive(Clone, Debug)]
pub struct Pump {
    /// Component name for debugging
    pub name: String,
    /// Commanded pressure rise (Pa), typically set by control system or speed map
    pub delta_p: Pressure,
    /// Pump efficiency (0 < eta <= 1), converts shaft power to hydraulic power
    pub eta: f64,
    /// Discharge coefficient for flow characteristic
    pub cd: f64,
    /// Effective flow area for mass flow computation
    pub area: Area,
}

impl Pump {
    /// Create a new pump.
    ///
    /// # Arguments
    /// * `name` - Component identifier
    /// * `delta_p` - Pressure rise (Pa)
    /// * `eta` - Efficiency (0 < eta <= 1)
    /// * `cd` - Discharge coefficient (typically 0.6-0.95)
    /// * `area` - Effective flow area
    ///
    /// # Errors
    /// Returns error if parameters are out of physical bounds.
    pub fn new(
        name: String,
        delta_p: Pressure,
        eta: f64,
        cd: f64,
        area: Area,
    ) -> ComponentResult<Self> {
        if eta <= 0.0 || eta > 1.0 {
            return Err(ComponentError::InvalidArg {
                what: "pump efficiency must be in (0,1]",
            });
        }
        if cd <= 0.0 {
            return Err(ComponentError::InvalidArg {
                what: "discharge coefficient must be positive",
            });
        }
        if area.value <= 0.0 {
            return Err(ComponentError::InvalidArg {
                what: "area must be positive",
            });
        }
        if delta_p.value < 0.0 {
            return Err(ComponentError::InvalidArg {
                what: "pump delta_p cannot be negative",
            });
        }

        Ok(Self {
            name,
            delta_p,
            eta,
            cd,
            area,
        })
    }

    /// Update commanded pressure rise (typically called each simulation step).
    pub fn set_delta_p(&mut self, delta_p: Pressure) {
        self.delta_p = delta_p.max(Pressure::new::<uom::si::pressure::pascal>(0.0));
    }
}

impl TwoPortComponent for Pump {
    fn name(&self) -> &str {
        &self.name
    }

    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate> {
        let p_in = ports.inlet.pressure();
        let p_out = ports.outlet.pressure();
        let rho_up = fluid.rho(ports.inlet)?;

        check_finite(p_in.value, "inlet pressure")?;
        check_finite(p_out.value, "outlet pressure")?;
        check_finite(rho_up.value, "inlet density")?;

        if rho_up.value <= 0.0 {
            return Err(ComponentError::NonPhysical {
                what: "density must be positive",
            });
        }

        // Effective pressure rise accounting for pump work
        let dp_eff = (p_in.value + self.delta_p.value) - p_out.value;

        // Modified orifice equation
        let mdot_mag = if dp_eff.abs() < 1e-6 {
            0.0
        } else {
            self.cd * self.area.value * (2.0 * rho_up.value * dp_eff.abs()).sqrt()
        };

        let mdot_val = if dp_eff >= 0.0 { mdot_mag } else { -mdot_mag };

        Ok(MassRate::new::<uom::si::mass_rate::kilogram_per_second>(
            clamp(mdot_val, -1e6, 1e6),
        ))
    }

    fn outlet_enthalpy(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        let h_in = fluid.h(ports.inlet)?;
        let rho_in = fluid.rho(ports.inlet)?;

        if rho_in.value <= 1e-6 {
            return Err(ComponentError::NonPhysical {
                what: "density too low for pump enthalpy calculation",
            });
        }

        // Ideal hydraulic work per unit mass
        let delta_h = self.delta_p.value / rho_in.value;

        Ok(h_in + delta_h)
    }

    fn shaft_power(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        mdot: MassRate,
    ) -> ComponentResult<Power> {
        if mdot.value.abs() < 1e-9 {
            return Ok(Power::new::<uom::si::power::watt>(0.0));
        }

        let rho_in = fluid.rho(ports.inlet)?;

        if rho_in.value <= 1e-6 {
            return Ok(Power::new::<uom::si::power::watt>(0.0));
        }

        // Ideal hydraulic work
        let delta_h = self.delta_p.value / rho_in.value;

        // Shaft power required accounting for efficiency
        let p_shaft = mdot.value * delta_h / self.eta;

        Ok(Power::new::<uom::si::power::watt>(p_shaft))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::{k, m, pa};
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};

    #[test]
    fn pump_creation() {
        let pump = Pump::new(
            "test_pump".to_string(),
            pa(100_000.0),
            0.8,
            0.85,
            m(0.01) * m(0.01),
        );
        assert!(pump.is_ok());
    }

    #[test]
    fn pump_invalid_efficiency() {
        let pump = Pump::new(
            "bad_pump".to_string(),
            pa(100_000.0),
            1.5,
            0.85,
            m(0.01) * m(0.01),
        );
        assert!(pump.is_err());
    }

    #[test]
    fn pump_mdot_increases_with_delta_p() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::H2O);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(250_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let pump1 = Pump::new(
            "pump1".to_string(),
            pa(100_000.0),
            0.8,
            0.85,
            m(0.01) * m(0.01),
        )
        .unwrap();

        let pump2 = Pump::new(
            "pump2".to_string(),
            pa(200_000.0),
            0.8,
            0.85,
            m(0.01) * m(0.01),
        )
        .unwrap();

        let mdot1 = pump1.mdot(&fluid, ports).unwrap();
        let mdot2 = pump2.mdot(&fluid, ports).unwrap();

        // Higher delta_p should give higher mdot
        assert!(mdot2.value > mdot1.value);
        assert!(mdot1.value > 0.0);
    }

    #[test]
    fn pump_shaft_power_positive() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::H2O);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(300_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let pump = Pump::new(
            "pump".to_string(),
            pa(150_000.0),
            0.8,
            0.85,
            m(0.01) * m(0.01),
        )
        .unwrap();

        let mdot = pump.mdot(&fluid, ports).unwrap();
        let power = pump.shaft_power(&fluid, ports, mdot).unwrap();

        // Pump consumes power (positive)
        assert!(power.value > 0.0);
    }

    #[test]
    fn pump_outlet_enthalpy_increases() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::H2O);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(300_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let pump = Pump::new(
            "pump".to_string(),
            pa(100_000.0),
            0.8,
            0.85,
            m(0.01) * m(0.01),
        )
        .unwrap();

        let mdot = pump.mdot(&fluid, ports).unwrap();
        let h_in = fluid.h(&state_in).unwrap();
        let h_out = pump.outlet_enthalpy(&fluid, ports, mdot).unwrap();

        // Pump adds enthalpy
        assert!(h_out > h_in);
    }
}
