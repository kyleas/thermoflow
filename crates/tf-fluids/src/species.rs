//! Chemical species definitions.

/// Chemical species relevant for rocket propulsion and thermodynamic systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    /// Oxygen (O₂)
    O2,
    /// Methane (CH₄)
    CH4,
    /// Hydrogen (H₂)
    H2,
    /// Helium (He)
    He,
    /// Nitrogen (N₂)
    N2,
    /// Argon (Ar)
    Ar,
    /// Carbon dioxide (CO₂)
    CO2,
    /// Carbon monoxide (CO)
    CO,
    /// Water (H₂O)
    H2O,
    /// RP-1 kerosene (placeholder, not supported by CoolProp)
    RP1,
}

impl Species {
    /// Get CoolProp fluid name for this species.
    ///
    /// Returns `None` for species not supported by CoolProp (e.g., RP-1).
    pub fn coolprop_name(&self) -> Option<&'static str> {
        match self {
            Species::O2 => Some("Oxygen"),
            Species::CH4 => Some("Methane"),
            Species::H2 => Some("Hydrogen"),
            Species::He => Some("Helium"),
            Species::N2 => Some("Nitrogen"),
            Species::Ar => Some("Argon"),
            Species::CO2 => Some("CarbonDioxide"),
            Species::CO => Some("CarbonMonoxide"),
            Species::H2O => Some("Water"),
            Species::RP1 => None, // Not supported by CoolProp
        }
    }

    /// Map to rfluids Pure enum (internal use for CoolProp backend).
    ///
    /// Returns `None` for species not available in rfluids.
    pub(crate) fn rfluids_pure(&self) -> Option<rfluids::substance::Pure> {
        use rfluids::substance::Pure;
        match self {
            Species::O2 => Some(Pure::Oxygen),
            Species::CH4 => Some(Pure::Methane),
            Species::H2 => Some(Pure::Hydrogen),
            Species::He => Some(Pure::Helium),
            Species::N2 => Some(Pure::Nitrogen),
            Species::Ar => Some(Pure::Argon),
            Species::CO2 => Some(Pure::CarbonDioxide),
            Species::CO => Some(Pure::CarbonMonoxide),
            Species::H2O => Some(Pure::Water),
            Species::RP1 => None,
        }
    }

    /// Get human-readable name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Species::O2 => "Oxygen",
            Species::CH4 => "Methane",
            Species::H2 => "Hydrogen",
            Species::He => "Helium",
            Species::N2 => "Nitrogen",
            Species::Ar => "Argon",
            Species::CO2 => "Carbon Dioxide",
            Species::CO => "Carbon Monoxide",
            Species::H2O => "Water",
            Species::RP1 => "RP-1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coolprop_mapping() {
        assert_eq!(Species::O2.coolprop_name(), Some("Oxygen"));
        assert_eq!(Species::CH4.coolprop_name(), Some("Methane"));
        assert_eq!(Species::H2.coolprop_name(), Some("Hydrogen"));
        assert_eq!(Species::He.coolprop_name(), Some("Helium"));
        assert_eq!(Species::N2.coolprop_name(), Some("Nitrogen"));
        assert_eq!(Species::Ar.coolprop_name(), Some("Argon"));
        assert_eq!(Species::CO2.coolprop_name(), Some("CarbonDioxide"));
        assert_eq!(Species::CO.coolprop_name(), Some("CarbonMonoxide"));
        assert_eq!(Species::H2O.coolprop_name(), Some("Water"));
        assert_eq!(Species::RP1.coolprop_name(), None);
    }

    #[test]
    fn display_names() {
        assert_eq!(Species::O2.display_name(), "Oxygen");
        assert_eq!(Species::RP1.display_name(), "RP-1");
    }

    #[test]
    fn species_equality() {
        assert_eq!(Species::H2O, Species::H2O);
        assert_ne!(Species::H2O, Species::H2);
    }

    #[test]
    fn rfluids_mapping() {
        // Just verify it compiles and returns Some/None appropriately
        assert!(Species::O2.rfluids_pure().is_some());
        assert!(Species::H2O.rfluids_pure().is_some());
        assert!(Species::RP1.rfluids_pure().is_none());
    }
}
