//! BenchHub Background Daemon Runtime and Unix Socket IPC Server.
//!
//! Owns all benchmark execution, runner lifecycle, SQLite history storage,
//! live hardware telemetry polling, sequential queue execution, and settings persistence.

use anyhow::{Context, Result};
use benchhub_core::{
    config,
    db::Db,
    engine::BenchmarkEngine,
    eula, export,
    logging::{clean_terminal_chunk, TerminalRingBuffer},
    service::BenchHubService,
    telemetry, thermal, utils,
};
use benchhub_types::{
    normalize_run_status, validate_custom_arg, ActiveTaskInfo, AppSettings, BenchmarkProfile,
    DaemonEvent, DaemonInfo, DaemonNotification, DaemonPayload, DaemonRequest, DaemonResponse,
    DaemonStateSnapshot, GpuMode, HistorySortOrder, QueueItem, RunBenchmarkParams,
    RunComparisonDiff, RunOutcome, RunResult, TelemetryData,
};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, watch, Mutex, RwLock};

const EMBEDDED_PROFILES: &[(&str, &str)] = &[
    ("7zip.toml", include_str!("../../../benchmarks/7zip.toml")),
    (
        "blender.toml",
        include_str!("../../../benchmarks/blender.toml"),
    ),
    ("cray.toml", include_str!("../../../benchmarks/cray.toml")),
    (
        "ffmpeg.toml",
        include_str!("../../../benchmarks/ffmpeg.toml"),
    ),
    ("fio.toml", include_str!("../../../benchmarks/fio.toml")),
    (
        "furmark.toml",
        include_str!("../../../benchmarks/furmark.toml"),
    ),
    (
        "geekbench.toml",
        include_str!("../../../benchmarks/geekbench.toml"),
    ),
    (
        "gravitymark.toml",
        include_str!("../../../benchmarks/gravitymark.toml"),
    ),
    (
        "llama-bench.toml",
        include_str!("../../../benchmarks/llama-bench.toml"),
    ),
    (
        "stream.toml",
        include_str!("../../../benchmarks/stream.toml"),
    ),
    (
        "unigine.toml",
        include_str!("../../../benchmarks/unigine.toml"),
    ),
    (
        "ycruncher.toml",
        include_str!("../../../benchmarks/ycruncher.toml"),
    ),
];

/// Resolves the Unix domain socket path for `benchhubd` IPC.
pub fn default_socket_path() -> PathBuf {
    benchhub_types::resolve_daemon_socket_path(unsafe { libc::geteuid() })
}

/// Ensures parent directory of the IPC socket exists with owner-only `0o700` permissions.
pub fn ensure_socket_parent_secure(socket_path: &Path) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create socket parent dir {:?}", parent))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
        }
    }
    Ok(())
}

/// Loads benchmark manifests from disk if present, falling back to embedded manifests.
pub fn load_daemon_profiles() -> Vec<BenchmarkProfile> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidate_dirs.push(exe_dir.join("benchmarks"));
            candidate_dirs.push(exe_dir.join("../share/benchhub/benchmarks"));
        }
    }
    candidate_dirs.push(PathBuf::from("benchmarks"));
    candidate_dirs.push(PathBuf::from("../../benchmarks"));
    candidate_dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks"));

    let mut profiles: Vec<BenchmarkProfile> = Vec::new();
    for dir in candidate_dirs {
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            match toml::from_str::<BenchmarkProfile>(&content) {
                                Ok(profile) => profiles.push(profile),
                                Err(err) => {
                                    tracing::warn!(
                                        "Failed to parse benchmark profile {:?}: {}",
                                        path,
                                        err
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if !profiles.is_empty() {
                break;
            }
        }
    }

    if profiles.is_empty() {
        for (name, raw) in EMBEDDED_PROFILES {
            match toml::from_str::<BenchmarkProfile>(raw) {
                Ok(profile) => profiles.push(profile),
                Err(err) => {
                    tracing::warn!("Failed to parse embedded profile {}: {}", name, err);
                }
            }
        }
    }

    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    profiles
}

enum LogCommand {
    Chunk(String),
    Clear(Option<tokio::sync::oneshot::Sender<()>>),
}

struct ActiveTaskRecord {
    info: ActiveTaskInfo,
    cancel_tx: Option<watch::Sender<bool>>,
}

/// Shared daemon state holding execution engine, telemetry stream, SQLite database, and task queue.
pub struct DaemonRuntime {
    data_dir: PathBuf,
    settings_path: PathBuf,
    service: RwLock<Arc<BenchHubService>>,
    settings: RwLock<AppSettings>,
    profiles: Arc<Vec<BenchmarkProfile>>,
    terminal_buffer: Arc<Mutex<TerminalRingBuffer>>,
    log_tx: mpsc::UnboundedSender<LogCommand>,
    queue: Mutex<Vec<QueueItem>>,
    is_queue_running: AtomicBool,
    active_task: Mutex<Option<ActiveTaskRecord>>,
    installed_cache: RwLock<(HashMap<String, bool>, HashMap<String, bool>)>,
    telemetry_interval_tx: watch::Sender<u64>,
    telemetry_gpu_mode_tx: watch::Sender<GpuMode>,
    configured_telemetry_interval_ms: AtomicU64,
    disable_telemetry_during_runs: AtomicBool,
    monitor_active: AtomicBool,
    run_telemetry_active: AtomicBool,
    telemetry_samples_recorder: Arc<Mutex<Option<Vec<TelemetryData>>>>,
    latest_telemetry: Arc<RwLock<Option<TelemetryData>>>,
    event_tx: broadcast::Sender<DaemonEvent>,
    exe_path: String,
    exe_modified_nanos: u128,
    shutdown_tx: watch::Sender<bool>,
}

/// Reduced background telemetry cadence (ms) when idle and not viewing the Monitor tab.
pub const IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS: u64 = 5_000;

/// Computes the effective daemon telemetry polling interval in milliseconds (`0` pauses polling).
pub fn effective_telemetry_interval_ms(
    base_interval_ms: u64,
    disable_during_runs: bool,
    monitor_active: bool,
    run_active: bool,
) -> u64 {
    if run_active && disable_during_runs {
        0
    } else if run_active || monitor_active {
        base_interval_ms
    } else {
        base_interval_ms.max(IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS)
    }
}

impl DaemonRuntime {
    /// Initializes the daemon runtime in the default application data directory.
    pub async fn new_default() -> Result<Arc<Self>> {
        let data_dir = utils::get_data_dir()?;
        let profiles = load_daemon_profiles();
        Self::new_with_config(data_dir, profiles).await
    }

    /// Initializes the daemon runtime with explicit data directory and profiles (useful for tests).
    pub async fn new_with_config(
        data_dir: PathBuf,
        mut profiles: Vec<BenchmarkProfile>,
    ) -> Result<Arc<Self>> {
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        tokio::fs::create_dir_all(&data_dir)
            .await
            .with_context(|| format!("Failed to create daemon data directory {:?}", data_dir))?;

        let settings_path = data_dir.join("settings.json");
        let mut settings = config::load_settings(&settings_path);
        settings.sanitize();

        let db_path = data_dir.join("benchhub.db");
        let db = Db::new(db_path)?;

        let custom_runners = non_empty_path(&settings.custom_runners_dir);
        let engine = BenchmarkEngine::new(data_dir.clone()).with_runners_dir(custom_runners);
        let profiles_arc = Arc::new(profiles);
        let service = Arc::new(BenchHubService::new(
            db,
            engine,
            profiles_arc.clone(),
            data_dir.clone(),
        ));

        let (event_tx, _) = broadcast::channel::<DaemonEvent>(512);
        let (shutdown_tx, _) = watch::channel(false);
        let (exe_path, exe_modified_nanos) = capture_startup_exe_metadata();
        let terminal_buffer = Arc::new(Mutex::new(TerminalRingBuffer::new(
            settings.terminal_buffer_lines,
        )));

        let (log_tx, mut log_rx) = mpsc::unbounded_channel::<LogCommand>();
        let terminal_buf_clone = terminal_buffer.clone();
        let event_tx_logs = event_tx.clone();
        tokio::spawn(async move {
            while let Some(cmd) = log_rx.recv().await {
                match cmd {
                    LogCommand::Clear(ack_tx) => {
                        {
                            let mut buf = terminal_buf_clone.lock().await;
                            buf.clear();
                        }
                        let _ = event_tx_logs.send(DaemonEvent::ClearTerminal);
                        if let Some(tx) = ack_tx {
                            let _ = tx.send(());
                        }
                    }
                    LogCommand::Chunk(raw_chunk) => {
                        let cleaned = clean_terminal_chunk(&raw_chunk);
                        if cleaned.is_empty() {
                            continue;
                        }
                        {
                            let mut buf = terminal_buf_clone.lock().await;
                            buf.push_str(&cleaned);
                        }
                        let _ = event_tx_logs.send(DaemonEvent::TerminalLog(cleaned));
                    }
                }
            }
        });

        let (tele_tx, mut tele_rx) = mpsc::channel::<TelemetryData>(128);
        let configured_interval_ms = settings.telemetry_interval_ms;
        let disable_during_runs = settings.disable_telemetry_during_runs;
        let initial_interval = effective_telemetry_interval_ms(
            configured_interval_ms,
            disable_during_runs,
            false,
            false,
        );
        let (interval_tx, interval_rx) = watch::channel(initial_interval);
        let (gpu_mode_tx, gpu_mode_rx) = watch::channel(settings.selected_gpu_mode);
        tokio::spawn(telemetry::start_telemetry_loop_with_mode(
            tele_tx,
            interval_rx,
            gpu_mode_rx,
        ));

        let telemetry_samples_recorder: Arc<Mutex<Option<Vec<TelemetryData>>>> =
            Arc::new(Mutex::new(None));
        let latest_telemetry: Arc<RwLock<Option<TelemetryData>>> = Arc::new(RwLock::new(None));

        let recorder_clone = telemetry_samples_recorder.clone();
        let latest_clone = latest_telemetry.clone();
        let event_tx_tele = event_tx.clone();
        tokio::spawn(async move {
            while let Some(sample) = tele_rx.recv().await {
                {
                    let mut latest = latest_clone.write().await;
                    *latest = Some(sample.clone());
                }
                {
                    let mut rec = recorder_clone.lock().await;
                    if let Some(ref mut vec) = *rec {
                        vec.push(sample.clone());
                    }
                }
                let _ = event_tx_tele.send(DaemonEvent::TelemetrySample(sample));
            }
        });

        let runtime = Arc::new(Self {
            data_dir,
            settings_path,
            service: RwLock::new(service),
            settings: RwLock::new(settings),
            profiles: profiles_arc,
            terminal_buffer,
            log_tx,
            queue: Mutex::new(Vec::new()),
            is_queue_running: AtomicBool::new(false),
            active_task: Mutex::new(None),
            installed_cache: RwLock::new((HashMap::new(), HashMap::new())),
            telemetry_interval_tx: interval_tx,
            telemetry_gpu_mode_tx: gpu_mode_tx,
            configured_telemetry_interval_ms: AtomicU64::new(configured_interval_ms),
            disable_telemetry_during_runs: AtomicBool::new(disable_during_runs),
            monitor_active: AtomicBool::new(false),
            run_telemetry_active: AtomicBool::new(false),
            telemetry_samples_recorder,
            latest_telemetry,
            event_tx,
            exe_path,
            exe_modified_nanos,
            shutdown_tx,
        });

        runtime.refresh_installed_cache().await;
        Ok(runtime)
    }

    /// Returns the currently active telemetry polling interval in milliseconds (`0` when paused).
    pub fn current_effective_telemetry_interval_ms(&self) -> u64 {
        effective_telemetry_interval_ms(
            self.configured_telemetry_interval_ms
                .load(Ordering::Acquire),
            self.disable_telemetry_during_runs.load(Ordering::Acquire),
            self.monitor_active.load(Ordering::Acquire),
            self.run_telemetry_active.load(Ordering::Acquire),
        )
    }

    fn sync_telemetry_interval(&self, force_sample_on_resume: bool) {
        let next = self.current_effective_telemetry_interval_ms();
        let mut modified = false;
        self.telemetry_interval_tx.send_if_modified(|curr| {
            if *curr != next {
                *curr = next;
                modified = true;
                true
            } else {
                false
            }
        });
        if !modified && force_sample_on_resume && next > 0 {
            let _ = self.telemetry_interval_tx.send(next);
        }
    }

    /// Updates whether the GUI Monitor tab is active and adjusts background polling cadence.
    pub fn set_monitor_active(&self, active: bool) {
        let prev = self.monitor_active.swap(active, Ordering::AcqRel);
        self.sync_telemetry_interval(!prev && active);
    }

    /// Subscribes to server-push daemon events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// Subscribes to daemon-initiated shutdown notifications (`DaemonRequest::Shutdown`).
    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    /// Clears pending queued work and cancels any active task before daemon shutdown so child
    /// benchmark process groups do not linger as orphans.
    pub async fn cancel_active_for_shutdown(&self) {
        {
            let mut q = self.queue.lock().await;
            q.clear();
        }
        let cancel_tx = {
            let guard = self.active_task.lock().await;
            guard.as_ref().and_then(|r| r.cancel_tx.clone())
        };
        if let Some(tx) = cancel_tx {
            let _ = tx.send(true);
            for _ in 0..20 {
                if self.active_task.lock().await.is_none() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }

    /// Returns the most recently collected hardware telemetry sample, if available.
    pub async fn latest_telemetry(&self) -> Option<TelemetryData> {
        self.latest_telemetry.read().await.clone()
    }

    async fn current_service(&self) -> Arc<BenchHubService> {
        self.service.read().await.clone()
    }

    fn emit_notification(&self, notification: DaemonNotification) {
        let _ = self.event_tx.send(DaemonEvent::Notification(notification));
    }

    async fn emit_state_changed(&self) {
        let snap = self.build_snapshot().await;
        let _ = self
            .event_tx
            .send(DaemonEvent::StateChanged(Box::new(snap)));
    }

    fn push_log_chunk(&self, chunk: String) {
        let _ = self.log_tx.send(LogCommand::Chunk(chunk));
    }

    async fn clear_terminal_logs_sync(&self) {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        if self.log_tx.send(LogCommand::Clear(Some(ack_tx))).is_ok() {
            let _ = ack_rx.await;
        }
    }

    /// Computes both profile-level and version-level installation status maps.
    pub async fn refresh_installed_cache(&self) {
        let service = self.current_service().await;
        let mut by_id = HashMap::new();
        let mut by_version = HashMap::new();

        for profile in self.profiles.iter() {
            let mut any_installed = false;
            if profile.versions.is_empty() {
                let default_ver = profile
                    .default_version
                    .as_deref()
                    .or(profile.version.as_deref())
                    .unwrap_or("");
                let installed_default = service
                    .engine
                    .is_version_installed_async(&profile.id, default_ver)
                    .await;
                let installed_plain = if !default_ver.is_empty() {
                    service
                        .engine
                        .is_version_installed_async(&profile.id, "")
                        .await
                } else {
                    installed_default
                };
                let ok = installed_default || installed_plain;
                by_version.insert(format!("{}::{}", profile.id, default_ver), ok);
                any_installed = ok;
            } else {
                for ver in &profile.versions {
                    let ok = service
                        .engine
                        .is_version_installed_async(&profile.id, &ver.version)
                        .await;
                    by_version.insert(format!("{}::{}", profile.id, ver.version), ok);
                    if ok {
                        any_installed = true;
                    }
                }
            }

            if !any_installed && service.engine.is_installed(&profile.id) {
                any_installed = true;
            }
            by_id.insert(profile.id.clone(), any_installed);
        }

        let mut cache = self.installed_cache.write().await;
        *cache = (by_id, by_version);
    }

    /// Builds a consistent snapshot of daemon state for newly connected or synchronized clients.
    pub async fn build_snapshot(&self) -> DaemonStateSnapshot {
        let (installed_map, installed_versions_map) = {
            let cache = self.installed_cache.read().await;
            (cache.0.clone(), cache.1.clone())
        };
        let active_task = {
            let guard = self.active_task.lock().await;
            guard.as_ref().map(|r| r.info.clone())
        };
        let queue = self.queue.lock().await.clone();
        let is_queue_running = self.is_queue_running.load(Ordering::SeqCst);
        let recent_terminal_logs = {
            let buf = self.terminal_buffer.lock().await;
            buf.lines_vec()
        };
        let settings = self.settings.read().await.clone();

        DaemonStateSnapshot {
            installed_map,
            installed_versions_map,
            active_task,
            queue,
            is_queue_running,
            recent_terminal_logs,
            settings,
        }
    }

    /// Handles a single RPC request from an attached client.
    pub async fn handle_request(self: &Arc<Self>, req: DaemonRequest) -> DaemonResponse {
        match self.dispatch_request(req).await {
            Ok(payload) => DaemonResponse::ok(payload),
            Err(err) => DaemonResponse::err(err),
        }
    }

    async fn dispatch_request(
        self: &Arc<Self>,
        req: DaemonRequest,
    ) -> std::result::Result<DaemonPayload, String> {
        match req {
            DaemonRequest::Ping => Ok(DaemonPayload::Pong),
            DaemonRequest::GetDaemonInfo => {
                let has_active_task = self.active_task.lock().await.is_some()
                    || self.is_queue_running.load(Ordering::SeqCst);
                Ok(DaemonPayload::DaemonInfo(DaemonInfo {
                    pid: std::process::id(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    exe_path: self.exe_path.clone(),
                    exe_modified_nanos: self.exe_modified_nanos,
                    exe_deleted: is_current_exe_deleted(),
                    has_active_task,
                }))
            }
            DaemonRequest::Shutdown => {
                let has_active_task = self.active_task.lock().await.is_some()
                    || self.is_queue_running.load(Ordering::SeqCst);
                if has_active_task {
                    return Err(
                        "Cannot shut down daemon while a benchmark or install task is active"
                            .to_string(),
                    );
                }
                let tx = self.shutdown_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    let _ = tx.send(true);
                });
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::ClearTerminalLogs => {
                self.clear_terminal_logs_sync().await;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::SubscribeEvents => {
                Err("SubscribeEvents must be handled at stream level".to_string())
            }
            DaemonRequest::GetStateSnapshot => Ok(DaemonPayload::Snapshot(Box::new(
                self.build_snapshot().await,
            ))),
            DaemonRequest::GetCatalog => Ok(DaemonPayload::Catalog((*self.profiles).clone())),
            DaemonRequest::GetInstalledMap => {
                self.refresh_installed_cache().await;
                let cache = self.installed_cache.read().await;
                Ok(DaemonPayload::InstalledMap(cache.0.clone()))
            }
            DaemonRequest::IsVersionInstalled { id, version } => {
                let service = self.current_service().await;
                let installed = service
                    .engine
                    .is_version_installed_async(&id, &version)
                    .await;
                Ok(DaemonPayload::Bool(installed))
            }
            DaemonRequest::InstallBenchmark {
                id,
                version,
                eula_accepted,
            } => {
                self.handle_install(id, version, eula_accepted).await?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::UninstallBenchmark { id, version } => {
                let service = self.current_service().await;
                if let Err(err) = service.uninstall(&id, &version).await {
                    let msg = err.to_string();
                    self.emit_notification(DaemonNotification::TaskFailed {
                        benchmark_id: id,
                        reason: msg.clone(),
                    });
                    return Err(msg);
                }
                self.refresh_installed_cache().await;
                self.emit_notification(DaemonNotification::UninstallCompleted {
                    benchmark_id: id,
                    version,
                });
                self.emit_state_changed().await;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::RunBenchmark(params) => {
                let outcome = self.execute_single_benchmark(params, false).await?;
                Ok(DaemonPayload::RunOutcome(outcome))
            }
            DaemonRequest::CancelActiveRun => {
                let cancel_tx = {
                    let guard = self.active_task.lock().await;
                    guard.as_ref().and_then(|r| r.cancel_tx.clone())
                };
                if let Some(tx) = cancel_tx {
                    let _ = tx.send(true);
                    self.push_log_chunk(
                        "[BenchHub] Cancellation requested by client...\n".to_string(),
                    );
                    Ok(DaemonPayload::Unit)
                } else {
                    Err("No cancellable task is currently active".to_string())
                }
            }
            DaemonRequest::EnqueueItem(item) => {
                self.validate_queue_item(&item)?;
                let snapshot = {
                    let mut q = self.queue.lock().await;
                    q.push(item.clone());
                    q.clone()
                };
                self.emit_notification(DaemonNotification::Enqueued {
                    benchmark_id: item.id,
                    version: item.version,
                });
                self.emit_state_changed().await;
                Ok(DaemonPayload::Queue(snapshot))
            }
            DaemonRequest::RemoveQueueItem { index } => {
                let snapshot = {
                    let mut q = self.queue.lock().await;
                    if index >= q.len() {
                        return Err(format!("Queue index {} out of bounds", index));
                    }
                    q.remove(index);
                    q.clone()
                };
                self.emit_state_changed().await;
                Ok(DaemonPayload::Queue(snapshot))
            }
            DaemonRequest::ClearQueue => {
                let snapshot = {
                    let mut q = self.queue.lock().await;
                    q.clear();
                    q.clone()
                };
                self.emit_state_changed().await;
                Ok(DaemonPayload::Queue(snapshot))
            }
            DaemonRequest::SetQueue(items) => {
                if self.is_queue_running.load(Ordering::SeqCst) {
                    return Err("Cannot modify execution queue while queue is running".to_string());
                }
                for item in &items {
                    self.validate_queue_item(item)?;
                }
                let snapshot = {
                    let mut q = self.queue.lock().await;
                    *q = items;
                    q.clone()
                };
                self.emit_state_changed().await;
                Ok(DaemonPayload::Queue(snapshot))
            }
            DaemonRequest::GetQueue => {
                let q = self.queue.lock().await.clone();
                Ok(DaemonPayload::Queue(q))
            }
            DaemonRequest::RunQueue => {
                self.start_queue_runner().await?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::GetEulaText { id } => Ok(DaemonPayload::Text(
                eula::get_eula_for_benchmark(&id).to_string(),
            )),
            DaemonRequest::GetSettings => Ok(DaemonPayload::Settings(Box::new(
                self.settings.read().await.clone(),
            ))),
            DaemonRequest::SaveSettings(new_settings) => {
                self.apply_and_save_settings(new_settings).await?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::GetFilteredHistory {
                search,
                category,
                gpu_mode,
                status,
                sort_order,
            } => {
                let service = self.current_service().await;
                let runs = service
                    .get_history(&search, &category, &gpu_mode, &status, sort_order)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::History(runs))
            }
            DaemonRequest::DeleteHistory { id } => {
                let service = self.current_service().await;
                service.delete_history(id).map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::ClearHistory => {
                let service = self.current_service().await;
                service.clear_history().map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::GetRunById { id } => {
                let service = self.current_service().await;
                let run = service.get_run_by_id(id).map_err(|e| e.to_string())?;
                Ok(DaemonPayload::RunOpt(run.map(Box::new)))
            }
            DaemonRequest::UpdateRunScore { id, score } => {
                if let Some(val) = score {
                    if !val.is_finite() {
                        return Err("Score must be a finite number".to_string());
                    }
                }
                let service = self.current_service().await;
                let mut run = service
                    .get_run_by_id(id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("History run #{} not found", id))?;
                run.score = score;
                service.update_run(&run).map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Run(Box::new(run)))
            }
            DaemonRequest::GetRunLog { id } => {
                let service = self.current_service().await;
                let run = service
                    .get_run_by_id(id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("History run #{} not found", id))?;
                let content = read_safe_run_log(&self.data_dir, &run.log_path)?;
                Ok(DaemonPayload::Text(content))
            }
            DaemonRequest::GetRunsByIds { ids } => {
                let service = self.current_service().await;
                let runs = service
                    .db
                    .get_runs_by_ids(&ids)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::History(runs))
            }
            DaemonRequest::CompareRuns { id_a, id_b } => {
                let service = self.current_service().await;
                let runs = service
                    .db
                    .get_runs_by_ids(&[id_a, id_b])
                    .map_err(|e| e.to_string())?;
                let run_a = runs.iter().find(|r| r.id == id_a).cloned();
                let run_b = runs.iter().find(|r| r.id == id_b).cloned();
                let diff = match (run_a, run_b) {
                    (Some(a), Some(b)) => Some(Box::new(RunComparisonDiff::calculate(a, b))),
                    _ => None,
                };
                Ok(DaemonPayload::Comparison(diff))
            }
            DaemonRequest::CreateMethodology { child_ids } => {
                let service = self.current_service().await;
                let run = service
                    .create_methodology(&child_ids)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Run(Box::new(run)))
            }
            DaemonRequest::GetMethodologyChildren { methodology_id } => {
                let service = self.current_service().await;
                let children = service
                    .get_methodology_children(methodology_id)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::History(children))
            }
            DaemonRequest::SetRunsGroup { ids, group_name } => {
                let service = self.current_service().await;
                service
                    .group_runs(&ids, &group_name)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::RemoveRunsFromGroup { ids } => {
                let service = self.current_service().await;
                service.ungroup_runs(&ids).map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::GetDistinctGroups => {
                let service = self.current_service().await;
                let groups = service.get_distinct_groups().map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Groups(groups))
            }
            DaemonRequest::DeleteGroup { group_name } => {
                let service = self.current_service().await;
                service
                    .delete_group(&group_name)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::RenameGroup { old_name, new_name } => {
                let service = self.current_service().await;
                service
                    .db
                    .rename_group(&old_name, &new_name)
                    .map_err(|e| e.to_string())?;
                Ok(DaemonPayload::Unit)
            }
            DaemonRequest::ExportHistoryCsv => {
                let service = self.current_service().await;
                let runs = service
                    .get_history("", "all", "all", "all", HistorySortOrder::DateDesc)
                    .map_err(|e| e.to_string())?;
                let exports_dir = self.data_dir.join("exports");
                std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;
                let filename = format!(
                    "benchhub_history_{}.csv",
                    Utc::now().format("%Y%m%d_%H%M%S")
                );
                let output_path = exports_dir.join(filename);
                export::export_history_csv(&runs, &output_path).map_err(|e| e.to_string())?;
                let path_str = output_path.to_string_lossy().to_string();
                self.emit_notification(DaemonNotification::ExportCompleted {
                    path: path_str.clone(),
                });
                Ok(DaemonPayload::Text(path_str))
            }
            DaemonRequest::ExportComparisonPng { run_ids } => {
                if run_ids.is_empty() {
                    return Err("No runs selected for comparison export".to_string());
                }
                let service = self.current_service().await;
                let runs = service
                    .db
                    .get_runs_by_ids(&run_ids)
                    .map_err(|e| e.to_string())?;
                let path = export::export_comparison_png(&runs).map_err(|e| e.to_string())?;
                let path_str = path.to_string_lossy().to_string();
                self.emit_notification(DaemonNotification::ExportCompleted {
                    path: path_str.clone(),
                });
                Ok(DaemonPayload::Text(path_str))
            }
            DaemonRequest::GetHardwareCapabilities => {
                Ok(DaemonPayload::Text(telemetry::get_sensor_diagnosis()))
            }
            DaemonRequest::GetPowerProfile => {
                Ok(DaemonPayload::Text(telemetry::get_power_profile()))
            }
            DaemonRequest::GetSystemInfoSummary => {
                Ok(DaemonPayload::Text(telemetry::get_system_info_summary()))
            }
            DaemonRequest::SetMonitorActive(active) => {
                self.set_monitor_active(active);
                Ok(DaemonPayload::Unit)
            }
        }
    }

    fn validate_queue_item(&self, item: &QueueItem) -> std::result::Result<(), String> {
        if !self.profiles.iter().any(|p| p.id == item.id) {
            return Err(format!("Unknown benchmark profile '{}'", item.id));
        }
        if let Some(ref raw_args) = item.custom_args {
            parse_and_validate_custom_args(raw_args)?;
        }
        Ok(())
    }

    async fn apply_and_save_settings(
        &self,
        mut new_settings: AppSettings,
    ) -> std::result::Result<(), String> {
        new_settings.sanitize();
        config::save_settings(&self.settings_path, &new_settings).map_err(|e| e.to_string())?;

        let prev_runners_dir = {
            let guard = self.settings.read().await;
            guard.custom_runners_dir.clone()
        };

        self.configured_telemetry_interval_ms
            .store(new_settings.telemetry_interval_ms, Ordering::Release);
        self.disable_telemetry_during_runs.store(
            new_settings.disable_telemetry_during_runs,
            Ordering::Release,
        );
        self.sync_telemetry_interval(false);
        if !self.run_telemetry_active.load(Ordering::Acquire) {
            let _ = self
                .telemetry_gpu_mode_tx
                .send(new_settings.selected_gpu_mode);
        }
        {
            let mut buf = self.terminal_buffer.lock().await;
            buf.set_max_lines(new_settings.terminal_buffer_lines);
        }

        if prev_runners_dir != new_settings.custom_runners_dir {
            let custom_runners = non_empty_path(&new_settings.custom_runners_dir);
            let old_service = self.current_service().await;
            let new_engine =
                BenchmarkEngine::new(self.data_dir.clone()).with_runners_dir(custom_runners);
            let rebuilt = Arc::new(BenchHubService::new(
                old_service.db.clone(),
                new_engine,
                self.profiles.clone(),
                self.data_dir.clone(),
            ));
            {
                let mut guard = self.service.write().await;
                *guard = rebuilt;
            }
        }

        {
            let mut guard = self.settings.write().await;
            *guard = new_settings;
        }

        self.refresh_installed_cache().await;
        self.emit_state_changed().await;
        Ok(())
    }

    async fn handle_install(
        &self,
        id: String,
        version: String,
        eula_accepted: bool,
    ) -> std::result::Result<(), String> {
        if !eula_accepted {
            return Err(
                "Explicit third-party license/EULA acceptance is required before installing."
                    .to_string(),
            );
        }

        let profile = self
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("Profile not found: {}", id))?;

        let (cancel_tx, cancel_rx) = watch::channel(false);

        {
            let mut active = self.active_task.lock().await;
            if active.is_some() || self.is_queue_running.load(Ordering::SeqCst) {
                self.emit_notification(DaemonNotification::AlreadyRunning);
                return Err("Another task is already running".to_string());
            }
            *active = Some(ActiveTaskRecord {
                info: ActiveTaskInfo {
                    benchmark_id: id.clone(),
                    version: version.clone(),
                    is_install: true,
                    is_stress: profile
                        .test_type
                        .as_deref()
                        .is_some_and(|t| t.eq_ignore_ascii_case("stress")),
                    started_at_epoch_ms: Utc::now().timestamp_millis(),
                },
                cancel_tx: Some(cancel_tx),
            });
        }
        self.clear_terminal_logs_sync().await;
        self.push_log_chunk(format!(
            "=== [BenchHub] Installing: {} ({}) ===\n",
            profile.name, version
        ));
        self.push_log_chunk(format!(
            "[BenchHub] Explicit third-party license/EULA acceptance confirmed by user for '{}' ({}).\n",
            id, version
        ));
        self.emit_state_changed().await;

        let service = self.current_service().await;
        let log_tx = self.log_tx.clone();
        let res = service
            .install_cancellable(&id, &version, cancel_rx, move |chunk| {
                let _ = log_tx.send(LogCommand::Chunk(chunk));
            })
            .await;

        {
            let mut active = self.active_task.lock().await;
            *active = None;
        }

        match res {
            Ok(()) => {
                self.push_log_chunk(format!(
                    "[BenchHub] Installation completed successfully for '{}' ({}).\n",
                    id, version
                ));
                self.refresh_installed_cache().await;
                self.emit_notification(DaemonNotification::InstallCompleted {
                    benchmark_id: id,
                    version,
                });
                self.emit_state_changed().await;
                Ok(())
            }
            Err(err) => {
                self.refresh_installed_cache().await;
                if err
                    .downcast_ref::<benchhub_core::engine::InstallCancelledError>()
                    .is_some()
                {
                    self.push_log_chunk(format!(
                        "[BenchHub] Installation cancelled by user for '{}' ({}).\n",
                        id, version
                    ));
                    self.emit_notification(DaemonNotification::TaskStopped { benchmark_id: id });
                    self.emit_state_changed().await;
                    Ok(())
                } else {
                    let reason = err.to_string();
                    self.push_log_chunk(format!(
                        "[BenchHub] Installation failed for '{}' ({}): {}\n",
                        id, version, reason
                    ));
                    self.emit_notification(DaemonNotification::TaskFailed {
                        benchmark_id: id,
                        reason: reason.clone(),
                    });
                    self.emit_state_changed().await;
                    Err(reason)
                }
            }
        }
    }

    async fn start_queue_runner(self: &Arc<Self>) -> std::result::Result<(), String> {
        let initial_count = {
            let q = self.queue.lock().await;
            if q.is_empty() {
                return Err("Execution queue is empty".to_string());
            }
            q.len()
        };
        if self.active_task.lock().await.is_some()
            || self
                .is_queue_running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            self.emit_notification(DaemonNotification::AlreadyRunning);
            return Err("A task or queue is already running".to_string());
        }

        self.clear_terminal_logs_sync().await;
        self.push_log_chunk(format!(
            "=== [BenchHub] Starting benchmark queue ({} items) ===\n",
            initial_count
        ));
        self.emit_state_changed().await;
        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            let mut total: usize = 0;
            let mut completed: usize = 0;
            let mut failed: usize = 0;

            loop {
                let next_item = {
                    let mut q = runtime.queue.lock().await;
                    if q.is_empty() {
                        None
                    } else {
                        Some(q.remove(0))
                    }
                };
                let Some(item) = next_item else {
                    break;
                };
                total += 1;
                let display_total = total.max(initial_count);
                runtime.push_log_chunk(format!(
                    "\n=== [BenchHub] Queue item {}/{}: {} ({}) ===\n",
                    total, display_total, item.id, item.version
                ));
                runtime.emit_state_changed().await;

                let params = queue_item_to_params(item);
                match runtime.execute_single_benchmark(params, true).await {
                    Ok(outcome) if should_abort_queue_for_status(&outcome.status) => {
                        failed += 1;
                        // Stop processing remaining queued items when user cancels an active queue item.
                        break;
                    }
                    Ok(outcome) if outcome.status == "completed" => {
                        completed += 1;
                    }
                    Ok(_) | Err(_) => {
                        failed += 1;
                    }
                }
            }

            runtime.is_queue_running.store(false, Ordering::SeqCst);
            runtime.emit_notification(DaemonNotification::QueueCompleted {
                total,
                completed,
                failed,
            });
            runtime.emit_state_changed().await;
        });

        Ok(())
    }

    async fn execute_single_benchmark(
        &self,
        params: benchhub_types::RunBenchmarkParams,
        from_queue: bool,
    ) -> std::result::Result<RunOutcome, String> {
        let profile = self
            .profiles
            .iter()
            .find(|p| p.id == params.id)
            .cloned()
            .ok_or_else(|| format!("Profile not found: {}", params.id))?;

        let settings = self.settings.read().await.clone();
        let is_stress = profile
            .test_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("stress"));

        let parsed_custom_args = if let Some(ref raw) = params.custom_args {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(parse_and_validate_custom_args(trimmed)?)
            }
        } else {
            None
        };

        let (cancel_tx, mut cancel_rx) = watch::channel(false);

        {
            let mut active = self.active_task.lock().await;
            if active.is_some() || (!from_queue && self.is_queue_running.load(Ordering::SeqCst)) {
                self.emit_notification(DaemonNotification::AlreadyRunning);
                return Err("Another benchmark or install task is already running".to_string());
            }
            *active = Some(ActiveTaskRecord {
                info: ActiveTaskInfo {
                    benchmark_id: params.id.clone(),
                    version: params.version.clone(),
                    is_install: false,
                    is_stress,
                    started_at_epoch_ms: Utc::now().timestamp_millis(),
                },
                cancel_tx: Some(cancel_tx.clone()),
            });
        }
        if !from_queue {
            self.clear_terminal_logs_sync().await;
        }
        self.push_log_chunk(format!(
            "=== [BenchHub] Starting: {} ({}) ===\n",
            profile.name, params.version
        ));
        self.emit_state_changed().await;

        let effective_args = build_effective_args(
            &profile,
            &params.version,
            parsed_custom_args.as_deref(),
            params.duration_secs_override,
        );
        let timeout_secs =
            resolve_timeout_secs(is_stress, &settings, params.duration_secs_override);

        // Optional pre-run thermal conditioning
        let log_tx_therm = self.log_tx.clone();
        let log_tx_status = self.log_tx.clone();
        let thermal_ok = thermal::run_thermal_conditioning(
            &settings,
            &mut cancel_rx,
            move |msg| {
                let _ = log_tx_therm.send(LogCommand::Chunk(msg));
            },
            move |status| {
                let _ = log_tx_status.send(LogCommand::Chunk(format!("[BenchHub] {}\n", status)));
            },
        )
        .await;

        if !thermal_ok {
            {
                let mut active = self.active_task.lock().await;
                *active = None;
            }
            self.emit_notification(DaemonNotification::TaskStopped {
                benchmark_id: params.id,
            });
            self.emit_state_changed().await;
            return Ok(RunOutcome {
                score: None,
                status: "cancelled".to_string(),
                run_id: None,
            });
        }

        struct RunTelemetryGuard<'a>(&'a DaemonRuntime);
        impl Drop for RunTelemetryGuard<'_> {
            fn drop(&mut self) {
                self.0.run_telemetry_active.store(false, Ordering::Release);
                self.0.sync_telemetry_interval(false);
            }
        }

        // Elevate or pause telemetry polling strictly across actual execution based on settings.
        self.run_telemetry_active.store(true, Ordering::Release);
        self.sync_telemetry_interval(false);
        let run_telemetry_guard = RunTelemetryGuard(self);

        let record_telemetry = !settings.disable_telemetry_during_runs;
        if record_telemetry {
            // Align live telemetry GPU sampling to the benchmark's active GPU mode and start recording.
            let _ = self.telemetry_gpu_mode_tx.send(params.gpu_mode);
            let mut rec = self.telemetry_samples_recorder.lock().await;
            *rec = Some(Vec::with_capacity(256));
        } else {
            let mut rec = self.telemetry_samples_recorder.lock().await;
            *rec = None;
        }

        let safe_id_for_log =
            utils::sanitize_identifier(&params.id).unwrap_or_else(|_| "benchmark".to_string());
        let safe_ver_for_log = params
            .version
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let logs_dir = self.data_dir.join("logs");
        let _ = std::fs::create_dir_all(&logs_dir);
        let run_timestamp = Utc::now().timestamp();
        let log_file_path = logs_dir.join(format!(
            "{}_{}_{}.log",
            safe_id_for_log, safe_ver_for_log, run_timestamp
        ));

        let started_at = Instant::now();
        let service = self.current_service().await;
        let log_tx_run = self.log_tx.clone();
        let run_res = service
            .run_benchmark_custom(
                &params.id,
                &params.version,
                effective_args.as_deref(),
                timeout_secs,
                params.gpu_mode,
                cancel_rx,
                Some(log_file_path.clone()),
                move |chunk| {
                    let _ = log_tx_run.send(LogCommand::Chunk(chunk));
                },
            )
            .await;

        let mut samples = if record_telemetry {
            let mut rec = self.telemetry_samples_recorder.lock().await;
            rec.take().unwrap_or_default()
        } else {
            let mut rec = self.telemetry_samples_recorder.lock().await;
            *rec = None;
            Vec::new()
        };
        if record_telemetry {
            let _ = self
                .telemetry_gpu_mode_tx
                .send(self.settings.read().await.selected_gpu_mode);
        }
        drop(run_telemetry_guard);

        // Short runs (< telemetry interval) can complete before the background tick fires;
        // seed a snapshot unless telemetry during runs is disabled.
        if record_telemetry
            && !self.disable_telemetry_during_runs.load(Ordering::Acquire)
            && samples.is_empty()
        {
            let fallback_sample = if let Some(cached) = self.latest_telemetry.read().await.clone() {
                cached
            } else {
                let mode = params.gpu_mode;
                tokio::task::spawn_blocking(move || {
                    telemetry::TelemetryEngine::new().read_current_for_mode(mode)
                })
                .await
                .unwrap_or_else(|_| telemetry::TelemetryEngine::new().read_current())
            };
            samples.push(fallback_sample);
        }

        {
            let mut active = self.active_task.lock().await;
            *active = None;
        }

        let elapsed_secs = started_at.elapsed().as_secs_f32().max(0.1);

        match run_res {
            Ok(output) => {
                let norm_status = normalize_run_status(&output.status).to_string();
                let is_stopped = matches!(norm_status.as_str(), "cancelled" | "stopped");
                let should_save = !is_stopped || settings.save_stopped_runs;
                let mut saved_run_id: Option<i64> = None;

                if should_save {
                    let summary = telemetry::calculate_telemetry_summary(&samples);
                    let run_record = RunResult {
                        id: 0,
                        benchmark_id: params.id.clone(),
                        category: profile.category.clone(),
                        preset_or_version: if params.version.is_empty() {
                            profile
                                .default_version
                                .clone()
                                .or_else(|| profile.version.clone())
                                .unwrap_or_else(|| "default".to_string())
                        } else {
                            params.version.clone()
                        },
                        gpu_mode: params.gpu_mode.as_str().to_string(),
                        score: output.score,
                        status: norm_status.clone(),
                        timestamp: run_timestamp,
                        duration_secs: elapsed_secs,
                        avg_cpu_usage: summary.avg_cpu_usage,
                        peak_cpu_usage: summary.peak_cpu_usage,
                        avg_cpu_temp: summary.avg_cpu_temp,
                        peak_cpu_temp: summary.peak_cpu_temp,
                        avg_cpu_freq_mhz: summary.avg_cpu_freq_mhz,
                        peak_cpu_freq_mhz: summary.peak_cpu_freq_mhz,
                        cpu_throttling: summary.cpu_throttling,
                        avg_gpu_usage: summary.avg_gpu_usage,
                        peak_gpu_usage: summary.peak_gpu_usage,
                        avg_gpu_temp: summary.avg_gpu_temp,
                        peak_gpu_temp: summary.peak_gpu_temp,
                        avg_gpu_freq_mhz: summary.avg_gpu_freq_mhz,
                        peak_gpu_freq_mhz: summary.peak_gpu_freq_mhz,
                        avg_vram_freq_mhz: summary.avg_vram_freq_mhz,
                        peak_vram_freq_mhz: summary.peak_vram_freq_mhz,
                        avg_gpu_power_w: summary.avg_gpu_power_w,
                        peak_gpu_power_w: summary.peak_gpu_power_w,
                        gpu_throttling: summary.gpu_throttling,
                        avg_ram_gb: summary.avg_ram_gb,
                        peak_ram_gb: summary.peak_ram_gb,
                        avg_vram_gb: summary.avg_vram_gb,
                        peak_vram_gb: summary.peak_vram_gb,
                        avg_power_w: summary.avg_power_w,
                        peak_power_w: summary.peak_power_w,
                        avg_ac_power_w: summary.avg_ac_power_w,
                        peak_ac_power_w: summary.peak_ac_power_w,
                        system_info_summary: telemetry::get_system_info_summary(),
                        log_path: log_file_path.to_string_lossy().to_string(),
                        power_profile: telemetry::get_power_profile(),
                        is_methodology: false,
                        methodology_parent_id: None,
                        group_name: None,
                    };

                    match service.insert_run(&run_record) {
                        Ok(run_id) => {
                            saved_run_id = Some(run_id);
                            if !samples.is_empty() {
                                if let Err(err) =
                                    service.db.insert_telemetry_batch(run_id, &samples)
                                {
                                    tracing::warn!(
                                        "Failed to insert telemetry batch for run {}: {}",
                                        run_id,
                                        err
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!("Failed to save run record to SQLite: {}", err);
                        }
                    }
                }

                if is_stopped {
                    self.emit_notification(DaemonNotification::TaskStopped {
                        benchmark_id: params.id,
                    });
                } else if norm_status == "failed" {
                    self.emit_notification(DaemonNotification::TaskFailed {
                        benchmark_id: params.id,
                        reason: format!("Process exited with status {}", output.status),
                    });
                } else {
                    self.emit_notification(DaemonNotification::BenchmarkCompleted {
                        benchmark_id: params.id,
                        version: params.version,
                        score: output.score,
                        status: norm_status.clone(),
                    });
                }

                self.emit_state_changed().await;
                Ok(RunOutcome {
                    score: output.score,
                    status: norm_status,
                    run_id: saved_run_id,
                })
            }
            Err(err) => {
                let reason = err.to_string();
                self.emit_notification(DaemonNotification::TaskFailed {
                    benchmark_id: params.id,
                    reason: reason.clone(),
                });
                self.emit_state_changed().await;
                Err(reason)
            }
        }
    }
}

fn queue_item_to_params(item: QueueItem) -> RunBenchmarkParams {
    RunBenchmarkParams {
        id: item.id,
        version: item.version,
        gpu_mode: item.gpu_mode,
        custom_args: item.custom_args,
        duration_secs_override: item.duration_secs_override,
    }
}

fn non_empty_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Safely reads a run's saved log file strictly within `<data_dir>/logs`,
/// capping returned output to the tail 64 KiB and stripping ANSI sequences.
fn read_safe_run_log(data_dir: &Path, raw_log_path: &str) -> std::result::Result<String, String> {
    let trimmed = raw_log_path.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let logs_base = data_dir.join("logs");
    if !logs_base.exists() {
        return Ok(String::new());
    }

    let raw_path = Path::new(trimmed);
    let mut resolved = None;
    if let Ok(safe) = utils::ensure_path_within(&logs_base, raw_path) {
        if safe.is_file() {
            resolved = Some(safe);
        }
    } else if raw_path.is_absolute() && raw_path.exists() {
        return Err("Log file path is outside the permitted logs directory".to_string());
    }

    if resolved.is_none() {
        if let Some(filename) = raw_path.file_name() {
            let fallback = logs_base.join(filename);
            if let Ok(safe) = utils::ensure_path_within(&logs_base, &fallback) {
                if safe.is_file() {
                    resolved = Some(safe);
                }
            }
        }
    }

    let Some(path) = resolved else {
        return Ok(String::new());
    };

    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read log file: {}", e))?;
    if bytes.is_empty() {
        return Ok(String::new());
    }

    const MAX_LOG_BYTES: usize = 65_536;
    let slice = if bytes.len() > MAX_LOG_BYTES {
        &bytes[bytes.len() - MAX_LOG_BYTES..]
    } else {
        &bytes[..]
    };
    let clean = utils::strip_ansi_codes(&String::from_utf8_lossy(slice));
    if bytes.len() > MAX_LOG_BYTES {
        Ok(format!("[... earlier log lines truncated ...]\n{}", clean))
    } else {
        Ok(clean)
    }
}

fn parse_and_validate_custom_args(input: &str) -> std::result::Result<Vec<String>, String> {
    let mut parsed = Vec::new();
    for token in input.split_whitespace() {
        if let Err(reason) = validate_custom_arg(token) {
            return Err(format!("Unsafe custom argument '{}': {}", token, reason));
        }
        parsed.push(token.to_string());
    }
    Ok(parsed)
}

fn build_effective_args(
    profile: &BenchmarkProfile,
    version: &str,
    custom_args: Option<&[String]>,
    duration_secs_override: Option<u64>,
) -> Option<Vec<String>> {
    benchhub_core::engine::apply_duration_override(
        profile,
        version,
        custom_args,
        duration_secs_override,
    )
}

fn resolve_timeout_secs(
    is_stress: bool,
    settings: &AppSettings,
    duration_secs_override: Option<u64>,
) -> Option<u64> {
    if is_stress {
        match duration_secs_override {
            // Stress duration `0` explicitly requests continuous execution until user cancellation.
            Some(0) => None,
            // Add a small exit grace window so tools like FurMark finish and emit their score cleanly.
            Some(secs) => Some(secs.saturating_add(15)),
            None => {
                if settings.max_test_duration_secs > 0 {
                    Some(settings.max_test_duration_secs)
                } else {
                    None
                }
            }
        }
    } else if settings.max_test_duration_secs > 0 {
        Some(settings.max_test_duration_secs)
    } else {
        None
    }
}

/// Runs the Unix socket accept loop until `shutdown_rx` receives `true`.
pub async fn serve_unix_socket(
    runtime: Arc<DaemonRuntime>,
    socket_path: PathBuf,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    ensure_socket_parent_secure(&socket_path)?;

    if socket_path.exists() {
        // Check whether another live daemon owns this socket before removing a stale socket file.
        if UnixStream::connect(&socket_path).await.is_ok() {
            anyhow::bail!(
                "Another benchhubd instance is already listening at {:?}",
                socket_path
            );
        }
        std::fs::remove_file(&socket_path)
            .with_context(|| format!("Failed to remove stale socket {:?}", socket_path))?;
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind Unix socket {:?}", socket_path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&socket_path, perms);
    }

    tracing::info!("benchhubd listening on {:?}", socket_path);
    let mut runtime_shutdown_rx = runtime.subscribe_shutdown();
    if !*shutdown_rx.borrow() && !*runtime_shutdown_rx.borrow() {
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                _ = runtime_shutdown_rx.changed() => {
                    if *runtime_shutdown_rx.borrow() {
                        break;
                    }
                }
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _addr)) => {
                            let rt = Arc::clone(&runtime);
                            tokio::spawn(async move {
                                if let Err(err) = handle_client_connection(rt, stream).await {
                                    tracing::debug!("Client IPC connection ended with error: {}", err);
                                }
                            });
                        }
                        Err(err) => {
                            tracing::warn!("Unix socket accept error: {}", err);
                        }
                    }
                }
            }
        }
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }
    Ok(())
}

fn capture_startup_exe_metadata() -> (String, u128) {
    let Ok(exe) = std::env::current_exe() else {
        return (String::new(), 0);
    };
    let cleaned = exe
        .to_string_lossy()
        .trim_end_matches(" (deleted)")
        .to_string();
    let canonical = std::fs::canonicalize(&cleaned).unwrap_or_else(|_| PathBuf::from(&cleaned));
    let modified_nanos = std::fs::metadata(&canonical)
        .and_then(|m| m.modified())
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| std::io::Error::other("before epoch"))
        })
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (canonical.to_string_lossy().to_string(), modified_nanos)
}

fn is_current_exe_deleted() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(link) = std::fs::read_link("/proc/self/exe") {
            return link.to_string_lossy().ends_with(" (deleted)");
        }
    }
    false
}

/// Parses an optional parent PID value (`BENCHHUB_PARENT_PID`), ignoring invalid or self PIDs.
pub fn parse_parent_pid(raw: Option<&str>) -> Option<i32> {
    let pid = raw?.trim().parse::<i32>().ok()?;
    if pid > 1 && pid as u32 != std::process::id() {
        Some(pid)
    } else {
        None
    }
}

/// Reads `BENCHHUB_PARENT_PID` when `benchhubd` is launched by a supervising GUI shell.
pub fn parse_parent_pid_env() -> Option<i32> {
    let raw = std::env::var("BENCHHUB_PARENT_PID").ok();
    parse_parent_pid(raw.as_deref())
}

/// Checks whether a host process with `pid` is currently running and not a zombie.
pub fn is_process_alive(pid: i32) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
            if let Some((_, rest)) = stat.rsplit_once(')') {
                if rest.trim_start().starts_with('Z') {
                    return false;
                }
            }
        }
        true
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

async fn handle_client_connection(runtime: Arc<DaemonRuntime>, stream: UnixStream) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: DaemonRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(err) => {
                let resp = DaemonResponse::err(format!("Malformed request JSON: {}", err));
                let encoded = serde_json::to_string(&resp)? + "\n";
                write_half.write_all(encoded.as_bytes()).await?;
                write_half.flush().await?;
                continue;
            }
        };

        if matches!(req, DaemonRequest::SubscribeEvents) {
            struct MonitorResetGuard(Arc<DaemonRuntime>);
            impl Drop for MonitorResetGuard {
                fn drop(&mut self) {
                    if self.0.event_tx.receiver_count() == 0 {
                        self.0.set_monitor_active(false);
                    }
                }
            }
            // Declare guard before `rx` so `rx` drops first and decrements `receiver_count()`.
            let _monitor_reset_guard = MonitorResetGuard(Arc::clone(&runtime));
            let mut rx = runtime.subscribe_events();
            let snapshot = runtime.build_snapshot().await;
            let init_event = DaemonEvent::StateChanged(Box::new(snapshot));
            let encoded = serde_json::to_string(&init_event)? + "\n";
            write_half.write_all(encoded.as_bytes()).await?;
            if let Some(sample) = runtime.latest_telemetry().await {
                let tele_event = DaemonEvent::TelemetrySample(sample);
                let tele_encoded = serde_json::to_string(&tele_event)? + "\n";
                write_half.write_all(tele_encoded.as_bytes()).await?;
            }
            write_half.flush().await?;

            loop {
                tokio::select! {
                    recv_res = rx.recv() => {
                        match recv_res {
                            Ok(event) => {
                                let encoded = serde_json::to_string(&event)? + "\n";
                                if write_half.write_all(encoded.as_bytes()).await.is_err() {
                                    return Ok(());
                                }
                                if write_half.flush().await.is_err() {
                                    return Ok(());
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                tracing::warn!("Event subscriber lagged by {} messages; resyncing snapshot", skipped);
                                let snap = runtime.build_snapshot().await;
                                let sync_event = DaemonEvent::StateChanged(Box::new(snap));
                                let encoded = serde_json::to_string(&sync_event)? + "\n";
                                if write_half.write_all(encoded.as_bytes()).await.is_err() {
                                    return Ok(());
                                }
                                let _ = write_half.flush().await;
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                return Ok(());
                            }
                        }
                    }
                    read_res = reader.read_line(&mut line) => {
                        match read_res {
                            Ok(0) | Err(_) => return Ok(()),
                            Ok(_) => {
                                line.clear();
                            }
                        }
                    }
                }
            }
        }

        let resp = runtime.handle_request(req).await;
        let encoded = serde_json::to_string(&resp)? + "\n";
        write_half.write_all(encoded.as_bytes()).await?;
        write_half.flush().await?;
    }
}

/// Returns `true` when an active queue item outcome represents user cancellation
/// (`stopped` or `cancelled`) so remaining queued items are aborted cleanly.
fn should_abort_queue_for_status(status: &str) -> bool {
    matches!(
        benchhub_types::normalize_run_status(status),
        "stopped" | "cancelled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_clear_terminal_logs_preserves_fifo_ordering() {
        let temp = TempDir::new().unwrap();
        let runtime = DaemonRuntime::new_with_config(temp.path().join("data"), vec![])
            .await
            .unwrap();
        let mut rx = runtime.subscribe_events();

        runtime.push_log_chunk("stale-line-1\n".to_string());
        runtime.clear_terminal_logs_sync().await;
        runtime.push_log_chunk("fresh-line-2\n".to_string());

        // Drain log worker until fresh chunk lands in terminal buffer
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let snap = runtime.build_snapshot().await;
        assert_eq!(
            snap.recent_terminal_logs,
            vec!["fresh-line-2\n".to_string()]
        );

        let mut saw_stale = false;
        let mut saw_clear = false;
        let mut saw_fresh = false;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                DaemonEvent::TerminalLog(line) if line.contains("stale-line-1") => {
                    saw_stale = true;
                }
                DaemonEvent::ClearTerminal => {
                    saw_clear = true;
                }
                DaemonEvent::TerminalLog(line) if line.contains("fresh-line-2") => {
                    saw_fresh = true;
                }
                _ => {}
            }
        }
        assert!(saw_stale);
        assert!(saw_clear);
        assert!(saw_fresh);
    }

    #[test]
    fn test_parse_parent_pid_filters_invalid_and_self_pids() {
        assert_eq!(parse_parent_pid(None), None);
        assert_eq!(parse_parent_pid(Some("")), None);
        assert_eq!(parse_parent_pid(Some("not-a-pid")), None);
        assert_eq!(parse_parent_pid(Some("0")), None);
        assert_eq!(parse_parent_pid(Some("1")), None);
        assert_eq!(parse_parent_pid(Some("-42")), None);
        let self_pid = std::process::id().to_string();
        assert_eq!(parse_parent_pid(Some(&self_pid)), None);
        assert_eq!(parse_parent_pid(Some(" 999999 ")), Some(999999));
    }

    #[test]
    fn test_is_process_alive_tracks_current_and_missing_pids() {
        assert!(is_process_alive(std::process::id() as i32));
        assert!(!is_process_alive(0));
        assert!(!is_process_alive(-1));
        assert!(!is_process_alive(i32::MAX));
    }

    #[tokio::test]
    async fn test_cancel_active_for_shutdown_clears_queue_and_signals_cancel() {
        let temp = TempDir::new().unwrap();
        let runtime = DaemonRuntime::new_with_config(temp.path().join("data"), vec![])
            .await
            .unwrap();

        {
            let mut q = runtime.queue.lock().await;
            q.push(QueueItem {
                id: "7zip".to_string(),
                version: "24.08".to_string(),
                gpu_mode: benchhub_types::GpuMode::Auto,
                custom_args: None,
                duration_secs_override: None,
            });
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        {
            let mut active = runtime.active_task.lock().await;
            *active = Some(ActiveTaskRecord {
                info: ActiveTaskInfo {
                    benchmark_id: "7zip".to_string(),
                    version: "24.08".to_string(),
                    is_install: false,
                    is_stress: false,
                    started_at_epoch_ms: 0,
                },
                cancel_tx: Some(cancel_tx),
            });
        }

        let rt_clear = Arc::clone(&runtime);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            let mut active = rt_clear.active_task.lock().await;
            *active = None;
        });

        runtime.cancel_active_for_shutdown().await;
        assert!(runtime.queue.lock().await.is_empty());
        assert!(*cancel_rx.borrow());
        assert!(runtime.active_task.lock().await.is_none());
    }

    #[test]
    fn test_should_abort_queue_for_status_handles_stopped_and_cancelled() {
        assert!(should_abort_queue_for_status("stopped"));
        assert!(should_abort_queue_for_status("Stopped"));
        assert!(should_abort_queue_for_status("cancelled"));
        assert!(should_abort_queue_for_status("Canceled"));
        assert!(!should_abort_queue_for_status("completed"));
        assert!(!should_abort_queue_for_status("failed"));
        assert!(!should_abort_queue_for_status("timeout"));
        assert!(!should_abort_queue_for_status("timeout_scored"));
    }

    #[test]
    fn test_effective_telemetry_interval_rules() {
        assert_eq!(
            effective_telemetry_interval_ms(500, false, false, false),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );
        assert_eq!(
            effective_telemetry_interval_ms(500, false, true, false),
            500
        );
        assert_eq!(
            effective_telemetry_interval_ms(500, false, false, true),
            500
        );
        assert_eq!(
            effective_telemetry_interval_ms(250, true, false, false),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );
        assert_eq!(effective_telemetry_interval_ms(250, true, true, false), 250);
        assert_eq!(effective_telemetry_interval_ms(250, true, false, true), 0);
        assert_eq!(effective_telemetry_interval_ms(250, true, true, true), 0);
        assert_eq!(
            effective_telemetry_interval_ms(10_000, false, false, false),
            10_000
        );
    }

    #[test]
    fn test_settings_and_ipc_serde_compat() {
        let legacy: AppSettings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.disable_telemetry_during_runs);

        let encoded = serde_json::to_string(&DaemonRequest::SetMonitorActive(true)).unwrap();
        let decoded: DaemonRequest = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(decoded, DaemonRequest::SetMonitorActive(true)));
    }

    #[tokio::test]
    async fn test_daemon_telemetry_throttles_off_monitor_and_honors_run_override() {
        let temp = TempDir::new().unwrap();
        let runtime = DaemonRuntime::new_with_config(temp.path().join("data"), vec![])
            .await
            .unwrap();

        assert_eq!(
            runtime.current_effective_telemetry_interval_ms(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );

        runtime
            .dispatch_request(DaemonRequest::SetMonitorActive(true))
            .await
            .unwrap();
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 500);

        runtime
            .dispatch_request(DaemonRequest::SetMonitorActive(false))
            .await
            .unwrap();
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );

        let mut updated = runtime.settings.read().await.clone();
        updated.telemetry_interval_ms = 250;
        updated.disable_telemetry_during_runs = true;
        runtime
            .apply_and_save_settings(updated.clone())
            .await
            .unwrap();
        // Still off Monitor tab and idle -> stays throttled.
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );

        // Simulated active run while disable_telemetry_during_runs = true pauses even on Monitor.
        runtime
            .dispatch_request(DaemonRequest::SetMonitorActive(true))
            .await
            .unwrap();
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 250);
        runtime.run_telemetry_active.store(true, Ordering::Release);
        runtime.sync_telemetry_interval(false);
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 0);

        // Turning off disable_telemetry_during_runs mid-run restores full user cadence even off-tab.
        runtime
            .dispatch_request(DaemonRequest::SetMonitorActive(false))
            .await
            .unwrap();
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 0);
        updated.disable_telemetry_during_runs = false;
        runtime.apply_and_save_settings(updated).await.unwrap();
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 250);

        runtime.run_telemetry_active.store(false, Ordering::Release);
        runtime.sync_telemetry_interval(false);
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );
    }

    #[tokio::test]
    async fn test_execute_single_benchmark_records_off_monitor_and_skips_when_disabled() {
        let temp = TempDir::new().unwrap();
        let data_dir = temp.path().join("data");
        let runner_dir = data_dir.join("runners").join("dummy_bench").join("1.0");
        std::fs::create_dir_all(&runner_dir).unwrap();
        let script_path = runner_dir.join("run.sh");
        std::fs::write(
            &script_path,
            "#!/bin/sh\nsleep 0.65\necho \"SCORE: 1234.5\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let profile = BenchmarkProfile {
            id: "dummy_bench".to_string(),
            name: "Dummy Bench".to_string(),
            category: "CPU".to_string(),
            run_cmd: "./run.sh".to_string(),
            score_regex: Some(r"SCORE:\s*([0-9.]+)".to_string()),
            default_version: Some("1.0".to_string()),
            binary_relative_path: Some("run.sh".to_string()),
            ..Default::default()
        };

        let runtime = DaemonRuntime::new_with_config(data_dir, vec![profile])
            .await
            .unwrap();

        let mut settings = runtime.settings.read().await.clone();
        settings.telemetry_interval_ms = 250;
        settings.disable_telemetry_during_runs = false;
        runtime
            .apply_and_save_settings(settings.clone())
            .await
            .unwrap();

        // Off-Monitor idle starts throttled, yet benchmark execution still captures full telemetry.
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );
        let outcome1 = runtime
            .execute_single_benchmark(
                RunBenchmarkParams {
                    id: "dummy_bench".to_string(),
                    version: "1.0".to_string(),
                    gpu_mode: GpuMode::Auto,
                    custom_args: None,
                    duration_secs_override: None,
                },
                false,
            )
            .await
            .unwrap();
        assert_eq!(outcome1.status, "completed");
        assert_eq!(outcome1.score, Some(1234.5));
        let run1_id = outcome1.run_id.unwrap();
        let service = runtime.current_service().await;
        let points1 = service.db.get_telemetry_points(run1_id).unwrap();
        assert!(
            !points1.is_empty(),
            "Off-Monitor run must still record telemetry points at configured cadence"
        );
        assert_eq!(
            *runtime.telemetry_interval_tx.borrow(),
            IDLE_BACKGROUND_TELEMETRY_INTERVAL_MS
        );

        // With disable_telemetry_during_runs = true, even active Monitor view records zero samples.
        settings.disable_telemetry_during_runs = true;
        runtime.apply_and_save_settings(settings).await.unwrap();
        runtime
            .dispatch_request(DaemonRequest::SetMonitorActive(true))
            .await
            .unwrap();
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 250);

        let outcome2 = runtime
            .execute_single_benchmark(
                RunBenchmarkParams {
                    id: "dummy_bench".to_string(),
                    version: "1.0".to_string(),
                    gpu_mode: GpuMode::Auto,
                    custom_args: None,
                    duration_secs_override: None,
                },
                false,
            )
            .await
            .unwrap();
        assert_eq!(outcome2.status, "completed");
        assert_eq!(outcome2.score, Some(1234.5));
        let run2_id = outcome2.run_id.unwrap();
        let points2 = service.db.get_telemetry_points(run2_id).unwrap();
        assert!(
            points2.is_empty(),
            "Runs with disabled telemetry must record zero telemetry points"
        );
        let run2 = service.db.get_run_by_id(run2_id).unwrap().unwrap();
        assert_eq!(run2.avg_cpu_temp, 0.0);
        assert_eq!(run2.peak_cpu_temp, 0.0);
        assert_eq!(run2.avg_power_w, 0.0);
        assert_eq!(run2.cpu_throttling, "");
        assert_eq!(*runtime.telemetry_interval_tx.borrow(), 250);
    }
}
