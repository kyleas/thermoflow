//! Turbine component model.

use crate::common::{check_finite, clamp};
use crate::error::{ComponentError, ComponentResult};
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{Area, MassRate, Power};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Gas or steam turbine for work extraction.
///
/// Models a turbine that extracts power from a pressurized gas stream.
///
/// ## Model
///
/// Mass flow is computed via an orifice-like restriction driven by pressure drop:
///
/// ```text
/// ΔP = P_inlet - P_outlet
/// mdot = Cd * A * sqrt(2 * rho * ΔP)  (if ΔP > 0)
/// ```
///
/// Work extraction uses an isentropic efficiency model:
/// 1. Estimate isentropic enthalpy drop using pressure ratio and gamma:
///    ```text
///    T_out_s = T_in * (P_out/P_in)^((γ-1)/γ)
///    Δh_s = cp * (T_in - T_out_s)
///    ```
/// 2. Actual work extracted:
///    ```text
///    W_extracted = eta * mdot * Δh_s
///    ```
/// 3. Outlet enthalpy:
///    ```text
///    h_out = h_in - eta * Δh_s
///    ```
///
/// ## Sign Conventions
///
/// - `shaft_power()` returns NEGATIVE value (power extracted from fluid to shaft)
/// - Mass flow is positive when flowing inlet → outlet
#[derive(Clone, Debug)]
pub struct Turbine {
    /// Component name for debugging
    pub name: String,
    /// Discharge coefficient for flow characteristic
    pub cd: f64,
    /// Effective flow area
    pub area: Area,
    /// Isentropic efficiency (0 < eta <= 1)
    pub eta: f64,
}

impl Turbine {
    /// Create a new turbine.
    ///
    /// # Arguments
    /// * `name` - Component identifier
    /// * `cd` - Discharge coefficient (typically 0.6-0.95)
    /// * `area` - Effective flow area
    /// * `eta` - Isentropic efficiency (0 < eta <= 1)
    ///
    /// # Errors
    /// Returns error if parameters are out of physical bounds.
    pub fn new(name: String, cd: f64, area: Area, eta: f64) -> ComponentResult<Self> {
        if eta <= 0.0 || eta > 1.0 {
            return Err(ComponentError::InvalidArg {
                what: "turbine efficiency must be in (0,1]",
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

        Ok(Self {
            name,
            cd,
            area,
            eta,
        })
    }

    /// Compute isentropic enthalpy drop using pressure ratio and fluid properties.
    fn isentropic_enthalpy_drop(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
    ) -> ComponentResult<f64> {
        let p_in = ports.inlet.pressure();
        let p_out = ports.outlet.pressure();
        let t_in = ports.inlet.temperature();

        if p_out.value >= p_in.value {
            // No expansion, no work
            return Ok(0.0);
        }

        let pr = p_out.value / p_in.value; // pressure ratio < 1

        // Get gas properties at inlet using property pack (Phase 11 optimization)
        // This batches cp and gamma queries into a single backend call
        let pack = fluid.property_pack(ports.inlet)?;
        let gamma = pack.gamma;
        let cp = pack.cp;

        // Isentropic temperature ratio: T_out_s/T_in = (P_out/P_in)^((γ-1)/γ)
        let exponent = (gamma - 1.0) / gamma;
        let t_ratio = pr.powf(exponent);
        let t_out_s = t_in.value * t_ratio;

        // Isentropic enthalpy drop (positive)
        let delta_h_s = cp * (t_in.value - t_out_s);

        Ok(delta_h_s.max(0.0))
    }
}

impl TwoPortComponent for Turbine {
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

        let dp = p_in.value - p_out.value;

        // Orifice-like flow characteristic
        let mdot_val = if dp > 1e-3 {
            self.cd * self.area.value * (2.0 * rho_up.value * dp).sqrt()
        } else if dp < -1e-3 {
            // Reverse flow (unusual for turbine but handle it)
            -self.cd * self.area.value * (2.0 * rho_up.value * (-dp)).sqrt()
        } else {
            0.0
        };

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
        let delta_h_s = self.isentropic_enthalpy_drop(fluid, ports)?;

        // Actual enthalpy drop accounting for efficiency
        let delta_h_actual = self.eta * delta_h_s;

        Ok(h_in - delta_h_actual)
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

        let delta_h_s = self.isentropic_enthalpy_drop(fluid, ports)?;

        // Power extracted from fluid (negative sign convention)
        let p_shaft = -mdot.value * self.eta * delta_h_s;

        Ok(Power::new::<uom::si::power::watt>(p_shaft))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::{k, m, pa};
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};

    #[test]
    fn turbine_creation() {
        let turbine = Turbine::new("test_turbine".to_string(), 0.85, m(0.01) * m(0.01), 0.85);
        assert!(turbine.is_ok());
    }

    #[test]
    fn turbine_invalid_efficiency() {
        let turbine = Turbine::new("bad_turbine".to_string(), 0.85, m(0.01) * m(0.01), 1.5);
        assert!(turbine.is_err());
    }

    #[test]
    fn turbine_mdot_with_pressure_drop() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(500_000.0),
                    t: k(400.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let turbine = Turbine::new("turbine".to_string(), 0.85, m(0.01) * m(0.01), 0.85).unwrap();

        let mdot = turbine.mdot(&fluid, ports).unwrap();

        // Should have positive flow with pressure drop
        assert!(mdot.value > 0.0);
    }

    #[test]
    fn turbine_shaft_power_negative() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(500_000.0),
                    t: k(400.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let turbine = Turbine::new("turbine".to_string(), 0.85, m(0.01) * m(0.01), 0.85).unwrap();

        let mdot = turbine.mdot(&fluid, ports).unwrap();
        let power = turbine.shaft_power(&fluid, ports, mdot).unwrap();

        // Turbine extracts power (negative)
        assert!(power.value < 0.0);
    }

    #[test]
    fn turbine_outlet_enthalpy_decreases() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = fluid
            .state(
                StateInput::PT {
                    p: pa(500_000.0),
                    t: k(400.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = fluid
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let turbine = Turbine::new("turbine".to_string(), 0.85, m(0.01) * m(0.01), 0.85).unwrap();

        let mdot = turbine.mdot(&fluid, ports).unwrap();
        let h_in = fluid.h(&state_in).unwrap();
        let h_out = turbine.outlet_enthalpy(&fluid, ports, mdot).unwrap();

        // Turbine extracts enthalpy
        assert!(h_out < h_in);
    }

    #[test]
    fn turbine_no_work_with_equal_pressures() {
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_both = fluid
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let ports = PortStates {
            inlet: &state_both,
            outlet: &state_both,
        };

        let turbine = Turbine::new("turbine".to_string(), 0.85, m(0.01) * m(0.01), 0.85).unwrap();

        let mdot = turbine.mdot(&fluid, ports).unwrap();
        let power = turbine.shaft_power(&fluid, ports, mdot).unwrap();

        // No pressure drop, no work
        assert!(power.value.abs() < 1.0); // Near zero
    }
}
