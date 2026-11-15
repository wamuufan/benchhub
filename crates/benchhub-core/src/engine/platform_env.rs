//! Platform, sandbox, dynamic-linker, and GPU environment handling for benchmark runners.
//!
//! Centralizes Native / Snap / Flatpak environment detection, CA bundle propagation,
//! graphics and hybrid-GPU environment variables, runner `LD_LIBRARY_PATH` construction,
//! ELF dynamic-link verification, executable permissions, `libgthread` shims, and
//! process-group termination.

use crate::models::GpuMode;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Host runtime packaging context in which the daemon executes benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSandbox {
    Native,
    Snap,
    Flatpak,
}

impl RuntimeSandbox {
    /// Detects the active runtime environment from standard sandbox markers.
    pub fn detect() -> Self {
        Self::detect_from(
            std::env::var_os("SNAP").is_some(),
            std::env::var_os("FLATPAK_ID").is_some(),
        )
    }

    fn detect_from(has_snap: bool, has_flatpak: bool) -> Self {
        if has_snap {
            Self::Snap
        } else if has_flatpak {
            Self::Flatpak
        } else {
            Self::Native
        }
    }
}

/// Returns `true` when `file_name` looks like an installer, script archive, or compressed package.
pub fn is_archive_or_installer(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower.ends_with(".run")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".xz")
        || lower.ends_with(".zip")
        || lower.ends_with(".deb")
        || lower.ends_with(".rpm")
        || lower.ends_with(".iso")
        || lower.ends_with(".7z")
        || lower.ends_with(".bz2")
        || lower.ends_with(".zst")
        || lower.contains("installer")
        || lower.contains("setup")
        || lower.contains("makeself")
        || lower.contains("extract")
}

/// Returns `true` if `path` starts with an ELF magic header or shebang (`#!`).
pub fn is_elf_or_script(path: &Path) -> bool {
    use std::io::Read;
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut header = [0u8; 4];
        if file.read_exact(&mut header).is_ok() && (&header == b"\x7fELF" || &header[..2] == b"#!")
        {
            return true;
        }
    }
    false
}

/// Returns the `(phoff, phentsize, phnum)` offsets from a little-endian ELF header.
fn elf_phdr_layout(header: &[u8; 64], is_64: bool) -> (u64, u64, usize) {
    if is_64 {
        let phoff = u64::from_le_bytes(header[32..40].try_into().unwrap_or_default());
        let phentsize = u16::from_le_bytes(header[54..56].try_into().unwrap_or_default()) as u64;
        let phnum = u16::from_le_bytes(header[56..58].try_into().unwrap_or_default()) as usize;
        (phoff, phentsize, phnum)
    } else {
        let phoff = u32::from_le_bytes(header[28..32].try_into().unwrap_or_default()) as u64;
        let phentsize = u16::from_le_bytes(header[42..44].try_into().unwrap_or_default()) as u64;
        let phnum = u16::from_le_bytes(header[44..46].try_into().unwrap_or_default()) as usize;
        (phoff, phentsize, phnum)
    }
}

/// Returns `true` if `path` is a little-endian ELF declaring a `PT_INTERP` program header.
pub fn is_dynamically_linked_elf(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut header = [0u8; 64];
    if file.read_exact(&mut header).is_err() || &header[0..4] != b"\x7fELF" {
        return false;
    }

    let is_64 = header[4] == 2;
    let is_little_endian = header[5] == 1;
    if !is_little_endian {
        return false;
    }

    let (phoff, phentsize, phnum) = elf_phdr_layout(&header, is_64);
    if phoff == 0 || phentsize < 4 || phnum == 0 || phnum > 256 {
        return false;
    }

    if file.seek(SeekFrom::Start(phoff)).is_err() {
        return false;
    }

    for _ in 0..phnum {
        let mut entry = vec![0u8; phentsize as usize];
        if file.read_exact(&mut entry).is_err() {
            return false;
        }
        let p_type = u32::from_le_bytes(entry[0..4].try_into().unwrap_or_default());
        if p_type == 3 {
            // PT_INTERP (0x3) found
            return true;
        }
    }

    false
}

/// Checks via `LD_TRACE_LOADED_OBJECTS=1` whether a dynamically linked ELF has missing
/// shared libraries or incompatible `GLIBC_*` symbol versions.
pub fn is_dynamically_broken(path: &Path) -> bool {
    if !is_dynamically_linked_elf(path) {
        return false;
    }

    let mut cmd = std::process::Command::new(path);
    cmd.env("LD_TRACE_LOADED_OBJECTS", "1");
    if let Some(parent) = path.parent() {
        let parent_str = parent.display().to_string();
        let extra = format!(
            "{parent_str}/bin/qt/lib:{parent_str}/qt/lib:{parent_str}:{parent_str}/lib:{parent_str}/lib64:{parent_str}/bin:{parent_str}/lib/x64:{parent_str}/bin/x64"
        );
        if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
            cmd.env("LD_LIBRARY_PATH", format!("{extra}:{existing}"));
        } else {
            cmd.env("LD_LIBRARY_PATH", extra);
        }
    }

    let output = match cmd.output() {
        Ok(out) => out,
        Err(_) => return true,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("not found") || stderr.contains("not found") {
        return true;
    }

    if !output.status.success()
        && (stderr.contains("error while loading shared libraries")
            || stderr.contains("cannot open shared object file")
            || stderr.contains("GLIBC_")
            || stderr.contains("version `"))
    {
        return true;
    }

    false
}

/// Recursively lists up to `max_entries` files and subdirectories for diagnostics.
pub fn list_dir_contents_recursive(dir: &Path, max_entries: usize) -> Vec<String> {
    let mut results = Vec::new();
    fn walk(base: &Path, current: &Path, results: &mut Vec<String>, max_entries: usize) {
        if results.len() >= max_entries {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                if path.is_dir() {
                    results.push(format!("{}/ (directory)", rel));
                    walk(base, &path, results, max_entries);
                } else if let Ok(meta) = path.metadata() {
                    results.push(format!("{} ({} bytes)", rel, meta.len()));
                }
            }
        }
    }
    walk(dir, dir, &mut results, max_entries);
    results
}

fn first_existing_path(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(Path::new)
        .find(|p| p.exists())
        .map(Path::to_path_buf)
}

/// Locates the host CA certificate bundle across common Linux distributions.
pub fn find_system_ca_bundle() -> Option<PathBuf> {
    first_existing_path(&[
        "/etc/ssl/ca-bundle.pem",
        "/etc/ssl/certs/ca-certificates.crt",
        "/var/lib/ca-certificates/ca-bundle.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/pki/tls/cert.pem",
    ])
}

/// Locates the host CA certificate directory across common Linux distributions.
pub fn find_system_ca_dir() -> Option<PathBuf> {
    first_existing_path(&["/etc/ssl/certs", "/etc/pki/tls/certs"])
}

/// Propagates system CA bundle and directory environment variables to `cmd`.
pub async fn inject_ssl_env(cmd: &mut Command) {
    if let Some(ca) = find_system_ca_bundle() {
        cmd.env("SSL_CERT_FILE", &ca);
        cmd.env("CURL_CA_BUNDLE", &ca);
    }
    if let Some(cadir) = find_system_ca_dir() {
        cmd.env("SSL_CERT_DIR", &cadir);
    }
}

/// Propagates display/session environment variables needed by GUI benchmarks.
pub fn inject_graphics_env(cmd: &mut Command) {
    const GRAPHICS_VARS: [&str; 6] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_SESSION_TYPE",
    ];
    for var in &GRAPHICS_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

/// Applies GPU selection environment variables (`NvidiaDgpu`, `Integrated`, or `Auto`),
/// including Snap Vulkan ICD mapping when running inside a Snap sandbox.
/// Discovers Vulkan ICD JSON files matching NVIDIA dGPU (`want_nvidia = true`)
/// or Mesa/AMD/Intel integrated GPUs (`want_nvidia = false`) across standard Linux paths.
fn find_vulkan_icd_files(want_nvidia: bool) -> Option<String> {
    let search_dirs = [
        "/etc/vulkan/icd.d",
        "/usr/share/vulkan/icd.d",
        "/usr/lib/x86_64-linux-gnu/GL/vulkan/icd.d",
        "/usr/lib/GL/vulkan/icd.d",
    ];
    let mut matched: Vec<String> = Vec::new();

    for dir in search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();

        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let is_match = if want_nvidia {
                lower.contains("nvidia")
            } else {
                (lower.contains("radeon") || lower.contains("intel") || lower.contains("amd"))
                    && !lower.contains("lvp")
            };
            if is_match {
                let s = path.display().to_string();
                if !matched.contains(&s) {
                    matched.push(s);
                }
            }
        }
    }

    if matched.is_empty() {
        None
    } else {
        Some(matched.join(":"))
    }
}

/// Configures environment variables (`__NV_PRIME_RENDER_OFFLOAD`, `DRI_PRIME`, `VK_ICD_FILENAMES`,
/// `VK_DRIVER_FILES`) to steer OpenGL and Vulkan applications toward the requested GPU.
pub fn inject_gpu_env(cmd: &mut Command, gpu_mode: GpuMode) {
    match gpu_mode {
        GpuMode::NvidiaDgpu => {
            cmd.env("__NV_PRIME_RENDER_OFFLOAD", "1");
            cmd.env("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
            cmd.env("DRI_PRIME", "1");
            if RuntimeSandbox::detect() == RuntimeSandbox::Snap {
                let icd_path = std::env::var("SNAP_USER_DATA")
                    .unwrap_or_else(|_| "/tmp".to_string())
                    + "/nvidia_icd.json";
                if !Path::new(&icd_path).exists() {
                    let _ = std::fs::write(
                        &icd_path,
                        r#"{"file_format_version":"1.0.0","ICD":{"library_path":"/var/lib/snapd/lib/gl/libGLX_nvidia.so.0","api_version":"1.3.0"}}"#,
                    );
                }
                cmd.env("VK_ICD_FILENAMES", &icd_path);
                cmd.env("VK_DRIVER_FILES", &icd_path);
            } else if let Some(icds) = find_vulkan_icd_files(true) {
                cmd.env("VK_ICD_FILENAMES", &icds);
                cmd.env("VK_DRIVER_FILES", &icds);
            }
        }
        GpuMode::Integrated => {
            cmd.env("__NV_PRIME_RENDER_OFFLOAD", "0");
            cmd.env("__GLX_VENDOR_LIBRARY_NAME", "mesa");
            cmd.env("DRI_PRIME", "0");
            if let Some(icds) = find_vulkan_icd_files(false) {
                cmd.env("VK_ICD_FILENAMES", &icds);
                cmd.env("VK_DRIVER_FILES", &icds);
            }
        }
        GpuMode::Auto => {
            // Keep default environment without overrides
        }
    }
}

/// Builds the runner `LD_LIBRARY_PATH` prepending bundled library subdirectories inside `work_dir`.
pub fn build_runner_ld_library_path(work_dir: &Path) -> String {
    let work_dir_str = work_dir.display().to_string();
    let extra_ld = format!(
        "{0}/bin/qt/lib:{0}/qt/lib:{0}/bin:{0}/lib:{0}/lib64:{0}/bin/x64:{0}/lib/x64:{0}:./bin/qt/lib:./qt/lib:./bin:./lib:./lib64:./bin/x64:./lib/x64:.",
        work_dir_str
    );
    match std::env::var("LD_LIBRARY_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{}:{}", extra_ld, existing),
        _ => extra_ld,
    }
}

/// Injects bundled library search paths, SSL certificates, graphics session variables, and
/// GPU selection variables onto `cmd`.
pub async fn inject_runner_env(cmd: &mut Command, work_dir: &Path, gpu_mode: GpuMode) {
    cmd.env("LD_LIBRARY_PATH", build_runner_ld_library_path(work_dir));
    inject_ssl_env(cmd).await;
    inject_graphics_env(cmd);
    inject_gpu_env(cmd, gpu_mode);
}

/// Recursively ensures executable permissions (`0o755`) on runner scripts and ELF binaries.
pub async fn set_permissions_recursive(dir: &Path, _mode: u32) -> Result<()> {
    ensure_executable_permissions(dir).await
}

/// Walks `dir` and marks scripts, binaries inside `/bin` or `/extracted`, and ELF executables as `0o755`.
pub async fn ensure_executable_permissions(dir: &Path) -> Result<()> {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(());
    }
    use std::os::unix::fs::PermissionsExt;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&current_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };

            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                // Skip files already marked user-executable to avoid scanning thousands of assets repeatedly.
                if (meta.permissions().mode() & 0o100) != 0 {
                    continue;
                }

                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_sh = file_name.ends_with(".sh")
                    || file_name.ends_with(".run")
                    || file_name == "heaven"
                    || file_name == "Superposition"
                    || file_name == "main";
                let path_str = path.to_string_lossy();
                let is_in_bin = path_str.contains("/bin") || path_str.contains("/extracted");

                let needs_chmod = if is_sh || is_in_bin {
                    true
                } else {
                    let skip_magic_check =
                        path.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|ext| {
                                matches!(
                                    ext.to_ascii_lowercase().as_str(),
                                    "txt"
                                        | "md"
                                        | "json"
                                        | "toml"
                                        | "cfg"
                                        | "ini"
                                        | "log"
                                        | "csv"
                                        | "png"
                                        | "jpg"
                                        | "jpeg"
                                        | "webp"
                                        | "dds"
                                        | "tga"
                                        | "wav"
                                        | "ogg"
                                        | "mp3"
                                        | "mp4"
                                        | "avi"
                                        | "html"
                                        | "css"
                                        | "js"
                                        | "xml"
                                        | "yaml"
                                        | "yml"
                                        | "gguf"
                                        | "mesh"
                                        | "anim"
                                        | "node"
                                        | "mat"
                                        | "prop"
                                        | "umount"
                                        | "pak"
                                        | "xz"
                                        | "gz"
                                        | "bz2"
                                        | "zip"
                                        | "7z"
                                        | "tar"
                                )
                            });
                    if skip_magic_check {
                        false
                    } else if let Ok(mut file) = tokio::fs::File::open(&path).await {
                        let mut header = [0u8; 4];
                        file.read_exact(&mut header).await.is_ok() && &header == b"\x7fELF"
                    } else {
                        false
                    }
                };

                if needs_chmod {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = tokio::fs::set_permissions(&path, perms).await;
                }
            }
        }
    }
    Ok(())
}

/// Sends `SIGKILL` to the negative process group ID corresponding to `pid`.
pub fn kill_process_group(pid: u32) {
    if pid > 0 && pid <= i32::MAX as u32 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

async fn first_existing_async<'a>(candidates: &[&'a str]) -> Option<&'a str> {
    for p in candidates {
        if tokio::fs::try_exists(p).await.unwrap_or(false) {
            return Some(*p);
        }
    }
    None
}

async fn ensure_symlink_if_missing(src: &str, dst: &Path) {
    if !tokio::fs::try_exists(dst).await.unwrap_or(false) {
        #[cfg(unix)]
        let _ = tokio::fs::symlink(src, dst).await;
    }
}

/// Creates a `libgthread-2.0.so.0` symlink pointing to `libglib-2.0.so.0` when a legacy
/// benchmark requires `libgthread` on newer distributions where GLib provides the same symbols.
pub async fn ensure_gthread_shim(work_dir: &Path) -> Result<()> {
    const SYSTEM_GTHREAD_PATHS: [&str; 5] = [
        "/lib64/libgthread-2.0.so.0",
        "/usr/lib64/libgthread-2.0.so.0",
        "/usr/lib/x86_64-linux-gnu/libgthread-2.0.so.0",
        "/usr/lib/libgthread-2.0.so.0",
        "/lib/libgthread-2.0.so.0",
    ];
    if first_existing_async(&SYSTEM_GTHREAD_PATHS).await.is_some() {
        return Ok(());
    }

    const GLIB_CANDIDATES: [&str; 5] = [
        "/lib64/libglib-2.0.so.0",
        "/usr/lib64/libglib-2.0.so.0",
        "/usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
        "/usr/lib/libglib-2.0.so.0",
        "/lib/libglib-2.0.so.0",
    ];
    if let Some(glib_path) = first_existing_async(&GLIB_CANDIDATES).await {
        let bin_dir = work_dir.join("bin");
        if tokio::fs::try_exists(&bin_dir).await.unwrap_or(false) {
            ensure_symlink_if_missing(glib_path, &bin_dir.join("libgthread-2.0.so.0")).await;
        }
        ensure_symlink_if_missing(glib_path, &work_dir.join("libgthread-2.0.so.0")).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_runtime_sandbox_priority() {
        assert_eq!(
            RuntimeSandbox::detect_from(false, false),
            RuntimeSandbox::Native
        );
        assert_eq!(
            RuntimeSandbox::detect_from(true, false),
            RuntimeSandbox::Snap
        );
        assert_eq!(
            RuntimeSandbox::detect_from(false, true),
            RuntimeSandbox::Flatpak
        );
        assert_eq!(
            RuntimeSandbox::detect_from(true, true),
            RuntimeSandbox::Snap
        );
    }

    #[test]
    fn ld_library_path_prepends_work_dir_locations() {
        let path = build_runner_ld_library_path(Path::new("/tmp/runner_work"));
        assert!(path.starts_with("/tmp/runner_work/bin/qt/lib:/tmp/runner_work/qt/lib:"));
    }
}
