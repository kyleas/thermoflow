//! Unit-aware numeric input system.
//!
//! Provides a reusable architecture for parsing and storing values with units.
//! Designed to be extensible for future expression-based inputs (e.g., `1psia + 2bar`).
//!
//! # Key Design
//!
//! - **Raw text**: User's input exactly as entered
//! - **Canonical value**: Parsed numeric in SI base units
//! - **Quantity type**: Temperature, Pressure, Density, etc.
//! - **Parser boundary**: Separated so future expression parsers can plug in cleanly
//!
//! # Future Extensibility
//!
//! The `parse_quantity` function can later be replaced/wrapped with an expression evaluator
//! without changing the widget or storage model.

use std::fmt;

/// Dimension/quantity family for a numeric input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    /// Temperature (canonical: Kelvin)
    Temperature,
    /// Absolute pressure (canonical: Pa)
    Pressure,
    /// Gauge pressure (canonical: Pa, but stored separately)
    PressureGauge,
    /// Density (canonical: kg/m^3)
    Density,
    /// Specific enthalpy / internal energy (canonical: J/kg)
    SpecificEnthalpy,
    /// Specific entropy / specific heat (canonical: J/(kg·K))
    SpecificEntropy,
    /// Quality/dryness fraction (canonical: 0-1)
    Quality,
    /// Dimensionless (canonical: as-is, but may include %)
    Dimensionless,
    /// Mass (canonical: kg)
    Mass,
    /// Force / weight (canonical: N)
    Force,
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temperature => write!(f, "Temperature"),
            Self::Pressure => write!(f, "Absolute Pressure"),
            Self::PressureGauge => write!(f, "Gauge Pressure"),
            Self::Density => write!(f, "Density"),
            Self::SpecificEnthalpy => write!(f, "Specific Enthalpy"),
            Self::SpecificEntropy => write!(f, "Specific Entropy"),
            Self::Quality => write!(f, "Quality"),
            Self::Dimensionless => write!(f, "Dimensionless"),
            Self::Mass => write!(f, "Mass"),
            Self::Force => write!(f, "Force"),
        }
    }
}

/// Error in unit parsing or conversion.
#[derive(Debug, Clone)]
pub enum UnitError {
    /// Input text did not parse to a number + optional unit
    ParseError(String),
    /// Unit not recognized for this quantity
    UnknownUnit { unit: String, quantity: String },
    /// Unit not allowed for this quantity (e.g., plain "psi" requires "psia" or "psig")
    AmbiguousUnit { unit: String, reason: String },
    /// Value out of physical range (e.g., negative absolute temperature)
    OutOfRange { value: f64, reason: String },
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::UnknownUnit { unit, quantity } => {
                write!(f, "Unknown unit '{}' for {}", unit, quantity)
            }
            Self::AmbiguousUnit { unit, reason } => {
                write!(f, "Ambiguous unit '{}': {}", unit, reason)
            }
            Self::OutOfRange { value, reason } => {
                write!(f, "Value {} out of range: {}", value, reason)
            }
        }
    }
}

impl std::error::Error for UnitError {}

/// Stores a value with its original user input and canonical SI representation.
///
/// This design preserves user text while providing a canonical parsed value for computation.
#[derive(Debug, Clone)]
pub struct UnitValue {
    /// Raw text as the user entered it (e.g., "14.7 psia", "70°F")
    pub raw_text: String,
    /// Canonical value in SI/base units for computation
    pub canonical_si: f64,
    /// Associated quantity type
    pub quantity: Quantity,
}

impl UnitValue {
    /// Create a new unit value from raw text, parsing and converting to canonical SI form.
    pub fn from_text(raw_text: impl Into<String>, quantity: Quantity) -> Result<Self, UnitError> {
        let text = raw_text.into();
        let canonical_si = parse_quantity(&text, quantity)?;
        Ok(Self {
            raw_text: text,
            canonical_si,
            quantity,
        })
    }

    /// Get the canonical SI value for this quantity.
    pub fn si_value(&self) -> f64 {
        self.canonical_si
    }

    /// Replace the raw text and re-parse, returning error if parsing fails.
    pub fn update_text(&mut self, new_text: impl Into<String>) -> Result<(), UnitError> {
        let text = new_text.into();
        let canonical_si = parse_quantity(&text, self.quantity)?;
        self.raw_text = text;
        self.canonical_si = canonical_si;
        Ok(())
    }
}

/// Parse a quantity value from user input text.
///
/// Handles unit tags and converts to canonical SI units.
///
/// # Arguments
///
/// - `raw_text`: User input (e.g., "70F", "14.7 psia", "50%")
/// - `quantity`: Dimension/quantity family for validation
///
/// # Design Note
///
/// This function is the **parsing boundary** for future expression support.
/// To implement expression parsing (e.g., `1psia + 2bar`), replace this function's
/// implementation or wrap it with an expression evaluator that ultimately calls
/// this same signature for each parsed sub-expression.
///
/// The rest of the system (widget, storage, etc.) should remain unchanged,
/// making the upgrade path clean.
pub fn parse_quantity(raw_text: &str, quantity: Quantity) -> Result<f64, UnitError> {
    let trimmed = raw_text.trim();

    match quantity {
        Quantity::Temperature => parse_temperature(trimmed),
        Quantity::Pressure => parse_pressure(trimmed, false),
        Quantity::PressureGauge => parse_pressure(trimmed, true),
        Quantity::Density => parse_density(trimmed),
        Quantity::SpecificEnthalpy => parse_specific_enthalpy(trimmed),
        Quantity::SpecificEntropy => parse_specific_entropy(trimmed),
        Quantity::Quality => parse_quality(trimmed),
        Quantity::Dimensionless => parse_dimensionless(trimmed),
        Quantity::Mass => parse_mass(trimmed),
        Quantity::Force => parse_force(trimmed),
    }
}

/// Parse temperature in various units, return Kelvin.
fn parse_temperature(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let kelvin = match unit.to_lowercase().as_str() {
        "k" | "kelvin" => value,
        "c" | "°c" | "celsius" => value + 273.15,
        "f" | "°f" | "fahrenheit" => (value + 459.67) * 5.0 / 9.0,
        "r" | "°r" | "rankine" => value * 5.0 / 9.0,
        "" if !input.is_empty() && input.ends_with(|c: char| c.is_alphabetic() || c == '°') => {
            // Try to extract unit from input if parsing with default unit
            return Err(UnitError::ParseError(format!(
                "Could not parse temperature from '{}'",
                input
            )));
        }
        // Default to Kelvin if no unit specified
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Temperature".to_string(),
            })
        }
    };

    if kelvin <= 0.0 {
        return Err(UnitError::OutOfRange {
            value: kelvin,
            reason: "Absolute temperature must be > 0 K".to_string(),
        });
    }

    Ok(kelvin)
}

/// Parse pressure in various units, return Pa (absolute).
/// If gauge=true, still converts gauge pressure to absolute assumes 1 atm reference.
fn parse_pressure(input: &str, _gauge: bool) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let pa = match unit.to_lowercase().as_str() {
        "pa" | "pascal" => value,
        "kpa" => value * 1e3,
        "mpa" => value * 1e6,
        "bar" => value * 1e5, // bar treated as absolute
        "mbar" | "millibar" => value * 100.0, // mbar treated as absolute
        "atm" => value * 101_325.0,
        "torr" => value * 133.322,
        "psia" => value * 6_894.76, // pounds absolute
        "psig" => (value + 14.696) * 6_894.76, // gauge -> absolute
        "ksia" => value * 6.894_76e6, // kilo-pounds absolute
        "ksig" => (value + 14.696) * 6.894_76e6, // gauge -> absolute (simplified)
        "barg" => (value + 1.0) * 1e5, // gauge -> absolute
        "kpag" => (value + 101.325) * 1e3, // gauge -> absolute
        "mpag" => (value + 0.101_325) * 1e6, // gauge -> absolute
        // Explicitly reject ambiguous units
        "psi" => {
            return Err(UnitError::AmbiguousUnit {
                unit: "psi".to_string(),
                reason: "Use 'psia' (absolute) or 'psig' (gauge)".to_string(),
            })
        }
        "ksi" => {
            return Err(UnitError::AmbiguousUnit {
                unit: "ksi".to_string(),
                reason: "Use 'ksia' (absolute) or 'ksig' (gauge)".to_string(),
            })
        }
        "" => value, // Default to Pa
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Pressure".to_string(),
            })
        }
    };

    if pa < 0.0 {
        return Err(UnitError::OutOfRange {
            value: pa,
            reason: "Absolute pressure cannot be negative".to_string(),
        });
    }

    Ok(pa)
}

/// Parse density in various units, return kg/m^3.
fn parse_density(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let kg_m3 = match unit.to_lowercase().as_str() {
        "kg/m^3" | "kg/m³" | "kg/m3" => value,
        "g/cm^3" | "g/cm³" | "g/cm3" => value * 1e3,
        "lbm/ft^3" | "lbm/ft3" => value * 16.018_46,
        "lbm/in^3" | "lbm/in3" => value * 27_679.9,
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Density".to_string(),
            })
        }
    };

    if kg_m3 <= 0.0 {
        return Err(UnitError::OutOfRange {
            value: kg_m3,
            reason: "Density must be positive".to_string(),
        });
    }

    Ok(kg_m3)
}

/// Parse specific enthalpy/internal energy in various units, return J/kg.
fn parse_specific_enthalpy(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let j_kg = match unit.to_lowercase().as_str() {
        "j/kg" => value,
        "kj/kg" => value * 1e3,
        "mj/kg" => value * 1e6,
        "btu/lbm" => value * 2_326.0,
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Specific Enthalpy".to_string(),
            })
        }
    };

    Ok(j_kg)
}

/// Parse specific entropy/specific heat, return J/(kg·K).
fn parse_specific_entropy(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let j_kg_k = match unit.to_lowercase().as_str() {
        "j/(kg·k)" | "j/(kg k)" | "j/(kg*k)" => value,
        "kj/(kg·k)" | "kj/(kg k)" | "kj/(kg*k)" => value * 1e3,
        "btu/(lbm·r)" | "btu/(lbm r)" | "btu/(lbm*r)" => value * 4_186.8,
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Specific Entropy".to_string(),
            })
        }
    };

    Ok(j_kg_k)
}

/// Parse quality (0-1), accepting percent notation.
fn parse_quality(input: &str) -> Result<f64, UnitError> {
    let trimmed = input.trim();
    let quality = if trimmed.ends_with('%') {
        let num_str = trimmed.trim_end_matches('%').trim();
        let percent: f64 = num_str.parse().map_err(|_| UnitError::ParseError(
            format!("Could not parse quality from '{}'", input),
        ))?;
        percent / 100.0
    } else {
        trimmed.parse().map_err(|_| UnitError::ParseError(
            format!("Could not parse quality from '{}'", input),
        ))?
    };

    if !(0.0..=1.0).contains(&quality) {
        return Err(UnitError::OutOfRange {
            value: quality,
            reason: "Quality must be between 0 and 1".to_string(),
        });
    }

    Ok(quality)
}

/// Parse dimensionless, accepting plain numbers or percent.
fn parse_dimensionless(input: &str) -> Result<f64, UnitError> {
    let trimmed = input.trim();
    let value = if trimmed.ends_with('%') {
        let num_str = trimmed.trim_end_matches('%').trim();
        let percent: f64 = num_str.parse().map_err(|_| UnitError::ParseError(
            format!("Could not parse dimensionless value from '{}'", input),
        ))?;
        percent / 100.0
    } else {
        trimmed.parse().map_err(|_| UnitError::ParseError(
            format!("Could not parse dimensionless value from '{}'", input),
        ))?
    };

    Ok(value)
}

/// Parse mass in various units, return kg.
fn parse_mass(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let kg = match unit.to_lowercase().as_str() {
        "kg" => value,
        "g" => value / 1e3,
        "lbm" => value * 0.453_592,
        "slug" => value * 14.593_9,
        // Reject ambiguous "lb"
        "lb" => {
            return Err(UnitError::AmbiguousUnit {
                unit: "lb".to_string(),
                reason: "Use 'lbm' for mass".to_string(),
            })
        }
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Mass".to_string(),
            })
        }
    };

    if kg < 0.0 {
        return Err(UnitError::OutOfRange {
            value: kg,
            reason: "Mass cannot be negative".to_string(),
        });
    }

    Ok(kg)
}

/// Parse force/weight in various units, return N.
fn parse_force(input: &str) -> Result<f64, UnitError> {
    let (value, unit) = split_value_and_unit(input)?;

    let n = match unit.to_lowercase().as_str() {
        "n" | "newton" => value,
        "kn" => value * 1e3,
        "lbf" => value * 4.448_22,
        "lbw" => value * 4.448_22, // weight ~ force on Earth
        "kgf" => value * 9.806_65,
        // Reject ambiguous "lb"
        "lb" => {
            return Err(UnitError::AmbiguousUnit {
                unit: "lb".to_string(),
                reason: "Use 'lbf' (force) or 'lbw' (weight)".to_string(),
            })
        }
        "" => value,
        _ => {
            return Err(UnitError::UnknownUnit {
                unit: unit.to_string(),
                quantity: "Force".to_string(),
            })
        }
    };

    Ok(n)
}

/// Split a value+unit string into (numeric_value, unit_string).
///
/// Examples:
/// - "70F" -> (70.0, "F")
/// - "14.7 psia" -> (14.7, "psia")
/// - "300" -> (300.0, "")
fn split_value_and_unit(input: &str) -> Result<(f64, String), UnitError> {
    let trimmed = input.trim();

    // Find where the numeric part ends
    let split_idx = trimmed
        .find(|c: char| !c.is_numeric() && c != '.' && c != '-' && c != '+' && c != 'e' && c != 'E')
        .unwrap_or(trimmed.len());

    let (num_part, unit_part) = trimmed.split_at(split_idx);
    let num_part = num_part.trim();
    let unit_part = unit_part.trim();

    let value: f64 = num_part.parse().map_err(|_| UnitError::ParseError(
        format!("Could not parse numeric value from '{}'", input),
    ))?;

    Ok((value, unit_part.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kelvin() {
        assert_eq!(parse_temperature("300 K").unwrap(), 300.0);
        assert_eq!(parse_temperature("300K").unwrap(), 300.0);
    }

    #[test]
    fn parse_celsius() {
        let c0 = parse_temperature("0C").unwrap();
        assert!((c0 - 273.15).abs() < 0.01);
    }

    #[test]
    fn parse_fahrenheit() {
        let f32 = parse_temperature("32F").unwrap();
        assert!((f32 - 273.15).abs() < 1.0);
    }

    #[test]
    fn reject_negative_temperature() {
        assert!(parse_temperature("-100K").is_err());
    }

    #[test]
    fn parse_pressure_absolute() {
        assert_eq!(parse_pressure("101325 Pa", false).unwrap(), 101325.0);
        assert_eq!(parse_pressure("1 bar", false).unwrap(), 1e5);
        let _ = parse_pressure("14.7 psia", false).unwrap();
    }

    #[test]
    fn reject_plain_psi() {
        assert!(matches!(
            parse_pressure("14.7 psi", false),
            Err(UnitError::AmbiguousUnit { unit, .. }) if unit == "psi"
        ));
    }

    #[test]
    fn reject_plain_ksi() {
        assert!(matches!(
            parse_pressure("1 ksi", false),
            Err(UnitError::AmbiguousUnit { unit, .. }) if unit == "ksi"
        ));
    }

    #[test]
    fn test_parse_density() {
        assert_eq!(parse_density("1000 kg/m^3").unwrap(), 1000.0);
        let g_cm3 = parse_density("1 g/cm^3").unwrap();
        assert!((g_cm3 - 1000.0).abs() < 1.0);
    }

    #[test]
    fn test_parse_quality() {
        assert_eq!(parse_quality("0.5").unwrap(), 0.5);
        assert_eq!(parse_quality("50%").unwrap(), 0.5);
        assert!(parse_quality("1.5").is_err());
    }

    #[test]
    fn test_parse_dimensionless() {
        assert_eq!(parse_dimensionless("0.75").unwrap(), 0.75);
        assert_eq!(parse_dimensionless("75%").unwrap(), 0.75);
    }

    #[test]
    fn test_reject_plain_lb_for_mass() {
        assert!(matches!(
            parse_mass("5 lb"),
            Err(UnitError::AmbiguousUnit { unit, .. }) if unit == "lb"
        ));
    }

    #[test]
    fn test_parse_lbm_for_mass() {
        let _ = parse_mass("5 lbm").unwrap();
    }

    #[test]
    fn test_reject_plain_lb_for_force() {
        assert!(matches!(
            parse_force("100 lb"),
            Err(UnitError::AmbiguousUnit { unit, .. }) if unit == "lb"
        ));
    }

    #[test]
    fn test_parse_lbf_for_force() {
        let _ = parse_force("100 lbf").unwrap();
    }

    #[test]
    fn unit_value_roundtrip() {
        let uv = UnitValue::from_text("70F", Quantity::Temperature).unwrap();
        assert_eq!(uv.raw_text, "70F");
        assert!(uv.canonical_si > 290.0 && uv.canonical_si < 295.0);
    }
}
