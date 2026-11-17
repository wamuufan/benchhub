//! Blender offline runner installation and cache linking adapter.

use super::{BenchmarkAdapter, BoxFuture};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Adapter embedding BenchHub's offline `blender_runner.py` shim and linking cached scenes/builds.
pub struct BlenderAdapter;

impl BenchmarkAdapter for BlenderAdapter {
    fn prepare_on_install<'a>(&'a self, runner_dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_blender_runner(runner_dir))
    }

    fn prepare_runner_dir<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_blender_runner(dir))
    }
}

/// Replaces the stock ELF `benchmark-launcher-cli` (backing it up to `benchmark-launcher-cli-orig`)
/// with BenchHub's Python runner and symlinks pre-cached Blender versions, scenes, and scripts.
pub async fn ensure_blender_runner(runner_dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(runner_dir).await.unwrap_or(false) {
        return Ok(());
    }

    let cli_path = runner_dir.join("benchmark-launcher-cli");
    let orig_path = runner_dir.join("benchmark-launcher-cli-orig");

    // Check if cli_path exists and is an ELF binary
    if tokio::fs::try_exists(&cli_path).await.unwrap_or(false) {
        use tokio::io::AsyncReadExt;
        if let Ok(mut file) = tokio::fs::File::open(&cli_path).await {
            let mut header = [0u8; 4];
            if file.read_exact(&mut header).await.is_ok() && &header == b"\x7fELF" {
                let _ = tokio::fs::rename(&cli_path, &orig_path).await;
            }
        }
    }

    // Write the offline runner script to benchmark-launcher-cli and blender-runner
    let runner_script = include_str!("../../../../../resources/scripts/blender_runner.py");
    let _ = tokio::fs::write(&cli_path, runner_script).await;
    let _ = tokio::fs::write(runner_dir.join("blender-runner"), runner_script).await;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        let _ = tokio::fs::set_permissions(&cli_path, perms.clone()).await;
        let _ = tokio::fs::set_permissions(runner_dir.join("blender-runner"), perms).await;
    }

    // Link cache directories into runner_dir if available
    let mut cache_candidates = Vec::new();
    if let Ok(real_home) = std::env::var("SNAP_REAL_HOME") {
        cache_candidates.push(
            PathBuf::from(real_home)
                .join(".cache")
                .join("blender-benchmark-launcher"),
        );
    }
    if let Some(cache_dir) = dirs::cache_dir() {
        cache_candidates.push(cache_dir.join("blender-benchmark-launcher"));
    }
    if let Ok(user) = std::env::var("USER") {
        cache_candidates.push(PathBuf::from(format!(
            "/home/{}/.cache/blender-benchmark-launcher",
            user
        )));
    }
    for base in cache_candidates {
        for sub in &["blender-versions", "scenes", "benchmark-scripts"] {
            let src = base.join(sub);
            let dst = runner_dir.join(sub);
            if tokio::fs::try_exists(&src).await.unwrap_or(false)
                && !tokio::fs::try_exists(&dst).await.unwrap_or(false)
                && tokio::fs::symlink_metadata(&dst).await.is_err()
            {
                #[cfg(unix)]
                let _ = tokio::fs::symlink(&src, &dst).await;
            }
        }
    }

    Ok(())
}
