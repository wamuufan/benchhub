use benchhub::engine::BenchmarkEngine;
use benchhub::models::{BenchmarkProfile, GpuMode};
use std::path::PathBuf;
use std::time::SystemTime;

#[test]
fn test_score_regex_extraction_7zip() {
    let output = r#"
7-Zip (z) 23.01 (x64) : Copyright (c) 1999-2023 Igor Pavlov : 2023-06-20
RAM size:   31786 MB,  # CPU hardware threads:  16
RAM usage:    883 MB,  # Benchmark threads:     16

Dict        Compressing          |        Decompressing
      Speed Usage    R/U Rating  |   Speed Usage    R/U Rating
       KB/s     %   MIPS   MIPS  |    KB/s     %   MIPS   MIPS
22:   85123  1520   5412  82276  | 1245100  1589   6912 109845
----------------------------------------------------------------
Tot:         1554   6162  96060  |          1589   6912 109845
"#;

    let regex_str = r"Tot:\s+\d+\s+\d+\s+(\d+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(96060.0));
}

#[test]
fn test_score_regex_extraction_geekbench() {
    let output = r#"
Geekbench 6.2.2 Corporate : https://www.geekbench.com/

Benchmark Summary
  Single-Core Score          2145
    Integer Score            2145
  Multi-Core Score           9870
    Integer Score            9870
"#;

    let regex_str = r"(?:Single-Core|Multi-Core) Score\s+(\d+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(9870.0));

    let single_regex = r"Single-Core Score\s+(\d+)";
    let single_score = BenchmarkEngine::extract_score(single_regex, output);
    assert_eq!(single_score, Some(2145.0));
}

#[test]
fn test_score_regex_extraction_cray() {
    let output = "Rendering took: 2 seconds (2341 milliseconds)\n";
    let regex_str = r"Rendering took:.*?\((\d+)\s+milliseconds\)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(2341.0));
}

#[test]
fn test_score_regex_extraction_unigine_heaven() {
    let output = r#"
Unigine Heaven Benchmark 4.0 Basic
FPS: 84.5
Score: 2128
Min FPS: 32.1
Max FPS: 180.4
Render: OpenGL
Mode: 1920x1080 8xAA fullscreen
"#;
    let regex_str = r"Score:\s*([0-9]+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(2128.0));
}

#[test]
fn test_score_regex_extraction_unigine_superposition() {
    let output = r#"
UNIGINE Superposition Benchmark 1.1
GPU: NVIDIA GeForce RTX 3080
FPS: min 58.2, avg 82.4, max 110.1
SCORE: 11054
GPU Temperature: 68 C
"#;
    let regex_str = r"SCORE:\s*([0-9]+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(11054.0));
}

#[test]
fn test_score_regex_extraction_unigine_superposition_automation_file() {
    let output = r#"
Superposition Benchmark v1.1 results:
FPS: 34.2
Score: 4575
Min FPS: 27.3
Max FPS: 42.7
"#;
    let regex_str = r"(?i)(?:^|\s)Score:\s*([0-9]+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(4575.0));
}

#[test]
fn test_score_regex_extraction_gravitymark() {
    let output = r#"
M:  2:52.338: Benchmark Finished
M:  2:52.339: NVIDIA GeForce RTX 4060 Laptop GPU
M:  2:52.339: API: Vulkan
M:  2:52.339: Platform: Linux
M:  2:52.339: Resolution: 640x480
M:  2:52.339: Antialiasing: Temporal
M:  2:52.339: Asteroids: 200,000
M:  2:52.339: Score: 44295
M:  2:52.339: Time: 167.0 s
M:  2:52.339: FPS: 265.2
M:  2:52.359: Clearing Scene
M:  2:52.541: GravityMark 1.89 Done
"#;
    let regex_str = r"(?i)Score:\s*([0-9]+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(44295.0));
}

#[test]
fn test_score_regex_extraction_ffmpeg() {
    let output = r#"
ffmpeg version 6.1-static https://johnvansickle.com/ffmpeg/
frame=  900 fps=142.5 q=-0.0 Lsize=N/A time=00:00:15.00 bitrate=N/A speed=2.38x
video:12345kB audio:0kB subtitle:0kB other streams:0kB global headers:0kB muxing overhead: unknown
bench: utime=10.250s stime=0.120s rtime=6.310s
"#;
    let regex_str = r"fps=\s*([\d\.]+)";
    let score = BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(142.5));
}

#[test]
fn test_ffmpeg_manifest_loading() {
    let ffmpeg_content =
        std::fs::read_to_string("benchmarks/ffmpeg.toml").expect("ffmpeg.toml must exist");
    let ffmpeg: BenchmarkProfile =
        toml::from_str(&ffmpeg_content).expect("ffmpeg.toml should parse into BenchmarkProfile");
    assert_eq!(ffmpeg.id, "ffmpeg");
    assert_eq!(ffmpeg.run_cmd, "./ffmpeg");
    assert_eq!(ffmpeg.default_version.as_deref(), Some("Latest Release"));
    assert_eq!(ffmpeg.versions.len(), 2);
}

#[test]
fn test_unigine_manifests_loading() {
    let unigine_content =
        std::fs::read_to_string("benchmarks/unigine.toml").expect("unigine.toml must exist");
    let unigine: BenchmarkProfile =
        toml::from_str(&unigine_content).expect("unigine.toml should parse into BenchmarkProfile");
    assert_eq!(unigine.id, "unigine");
    assert_eq!(unigine.default_version.as_deref(), Some("Superposition"));
    assert_eq!(unigine.versions.len(), 1);

    let superpos_ver = unigine.for_version("Superposition");
    assert_eq!(superpos_ver.archive_type.as_deref(), Some("run"));
    assert_eq!(
        superpos_ver.binary_relative_path.as_deref(),
        Some("extracted/bin/superposition")
    );
    assert_eq!(superpos_ver.display_download_size(), "~1450 MB");
}

#[tokio::test]
async fn test_unigine_execution_isolation_and_cwd() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_unigine_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let engine = BenchmarkEngine::new(tmp_dir.clone());
    let extracted_dir = tmp_dir
        .join("runners")
        .join("unigine_mock")
        .join("1.0")
        .join("extracted");
    tokio::fs::create_dir_all(&extracted_dir).await.unwrap();

    let mock_bin = extracted_dir.join("Superposition");
    let script_content =
        "#!/bin/sh\necho \"CWD=$(pwd)\"\necho \"LD=$LD_LIBRARY_PATH\"\necho \"SCORE: 12345\"\n";
    tokio::fs::write(&mock_bin, script_content).await.unwrap();

    let profile = BenchmarkProfile {
        id: "unigine_mock".to_string(),
        name: "Mock Superposition".to_string(),
        category: "GPU".to_string(),
        download_size: None,
        description: None,
        download_url: None,
        download_cmd: None,
        run_cmd: String::new(),
        run_args: vec![],
        score_regex: Some(r"SCORE:\s*([0-9]+)".to_string()),
        versions: vec![],
        default_version: None,
        version: Some("1.0".to_string()),
        archive_type: Some("run".to_string()),
        binary_relative_path: Some("extracted/Superposition".to_string()),
        score_unit: Some("Puan".to_string()),
        approx_size_mb: Some(1450),
    };

    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let mut captured_output = String::new();
    let result = engine
        .run_version(&profile, "1.0", cancel_rx, None, GpuMode::Auto, {
            move |line| {
                captured_output.push_str(&line);
            }
        })
        .await
        .expect("Run should succeed");

    assert_eq!(result.score, Some(12345.0));
    assert_eq!(result.status, "Success");

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[test]
fn test_is_installed_empty_dir() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_test_engine_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = BenchmarkEngine::new(tmp_dir.clone());
    let runner_cray = tmp_dir.join("runners").join("cray");
    std::fs::create_dir_all(&runner_cray).unwrap();

    rt.block_on(async {
        // Empty directory should NOT be reported as installed
        assert!(!engine.is_installed("cray"));
        assert!(!engine.is_version_installed_async("cray", "").await);

        // If marker file exists inside, it should be installed
        tokio::fs::write(runner_cray.join(".successfully-installed"), b"")
            .await
            .unwrap();
        assert!(engine.is_installed("cray"));
        assert!(engine.is_version_installed_async("cray", "").await);
    });

    let _ = std::fs::remove_dir_all(tmp_dir);
}

#[test]
fn test_runner_isolation_path() {
    let base_dir = PathBuf::from("/home/user/.local/share/benchhub");
    let engine = BenchmarkEngine::new(base_dir.clone());
    let runner_dir = engine.get_runner_dir("geekbench7").unwrap();
    assert_eq!(
        runner_dir,
        PathBuf::from("/home/user/.local/share/benchhub/runners/geekbench7")
    );
}

#[tokio::test]
async fn test_engine_uninstall() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_engine_test_{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let engine = BenchmarkEngine::new(tmp_dir.clone());
    let runner_dir = engine.get_runner_dir("test_bench").unwrap();
    tokio::fs::create_dir_all(&runner_dir)
        .await
        .expect("Failed to create dummy runner dir");
    tokio::fs::write(runner_dir.join(".successfully-installed"), "")
        .await
        .expect("Failed to write mock install file");
    assert!(engine.is_installed("test_bench"));

    engine
        .uninstall("test_bench")
        .await
        .expect("Failed to uninstall");
    assert!(!engine.is_installed("test_bench"));

    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[tokio::test]
async fn test_engine_cancellation_stopping() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_cancel_test_{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let engine = BenchmarkEngine::new(tmp_dir.clone());
    let profile = BenchmarkProfile {
        id: "sleep_test".to_string(),
        name: "Sleep Test".to_string(),
        category: "Test".to_string(),
        download_size: None,
        description: None,
        download_url: None,
        download_cmd: None,
        run_cmd: "sleep".to_string(),
        run_args: vec!["10".to_string()],
        score_regex: None,
        versions: vec![],
        default_version: None,
        ..Default::default()
    };

    let runner_dir = engine.get_runner_dir("sleep_test").unwrap();
    let _ = tokio::fs::create_dir_all(&runner_dir).await;

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    // Stop the process after 200ms
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let _ = cancel_tx.send(true);
    });

    let res = engine
        .run(&profile, cancel_rx, None, |_| {})
        .await
        .expect("Engine run with cancel should succeed");

    assert_eq!(res.status, "Stopped");
    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[tokio::test]
async fn test_engine_timeout_stopping() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "benchhub_timeout_test_{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let engine = BenchmarkEngine::new(tmp_dir.clone());
    let profile = BenchmarkProfile {
        id: "sleep_test_timeout".to_string(),
        name: "Sleep Test Timeout".to_string(),
        category: "Test".to_string(),
        download_size: None,
        description: None,
        download_url: None,
        download_cmd: None,
        run_cmd: "sleep".to_string(),
        run_args: vec!["10".to_string()],
        score_regex: None,
        versions: vec![],
        default_version: None,
        ..Default::default()
    };

    let runner_dir = engine.get_runner_dir("sleep_test_timeout").unwrap();
    let _ = tokio::fs::create_dir_all(&runner_dir).await;

    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let res = engine
        .run_version_custom(
            &profile,
            "",
            None,
            Some(1),
            cancel_rx,
            None,
            GpuMode::Auto,
            |_| {},
        )
        .await
        .expect("Engine run with timeout should succeed");

    assert_eq!(res.status, "Timeout");
    let _ = tokio::fs::remove_dir_all(tmp_dir).await;
}

#[test]
fn test_trim_terminal_buffer() {
    use benchhub::logging::trim_terminal_buffer;

    let mut buf = String::new();
    for i in 1..=500 {
        buf.push_str(&format!("Line number {}\n", i));
    }

    // Limit to 50 lines and 2000 bytes
    trim_terminal_buffer(&mut buf, 50, 2000);
    assert!(buf.lines().count() <= 50);
    assert!(buf.len() <= 2000);
    assert!(buf.ends_with("Line number 500\n"));
}

#[test]
fn test_read_last_lines_empty_and_nonexistent() {
    use benchhub::logging::read_last_lines;
    use std::path::Path;

    let non_existent = Path::new("/tmp/non_existent_file_12345.log");
    let result =
        read_last_lines(non_existent, 100).expect("Should handle non-existent file gracefully");
    assert_eq!(result, "");

    let tmp_file = std::env::temp_dir().join("benchhub_empty_test.log");
    std::fs::write(&tmp_file, "").expect("Failed to write empty file");
    let result = read_last_lines(&tmp_file, 100).expect("Should handle empty file");
    assert_eq!(result, "");
    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_read_last_lines_tail_limiting() {
    use benchhub::logging::read_last_lines;

    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_tail_test_{}.log",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut content = String::new();
    for i in 1..=20 {
        content.push_str(&format!("Line {}\n", i));
    }
    std::fs::write(&tmp_file, content).expect("Failed to write test lines");

    let last_5 = read_last_lines(&tmp_file, 5).expect("Failed to read last 5 lines");
    let lines: Vec<&str> = last_5.lines().collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0], "Line 16");
    assert_eq!(lines[4], "Line 20");

    let last_50 = read_last_lines(&tmp_file, 50).expect("Failed to read last 50 lines");
    let lines_50: Vec<&str> = last_50.lines().collect();
    assert_eq!(lines_50.len(), 20);
    assert_eq!(lines_50[0], "Line 1");
    assert_eq!(lines_50[19], "Line 20");

    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_read_last_lines_large_file() {
    use benchhub::logging::read_last_lines;

    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_large_test_{}.log",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut content = String::new();
    for i in 1..=5000 {
        content.push_str(&format!("Log entry line number {}\n", i));
    }
    std::fs::write(&tmp_file, content).expect("Failed to write large test log");

    let last_100 = read_last_lines(&tmp_file, 100).expect("Failed to read last 100 lines");
    let lines: Vec<&str> = last_100.lines().collect();
    assert_eq!(lines.len(), 100);
    assert_eq!(lines[0], "Log entry line number 4901");
    assert_eq!(lines[99], "Log entry line number 5000");

    let _ = std::fs::remove_file(tmp_file);
}

#[tokio::test]
async fn test_geekbench_engine_execution_and_score_capture() {
    let local_data = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("benchhub");
    let runner_dir_6 = local_data.join("runners/geekbench/6.2.2");
    let runner_dir_legacy = local_data.join("runners/geekbench6");
    let (runner_id, run_cmd) = if runner_dir_6.exists() {
        ("geekbench", "./geekbench6")
    } else if runner_dir_legacy.exists() {
        ("geekbench6", "./geekbench6")
    } else {
        return;
    };

    let base_dir = local_data;
    let engine = BenchmarkEngine::new(base_dir);
    let profile = BenchmarkProfile {
        id: runner_id.to_string(),
        name: "Geekbench".to_string(),
        category: "CPU".to_string(),
        download_size: None,
        description: None,
        download_url: None,
        download_cmd: None,
        run_cmd: run_cmd.to_string(),
        run_args: vec![
            "--section".to_string(),
            "1".to_string(),
            "--workload".to_string(),
            "101".to_string(),
            "--no-upload".to_string(),
        ],
        score_regex: Some(r"(?:Single-Core|Multi-Core) Score\s+(\d+)".to_string()),
        versions: vec![],
        default_version: None,
        ..Default::default()
    };

    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let result = engine
        .run(&profile, cancel_rx, None, |line| {
            print!("{}", line);
        })
        .await
        .expect("Geekbench run should succeed");

    assert!(result.score.is_some(), "Score must be captured!");
    assert!(result.score.unwrap() > 100.0, "Score must be valid number");
    assert_eq!(result.status, "Success");
}

#[test]
fn test_ycruncher_manifest_loading() {
    let ycruncher_content =
        std::fs::read_to_string("benchmarks/ycruncher.toml").expect("ycruncher.toml must exist");
    let ycruncher: benchhub::models::BenchmarkProfile =
        toml::from_str(&ycruncher_content).expect("ycruncher.toml should parse");
    assert_eq!(ycruncher.id, "ycruncher");
    assert_eq!(ycruncher.run_cmd, "./y-cruncher");
    assert_eq!(ycruncher.default_version.as_deref(), Some("500M"));
    assert_eq!(ycruncher.versions.len(), 3);
}

#[test]
fn test_score_regex_extraction_ycruncher() {
    let output = r#"
Total Computation Time:     24.385 seconds  ( 0.406 minutes )
Start-to-End Wall Time:     25.444 seconds  ( 0.424 minutes )

CPU Utilization:         1229.63 %  +   11.44 % kernel overhead
Multi-core Efficiency:     76.85 %  +    0.72 % kernel overhead
"#;
    let regex_str = r"Total Computation Time:\s*([0-9\.]+)";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(24.385));
}

#[test]
fn test_stream_manifest_loading() {
    let stream_content =
        std::fs::read_to_string("benchmarks/stream.toml").expect("stream.toml must exist");
    let stream: benchhub::models::BenchmarkProfile =
        toml::from_str(&stream_content).expect("stream.toml should parse");
    assert_eq!(stream.id, "stream");
    assert_eq!(stream.run_cmd, "./stream");
    assert_eq!(stream.default_version.as_deref(), Some("Standard"));
    assert_eq!(stream.versions.len(), 3);
}

#[test]
fn test_score_regex_extraction_stream() {
    let output = r#"
Function    Best Rate MB/s  Avg time     Min time     Max time
Copy:           26289.0     0.064157     0.060862     0.070592
Scale:          16908.1     0.099730     0.094629     0.105017
Add:            18575.1     0.135252     0.129205     0.147357
Triad:          18537.1     0.135190     0.129470     0.144045
-------------------------------------------------------------
Solution Validates: avg error less than 1.000000e-13 on all three arrays
"#;
    let regex_str = r"Triad:\s*([0-9\.]+)";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(18537.1));
}

#[test]
fn test_llama_bench_manifest_loading() {
    let llama_content = std::fs::read_to_string("benchmarks/llama-bench.toml")
        .expect("llama-bench.toml must exist");
    let llama: benchhub::models::BenchmarkProfile =
        toml::from_str(&llama_content).expect("llama-bench.toml should parse");
    assert_eq!(llama.id, "llama-bench");
    assert_eq!(llama.default_version.as_deref(), Some("Fast (stories260K)"));
    assert_eq!(llama.versions.len(), 2);
}

#[test]
fn test_score_regex_extraction_llama_bench() {
    let output = r#"
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
| llama ?B (guessed) all F32     |   1.12 MiB |     0.29 M | CPU        |       8 |           pp512 |     7746.26 ± 156.31 |
| llama ?B (guessed) all F32     |   1.12 MiB |     0.29 M | CPU        |       8 |           tg128 |     4989.73 ± 220.36 |
"#;
    let regex_str = r"\|\s*tg128\s*\|\s*(?:[^|]*\|\s*)?([\d\.]+)\s*±";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(4989.73));
}

#[test]
fn test_fio_manifest_loading() {
    let fio_content = std::fs::read_to_string("benchmarks/fio.toml").expect("fio.toml must exist");
    let fio: benchhub::models::BenchmarkProfile =
        toml::from_str(&fio_content).expect("fio.toml should parse");
    assert_eq!(fio.id, "fio");
    assert_eq!(fio.run_cmd, "./fio");
    assert_eq!(fio.default_version.as_deref(), Some("Seq-Read"));
    assert_eq!(fio.versions.len(), 4);
    assert_eq!(fio.score_unit.as_deref(), Some("MB/s"));
}

#[test]
fn test_score_regex_extraction_fio() {
    let output_read = r#"
quick_test: (groupid=0, jobs=1): err= 0: pid=47360: Tue Sep  1 07:18:13 2026
  read: IOPS=596k, BW=2329MiB/s (2442MB/s)(6986MiB/3000msec)
    clat (nsec): min=581, max=65223, avg=1215.81, stdev=530.29
Run status group 0 (all jobs):
   READ: bw=2329MiB/s (2442MB/s), 2329MiB/s-2329MiB/s (2442MB/s-2442MB/s), io=6986MiB (7325MB), run=3000-3000msec
"#;
    let regex_str = r"(?:bw|BW)=([0-9\.]+)(?:MiB/s|MB/s)";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output_read);
    assert_eq!(score, Some(2329.0));

    let output_write = r#"
write_test: (groupid=0, jobs=1): err= 0: pid=47361: Tue Sep  1 07:18:15 2026
  write: IOPS=450k, BW=1850.5MiB/s (1940MB/s)(5550MiB/3000msec)
"#;
    let score_w = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output_write);
    assert_eq!(score_w, Some(1850.5));
}

#[test]
fn test_score_regex_extraction_ycruncher_with_ansi_escapes() {
    let output = "\u{1b}[01;32mTotal Computation Time:      \u{1b}[01;33m22.678 seconds  ( 0.378 minutes ) \u{1b}[01;37m\n\u{1b}[01;32mStart-to-End Wall Time:      \u{1b}[01;33m23.542 seconds  ( 0.392 minutes ) \u{1b}[01;37m";
    let regex_str = r"Total Computation Time:[^\d]*([0-9\.]+)";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(22.678));
}

#[test]
fn test_score_regex_extraction_llama_bench_variations() {
    let output = r#"
| model                          |       size |     params | backend    | threads |            test |                  t/s |
| ------------------------------ | ---------: | ---------: | ---------- | ------: | --------------: | -------------------: |
| llama ?B (guessed) all F32     |   1.12 MiB |     0.29 M | CPU        |       8 |           pp128 |      42276.89 ± 0.00 |
| llama ?B (guessed) all F32     |   1.12 MiB |     0.29 M | CPU        |       8 |            tg64 |      10248.39 ± 0.00 |
"#;
    let regex_str = r"\|\s*tg\d*\s*\|\s*([\d\.]+)\s*±";
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(10248.39));

    let output_128 = r#"
| stories260K.gguf               |   1.12 MiB |     0.29 M | CPU        |      16 |           tg128 |        185.32 ± 1.20 |
"#;
    let score_128 = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output_128);
    assert_eq!(score_128, Some(185.32));
}

#[test]
fn test_score_regex_extraction_blender_json() {
    let output = r#"
{
  "blender_version": "4.2.0",
  "benchmark_launcher_version": "3.3.0",
  "device_type": "CPU",
  "scene": "monster",
  "samples_per_minute": 154.23,
  "total_render_time": 42.1
}
"#;
    let regex_str = r#""samples_per_minute"[^\d]*([0-9\.]+)"#;
    let score = benchhub::engine::BenchmarkEngine::extract_score(regex_str, output);
    assert_eq!(score, Some(154.23));
}
