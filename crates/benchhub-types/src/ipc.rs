use crate::{
    AppSettings, BenchmarkProfile, GpuMode, HistorySortOrder, RunComparisonDiff, RunResult,
    TelemetryData,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum allowed size in bytes for a single newline-delimited IPC frame.
pub const MAX_IPC_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Queued benchmark or stress task managed by the daemon execution queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub version: String,
    pub gpu_mode: GpuMode,
    #[serde(default)]
    pub custom_args: Option<String>,
    #[serde(default)]
    pub duration_secs_override: Option<u64>,
}

/// Active benchmark or installer task metadata exposed by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTaskInfo {
    pub benchmark_id: String,
    pub version: String,
    pub is_install: bool,
    pub is_stress: bool,
    pub started_at_epoch_ms: i64,
}

/// Result summary returned after starting/completing a single benchmark execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutcome {
    pub score: Option<f64>,
    pub status: String,
    pub run_id: Option<i64>,
}

/// Arguments required by the daemon to execute a benchmark or stress test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunBenchmarkParams {
    pub id: String,
    pub version: String,
    pub gpu_mode: GpuMode,
    #[serde(default)]
    pub custom_args: Option<String>,
    #[serde(default)]
    pub duration_secs_override: Option<u64>,
}

/// Structured, language-neutral notification events emitted by the daemon.
/// Clients translate each variant according to their active UI language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaemonNotification {
    BenchmarkCompleted {
        benchmark_id: String,
        version: String,
        score: Option<f64>,
        status: String,
    },
    InstallCompleted {
        benchmark_id: String,
        version: String,
    },
    UninstallCompleted {
        benchmark_id: String,
        version: String,
    },
    TaskStopped {
        benchmark_id: String,
    },
    TaskFailed {
        benchmark_id: String,
        reason: String,
    },
    Enqueued {
        benchmark_id: String,
        version: String,
    },
    QueueCompleted {
        total: usize,
        completed: usize,
        failed: usize,
    },
    AlreadyRunning,
    ExportCompleted {
        path: String,
    },
}

/// Full daemon runtime snapshot synchronized to newly attached clients and on state transitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DaemonStateSnapshot {
    pub installed_map: HashMap<String, bool>,
    pub installed_versions_map: HashMap<String, bool>,
    pub active_task: Option<ActiveTaskInfo>,
    pub queue: Vec<QueueItem>,
    pub is_queue_running: bool,
    pub recent_terminal_logs: Vec<String>,
    pub settings: AppSettings,
}

/// Client-to-daemon JSON RPC requests over Unix domain socket.
/// Runtime metadata returned by `DaemonRequest::GetDaemonInfo` so desktop
/// clients can detect stale or replaced `benchhubd` binaries cleanly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    /// Process ID of the running `benchhubd` instance.
    pub pid: u32,
    /// Package version compiled into `benchhubd`.
    pub version: String,
    /// Executable path of the daemon process captured at startup.
    pub exe_path: String,
    /// Executable file modification timestamp (Unix epoch nanoseconds) at startup.
    pub exe_modified_nanos: u128,
    /// Whether the running executable has been deleted or replaced on disk.
    pub exe_deleted: bool,
    /// Whether any install, benchmark run, stress run, or queue is active.
    pub has_active_task: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", content = "data", rename_all = "snake_case")]
pub enum DaemonRequest {
    Ping,
    GetDaemonInfo,
    Shutdown,
    ClearTerminalLogs,
    SubscribeEvents,
    GetStateSnapshot,
    GetCatalog,
    GetInstalledMap,
    IsVersionInstalled {
        id: String,
        version: String,
    },
    InstallBenchmark {
        id: String,
        version: String,
        #[serde(default)]
        eula_accepted: bool,
    },
    UninstallBenchmark {
        id: String,
        version: String,
    },
    RunBenchmark(RunBenchmarkParams),
    CancelActiveRun,
    EnqueueItem(QueueItem),
    RemoveQueueItem {
        index: usize,
    },
    ClearQueue,
    SetQueue(Vec<QueueItem>),
    RunQueue,
    GetQueue,
    GetEulaText {
        id: String,
    },
    GetSettings,
    SaveSettings(AppSettings),
    GetFilteredHistory {
        search: String,
        category: String,
        gpu_mode: String,
        status: String,
        sort_order: HistorySortOrder,
    },
    DeleteHistory {
        id: i64,
    },
    ClearHistory,
    GetRunById {
        id: i64,
    },
    UpdateRunScore {
        id: i64,
        score: Option<f64>,
    },
    GetRunLog {
        id: i64,
    },
    GetRunsByIds {
        ids: Vec<i64>,
    },
    CompareRuns {
        id_a: i64,
        id_b: i64,
    },
    CreateMethodology {
        child_ids: Vec<i64>,
    },
    GetMethodologyChildren {
        methodology_id: i64,
    },
    SetRunsGroup {
        ids: Vec<i64>,
        group_name: String,
    },
    RemoveRunsFromGroup {
        ids: Vec<i64>,
    },
    GetDistinctGroups,
    DeleteGroup {
        group_name: String,
    },
    RenameGroup {
        old_name: String,
        new_name: String,
    },
    ExportHistoryCsv,
    ExportComparisonPng {
        run_ids: Vec<i64>,
    },
    GetHardwareCapabilities,
    GetPowerProfile,
    GetSystemInfoSummary,
    /// Updates whether the GUI Monitor tab is actively visible.
    SetMonitorActive(bool),
}

/// Strongly-typed response payloads returned by the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DaemonPayload {
    Unit,
    Pong,
    DaemonInfo(DaemonInfo),
    Snapshot(Box<DaemonStateSnapshot>),
    Catalog(Vec<BenchmarkProfile>),
    InstalledMap(HashMap<String, bool>),
    Bool(bool),
    Text(String),
    Settings(Box<AppSettings>),
    History(Vec<RunResult>),
    RunOpt(Option<Box<RunResult>>),
    Run(Box<RunResult>),
    Comparison(Option<Box<RunComparisonDiff>>),
    Groups(Vec<String>),
    RunOutcome(RunOutcome),
    Queue(Vec<QueueItem>),
}

/// Daemon-to-client RPC envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<DaemonPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DaemonResponse {
    pub fn ok(payload: DaemonPayload) -> Self {
        Self {
            ok: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            payload: None,
            error: Some(message.into()),
        }
    }

    pub fn into_result(self) -> Result<DaemonPayload, String> {
        if self.ok {
            Ok(self.payload.unwrap_or(DaemonPayload::Unit))
        } else {
            Err(self
                .error
                .unwrap_or_else(|| "Unknown daemon error".to_string()))
        }
    }
}

/// Server-push events streamed over the dedicated event subscription socket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum DaemonEvent {
    TelemetrySample(TelemetryData),
    TerminalLog(String),
    ClearTerminal,
    StateChanged(Box<DaemonStateSnapshot>),
    Notification(DaemonNotification),
}

/// Resolves the `benchhubd` Unix domain socket path consistently across daemon and desktop client.
pub fn resolve_daemon_socket_path(fallback_uid: u32) -> std::path::PathBuf {
    for (var, append_sock) in [("BENCHHUB_SOCKET_PATH", false), ("XDG_RUNTIME_DIR", true)] {
        if let Ok(val) = std::env::var(var) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                let p = std::path::PathBuf::from(trimmed);
                return if append_sock {
                    p.join("benchhubd.sock")
                } else {
                    p
                };
            }
        }
    }

    std::env::temp_dir()
        .join(format!("benchhub-{}", fallback_uid))
        .join("benchhubd.sock")
}
