//! Per-benchmark customization adapters isolating tool-specific quirks from the core runner.
//!
//! Benchmarks with standard TOML definitions (`ffmpeg`, `gravitymark`, `llama-bench`,
//! `ycruncher`, etc.) use [`StandardAdapter`] with zero special code. Tools that need custom source
//! compilation, offline shims, runtime flags, executable aliases, upload relays, or result file
//! fallbacks register a [`BenchmarkAdapter`] implementation here.

pub mod blender;
pub mod cray;
pub mod fio;
pub mod furmark;
pub mod geekbench;
pub mod stream;
pub mod unigine;

pub use blender::ensure_blender_runner;
pub use cray::ensure_cray_runner;
pub use fio::ensure_fio_runner;
pub use stream::ensure_stream_runner;

use crate::models::BenchmarkProfile;
use anyhow::Result;
use regex::Regex;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::process::Command;

/// Boxed `Send` future returned by async [`BenchmarkAdapter`] hooks.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Output chunks and optional score extracted by an adapter after process termination.
#[derive(Debug, Default)]
pub struct FallbackResult {
    pub output_chunks: Vec<String>,
    pub score: Option<f64>,
}

/// Per-run session started before spawning the benchmark process and finalized after exit.
pub trait AdapterRunSession: Send {
    /// Stops run-scoped helper services and returns captured output chunks and/or score.
    fn finish<'a>(self: Box<Self>, score_regex: &'a Option<Regex>)
        -> BoxFuture<'a, FallbackResult>;
}

/// Extension points allowing individual benchmark families to adjust install, runner setup,
/// execution arguments, executable aliases, and score fallback without polluting core execution.
pub trait BenchmarkAdapter: Sync {
    /// Applies required runtime safety flags in place (e.g. FurMark `--no-osi`).
    fn adjust_run_args(&self, _args: &mut Vec<String>) {}

    /// Applies an optional stress/duration override on top of custom or default run arguments.
    fn apply_duration_override(
        &self,
        _profile: &BenchmarkProfile,
        _version: &str,
        custom_args: Option<&[String]>,
        _duration_secs_override: Option<u64>,
    ) -> Option<Vec<String>> {
        custom_args.map(|args| args.to_vec())
    }

    /// Returns candidate executable names when resolving a command from `PATH`.
    fn path_candidates(&self, _profile: &BenchmarkProfile, clean_cmd: &str) -> Vec<String> {
        vec![clean_cmd.to_string()]
    }

    /// Runs benchmark-specific setup right after installation finishes.
    fn prepare_on_install<'a>(&'a self, _runner_dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// Prepares runner binaries, scripts, or caches inside `dir` before process launch.
    fn prepare_runner_dir<'a>(&'a self, _dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// Cleans stale external state before starting a benchmark process.
    fn pre_run_cleanup<'a>(
        &'a self,
        _profile: &'a BenchmarkProfile,
        _version: &'a str,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// Configures run-scoped environment overrides or helper services before process launch.
    fn prepare_run_session<'a>(
        &'a self,
        _command: &'a mut Command,
        _profile: &'a BenchmarkProfile,
        _version: &'a str,
    ) -> BoxFuture<'a, Result<Option<Box<dyn AdapterRunSession>>>> {
        Box::pin(async { Ok(None) })
    }

    /// Collects benchmark output/score from auxiliary files when stdout/stderr had no match.
    fn collect_fallback_output<'a>(
        &'a self,
        _profile: &'a BenchmarkProfile,
        _version: &'a str,
        _regex: &'a Option<Regex>,
    ) -> BoxFuture<'a, FallbackResult> {
        Box::pin(async { FallbackResult::default() })
    }

    /// Collects fallback output/score using the active runner directory and captured stream tail.
    fn collect_fallback_output_with_context<'a>(
        &'a self,
        _runner_dir: &'a Path,
        _captured_output: &'a str,
        profile: &'a BenchmarkProfile,
        version: &'a str,
        regex: &'a Option<Regex>,
    ) -> BoxFuture<'a, FallbackResult> {
        self.collect_fallback_output(profile, version, regex)
    }
}

/// Default no-op adapter for standard TOML-driven benchmarks.
pub struct StandardAdapter;
impl BenchmarkAdapter for StandardAdapter {}

/// Adapter handling distribution binary name aliases for 7-Zip (`7zz` / `7z`).
pub struct SevenZipAdapter;

impl BenchmarkAdapter for SevenZipAdapter {
    fn path_candidates(&self, _profile: &BenchmarkProfile, clean_cmd: &str) -> Vec<String> {
        let mut candidates = vec![clean_cmd.to_string()];
        if clean_cmd == "7zz" {
            candidates.push("7z".to_string());
        }
        candidates
    }
}

static FURMARK: furmark::FurmarkAdapter = furmark::FurmarkAdapter;
static BLENDER: blender::BlenderAdapter = blender::BlenderAdapter;
static CRAY: cray::CrayAdapter = cray::CrayAdapter;
static STREAM: stream::StreamAdapter = stream::StreamAdapter;
static FIO: fio::FioAdapter = fio::FioAdapter;
static UNIGINE: unigine::UnigineAdapter = unigine::UnigineAdapter;
static GEEKBENCH: geekbench::GeekbenchAdapter = geekbench::GeekbenchAdapter;
static SEVENZIP: SevenZipAdapter = SevenZipAdapter;
static STANDARD: StandardAdapter = StandardAdapter;

/// Selects the adapter registered for a benchmark profile ID and display name.
pub fn adapter_for(profile_id: &str, profile_name: &str) -> &'static dyn BenchmarkAdapter {
    adapter_for_run(profile_id, profile_name, "")
}

/// Selects the adapter registered for a benchmark profile and optional sub-version name.
pub fn adapter_for_run(
    profile_id: &str,
    profile_name: &str,
    version: &str,
) -> &'static dyn BenchmarkAdapter {
    let id = profile_id.to_lowercase();
    let name = profile_name.to_lowercase();
    let ver = version.to_lowercase();

    if id == "furmark" {
        &FURMARK
    } else if id.contains("blender") || name.contains("blender") {
        &BLENDER
    } else if id.contains("cray") {
        &CRAY
    } else if id.contains("stream") {
        &STREAM
    } else if id.contains("fio") {
        &FIO
    } else if id.contains("unigine")
        || id.contains("superposition")
        || name.contains("superposition")
        || ver.contains("superposition")
    {
        &UNIGINE
    } else if id.contains("geekbench") || name.contains("geekbench") {
        &GEEKBENCH
    } else if id.contains("7zip") || name.contains("7-zip") {
        &SEVENZIP
    } else {
        &STANDARD
    }
}

/// Applies required runtime safety flags for specific benchmark tools.
pub fn ensure_profile_runtime_args(profile_id: &str, args: &mut Vec<String>) {
    adapter_for(profile_id, "").adjust_run_args(args);
}

/// Computes effective argument overrides when a stress test duration is supplied.
pub fn apply_duration_override(
    profile: &BenchmarkProfile,
    version: &str,
    custom_args: Option<&[String]>,
    duration_secs_override: Option<u64>,
) -> Option<Vec<String>> {
    adapter_for_run(&profile.id, &profile.name, version).apply_duration_override(
        profile,
        version,
        custom_args,
        duration_secs_override,
    )
}

pub(super) async fn compile_c_source(runner_dir: &Path, args: &[&str]) {
    let _ = tokio::process::Command::new("gcc")
        .current_dir(runner_dir)
        .args(args)
        .status()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile(id: &str, name: &str, run_args: Vec<&str>) -> BenchmarkProfile {
        BenchmarkProfile {
            id: id.to_string(),
            name: name.to_string(),
            run_cmd: id.to_string(),
            run_args: run_args.into_iter().map(str::to_string).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn furmark_adapter_adds_no_osi_and_max_time_override() {
        let profile = sample_profile("furmark", "FurMark 2", vec!["--demo", "furmark-gl"]);
        let mut args = profile.run_args.clone();
        ensure_profile_runtime_args(&profile.id, &mut args);
        assert!(args.contains(&"--no-osi".to_string()));

        // Calling again should not duplicate --no-osi
        ensure_profile_runtime_args(&profile.id, &mut args);
        assert_eq!(args.iter().filter(|a| *a == "--no-osi").count(), 1);

        let stress = apply_duration_override(&profile, "", None, Some(300)).unwrap();
        assert_eq!(stress, vec!["--demo", "furmark-gl", "--max-time", "300"]);

        // Existing --max-time is preserved
        let custom = vec!["--max-time".to_string(), "60".to_string()];
        let kept = apply_duration_override(&profile, "", Some(&custom), Some(300)).unwrap();
        assert_eq!(kept, vec!["--max-time", "60"]);
    }

    #[test]
    fn standard_and_geekbench_adapters_leave_args_and_duration_untouched() {
        for (id, name, args_in) in [
            ("gravitymark", "GravityMark", vec!["--vk"]),
            ("geekbench", "Geekbench", vec!["--cpu"]),
        ] {
            let profile = sample_profile(id, name, args_in.clone());
            let mut args = profile.run_args.clone();
            ensure_profile_runtime_args(&profile.id, &mut args);
            assert_eq!(args, args_in);

            assert_eq!(apply_duration_override(&profile, "", None, Some(300)), None);
            let custom = vec!["--gpu".to_string()];
            assert_eq!(
                apply_duration_override(&profile, "", Some(&custom), Some(300)),
                Some(vec!["--gpu".to_string()])
            );
        }
    }

    #[test]
    fn sevenzip_adapter_adds_7z_fallback_alias() {
        let profile = sample_profile("7zip", "7-Zip", vec!["b"]);
        let adapter = adapter_for(&profile.id, &profile.name);
        assert_eq!(
            adapter.path_candidates(&profile, "7zz"),
            vec!["7zz".to_string(), "7z".to_string()]
        );
        assert_eq!(
            adapter.path_candidates(&profile, "7za"),
            vec!["7za".to_string()]
        );
    }

    #[test]
    fn unigine_superposition_detection_matches_default_or_superposition_only() {
        let unigine = sample_profile("unigine", "Unigine Benchmark", vec![]);
        assert!(unigine::is_superposition(&unigine, ""));
        assert!(unigine::is_superposition(&unigine, "superposition"));
        assert!(!unigine::is_superposition(&unigine, "heaven"));
    }

    #[test]
    fn stream_adapter_selects_array_size_by_version_dir() {
        assert_eq!(
            stream::stream_array_size_for_dir(Path::new("/tmp/runners/stream/Compact")),
            "20000000"
        );
        assert_eq!(
            stream::stream_array_size_for_dir(Path::new("/tmp/runners/stream/Extended")),
            "160000000"
        );
        assert_eq!(
            stream::stream_array_size_for_dir(Path::new("/tmp/runners/stream/Standard")),
            "80000000"
        );
    }

    #[tokio::test]
    async fn furmark_adapter_parses_max_time_avg_fps_fallback() {
        let profile = sample_profile("furmark", "FurMark", vec!["--demo", "furmark-vk"]);
        let adapter = adapter_for(&profile.id, &profile.name);
        let sample_log = "- GPU temp: 65°C\n- FPS (min/avg/max) : 64 / 68 / 71\n";
        assert_eq!(furmark::parse_furmark_avg_fps(sample_log), Some(68.0));

        let fallback = adapter
            .collect_fallback_output_with_context(
                Path::new("/tmp"),
                sample_log,
                &profile,
                "vulkan",
                &None,
            )
            .await;
        assert_eq!(fallback.score, Some(68.0));
        assert!(!fallback.output_chunks.is_empty());
    }
}
