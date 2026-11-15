//! STREAM memory bandwidth OpenMP compilation adapter.

use super::{compile_c_source, BenchmarkAdapter, BoxFuture};
use crate::engine::platform_env::is_dynamically_broken;
use anyhow::Result;
use std::path::Path;

/// Adapter that builds `stream` from `stream.c` when missing or dynamically broken.
pub struct StreamAdapter;

impl BenchmarkAdapter for StreamAdapter {
    fn prepare_runner_dir<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_stream_runner(dir))
    }
}

/// Returns the `STREAM_ARRAY_SIZE` macro value matching the version directory name.
pub fn stream_array_size_for_dir(runner_dir: &Path) -> &'static str {
    let dir_name = runner_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if dir_name.contains("compact") {
        "20000000"
    } else if dir_name.contains("extended") {
        "160000000"
    } else {
        "80000000"
    }
}

/// Returns the Flatpak prebuilt STREAM binary path matching the version directory name.
pub fn flatpak_prebuilt_stream_for_dir(runner_dir: &Path) -> &'static str {
    match stream_array_size_for_dir(runner_dir) {
        "20000000" => "/app/bin/stream-compact",
        "160000000" => "/app/bin/stream-extended",
        _ => "/app/bin/stream-standard",
    }
}

/// Ensures `stream` exists and links cleanly in `runner_dir`, recompiling `stream.c` with OpenMP if needed.
pub async fn ensure_stream_runner(runner_dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(runner_dir).await.unwrap_or(false) {
        return Ok(());
    }

    let source_file = runner_dir.join("stream.c");
    let bin_path = runner_dir.join("stream");
    if !tokio::fs::try_exists(&source_file).await.unwrap_or(false) {
        return Ok(());
    }

    let needs_rebuild = if tokio::fs::try_exists(&bin_path).await.unwrap_or(false) {
        is_dynamically_broken(&bin_path)
    } else {
        true
    };

    if needs_rebuild {
        let array_size_flag = format!(
            "-DSTREAM_ARRAY_SIZE={}",
            stream_array_size_for_dir(runner_dir)
        );
        tracing::info!(
            "Rebuilding stream runner in {:?} ({})",
            runner_dir,
            array_size_flag
        );
        compile_c_source(
            runner_dir,
            &[
                "-O3",
                "-fopenmp",
                "-mcmodel=medium",
                &array_size_flag,
                "-DNTIMES=10",
                "stream.c",
                "-o",
                "stream",
            ],
        )
        .await;

        if !tokio::fs::try_exists(&bin_path).await.unwrap_or(false)
            || is_dynamically_broken(&bin_path)
        {
            let prebuilt = Path::new(flatpak_prebuilt_stream_for_dir(runner_dir));
            if prebuilt.is_file() {
                tokio::fs::copy(prebuilt, &bin_path).await?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = tokio::fs::metadata(&bin_path).await?.permissions();
                    perms.set_mode(0o755);
                    tokio::fs::set_permissions(&bin_path, perms).await?;
                }
            }
        }
    }
    Ok(())
}
