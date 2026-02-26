//! Core traits for component models.

use crate::error::{ComponentError, ComponentResult};
use tf_core::units::{MassRate, Pressure};
use tf_fluids::{FluidModel, SpecEnthalpy, ThermoState};

/// References to inlet and outlet thermodynamic states.
#[derive(Clone, Copy)]
pub struct PortStates<'a> {
    pub inlet: &'a ThermoState,
    pub outlet: &'a ThermoState,
}

/// Trait for two-port components that connect an inlet node to an outlet node.
///
/// Components are deterministic functions of state and parameters, suitable for
/// parallel evaluation and network solving.
pub trait TwoPortComponent: Send + Sync {
    /// Component name for debugging and identification.
    fn name(&self) -> &str;

    /// Compute mass flow rate from inlet to outlet given port states.
    ///
    /// Positive flow means inlet → outlet. Negative flow is allowed if pressure
    /// reverses and the component supports it.
    ///
    /// # Arguments
    /// * `fluid` - Fluid property model for computing densities, etc.
    /// * `ports` - Inlet and outlet thermodynamic states
    ///
    /// # Returns
    /// Mass flow rate in kg/s
    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate>;

    /// Optional: compute pressure drop for a given mass flow rate.
    ///
    /// Some components naturally compute mdot from ΔP (like orifices), while others
    /// naturally compute ΔP from mdot (like pipes). This method allows inverting
    /// the relationship when supported.
    ///
    /// Default implementation returns NotSupported.
    fn delta_p(
        &self,
        _fluid: &dyn FluidModel,
        _ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<Pressure> {
        Err(ComponentError::NotSupported {
            what: "delta_p not implemented for this component",
        })
    }

    /// Compute outlet specific enthalpy for energy balance.
    ///
    /// For isenthalpic components (orifices, valves, pipes), this returns the
    /// inlet enthalpy. For components with work or heat transfer (pumps, turbines,
    /// heat exchangers), this would compute modified enthalpy.
    ///
    /// # Arguments
    /// * `fluid` - Fluid property model
    /// * `ports` - Inlet and outlet thermodynamic states
    /// * `mdot` - Mass flow rate through component
    ///
    /// # Returns
    /// Specific enthalpy at outlet (J/kg)
    ///
    /// Default implementation returns NotSupported.
    fn outlet_enthalpy(
        &self,
        _fluid: &dyn FluidModel,
        _ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        Err(ComponentError::NotSupported {
            what: "outlet_enthalpy not implemented for this component",
        })
    }

    /// Compute shaft power transfer for components with rotating machinery.
    ///
    /// Sign convention:
    /// - Positive: power added TO fluid (pump consuming shaft power)
    /// - Negative: power extracted FROM fluid (turbine producing shaft power)
    ///
    /// For components without rotating machinery (orifices, valves, pipes),
    /// this returns 0 W by default.
    ///
    /// # Arguments
    /// * `fluid` - Fluid property model
    /// * `ports` - Inlet and outlet thermodynamic states
    /// * `mdot` - Mass flow rate through component
    ///
    /// # Returns
    /// Power transfer in watts (positive = fluid gains power)
    fn shaft_power(
        &self,
        _fluid: &dyn FluidModel,
        _ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<tf_core::units::Power> {
        Ok(tf_core::units::Power::new::<uom::si::power::watt>(0.0))
    }

    /// Compute heat transfer rate for components with thermal exchange.
    ///
    /// Sign convention:
    /// - Positive: heat added TO fluid
    /// - Negative: heat removed FROM fluid
    ///
    /// For adiabatic components (default), this returns 0 W.
    ///
    /// # Arguments
    /// * `fluid` - Fluid property model
    /// * `ports` - Inlet and outlet thermodynamic states
    /// * `mdot` - Mass flow rate through component
    ///
    /// # Returns
    /// Heat transfer rate in watts (positive = fluid gains heat)
    fn heat_rate(
        &self,
        _fluid: &dyn FluidModel,
        _ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<tf_core::units::Power> {
        Ok(tf_core::units::Power::new::<uom::si::power::watt>(0.0))
    }
}
