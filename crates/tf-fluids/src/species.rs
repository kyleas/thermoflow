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
    /// Air (pseudo-pure backend fluid)
    Air,
    /// Nitrous oxide (N₂O)
    NitrousOxide,
    /// Ammonia (NH₃)
    Ammonia,
    /// Ethane
    Ethane,
    /// Ethylene
    Ethylene,
    /// Propane
    Propane,
    /// Propylene
    Propylene,
    /// n-Butane
    NButane,
    /// Isobutane
    Isobutane,
    /// n-Pentane
    NPentane,
    /// Isopentane
    Isopentane,
    /// n-Hexane
    NHexane,
    /// Neon
    Neon,
    /// Krypton
    Krypton,
    /// Xenon
    Xenon,
    /// Sulfur dioxide (SO₂)
    SulfurDioxide,
    /// Refrigerant R32
    R32,
    /// Refrigerant R125
    R125,
    /// Refrigerant R134a
    R134a,
    /// Refrigerant R152a
    R152a,
    /// Refrigerant R245fa
    R245fa,
    /// Refrigerant R1234yf
    R1234yf,
    /// RP-1 kerosene (placeholder, not supported by CoolProp)
    RP1,
}

impl Species {
    pub const ALL: [Species; 32] = [
        Species::O2,
        Species::CH4,
        Species::H2,
        Species::He,
        Species::N2,
        Species::Ar,
        Species::CO2,
        Species::CO,
        Species::H2O,
        Species::Air,
        Species::NitrousOxide,
        Species::Ammonia,
        Species::Ethane,
        Species::Ethylene,
        Species::Propane,
        Species::Propylene,
        Species::NButane,
        Species::Isobutane,
        Species::NPentane,
        Species::Isopentane,
        Species::NHexane,
        Species::Neon,
        Species::Krypton,
        Species::Xenon,
        Species::SulfurDioxide,
        Species::R32,
        Species::R125,
        Species::R134a,
        Species::R152a,
        Species::R245fa,
        Species::R1234yf,
        Species::RP1,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            Species::O2 => "O2",
            Species::CH4 => "CH4",
            Species::H2 => "H2",
            Species::He => "He",
            Species::N2 => "N2",
            Species::Ar => "Ar",
            Species::CO2 => "CO2",
            Species::CO => "CO",
            Species::H2O => "H2O",
            Species::Air => "Air",
            Species::NitrousOxide => "N2O",
            Species::Ammonia => "NH3",
            Species::Ethane => "Ethane",
            Species::Ethylene => "Ethylene",
            Species::Propane => "Propane",
            Species::Propylene => "Propylene",
            Species::NButane => "nButane",
            Species::Isobutane => "Isobutane",
            Species::NPentane => "nPentane",
            Species::Isopentane => "Isopentane",
            Species::NHexane => "nHexane",
            Species::Neon => "Ne",
            Species::Krypton => "Kr",
            Species::Xenon => "Xe",
            Species::SulfurDioxide => "SO2",
            Species::R32 => "R32",
            Species::R125 => "R125",
            Species::R134a => "R134a",
            Species::R152a => "R152a",
            Species::R245fa => "R245fa",
            Species::R1234yf => "R1234yf",
            Species::RP1 => "RP1",
        }
    }
}

impl std::str::FromStr for Species {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "O2" | "OXYGEN" => Ok(Species::O2),
            "CH4" | "METHANE" => Ok(Species::CH4),
            "H2" | "HYDROGEN" => Ok(Species::H2),
            "HE" | "HELIUM" => Ok(Species::He),
            "N2" | "NITROGEN" => Ok(Species::N2),
            "AR" | "ARGON" => Ok(Species::Ar),
            "CO2" | "CARBONDIOXIDE" | "CARBON DIOXIDE" => Ok(Species::CO2),
            "CO" | "CARBONMONOXIDE" | "CARBON MONOXIDE" => Ok(Species::CO),
            "H2O" | "WATER" => Ok(Species::H2O),
            "AIR" => Ok(Species::Air),
            "N2O" | "NITROUSOXIDE" | "NITROUS OXIDE" => Ok(Species::NitrousOxide),
            "NH3" | "AMMONIA" => Ok(Species::Ammonia),
            "ETHANE" => Ok(Species::Ethane),
            "ETHYLENE" => Ok(Species::Ethylene),
            "PROPANE" | "C3H8" => Ok(Species::Propane),
            "PROPYLENE" | "C3H6" => Ok(Species::Propylene),
            "NBUTANE" | "N-BUTANE" | "BUTANE" => Ok(Species::NButane),
            "ISOBUTANE" | "I-BUTANE" => Ok(Species::Isobutane),
            "NPENTANE" | "N-PENTANE" | "PENTANE" => Ok(Species::NPentane),
            "ISOPENTANE" | "I-PENTANE" => Ok(Species::Isopentane),
            "NHEXANE" | "N-HEXANE" | "HEXANE" => Ok(Species::NHexane),
            "NE" | "NEON" => Ok(Species::Neon),
            "KR" | "KRYPTON" => Ok(Species::Krypton),
            "XE" | "XENON" => Ok(Species::Xenon),
            "SO2" | "SULFURDIOXIDE" | "SULFUR DIOXIDE" => Ok(Species::SulfurDioxide),
            "R32" => Ok(Species::R32),
            "R125" => Ok(Species::R125),
            "R134A" => Ok(Species::R134a),
            "R152A" => Ok(Species::R152a),
            "R245FA" => Ok(Species::R245fa),
            "R1234YF" => Ok(Species::R1234yf),
            "RP1" | "RP-1" => Ok(Species::RP1),
            _ => Err("unknown species"),
        }
    }
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
            Species::Air => Some("Air"),
            Species::NitrousOxide => Some("NitrousOxide"),
            Species::Ammonia => Some("Ammonia"),
            Species::Ethane => Some("Ethane"),
            Species::Ethylene => Some("Ethylene"),
            Species::Propane => Some("n-Propane"),
            Species::Propylene => Some("Propylene"),
            Species::NButane => Some("n-Butane"),
            Species::Isobutane => Some("Isobutane"),
            Species::NPentane => Some("n-Pentane"),
            Species::Isopentane => Some("Isopentane"),
            Species::NHexane => Some("n-Hexane"),
            Species::Neon => Some("Neon"),
            Species::Krypton => Some("Krypton"),
            Species::Xenon => Some("Xenon"),
            Species::SulfurDioxide => Some("SulfurDioxide"),
            Species::R32 => Some("R32"),
            Species::R125 => Some("R125"),
            Species::R134a => Some("R134a"),
            Species::R152a => Some("R152a"),
            Species::R245fa => Some("R245fa"),
            Species::R1234yf => Some("R1234yf"),
            Species::RP1 => None,
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
            Species::Air => Some(Pure::Air),
            Species::NitrousOxide => Some(Pure::NitrousOxide),
            Species::Ammonia => Some(Pure::Ammonia),
            Species::Ethane => Some(Pure::Ethane),
            Species::Ethylene => Some(Pure::Ethylene),
            Species::Propane => Some(Pure::nPropane),
            Species::Propylene => Some(Pure::Propylene),
            Species::NButane => Some(Pure::nButane),
            Species::Isobutane => Some(Pure::Isobutane),
            Species::NPentane => Some(Pure::nPentane),
            Species::Isopentane => Some(Pure::Isopentane),
            Species::NHexane => Some(Pure::nHexane),
            Species::Neon => Some(Pure::Neon),
            Species::Krypton => Some(Pure::Krypton),
            Species::Xenon => Some(Pure::Xenon),
            Species::SulfurDioxide => Some(Pure::SulfurDioxide),
            Species::R32 => Some(Pure::R32),
            Species::R125 => Some(Pure::R125),
            Species::R134a => Some(Pure::R134a),
            Species::R152a => Some(Pure::R152a),
            Species::R245fa => Some(Pure::R245fa),
            Species::R1234yf => Some(Pure::R1234yf),
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
            Species::Air => "Air",
            Species::NitrousOxide => "Nitrous Oxide",
            Species::Ammonia => "Ammonia",
            Species::Ethane => "Ethane",
            Species::Ethylene => "Ethylene",
            Species::Propane => "Propane",
            Species::Propylene => "Propylene",
            Species::NButane => "n-Butane",
            Species::Isobutane => "Isobutane",
            Species::NPentane => "n-Pentane",
            Species::Isopentane => "Isopentane",
            Species::NHexane => "n-Hexane",
            Species::Neon => "Neon",
            Species::Krypton => "Krypton",
            Species::Xenon => "Xenon",
            Species::SulfurDioxide => "Sulfur Dioxide",
            Species::R32 => "R32",
            Species::R125 => "R125",
            Species::R134a => "R134a",
            Species::R152a => "R152a",
            Species::R245fa => "R245fa",
            Species::R1234yf => "R1234yf",
            Species::RP1 => "RP-1",
        }
    }

    /// Get molar mass [kg/kmol] for this species.
    ///
    /// Values sourced from standard reference data (e.g., NIST).
    pub fn molar_mass(&self) -> f64 {
        match self {
            Species::O2 => 31.999,
            Species::CH4 => 16.043,
            Species::H2 => 2.016,
            Species::He => 4.003,
            Species::N2 => 28.014,
            Species::Ar => 39.948,
            Species::CO2 => 44.010,
            Species::CO => 28.010,
            Species::H2O => 18.015,
            Species::Air => 28.965,
            Species::NitrousOxide => 44.013,
            Species::Ammonia => 17.031,
            Species::Ethane => 30.070,
            Species::Ethylene => 28.054,
            Species::Propane => 44.097,
            Species::Propylene => 42.081,
            Species::NButane => 58.124,
            Species::Isobutane => 58.124,
            Species::NPentane => 72.151,
            Species::Isopentane => 72.151,
            Species::NHexane => 86.178,
            Species::Neon => 20.180,
            Species::Krypton => 83.798,
            Species::Xenon => 131.293,
            Species::SulfurDioxide => 64.066,
            Species::R32 => 52.024,
            Species::R125 => 120.022,
            Species::R134a => 102.031,
            Species::R152a => 66.051,
            Species::R245fa => 134.048,
            Species::R1234yf => 114.042,
            Species::RP1 => 170.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coolprop_mapping() {
        assert_eq!(Species::O2.coolprop_name(), Some("Oxygen"));
        assert_eq!(Species::NitrousOxide.coolprop_name(), Some("NitrousOxide"));
        assert_eq!(Species::R134a.coolprop_name(), Some("R134a"));
        assert_eq!(Species::RP1.coolprop_name(), None);
    }

    #[test]
    fn parse_aliases_include_nitrous_oxide() {
        assert_eq!("N2O".parse::<Species>().unwrap(), Species::NitrousOxide);
        assert_eq!(
            "Nitrous Oxide".parse::<Species>().unwrap(),
            Species::NitrousOxide
        );
        assert_eq!("NH3".parse::<Species>().unwrap(), Species::Ammonia);
    }

    #[test]
    fn display_names() {
        assert_eq!(Species::NitrousOxide.display_name(), "Nitrous Oxide");
        assert_eq!(Species::RP1.display_name(), "RP-1");
    }

    #[test]
    fn rfluids_mapping() {
        assert!(Species::NitrousOxide.rfluids_pure().is_some());
        assert!(Species::R245fa.rfluids_pure().is_some());
        assert!(Species::RP1.rfluids_pure().is_none());
    }

    #[test]
    fn canonical_key_roundtrip_for_nitrous() {
        let species = Species::NitrousOxide;
        let parsed = species
            .key()
            .parse::<Species>()
            .expect("canonical key should parse");
        assert_eq!(parsed, species);
    }
}
