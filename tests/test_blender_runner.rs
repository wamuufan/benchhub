use benchhub::engine::ensure_blender_runner;
use tempfile::tempdir;

#[tokio::test]
async fn test_ensure_blender_runner_creates_scripts() {
    let dir = tempdir().expect("Failed to create tempdir");
    let runner_dir = dir.path();

    ensure_blender_runner(runner_dir)
        .await
        .expect("ensure_blender_runner failed");

    let cli_path = runner_dir.join("benchmark-launcher-cli");
    let runner_path = runner_dir.join("blender-runner");

    assert!(cli_path.is_file(), "benchmark-launcher-cli was not created");
    assert!(runner_path.is_file(), "blender-runner was not created");

    let content = tokio::fs::read_to_string(&cli_path).await.unwrap();
    assert!(content.contains("#!/usr/bin/env python3"));
    assert!(content.contains("samples_per_minute"));
    assert!(content.contains("CYCLES"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = tokio::fs::metadata(&cli_path).await.unwrap().permissions();
        assert_eq!(perms.mode() & 0o111, 0o111, "Script should be executable");
    }
}

#[tokio::test]
async fn test_ensure_blender_runner_backs_up_elf() {
    let dir = tempdir().expect("Failed to create tempdir");
    let runner_dir = dir.path();

    // Create a mock Go ELF binary
    let cli_path = runner_dir.join("benchmark-launcher-cli");
    tokio::fs::write(&cli_path, b"\x7fELFfake_go_binary_data")
        .await
        .unwrap();

    ensure_blender_runner(runner_dir)
        .await
        .expect("ensure_blender_runner failed");

    let orig_path = runner_dir.join("benchmark-launcher-cli-orig");
    assert!(
        orig_path.is_file(),
        "Original ELF binary should be backed up to benchmark-launcher-cli-orig"
    );

    let orig_content = tokio::fs::read(&orig_path).await.unwrap();
    assert_eq!(orig_content, b"\x7fELFfake_go_binary_data");

    // The new benchmark-launcher-cli should be the python script
    let new_content = tokio::fs::read_to_string(&cli_path).await.unwrap();
    assert!(new_content.contains("#!/usr/bin/env python3"));
}
