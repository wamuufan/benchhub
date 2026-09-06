use crate::db::Db;
use crate::engine::BenchmarkEngine;
use crate::models::{BenchmarkProfile, GpuMode, HistorySortOrder, RunResult};
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
        let safe_id = crate::utils::sanitize_identifier(bench_id)?;
        self.engine.uninstall_version(&safe_id, version).await
    }

    pub fn clear_history(&self) -> anyhow::Result<()> {
        self.db.clear_history()
    }

    pub async fn clean_all_runners(&self) -> anyhow::Result<()> {
        let runners_dir = self.data_dir.join("runners");
        if runners_dir.exists() {
            let safe_runners_dir =
                crate::utils::ensure_path_within(&self.data_dir, &runners_dir)?;
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
    ) -> anyhow::Result<crate::engine::BenchmarkRunOutput>
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

    pub fn create_methodology(&self, run_ids: &[i64]) -> anyhow::Result<RunResult> {
        if run_ids.len() < 2 {
            anyhow::bail!("En az 2 test seçilmelidir.");
        }

        let mut runs = Vec::new();
        for id in run_ids {
            let run = self.db.get_run_by_id(*id)?.ok_or_else(|| anyhow::anyhow!("Test bulunamadı: {}", id))?;
            runs.push(run);
        }

        let benchmark_id = runs[0].benchmark_id.clone();
        if !runs.iter().all(|r| r.benchmark_id == benchmark_id) {
            anyhow::bail!("Tüm testlerin benchmark_id değeri aynı olmalıdır.");
        }

        let mut score_sum = 0.0;
        let mut score_count = 0;
        let mut duration_sum = 0.0;
        let mut avg_cpu_usage_sum = 0.0;
        let mut avg_gpu_usage_sum = 0.0;
        let mut avg_cpu_temp_sum = 0.0;
        let mut avg_gpu_temp_sum = 0.0;
        let mut avg_power_w_sum = 0.0;
        let mut avg_ac_power_w_sum = 0.0;
        let mut avg_ram_gb_sum = 0.0;
        let mut avg_vram_gb_sum = 0.0;
        let mut avg_cpu_freq_sum: u64 = 0;
        let mut avg_gpu_freq_sum: u64 = 0;
        let mut avg_vram_freq_sum: u64 = 0;

        let mut peak_cpu_usage = 0.0_f32;
        let mut peak_gpu_usage = 0.0_f32;
        let mut peak_cpu_temp = 0.0_f32;
        let mut peak_gpu_temp = 0.0_f32;
        let mut peak_cpu_freq_mhz = 0;
        let mut peak_gpu_freq_mhz = 0;
        let mut peak_vram_freq_mhz = 0;
        let mut peak_gpu_power_w = 0.0_f32;
        let mut peak_power_w = 0.0_f32;
        let mut peak_ac_power_w = 0.0_f32;
        let mut peak_ram_gb = 0.0_f32;
        let mut peak_vram_gb = 0.0_f32;

        let mut cpu_throttling = "Yok".to_string();
        let mut gpu_throttling = "Yok".to_string();

        for run in &runs {
            if let Some(s) = run.score {
                score_sum += s;
                score_count += 1;
            }
            duration_sum += run.duration_secs;
            avg_cpu_usage_sum += run.avg_cpu_usage;
            avg_gpu_usage_sum += run.avg_gpu_usage;
            avg_cpu_temp_sum += run.avg_cpu_temp;
            avg_gpu_temp_sum += run.avg_gpu_temp;
            avg_power_w_sum += run.avg_power_w;
            avg_ac_power_w_sum += run.avg_ac_power_w;
            avg_ram_gb_sum += run.avg_ram_gb;
            avg_vram_gb_sum += run.avg_vram_gb;
            avg_cpu_freq_sum += run.avg_cpu_freq_mhz as u64;
            avg_gpu_freq_sum += run.avg_gpu_freq_mhz as u64;
            avg_vram_freq_sum += run.avg_vram_freq_mhz as u64;

            peak_cpu_usage = peak_cpu_usage.max(run.peak_cpu_usage);
            peak_gpu_usage = peak_gpu_usage.max(run.peak_gpu_usage);
            peak_cpu_temp = peak_cpu_temp.max(run.peak_cpu_temp);
            peak_gpu_temp = peak_gpu_temp.max(run.peak_gpu_temp);
            peak_cpu_freq_mhz = peak_cpu_freq_mhz.max(run.peak_cpu_freq_mhz);
            peak_gpu_freq_mhz = peak_gpu_freq_mhz.max(run.peak_gpu_freq_mhz);
            peak_vram_freq_mhz = peak_vram_freq_mhz.max(run.peak_vram_freq_mhz);
            peak_gpu_power_w = peak_gpu_power_w.max(run.peak_gpu_power_w);
            peak_power_w = peak_power_w.max(run.peak_power_w);
            peak_ac_power_w = peak_ac_power_w.max(run.peak_ac_power_w);
            peak_ram_gb = peak_ram_gb.max(run.peak_ram_gb);
            peak_vram_gb = peak_vram_gb.max(run.peak_vram_gb);

            if run.cpu_throttling != "Yok" { cpu_throttling = run.cpu_throttling.clone(); }
            if run.gpu_throttling != "Yok" { gpu_throttling = run.gpu_throttling.clone(); }
        }

        let n = runs.len() as f32;
        let score = if score_count > 0 { Some(score_sum / score_count as f64) } else { None };
        
        let gpu_mode = if runs.iter().all(|r| r.gpu_mode == runs[0].gpu_mode) {
            runs[0].gpu_mode.clone()
        } else {
            "Karma".to_string()
        };

        let timestamp = chrono::Utc::now().timestamp();
        
        let log_dir = self.data_dir.join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("methodology_{}.txt", timestamp));
        
        let mut log_content = String::new();
        log_content.push_str(&format!("Methodology ({})\n", benchmark_id));
        for run in &runs {
            log_content.push_str(&format!("ID: {}, Date: {}, Score: {:?}\n", run.id, run.timestamp, run.score));
        }
        std::fs::write(&log_path, log_content)?;

        let mut methodology = RunResult {
            id: 0,
            benchmark_id,
            category: runs[0].category.clone(),
            preset_or_version: format!("{} Test Ortalaması", runs.len()),
            gpu_mode,
            score,
            status: "Tamamlandı".to_string(),
            timestamp,
            duration_secs: duration_sum / n,
            avg_cpu_usage: avg_cpu_usage_sum / n,
            peak_cpu_usage,
            avg_cpu_temp: avg_cpu_temp_sum / n,
            peak_cpu_temp,
            avg_cpu_freq_mhz: (avg_cpu_freq_sum / runs.len() as u64) as u32,
            peak_cpu_freq_mhz,
            cpu_throttling,
            avg_gpu_usage: avg_gpu_usage_sum / n,
            peak_gpu_usage,
            avg_gpu_temp: avg_gpu_temp_sum / n,
            peak_gpu_temp,
            avg_gpu_freq_mhz: (avg_gpu_freq_sum / runs.len() as u64) as u32,
            peak_gpu_freq_mhz,
            avg_vram_freq_mhz: (avg_vram_freq_sum / runs.len() as u64) as u32,
            peak_vram_freq_mhz,
            avg_gpu_power_w: runs.iter().map(|r| r.avg_gpu_power_w).sum::<f32>() / n,
            peak_gpu_power_w,
            avg_power_w: avg_power_w_sum / n,
            peak_power_w,
            avg_ac_power_w: avg_ac_power_w_sum / n,
            peak_ac_power_w,
            avg_ram_gb: avg_ram_gb_sum / n,
            peak_ram_gb,
            avg_vram_gb: avg_vram_gb_sum / n,
            peak_vram_gb,
            gpu_throttling,
            system_info_summary: runs.last().unwrap().system_info_summary.clone(),
            log_path: log_path.to_string_lossy().to_string(),
            power_profile: "Metodoloji".to_string(),
            is_methodology: true,
            methodology_parent_id: None,
        };

        let new_id = self.db.create_methodology_record(&methodology, run_ids)?;
        methodology.id = new_id;
        Ok(methodology)
    }

    pub fn get_methodology_children(&self, methodology_id: i64) -> anyhow::Result<Vec<RunResult>> {
        Ok(self.db.get_methodology_children(methodology_id)?)
    }
}
