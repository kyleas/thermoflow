use crate::Species;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FluidCatalogEntry {
    pub species: Species,
    pub canonical_id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
}

impl FluidCatalogEntry {
    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }

        self.canonical_id.to_ascii_lowercase().contains(&query)
            || self.display_name.to_ascii_lowercase().contains(&query)
            || self
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase().contains(&query))
    }
}

const PRACTICAL_COOLPROP_CATALOG: [FluidCatalogEntry; 31] = [
    FluidCatalogEntry {
        species: Species::Air,
        canonical_id: "Air",
        display_name: "Air",
        aliases: &["atmosphere"],
    },
    FluidCatalogEntry {
        species: Species::N2,
        canonical_id: "N2",
        display_name: "Nitrogen",
        aliases: &["nitrogen"],
    },
    FluidCatalogEntry {
        species: Species::O2,
        canonical_id: "O2",
        display_name: "Oxygen",
        aliases: &["oxygen"],
    },
    FluidCatalogEntry {
        species: Species::NitrousOxide,
        canonical_id: "N2O",
        display_name: "Nitrous Oxide",
        aliases: &["nitrous oxide"],
    },
    FluidCatalogEntry {
        species: Species::H2,
        canonical_id: "H2",
        display_name: "Hydrogen",
        aliases: &["hydrogen"],
    },
    FluidCatalogEntry {
        species: Species::He,
        canonical_id: "He",
        display_name: "Helium",
        aliases: &["helium"],
    },
    FluidCatalogEntry {
        species: Species::Ar,
        canonical_id: "Ar",
        display_name: "Argon",
        aliases: &["argon"],
    },
    FluidCatalogEntry {
        species: Species::Neon,
        canonical_id: "Ne",
        display_name: "Neon",
        aliases: &["neon"],
    },
    FluidCatalogEntry {
        species: Species::Krypton,
        canonical_id: "Kr",
        display_name: "Krypton",
        aliases: &["krypton"],
    },
    FluidCatalogEntry {
        species: Species::Xenon,
        canonical_id: "Xe",
        display_name: "Xenon",
        aliases: &["xenon"],
    },
    FluidCatalogEntry {
        species: Species::CH4,
        canonical_id: "CH4",
        display_name: "Methane",
        aliases: &["methane"],
    },
    FluidCatalogEntry {
        species: Species::Ethane,
        canonical_id: "Ethane",
        display_name: "Ethane",
        aliases: &["c2h6"],
    },
    FluidCatalogEntry {
        species: Species::Ethylene,
        canonical_id: "Ethylene",
        display_name: "Ethylene",
        aliases: &["c2h4"],
    },
    FluidCatalogEntry {
        species: Species::Propane,
        canonical_id: "Propane",
        display_name: "Propane",
        aliases: &["c3h8", "n-propane"],
    },
    FluidCatalogEntry {
        species: Species::Propylene,
        canonical_id: "Propylene",
        display_name: "Propylene",
        aliases: &["c3h6"],
    },
    FluidCatalogEntry {
        species: Species::NButane,
        canonical_id: "nButane",
        display_name: "n-Butane",
        aliases: &["butane", "n-butane"],
    },
    FluidCatalogEntry {
        species: Species::Isobutane,
        canonical_id: "Isobutane",
        display_name: "Isobutane",
        aliases: &["i-butane"],
    },
    FluidCatalogEntry {
        species: Species::NPentane,
        canonical_id: "nPentane",
        display_name: "n-Pentane",
        aliases: &["pentane", "n-pentane"],
    },
    FluidCatalogEntry {
        species: Species::Isopentane,
        canonical_id: "Isopentane",
        display_name: "Isopentane",
        aliases: &["i-pentane"],
    },
    FluidCatalogEntry {
        species: Species::NHexane,
        canonical_id: "nHexane",
        display_name: "n-Hexane",
        aliases: &["hexane", "n-hexane"],
    },
    FluidCatalogEntry {
        species: Species::CO2,
        canonical_id: "CO2",
        display_name: "Carbon Dioxide",
        aliases: &["carbon dioxide"],
    },
    FluidCatalogEntry {
        species: Species::CO,
        canonical_id: "CO",
        display_name: "Carbon Monoxide",
        aliases: &["carbon monoxide"],
    },
    FluidCatalogEntry {
        species: Species::H2O,
        canonical_id: "H2O",
        display_name: "Water",
        aliases: &["water"],
    },
    FluidCatalogEntry {
        species: Species::Ammonia,
        canonical_id: "NH3",
        display_name: "Ammonia",
        aliases: &["ammonia"],
    },
    FluidCatalogEntry {
        species: Species::SulfurDioxide,
        canonical_id: "SO2",
        display_name: "Sulfur Dioxide",
        aliases: &["sulfur dioxide"],
    },
    FluidCatalogEntry {
        species: Species::R32,
        canonical_id: "R32",
        display_name: "R32",
        aliases: &[],
    },
    FluidCatalogEntry {
        species: Species::R125,
        canonical_id: "R125",
        display_name: "R125",
        aliases: &[],
    },
    FluidCatalogEntry {
        species: Species::R134a,
        canonical_id: "R134a",
        display_name: "R134a",
        aliases: &[],
    },
    FluidCatalogEntry {
        species: Species::R152a,
        canonical_id: "R152a",
        display_name: "R152a",
        aliases: &[],
    },
    FluidCatalogEntry {
        species: Species::R245fa,
        canonical_id: "R245fa",
        display_name: "R245fa",
        aliases: &[],
    },
    FluidCatalogEntry {
        species: Species::R1234yf,
        canonical_id: "R1234yf",
        display_name: "R1234yf",
        aliases: &[],
    },
];

pub fn practical_coolprop_catalog() -> &'static [FluidCatalogEntry] {
    &PRACTICAL_COOLPROP_CATALOG
}

pub fn filter_practical_coolprop_catalog(query: &str) -> Vec<FluidCatalogEntry> {
    practical_coolprop_catalog()
        .iter()
        .copied()
        .filter(|entry| entry.matches_query(query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn canonical_ids_are_unique() {
        let mut seen = HashSet::new();
        for entry in practical_coolprop_catalog() {
            assert!(
                seen.insert(entry.canonical_id),
                "duplicate canonical id: {}",
                entry.canonical_id
            );
        }
    }

    #[test]
    fn nitrous_oxide_is_present() {
        let nitrous = practical_coolprop_catalog()
            .iter()
            .find(|entry| entry.species == Species::NitrousOxide)
            .expect("nitrous oxide should be in catalog");

        assert_eq!(nitrous.canonical_id, "N2O");
        assert_eq!(nitrous.display_name, "Nitrous Oxide");
    }

    #[test]
    fn search_finds_nitrous_oxide() {
        let results = filter_practical_coolprop_catalog("nitrous");
        assert!(
            results
                .iter()
                .any(|entry| entry.species == Species::NitrousOxide)
        );
    }
}
