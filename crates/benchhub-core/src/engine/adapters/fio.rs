//! FIO storage benchmark system-fallback and minimal rebuild adapter.

use super::{BenchmarkAdapter, BoxFuture, FallbackResult};
use crate::engine::platform_env::{inject_ssl_env, is_dynamically_broken};
use crate::models::BenchmarkProfile;
use crate::utils::strip_ansi_codes;
use anyhow::Result;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// Parses `fio` bandwidth (`bw=...` / `BW=...`) and normalizes `GiB/s`, `GB/s`, `KiB/s`, `KB/s`
/// into `MB/s` (`MiB/s`-equivalent) when `fio` auto-scales units away from `MiB/s`.
pub fn parse_fio_scaled_bandwidth_mb_s(output: &str) -> Option<f64> {
    static BW_RE: OnceLock<Regex> = OnceLock::new();
    let re = BW_RE.get_or_init(|| {
        Regex::new(r"(?:bw|BW)=([0-9.]+)(GiB/s|GB/s|MiB/s|MB/s|KiB/s|KB/s)")
            .expect("FIO bandwidth regex must compile")
    });

    let mut last_mb_s = None;
    for line in output.lines() {
        let stripped = strip_ansi_codes(line);
        for caps in re.captures_iter(&stripped) {
            let Some(val_m) = caps.get(1) else {
                continue;
            };
            let Some(unit_m) = caps.get(2) else {
                continue;
            };
            let Ok(val) = val_m.as_str().parse::<f64>() else {
                continue;
            };
            let scaled = match unit_m.as_str() {
                "GiB/s" => val * 1024.0,
                "GB/s" => val * 1000.0,
                "MiB/s" | "MB/s" => val,
                "KiB/s" => val / 1024.0,
                "KB/s" => val / 1000.0,
                _ => continue,
            };
            if scaled.is_finite() && scaled > 0.0 {
                last_mb_s = Some(scaled);
            }
        }
    }
    last_mb_s
}

/// Adapter that verifies `fio` dynamic links, defers to a healthy `PATH` binary (e.g. Snap/Flatpak),
/// applies `--runtime` duration overrides, normalizes scaled bandwidth units,
/// or rebuilds `fio` with minimal dependencies on Native hosts.
pub struct FioAdapter;

impl BenchmarkAdapter for FioAdapter {
    fn apply_duration_override(
        &self,
        profile: &BenchmarkProfile,
        version: &str,
        custom_args: Option<&[String]>,
        duration_secs_override: Option<u64>,
    ) -> Option<Vec<String>> {
        let Some(secs) = duration_secs_override.filter(|&d| d > 0) else {
            return custom_args.map(|args| args.to_vec());
        };

        let base = match custom_args {
            Some(args) => args.to_vec(),
            None => profile.for_version(version).run_args,
        };

        let mut args = Vec::with_capacity(base.len() + 2);
        let mut iter = base.into_iter();
        while let Some(arg) = iter.next() {
            if arg == "--runtime" {
                let _ = iter.next();
                continue;
            }
            if arg.starts_with("--runtime=") {
                continue;
            }
            args.push(arg);
        }
        if !args.iter().any(|a| a == "--time_based") {
            args.push("--time_based".to_string());
        }
        args.push(format!("--runtime={}", secs));

        Some(args)
    }

    fn prepare_runner_dir<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_fio_runner(dir))
    }

    fn collect_fallback_output_with_context<'a>(
        &'a self,
        _runner_dir: &'a Path,
        captured_output: &'a str,
        _profile: &'a BenchmarkProfile,
        _version: &'a str,
        _regex: &'a Option<Regex>,
    ) -> BoxFuture<'a, FallbackResult> {
        Box::pin(async move {
            if let Some(mb_s) = parse_fio_scaled_bandwidth_mb_s(captured_output) {
                return FallbackResult {
                    output_chunks: vec![format!(
                        "[BenchHub] Normalized FIO bandwidth: bw={:.2}MiB/s\n",
                        mb_s
                    )],
                    score: Some(mb_s),
                };
            }
            FallbackResult::default()
        })
    }
}

/// Ensures a working `fio` binary is available either in `runner_dir` or on `PATH`, compiling
/// `fio-3.38` locally with minimal shared dependencies when needed.
pub async fn ensure_fio_runner(runner_dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(runner_dir).await.unwrap_or(false) {
        return Ok(());
    }

    let bin_path = runner_dir.join("fio");
    let is_broken = if tokio::fs::try_exists(&bin_path).await.unwrap_or(false) {
        is_dynamically_broken(&bin_path)
    } else {
        true
    };

    if is_broken {
        // First check if a working system fio exists in PATH (e.g. staged in Snap or bundled in Flatpak).
        // If system fio exists and works, we DO NOT overwrite bin_path with a container binary,
        // because resolve_executable will seamlessly use the working system binary from PATH.
        let mut has_working_system = false;
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(':') {
                let candidate = Path::new(dir).join("fio");
                if candidate.is_file()
                    && candidate != bin_path
                    && !is_dynamically_broken(&candidate)
                {
                    has_working_system = true;
                    break;
                }
            }
        }

        // If no working system fio exists in PATH (e.g. on host Native when bin_path was corrupted),
        // recompile clean fio locally with minimal dependencies so it only links libc, libm, libz.
        if !has_working_system {
            tracing::info!(
                "Rebuilding fio runner with minimal dependencies in {:?}",
                runner_dir
            );
            let rebuild_cmd = "(wget --progress=bar:force:noscroll -c https://github.com/axboe/fio/archive/refs/tags/fio-3.38.tar.gz -O fio-3.38.tar.gz && tar --no-same-owner --no-same-permissions --no-xattrs --no-acls -xzf fio-3.38.tar.gz && cd fio-fio-3.38 && ./configure --disable-gfapi --disable-numa --disable-pmem --disable-rados --disable-rbd --disable-rdma > /dev/null && make -j$(nproc) fio > /dev/null && cp fio ../fio && cd .. && rm -rf fio-fio-3.38 fio-3.38.tar.gz && chmod +x fio)";
            let mut cmd = tokio::process::Command::new("sh");
            if let Ok(cur_path) = std::env::var("PATH") {
                if !cur_path.starts_with("/bin:") && !cur_path.starts_with("/usr/bin:") {
                    cmd.env("PATH", format!("/bin:/usr/bin:{}", cur_path));
                }
            }
            cmd.env("LC_ALL", "C")
                .arg("-c")
                .arg(rebuild_cmd)
                .current_dir(runner_dir);
            inject_ssl_env(&mut cmd).await;
            let _ = cmd.status().await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BenchmarkVersion;

    #[test]
    fn parse_fio_scaled_bandwidth_handles_gib_mib_kib() {
        let gib_out =
            "Run status group 0 (all jobs):\n   READ: bw=11.0GiB/s (11.8GB/s), io=22.0GiB";
        let score = parse_fio_scaled_bandwidth_mb_s(gib_out).expect("GiB/s should parse");
        assert!((score - 11.0 * 1024.0).abs() < 0.01);

        let kib_out = "   READ: bw=512KiB/s (524kB/s)";
        let score_kib = parse_fio_scaled_bandwidth_mb_s(kib_out).expect("KiB/s should parse");
        assert!((score_kib - 0.5).abs() < 0.01);
    }

    #[test]
    fn fio_adapter_replaces_runtime_on_duration_override() {
        let profile = BenchmarkProfile {
            id: "fio".to_string(),
            name: "FIO".to_string(),
            versions: vec![BenchmarkVersion {
                version: "Seq-Read".to_string(),
                run_args: Some(vec![
                    "--name=seq_read".to_string(),
                    "--time_based".to_string(),
                    "--runtime=15".to_string(),
                ]),
                ..Default::default()
            }],
            ..Default::default()
        };

        let args = FioAdapter
            .apply_duration_override(&profile, "Seq-Read", None, Some(3))
            .expect("override should produce args");
        assert!(args.contains(&"--time_based".to_string()));
        assert!(args.contains(&"--runtime=3".to_string()));
        assert!(!args.contains(&"--runtime=15".to_string()));
    }
}
