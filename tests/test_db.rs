use benchhub::db::Db;
use benchhub::models::{GpuMode, HistorySortOrder, RunResult, TelemetryData};
use std::time::SystemTime;
use tempfile::tempdir;

#[test]
fn test_sqlite_db_operations() {
    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_test_{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let db = Db::new(tmp_file.clone()).expect("Failed to create test db");

    let run1 = RunResult {
        id: 0,
        benchmark_id: "7zip".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "Multi-Threaded".to_string(),
        gpu_mode: "Harici GPU (NVIDIA)".to_string(),
        score: Some(96060.0),
        status: "Başarılı".to_string(),
        timestamp: 1724500000,
        duration_secs: 42.5,
        avg_cpu_usage: 98.2,
        peak_cpu_usage: 100.0,
        avg_cpu_temp: 65.4,
        peak_cpu_temp: 78.1,
        avg_cpu_freq_mhz: 3800,
        peak_cpu_freq_mhz: 4200,
        avg_gpu_usage: 15.0,
        peak_gpu_usage: 25.0,
        avg_gpu_temp: 55.0,
        peak_gpu_temp: 62.0,
        avg_gpu_freq_mhz: 1500,
        peak_gpu_freq_mhz: 1800,
        avg_vram_freq_mhz: 6000,
        peak_vram_freq_mhz: 6000,
        avg_gpu_power_w: 35.0,
        peak_gpu_power_w: 50.0,
        avg_power_w: 45.2,
        peak_power_w: 65.0,
        avg_ac_power_w: 92.2,
        peak_ac_power_w: 127.0,
        avg_ram_gb: 8.4,
        peak_ram_gb: 11.2,
        avg_vram_gb: 2.5,
        peak_vram_gb: 3.8,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "Intel Core i7 | NVIDIA RTX | Linux 6.8".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/log.txt".to_string(),
    };

    let id1 = db.insert_run(&run1).expect("Failed to insert run");
    assert!(id1 > 0);

    let history = db.get_history().expect("Failed to query history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].benchmark_id, "7zip");
    assert_eq!(history[0].category, "CPU");
    assert_eq!(history[0].preset_or_version, "Multi-Threaded");
    assert_eq!(history[0].gpu_mode, "Harici GPU (NVIDIA)");
    assert_eq!(history[0].score, Some(96060.0));
    assert_eq!(history[0].status, "Başarılı");
    assert_eq!(history[0].duration_secs, 42.5);
    assert_eq!(history[0].avg_cpu_temp, 65.4);
    assert_eq!(history[0].peak_cpu_temp, 78.1);
    assert_eq!(history[0].avg_gpu_temp, 55.0);
    assert_eq!(history[0].peak_gpu_temp, 62.0);
    assert_eq!(history[0].avg_gpu_power_w, 35.0);
    assert_eq!(history[0].peak_gpu_power_w, 50.0);
    assert_eq!(history[0].avg_power_w, 45.2);
    assert_eq!(history[0].peak_power_w, 65.0);
    assert_eq!(history[0].avg_ac_power_w, 92.2);
    assert_eq!(history[0].peak_ac_power_w, 127.0);
    assert_eq!(history[0].avg_cpu_freq_mhz, 3800);
    assert_eq!(
        history[0].system_info_summary,
        "Intel Core i7 | NVIDIA RTX | Linux 6.8"
    );
    assert_eq!(history[0].log_path, "/tmp/log.txt");

    db.clear_history().expect("Failed to clear history");
    let history_after = db
        .get_history()
        .expect("Failed to query history after clear");
    assert_eq!(history_after.len(), 0);

    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_sqlite_schema_migration() {
    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_migration_test_{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // 1. Create legacy schema without status, duration_secs, log_path
    {
        let legacy_conn = rusqlite::Connection::open(&tmp_file).unwrap();
        legacy_conn
            .execute(
                "CREATE TABLE runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                benchmark_id TEXT NOT NULL,
                score REAL,
                timestamp INTEGER NOT NULL,
                avg_temp REAL,
                max_temp REAL,
                avg_power REAL
            );",
                [],
            )
            .unwrap();
        legacy_conn
            .execute(
                "INSERT INTO runs (benchmark_id, score, timestamp, avg_temp, max_temp, avg_power)
             VALUES ('legacy_bench', 1234.5, 1700000000, 50.0, 60.0, 35.0);",
                [],
            )
            .unwrap();
    }

    // 2. Open using Db::new which must automatically migrate missing columns
    let db = Db::new(tmp_file.clone()).expect("Db::new should migrate legacy table without error");

    // 3. Query history: should read legacy record with default status and duration
    let history = db
        .get_history()
        .expect("get_history should succeed on migrated DB");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].benchmark_id, "legacy_bench");
    assert_eq!(history[0].score, Some(1234.5));
    assert_eq!(history[0].status, "Tamamlandı");
    assert_eq!(history[0].duration_secs, 0.0);
    assert_eq!(history[0].avg_cpu_temp, 50.0);
    assert_eq!(history[0].peak_cpu_temp, 60.0);
    assert_eq!(history[0].avg_power_w, 35.0);
    assert_eq!(history[0].log_path, "");

    // 4. Insert new record with all fields
    let new_run = RunResult {
        id: 0,
        benchmark_id: "new_bench".to_string(),
        category: "GPU".to_string(),
        preset_or_version: "1080p Extreme".to_string(),
        gpu_mode: "⚡ Harici GPU".to_string(),
        score: Some(9999.0),
        status: "Başarılı".to_string(),
        timestamp: 1700000100,
        duration_secs: 15.2,
        avg_cpu_usage: 45.0,
        peak_cpu_usage: 60.0,
        avg_cpu_temp: 55.0,
        peak_cpu_temp: 70.0,
        avg_cpu_freq_mhz: 4200,
        peak_cpu_freq_mhz: 4500,
        avg_gpu_usage: 95.0,
        peak_gpu_usage: 99.0,
        avg_gpu_temp: 60.0,
        peak_gpu_temp: 75.0,
        avg_gpu_freq_mhz: 2000,
        peak_gpu_freq_mhz: 2150,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_gpu_power_w: 70.0,
        peak_gpu_power_w: 85.0,
        avg_power_w: 40.0,
        peak_power_w: 55.0,
        avg_ac_power_w: 122.0,
        peak_ac_power_w: 152.0,
        avg_ram_gb: 6.0,
        peak_ram_gb: 8.0,
        avg_vram_gb: 3.5,
        peak_vram_gb: 4.5,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "AMD Ryzen | NVIDIA RTX | Linux 6.8".to_string(),
        power_profile: "Dengeli (balanced)".to_string(),
        log_path: "/tmp/new.log".to_string(),
    };
    db.insert_run(&new_run)
        .expect("insert_run should succeed on migrated DB");

    let history2 = db
        .get_history()
        .expect("get_history should succeed after insert");
    assert_eq!(history2.len(), 2);
    assert_eq!(history2[0].benchmark_id, "new_bench");
    assert_eq!(history2[0].category, "GPU");
    assert_eq!(history2[0].preset_or_version, "1080p Extreme");
    assert_eq!(history2[0].status, "Başarılı");
    assert_eq!(history2[0].log_path, "/tmp/new.log");

    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_sqlite_filtering_and_deletion() {
    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_filter_test_{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let db = Db::new(tmp_file.clone()).expect("Failed to create test db");

    let run_cpu = RunResult {
        id: 0,
        benchmark_id: "geekbench".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "v6.2.2".to_string(),
        gpu_mode: "Sistem".to_string(),
        score: Some(2500.0),
        status: "Başarılı".to_string(),
        timestamp: 1724500100,
        duration_secs: 120.0,
        avg_cpu_usage: 90.0,
        peak_cpu_usage: 100.0,
        avg_cpu_temp: 72.0,
        peak_cpu_temp: 85.0,
        avg_cpu_freq_mhz: 4100,
        peak_cpu_freq_mhz: 4400,
        avg_gpu_usage: 0.0,
        peak_gpu_usage: 0.0,
        avg_gpu_temp: 0.0,
        peak_gpu_temp: 0.0,
        avg_gpu_freq_mhz: 0,
        peak_gpu_freq_mhz: 0,
        avg_vram_freq_mhz: 0,
        peak_vram_freq_mhz: 0,
        avg_gpu_power_w: 0.0,
        peak_gpu_power_w: 0.0,
        avg_power_w: 48.0,
        peak_power_w: 68.0,
        avg_ac_power_w: 60.0,
        peak_ac_power_w: 80.0,
        avg_ram_gb: 4.5,
        peak_ram_gb: 6.0,
        avg_vram_gb: 0.0,
        peak_vram_gb: 0.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "Intel i7".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/geekbench.log".to_string(),
    };

    let run_gpu = RunResult {
        id: 0,
        benchmark_id: "unigine_superposition".to_string(),
        category: "GPU".to_string(),
        preset_or_version: "1080p Extreme".to_string(),
        gpu_mode: "Harici GPU (NVIDIA)".to_string(),
        score: Some(11000.0),
        status: "Tamamlandı".to_string(),
        timestamp: 1724500200,
        duration_secs: 180.0,
        avg_cpu_usage: 30.0,
        peak_cpu_usage: 45.0,
        avg_cpu_temp: 60.0,
        peak_cpu_temp: 68.0,
        avg_cpu_freq_mhz: 3600,
        peak_cpu_freq_mhz: 4000,
        avg_gpu_usage: 99.0,
        peak_gpu_usage: 100.0,
        avg_gpu_temp: 74.0,
        peak_gpu_temp: 82.0,
        avg_gpu_freq_mhz: 1900,
        peak_gpu_freq_mhz: 2000,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_gpu_power_w: 110.0,
        peak_gpu_power_w: 125.0,
        avg_power_w: 130.0,
        peak_power_w: 145.0,
        avg_ac_power_w: 252.0,
        peak_ac_power_w: 282.0,
        avg_ram_gb: 8.0,
        peak_ram_gb: 10.5,
        avg_vram_gb: 5.5,
        peak_vram_gb: 6.8,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "NVIDIA RTX 3070".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/superposition.log".to_string(),
    };

    let id_cpu = db.insert_run(&run_cpu).expect("Insert CPU run");
    let id_gpu = db.insert_run(&run_gpu).expect("Insert GPU run");

    // 1. Filter by category
    let cpu_filtered = db
        .get_filtered_history("", "CPU", "", "", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(cpu_filtered.len(), 1);
    assert_eq!(cpu_filtered[0].benchmark_id, "geekbench");

    let gpu_filtered = db
        .get_filtered_history("", "GPU", "", "", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(gpu_filtered.len(), 1);
    assert_eq!(gpu_filtered[0].benchmark_id, "unigine_superposition");

    // 2. Filter by search text
    let search_res = db
        .get_filtered_history("RTX 3070", "", "", "", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].benchmark_id, "unigine_superposition");

    // 3. Filter by GPU mode
    let gpu_mode_res = db
        .get_filtered_history("", "", "Harici", "", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(gpu_mode_res.len(), 1);
    assert_eq!(gpu_mode_res[0].benchmark_id, "unigine_superposition");

    // 4. Filter by status
    let status_res = db
        .get_filtered_history("", "", "", "Başarılı", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(status_res.len(), 1);
    assert_eq!(status_res[0].benchmark_id, "geekbench");

    // 5. Delete specific run
    db.delete_run(id_cpu).expect("Delete CPU run");
    let remaining = db.get_history().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, id_gpu);

    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_sqlite_sorting_orders() {
    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_sort_test_{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let db = Db::new(tmp_file.clone()).expect("Failed to create test db");

    let run_a = RunResult {
        id: 0,
        benchmark_id: "bench_a".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "v1".to_string(),
        gpu_mode: "Sistem".to_string(),
        score: Some(1000.0),
        status: "Başarılı".to_string(),
        timestamp: 1000,
        duration_secs: 10.0,
        avg_cpu_usage: 50.0,
        peak_cpu_usage: 60.0,
        avg_cpu_temp: 50.0,
        peak_cpu_temp: 60.0,
        avg_cpu_freq_mhz: 3000,
        peak_cpu_freq_mhz: 3200,
        avg_gpu_usage: 20.0,
        peak_gpu_usage: 30.0,
        avg_gpu_temp: 40.0,
        peak_gpu_temp: 50.0,
        avg_gpu_freq_mhz: 1000,
        peak_gpu_freq_mhz: 1200,
        avg_vram_freq_mhz: 4000,
        peak_vram_freq_mhz: 4000,
        avg_gpu_power_w: 15.0,
        peak_gpu_power_w: 20.0,
        avg_power_w: 30.0,
        peak_power_w: 40.0,
        avg_ac_power_w: 57.0,
        peak_ac_power_w: 72.0,
        avg_ram_gb: 4.0,
        peak_ram_gb: 5.0,
        avg_vram_gb: 1.0,
        peak_vram_gb: 1.5,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "PC A".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/a.log".to_string(),
    };

    let run_b = RunResult {
        id: 0,
        benchmark_id: "bench_b".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "v2".to_string(),
        gpu_mode: "⚡ Harici GPU".to_string(),
        score: Some(2000.0),
        status: "Başarılı".to_string(),
        timestamp: 2000,
        duration_secs: 20.0,
        avg_cpu_usage: 80.0,
        peak_cpu_usage: 95.0,
        avg_cpu_temp: 70.0,
        peak_cpu_temp: 85.0,
        avg_cpu_freq_mhz: 4000,
        peak_cpu_freq_mhz: 4400,
        avg_gpu_usage: 75.0,
        peak_gpu_usage: 90.0,
        avg_gpu_temp: 65.0,
        peak_gpu_temp: 80.0,
        avg_gpu_freq_mhz: 1800,
        peak_gpu_freq_mhz: 2000,
        avg_vram_freq_mhz: 6000,
        peak_vram_freq_mhz: 6000,
        avg_gpu_power_w: 50.0,
        peak_gpu_power_w: 70.0,
        avg_power_w: 60.0,
        peak_power_w: 80.0,
        avg_ac_power_w: 122.0,
        peak_ac_power_w: 162.0,
        avg_ram_gb: 8.0,
        peak_ram_gb: 10.0,
        avg_vram_gb: 2.0,
        peak_vram_gb: 3.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "PC B".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/b.log".to_string(),
    };

    let id_a = db.insert_run(&run_a).unwrap();
    let id_b = db.insert_run(&run_b).unwrap();

    // Date Desc (Default: B then A)
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::DateDesc)
        .unwrap();
    assert_eq!(res[0].id, id_b);
    assert_eq!(res[1].id, id_a);

    // Date Asc (A then B)
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::DateAsc)
        .unwrap();
    assert_eq!(res[0].id, id_a);
    assert_eq!(res[1].id, id_b);

    // Score Desc (B then A)
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::ScoreDesc)
        .unwrap();
    assert_eq!(res[0].id, id_b);
    assert_eq!(res[1].id, id_a);

    // Score Asc (A then B)
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::ScoreAsc)
        .unwrap();
    assert_eq!(res[0].id, id_a);
    assert_eq!(res[1].id, id_b);

    // CPU Temp Peak (B (85) then A (60))
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::CpuTempPeak)
        .unwrap();
    assert_eq!(res[0].id, id_b);
    assert_eq!(res[1].id, id_a);

    // GPU Temp Peak (B (80) then A (50))
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::GpuTempPeak)
        .unwrap();
    assert_eq!(res[0].id, id_b);
    assert_eq!(res[1].id, id_a);

    // Power Avg (B (60) then A (30))
    let res = db
        .get_filtered_history("", "", "", "", HistorySortOrder::PowerAvg)
        .unwrap();
    assert_eq!(res[0].id, id_b);
    assert_eq!(res[1].id, id_a);

    let _ = std::fs::remove_file(tmp_file);
}

#[test]
fn test_db_get_run_by_id() {
    let tmp_file = std::env::temp_dir().join(format!(
        "benchhub_get_by_id_{}.sqlite",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let db = Db::new(tmp_file.clone()).expect("Failed to create test db");

    let run = RunResult {
        id: 0,
        benchmark_id: "test_bench".to_string(),
        category: "CPU".to_string(),
        preset_or_version: "v1.0".to_string(),
        gpu_mode: "Auto".to_string(),
        score: Some(1500.0),
        status: "Başarılı".to_string(),
        timestamp: 123456,
        duration_secs: 12.5,
        avg_cpu_usage: 85.0,
        peak_cpu_usage: 95.0,
        avg_cpu_temp: 55.0,
        peak_cpu_temp: 65.0,
        avg_cpu_freq_mhz: 3600,
        peak_cpu_freq_mhz: 3900,
        avg_gpu_usage: 10.0,
        peak_gpu_usage: 20.0,
        avg_gpu_temp: 45.0,
        peak_gpu_temp: 50.0,
        avg_gpu_freq_mhz: 1100,
        peak_gpu_freq_mhz: 1300,
        avg_vram_freq_mhz: 5000,
        peak_vram_freq_mhz: 5000,
        avg_gpu_power_w: 8.0,
        peak_gpu_power_w: 12.0,
        avg_power_w: 40.0,
        peak_power_w: 50.0,
        avg_ac_power_w: 60.0,
        peak_ac_power_w: 74.0,
        avg_ram_gb: 4.2,
        peak_ram_gb: 5.5,
        avg_vram_gb: 1.2,
        peak_vram_gb: 2.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "SysInfo".to_string(),
        power_profile: "Performans (performance)".to_string(),
        log_path: "/tmp/log".to_string(),
    };

    let id = db.insert_run(&run).unwrap();

    let fetched = db.get_run_by_id(id).unwrap();
    assert!(fetched.is_some());
    let r = fetched.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.benchmark_id, "test_bench");
    assert_eq!(r.score, Some(1500.0));
    assert_eq!(r.avg_cpu_usage, 85.0);
    assert_eq!(r.peak_cpu_usage, 95.0);
    assert_eq!(r.peak_cpu_freq_mhz, 3900);
    assert_eq!(r.avg_gpu_usage, 10.0);
    assert_eq!(r.peak_gpu_usage, 20.0);
    assert_eq!(r.avg_gpu_power_w, 8.0);
    assert_eq!(r.peak_gpu_power_w, 12.0);
    assert_eq!(r.avg_power_w, 40.0);
    assert_eq!(r.peak_power_w, 50.0);
    assert_eq!(r.avg_ac_power_w, 60.0);
    assert_eq!(r.peak_ac_power_w, 74.0);
    assert_eq!(r.avg_ram_gb, 4.2);
    assert_eq!(r.peak_ram_gb, 5.5);

    let non_existent = db.get_run_by_id(99999).unwrap();
    assert!(non_existent.is_none());

    let _ = std::fs::remove_file(tmp_file);
}

fn create_mock_run(
    id: i64,
    benchmark: &str,
    category: &str,
    score: Option<f64>,
    date_offset: i64,
) -> RunResult {
    RunResult {
        id,
        benchmark_id: benchmark.to_string(),
        category: category.to_string(),
        preset_or_version: "Default".to_string(),
        gpu_mode: GpuMode::NvidiaDgpu.to_display_str().to_string(),
        score,
        status: if score.is_some() {
            "Başarılı".to_string()
        } else {
            "İptal Edildi".to_string()
        },
        timestamp: 1700000000 + date_offset,
        duration_secs: 60.0 + (date_offset as f32),
        avg_cpu_usage: 50.0,
        peak_cpu_usage: 80.0,
        avg_cpu_temp: 60.0 + (score.unwrap_or(0.0) as f32 % 20.0),
        peak_cpu_temp: 75.0 + (score.unwrap_or(0.0) as f32 % 15.0),
        avg_cpu_freq_mhz: 3600,
        peak_cpu_freq_mhz: 4200,
        avg_gpu_usage: 70.0,
        peak_gpu_usage: 95.0,
        avg_gpu_temp: 55.0 + (score.unwrap_or(0.0) as f32 % 10.0),
        peak_gpu_temp: 68.0,
        avg_gpu_freq_mhz: 1800,
        peak_gpu_freq_mhz: 2100,
        avg_vram_freq_mhz: 7000,
        peak_vram_freq_mhz: 7000,
        avg_gpu_power_w: 45.0,
        peak_gpu_power_w: 60.0,
        avg_power_w: 65.0 + (score.unwrap_or(0.0) as f32 % 30.0),
        peak_power_w: 90.0,
        avg_ac_power_w: 110.0,
        peak_ac_power_w: 140.0,
        avg_ram_gb: 8.0,
        peak_ram_gb: 12.0,
        avg_vram_gb: 3.5,
        peak_vram_gb: 5.0,
        cpu_throttling: "Yok".to_string(),
        gpu_throttling: "Yok".to_string(),
        system_info_summary: "Test CPU | Test GPU | Test OS".to_string(),
        power_profile: "Performans".to_string(),
        log_path: "/tmp/test.log".to_string(),
    }
}

#[test]
fn test_database_creation_and_schema_initialization() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("benchhub_test.sqlite");

    let db = Db::new(db_path).expect("Failed to create SQLite DB");
    let history = db.get_history().expect("Failed to query empty history");
    assert!(history.is_empty(), "Fresh DB history should be empty");
}

#[test]
fn test_insert_query_and_delete_single_run() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_crud.sqlite");
    let db = Db::new(db_path).unwrap();

    let run = create_mock_run(0, "geekbench6", "CPU", Some(2500.0), 0);
    let inserted_id = db.insert_run(&run).expect("Failed to insert run");
    assert!(inserted_id > 0);

    let retrieved = db.get_run_by_id(inserted_id).expect("Query failed");
    assert!(retrieved.is_some());
    let mut item = retrieved.unwrap();
    assert_eq!(item.benchmark_id, "geekbench6");
    assert_eq!(item.score, Some(2500.0));

    // Update run
    item.score = Some(2650.0);
    item.status = "Güncellendi".to_string();
    db.update_run(&item).expect("Update failed");

    let updated = db.get_run_by_id(inserted_id).unwrap().unwrap();
    assert_eq!(updated.score, Some(2650.0));
    assert_eq!(updated.status, "Güncellendi");

    // Delete run
    db.delete_run(inserted_id).expect("Delete failed");
    let after_delete = db.get_run_by_id(inserted_id).unwrap();
    assert!(after_delete.is_none());
}

#[test]
fn test_telemetry_samples_insertion_and_clear_history() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_telemetry.sqlite");
    let db = Db::new(db_path).unwrap();

    let run = create_mock_run(0, "7zip", "CPU", Some(50000.0), 0);
    let run_id = db.insert_run(&run).unwrap();

    let sample1 = TelemetryData {
        timestamp: 1700000001,
        cpu_temp: 65.0,
        gpu_temp: 50.0,
        cpu_usage: 95.0,
        gpu_usage: 10.0,
        cpu_freq: 3800.0,
        gpu_freq_mhz: 1500.0,
        vram_freq_mhz: 6000.0,
        power_w: 45.0,
        gpu_power_w: 20.0,
        ac_power_w: 80.0,
        ram_usage_mb: 8192.0,
        vram_usage_mb: 2048.0,
        cpu_throttle: "Yok".to_string(),
        gpu_throttle: "Yok".to_string(),
    };

    let sample2 = TelemetryData {
        timestamp: 1700000002,
        cpu_temp: 72.0,
        gpu_temp: 55.0,
        cpu_usage: 99.0,
        gpu_usage: 12.0,
        cpu_freq: 4100.0,
        gpu_freq_mhz: 1600.0,
        vram_freq_mhz: 6000.0,
        power_w: 55.0,
        gpu_power_w: 25.0,
        ac_power_w: 95.0,
        ram_usage_mb: 8500.0,
        vram_usage_mb: 2148.0,
        cpu_throttle: "Yok".to_string(),
        gpu_throttle: "Yok".to_string(),
    };

    db.insert_telemetry_sample(run_id, &sample1)
        .expect("Failed to insert telemetry sample 1");
    db.insert_telemetry_sample(run_id, &sample2)
        .expect("Failed to insert telemetry sample 2");

    let samples = db
        .get_telemetry_points(run_id)
        .expect("Failed to query telemetry samples");
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].cpu_temp, 65.0);
    assert_eq!(samples[1].cpu_temp, 72.0);

    // Clear history should delete runs and cascade/remove telemetry samples
    db.clear_history().expect("Failed to clear history");
    assert!(db.get_history().unwrap().is_empty());
    assert!(db.get_telemetry_points(run_id).unwrap().is_empty());
}
