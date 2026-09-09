use crate::models::{BenchmarkProfile, GpuMode};
use crate::utils::{ensure_path_within, sanitize_identifier};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct BenchmarkRunOutput {
    pub score: Option<f64>,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Clone)]
pub struct BenchmarkEngine {
    base_dir: PathBuf,
}

impl BenchmarkEngine {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn get_runners_dir(&self) -> PathBuf {
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

    pub fn get_active_runner_dir(&self, id: &str, version: &str) -> Result<PathBuf> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| {
                handle.block_on(async { self.get_active_runner_dir_async(id, version).await })
            })
        } else {
            anyhow::bail!(
                "get_active_runner_dir must be called from within a Tokio runtime context."
            );
        }
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

    pub fn get_logs_dir(&self) -> PathBuf {
        self.base_dir.join("logs")
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

    pub fn is_version_installed(&self, id: &str, version: &str) -> bool {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| {
                handle.block_on(async { self.is_version_installed_async(id, version).await })
            })
        } else {
            false
        }
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

    #[tracing::instrument(skip_all)]
    pub async fn install<F>(&self, profile: &BenchmarkProfile, on_output: F) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        self.install_version(profile, "", on_output).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn install_benchmark<F>(&self, profile: &BenchmarkProfile, on_output: F) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        self.install(profile, on_output).await
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
        mut on_output: F,
    ) -> Result<()>
    where
        F: FnMut(String) + Send,
    {
        command.process_group(0);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .context("Installation command failed to start")?;

        let stdout = child.stdout.take().context("Failed to open stdout")?;
        let stderr = child.stderr.take().context("Failed to open stderr")?;

        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<String>(1024);

        let out_tx_stdout = out_tx.clone();
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if out_tx_stdout.send(line).await.is_err() {
                    break;
                }
            }
        });

        let out_tx_stderr = out_tx.clone();
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if out_tx_stderr.send(line).await.is_err() {
                    break;
                }
            }
        });

        drop(out_tx);

        let mut exit_status = None;

        loop {
            tokio::select! {
                status_res = child.wait(), if exit_status.is_none() => {
                    exit_status = status_res.ok();
                }
                maybe_line = out_rx.recv() => {
                    match maybe_line {
                        Some(l) => {
                            on_output(format!("{}\n", l));
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
        }

        let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
            let _ = tokio::join!(stdout_task, stderr_task);
        })
        .await;

        let status = match exit_status {
            Some(st) => st,
            None => child.wait().await?,
        };

        if !status.success() {
            anyhow::bail!("Installation failed (exit code: {})", status);
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn install_version<F>(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
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

        let res: Result<()> = async {
            let is_run;

            if let Some(cmd_str) = &effective_profile.download_cmd {
                Self::validate_download_command(cmd_str)?;
                is_run = false;
                on_output(format!(
                    "[BenchHub] {}: {}\n",
                    crate::i18n::t("status_installing_log"),
                    effective_profile.name
                ));
                let mut command = Command::new("sh");
                command.arg("-c").arg(cmd_str).current_dir(&runner_dir);
                inject_ssl_env(&mut command, &self.base_dir).await;
                Self::execute_command_with_output(command, &mut on_output).await?;
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
                    "[BenchHub] {}: {}\n",
                    crate::i18n::t("status_installing_log"),
                    effective_profile.name
                ));

                let mut wget = Command::new("wget");
                wget.args(["-nv", "-c", url, "-O", run_filename])
                    .current_dir(&runner_dir);
                inject_ssl_env(&mut wget, &self.base_dir).await;
                Self::execute_command_with_output(wget, &mut on_output).await?;

                if is_run {
                    let mut chmod = Command::new("chmod");
                    chmod.args(["+x", run_filename]).current_dir(&runner_dir);
                    Self::execute_command_with_output(chmod, &mut on_output).await?;

                    let mut sh_run = Command::new("sh");
                    sh_run
                        .arg(format!("./{}", run_filename))
                        .args([
                            "--accept",
                            "--noexec",
                            "--target",
                            &extracted_dir.to_string_lossy(),
                        ])
                        .current_dir(&runner_dir);
                    Self::execute_command_with_output(sh_run, &mut on_output).await?;
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

            let _ = ensure_executable_permissions(&runner_dir).await;
            let _ = tokio::fs::write(runner_dir.join(".successfully-installed"), "").await;
            on_output(format!(
                "[BenchHub] {}: {}\n",
                crate::i18n::t("status_install_success_log"),
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
    pub async fn run_benchmark<F>(
        &self,
        profile: &BenchmarkProfile,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        log_path: Option<PathBuf>,
        on_output: F,
    ) -> Result<BenchmarkRunOutput>
    where
        F: FnMut(String) + Send + 'static,
    {
        self.run(profile, cancel_rx, log_path, on_output).await
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
                            return Some(v);
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

        // Resolve executable using priority path and dynamic recursive fallback search
        let resolved = match Self::resolve_executable(&runner_dir, &effective_profile) {
            Ok(res) => res,
            Err(diag) => {
                let err_msg = format!(
                    "[BenchHub] {}: {}\n",
                    crate::i18n::t("log_error_prefix"),
                    diag
                );
                log_error(&mut log_file, &err_msg, &mut on_output);
                anyhow::bail!("Executable not found: {}", diag);
            }
        };

        let _ = ensure_executable_permissions(&resolved.work_dir).await;
        let _ = ensure_gthread_shim(&resolved.work_dir).await;
        let _ = ensure_gthread_shim(&runner_dir).await;

        let is_en = crate::i18n::get_language() == "en";
        let header = if is_en {
            format!(
                "[BenchHub] Running: {} {:?} (Directory: {})\n",
                resolved.executable,
                effective_profile.run_args,
                resolved.work_dir.display()
            )
        } else {
            format!(
                "[BenchHub] Çalıştırılıyor: {} {:?} (Dizin: {})\n",
                resolved.executable,
                effective_profile.run_args,
                resolved.work_dir.display()
            )
        };
        on_output(header.clone());

        if let Some(f) = &mut log_file {
            use std::io::Write;
            let _ = f.write_all(header.as_bytes());
            let _ = f.flush();
        }

        let mut command = Command::new(&resolved.executable);
        command
            .args(&effective_profile.run_args)
            .current_dir(&resolved.work_dir);
        command.process_group(0); // Launch in a distinct process group for clean group termination

        // Configure library search paths for bundled dynamic libraries (Qt, engine, openal, sensors)
        let work_dir_str = resolved.work_dir.display().to_string();
        let extra_ld = format!(
            "{0}/bin/qt/lib:{0}/qt/lib:{0}/bin:{0}/lib:{0}/lib64:{0}/bin/x64:{0}/lib/x64:{0}:./bin/qt/lib:./qt/lib:./bin:./lib:./lib64:./bin/x64:./lib/x64:.",
            work_dir_str
        );
        let ld_lib_path = match std::env::var("LD_LIBRARY_PATH") {
            Ok(existing) if !existing.is_empty() => format!("{}:{}", extra_ld, existing),
            _ => extra_ld,
        };
        command.env("LD_LIBRARY_PATH", ld_lib_path);

        inject_ssl_env(&mut command, &self.base_dir).await;
        inject_graphics_env(&mut command);
        inject_gpu_env(&mut command, gpu_mode);

        let is_superposition = profile.id.contains("superposition")
            || profile.name.to_lowercase().contains("superposition")
            || version.to_lowercase().contains("superposition")
            || (profile.id.contains("unigine") && version.is_empty());
        if is_superposition {
            if let Ok(home) = std::env::var("HOME") {
                let auto_dir = Path::new(&home).join(".Superposition").join("automation");
                if auto_dir.exists() {
                    let _ = tokio::fs::remove_dir_all(&auto_dir).await;
                }
            }
        }

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

        let out_tx_stdout = out_tx.clone();
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if out_tx_stdout.send(line).await.is_err() {
                    break;
                }
            }
        });

        let out_tx_stderr = out_tx.clone();
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if out_tx_stderr.send(line).await.is_err() {
                    break;
                }
            }
        });

        drop(out_tx);

        let mut score: Option<f64> = None;
        let regex = effective_profile
            .score_regex
            .as_ref()
            .and_then(|r| Regex::new(r).ok());

        let mut stopped_by_user = false;
        let mut timed_out = false;
        let mut exit_status = None;

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
                        let stop_msg = "\n[BenchHub] ⏹ Benchmark stopped by user.\n".to_string();
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
                }
                maybe_line = out_rx.recv() => {
                    match maybe_line {
                        Some(l) => {
                            if let Some(s) = Self::extract_score_from_line(&regex, &l) {
                                score = Some(s);
                            }
                            let line_str = format!("{}\n", l);
                            if let Some(f) = &mut log_file {
                                use std::io::Write;
                                let _ = f.write_all(line_str.as_bytes());
                            }
                            on_output(line_str);
                        }
                        None => {
                            // All stdout and stderr closed
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(400)), if exit_status.is_some() => {
                    // Process exited; drain any remaining buffered lines
                    while let Ok(l) = out_rx.try_recv() {
                        if let Some(s) = Self::extract_score_from_line(&regex, &l) {
                            score = Some(s);
                        }
                        let line_str = format!("{}\n", l);
                        if let Some(f) = &mut log_file {
                            use std::io::Write;
                            let _ = f.write_all(line_str.as_bytes());
                        }
                        on_output(line_str);
                    }
                    break;
                }
            }
        }

        let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
            let _ = tokio::join!(stdout_task, stderr_task);
        })
        .await;

        if let Some(f) = &mut log_file {
            use std::io::Write;
            let _ = f.flush();
        }

        // If score was not captured directly from stdout/stderr, inspect Unigine automation result files
        let is_superposition = profile.id.contains("superposition")
            || profile.name.to_lowercase().contains("superposition")
            || version.to_lowercase().contains("superposition")
            || (profile.id.contains("unigine") && version.is_empty());

        if score.is_none() && is_superposition {
            if let Ok(home) = std::env::var("HOME") {
                let auto_dir = Path::new(&home).join(".Superposition").join("automation");
                if let Ok(mut entries) = tokio::fs::read_dir(&auto_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if let Ok(meta) = entry.metadata().await {
                            if meta.is_file() {
                                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                    let summary_header =
                                    "\n[BenchHub] === Superposition Benchmark Result Summary ===\n";
                                    on_output(summary_header.to_string());
                                    if let Some(f) = &mut log_file {
                                        use std::io::Write;
                                        let _ = f.write_all(summary_header.as_bytes());
                                        let _ = f.write_all(content.as_bytes());
                                        let _ = f.flush();
                                    }
                                    on_output(content.clone());

                                    if score.is_none() {
                                        for line in content.lines() {
                                            if let Some(s) =
                                                Self::extract_score_from_line(&regex, line)
                                            {
                                                score = Some(s);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let exit_code = if stopped_by_user {
            None
        } else if let Some(st) = exit_status {
            st.code()
        } else {
            match child.wait().await {
                Ok(status) => status.code(),
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
            "Scored".to_string()
        } else {
            "Completed (Log Saved)".to_string()
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
                            score = Some(val);
                            break;
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

        // 2. Check if run_cmd is a system command in PATH (e.g. "sleep", "sh")
        if !profile.run_cmd.is_empty() {
            let clean = profile.run_cmd.trim_start_matches("./");
            if !clean.contains('/') && !is_archive_or_installer(clean) {
                if let Ok(path_var) = std::env::var("PATH") {
                    for path_entry in std::env::split_paths(&path_var) {
                        let sys_bin = path_entry.join(clean);
                        if sys_bin.is_file() {
                            return Ok(ResolvedExecutable {
                                work_dir: runner_dir.to_path_buf(),
                                executable: clean.to_string(),
                                full_path: sys_bin,
                            });
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

                            if best_score > 0 {
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

pub fn is_archive_or_installer(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower.ends_with(".run")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".xz")
        || lower.ends_with(".zip")
        || lower.ends_with(".deb")
        || lower.ends_with(".rpm")
        || lower.ends_with(".iso")
        || lower.ends_with(".7z")
        || lower.ends_with(".bz2")
        || lower.ends_with(".zst")
        || lower.contains("installer")
        || lower.contains("setup")
        || lower.contains("makeself")
        || lower.contains("extract")
}

pub fn is_elf_or_script(path: &Path) -> bool {
    use std::io::Read;
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut header = [0u8; 4];
        if file.read_exact(&mut header).is_ok() && (&header == b"\x7fELF" || &header[..2] == b"#!")
        {
            return true;
        }
    }
    false
}

pub fn list_dir_contents_recursive(dir: &Path, max_entries: usize) -> Vec<String> {
    let mut results = Vec::new();
    fn walk(base: &Path, current: &Path, results: &mut Vec<String>, max_entries: usize) {
        if results.len() >= max_entries {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                if path.is_dir() {
                    results.push(format!("{}/ (dizin)", rel));
                    walk(base, &path, results, max_entries);
                } else if let Ok(meta) = path.metadata() {
                    results.push(format!("{} ({} bayt)", rel, meta.len()));
                }
            }
        }
    }
    walk(dir, dir, &mut results, max_entries);
    results
}

pub fn find_system_ca_bundle() -> Option<PathBuf> {
    let candidates = [
        "/etc/ssl/ca-bundle.pem",
        "/etc/ssl/certs/ca-certificates.crt",
        "/var/lib/ca-certificates/ca-bundle.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/pki/tls/cert.pem",
    ];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

pub fn find_system_ca_dir() -> Option<PathBuf> {
    let candidates = ["/etc/ssl/certs", "/etc/pki/tls/certs"];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

pub async fn ensure_ssl_shim(base_dir: &Path) -> Option<PathBuf> {
    let shim_path = base_dir.join("libssl_shim.so");
    if tokio::fs::try_exists(&shim_path).await.unwrap_or(false) {
        return Some(shim_path);
    }

    let embedded_shim = include_bytes!(concat!(env!("OUT_DIR"), "/libssl_shim.so"));
    if tokio::fs::write(&shim_path, embedded_shim).await.is_ok() {
        return Some(shim_path);
    }

    None
}

async fn inject_ssl_env(cmd: &mut Command, base_dir: &Path) {
    if let Some(shim) = ensure_ssl_shim(base_dir).await {
        cmd.env("LD_PRELOAD", &shim);
    }
    if let Some(ca) = find_system_ca_bundle() {
        cmd.env("SSL_CERT_FILE", &ca);
        cmd.env("CURL_CA_BUNDLE", &ca);
    }
    if let Some(cadir) = find_system_ca_dir() {
        cmd.env("SSL_CERT_DIR", &cadir);
    }
}

pub fn inject_graphics_env(cmd: &mut Command) {
    let vars = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_SESSION_TYPE",
    ];
    for var in &vars {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

pub fn inject_gpu_env(cmd: &mut Command, gpu_mode: GpuMode) {
    match gpu_mode {
        GpuMode::NvidiaDgpu => {
            cmd.env("__NV_PRIME_RENDER_OFFLOAD", "1");
            cmd.env("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
            cmd.env("__VK_LAYER_NV_optimus", "NVIDIA_only");
            cmd.env("DRI_PRIME", "1");
        }
        GpuMode::Integrated => {
            cmd.env("__NV_PRIME_RENDER_OFFLOAD", "0");
            cmd.env("DRI_PRIME", "0");
        }
        GpuMode::Auto => {
            // Keep default environment without overrides
        }
    }
}

pub async fn set_permissions_recursive(dir: &Path, _mode: u32) -> Result<()> {
    ensure_executable_permissions(dir).await
}

pub async fn ensure_executable_permissions(dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(());
    }
    use std::os::unix::fs::PermissionsExt;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&current_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };

            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_sh = file_name.ends_with(".sh")
                    || file_name.ends_with(".run")
                    || file_name == "heaven"
                    || file_name == "Superposition"
                    || file_name == "main";
                let is_in_bin = path.to_string_lossy().contains("/bin")
                    || path.to_string_lossy().contains("/extracted");

                let is_elf = if let Ok(mut file) = tokio::fs::File::open(&path).await {
                    let mut header = [0u8; 4];
                    file.read_exact(&mut header).await.is_ok() && &header == b"\x7fELF"
                } else {
                    false
                };

                if is_sh || is_elf || is_in_bin {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = tokio::fs::set_permissions(&path, perms).await;
                }
            }
        }
    }
    Ok(())
}

pub fn kill_process_group(pid: u32) {
    if pid > 0 && pid <= i32::MAX as u32 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

pub async fn ensure_gthread_shim(work_dir: &Path) -> Result<()> {
    let system_gthread_paths = [
        "/lib64/libgthread-2.0.so.0",
        "/usr/lib64/libgthread-2.0.so.0",
        "/usr/lib/x86_64-linux-gnu/libgthread-2.0.so.0",
        "/usr/lib/libgthread-2.0.so.0",
        "/lib/libgthread-2.0.so.0",
    ];
    let mut has_system_gthread = false;
    for p in &system_gthread_paths {
        if tokio::fs::try_exists(p).await.unwrap_or(false) {
            has_system_gthread = true;
            break;
        }
    }
    if has_system_gthread {
        return Ok(());
    }

    let glib_candidates = [
        "/lib64/libglib-2.0.so.0",
        "/usr/lib64/libglib-2.0.so.0",
        "/usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
        "/usr/lib/libglib-2.0.so.0",
        "/lib/libglib-2.0.so.0",
    ];
    let mut found_glib = None;
    for p in &glib_candidates {
        if tokio::fs::try_exists(p).await.unwrap_or(false) {
            found_glib = Some(*p);
            break;
        }
    }

    if let Some(glib_path) = found_glib {
        let bin_dir = work_dir.join("bin");
        if tokio::fs::try_exists(&bin_dir).await.unwrap_or(false) {
            let target_link = bin_dir.join("libgthread-2.0.so.0");
            if !tokio::fs::try_exists(&target_link).await.unwrap_or(false) {
                #[cfg(unix)]
                let _ = tokio::fs::symlink(glib_path, &target_link).await;
            }
        }
        let target_link_root = work_dir.join("libgthread-2.0.so.0");
        if !tokio::fs::try_exists(&target_link_root)
            .await
            .unwrap_or(false)
        {
            #[cfg(unix)]
            let _ = tokio::fs::symlink(glib_path, &target_link_root).await;
        }
    }
    Ok(())
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
