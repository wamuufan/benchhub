use benchhub_core::engine::ensure_blender_runner;
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

#[tokio::test]
async fn test_blender_runner_preserves_cpu_device_and_prefers_matching_version() {
    let dir = tempdir().expect("Failed to create tempdir");
    let runner_dir = dir.path();
    // Prevent ensure_blender_runner from symlinking host ~/.cache into tempdir.
    for sub in ["blender-versions", "scenes", "benchmark-scripts"] {
        tokio::fs::create_dir_all(runner_dir.join(sub))
            .await
            .unwrap();
    }
    ensure_blender_runner(runner_dir)
        .await
        .expect("ensure_blender_runner failed");

    // Set up fake HOME cache with both 4.1.0 and 4.2.0 executables, scene, and main.py
    let fake_home = dir.path().join("fake_home");
    let cache = fake_home.join(".cache").join("blender-benchmark-launcher");
    let ver_41 = cache
        .join("blender-versions")
        .join("4.1.0")
        .join("blender-4.1.0");
    let ver_42 = cache
        .join("blender-versions")
        .join("4.2.0")
        .join("blender-4.2.0");
    let scene_dir = cache.join("scenes").join("4.2.0").join("monster");
    let script_dir = cache
        .join("benchmark-scripts")
        .join("4.2.0")
        .join("scripts");
    for d in [&ver_41, &ver_42, &scene_dir, &script_dir] {
        tokio::fs::create_dir_all(d).await.unwrap();
    }

    let mock_blender =
        "#!/bin/sh\necho \"BIN=$0 ARGS=$*\"\necho \"Rendered 120 samples in 60.0 seconds\"\n";
    for (p, marker) in [
        (ver_41.join("blender"), "4.1.0"),
        (ver_42.join("blender"), "4.2.0"),
    ] {
        tokio::fs::write(&p, mock_blender).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))
                .await
                .unwrap();
        }
        let _ = marker;
    }
    tokio::fs::write(scene_dir.join("main.blend"), "dummy")
        .await
        .unwrap();
    tokio::fs::write(script_dir.join("main.py"), "print('ok')")
        .await
        .unwrap();

    let output = tokio::process::Command::new(runner_dir.join("blender-runner"))
        .args([
            "benchmark",
            "monster",
            "--blender-version",
            "4.2.0",
            "--device-type",
            "CPU",
            "--json",
        ])
        .current_dir(runner_dir)
        .env("HOME", &fake_home)
        .env("USER", "benchhub-test-nobody")
        .env_remove("SNAP_REAL_HOME")
        .env("__NV_PRIME_RENDER_OFFLOAD", "1")
        .output()
        .await
        .expect("Failed to execute blender-runner python shim");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "blender-runner failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Device Type: CPU") && stdout.contains("--device-type CPU"),
        "Explicit --device-type CPU must NOT be overridden to OPTIX: {stdout}"
    );
    assert!(
        stdout.contains("4.2.0"),
        "Expected --blender-version 4.2.0 binary to be selected over 4.1.0: {stdout}"
    );
    assert!(
        stdout.contains("\"samples_per_minute\": 120.0"),
        "Expected JSON score output: {stdout}"
    );
}

#[tokio::test]
async fn test_blender_runner_matches_hashed_blender_versions_in_runner_dir() {
    let dir = tempdir().expect("Failed to create tempdir");
    let runner_dir = dir.path();

    // Replicate real blender-benchmark-launcher hashed directory layout inside runner_dir
    // before calling ensure_blender_runner so it does not symlink host ~/.cache into tempdir.
    let hash_41 = "1".repeat(64);
    let hash_42 = "2".repeat(64);
    let ver_41 = runner_dir
        .join("blender-versions")
        .join(hash_41)
        .join("blender-4.1.0-linux-x64");
    let ver_42 = runner_dir
        .join("blender-versions")
        .join(hash_42)
        .join("blender-4.2.0-linux-x64");

    let fake_home = dir.path().join("fake_home");
    let cache = fake_home.join(".cache").join("blender-benchmark-launcher");
    let scene_dir = cache.join("scenes").join("4.2.0").join("monster");
    let script_dir = cache
        .join("benchmark-scripts")
        .join("4.2.0")
        .join("scripts");
    for d in [
        &ver_41,
        &ver_42,
        &scene_dir,
        &script_dir,
        &runner_dir.join("scenes"),
        &runner_dir.join("benchmark-scripts"),
    ] {
        tokio::fs::create_dir_all(d).await.unwrap();
    }

    ensure_blender_runner(runner_dir)
        .await
        .expect("ensure_blender_runner failed");

    let mock_blender =
        "#!/bin/sh\necho \"BIN=$0 ARGS=$*\"\necho \"Rendered 95 samples in 60.0 seconds\"\n";
    for bin in [ver_41.join("blender"), ver_42.join("blender")] {
        tokio::fs::write(&bin, mock_blender).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
                .await
                .unwrap();
        }
    }
    tokio::fs::write(scene_dir.join("main.blend"), "dummy")
        .await
        .unwrap();
    tokio::fs::write(script_dir.join("main.py"), "print('ok')")
        .await
        .unwrap();

    let output = tokio::process::Command::new(runner_dir.join("blender-runner"))
        .args([
            "benchmark",
            "monster",
            "--blender-version",
            "4.2.0",
            "--device-type",
            "CPU",
            "--json",
        ])
        .current_dir(runner_dir)
        .env("HOME", &fake_home)
        .env("USER", "benchhub-test-nobody")
        .env_remove("SNAP_REAL_HOME")
        .output()
        .await
        .expect("Failed to execute blender-runner python shim");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "blender-runner failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("blender-4.2.0-linux-x64/blender"),
        "Expected hashed runner_dir 4.2.0 executable to be selected: {stdout}"
    );
    assert!(
        !stdout.contains("blender-4.1.0-linux-x64/blender"),
        "Must not select 4.1.0 executable when 4.2.0 is requested: {stdout}"
    );
    assert!(
        stdout.contains("\"samples_per_minute\": 95.0"),
        "Expected JSON score output: {stdout}"
    );
}
