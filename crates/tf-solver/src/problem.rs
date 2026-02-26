//! Problem definition for steady-state network solving.

use crate::error::{SolverError, SolverResult};
use std::collections::HashMap;
use tf_components::TwoPortComponent;
use tf_core::units::{Pressure, Temperature};
use tf_core::{CompId, NodeId};
use tf_fluids::{Composition, FluidModel, SpecEnthalpy, StateInput};
use tf_graph::Graph;

/// Steady-state network problem definition.
///
/// Models a fluid network where each node has unknown pressure and enthalpy,
/// constrained by mass and energy balances at each node.
pub struct SteadyProblem<'a> {
    /// Network topology
    pub graph: &'a Graph,

    /// Fluid property model
    pub fluid: &'a dyn FluidModel,

    /// Fluid composition (assumed uniform across network)
    pub composition: Composition,

    /// Fixed pressure boundary conditions (None = free variable)
    pub bc_pressure: Vec<Option<Pressure>>,

    /// Fixed enthalpy boundary conditions (None = free variable)
    pub bc_enthalpy: Vec<Option<SpecEnthalpy>>,

    /// Fixed temperature boundary conditions (converted to enthalpy if specified)
    pub bc_temperature: Vec<Option<Temperature>>,

    /// Components indexed by CompId
    pub components: HashMap<CompId, Box<dyn TwoPortComponent>>,
}

impl<'a> SteadyProblem<'a> {
    /// Create a new problem with validation.
    pub fn new(graph: &'a Graph, fluid: &'a dyn FluidModel, composition: Composition) -> Self {
        let node_count = graph.nodes().len();
        Self {
            graph,
            fluid,
            composition,
            bc_pressure: vec![None; node_count],
            bc_enthalpy: vec![None; node_count],
            bc_temperature: vec![None; node_count],
            components: HashMap::new(),
        }
    }

    /// Set pressure boundary condition for a node.
    pub fn set_pressure_bc(&mut self, node: NodeId, pressure: Pressure) -> SolverResult<()> {
        let idx = node.index() as usize;
        self.bc_pressure[idx] = Some(pressure);
        Ok(())
    }

    /// Set temperature boundary condition for a node.
    pub fn set_temperature_bc(
        &mut self,
        node: NodeId,
        temperature: Temperature,
    ) -> SolverResult<()> {
        let idx = node.index() as usize;
        self.bc_temperature[idx] = Some(temperature);
        Ok(())
    }

    /// Set enthalpy boundary condition for a node.
    pub fn set_enthalpy_bc(&mut self, node: NodeId, enthalpy: SpecEnthalpy) -> SolverResult<()> {
        let idx = node.index() as usize;
        self.bc_enthalpy[idx] = Some(enthalpy);
        Ok(())
    }

    /// Add a component to the problem.
    pub fn add_component(
        &mut self,
        comp_id: CompId,
        component: Box<dyn TwoPortComponent>,
    ) -> SolverResult<()> {
        if self.components.contains_key(&comp_id) {
            return Err(SolverError::ProblemSetup {
                what: format!("Component {:?} already exists", comp_id),
            });
        }
        self.components.insert(comp_id, component);
        Ok(())
    }

    /// Validate problem setup.
    pub fn validate(&self) -> SolverResult<()> {
        let node_count = self.graph.nodes().len();

        // Check vector lengths
        if self.bc_pressure.len() != node_count {
            return Err(SolverError::ProblemSetup {
                what: format!(
                    "bc_pressure length mismatch: {} != {}",
                    self.bc_pressure.len(),
                    node_count
                ),
            });
        }
        if self.bc_enthalpy.len() != node_count {
            return Err(SolverError::ProblemSetup {
                what: format!(
                    "bc_enthalpy length mismatch: {} != {}",
                    self.bc_enthalpy.len(),
                    node_count
                ),
            });
        }
        if self.bc_temperature.len() != node_count {
            return Err(SolverError::ProblemSetup {
                what: format!(
                    "bc_temperature length mismatch: {} != {}",
                    self.bc_temperature.len(),
                    node_count
                ),
            });
        }

        // Check that all nodes with fixed pressure also have fixed enthalpy or temperature
        for i in 0..node_count {
            if self.bc_pressure[i].is_some()
                && self.bc_enthalpy[i].is_none()
                && self.bc_temperature[i].is_none()
            {
                return Err(SolverError::ProblemSetup {
                    what: format!(
                        "Node {} has fixed pressure but no enthalpy or temperature",
                        i
                    ),
                });
            }
        }

        // Check that all components exist
        for comp in self.graph.components() {
            if !self.components.contains_key(&comp.id) {
                return Err(SolverError::ProblemSetup {
                    what: format!("Component {:?} missing from problem", comp.id),
                });
            }
        }

        Ok(())
    }

    /// Get the number of free variables (unknowns).
    pub fn num_free_vars(&self) -> usize {
        let mut count = 0;
        for i in 0..self.graph.nodes().len() {
            if self.bc_pressure[i].is_none() {
                count += 1; // Free pressure
            }
            if self.bc_enthalpy[i].is_none() && self.bc_temperature[i].is_none() {
                count += 1; // Free enthalpy
            }
        }
        count
    }

    /// Convert temperature BC to enthalpy BC at the given node.
    pub fn convert_temperature_bc(&mut self, node_idx: usize) -> SolverResult<()> {
        if let (Some(p), Some(t)) = (self.bc_pressure[node_idx], self.bc_temperature[node_idx]) {
            let state = self
                .fluid
                .state(StateInput::PT { p, t }, self.composition.clone())?;
            let h = self.fluid.h(&state)?;
            self.bc_enthalpy[node_idx] = Some(h);
            self.bc_temperature[node_idx] = None; // Mark as converted
        }
        Ok(())
    }

    /// Convert all temperature BCs to enthalpy BCs.
    pub fn convert_all_temperature_bcs(&mut self) -> SolverResult<()> {
        for i in 0..self.graph.nodes().len() {
            self.convert_temperature_bc(i)?;
        }
        Ok(())
    }
}
