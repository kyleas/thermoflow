//! Control volume initialization validation and mode inference.
//!
//! This module provides utilities to:
//! - Validate control volume initial conditions
//! - Infer initialization mode from optional fields (backward compat)
//! - Compute derived thermodynamic values from explicit modes

use crate::schema::InitialCvDef;

/// A validated, explicit CV initialization mode.
///
/// Names use physics notation: 'm' refers to mass (thermodynamic convention).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub enum CvInitMode {
    /// Pressure and temperature specified; mass and enthalpy will be computed.
    PT { p_pa: f64, t_k: f64 },

    /// Pressure and enthalpy specified; mass and temperature will be computed.
    PH { p_pa: f64, h_j_per_kg: f64 },

    /// Mass and temperature specified; pressure and enthalpy will be computed.
    mT { m_kg: f64, t_k: f64 },

    /// Mass and enthalpy specified; pressure and temperature will be computed.
    mH { m_kg: f64, h_j_per_kg: f64 },
}

impl CvInitMode {
    /// Attempt to infer a mode from partial optional fields.
    ///
    /// Returns `Some(mode)` if a clear, unambiguous mode can be inferred.
    /// Returns `None` if the specification is ambiguous or incomplete.
    pub fn infer(def: &InitialCvDef) -> Option<CvInitMode> {
        match (def.p_pa, def.t_k, def.h_j_per_kg, def.m_kg) {
            // Explicit modes (unambiguous):
            (Some(p), Some(t), None, None) => Some(CvInitMode::PT { p_pa: p, t_k: t }),
            (Some(p), None, Some(h), None) => Some(CvInitMode::PH {
                p_pa: p,
                h_j_per_kg: h,
            }),
            (None, Some(t), None, Some(m)) => Some(CvInitMode::mT { m_kg: m, t_k: t }),
            (None, None, Some(h), Some(m)) => Some(CvInitMode::mH {
                m_kg: m,
                h_j_per_kg: h,
            }),

            // Over-constrained or ambiguous: these need validation
            _ => None,
        }
    }

    /// Validate and return a mode from the given definition.
    ///
    /// If `def.mode` is explicitly specified, use it directly.
    /// Otherwise, try to infer a mode.
    ///
    /// Returns an error message if the definition is invalid or ambiguous.
    pub fn from_def(def: &InitialCvDef, node_id: &str) -> Result<CvInitMode, String> {
        // If explicit mode is provided, validate it
        if let Some(mode_str) = &def.mode {
            return CvInitMode::from_explicit_mode(mode_str, def, node_id);
        }

        // Try to infer a mode from optional fields
        CvInitMode::infer(def).ok_or_else(|| {
            format!(
                "Control volume '{}' has invalid or over-constrained initial conditions. \
                 Either:\n\
                 1. Specify explicit mode: mode: PT (with p_pa, t_k) or PH/mT/mH, OR\n\
                 2. Fix ambiguous specification:\n\
                   p_pa={:?}, t_k={:?}, h={:?}, m_kg={:?}\n\
                 Hint: PT mode computes mass from (P,T,V). PH mode computes mass from (P,h,V). \
                 If you want to specify both T and m, use mT mode (pressure will be computed).",
                node_id, def.p_pa, def.t_k, def.h_j_per_kg, def.m_kg
            )
        })
    }

    /// Resolve an explicit mode string with the definition parameters.
    fn from_explicit_mode(
        mode_str: &str,
        def: &InitialCvDef,
        node_id: &str,
    ) -> Result<CvInitMode, String> {
        match mode_str.to_uppercase().as_str() {
            "PT" => {
                let p = def
                    .p_pa
                    .ok_or_else(|| format!("CV '{}' PT mode requires p_pa", node_id))?;
                let t = def
                    .t_k
                    .ok_or_else(|| format!("CV '{}' PT mode requires t_k", node_id))?;
                Ok(CvInitMode::PT { p_pa: p, t_k: t })
            }
            "PH" => {
                let p = def
                    .p_pa
                    .ok_or_else(|| format!("CV '{}' PH mode requires p_pa", node_id))?;
                let h = def
                    .h_j_per_kg
                    .ok_or_else(|| format!("CV '{}' PH mode requires h_j_per_kg", node_id))?;
                Ok(CvInitMode::PH {
                    p_pa: p,
                    h_j_per_kg: h,
                })
            }
            "MT" => {
                let m = def
                    .m_kg
                    .ok_or_else(|| format!("CV '{}' mT mode requires m_kg", node_id))?;
                let t = def
                    .t_k
                    .ok_or_else(|| format!("CV '{}' mT mode requires t_k", node_id))?;
                Ok(CvInitMode::mT { m_kg: m, t_k: t })
            }
            "MH" => {
                let m = def
                    .m_kg
                    .ok_or_else(|| format!("CV '{}' mH mode requires m_kg", node_id))?;
                let h = def
                    .h_j_per_kg
                    .ok_or_else(|| format!("CV '{}' mH mode requires h_j_per_kg", node_id))?;
                Ok(CvInitMode::mH {
                    m_kg: m,
                    h_j_per_kg: h,
                })
            }
            other => Err(format!(
                "CV '{}' has unknown mode '{}'. Valid modes: PT, PH, mT, mH",
                node_id, other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_pt_mode() {
        let def = InitialCvDef {
            p_pa: Some(300000.0),
            t_k: Some(300.0),
            h_j_per_kg: None,
            m_kg: None,
            mode: None,
        };
        let mode = CvInitMode::infer(&def);
        assert!(matches!(mode, Some(CvInitMode::PT { .. })));
    }

    #[test]
    fn test_reject_overconstrained() {
        let def = InitialCvDef {
            p_pa: Some(300000.0),
            t_k: Some(300.0),
            h_j_per_kg: None,
            m_kg: Some(2.0),
            mode: None,
        };
        let mode = CvInitMode::infer(&def);
        assert!(
            mode.is_none(),
            "Over-constrained spec should not infer a mode"
        );
    }

    #[test]
    fn test_explicit_mode_pt() {
        let def = InitialCvDef {
            mode: Some("PT".to_string()),
            p_pa: Some(3500000.0),
            t_k: Some(300.0),
            h_j_per_kg: None,
            m_kg: None,
        };
        let result = CvInitMode::from_def(&def, "test_cv");
        assert!(result.is_ok());
    }

    #[test]
    fn test_explicit_mode_missing_field() {
        let def = InitialCvDef {
            mode: Some("PT".to_string()),
            p_pa: Some(3500000.0),
            t_k: None,
            h_j_per_kg: None,
            m_kg: None,
        };
        let result = CvInitMode::from_def(&def, "test_cv");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("t_k"));
    }
}
