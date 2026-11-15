//! Unigine Superposition automation directory cleanup and result summary adapter.

use super::{BenchmarkAdapter, BoxFuture, FallbackResult};
use crate::engine::BenchmarkEngine;
use crate::models::BenchmarkProfile;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Adapter managing Superposition's `~/.Superposition/automation` cleanup and fallback summary parsing.
pub struct UnigineAdapter;

impl BenchmarkAdapter for UnigineAdapter {
    fn pre_run_cleanup<'a>(
        &'a self,
        profile: &'a BenchmarkProfile,
        version: &'a str,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if is_superposition(profile, version) {
                if let Some(auto_dir) = automation_dir() {
                    if auto_dir.exists() {
                        let _ = tokio::fs::remove_dir_all(&auto_dir).await;
                    }
                }
            }
        })
    }

    fn collect_fallback_output<'a>(
        &'a self,
        profile: &'a BenchmarkProfile,
        version: &'a str,
        regex: &'a Option<Regex>,
    ) -> BoxFuture<'a, FallbackResult> {
        Box::pin(async move {
            if !is_superposition(profile, version) {
                return FallbackResult::default();
            }

            let Some(auto_dir) = automation_dir() else {
                return FallbackResult::default();
            };

            let mut result = FallbackResult::default();
            if let Ok(mut entries) = tokio::fs::read_dir(&auto_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    let Ok(meta) = entry.metadata().await else {
                        continue;
                    };
                    if !meta.is_file() {
                        continue;
                    }
                    let Ok(content) = tokio::fs::read_to_string(&path).await else {
                        continue;
                    };

                    result.output_chunks.push(
                        "\n[BenchHub] === Superposition Benchmark Result Summary ===\n".to_string(),
                    );
                    result.output_chunks.push(content.clone());

                    if result.score.is_none() {
                        for line in content.lines() {
                            if let Some(s) = BenchmarkEngine::extract_score_from_line(regex, line) {
                                result.score = Some(s);
                                break;
                            }
                        }
                    }
                }
            }
            result
        })
    }
}

pub(crate) fn is_superposition(profile: &BenchmarkProfile, version: &str) -> bool {
    profile.id.contains("superposition")
        || profile.name.to_lowercase().contains("superposition")
        || version.to_lowercase().contains("superposition")
        || (profile.id.contains("unigine") && version.is_empty())
}

fn automation_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join(".Superposition").join("automation"))
}
