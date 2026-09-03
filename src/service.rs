use benchhub::db::Db;
use benchhub::engine::BenchmarkEngine;
use benchhub::models::{BenchmarkProfile, GpuMode, HistorySortOrder, RunResult};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch::Receiver;

pub struct BenchHubService {
    pub db: Db,
    pub engine: BenchmarkEngine,
    pub profiles: Arc<Vec<BenchmarkProfile>>,
    pub data_dir: PathBuf,
}

impl BenchHubService {
    pub fn new(
        db: Db,
        engine: BenchmarkEngine,
        profiles: Arc<Vec<BenchmarkProfile>>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            db,
            engine,
            profiles,
            data_dir,
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn install<F>(&self, bench_id: &str, version: &str, on_log: F) -> anyhow::Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.id == bench_id)
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;
        self.engine.install_version(profile, version, on_log).await
    }

    pub async fn uninstall(&self, bench_id: &str, version: &str) -> anyhow::Result<()> {
        let safe_id = benchhub::utils::sanitize_identifier(bench_id)?;
        self.engine.uninstall_version(&safe_id, version).await
    }

    pub fn clear_history(&self) -> anyhow::Result<()> {
        self.db.clear_history()
    }

    pub async fn clean_all_runners(&self) -> anyhow::Result<()> {
        let runners_dir = self.data_dir.join("runners");
        if runners_dir.exists() {
            let safe_runners_dir =
                benchhub::utils::ensure_path_within(&self.data_dir, &runners_dir)?;
            tokio::fs::remove_dir_all(&safe_runners_dir).await?;
            tokio::fs::create_dir_all(&safe_runners_dir).await?;
        }
        Ok(())
    }

    pub fn get_history(
        &self,
        search: &str,
        category: &str,
        gpu_mode: &str,
        status: &str,
        sort_order: HistorySortOrder,
    ) -> anyhow::Result<Vec<RunResult>> {
        self.db
            .get_filtered_history(search, category, gpu_mode, status, sort_order)
    }

    pub fn delete_history(&self, id: i64) -> anyhow::Result<()> {
        self.db.delete_run(id)
    }

    #[tracing::instrument(skip_all)]
    pub async fn run_benchmark<F>(
        &self,
        bench_id: &str,
        version: &str,
        current_gpu_mode: GpuMode,
        cancel_rx: Receiver<bool>,
        log_file_path: Option<PathBuf>,
        on_log: F,
    ) -> anyhow::Result<benchhub::engine::BenchmarkRunOutput>
    where
        F: FnMut(String) + Send + 'static,
    {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.id == bench_id)
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;
        self.engine
            .run_version(
                profile,
                version,
                cancel_rx,
                log_file_path,
                current_gpu_mode,
                on_log,
            )
            .await
    }

    pub fn insert_run(&self, run: &RunResult) -> anyhow::Result<i64> {
        self.db.insert_run(run)
    }

    pub fn update_run(&self, run: &RunResult) -> anyhow::Result<()> {
        self.db.update_run(run)
    }

    pub fn get_run_by_id(&self, id: i64) -> anyhow::Result<Option<RunResult>> {
        self.db.get_run_by_id(id)
    }
}
