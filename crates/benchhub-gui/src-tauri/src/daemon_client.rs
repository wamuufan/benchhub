//! Thin Unix Socket IPC Client for Communicating with `benchhubd`.

use benchhub_types::{
    ipc::MAX_IPC_FRAME_BYTES, DaemonEvent, DaemonInfo, DaemonPayload, DaemonRequest, DaemonResponse,
};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

/// Resolves the `benchhubd` Unix domain socket path using the same convention as the daemon.
pub fn resolve_socket_path() -> PathBuf {
    benchhub_types::resolve_daemon_socket_path(unsafe { libc::geteuid() })
}

fn find_daemon_executable() -> PathBuf {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("benchhubd"));
            candidates.push(exe_dir.join("../deps/benchhubd"));
        }
    }

    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from("target/debug/benchhubd"));
        candidates.push(PathBuf::from("../../target/debug/benchhubd"));
        candidates.push(PathBuf::from("target/release/benchhubd"));
        candidates.push(PathBuf::from("../../target/release/benchhubd"));
    } else {
        candidates.push(PathBuf::from("target/release/benchhubd"));
        candidates.push(PathBuf::from("../../target/release/benchhubd"));
        candidates.push(PathBuf::from("target/debug/benchhubd"));
        candidates.push(PathBuf::from("../../target/debug/benchhubd"));
    }

    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("benchhubd");
            if candidate.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if candidate
                        .metadata()
                        .map(|m| m.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
                    {
                        return candidate;
                    }
                }
                #[cfg(not(unix))]
                return candidate;
            }
        }
    }

    PathBuf::from("benchhubd")
}

fn local_daemon_exe_metadata(daemon_bin: &Path) -> (String, u128) {
    if !daemon_bin.is_file() {
        return (String::new(), 0);
    }
    let canonical = std::fs::canonicalize(daemon_bin).unwrap_or_else(|_| daemon_bin.to_path_buf());
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

fn is_daemon_stale(info: &DaemonInfo, local_exe_path: &str, local_modified_nanos: u128) -> bool {
    if info.exe_deleted || info.version != env!("CARGO_PKG_VERSION") {
        return true;
    }
    if !local_exe_path.is_empty() && !info.exe_path.is_empty() && info.exe_path != local_exe_path {
        return true;
    }
    if local_modified_nanos != 0
        && info.exe_modified_nanos != 0
        && info.exe_modified_nanos != local_modified_nanos
    {
        return true;
    }
    false
}

#[cfg(unix)]
fn is_benchhubd_process(pid: i32) -> bool {
    if let Ok(link) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
        let raw = link.to_string_lossy();
        let trimmed = raw.trim_end_matches(" (deleted)");
        if Path::new(trimmed)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "benchhubd")
        {
            return true;
        }
    }
    if let Ok(comm) = std::fs::read_to_string(format!("/proc/{}/comm", pid)) {
        return comm.trim() == "benchhubd";
    }
    false
}

/// Reads a single newline-terminated UTF-8 IPC frame bounded by `max_bytes`.
async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    out: &mut String,
    max_bytes: usize,
) -> std::io::Result<usize> {
    out.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }
        if let Some(nl_pos) = available.iter().position(|&b| b == b'\n') {
            let to_copy = nl_pos + 1;
            if buf.len().saturating_add(to_copy) > max_bytes {
                reader.consume(to_copy);
                buf.clear();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "IPC frame exceeds maximum allowed size",
                ));
            }
            buf.extend_from_slice(&available[..to_copy]);
            reader.consume(to_copy);
            break;
        } else {
            let len = available.len();
            if buf.len().saturating_add(len) > max_bytes {
                reader.consume(len);
                buf.clear();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "IPC frame exceeds maximum allowed size",
                ));
            }
            buf.extend_from_slice(available);
            reader.consume(len);
        }
    }
    let n = buf.len();
    let text = std::str::from_utf8(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.push_str(text);
    buf.clear();
    Ok(n)
}

/// Asynchronous IPC client managing daemon auto-launch, RPC calls, and live event forwarding.
pub struct DaemonClient {
    socket_path: PathBuf,
    spawn_lock: Mutex<()>,
    verified: AtomicBool,
    shutting_down: AtomicBool,
    monitor_active: AtomicBool,
}

impl DaemonClient {
    pub fn new(socket_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            socket_path,
            spawn_lock: Mutex::new(()),
            verified: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            monitor_active: AtomicBool::new(false),
        })
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    async fn send_once(&self, req: &DaemonRequest) -> Result<DaemonResponse, String> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| format!("Failed to connect to benchhubd: {}", e))?;

        let (read_half, mut write_half) = stream.into_split();
        let encoded = serde_json::to_string(req)
            .map_err(|e| format!("Failed to serialize daemon request: {}", e))?
            + "\n";

        write_half
            .write_all(encoded.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to daemon socket: {}", e))?;
        write_half
            .flush()
            .await
            .map_err(|e| format!("Failed to flush daemon socket: {}", e))?;

        let mut reader = BufReader::new(read_half);
        let mut raw_buf = Vec::new();
        let mut line = String::new();
        let n = read_bounded_line(&mut reader, &mut raw_buf, &mut line, MAX_IPC_FRAME_BYTES)
            .await
            .map_err(|e| format!("Failed to read response from daemon socket: {}", e))?;
        if n == 0 {
            return Err("Daemon closed IPC connection unexpectedly".to_string());
        }

        serde_json::from_str::<DaemonResponse>(&line)
            .map_err(|e| format!("Malformed daemon response JSON: {}", e))
    }

    async fn send_once_timeout(
        &self,
        req: &DaemonRequest,
        timeout: std::time::Duration,
    ) -> Result<DaemonResponse, String> {
        match tokio::time::timeout(timeout, self.send_once(req)).await {
            Ok(res) => res,
            Err(_) => Err("Timed out waiting for benchhubd response".to_string()),
        }
    }

    async fn ping_once(&self) -> bool {
        matches!(
            self.send_once(&DaemonRequest::Ping).await,
            Ok(resp) if resp.ok && matches!(resp.payload, Some(DaemonPayload::Pong))
        )
    }

    async fn terminate_legacy_idle_daemon(&self) -> bool {
        #[cfg(unix)]
        {
            let Ok(stream) = UnixStream::connect(&self.socket_path).await else {
                return false;
            };
            let Ok(cred) = stream.peer_cred() else {
                return false;
            };
            let my_uid = unsafe { libc::geteuid() };
            let Some(pid) = cred.pid() else {
                return false;
            };
            if cred.uid() != my_uid || pid <= 0 || pid as u32 == std::process::id() {
                return false;
            }
            if !is_benchhubd_process(pid) {
                return false;
            }
            drop(stream);
            eprintln!(
                "[benchhub-desktop] Stopping legacy pre-handshake idle benchhubd PID {}",
                pid
            );
            unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    async fn wait_until_daemon_stopped_within(&self, max_attempts: usize) -> bool {
        for _ in 0..max_attempts {
            if !self.ping_once().await {
                if self.socket_path.exists()
                    && UnixStream::connect(&self.socket_path).await.is_err()
                {
                    let _ = std::fs::remove_file(&self.socket_path);
                }
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        false
    }

    async fn wait_until_daemon_stopped(&self) {
        let _ = self.wait_until_daemon_stopped_within(60).await;
    }

    /// Checks whether the currently reachable daemon matches the local binary; replaces it when idle.
    /// Returns `true` if a suitable daemon is ready to use without spawning a new one.
    async fn check_or_replace_existing_daemon(&self, daemon_bin: &Path) -> bool {
        if !self.ping_once().await {
            return false;
        }

        let (local_exe_path, local_modified_nanos) = local_daemon_exe_metadata(daemon_bin);
        match self.send_once(&DaemonRequest::GetDaemonInfo).await {
            Ok(resp) if resp.ok => {
                if let Some(DaemonPayload::DaemonInfo(info)) = resp.payload {
                    if !is_daemon_stale(&info, &local_exe_path, local_modified_nanos) {
                        return true;
                    }
                    if info.has_active_task {
                        eprintln!(
                            "[benchhub-desktop] Running benchhubd PID {} is stale, keeping active task alive",
                            info.pid
                        );
                        return true;
                    }
                    eprintln!(
                        "[benchhub-desktop] Replacing stale idle benchhubd PID {} with '{}'",
                        info.pid,
                        daemon_bin.display()
                    );
                    if self.send_once(&DaemonRequest::Shutdown).await.is_ok() {
                        self.wait_until_daemon_stopped().await;
                        return false;
                    }
                }
                true
            }
            Ok(resp) if !resp.ok => {
                // Legacy pre-GetDaemonInfo daemon detected; check active status before replacing.
                if let Ok(snap_resp) = self.send_once(&DaemonRequest::GetStateSnapshot).await {
                    if let Some(DaemonPayload::Snapshot(snap)) = snap_resp.payload {
                        if snap.active_task.is_some() || snap.is_queue_running {
                            eprintln!(
                                "[benchhub-desktop] Legacy benchhubd has an active task; skipping restart"
                            );
                            return true;
                        }
                    }
                }
                if self.terminate_legacy_idle_daemon().await {
                    self.wait_until_daemon_stopped().await;
                    return false;
                }
                true
            }
            Ok(_) | Err(_) => false,
        }
    }

    /// Ensures `benchhubd` is listening on the IPC socket, spawning or refreshing it when needed.
    pub async fn ensure_daemon(&self) -> Result<(), String> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("Desktop client is shutting down".to_string());
        }
        if self.verified.load(Ordering::Acquire) && self.ping_once().await {
            return Ok(());
        }

        let _guard = self.spawn_lock.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("Desktop client is shutting down".to_string());
        }
        if self.verified.load(Ordering::Acquire) && self.ping_once().await {
            return Ok(());
        }

        let daemon_bin = find_daemon_executable();
        if self.check_or_replace_existing_daemon(&daemon_bin).await {
            self.verified.store(true, Ordering::Release);
            return Ok(());
        }

        if let Some(parent) = self.socket_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
        }

        let mut cmd = std::process::Command::new(&daemon_bin);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("BENCHHUB_PARENT_PID", std::process::id().to_string());

        if !self.socket_path.as_os_str().is_empty() {
            cmd.env("BENCHHUB_SOCKET_PATH", &self.socket_path);
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        cmd.spawn().map_err(|e| {
            format!(
                "Failed to launch background daemon '{}': {}",
                daemon_bin.display(),
                e
            )
        })?;

        for _ in 0..125 {
            if self.shutting_down.load(Ordering::Acquire) {
                return Err("Desktop client is shutting down".to_string());
            }
            if self.ping_once().await {
                self.verified.store(true, Ordering::Release);
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }

        Err(format!(
            "Timed out waiting for benchhubd socket at {:?}",
            self.socket_path
        ))
    }

    /// Marks the GUI as exiting, prevents background reconnects from respawning `benchhubd`,
    /// cancels any active benchmark/queue work, and stops the attached daemon cleanly.
    pub async fn shutdown_daemon(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.verified.store(false, Ordering::Release);

        // Wait if an initial spawn is mid-flight so we do not orphan a newly launched daemon.
        let _guard = self.spawn_lock.lock().await;

        if !self.socket_path.exists() {
            return;
        }

        let daemon_pid = match self
            .send_once_timeout(
                &DaemonRequest::GetDaemonInfo,
                std::time::Duration::from_millis(300),
            )
            .await
        {
            Ok(resp) if resp.ok => match resp.payload {
                Some(DaemonPayload::DaemonInfo(info)) => Some(info.pid as i32),
                _ => None,
            },
            Ok(_) => None,
            Err(_) if UnixStream::connect(&self.socket_path).await.is_err() => {
                let _ = std::fs::remove_file(&self.socket_path);
                return;
            }
            Err(_) => None,
        };

        let graceful_ok = matches!(
            self.send_once_timeout(&DaemonRequest::Shutdown, std::time::Duration::from_millis(400))
                .await,
            Ok(resp) if resp.ok
        );

        if graceful_ok && self.wait_until_daemon_stopped_within(30).await {
            return;
        }

        // Active task or queue prevented immediate Shutdown; cancel and retry before falling back.
        let _ = self
            .send_once_timeout(
                &DaemonRequest::ClearQueue,
                std::time::Duration::from_millis(250),
            )
            .await;
        let _ = self
            .send_once_timeout(
                &DaemonRequest::CancelActiveRun,
                std::time::Duration::from_millis(250),
            )
            .await;

        for _ in 0..8 {
            if matches!(
                self.send_once_timeout(
                    &DaemonRequest::Shutdown,
                    std::time::Duration::from_millis(250),
                )
                .await,
                Ok(resp) if resp.ok
            ) && self.wait_until_daemon_stopped_within(20).await
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        #[cfg(unix)]
        if let Some(pid) = daemon_pid {
            if pid > 1 && pid as u32 != std::process::id() && is_benchhubd_process(pid) {
                eprintln!(
                    "[benchhub-desktop] Stopping benchhubd PID {} on GUI exit",
                    pid
                );
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                let _ = self.wait_until_daemon_stopped_within(10).await;
            }
        }
    }

    /// Sends an RPC request to `benchhubd`, auto-spawning or refreshing the daemon when needed.
    pub async fn call(&self, req: &DaemonRequest) -> Result<DaemonPayload, String> {
        if !self.verified.load(Ordering::Acquire) {
            self.ensure_daemon().await?;
        }
        let resp = match self.send_once(req).await {
            Ok(r) => r,
            Err(_) => {
                self.verified.store(false, Ordering::Release);
                self.ensure_daemon().await?;
                self.send_once(req).await?
            }
        };
        resp.into_result()
    }

    /// Updates whether the GUI Monitor tab is active and syncs the daemon sampling cadence.
    pub async fn set_monitor_active(&self, active: bool) -> Result<(), String> {
        self.monitor_active.store(active, Ordering::Release);
        self.call(&DaemonRequest::SetMonitorActive(active)).await?;
        Ok(())
    }

    /// Maintains a persistent event subscription with `benchhubd` and forwards events to the WebView.
    pub async fn run_event_forwarder(self: Arc<Self>, app_handle: tauri::AppHandle) {
        const INITIAL_BACKOFF_MS: u64 = 250;
        const MAX_BACKOFF_MS: u64 = 3000;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        while !self.shutting_down.load(Ordering::Acquire) {
            if let Err(err) = self.ensure_daemon().await {
                if self.shutting_down.load(Ordering::Acquire) {
                    break;
                }
                eprintln!(
                    "[benchhub-desktop] Daemon unavailable for event stream: {}",
                    err
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                continue;
            }

            let stream = match UnixStream::connect(&self.socket_path).await {
                Ok(s) => s,
                Err(err) => {
                    if self.shutting_down.load(Ordering::Acquire) {
                        break;
                    }
                    eprintln!("[benchhub-desktop] Failed to open event stream: {}", err);
                    self.verified.store(false, Ordering::Release);
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                    continue;
                }
            };

            let (read_half, mut write_half) = stream.into_split();
            let sub_payload = match serde_json::to_string(&DaemonRequest::SubscribeEvents) {
                Ok(s) => s + "\n",
                Err(_) => break,
            };
            if write_half.write_all(sub_payload.as_bytes()).await.is_err()
                || write_half.flush().await.is_err()
            {
                self.verified.store(false, Ordering::Release);
                if self.shutting_down.load(Ordering::Acquire) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                continue;
            }

            // Synchronize monitor visibility after daemon startup or reconnect so active Monitor views stay responsive.
            if self.monitor_active.load(Ordering::Acquire) {
                let _ = self.send_once(&DaemonRequest::SetMonitorActive(true)).await;
            }

            let mut reader = BufReader::new(read_half);
            let mut raw_buf = Vec::new();
            let mut line = String::new();
            loop {
                line.clear();
                match read_bounded_line(&mut reader, &mut raw_buf, &mut line, MAX_IPC_FRAME_BYTES)
                    .await
                {
                    Ok(0) => break,
                    Ok(_) => {
                        backoff_ms = INITIAL_BACKOFF_MS;
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<DaemonEvent>(trimmed) {
                            Ok(event) => match event {
                                DaemonEvent::TelemetrySample(sample) => {
                                    if self.monitor_active.load(Ordering::Acquire) {
                                        let _ = app_handle.emit("telemetry-sample", &sample);
                                    }
                                }
                                DaemonEvent::TerminalLog(chunk) => {
                                    let _ = app_handle.emit("terminal-log", &chunk);
                                }
                                DaemonEvent::ClearTerminal => {
                                    let _ = app_handle.emit("terminal-clear", ());
                                }
                                DaemonEvent::StateChanged(snapshot) => {
                                    let _ = app_handle.emit("daemon-state", snapshot.as_ref());
                                }
                                DaemonEvent::Notification(notification) => {
                                    let _ = app_handle.emit("daemon-notification", &notification);
                                }
                            },
                            Err(err) => {
                                eprintln!(
                                    "[benchhub-desktop] Failed to decode daemon event: {}",
                                    err
                                );
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[benchhub-desktop] Event stream read error: {}", err);
                        break;
                    }
                }
            }

            self.verified.store(false, Ordering::Release);
            if self.shutting_down.load(Ordering::Acquire) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn shutdown_daemon_sends_shutdown_and_blocks_further_spawns() {
        let sock_path = std::env::temp_dir().join(format!(
            "benchhub-client-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let sock_clone = sock_path.clone();
        let server = tokio::spawn(async move {
            let mut saw_shutdown = false;
            for _ in 0..4 {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    continue;
                }
                let req: DaemonRequest = serde_json::from_str(line.trim()).unwrap();
                match req {
                    DaemonRequest::GetDaemonInfo => {
                        let resp = DaemonResponse::err("skip info");
                        let encoded = serde_json::to_string(&resp).unwrap() + "\n";
                        let _ = write_half.write_all(encoded.as_bytes()).await;
                        let _ = write_half.flush().await;
                    }
                    DaemonRequest::Shutdown => {
                        saw_shutdown = true;
                        let resp = DaemonResponse::ok(DaemonPayload::Unit);
                        let encoded = serde_json::to_string(&resp).unwrap() + "\n";
                        let _ = write_half.write_all(encoded.as_bytes()).await;
                        let _ = write_half.flush().await;
                        drop(listener);
                        let _ = std::fs::remove_file(&sock_clone);
                        break;
                    }
                    _ => {}
                }
            }
            saw_shutdown
        });

        let client = DaemonClient::new(sock_path.clone());
        client.shutdown_daemon().await;

        assert!(
            server.await.unwrap(),
            "Client should send DaemonRequest::Shutdown"
        );
        let respawn_err = client.ensure_daemon().await.unwrap_err();
        assert!(respawn_err.contains("shutting down"));
        let _ = std::fs::remove_file(&sock_path);
    }
}
