//! C-Ray raytracing source compilation adapter.

use super::{compile_c_source, BenchmarkAdapter, BoxFuture};
use anyhow::Result;
use std::path::Path;

/// Adapter that compiles or repairs the `c-ray` binary from `c-ray-mt.c` when missing or GLIBC-incompatible.
pub struct CrayAdapter;

impl BenchmarkAdapter for CrayAdapter {
    fn prepare_runner_dir<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(ensure_cray_runner(dir))
    }
}

/// Ensures `c-ray` exists and is runnable in `runner_dir`, rebuilding from `c-ray-mt.c` if necessary.
pub async fn ensure_cray_runner(runner_dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(runner_dir).await.unwrap_or(false) {
        return Ok(());
    }

    let source_file = runner_dir.join("c-ray-mt.c");
    let bin_path = runner_dir.join("c-ray");
    if !tokio::fs::try_exists(&source_file).await.unwrap_or(false) {
        return Ok(());
    }

    let needs_rebuild = if tokio::fs::try_exists(&bin_path).await.unwrap_or(false) {
        let check = tokio::process::Command::new(&bin_path)
            .arg("-h")
            .output()
            .await;
        match check {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                stderr.contains("GLIBC_") || stderr.contains("version `")
            }
            Err(_) => true,
        }
    } else {
        true
    };

    if needs_rebuild {
        compile_c_source(
            runner_dir,
            &["-O3", "-pthread", "c-ray-mt.c", "-lm", "-o", "c-ray"],
        )
        .await;

        if !tokio::fs::try_exists(&bin_path).await.unwrap_or(false) {
            let prebuilt = Path::new("/app/bin/c-ray");
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
