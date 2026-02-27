//! Run storage API.

use crate::types::{RunManifest, TimeseriesRecord};
use crate::{ResultsError, ResultsResult};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct RunStore {
    root_dir: PathBuf,
}

impl RunStore {
    pub fn new(root_dir: PathBuf) -> ResultsResult<Self> {
        // Use a more resilient approach that handles Windows race conditions
        if !root_dir.exists() {
            fs::create_dir_all(&root_dir)?;
        }
        Ok(Self { root_dir })
    }

    pub fn for_project(project_path: &Path) -> ResultsResult<Self> {
        let project_dir = project_path
            .parent()
            .ok_or_else(|| ResultsError::InvalidPath {
                message: "project path has no parent directory".to_string(),
            })?;
        let runs_dir = project_dir.join(".thermoflow").join("runs");
        Self::new(runs_dir)
    }

    fn run_dir(&self, run_id: &str) -> PathBuf {
        self.root_dir.join(run_id)
    }

    pub fn has_run(&self, run_id: &str) -> bool {
        self.run_dir(run_id).join("manifest.json").exists()
    }

    pub fn save_run(
        &self,
        manifest: &RunManifest,
        records: &[TimeseriesRecord],
    ) -> ResultsResult<()> {
        let run_dir = self.run_dir(&manifest.run_id);
        fs::create_dir_all(&run_dir)?;

        let manifest_path = run_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(manifest)?;
        fs::write(manifest_path, manifest_json)?;

        let timeseries_path = run_dir.join("timeseries.jsonl");
        let mut timeseries_content = String::new();
        for record in records {
            let line = serde_json::to_string(record)?;
            timeseries_content.push_str(&line);
            timeseries_content.push('\n');
        }
        fs::write(timeseries_path, timeseries_content)?;

        Ok(())
    }

    pub fn load_manifest(&self, run_id: &str) -> ResultsResult<RunManifest> {
        let manifest_path = self.run_dir(run_id).join("manifest.json");

        if !manifest_path.exists() {
            return Err(ResultsError::RunNotFound {
                run_id: run_id.to_string(),
            });
        }

        let content = fs::read_to_string(manifest_path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn load_timeseries(&self, run_id: &str) -> ResultsResult<Vec<TimeseriesRecord>> {
        let timeseries_path = self.run_dir(run_id).join("timeseries.jsonl");

        if !timeseries_path.exists() {
            return Err(ResultsError::RunNotFound {
                run_id: run_id.to_string(),
            });
        }

        let content = fs::read_to_string(timeseries_path)?;
        let mut records = Vec::new();
        for line in content.lines() {
            if !line.trim().is_empty() {
                let record: TimeseriesRecord = serde_json::from_str(line)?;
                records.push(record);
            }
        }

        Ok(records)
    }

    pub fn list_runs(&self, system_id: &str) -> ResultsResult<Vec<RunManifest>> {
        let mut runs = Vec::new();

        if !self.root_dir.exists() {
            return Ok(runs);
        }

        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let run_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(manifest) = self.load_manifest(&run_id)
                    && manifest.system_id == system_id
                {
                    runs.push(manifest);
                }
            }
        }

        Ok(runs)
    }

    pub fn delete_run(&self, run_id: &str) -> ResultsResult<()> {
        let run_dir = self.run_dir(run_id);
        if run_dir.exists() {
            fs::remove_dir_all(run_dir)?;
        }
        Ok(())
    }
}
