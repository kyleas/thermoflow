use tf_fluids::{EquilibriumState, FluidInputPair, Species};
use tf_project::schema::{FluidCaseDef, FluidInputPairDef, FluidWorkspaceDef};

/// Computation status for a state point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeStatus {
    /// Property computation successful
    Success,
    /// Property computation failed
    Failed,
    /// Currently computing (for async operations)
    Computing,
    /// Not yet computed or inputs changed
    NotComputed,
}

/// Single fluid state point (row) in the workspace.
#[derive(Debug, Clone)]
pub struct StatePoint {
    /// Unique identifier for this state point
    pub id: String,
    /// User-defined label for this state point (e.g., "Inlet", "State 1")
    pub label: String,
    /// Selected fluid species
    pub species: Species,
    /// Selected input pair
    pub input_pair: FluidInputPair,
    /// First input value (meaning depends on pair)
    pub input_1: f64,
    /// Raw text input for first value (preserves user units)
    pub input_1_text: String,
    /// Second input value (meaning depends on pair)
    pub input_2: f64,
    /// Raw text input for second value (preserves user units)
    pub input_2_text: String,
    /// Optional quality for two-phase disambiguation (0.0 = saturated liquid, 1.0 = saturated vapor)
    pub quality: Option<f64>,
    /// Last computed result
    pub last_result: Option<EquilibriumState>,
    /// Computation status
    pub status: ComputeStatus,
    /// Error message if computation failed
    pub error_message: Option<String>,
    /// Whether disambiguation is needed (detected after compute attempt)
    pub needs_disambiguation: bool,
}

impl Default for StatePoint {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label: "State 1".to_string(),
            species: Species::N2,
            input_pair: FluidInputPair::PT,
            input_1: 101_325.0,
            input_1_text: "101325".to_string(),
            input_2: 300.0,
            input_2_text: "300".to_string(),
            quality: None,
            last_result: None,
            status: ComputeStatus::NotComputed,
            error_message: None,
            needs_disambiguation: false,
        }
    }
}

impl StatePoint {
    pub fn new_with_label(label: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label,
            ..Default::default()
        }
    }

    pub fn clear_result(&mut self) {
        self.last_result = None;
        self.status = ComputeStatus::NotComputed;
        self.error_message = None;
        self.needs_disambiguation = false;
    }

    /// Check if inputs are complete (both values entered)
    pub fn inputs_complete(&self) -> bool {
        !self.input_1_text.trim().is_empty() && !self.input_2_text.trim().is_empty()
    }
}

/// Row-based fluid comparison workspace.
#[derive(Debug, Clone)]
pub struct FluidWorkspace {
    /// Collection of state points (rows)
    pub state_points: Vec<StatePoint>,
}

impl Default for FluidWorkspace {
    fn default() -> Self {
        Self {
            state_points: vec![StatePoint::default()],
        }
    }
}

impl FluidWorkspace {
    pub fn from_def(def: &FluidWorkspaceDef) -> Self {
        let state_points = if def.cases.is_empty() {
            vec![StatePoint::default()]
        } else {
            def.cases
                .iter()
                .enumerate()
                .map(|(i, case_def)| StatePoint {
                    id: case_def.id.clone(),
                    label: format!("State {}", i + 1),
                    species: case_def.species.parse::<Species>().unwrap_or(Species::N2),
                    input_pair: input_pair_from_def(case_def.input_pair),
                    input_1: case_def.input_1,
                    input_1_text: case_def.input_1.to_string(),
                    input_2: case_def.input_2,
                    input_2_text: case_def.input_2.to_string(),
                    quality: case_def.quality,
                    last_result: None,
                    status: ComputeStatus::NotComputed,
                    error_message: None,
                    needs_disambiguation: false,
                })
                .collect()
        };

        Self { state_points }
    }

    pub fn to_def(&self) -> FluidWorkspaceDef {
        FluidWorkspaceDef {
            cases: self
                .state_points
                .iter()
                .map(|state| FluidCaseDef {
                    id: state.id.clone(),
                    species: state.species.key().to_string(),
                    input_pair: input_pair_to_def(state.input_pair),
                    input_1: state.input_1,
                    input_2: state.input_2,
                    quality: state.quality,
                })
                .collect(),
        }
    }

    pub fn add_state_point(&mut self) {
        let next_num = self.state_points.len() + 1;
        self.state_points
            .push(StatePoint::new_with_label(format!("State {}", next_num)));
    }

    pub fn remove_state_point(&mut self, state_id: &str) {
        self.state_points.retain(|s| s.id != state_id);
        if self.state_points.is_empty() {
            self.state_points.push(StatePoint::default());
        }
    }

    /// Legacy method for backward compatibility
    #[deprecated(note = "Use state_points field directly")]
    pub fn cases(&self) -> &[StatePoint] {
        &self.state_points
    }

    /// Legacy method for backward compatibility
    #[deprecated(note = "Use add_state_point")]
    pub fn add_case(&mut self) {
        self.add_state_point();
    }

    /// Legacy method for backward compatibility
    #[deprecated(note = "Use remove_state_point")]
    pub fn remove_case(&mut self, case_id: &str) {
        self.remove_state_point(case_id);
    }
}

pub fn input_pair_from_def(def: FluidInputPairDef) -> FluidInputPair {
    match def {
        FluidInputPairDef::PT => FluidInputPair::PT,
        FluidInputPairDef::PH => FluidInputPair::PH,
        FluidInputPairDef::RhoH => FluidInputPair::RhoH,
        FluidInputPairDef::PS => FluidInputPair::PS,
    }
}

pub fn input_pair_to_def(pair: FluidInputPair) -> FluidInputPairDef {
    match pair {
        FluidInputPair::PT => FluidInputPairDef::PT,
        FluidInputPair::PH => FluidInputPairDef::PH,
        FluidInputPair::RhoH => FluidInputPairDef::RhoH,
        FluidInputPair::PS => FluidInputPairDef::PS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_roundtrip_def() {
        let mut workspace = FluidWorkspace::default();
        workspace.state_points[0].species = Species::NitrousOxide;
        workspace.state_points[0].input_pair = FluidInputPair::PS;
        workspace.state_points[0].input_1 = 202_650.0;
        workspace.state_points[0].input_2 = 7000.0;

        let def = workspace.to_def();
        let restored = FluidWorkspace::from_def(&def);

        assert_eq!(restored.state_points.len(), 1);
        assert_eq!(restored.state_points[0].species, Species::NitrousOxide);
        assert_eq!(restored.state_points[0].input_pair, FluidInputPair::PS);
        assert_eq!(restored.state_points[0].input_1, 202_650.0);
        assert_eq!(restored.state_points[0].input_2, 7000.0);
    }

    #[test]
    fn workspace_multi_case_roundtrip() {
        let mut workspace = FluidWorkspace::default();
        workspace.add_state_point();
        workspace.state_points[0].species = Species::N2;
        workspace.state_points[0].input_pair = FluidInputPair::PT;
        workspace.state_points[1].species = Species::NitrousOxide;
        workspace.state_points[1].input_pair = FluidInputPair::PH;
        workspace.state_points[1].quality = Some(0.5);

        let def = workspace.to_def();
        let restored = FluidWorkspace::from_def(&def);

        assert_eq!(restored.state_points.len(), 2);
        assert_eq!(restored.state_points[0].species, Species::N2);
        assert_eq!(restored.state_points[1].species, Species::NitrousOxide);
        assert_eq!(restored.state_points[1].quality, Some(0.5));
    }
}
