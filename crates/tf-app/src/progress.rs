use crate::run_service::RunMode;

#[derive(Debug, Clone)]
pub enum RunStage {
    LoadingProject,
    CheckingCache,
    LoadingCachedResult,
    CompilingRuntime,
    BuildingSteadyProblem,
    SolvingSteady,
    RunningTransient,
    SavingResults,
    Completed,
}

impl RunStage {
    pub fn label(&self) -> &'static str {
        match self {
            RunStage::LoadingProject => "Loading project",
            RunStage::CheckingCache => "Checking cache",
            RunStage::LoadingCachedResult => "Loading cached result",
            RunStage::CompilingRuntime => "Compiling runtime",
            RunStage::BuildingSteadyProblem => "Building steady problem",
            RunStage::SolvingSteady => "Solving steady",
            RunStage::RunningTransient => "Running transient",
            RunStage::SavingResults => "Saving results",
            RunStage::Completed => "Completed",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SteadyProgress {
    pub outer_iteration: Option<usize>,
    pub max_outer_iterations: Option<usize>,
    pub iteration: Option<usize>,
    pub residual_norm: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct TransientProgress {
    pub sim_time_s: f64,
    pub t_end_s: f64,
    pub fraction_complete: f64,
    pub step: usize,
    pub cutback_retries: usize,
    pub fallback_uses: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RunProgressEvent {
    pub mode: RunMode,
    pub stage: RunStage,
    pub elapsed_wall_s: f64,
    pub initialization_strategy: Option<String>,
    pub message: Option<String>,
    pub steady: Option<SteadyProgress>,
    pub transient: Option<TransientProgress>,
}

impl RunProgressEvent {
    pub fn stage(
        mode: RunMode,
        stage: RunStage,
        elapsed_wall_s: f64,
        initialization_strategy: Option<String>,
        message: Option<String>,
    ) -> Self {
        Self {
            mode,
            stage,
            elapsed_wall_s,
            initialization_strategy,
            message,
            steady: None,
            transient: None,
        }
    }
}
