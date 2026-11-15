//! FurMark-specific execution, stress duration, and FPS fallback adapter.

use super::{BenchmarkAdapter, BoxFuture, FallbackResult};
use crate::models::BenchmarkProfile;
use crate::utils::strip_ansi_codes;
use anyhow::Result;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Parses average FPS from FurMark's `--max-time` stress summary line, e.g.
/// `- FPS (min/avg/max) : 67 / 67 / 67`.
pub fn parse_furmark_avg_fps(output: &str) -> Option<f64> {
    static FPS_RE: OnceLock<Regex> = OnceLock::new();
    let re = FPS_RE.get_or_init(|| {
        Regex::new(r"(?i)FPS\s*\(min/avg/max\)\s*:\s*[0-9.]+\s*/\s*([0-9.]+)\s*/\s*[0-9.]+")
            .expect("FurMark avg FPS regex must compile")
    });

    let mut avg_fps = None;
    for line in output.lines() {
        let stripped = strip_ansi_codes(line);
        if let Some(caps) = re.captures(&stripped).or_else(|| re.captures(line)) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f64>() {
                    avg_fps = Some(val);
                }
            }
        }
    }
    avg_fps
}

/// Adapter encapsulating FurMark runtime safety flags, `--max-time` overrides,
/// and stress-run average FPS fallback extraction.
pub struct FurmarkAdapter;

impl BenchmarkAdapter for FurmarkAdapter {
    fn prepare_on_install<'a>(&'a self, runner_dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_furmark_runner(runner_dir))
    }

    fn prepare_runner_dir<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_furmark_runner(dir))
    }

    fn adjust_run_args(&self, args: &mut Vec<String>) {
        if !args.iter().any(|a| a == "--no-osi") {
            // FurMark's ImGui OSI crashes with SIGABRT on hybrid GPUs when a secondary GPU name is empty.
            args.push("--no-osi".to_string());
        }
    }

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

        let mut args = match custom_args {
            Some(args) => args.to_vec(),
            None => profile.for_version(version).run_args,
        };

        if !args.iter().any(|a| a == "--max-time") {
            args.push("--max-time".to_string());
            args.push(secs.to_string());
        }

        Some(args)
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
            if let Some(avg_fps) = parse_furmark_avg_fps(captured_output) {
                return FallbackResult {
                    output_chunks: vec![format!(
                        "[BenchHub] FurMark stress summary (avg FPS fallback): {:.1}\n",
                        avg_fps
                    )],
                    score: Some(avg_fps),
                };
            }
            FallbackResult::default()
        })
    }
}

/// Removes any 0-byte `libGLU.so.1` left by failed extraction and populates it from
/// Flatpak (`/app/lib`), Snap, or host library paths when available.
pub async fn ensure_furmark_runner(runner_dir: &Path) -> Result<()> {
    let dylibs_dir = runner_dir.join("FurMark_linux64").join("dylibs");
    let libglu_path = dylibs_dir.join("libGLU.so.1");

    if tokio::fs::try_exists(&libglu_path).await.unwrap_or(false) {
        if let Ok(meta) = tokio::fs::metadata(&libglu_path).await {
            if meta.len() == 0 {
                tracing::warn!("Removing corrupted 0-byte {:?}", libglu_path);
                tokio::fs::remove_file(&libglu_path).await?;
            }
        }
    }

    if !tokio::fs::try_exists(&libglu_path).await.unwrap_or(false)
        && tokio::fs::try_exists(&dylibs_dir).await.unwrap_or(false)
    {
        let mut candidates = vec![
            PathBuf::from("/app/lib/libGLU.so.1"),
            PathBuf::from("/usr/lib/x86_64-linux-gnu/libGLU.so.1"),
            PathBuf::from("/usr/lib64/libGLU.so.1"),
        ];
        if let Some(snap_dir) = std::env::var_os("SNAP") {
            candidates.push(PathBuf::from(snap_dir).join("usr/lib/x86_64-linux-gnu/libGLU.so.1"));
        }

        for candidate in candidates {
            if let Ok(meta) = tokio::fs::metadata(&candidate).await {
                if meta.is_file() && meta.len() > 0 {
                    tokio::fs::copy(&candidate, &libglu_path).await?;
                    break;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_furmark_runner_removes_zero_byte_libglu() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "benchhub_furmark_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dylibs_dir = tmp_dir.join("FurMark_linux64").join("dylibs");
        tokio::fs::create_dir_all(&dylibs_dir).await.unwrap();

        let libglu_path = dylibs_dir.join("libGLU.so.1");
        tokio::fs::write(&libglu_path, b"").await.unwrap();
        assert_eq!(tokio::fs::metadata(&libglu_path).await.unwrap().len(), 0);

        ensure_furmark_runner(&tmp_dir).await.unwrap();

        if let Ok(meta) = tokio::fs::metadata(&libglu_path).await {
            assert!(
                meta.len() > 0,
                "libGLU.so.1 must not remain 0 bytes after ensure_furmark_runner"
            );
        }

        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    }
}
