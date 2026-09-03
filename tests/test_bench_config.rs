use benchhub::bench_config::{FfmpegConfig, SevenZipConfig, UnigineConfig};

#[test]
fn test_ffmpeg_default_config_generation() {
    let cfg = FfmpegConfig::default();
    let (args, preview, preset_display) = cfg.to_run_args();

    assert!(args.contains(&"-c:v".to_string()));
    assert!(args.contains(&"libx264".to_string()));
    assert!(args.contains(&"-crf".to_string()));
    assert!(args.contains(&"23".to_string()));
    assert!(args.contains(&"-preset".to_string()));
    assert!(args.contains(&"medium".to_string()));
    assert!(preview.starts_with("./ffmpeg "));
    assert!(preset_display.contains("1080p"));
    assert!(preset_display.contains("libx264"));
}

#[test]
fn test_ffmpeg_av1_aom_and_custom_duration() {
    let cfg = FfmpegConfig {
        codec: "AV1 (libaom-av1 - CPU Referans)".to_string(),
        crf: 28,
        preset: "medium".to_string(),
        resolution: "4K UHD (3840x2160)".to_string(),
        input_source: "Sentetik Test Deseni".to_string(),
        duration_enabled: true,
        duration_secs: 15,
        threads: 16,
        custom_args: "-an".to_string(),
    };
    let (args, preview, preset_display) = cfg.to_run_args();

    assert!(args.contains(&"libaom-av1".to_string()));
    assert!(args.contains(&"28".to_string()));
    assert!(args.contains(&"-threads".to_string()));
    assert!(args.contains(&"16".to_string()));
    assert!(args.contains(&"-an".to_string()));
    assert!(preview.contains("3840x2160"));
    assert!(preset_display.contains("4K"));
}

#[test]
fn test_ffmpeg_unlimited_duration_and_nvenc() {
    let cfg = FfmpegConfig {
        codec: "H.264 NVENC (NVIDIA GPU)".to_string(),
        crf: 18,
        preset: "p4".to_string(),
        resolution: "1440p 2K (2560x1440)".to_string(),
        input_source: "Sentetik Test Deseni".to_string(),
        duration_enabled: false,
        duration_secs: 0,
        threads: 0,
        custom_args: "-tune film".to_string(),
    };
    let (args, preview, preset_display) = cfg.to_run_args();

    assert!(args.contains(&"h264_nvenc".to_string()));
    assert!(args.contains(&"-cq".to_string()));
    assert!(args.contains(&"18".to_string()));
    assert!(args.contains(&"-tune".to_string()));
    assert!(args.contains(&"film".to_string()));
    assert!(!preview.contains("duration="));
    assert!(preview.contains("2560x1440"));
    assert!(preset_display.contains("1440p"));
}

#[test]
fn test_7zip_config_generation() {
    let cfg = SevenZipConfig {
        threads: 8,
        dict_size: "64MB".to_string(),
        passes: 3,
        custom_args: "-mm=lzma".to_string(),
    };
    let (args, preview, preset_display) = cfg.to_run_args();

    assert!(args.contains(&"b".to_string()));
    assert!(args.contains(&"-mmt=8".to_string()));
    assert!(args.contains(&"-md=64m".to_string()));
    assert!(args.contains(&"3".to_string()));
    assert!(args.contains(&"-mm=lzma".to_string()));
    assert!(preview.contains("-mmt=8 -md=64m 3"));
    assert!(preset_display.contains("8T"));
    assert!(preset_display.contains("3p"));
}

#[test]
fn test_unigine_config_generation() {
    let cfg_super = UnigineConfig {
        preset: "1080p High".to_string(),
        custom_args: "-fullscreen 1".to_string(),
    };
    let (args_super, prev_super, display_super) = cfg_super.to_run_args("Superposition");
    assert!(args_super.contains(&"-preset".to_string()));
    assert!(args_super.contains(&"3".to_string()));
    assert!(prev_super.contains("./superposition"));
    assert!(display_super.contains("High"));

    let cfg_heaven = UnigineConfig {
        preset: "Basic (720p)".to_string(),
        custom_args: String::new(),
    };
    let (args_heaven, prev_heaven, display_heaven) = cfg_heaven.to_run_args("Heaven");
    assert!(args_heaven.contains(&"1280".to_string()));
    assert!(args_heaven.contains(&"720".to_string()));
    assert!(prev_heaven.contains("./heaven_x64"));
    assert!(display_heaven.contains("Basic"));
}

#[test]
fn test_bench_duration_parsing_and_formatting() {
    use benchhub::bench_config::{format_bench_duration, parse_bench_duration};

    assert_eq!(parse_bench_duration("Varsayılan (Genel Ayar)"), None);
    assert_eq!(parse_bench_duration("Default (Global Setting)"), None);
    assert_eq!(parse_bench_duration("Sınırsız (Limit Yok)"), Some(0));
    assert_eq!(parse_bench_duration("Unlimited (No Limit)"), Some(0));
    assert_eq!(parse_bench_duration("30 saniye"), Some(30));
    assert_eq!(parse_bench_duration("60 saniye (1 dk)"), Some(60));
    assert_eq!(parse_bench_duration("120 saniye (2 dk)"), Some(120));
    assert_eq!(parse_bench_duration("180 seconds"), Some(180));
    assert_eq!(parse_bench_duration("300 saniye"), Some(300));
    assert_eq!(parse_bench_duration("600 saniye (10 dk)"), Some(600));

    assert_eq!(format_bench_duration(None, "tr"), "Varsayılan (Genel Ayar)");
    assert_eq!(
        format_bench_duration(None, "en"),
        "Default (Global Setting)"
    );
    assert_eq!(format_bench_duration(Some(0), "tr"), "Sınırsız (Limit Yok)");
    assert_eq!(format_bench_duration(Some(0), "en"), "Unlimited (No Limit)");
    assert_eq!(format_bench_duration(Some(60), "tr"), "60 saniye (1 dk)");
    assert_eq!(format_bench_duration(Some(60), "en"), "60 seconds (1 min)");
}
