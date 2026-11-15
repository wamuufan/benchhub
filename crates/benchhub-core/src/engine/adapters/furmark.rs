//! FurMark-specific execution, stress duration, and FPS fallback adapter.

use super::{BenchmarkAdapter, BoxFuture, FallbackResult};
use crate::models::BenchmarkProfile;
use crate::utils::strip_ansi_codes;
use regex::Regex;
use std::path::Path;
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
