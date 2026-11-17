pub mod adapters;
pub mod platform_env;

pub use adapters::{
    adapter_for, apply_duration_override, ensure_blender_runner, ensure_cray_runner,
    ensure_fio_runner, ensure_profile_runtime_args, ensure_stream_runner,
};
pub use platform_env::{
    ensure_executable_permissions, ensure_gthread_shim, find_system_ca_bundle, find_system_ca_dir,
    inject_gpu_env, inject_graphics_env, inject_ssl_env, is_archive_or_installer,
    is_dynamically_broken, is_dynamically_linked_elf, is_elf_or_script, kill_process_group,
    list_dir_contents_recursive, set_permissions_recursive, RuntimeSandbox,
};

use crate::models::{BenchmarkProfile, GpuMode};
use crate::utils::{ensure_path_within, sanitize_identifier};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Error returned when an active benchmark installation is cancelled by the client.
#[derive(Debug)]
pub struct InstallCancelledError;

impl std::fmt::Display for InstallCancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Installation cancelled by user")
    }
}

impl std::error::Error for InstallCancelledError {}

fn format_bytes_human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_elapsed_mm_ss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Inspects total regular-file bytes and file count under `dir` (bounded traversal).
async fn inspect_dir_progress(dir: &Path) -> (u64, usize) {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return (0, 0);
    }
    let mut total_bytes = 0u64;
    let mut file_count = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    let mut visited_entries = 0usize;

    while let Some(current) = stack.pop() {
        let Ok(mut rd) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            visited_entries += 1;
            if visited_entries > 25_000 {
                return (total_bytes, file_count);
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total_bytes = total_bytes.saturating_add(meta.len());
                file_count += 1;
            }
        }
    }

    (total_bytes, file_count)
}

/// Decodes raw terminal bytes into UTF-8 while normalizing standalone Latin-1/CP1252
/// degree symbols (`0xB0`, emitted by tools such as FurMark) into UTF-8 `°`.
fn decode_terminal_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(bytes.len() + 8);
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(err) => {
                let (valid, after_valid) = rest.split_at(err.valid_up_to());
                if let Ok(s) = std::str::from_utf8(valid) {
                    out.push_str(s);
                }
                let err_len = err.error_len().unwrap_or(1);
                let (invalid, remainder) = after_valid.split_at(err_len.min(after_valid.len()));
                if invalid == [0xB0] {
                    out.push('°');
                } else {
                    out.push('\u{FFFD}');
                }
                rest = remainder;
            }
        }
    }
    out
}

/// Spawns an async reader over stdout/stderr that streams both finalized `\n` lines
/// and in-place `\r` progress updates without blocking on carriage returns.
fn spawn_terminal_stream_reader<R>(
    mut stream: R,
    tx: tokio::sync::mpsc::Sender<String>,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut read_buf = [0u8; 4096];
        let mut pending: Vec<u8> = Vec::with_capacity(512);
        let mut skipped_cr: Option<Vec<u8>> = None;
        let mut last_cr_emit = tokio::time::Instant::now() - tokio::time::Duration::from_secs(1);

        loop {
            let n = match stream.read(&mut read_buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            for &b in &read_buf[..n] {
                if b == b'\n' {
                    if pending.last() == Some(&b'\r') {
                        pending.pop();
                    }
                    // Preserve a throttled `\r` progress line if `\n` immediately follows (`\r\n`).
                    let line_bytes = if pending.is_empty() {
                        skipped_cr.take()
                    } else {
                        skipped_cr = None;
                        Some(std::mem::take(&mut pending))
                    };
                    let mut text = line_bytes
                        .map(|bytes| decode_terminal_bytes(&bytes))
                        .unwrap_or_default();
                    text.push('\n');
                    if tx.send(text).await.is_err() {
                        return;
                    }
                } else if b == b'\r' {
                    if !pending.is_empty() {
                        if last_cr_emit.elapsed() >= tokio::time::Duration::from_millis(200) {
                            skipped_cr = None;
                            let mut text = decode_terminal_bytes(&pending);
                            text.push('\r');
                            last_cr_emit = tokio::time::Instant::now();
                            pending.clear();
                            if tx.send(text).await.is_err() {
                                return;
                            }
                        } else {
                            skipped_cr = Some(std::mem::take(&mut pending));
                        }
                    }
                } else {
                    pending.push(b);
                    // Guard against unbounded memory growth on gigantic line without delimiters
                    if pending.len() >= 16_384 {
                        skipped_cr = None;
                        let text = decode_terminal_bytes(&pending);
                        pending.clear();
                        if tx.send(text).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }

        let final_line = if !pending.is_empty() {
            Some(pending)
        } else {
            skipped_cr.take()
        };
        if let Some(bytes) = final_line {
            let mut text = decode_terminal_bytes(&bytes);
            text.push('\n');
            let _ = tx.send(text).await;
        }
    })
}

/// Processes a single output chunk from a running benchmark: extracts scores, writes the log,
/// retains a bounded tail buffer for adapter fallbacks, and forwards it to the UI callback.
fn record_run_output_chunk(
    chunk: String,
    regex: &Option<Regex>,
    score: &mut Option<f64>,
    trailing_cr: &mut bool,
    log_file: &mut Option<std::io::BufWriter<std::fs::File>>,
    captured_tail: &mut String,
    on_output: &mut impl FnMut(String),
) {
    if let Some(s) = BenchmarkEngine::extract_score_from_line(regex, &chunk) {
        *score = Some(s);
    }
    *trailing_cr = chunk.ends_with('\r');
    if !chunk.ends_with('\r') {
        if let Some(f) = log_file {
            use std::io::Write;
            let _ = f.write_all(chunk.as_bytes());
        }
        captured_tail.push_str(&chunk);
        if captured_tail.len() > 32_768 {
            let excess = captured_tail.len() - 16_384;
            let cut = (excess..captured_tail.len())
                .find(|&i| captured_tail.is_char_boundary(i))
                .unwrap_or(excess);
            captured_tail.drain(..cut);
        }
    }
    on_output(chunk);
}

/// Awaits stdout/stderr reader tasks (bounded by a short timeout) so EOF flushes
/// complete before draining any remaining buffered chunks from `out_rx`.
async fn join_and_drain_stream_output<F>(
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
    out_rx: &mut tokio::sync::mpsc::Receiver<String>,
    mut on_chunk: F,
) where
    F: FnMut(String),
{
    let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
        let _ = tokio::join!(stdout_task, stderr_task);
    })
    .await;

    while let Ok(chunk) = out_rx.try_recv() {
        on_chunk(chunk);
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkRunOutput {
    pub score: Option<f64>,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Clone)]
pub struct BenchmarkEngine {
    base_dir: PathBuf,
    runners_dir: Option<PathBuf>,
}

impl BenchmarkEngine {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            runners_dir: None,
        }
    }

    pub fn with_runners_dir(mut self, runners_dir: Option<PathBuf>) -> Self {
        self.runners_dir = runners_dir;
        self
    }

    pub fn get_runners_dir(&self) -> PathBuf {
        if let Some(ref custom) = self.runners_dir {
            if !custom.as_os_str().is_empty() {
                return custom.clone();
            }
        }
        self.base_dir.join("runners")
    }

    pub fn get_runner_dir(&self, id: &str) -> Result<PathBuf> {
        let safe_id = sanitize_identifier(id)?;
        let runner_dir = self.get_runners_dir().join(safe_id);
        ensure_path_within(&self.get_runners_dir(), &runner_dir)
    }

    pub fn get_version_runner_dir(&self, id: &str, version: &str) -> Result<PathBuf> {
        let safe_id = sanitize_identifier(id)?;
        let runners_dir = self.get_runners_dir();
        let target = if version.is_empty() {
            runners_dir.join(safe_id)
        } else {
            let safe_version = sanitize_identifier(version)?;
            runners_dir.join(safe_id).join(safe_version)
        };
        ensure_path_within(&runners_dir, &target)
    }

    pub async fn get_active_runner_dir_async(&self, id: &str, version: &str) -> Result<PathBuf> {
        let ver_dir = self.get_version_runner_dir(id, version)?;
        if tokio::fs::try_exists(ver_dir.join(".successfully-installed"))
            .await
            .unwrap_or(false)
        {
            return Ok(ver_dir);
        }
        if !version.is_empty() {
            let major = version.split('.').next().unwrap_or(version);
            if let Ok(safe_major) = sanitize_identifier(major) {
                let safe_id = sanitize_identifier(id)?;
                let suffixed = self
                    .get_runners_dir()
                    .join(format!("{}{}", safe_id, safe_major));
                if let Ok(safe_suffixed) = ensure_path_within(&self.get_runners_dir(), &suffixed) {
                    if tokio::fs::try_exists(safe_suffixed.join(".successfully-installed"))
                        .await
                        .unwrap_or(false)
                    {
                        return Ok(safe_suffixed);
                    }
                }
            }
            return Ok(ver_dir);
        }
        let base_dir = self.get_runner_dir(id)?;
        if tokio::fs::try_exists(base_dir.join(".successfully-installed"))
            .await
            .unwrap_or(false)
        {
            return Ok(base_dir);
        }
        if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let p = entry.path();
                if let Ok(meta) = entry.metadata().await {
                    if meta.is_dir()
                        && tokio::fs::try_exists(p.join(".successfully-installed"))
                            .await
                            .unwrap_or(false)
                    {
                        if let Ok(safe_p) = ensure_path_within(&self.get_runners_dir(), &p) {
                            return Ok(safe_p);
                        }
                    }
                }
            }
        }
        if tokio::fs::try_exists(&ver_dir).await.unwrap_or(false) {
            return Ok(ver_dir);
        }
        Ok(base_dir)
    }

    fn is_installed_dir(path: &Path) -> bool {
        path.join(".successfully-installed").exists()
    }

    pub fn is_installed(&self, id: &str) -> bool {
        match self.get_runner_dir(id) {
            Ok(dir) => Self::is_installed_dir(&dir),
            Err(_) => false,
        }
    }

    pub async fn is_version_installed_async(&self, id: &str, version: &str) -> bool {
        let ver_dir = match self.get_version_runner_dir(id, version) {
            Ok(d) => d,
            Err(_) => return false,
        };
        if tokio::fs::try_exists(ver_dir.join(".successfully-installed"))
            .await
            .unwrap_or(false)
        {
            return true;
        }
        if !version.is_empty() {
            let major = version.split('.').next().unwrap_or(version);
            if let Ok(safe_major) = sanitize_identifier(major) {
                if let Ok(safe_id) = sanitize_identifier(id) {
                    let suffixed = self
                        .get_runners_dir()
                        .join(format!("{}{}", safe_id, safe_major));
                    if let Ok(safe_suffixed) =
                        ensure_path_within(&self.get_runners_dir(), &suffixed)
                    {
                        if tokio::fs::try_exists(safe_suffixed.join(".successfully-installed"))
                            .await
                            .unwrap_or(false)
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        let base_dir = match self.get_runner_dir(id) {
            Ok(d) => d,
            Err(_) => return false,
        };
        if tokio::fs::try_exists(base_dir.join(".successfully-installed"))
            .await
            .unwrap_or(false)
        {
            return true;
        }
        if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let p = entry.path();
                if let Ok(meta) = entry.metadata().await {
                    if meta.is_dir()
                        && tokio::fs::try_exists(p.join(".successfully-installed"))
                            .await
                            .unwrap_or(false)
                        && ensure_path_within(&self.get_runners_dir(), &p).is_ok()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[tracing::instrument(skip(self))]
    pub async fn uninstall(&self, id: &str) -> Result<()> {
        let runner_dir = self.get_runner_dir(id)?;
        let validated = ensure_path_within(&self.get_runners_dir(), &runner_dir)?;
        if validated.exists() {
            tokio::fs::remove_dir_all(&validated)
                .await
                .with_context(|| format!("Failed to remove runner directory {:?}", validated))?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub async fn uninstall_version(&self, id: &str, version: &str) -> Result<()> {
        let runners_dir = self.get_runners_dir();

        if let Ok(ver_dir) = self.get_version_runner_dir(id, version) {
            if ver_dir.exists() {
                if let Ok(safe_ver_dir) = ensure_path_within(&runners_dir, &ver_dir) {
                    let _ = tokio::fs::remove_dir_all(&safe_ver_dir).await;
                }
            }
        }
        if !version.is_empty() {
            let major = version.split('.').next().unwrap_or(version);
            if let (Ok(safe_id), Ok(safe_major)) =
                (sanitize_identifier(id), sanitize_identifier(major))
            {
                let suffixed = runners_dir.join(format!("{}{}", safe_id, safe_major));
                if suffixed.exists() {
                    if let Ok(safe_suffixed) = ensure_path_within(&runners_dir, &suffixed) {
                        let _ = tokio::fs::remove_dir_all(&safe_suffixed).await;
                    }
                }
            }
        }
        if let Ok(base_dir) = self.get_runner_dir(id) {
            if base_dir.exists() {
                if let Ok(safe_base_dir) = ensure_path_within(&runners_dir, &base_dir) {
                    if version.is_empty() || Self::is_installed_dir(&safe_base_dir) {
                        let _ = tokio::fs::remove_dir_all(&safe_base_dir).await;
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates a download command string for basic security and shell injection prevention.
    pub fn validate_download_command(cmd: &str) -> Result<()> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            anyhow::bail!("Download command cannot be empty");
        }
        if trimmed.contains('\0') {
            anyhow::bail!("Download command contains forbidden null byte");
        }

        let lower = trimmed.to_lowercase();

        // Reject privilege escalation commands
        let forbidden_escalation = ["sudo", "pkexec", "doas", "su"];
        for token in lower.split(|c: char| {
            c == ';'
                || c == '&'
                || c == '|'
                || c == '\n'
                || c == '('
                || c == ')'
                || c.is_whitespace()
        }) {
            if forbidden_escalation.contains(&token) {
                anyhow::bail!(
                    "Download command contains forbidden privilege escalation: '{}'",
                    token
                );
            }
        }

        // Reject dangerous patterns and reverse shell constructs
        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            "rm -r /",
            "rm -r /*",
            "mkfs",
            "dd if=/dev/zero",
            "dd if=/dev/urandom",
            ":(){ :|:& };:",
            "/dev/tcp/",
            "/dev/udp/",
            "| sh",
            "| bash",
            "| zsh",
        ];
        for pat in &dangerous_patterns {
            if lower.contains(pat) {
                anyhow::bail!("Download command contains dangerous pattern: '{}'", pat);
            }
        }

        // Reject unauthorized targeting of sensitive system directories
        let forbidden_targets = [
            "> /etc",
            "> /boot",
            "> /dev/sd",
            "> /dev/nvme",
            "/etc/shadow",
            "/etc/passwd",
            "/root/",
        ];
        for target in &forbidden_targets {
            if lower.contains(target) {
                anyhow::bail!(
                    "Download command references forbidden system path: '{}'",
                    target
                );
            }
        }

        Ok(())
    }

    async fn execute_command_with_output<F>(
        mut command: tokio::process::Command,
        mut cancel_rx: tokio::sync::watch::Receiver<bool>,
        monitor: Option<(&Path, &str)>,
        mut on_output: F,
    ) -> Result<()>
    where
        F: FnMut(String) + Send,
    {
        if *cancel_rx.borrow() {
            on_output("\n[BenchHub] Installation cancelled by user.\n".to_string());
            return Err(InstallCancelledError.into());
        }

        command.process_group(0);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .context("Installation command failed to start")?;

        let child_pid = child.id();
        let stdout = child.stdout.take().context("Failed to open stdout")?;
        let stderr = child.stderr.take().context("Failed to open stderr")?;

        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<String>(1024);
        let stdout_task = spawn_terminal_stream_reader(stdout, out_tx.clone());
        let stderr_task = spawn_terminal_stream_reader(stderr, out_tx);

        let started_at = tokio::time::Instant::now();
        let mut last_child_output = started_at;
        let mut last_heartbeat = started_at;
        let mut prev_bytes: Option<u64> = None;
        let mut last_tick_at = started_at;
        let mut trailing_cr = false;
        let mut exit_status = None;

        let mut poll_interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        if let Some(pid) = child_pid {
                            kill_process_group(pid);
                        }
                        let _ = child.kill().await;
                        let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
                            let _ = tokio::join!(stdout_task, stderr_task);
                        }).await;
                        if trailing_cr {
                            on_output("\n".to_string());
                        }
                        on_output("[BenchHub] Installation cancelled by user.\n".to_string());
                        return Err(InstallCancelledError.into());
                    }
                }
                status_res = child.wait(), if exit_status.is_none() => {
                    exit_status = status_res.ok();
                }
                maybe_chunk = out_rx.recv() => {
                    match maybe_chunk {
                        Some(chunk) => {
                            last_child_output = tokio::time::Instant::now();
                            trailing_cr = chunk.ends_with('\r');
                            on_output(chunk);
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = poll_interval.tick(), if monitor.is_some() && exit_status.is_none() => {
                    let Some((watch_dir, phase_label)) = monitor else {
                        continue;
                    };
                    let now = tokio::time::Instant::now();
                    if now.duration_since(started_at) < tokio::time::Duration::from_secs(2) {
                        continue;
                    }

                    let (bytes, files) = inspect_dir_progress(watch_dir).await;
                    let dt = now.duration_since(last_tick_at).as_secs_f64().max(0.1);
                    let changed = prev_bytes.map(|p| bytes != p).unwrap_or(bytes > 0);
                    let rate_bps = prev_bytes
                        .map(|p| bytes.saturating_sub(p) as f64 / dt)
                        .unwrap_or(0.0);
                    prev_bytes = Some(bytes);
                    last_tick_at = now;

                    let quiet_secs = now.duration_since(last_child_output).as_secs();
                    let since_hb_secs = now.duration_since(last_heartbeat).as_secs();
                    if (quiet_secs >= 3 && changed) || (quiet_secs >= 8 && since_hb_secs >= 8) {
                        last_heartbeat = now;
                        let elapsed = format_elapsed_mm_ss(now.duration_since(started_at).as_secs());
                        let msg = if rate_bps > 1024.0 {
                            format!(
                                "\r[BenchHub] {}... {} ({} files, {}/s, elapsed {})\r",
                                phase_label,
                                format_bytes_human(bytes),
                                files,
                                format_bytes_human(rate_bps as u64),
                                elapsed
                            )
                        } else if bytes > 0 {
                            format!(
                                "\r[BenchHub] {}... {} on disk ({} files, elapsed {})\r",
                                phase_label,
                                format_bytes_human(bytes),
                                files,
                                elapsed
                            )
                        } else {
                            format!(
                                "\r[BenchHub] {}... active in background (elapsed {})\r",
                                phase_label,
                                elapsed
                            )
                        };
                        trailing_cr = true;
                        on_output(msg);
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(150)), if exit_status.is_some() => {
                    break;
                }
            }
        }

        join_and_drain_stream_output(stdout_task, stderr_task, &mut out_rx, |chunk| {
            trailing_cr = chunk.ends_with('\r');
            on_output(chunk);
        })
        .await;

        if trailing_cr {
            on_output("\n".to_string());
        }

        let status = match exit_status {
            Some(st) => st,
            None => child.wait().await?,
        };

        if !status.success() {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string());
            anyhow::bail!("Installation failed (exit code: {})", code);
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn install_version<F>(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
        on_output: F,
    ) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        self.install_version_cancellable(profile, version, rx, on_output)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn install_version_cancellable<F>(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        mut on_output: F,
    ) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        let effective_profile = profile.for_version(version);
        let runner_dir = self.get_version_runner_dir(&profile.id, version)?;

        let mut backup_dir = None;
        if runner_dir.exists() {
            let backup_path = runner_dir.with_extension("bak");
            let _ = tokio::fs::remove_dir_all(&backup_path).await;
            if tokio::fs::rename(&runner_dir, &backup_path).await.is_ok() {
                backup_dir = Some(backup_path);
            }
        }

        tokio::fs::create_dir_all(&runner_dir)
            .await
            .with_context(|| format!("Failed to create runner directory {:?}", runner_dir))?;

        let size_hint = effective_profile.display_download_size();
        on_output(format!(
            "[BenchHub] Installing: {} (Est. size: {})\n",
            effective_profile.name, size_hint
        ));

        let res: Result<()> = async {
            let is_run;

            if let Some(cmd_str) = &effective_profile.download_cmd {
                Self::validate_download_command(cmd_str)?;
                is_run = false;
                on_output("[BenchHub] Downloading & preparing package...\n".to_string());
                let mut command = Command::new("sh");
                if let Ok(cur_path) = std::env::var("PATH") {
                    if !cur_path.starts_with("/bin:") && !cur_path.starts_with("/usr/bin:") {
                        command.env("PATH", format!("/bin:/usr/bin:{}", cur_path));
                    }
                }
                command
                    .env("LC_ALL", "C")
                    .arg("-c")
                    .arg(cmd_str)
                    .current_dir(&runner_dir);
                inject_ssl_env(&mut command).await;
                Self::execute_command_with_output(
                    command,
                    cancel_rx.clone(),
                    Some((&runner_dir, "Downloading / Installing")),
                    &mut on_output,
                )
                .await?;
            } else if let Some(ref url) = effective_profile.download_url {
                is_run = effective_profile.archive_type.as_deref() == Some("run")
                    || url.ends_with(".run");
                let url_path = url.split('?').next().unwrap_or(url);
                let run_filename = url_path.rsplit('/').next().unwrap_or("package.run");

                if url.contains('\'')
                    || url.contains('"')
                    || url.contains(' ')
                    || url.contains(';')
                    || url.contains('&')
                {
                    anyhow::bail!("URL contains invalid characters: {}", url);
                }
                if run_filename.contains('/')
                    || run_filename.contains('\\')
                    || run_filename.is_empty()
                {
                    anyhow::bail!("Extracted filename from URL is invalid: {}", run_filename);
                }

                let extracted_dir = runner_dir.join("extracted");
                tokio::fs::create_dir_all(&extracted_dir)
                    .await
                    .with_context(|| {
                        format!("Failed to create extracted directory {:?}", extracted_dir)
                    })?;

                on_output(format!(
                    "[BenchHub] [1/2] Downloading {} from {}...\n",
                    run_filename, url_path
                ));

                let mut wget = Command::new("wget");
                wget.env("LC_ALL", "C")
                    .args([
                        "--progress=bar:force:noscroll",
                        "-c",
                        url,
                        "-O",
                        run_filename,
                    ])
                    .current_dir(&runner_dir);
                inject_ssl_env(&mut wget).await;
                Self::execute_command_with_output(
                    wget,
                    cancel_rx.clone(),
                    Some((&runner_dir, "Downloading")),
                    &mut on_output,
                )
                .await?;

                if is_run {
                    on_output(format!(
                        "[BenchHub] [2/2] Extracting installer archive ({})...\n",
                        run_filename
                    ));

                    let mut chmod = Command::new("chmod");
                    chmod.args(["+x", run_filename]).current_dir(&runner_dir);
                    Self::execute_command_with_output(
                        chmod,
                        cancel_rx.clone(),
                        None,
                        &mut on_output,
                    )
                    .await?;

                    let mut sh_run = Command::new("sh");
                    if let Ok(cur_path) = std::env::var("PATH") {
                        if !cur_path.starts_with("/bin:") && !cur_path.starts_with("/usr/bin:") {
                            sh_run.env("PATH", format!("/bin:/usr/bin:{}", cur_path));
                        }
                    }
                    sh_run
                        .env("LC_ALL", "C")
                        .env("TAR_OPTIONS", "--no-same-owner --no-same-permissions")
                        .arg(format!("../{}", run_filename))
                        .args(["--tar", "-xf"])
                        .current_dir(&extracted_dir);
                    Self::execute_command_with_output(
                        sh_run,
                        cancel_rx.clone(),
                        Some((&extracted_dir, "Extracting archive")),
                        &mut on_output,
                    )
                    .await?;
                }
            } else {
                anyhow::bail!(
                    "Installation command or download link not defined: {}",
                    effective_profile.name
                );
            }

            // Set executable permissions on target binaries
            use std::os::unix::fs::PermissionsExt;
            if let Some(ref rel_bin) = effective_profile.binary_relative_path {
                let bin_path = runner_dir.join(rel_bin);
                if let Ok(meta) = tokio::fs::metadata(&bin_path).await {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = tokio::fs::set_permissions(&bin_path, perms).await;
                }
            }

            if is_run {
                let extracted_dir = runner_dir.join("extracted");
                let _ = set_permissions_recursive(&extracted_dir, 0o755).await;
                if let Ok(mut entries) = tokio::fs::read_dir(&runner_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if let Ok(meta) = entry.metadata().await {
                            if meta.is_file() {
                                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                if name.ends_with(".run") || name.ends_with(".run.download") {
                                    let _ = tokio::fs::remove_file(&path).await;
                                }
                            }
                        }
                    }
                }
            }

            on_output("[BenchHub] Finalizing permissions and verifying runner...\n".to_string());
            let _ = ensure_executable_permissions(&runner_dir).await;
            let adapter = adapters::adapter_for(&effective_profile.id, &effective_profile.name);
            let _ = adapter.prepare_on_install(&runner_dir).await;
            let _ = tokio::fs::write(runner_dir.join(".successfully-installed"), "").await;
            on_output(format!(
                "[BenchHub] Installation successful: {}\n",
                effective_profile.name
            ));
            Ok(())
        }
        .await;

        if res.is_err() {
            if let Ok(safe_dir) = ensure_path_within(&self.get_runners_dir(), &runner_dir) {
                let _ = tokio::fs::remove_dir_all(&safe_dir).await;
            }
            if let Some(backup_path) = backup_dir {
                let _ = tokio::fs::rename(&backup_path, &runner_dir).await;
            }
        } else if let Some(backup_path) = backup_dir {
            let _ = tokio::fs::remove_dir_all(&backup_path).await;
        }

        res
    }

    #[tracing::instrument(skip_all)]
    pub async fn run<F>(
        &self,
        profile: &BenchmarkProfile,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        log_path: Option<PathBuf>,
        on_output: F,
    ) -> Result<BenchmarkRunOutput>
    where
        F: FnMut(String) + Send + 'static,
    {
        self.run_version(profile, "", cancel_rx, log_path, GpuMode::Auto, on_output)
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn run_version<F>(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        log_path: Option<PathBuf>,
        gpu_mode: GpuMode,
        on_output: F,
    ) -> Result<BenchmarkRunOutput>
    where
        F: FnMut(String) + Send + 'static,
    {
        self.run_version_custom(
            profile, version, None, None, cancel_rx, log_path, gpu_mode, on_output,
        )
        .await
    }

    fn extract_score_from_line(regex: &Option<Regex>, line: &str) -> Option<f64> {
        if let Some(re) = regex {
            let stripped = strip_ansi_codes(line);
            if let Some(caps) = re.captures(&stripped).or_else(|| re.captures(line)) {
                for i in 1..caps.len() {
                    if let Some(s) = caps.get(i) {
                        if let Ok(v) = s.as_str().parse::<f64>() {
                            if v.is_finite() {
                                return Some(v);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    pub async fn run_version_custom<F>(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
        custom_args: Option<&[String]>,
        timeout_secs: Option<u64>,
        mut cancel_rx: tokio::sync::watch::Receiver<bool>,
        log_path: Option<PathBuf>,
        gpu_mode: GpuMode,
        mut on_output: F,
    ) -> Result<BenchmarkRunOutput>
    where
        F: FnMut(String) + Send + 'static,
    {
        let mut effective_profile = profile.for_version(version);
        if let Some(args) = custom_args {
            effective_profile.run_args = args.to_vec();
        }
        let adapter =
            adapters::adapter_for_run(&effective_profile.id, &effective_profile.name, version);
        adapter.adjust_run_args(&mut effective_profile.run_args);
        let runner_dir = self
            .get_active_runner_dir_async(&profile.id, version)
            .await?;
        let _ = ensure_executable_permissions(&runner_dir).await;

        let mut log_file = if let Some(path) = log_path {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!("Failed to create log parent directory {:?}", parent)
                })?;
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("Failed to open log file {:?}", path))?;
            Some(std::io::BufWriter::new(file))
        } else {
            None
        };

        let _ = adapter.prepare_runner_dir(&runner_dir).await;

        // Resolve executable using priority path and dynamic recursive fallback search
        let resolved = match Self::resolve_executable(&runner_dir, &effective_profile) {
            Ok(res) => res,
            Err(diag) => {
                let err_msg = format!("[BenchHub] ERROR: {}\n", diag);
                log_error(&mut log_file, &err_msg, &mut on_output);
                anyhow::bail!("Executable not found: {}", diag);
            }
        };

        if resolved.work_dir != runner_dir {
            let _ = ensure_executable_permissions(&resolved.work_dir).await;
            let _ = ensure_gthread_shim(&resolved.work_dir).await;
            let _ = adapter.prepare_runner_dir(&resolved.work_dir).await;
        }
        let _ = ensure_gthread_shim(&runner_dir).await;

        let header = format!(
            "[BenchHub] Running: {} {:?} (Directory: {})\n",
            resolved.executable,
            effective_profile.run_args,
            resolved.work_dir.display()
        );
        on_output(header.clone());

        if let Some(f) = &mut log_file {
            use std::io::Write;
            let _ = f.write_all(header.as_bytes());
            let _ = f.flush();
        }

        if *cancel_rx.borrow() {
            let stop_msg = "\n[BenchHub] ⏹ Benchmark stopped by user.\n".to_string();
            if let Some(f) = &mut log_file {
                use std::io::Write;
                let _ = f.write_all(stop_msg.as_bytes());
                let _ = f.flush();
            }
            on_output(stop_msg);
            return Ok(BenchmarkRunOutput {
                score: None,
                status: "Stopped".to_string(),
                exit_code: None,
            });
        }

        let mut command = Command::new(&resolved.executable);
        command
            .args(&effective_profile.run_args)
            .current_dir(&resolved.work_dir);
        command.process_group(0); // Launch in a distinct process group for clean group termination

        platform_env::inject_runner_env(&mut command, &resolved.work_dir, gpu_mode).await;
        adapter.pre_run_cleanup(&effective_profile, version).await;
        let run_session = match adapter
            .prepare_run_session(&mut command, &effective_profile, version)
            .await
        {
            Ok(session) => session,
            Err(e) => {
                let warn_msg = format!(
                    "[BenchHub] Warning: Failed to initialize benchmark helper session: {}\n",
                    e
                );
                log_error(&mut log_file, &warn_msg, &mut on_output);
                None
            }
        };

        let mut child = match command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!(
                    "[BenchHub] ERROR: Failed to start benchmark: {} (Directory: {})\nDetail: {}\n",
                    resolved.executable,
                    resolved.work_dir.display(),
                    e
                );
                log_error(&mut log_file, &err_msg, &mut on_output);
                return Err(e).context(format!(
                    "Failed to start benchmark: {}",
                    resolved.executable
                ));
            }
        };

        let child_pid = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stderr"))?;

        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<String>(1024);
        let stdout_task = spawn_terminal_stream_reader(stdout, out_tx.clone());
        let stderr_task = spawn_terminal_stream_reader(stderr, out_tx);

        let mut score: Option<f64> = None;
        let regex = effective_profile
            .score_regex
            .as_ref()
            .and_then(|r| Regex::new(r).ok());
        let mut captured_tail = String::with_capacity(2048);

        let mut stopped_by_user = false;
        let mut timed_out = false;
        let mut exit_status = None;
        let mut trailing_cr = false;

        let timeout_fut = async {
            if let Some(secs) = timeout_secs {
                if secs > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                    return true;
                }
            }
            std::future::pending::<bool>().await
        };
        tokio::pin!(timeout_fut);

        if *cancel_rx.borrow() {
            stopped_by_user = true;
            if let Some(pid) = child_pid {
                kill_process_group(pid);
            }
            let _ = child.kill().await;
            let stop_msg = "\n[BenchHub] ⏹ Benchmark stopped by user.\n".to_string();
            if let Some(f) = &mut log_file {
                use std::io::Write;
                let _ = f.write_all(stop_msg.as_bytes());
                let _ = f.flush();
            }
            on_output(stop_msg);
        } else {
            loop {
                tokio::select! {
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            stopped_by_user = true;
                            // Kill entire process group
                            if let Some(pid) = child_pid {
                                kill_process_group(pid);
                            }
                            let _ = child.kill().await;
                            let stop_msg = if trailing_cr {
                                "\n\n[BenchHub] ⏹ Benchmark stopped by user.\n".to_string()
                            } else {
                                "\n[BenchHub] ⏹ Benchmark stopped by user.\n".to_string()
                            };
                            if let Some(f) = &mut log_file {
                                use std::io::Write;
                                let _ = f.write_all(stop_msg.as_bytes());
                                let _ = f.flush();
                            }
                            on_output(stop_msg);
                            break;
                        }
                    }
                    _ = &mut timeout_fut => {
                        timed_out = true;
                        if let Some(pid) = child_pid {
                            kill_process_group(pid);
                        }
                        let _ = child.kill().await;
                        let timeout_msg = format!(
                            "\n[BenchHub] ⏱ Timeout: Benchmark exceeded maximum duration ({} s) and was terminated.\n",
                            timeout_secs.unwrap_or(0)
                        );
                        if let Some(f) = &mut log_file {
                            use std::io::Write;
                            let _ = f.write_all(timeout_msg.as_bytes());
                            let _ = f.flush();
                        }
                        on_output(timeout_msg);
                        break;
                    }
                    status_res = child.wait(), if exit_status.is_none() => {
                        exit_status = status_res.ok();
                        if let Some(pid) = child_pid {
                            kill_process_group(pid);
                        }
                    }
                    maybe_chunk = out_rx.recv() => {
                        match maybe_chunk {
                            Some(chunk) => {
                                record_run_output_chunk(
                                    chunk,
                                    &regex,
                                    &mut score,
                                    &mut trailing_cr,
                                    &mut log_file,
                                    &mut captured_tail,
                                    &mut on_output,
                                );
                            }
                            None => {
                                // All stdout and stderr closed
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(150)), if exit_status.is_some() => {
                        break;
                    }
                }
            }
        }

        join_and_drain_stream_output(stdout_task, stderr_task, &mut out_rx, |chunk| {
            record_run_output_chunk(
                chunk,
                &regex,
                &mut score,
                &mut trailing_cr,
                &mut log_file,
                &mut captured_tail,
                &mut on_output,
            );
        })
        .await;

        if trailing_cr {
            on_output("\n".to_string());
        }

        if let Some(f) = &mut log_file {
            use std::io::Write;
            let _ = f.flush();
        }

        // Finalize any run-scoped helper session (e.g., loopback upload relay) before fallbacks.
        if let Some(session) = run_session {
            let session_result = session.finish(&regex).await;
            if !stopped_by_user && score.is_none() {
                for chunk in session_result.output_chunks {
                    log_error(&mut log_file, &chunk, &mut on_output);
                }
                if let Some(s) = session_result.score {
                    score = Some(s);
                }
            }
        }

        // If score was not captured directly from stdout/stderr, inspect adapter fallback result files or tail
        if score.is_none() {
            let fallback = adapter
                .collect_fallback_output_with_context(
                    &runner_dir,
                    &captured_tail,
                    &effective_profile,
                    version,
                    &regex,
                )
                .await;
            for chunk in fallback.output_chunks {
                log_error(&mut log_file, &chunk, &mut on_output);
            }
            if let Some(s) = fallback.score {
                score = Some(s);
            }
        }

        let exit_code = if stopped_by_user {
            None
        } else if let Some(st) = exit_status {
            st.code()
        } else {
            match child.wait().await {
                Ok(status) => {
                    if let Some(pid) = child_pid {
                        kill_process_group(pid);
                    }
                    status.code()
                }
                Err(_) => None,
            }
        };

        let finish_msg = if stopped_by_user {
            "[BenchHub] Benchmark process stopped.\n".to_string()
        } else if timed_out {
            format!(
                "[BenchHub] ⏱ Benchmark terminated due to timeout (Score: {:?})\n",
                score
            )
        } else {
            format!(
                "[BenchHub] Benchmark process finished (Exit code: {:?}, Score: {:?})\n",
                exit_code, score
            )
        };
        if let Some(f) = &mut log_file {
            use std::io::Write;
            let _ = f.write_all(finish_msg.as_bytes());
            let _ = f.flush();
        }
        on_output(finish_msg);

        let status_desc = if stopped_by_user {
            "Stopped".to_string()
        } else if timed_out {
            if score.is_some() {
                "Timeout (Scored)".to_string()
            } else {
                "Timeout".to_string()
            }
        } else if exit_code == Some(0) && score.is_some() {
            "Success".to_string()
        } else if exit_code == Some(0) {
            "Completed".to_string()
        } else if score.is_some() {
            "Failed (Scored)".to_string()
        } else {
            "Failed".to_string()
        };

        Ok(BenchmarkRunOutput {
            score,
            status: status_desc,
            exit_code,
        })
    }

    pub fn extract_score(regex_str: &str, output: &str) -> Option<f64> {
        let re = Regex::new(regex_str).ok()?;
        let mut score = None;
        for line in output.lines() {
            let stripped = strip_ansi_codes(line);
            if let Some(caps) = re.captures(&stripped).or_else(|| re.captures(line)) {
                for i in 1..caps.len() {
                    if let Some(m) = caps.get(i) {
                        if let Ok(val) = m.as_str().parse::<f64>() {
                            if val.is_finite() {
                                score = Some(val);
                                break;
                            }
                        }
                    }
                }
            }
        }
        score
    }

    /// Resolves executable path and its CWD using priority checks and recursive auto-discovery
    pub fn resolve_executable(
        runner_dir: &Path,
        profile: &BenchmarkProfile,
    ) -> Result<ResolvedExecutable, String> {
        // 1. Direct candidate checks
        let mut direct_candidates: Vec<PathBuf> = Vec::new();
        if let Some(ref rel) = profile.binary_relative_path {
            direct_candidates.push(runner_dir.join(rel));
            let name = Path::new(rel).file_name().unwrap_or_default();
            direct_candidates.push(runner_dir.join("extracted").join(name));
            direct_candidates.push(runner_dir.join(name));
        }
        if !profile.run_cmd.is_empty() {
            let clean = profile.run_cmd.trim_start_matches("./");
            direct_candidates.push(runner_dir.join(clean));
            let name = Path::new(clean).file_name().unwrap_or_default();
            direct_candidates.push(runner_dir.join("extracted").join(name));
            direct_candidates.push(runner_dir.join(name));
        }

        for candidate in direct_candidates {
            let file_name = candidate.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_archive_or_installer(file_name) {
                continue;
            }
            if candidate.is_file() {
                if let Ok(safe_candidate) = ensure_path_within(runner_dir, &candidate) {
                    if is_dynamically_broken(&safe_candidate) {
                        continue;
                    }
                    let work_dir = safe_candidate.parent().unwrap_or(runner_dir).to_path_buf();
                    let file_name = safe_candidate
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    return Ok(ResolvedExecutable {
                        work_dir,
                        executable: format!("./{}", file_name),
                        full_path: safe_candidate,
                    });
                }
            }
        }

        // 2. Check if run_cmd is a system command in PATH (e.g. "sleep", "sh", "fio", "7zz")
        if !profile.run_cmd.is_empty() {
            let clean = profile.run_cmd.trim_start_matches("./");
            if !clean.contains('/') && !is_archive_or_installer(clean) {
                let adapter = adapters::adapter_for(&profile.id, &profile.name);
                let path_candidates = adapter.path_candidates(profile, clean);
                if let Ok(path_var) = std::env::var("PATH") {
                    for path_entry in std::env::split_paths(&path_var) {
                        for bin_name in &path_candidates {
                            let sys_bin = path_entry.join(bin_name);
                            if sys_bin.is_file() && !is_dynamically_broken(&sys_bin) {
                                return Ok(ResolvedExecutable {
                                    work_dir: runner_dir.to_path_buf(),
                                    executable: bin_name.clone(),
                                    full_path: sys_bin,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3. Recursive fallback search (strictly excluding archives/installers)
        let mut search_terms: Vec<String> = Vec::new();
        if let Some(ref rel) = profile.binary_relative_path {
            if let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) {
                search_terms.push(name.to_lowercase());
            }
        }
        if !profile.run_cmd.is_empty() {
            let clean = profile.run_cmd.trim_start_matches("./");
            if let Some(name) = Path::new(clean).file_name().and_then(|n| n.to_str()) {
                search_terms.push(name.to_lowercase());
            }
        }
        search_terms.push(profile.id.to_lowercase());
        for part in profile.id.split('_') {
            if part.len() > 3 {
                search_terms.push(part.to_lowercase());
            }
        }
        for word in profile.name.split_whitespace() {
            if word.len() > 3 {
                search_terms.push(word.to_lowercase());
            }
        }

        let mut found_candidates: Vec<(i32, usize, PathBuf)> = Vec::new(); // (score, depth, path)

        fn scan_dir(
            _base: &Path,
            current: &Path,
            depth: usize,
            search_terms: &[String],
            candidates: &mut Vec<(i32, usize, PathBuf)>,
        ) {
            if depth > 6 {
                return;
            }
            if let Ok(entries) = std::fs::read_dir(current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        scan_dir(_base, &path, depth + 1, search_terms, candidates);
                    } else if path.is_file() {
                        let file_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        // Strictly skip installer archives and setup scripts
                        if is_archive_or_installer(&file_name) {
                            continue;
                        }

                        let is_executable_format =
                            file_name.ends_with(".sh") || is_elf_or_script(&path);

                        if is_executable_format {
                            let mut best_score = 0;
                            for term in search_terms {
                                if file_name == *term || file_name == format!("{}.sh", term) {
                                    best_score = best_score.max(100);
                                } else if file_name.starts_with(term) || file_name.ends_with(term) {
                                    best_score = best_score.max(80);
                                } else if file_name.contains(term) {
                                    best_score = best_score.max(60);
                                }
                            }

                            // Prioritize files inside extracted/ over root runner folder
                            if path.to_string_lossy().contains("/extracted/")
                                || path.to_string_lossy().contains("/extracted")
                            {
                                best_score += 20;
                            }

                            // Prefer main/root launcher over deeply nested bin subfolder if launcher exists
                            let is_sub_bin = path.to_string_lossy().contains("/bin/")
                                || path.to_string_lossy().contains("/x64/");
                            if is_sub_bin && best_score > 0 {
                                best_score -= 15;
                            }

                            if best_score > 0 && !is_dynamically_broken(&path) {
                                candidates.push((best_score, depth, path));
                            }
                        }
                    }
                }
            }
        }

        scan_dir(
            runner_dir,
            runner_dir,
            0,
            &search_terms,
            &mut found_candidates,
        );

        // Sort by score DESC, depth ASC
        found_candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        if let Some((_, _, best_path)) = found_candidates.first() {
            let work_dir = best_path.parent().unwrap_or(runner_dir).to_path_buf();
            let file_name = best_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("run");
            return Ok(ResolvedExecutable {
                work_dir,
                executable: format!("./{}", file_name),
                full_path: best_path.clone(),
            });
        }

        // 3. Diagnostics listing if not found
        let contents = list_dir_contents_recursive(runner_dir, 30);
        let contents_formatted = if contents.is_empty() {
            "  (Directory is completely empty)".to_string()
        } else {
            contents
                .iter()
                .map(|s| format!("  - {}", s))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Err(format!(
            "Executable not found!\nProfile searched: {} (id: {})\nRunner directory: {}\nCurrent Directory Contents:\n{}",
            profile.name,
            profile.id,
            runner_dir.display(),
            contents_formatted
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedExecutable {
    pub work_dir: PathBuf,
    pub executable: String,
    pub full_path: PathBuf,
}

pub use crate::utils::strip_ansi_codes;

fn log_error(
    log_file: &mut Option<std::io::BufWriter<std::fs::File>>,
    err_msg: &str,
    on_output: &mut impl FnMut(String),
) {
    if let Some(f) = log_file {
        use std::io::Write;
        let _ = f.write_all(err_msg.as_bytes());
        let _ = f.flush();
    }
    on_output(err_msg.to_string());
}

pub fn validate_download_command(cmd: &str) -> Result<()> {
    BenchmarkEngine::validate_download_command(cmd)
}
