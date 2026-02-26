//! Valve component with position control and opening laws.

use crate::error::ComponentResult;
use crate::orifice::Orifice;
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{Area, MassRate};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Valve opening characteristic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValveLaw {
    /// Effective area = area_max * position
    Linear,
    /// Effective area = area_max * position^2
    Quadratic,
}

/// Variable-area valve with position control.
///
/// Behaves like an orifice with effective area determined by position and valve law.
#[derive(Debug, Clone)]
pub struct Valve {
    name: String,
    /// Discharge coefficient
    pub cd: f64,
    /// Maximum flow area (fully open)
    pub area_max: Area,
    /// Valve position: 0.0 (closed) to 1.0 (fully open)
    pub position: f64,
    /// Valve opening characteristic
    pub law: ValveLaw,
    /// Treat flow as compressible
    pub treat_as_gas: bool,
}

impl Valve {
    /// Create a new valve with linear opening law.
    pub fn new(name: String, cd: f64, area_max: Area, position: f64) -> Self {
        Self {
            name,
            cd,
            area_max,
            position: position.clamp(0.0, 1.0),
            law: ValveLaw::Linear,
            treat_as_gas: false,
        }
    }

    /// Create a valve with specified law.
    pub fn with_law(mut self, law: ValveLaw) -> Self {
        self.law = law;
        self
    }

    /// Create a valve that treats flow as compressible.
    pub fn with_compressible(mut self) -> Self {
        self.treat_as_gas = true;
        self
    }

    /// Set valve position (clamped to 0..1).
    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.0, 1.0);
    }

    /// Compute effective area based on position and law.
    fn effective_area(&self) -> Area {
        let factor = match self.law {
            ValveLaw::Linear => self.position,
            ValveLaw::Quadratic => self.position * self.position,
        };

        use uom::si::area::square_meter;
        Area::new::<square_meter>(self.area_max.value * factor)
    }
}

impl TwoPortComponent for Valve {
    fn name(&self) -> &str {
        &self.name
    }

    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate> {
        // Delegate to orifice with effective area
        let eff_area = self.effective_area();

        let orifice = if self.treat_as_gas {
            Orifice::new_compressible(self.name.clone(), self.cd, eff_area)
        } else {
            Orifice::new(self.name.clone(), self.cd, eff_area)
        };

        orifice.mdot(fluid, ports)
    }

    fn outlet_enthalpy(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        // Isenthalpic (throttling) process: h_out = h_in
        Ok(fluid.h(ports.inlet)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::{k, pa};
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};
    use uom::si::area::square_meter;

    #[test]
    fn valve_closed_zero_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let valve = Valve::new("test".into(), 0.7, Area::new::<square_meter>(0.001), 0.0);

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = valve.mdot(&model, ports).unwrap();
        assert!(
            mdot.value.abs() < 1e-9,
            "Closed valve should have ~zero flow"
        );
    }

    #[test]
    fn valve_position_monotonic() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let positions = [0.0, 0.25, 0.5, 0.75, 1.0];
        let mut prev_mdot = 0.0;

        for &pos in &positions {
            let valve = Valve::new("test".into(), 0.7, Area::new::<square_meter>(0.001), pos);

            let ports = PortStates {
                inlet: &state_in,
                outlet: &state_out,
            };

            let mdot = valve.mdot(&model, ports).unwrap().value;
            assert!(mdot >= prev_mdot, "Flow should increase with position");
            prev_mdot = mdot;
        }
    }

    #[test]
    fn valve_law_quadratic_slower_opening() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let pos = 0.5;
        let valve_linear = Valve::new("linear".into(), 0.7, Area::new::<square_meter>(0.001), pos)
            .with_law(ValveLaw::Linear);

        let valve_quad = Valve::new("quad".into(), 0.7, Area::new::<square_meter>(0.001), pos)
            .with_law(ValveLaw::Quadratic);

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot_linear = valve_linear.mdot(&model, ports).unwrap().value;
        let mdot_quad = valve_quad.mdot(&model, ports).unwrap().value;

        assert!(
            mdot_quad < mdot_linear,
            "Quadratic valve should flow less at 50% position"
        );
    }
}
