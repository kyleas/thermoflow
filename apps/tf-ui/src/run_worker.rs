use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tf_app::{RunMode, RunOptions, RunProgressEvent, RunRequest, RunTimingSummary};

#[derive(Debug, Clone)]
pub enum RunType {
    Steady,
    Transient { dt_s: f64, t_end_s: f64 },
}

pub struct RunWorker {
    pub progress_rx: Receiver<WorkerMessage>,
    _handle: JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    Progress(RunProgressEvent),
    Complete {
        run_id: String,
        loaded_from_cache: bool,
        timing: RunTimingSummary,
    },
    Error {
        message: String,
    },
}

impl RunWorker {
    pub fn start(
        run_type: RunType,
        project_path: &Path,
        system_id: &str,
        use_cached: bool,
    ) -> Self {
        let (tx, rx) = channel();
        let project_path = project_path.to_path_buf();
        let system_id = system_id.to_string();

        let handle = thread::spawn(move || {
            if let Err(e) =
                Self::run_simulation(run_type, &project_path, &system_id, use_cached, &tx)
            {
                let _ = tx.send(WorkerMessage::Error {
                    message: format!("Worker error: {}", e),
                });
            }
        });

        Self {
            progress_rx: rx,
            _handle: handle,
        }
    }

    fn run_simulation(
        run_type: RunType,
        project_path: &Path,
        system_id: &str,
        use_cached: bool,
        tx: &std::sync::mpsc::Sender<WorkerMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Convert UI RunType to tf-app RunMode
        let mode = match run_type {
            RunType::Steady => RunMode::Steady,
            RunType::Transient { dt_s, t_end_s } => RunMode::Transient { dt_s, t_end_s },
        };

        // Build request
        let request = RunRequest {
            project_path,
            system_id,
            mode,
            options: RunOptions {
                use_cache: use_cached,
                solver_version: "0.1.0".to_string(),
                initialization_strategy: None,
            },
        };

        let mut last_emit = Instant::now();
        let mut last_stage_key = String::new();
        let mut last_fraction = -1.0f64;

        // Execute via tf-app
        let response = tf_app::run_service::ensure_run_with_progress(
            &request,
            Some(&mut |event| {
                let stage_key = format!("{:?}", event.stage);
                let fraction = event
                    .transient
                    .as_ref()
                    .map(|t| t.fraction_complete)
                    .unwrap_or(-1.0);

                let emit_now = stage_key != last_stage_key
                    || (fraction >= 0.0 && (fraction - last_fraction).abs() >= 0.01)
                    || last_emit.elapsed().as_millis() >= 100;

                if emit_now {
                    let _ = tx.send(WorkerMessage::Progress(event));
                    last_emit = Instant::now();
                    last_stage_key = stage_key;
                    last_fraction = fraction;
                }
            }),
        )?;

        // Notify completion
        tx.send(WorkerMessage::Complete {
            run_id: response.run_id,
            loaded_from_cache: response.loaded_from_cache,
            timing: response.timing,
        })?;

        Ok(())
    }
}
