use serde::{Deserialize, Serialize};

/// Validates an individual CLI argument token for safety.
/// Rejects dangerous flags (-o, --output, -y), path traversal ('..'), absolute root paths ('/'),
/// and dangerous shell injection characters.
pub fn is_safe_custom_arg(arg: &str) -> bool {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Reject dangerous flags: -o, --output, -y
    if trimmed == "-o"
        || trimmed == "--output"
        || trimmed == "-y"
        || trimmed.starts_with("-o=")
        || trimmed.starts_with("--output=")
        || trimmed.starts_with("-y=")
    {
        return false;
    }
    // Reject path traversal ('..')
    if trimmed.contains("..") {
        return false;
    }
    // Reject absolute root paths ('/') or options assigning root path
    if trimmed.starts_with('/') || trimmed.contains("=/") {
        return false;
    }
    // Reject dangerous shell metacharacters
    if trimmed
        .chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n' | '\r'))
    {
        return false;
    }
    true
}

/// Filters a string of custom arguments, keeping only safe tokens.
pub fn filter_safe_custom_args(custom_args: &str) -> Vec<String> {
    custom_args
        .split_whitespace()
        .filter(|token| is_safe_custom_arg(token))
        .map(|s| s.to_string())
        .collect()
}

/// Validates an individual custom argument or returns an error string describing why it's unsafe.
pub fn validate_custom_arg(arg: &str) -> Result<(), &'static str> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Err("Argüman boş olamaz");
    }
    if trimmed == "-o"
        || trimmed == "--output"
        || trimmed == "-y"
        || trimmed.starts_with("-o=")
        || trimmed.starts_with("--output=")
        || trimmed.starts_with("-y=")
    {
        return Err("Tehlikeli bayrak kullanımı (-o, --output, -y) yasaktır");
    }
    if trimmed.contains("..") {
        return Err("Dizin geçişi ('..') içeren argümanlar yasaktır");
    }
    if trimmed.starts_with('/') || trimmed.contains("=/") {
        return Err("Kök dizini ('/') referans alan mutlak yollar yasaktır");
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n' | '\r'))
    {
        return Err("Tehlikeli kabuk karakterleri içeren argümanlar yasaktır");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfmpegConfig {
    pub codec: String,
    pub crf: u32,
    pub preset: String,
    pub resolution: String,
    pub input_source: String,
    pub duration_enabled: bool,
    pub duration_secs: u32,
    pub threads: u32,
    pub custom_args: String,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            codec: "H.264 (libx264 - CPU)".to_string(),
            crf: 23,
            preset: "medium".to_string(),
            resolution: "1080p FHD (1920x1080)".to_string(),
            input_source: "Sentetik Test Kaynağı (lavfi testsrc2)".to_string(),
            duration_enabled: true,
            duration_secs: 20,
            threads: 0,
            custom_args: String::new(),
        }
    }
}

impl FfmpegConfig {
    pub fn for_version(_version: &str) -> Self {
        Self::default()
    }

    pub fn to_run_args(&self) -> (Vec<String>, String, String) {
        let codec_lower = self.codec.to_lowercase();
        let codec_raw = if codec_lower.contains("libx265")
            || (codec_lower.contains("h.265") && codec_lower.contains("cpu"))
        {
            "libx265"
        } else if codec_lower.contains("libsvtav1") || codec_lower.contains("svt") {
            "libsvtav1"
        } else if codec_lower.contains("libaom") || codec_lower.contains("aom") {
            "libaom-av1"
        } else if codec_lower.contains("vp9") {
            "libvpx-vp9"
        } else if codec_lower.contains("h264_nvenc")
            || (codec_lower.contains("h.264") && codec_lower.contains("nvenc"))
        {
            "h264_nvenc"
        } else if codec_lower.contains("hevc_nvenc")
            || ((codec_lower.contains("hevc") || codec_lower.contains("h.265"))
                && codec_lower.contains("nvenc"))
        {
            "hevc_nvenc"
        } else if codec_lower.contains("av1_nvenc")
            || (codec_lower.contains("av1") && codec_lower.contains("nvenc"))
        {
            "av1_nvenc"
        } else if codec_lower.contains("h264_vaapi")
            || (codec_lower.contains("h.264") && codec_lower.contains("vaapi"))
        {
            "h264_vaapi"
        } else if codec_lower.contains("hevc_vaapi")
            || ((codec_lower.contains("hevc") || codec_lower.contains("h.265"))
                && codec_lower.contains("vaapi"))
        {
            "hevc_vaapi"
        } else {
            "libx264"
        };

        let res_raw = if self.resolution.contains("3840x2160") || self.resolution.contains("4K") {
            "3840x2160"
        } else if self.resolution.contains("2560x1440")
            || self.resolution.contains("1440p")
            || self.resolution.contains("2K")
        {
            "2560x1440"
        } else if self.resolution.contains("1280x720") || self.resolution.contains("720p") {
            "1280x720"
        } else if self.resolution.contains("854x480") || self.resolution.contains("480p") {
            "854x480"
        } else {
            "1920x1080"
        };

        let preset_raw = self.preset.trim();

        let is_demo_video = self.input_source.contains("Buck Bunny")
            || self.input_source.contains("Tears of Steel");

        let mut args: Vec<String> = vec!["-y".to_string()];

        if is_demo_video {
            let video_file = if self.input_source.contains("Buck Bunny") {
                "big_buck_bunny_1080p.mp4"
            } else {
                "tears_of_steel_1080p.mp4"
            };
            args.push("-i".to_string());
            args.push(video_file.to_string());
            // Video filtreleri — hedef çözünürlüğe scale
            args.push("-vf".to_string());
            args.push(format!("scale={}", res_raw.replace('x', ":")));
        } else {
            // Sentetik kaynak
            let input_src = if self.duration_enabled && self.duration_secs > 0 {
                format!(
                    "testsrc2=duration={}:size={}:rate=60",
                    self.duration_secs, res_raw
                )
            } else {
                format!("testsrc2=size={}:rate=60", res_raw)
            };
            args.push("-f".to_string());
            args.push("lavfi".to_string());
            args.push("-i".to_string());
            args.push(input_src);
        }

        let is_nvenc = codec_raw.ends_with("_nvenc");
        let is_vaapi = codec_raw.ends_with("_vaapi");

        if is_vaapi {
            args.push("-vaapi_device".to_string());
            args.push("/dev/dri/renderD128".to_string());
            args.push("-vf".to_string());
            args.push("format=nv12,hwupload".to_string());
        }

        args.push("-c:v".to_string());
        args.push(codec_raw.to_string());

        if is_nvenc {
            args.push("-cq".to_string());
            args.push(self.crf.to_string());
            args.push("-preset".to_string());
            args.push("p4".to_string());
        } else if is_vaapi {
            args.push("-qp".to_string());
            args.push(self.crf.to_string());
        } else if codec_raw == "libvpx-vp9" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
        } else if codec_raw == "libsvtav1" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            let svt_preset = if preset_raw.chars().all(|c| c.is_ascii_digit()) {
                preset_raw
            } else {
                "6"
            };
            args.push("-preset".to_string());
            args.push(svt_preset.to_string());
        } else if codec_raw == "libaom-av1" {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-b:v".to_string());
            args.push("0".to_string());
            args.push("-cpu-used".to_string());
            args.push("4".to_string());
        } else {
            args.push("-crf".to_string());
            args.push(self.crf.to_string());
            args.push("-preset".to_string());
            args.push(preset_raw.to_string());
        }

        if self.threads > 0 {
            args.push("-threads".to_string());
            args.push(self.threads.to_string());
        }

        if !self.custom_args.trim().is_empty() {
            for token in self.custom_args.split_whitespace() {
                if is_safe_custom_arg(token) {
                    args.push(token.to_string());
                }
            }
        }

        args.push("-f".to_string());
        args.push("null".to_string());
        args.push("-".to_string());

        let cmd_preview = format!("./ffmpeg {}", args.join(" "));
        let res_name = if res_raw == "3840x2160" {
            "4K"
        } else if res_raw == "1920x1080" {
            "1080p"
        } else if res_raw == "2560x1440" {
            "1440p"
        } else if res_raw == "1280x720" {
            "720p"
        } else {
            res_raw
        };

        let preset_display = format!(
            "{} {} (CRF {}, {})",
            res_name, codec_raw, self.crf, preset_raw
        );

        (args, cmd_preview, preset_display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SevenZipConfig {
    pub threads: u32,
    pub dict_size: String,
    pub passes: u32,
    pub custom_args: String,
}

impl Default for SevenZipConfig {
    fn default() -> Self {
        Self {
            threads: 0,
            dict_size: "32MB (Varsayılan)".to_string(),
            passes: 1,
            custom_args: String::new(),
        }
    }
}

impl SevenZipConfig {
    pub fn to_run_args(&self) -> (Vec<String>, String, String) {
        let mut args = vec!["b".to_string()];

        if self.threads > 0 {
            args.push(format!("-mmt={}", self.threads));
        }

        let dict_flag = if self.dict_size.contains("64") {
            "-md=64m"
        } else if self.dict_size.contains("128") {
            "-md=128m"
        } else if self.dict_size.contains("256") {
            "-md=256m"
        } else if self.dict_size.contains("512") {
            "-md=512m"
        } else if self.dict_size.contains("1024")
            || self.dict_size.contains("1 GB")
            || self.dict_size.contains("1GB")
        {
            "-md=1024m"
        } else {
            "-md=32m"
        };
        args.push(dict_flag.to_string());

        if self.passes > 1 {
            args.push(self.passes.to_string());
        }

        if !self.custom_args.trim().is_empty() {
            for token in self.custom_args.split_whitespace() {
                if is_safe_custom_arg(token) {
                    args.push(token.to_string());
                }
            }
        }

        let cmd_preview = format!("./7zz {}", args.join(" "));
        let dict_label = dict_flag.trim_start_matches("-md=");
        let threads_label = if self.threads == 0 {
            "Auto".to_string()
        } else {
            self.threads.to_string()
        };
        let preset_display = format!(
            "7z ({}T, {} Dict, {}p)",
            threads_label, dict_label, self.passes
        );

        (args, cmd_preview, preset_display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnigineConfig {
    pub preset: String,
    pub custom_args: String,
}

impl Default for UnigineConfig {
    fn default() -> Self {
        Self {
            preset: "1080p Extreme".to_string(),
            custom_args: String::new(),
        }
    }
}

impl UnigineConfig {
    pub fn for_version(version: &str) -> Self {
        let preset = if version.contains("Heaven") {
            "Extreme (1080p)".to_string()
        } else {
            "1080p Extreme".to_string()
        };
        Self {
            preset,
            custom_args: String::new(),
        }
    }

    pub fn to_run_args(&self, version: &str) -> (Vec<String>, String, String) {
        let is_heaven = version.contains("Heaven");
        let mut args = Vec::new();

        if is_heaven {
            args.extend(vec![
                "-data_path".to_string(),
                "../".to_string(),
                "-sound_app".to_string(),
                "null".to_string(),
                "-engine_config".to_string(),
                "../data/heaven_4.0.cfg".to_string(),
                "-system_script".to_string(),
                "heaven/unigine.cpp".to_string(),
                "-video_mode".to_string(),
                "-1".to_string(),
                "-extern_define".to_string(),
                "PHORONIX,RELEASE".to_string(),
                "-video_app".to_string(),
                "opengl".to_string(),
                "-video_fullscreen".to_string(),
                "1".to_string(),
            ]);

            if self.preset.contains("720p") || self.preset.contains("Basic") {
                args.push("-video_width".to_string());
                args.push("1280".to_string());
                args.push("-video_height".to_string());
                args.push("720".to_string());
            } else {
                args.push("-video_width".to_string());
                args.push("1920".to_string());
                args.push("-video_height".to_string());
                args.push("1080".to_string());
            }
        } else {
            // Superposition
            let (w, h, p_num) = if self.preset.contains("8K") {
                ("7680", "4320", "6")
            } else if self.preset.contains("4K") {
                ("3840", "2160", "5")
            } else if self.preset.contains("720p") || self.preset.contains("Low") {
                ("1280", "720", "1")
            } else if self.preset.contains("Medium") {
                ("1920", "1080", "2")
            } else if self.preset.contains("High") {
                ("1920", "1080", "3")
            } else {
                // 1080p Extreme (Default)
                ("1920", "1080", "4")
            };

            args.extend(vec![
                "-sound_app".to_string(),
                "openal".to_string(),
                "-system_script".to_string(),
                "superposition/system_script.cpp".to_string(),
                "-data_path".to_string(),
                "../".to_string(),
                "-engine_config".to_string(),
                "../data/superposition/unigine.cfg".to_string(),
                "-video_width".to_string(),
                w.to_string(),
                "-video_height".to_string(),
                h.to_string(),
                "-video_mode".to_string(),
                "-1".to_string(),
                "-project_name".to_string(),
                "Superposition".to_string(),
                "-video_resizable".to_string(),
                "1".to_string(),
                "-console_command".to_string(),
                "config_readonly 1 && world_load superposition/superposition".to_string(),
                "-mode".to_string(),
                "2".to_string(),
                "-preset".to_string(),
                p_num.to_string(),
            ]);
        }

        if !self.custom_args.trim().is_empty() {
            for token in self.custom_args.split_whitespace() {
                if is_safe_custom_arg(token) {
                    args.push(token.to_string());
                }
            }
        }

        let cmd_name = if is_heaven {
            "./heaven_x64"
        } else {
            "./superposition"
        };
        let cmd_preview = format!("{} {}", cmd_name, args.join(" "));
        let preset_display = format!("{} ({})", version, self.preset);

        (args, cmd_preview, preset_display)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BenchmarkCustomConfig {
    pub benchmark_id: String,
    pub ffmpeg: Option<FfmpegConfig>,
    pub seven_zip: Option<SevenZipConfig>,
    pub unigine: Option<UnigineConfig>,
    pub custom_args: String,
    pub timeout_secs: Option<u64>,
    pub effective_run_args: Option<Vec<String>>,
    pub custom_preset_name: Option<String>,
}

pub fn parse_bench_duration(val: &str) -> Option<u64> {
    if val.contains("Varsayılan") || val.contains("Default") {
        None
    } else if val.contains("Sınırsız") || val.contains("Unlimited") {
        Some(0)
    } else if val.contains("600") {
        Some(600)
    } else if val.contains("300") {
        Some(300)
    } else if val.contains("180") {
        Some(180)
    } else if val.contains("120") {
        Some(120)
    } else if val.contains("60") {
        Some(60)
    } else if val.contains("30") {
        Some(30)
    } else {
        None
    }
}

pub fn format_bench_duration(opt: Option<u64>, lang: &str) -> String {
    let is_en = lang == "en";
    match opt {
        None => {
            if is_en {
                "Default (Global Setting)".to_string()
            } else {
                "Varsayılan (Genel Ayar)".to_string()
            }
        }
        Some(0) => {
            if is_en {
                "Unlimited (No Limit)".to_string()
            } else {
                "Sınırsız (Limit Yok)".to_string()
            }
        }
        Some(30) => {
            if is_en {
                "30 seconds".to_string()
            } else {
                "30 saniye".to_string()
            }
        }
        Some(60) => {
            if is_en {
                "60 seconds (1 min)".to_string()
            } else {
                "60 saniye (1 dk)".to_string()
            }
        }
        Some(120) => {
            if is_en {
                "120 seconds (2 min)".to_string()
            } else {
                "120 saniye (2 dk)".to_string()
            }
        }
        Some(180) => {
            if is_en {
                "180 seconds (3 min)".to_string()
            } else {
                "180 saniye (3 dk)".to_string()
            }
        }
        Some(300) => {
            if is_en {
                "300 seconds (5 min)".to_string()
            } else {
                "300 saniye (5 dk)".to_string()
            }
        }
        Some(600) => {
            if is_en {
                "600 seconds (10 min)".to_string()
            } else {
                "600 saniye (10 dk)".to_string()
            }
        }
        Some(secs) => {
            if is_en {
                format!("{} seconds", secs)
            } else {
                format!("{} saniye", secs)
            }
        }
    }
}
