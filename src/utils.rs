use anyhow::{bail, Result};
use regex::Regex;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// Strips ANSI escape sequences (e.g. colors, formatting) from a line of text.
pub fn strip_ansi_codes(input: &str) -> String {
    static ANSI_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = ANSI_REGEX.get_or_init(|| Regex::new(r"\x1B\[[0-9;]*[mK]").unwrap());
    re.replace_all(input, "").to_string()
}

/// Sanitizes a string intended to be an identifier (such as a benchmark ID, version, or category).
/// Rejects path traversal sequences (`..`), path separators (`/`, `\`), null bytes, control characters,
/// whitespace, and characters outside `[a-zA-Z0-9_.-+]`.
pub fn sanitize_identifier(input: &str) -> Result<String> {
    if input.is_empty() {
        bail!("Identifier cannot be empty");
    }

    if input.contains('\0') {
        bail!("Identifier contains forbidden null byte");
    }

    if input.contains('/') || input.contains('\\') {
        bail!(
            "Identifier contains forbidden path separators ('/' or '\\'): {}",
            input
        );
    }

    if input == "." || input == ".." || input.contains("..") {
        bail!(
            "Identifier contains forbidden path traversal sequence: {}",
            input
        );
    }

    for c in input.chars() {
        if c.is_control() {
            bail!("Identifier contains forbidden control character: {:?}", c);
        }
        if !c.is_alphanumeric()
            && c != '_'
            && c != '-'
            && c != '.'
            && c != '+'
            && c != ' '
            && c != '('
            && c != ')'
            && c != '['
            && c != ']'
        {
            bail!("Identifier contains forbidden character '{}': {}", c, input);
        }
    }

    Ok(input.to_string())
}

/// Sanitizes a filename (e.g. for log files, export CSV/PNG files).
/// Rejects path traversal sequences (`..`), path separators (`/`, `\`), null bytes, and control characters.
pub fn sanitize_filename(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Filename cannot be empty");
    }

    if input.contains('\0') {
        bail!("Filename contains forbidden null byte");
    }

    if input.contains('/') || input.contains('\\') {
        bail!(
            "Filename contains forbidden path separators ('/' or '\\'): {}",
            input
        );
    }

    if input == "." || input == ".." || input.contains("..") {
        bail!(
            "Filename contains forbidden path traversal sequence: {}",
            input
        );
    }

    for c in input.chars() {
        if c.is_control() {
            bail!("Filename contains forbidden control character: {:?}", c);
        }
    }

    Ok(input.trim().to_string())
}

/// Normalizes a path by removing redundant `.` and resolving `..` components logically
/// without touching the filesystem.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                } else if !components.is_empty()
                    && matches!(
                        components.last(),
                        Some(Component::RootDir) | Some(Component::Prefix(_))
                    )
                {
                    // Cannot go above root dir
                } else {
                    components.push(comp);
                }
            }
            _ => components.push(comp),
        }
    }
    components.into_iter().collect()
}

/// Verifies that `target` is strictly within `base_dir` (and is not identical to `base_dir`).
/// Returns the normalized/canonical target PathBuf if safe, or an Error if a traversal or out-of-bounds path is detected.
pub fn ensure_path_within(base_dir: &Path, target: &Path) -> Result<PathBuf> {
    let base_norm = if base_dir.is_absolute() {
        normalize_path(base_dir)
    } else {
        let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        normalize_path(&current.join(base_dir))
    };

    let target_norm = if target.is_absolute() {
        normalize_path(target)
    } else {
        normalize_path(&base_norm.join(target))
    };

    let base_resolved = base_norm.canonicalize().unwrap_or(base_norm);

    let mut current = target_norm.clone();
    let mut missing = Vec::new();
    while !current.exists() {
        if let Some(parent) = current.parent() {
            if let Some(file_name) = current.file_name() {
                missing.push(file_name.to_os_string());
                current = parent.to_path_buf();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let mut target_resolved = current.canonicalize().unwrap_or(current);
    for comp in missing.into_iter().rev() {
        target_resolved.push(comp);
    }

    if !target_resolved.starts_with(&base_resolved) {
        bail!(
            "Path traversal violation: Target path '{:?}' is outside base directory '{:?}'",
            target,
            base_dir
        );
    }

    if target_resolved == base_resolved {
        bail!(
            "Target path cannot be identical to base directory: {:?}",
            base_dir
        );
    }

    Ok(target_resolved)
}

/// Helper returning bool whether `target` is safely within `base_dir`.
pub fn is_safe_path(base_dir: &Path, target: &Path) -> bool {
    ensure_path_within(base_dir, target).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_identifier_valid() {
        assert_eq!(sanitize_identifier("geekbench6").unwrap(), "geekbench6");
        assert_eq!(
            sanitize_identifier("unigine_heaven").unwrap(),
            "unigine_heaven"
        );
        assert_eq!(
            sanitize_identifier("v1.2.3-beta_1+opt").unwrap(),
            "v1.2.3-beta_1+opt"
        );
        assert_eq!(sanitize_identifier("7zip").unwrap(), "7zip");
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
    fn test_sanitize_identifier_rejects_traversal() {
        assert!(sanitize_identifier("../etc").is_err());
        assert!(sanitize_identifier("../../root").is_err());
        assert!(sanitize_identifier("foo/../bar").is_err());
        assert!(sanitize_identifier("..").is_err());
        assert!(sanitize_identifier(".").is_err());
    }

    #[test]
    fn test_sanitize_identifier_rejects_slashes_and_null() {
        assert!(sanitize_identifier("/bin/sh").is_err());
        assert!(sanitize_identifier("foo/bar").is_err());
        assert!(sanitize_identifier("foo\\bar").is_err());
        assert!(sanitize_identifier("foo\0bar").is_err());
    }

    #[test]
    fn test_sanitize_identifier_rejects_dangerous_chars() {
        assert!(sanitize_identifier("foo; rm -rf /").is_err());
        assert!(sanitize_identifier("foo && echo 1").is_err());
        assert!(sanitize_identifier("`calc`").is_err());
        assert!(sanitize_identifier("$HOME").is_err());
        assert!(sanitize_identifier("foo|bar").is_err());
        assert!(sanitize_identifier("foo\nbar").is_err());
    }

    #[test]
    fn test_sanitize_filename_valid_and_invalid() {
        assert_eq!(
            sanitize_filename("benchhub_history.csv").unwrap(),
            "benchhub_history.csv"
        );
        assert_eq!(sanitize_filename("run_123.log").unwrap(), "run_123.log");

        assert!(sanitize_filename("../log.txt").is_err());
        assert!(sanitize_filename("/etc/passwd").is_err());
        assert!(sanitize_filename("log\0.txt").is_err());
        assert!(sanitize_filename("").is_err());
    }

    #[test]
    fn test_strip_ansi_codes() {
        let colored = "\x1B[31mRed Text\x1B[0m \x1B[1mBold\x1B[m";
        assert_eq!(strip_ansi_codes(colored), "Red Text Bold");
    }

    #[test]
    fn test_ensure_path_within_valid() {
        let base = Path::new("/tmp/benchhub/runners");
        let target = Path::new("/tmp/benchhub/runners/geekbench6/1.0");
        let resolved = ensure_path_within(base, target).unwrap();
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/benchhub/runners/geekbench6/1.0")
        );
    }

    #[test]
    fn test_ensure_path_within_rejects_traversal() {
        let base = Path::new("/tmp/benchhub/runners");
        let target = Path::new("/tmp/benchhub/runners/../../etc/passwd");
        assert!(ensure_path_within(base, target).is_err());

        let target_same = Path::new("/tmp/benchhub/runners");
        assert!(ensure_path_within(base, target_same).is_err());
    }
}
