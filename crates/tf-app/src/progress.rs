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
    pub message: Option<String>,
    pub steady: Option<SteadyProgress>,
    pub transient: Option<TransientProgress>,
}

impl RunProgressEvent {
    pub fn stage(
        mode: RunMode,
        stage: RunStage,
        elapsed_wall_s: f64,
        message: Option<String>,
    ) -> Self {
        Self {
            mode,
            stage,
            elapsed_wall_s,
            message,
            steady: None,
            transient: None,
        }
    }
}
