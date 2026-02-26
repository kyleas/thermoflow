use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::thread::{self, JoinHandle};
use tf_app::{RunMode, RunOptions, RunRequest};

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
    #[allow(dead_code)]
    Progress {
        step: usize,
        total: usize,
    },
    Complete {
        run_id: String,
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
            },
        };

        // Execute via tf-app
        let response = tf_app::run_service::ensure_run(&request)?;

        // Notify completion
        tx.send(WorkerMessage::Complete {
            run_id: response.run_id,
        })?;

        Ok(())
    }
}
