use crate::{Composition, FluidError, FluidModel, FluidResult, Species, StateInput};
use tf_core::units::{Density, Pressure, Temperature, Velocity, k, pa};
use uom::si::{
    mass_density::kilogram_per_cubic_meter, pressure::pascal, thermodynamic_temperature::kelvin,
    velocity::meter_per_second,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluidInputPair {
    PT,
    PH,
    RhoH,
    PS,
}

impl FluidInputPair {
    pub fn label(self) -> &'static str {
        match self {
            Self::PT => "P-T",
            Self::PH => "P-h",
            Self::RhoH => "rho-h",
            Self::PS => "P-s",
        }
    }

    pub fn first_label(self) -> &'static str {
        match self {
            Self::PT | Self::PH | Self::PS => "Pressure [Pa]",
            Self::RhoH => "Density [kg/m^3]",
        }
    }

    pub fn second_label(self) -> &'static str {
        match self {
            Self::PT => "Temperature [K]",
            Self::PH | Self::RhoH => "Enthalpy [J/kg]",
            Self::PS => "Entropy [J/(kg K)]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EquilibriumState {
    pub pressure: Pressure,
    pub temperature: Temperature,
    pub density: Density,
    pub enthalpy_j_per_kg: f64,
    pub entropy_j_per_kg_k: f64,
    pub cp_j_per_kg_k: f64,
    pub cv_j_per_kg_k: f64,
    pub gamma: f64,
    pub speed_of_sound: Velocity,
    pub phase: Option<String>,
    pub quality: Option<f64>,
}

impl EquilibriumState {
    pub fn pressure_pa(&self) -> f64 {
        self.pressure.get::<pascal>()
    }

    pub fn temperature_k(&self) -> f64 {
        self.temperature.get::<kelvin>()
    }

    pub fn density_kg_m3(&self) -> f64 {
        self.density.get::<kilogram_per_cubic_meter>()
    }

    pub fn speed_of_sound_m_s(&self) -> f64 {
        self.speed_of_sound.get::<meter_per_second>()
    }
}

pub fn compute_equilibrium_state(
    model: &dyn FluidModel,
    species: Species,
    pair: FluidInputPair,
    first: f64,
    second: f64,
) -> FluidResult<EquilibriumState> {
    if !first.is_finite() || !second.is_finite() {
        return Err(FluidError::InvalidArg {
            what: "input values must be finite",
        });
    }

    let composition = Composition::pure(species);
    if !model.supports_composition(&composition) {
        return Err(FluidError::NotSupported {
            what: "selected species is not supported by active fluid model",
        });
    }

    let input = match pair {
        FluidInputPair::PT => StateInput::PT {
            p: pa(first),
            t: k(second),
        },
        FluidInputPair::PH => StateInput::PH {
            p: pa(first),
            h: second,
        },
        FluidInputPair::RhoH => StateInput::RhoH {
            rho_kg_m3: first,
            h: second,
        },
        FluidInputPair::PS => StateInput::PS {
            p: pa(first),
            s: second,
        },
    };

    let state = model.state(input, composition)?;
    let pressure = state.pressure();
    let temperature = state.temperature();
    let density = model.rho(&state)?;
    let enthalpy_j_per_kg = model.h(&state)?;
    let entropy_j_per_kg_k = model.s(&state)?;
    let cp_j_per_kg_k = model.cp(&state)?;
    let cv_j_per_kg_k = model.cv(&state)?;
    let gamma = model.gamma(&state)?;
    let speed_of_sound = model.a(&state)?;

    Ok(EquilibriumState {
        pressure,
        temperature,
        density,
        enthalpy_j_per_kg,
        entropy_j_per_kg_k,
        cp_j_per_kg_k,
        cv_j_per_kg_k,
        gamma,
        speed_of_sound,
        phase: None,
        quality: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoolPropModel;

    #[test]
    fn compute_pt_state_works() {
        let model = CoolPropModel::new();
        let result =
            compute_equilibrium_state(&model, Species::N2, FluidInputPair::PT, 101325.0, 300.0)
                .expect("PT state should compute");
        assert!(result.density_kg_m3() > 0.0);
        assert!(result.cp_j_per_kg_k > result.cv_j_per_kg_k);
    }

    #[test]
    fn compute_ph_state_works() {
        let model = CoolPropModel::new();
        let result =
            compute_equilibrium_state(&model, Species::N2, FluidInputPair::PH, 101325.0, 311000.0)
                .expect("PH state should compute");
        assert!(result.temperature_k() > 0.0);
    }

    #[test]
    fn compute_rho_h_state_works() {
        let model = CoolPropModel::new();
        let pt =
            compute_equilibrium_state(&model, Species::N2, FluidInputPair::PT, 101325.0, 300.0)
                .expect("baseline PT state should compute");

        let rho_h = compute_equilibrium_state(
            &model,
            Species::N2,
            FluidInputPair::RhoH,
            pt.density_kg_m3(),
            pt.enthalpy_j_per_kg,
        )
        .expect("rho-h state should compute");

        assert!(rho_h.pressure_pa() > 0.0);
    }

    #[test]
    fn compute_ps_state_works() {
        let model = CoolPropModel::new();
        let baseline =
            compute_equilibrium_state(&model, Species::N2, FluidInputPair::PT, 101325.0, 300.0)
                .expect("baseline PT state should compute");

        let result = compute_equilibrium_state(
            &model,
            Species::N2,
            FluidInputPair::PS,
            baseline.pressure_pa(),
            baseline.entropy_j_per_kg_k,
        )
        .expect("P-s state should compute");

        assert!((result.temperature_k() - baseline.temperature_k()).abs() < 2.0);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let model = CoolPropModel::new();
        let err =
            compute_equilibrium_state(&model, Species::N2, FluidInputPair::PT, f64::NAN, 300.0)
                .expect_err("NaN should fail");

        assert!(matches!(err, FluidError::InvalidArg { .. }));
    }

    #[test]
    fn nitrous_oxide_pt_state_works() {
        let model = CoolPropModel::new();
        let result = compute_equilibrium_state(
            &model,
            Species::NitrousOxide,
            FluidInputPair::PT,
            5.0e6,
            290.0,
        )
        .expect("nitrous oxide PT state should compute");

        assert!(result.pressure_pa() > 0.0);
        assert!(result.temperature_k() > 0.0);
        assert!(result.density_kg_m3() > 0.0);
    }

    #[test]
    fn nitrous_oxide_ph_state_works() {
        let model = CoolPropModel::new();
        let baseline = compute_equilibrium_state(
            &model,
            Species::NitrousOxide,
            FluidInputPair::PT,
            2.5e6,
            300.0,
        )
        .expect("baseline nitrous state should compute");

        let solved = compute_equilibrium_state(
            &model,
            Species::NitrousOxide,
            FluidInputPair::PH,
            baseline.pressure_pa(),
            baseline.enthalpy_j_per_kg,
        )
        .expect("nitrous PH state should compute");

        assert!(solved.temperature_k() > 0.0);
    }
}
