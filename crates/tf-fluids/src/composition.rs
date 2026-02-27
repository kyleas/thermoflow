//! Fluid composition (pure or mixtures).

use crate::error::{FluidError, FluidResult};
use crate::species::Species;
use tf_core::numeric::{Tolerances, nearly_equal};

/// Fluid composition defined by normalized mole fractions.
///
/// The composition is always normalized (mole fractions sum to 1.0).
/// Currently, CoolProp backend supports pure fluids only; mixtures will be added later.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    /// Species and their mole fractions (always normalized to sum=1).
    items: Vec<(Species, f64)>,
}

impl Composition {
    /// Create a pure-species composition.
    pub fn pure(species: Species) -> Self {
        Self {
            items: vec![(species, 1.0)],
        }
    }

    /// Create a composition from mole fractions.
    ///
    /// Validates that all fractions are finite, non-negative, and have a positive sum,
    /// then normalizes to sum=1.
    pub fn new_mole_fractions(fractions: Vec<(Species, f64)>) -> FluidResult<Self> {
        if fractions.is_empty() {
            return Err(FluidError::InvalidArg {
                what: "empty composition",
            });
        }

        // Validate and compute sum
        let mut sum = 0.0;
        for (_, frac) in &fractions {
            if !frac.is_finite() {
                return Err(FluidError::NonPhysical {
                    what: "non-finite mole fraction",
                });
            }
            if *frac < 0.0 {
                return Err(FluidError::NonPhysical {
                    what: "negative mole fraction",
                });
            }
            sum += frac;
        }

        if sum <= 0.0 || !sum.is_finite() {
            return Err(FluidError::NonPhysical {
                what: "mole fractions sum to zero or non-finite",
            });
        }

        // Normalize
        let normalized: Vec<(Species, f64)> = fractions
            .into_iter()
            .map(|(s, f)| (s, f / sum))
            .filter(|(_, f)| *f > 1e-15) // Drop negligible species
            .collect();

        if normalized.is_empty() {
            return Err(FluidError::NonPhysical {
                what: "all mole fractions negligible",
            });
        }

        Ok(Self { items: normalized })
    }

    /// Get mole fraction of a species (0.0 if not present).
    pub fn mole_fraction(&self, species: Species) -> f64 {
        self.items
            .iter()
            .find(|(s, _)| *s == species)
            .map(|(_, f)| *f)
            .unwrap_or(0.0)
    }

    /// Check if this is a pure-species composition.
    ///
    /// Returns `Some(species)` if exactly one species has fraction ≈1.0.
    pub fn is_pure(&self) -> Option<Species> {
        if self.items.len() == 1 {
            let (species, frac) = self.items[0];
            let tol = Tolerances {
                abs: 1e-10,
                rel: 1e-10,
            };
            if nearly_equal(frac, 1.0, tol) {
                return Some(species);
            }
        }
        None
    }

    /// Iterate over all species with non-zero mole fractions.
    pub fn iter(&self) -> impl Iterator<Item = (Species, f64)> + '_ {
        self.items.iter().copied()
    }

    /// Compute mixture molar mass [kg/kmol] from species mole fractions.
    ///
    /// For a mixture: M_mix = Σ (x_i * M_i) where x_i is mole fraction of species i.
    pub fn molar_mass(&self) -> f64 {
        self.items
            .iter()
            .map(|(species, mole_frac)| species.molar_mass() * mole_frac)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_composition() {
        let comp = Composition::pure(Species::O2);
        assert_eq!(comp.is_pure(), Some(Species::O2));
        assert_eq!(comp.mole_fraction(Species::O2), 1.0);
        assert_eq!(comp.mole_fraction(Species::N2), 0.0);
    }

    #[test]
    fn mixture_normalization() {
        let comp =
            Composition::new_mole_fractions(vec![(Species::O2, 0.5), (Species::N2, 0.5)]).unwrap();

        assert_eq!(comp.is_pure(), None);
        let tol = Tolerances {
            abs: 1e-10,
            rel: 1e-10,
        };
        assert!(nearly_equal(comp.mole_fraction(Species::O2), 0.5, tol));
        assert!(nearly_equal(comp.mole_fraction(Species::N2), 0.5, tol));
    }

    #[test]
    fn mixture_normalization_non_unit_sum() {
        let comp =
            Composition::new_mole_fractions(vec![(Species::O2, 2.0), (Species::N2, 8.0)]).unwrap();

        // Should normalize to 0.2 and 0.8
        let tol = Tolerances {
            abs: 1e-10,
            rel: 1e-10,
        };
        assert!(nearly_equal(comp.mole_fraction(Species::O2), 0.2, tol));
        assert!(nearly_equal(comp.mole_fraction(Species::N2), 0.8, tol));
    }

    #[test]
    fn invalid_negative_fraction() {
        let result = Composition::new_mole_fractions(vec![(Species::O2, -0.5), (Species::N2, 1.5)]);

        assert!(result.is_err());
    }

    #[test]
    fn invalid_zero_sum() {
        let result = Composition::new_mole_fractions(vec![(Species::O2, 0.0), (Species::N2, 0.0)]);

        assert!(result.is_err());
    }

    #[test]
    fn invalid_non_finite() {
        let result = Composition::new_mole_fractions(vec![(Species::O2, f64::NAN)]);

        assert!(result.is_err());
    }

    #[test]
    fn composition_sum_is_one() {
        let comp = Composition::new_mole_fractions(vec![
            (Species::O2, 1.0),
            (Species::N2, 2.0),
            (Species::Ar, 3.0),
        ])
        .unwrap();

        let sum: f64 = comp.iter().map(|(_, f)| f).sum();
        let tol = Tolerances {
            abs: 1e-10,
            rel: 1e-10,
        };
        assert!(nearly_equal(sum, 1.0, tol));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn normalized_sum_is_one(fracs in prop::collection::vec(0.0_f64..1.0_f64, 1..5)) {
            let species = [Species::O2, Species::N2, Species::H2, Species::He, Species::Ar];
            let composition_input: Vec<(Species, f64)> = fracs
                .iter()
                .enumerate()
                .map(|(i, &f)| (species[i % species.len()], f))
                .collect();

            if let Ok(comp) = Composition::new_mole_fractions(composition_input) {
                let sum: f64 = comp.iter().map(|(_, f)| f).sum();
                let tol = Tolerances { abs: 1e-9, rel: 1e-9 };
                prop_assert!(nearly_equal(sum, 1.0, tol));
            }
        }
    }
}
