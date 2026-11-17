//! Terminal Output Sanitization & In-Place Ring Buffer
//!
//! Shared across both the daemon (`benchhubd`) and WASM GUI (`benchhub-gui`)
//! to normalize ANSI codes, handle `\r` progress bar overwrites in place,
//! keep log lines bounded, and redact overly verbose signed URL query strings.

use std::collections::VecDeque;

/// Strips ANSI CSI (`\x1B[...`) and OSC (`\x1B]...`) terminal escape sequences.
pub fn strip_ansi_sequences(input: &str) -> String {
    if !input.contains('\x1B') {
        return input.to_string();
    }

    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut last = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == 0x1B && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'[' => {
                    // CSI sequence: parameter/intermediate bytes in 0x20..=0x3F, final byte in 0x40..=0x7E
                    let mut j = i + 2;
                    while j < bytes.len() && (0x20..=0x3F).contains(&bytes[j]) {
                        j += 1;
                    }
                    if j < bytes.len() && (0x40..=0x7E).contains(&bytes[j]) {
                        out.push_str(&input[last..i]);
                        i = j + 1;
                        last = i;
                        continue;
                    }
                }
                b']' => {
                    // OSC sequence: terminated by BEL (\x07) or ST (\x1B\)
                    let mut j = i + 2;
                    let mut end = None;
                    while j < bytes.len() {
                        if bytes[j] == 0x07 {
                            end = Some(j + 1);
                            break;
                        }
                        if bytes[j] == 0x1B && j + 1 < bytes.len() && bytes[j + 1] == b'\\' {
                            end = Some(j + 2);
                            break;
                        }
                        if bytes[j] >= 0x80 {
                            break;
                        }
                        j += 1;
                    }
                    if let Some(next_idx) = end {
                        out.push_str(&input[last..i]);
                        i = next_idx;
                        last = i;
                        continue;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    out.push_str(&input[last..]);
    out
}

/// Shortens overly long HTTP(S) query strings (such as AWS/CloudFront pre-signed redirect URLs)
/// to avoid flooding the live terminal box with 8-line credential tokens.
pub fn shorten_long_urls(input: &str) -> String {
    if !input.contains("http://") && !input.contains("https://") {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;

    while cursor < input.len() {
        let rest = &input[cursor..];
        let next_http = rest.find("http://");
        let next_https = rest.find("https://");
        let next_offset = match (next_http, next_https) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => {
                out.push_str(rest);
                break;
            }
        };

        out.push_str(&rest[..next_offset]);
        let url_start = cursor + next_offset;
        let after_scheme = &input[url_start..];
        let url_end_rel = after_scheme
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '>' | '<' | ')'))
            .unwrap_or(after_scheme.len());
        let url = &after_scheme[..url_end_rel];

        if let Some(q_idx) = url.find('?') {
            let query = &url[q_idx + 1..];
            if query.len() > 48 {
                out.push_str(&url[..=q_idx]);
                out.push_str("...");
            } else {
                out.push_str(url);
            }
        } else {
            out.push_str(url);
        }

        cursor = url_start + url_end_rel;
    }

    out
}

/// Sanitizes incoming terminal chunks while preserving standalone `\r` so progress bars overwrite cleanly:
/// - Replaces tabs (`\t`) with 4 spaces.
/// - Normalizes `\r\n` to `\n`.
/// - Strips ANSI color and cursor escape sequences.
/// - Shortens oversized signed URL query parameters.
pub fn clean_terminal_chunk(text: &str) -> String {
    let without_tabs = text.replace('\t', "    ");
    let normalized_crlf = without_tabs.replace("\r\n", "\n");
    let stripped = strip_ansi_sequences(&normalized_crlf);
    shorten_long_urls(&stripped)
}

/// A line-based fixed-capacity ring buffer with real terminal `\r` overwrite semantics.
///
/// Finalized lines end with `\n`. Standalone `\r` returns the cursor to the start of the
/// current uncommitted line so download/extraction progress indicators update smoothly in place.
#[derive(Debug, Clone)]
pub struct TerminalRingBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
    has_partial_line: bool,
    cursor_at_line_start: bool,
}

impl TerminalRingBuffer {
    pub fn new(max_lines: usize) -> Self {
        let max_lines = max_lines.max(1);
        Self {
            lines: VecDeque::with_capacity(max_lines + 1),
            max_lines,
            has_partial_line: false,
            cursor_at_line_start: false,
        }
    }

    pub fn set_max_lines(&mut self, max_lines: usize) {
        self.max_lines = max_lines.max(1);
        self.trim();
    }

    pub fn max_lines(&self) -> usize {
        self.max_lines
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.has_partial_line = false;
        self.cursor_at_line_start = false;
    }

    /// Returns a cloned vector of stored lines (preserving `\n` on finalized lines).
    pub fn lines_vec(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }

    /// Replaces the entire buffer content cleanly from persisted/snapshot lines.
    pub fn replace_from_snapshot_lines(&mut self, snapshot_lines: &[String]) {
        self.clear();
        if snapshot_lines.is_empty() {
            return;
        }
        for (idx, raw) in snapshot_lines.iter().enumerate() {
            if raw.ends_with('\n') {
                self.push_str(raw);
            } else if idx + 1 < snapshot_lines.len() {
                self.push_str(&format!("{}\n", raw));
            } else {
                self.push_str(raw);
            }
        }
    }

    /// Appends incoming text (supporting multi-line chunks, partial lines, and `\r` overwrites).
    pub fn push_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let cleaned = clean_terminal_chunk(text);
        if cleaned.is_empty() {
            return;
        }

        let bytes = cleaned.as_bytes();
        let mut seg_start = 0usize;

        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\r' {
                if i > seg_start {
                    self.write_segment(&cleaned[seg_start..i]);
                }
                if self.has_partial_line {
                    self.cursor_at_line_start = true;
                }
                seg_start = i + 1;
            } else if b == b'\n' {
                if i > seg_start {
                    self.write_segment(&cleaned[seg_start..i]);
                }
                self.finalize_newline();
                seg_start = i + 1;
            }
        }

        if seg_start < cleaned.len() {
            self.write_segment(&cleaned[seg_start..]);
        }
    }

    fn write_segment(&mut self, seg: &str) {
        const MAX_LINE_BYTES: usize = 16 * 1024;

        if seg.is_empty() {
            return;
        }
        if self.has_partial_line {
            if let Some(last) = self.lines.back_mut() {
                if self.cursor_at_line_start {
                    last.clear();
                    self.cursor_at_line_start = false;
                }
                let remaining = MAX_LINE_BYTES.saturating_sub(last.len());
                if remaining > 0 {
                    if seg.len() <= remaining {
                        last.push_str(seg);
                    } else {
                        let cut = floor_char_boundary(seg, remaining);
                        last.push_str(&seg[..cut]);
                    }
                }
            }
        } else {
            let bounded = if seg.len() <= MAX_LINE_BYTES {
                seg
            } else {
                &seg[..floor_char_boundary(seg, MAX_LINE_BYTES)]
            };
            self.lines.push_back(bounded.to_string());
            self.has_partial_line = true;
            self.cursor_at_line_start = false;
            self.trim();
        }
    }

    fn finalize_newline(&mut self) {
        if self.has_partial_line {
            if let Some(last) = self.lines.back_mut() {
                last.push('\n');
            }
            self.has_partial_line = false;
            self.cursor_at_line_start = false;
        } else {
            self.lines.push_back("\n".to_string());
            self.trim();
        }
    }

    fn trim(&mut self) {
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }
}

impl std::fmt::Display for TerminalRingBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for line in &self.lines {
            f.write_str(line)?;
        }
        Ok(())
    }
}

fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carriage_return_overwrites_in_place() {
        let mut buf = TerminalRingBuffer::new(10);
        buf.push_str("[BenchHub] Downloading...\n");
        buf.push_str("  10% [=>   ]\r");
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.to_string(), "[BenchHub] Downloading...\n  10% [=>   ]");

        buf.push_str("  60% [===> ]\r");
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.to_string(), "[BenchHub] Downloading...\n  60% [===> ]");

        buf.push_str(" 100% [====>]\n");
        assert_eq!(buf.len(), 2);
        assert_eq!(
            buf.to_string(),
            "[BenchHub] Downloading...\n 100% [====>]\n"
        );
    }

    #[test]
    fn test_leading_cr_and_crlf_do_not_erase_finalized_lines() {
        let mut buf = TerminalRingBuffer::new(10);
        buf.push_str("done\r\n");
        buf.push_str("\rnext\rupdated");
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.to_string(), "done\nupdated");
    }

    #[test]
    fn test_strip_ansi_and_shorten_long_urls() {
        let raw = "\x1B[32m2026-11-13 URL:https://cdn.example.com/archive.tar.gz?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=VERY_LONG_CREDENTIAL_STRING_123456 [1024/1024]\x1B[0m\n";
        let cleaned = clean_terminal_chunk(raw);
        assert_eq!(
            cleaned,
            "2026-11-13 URL:https://cdn.example.com/archive.tar.gz?... [1024/1024]\n"
        );
    }

    #[test]
    fn test_snapshot_restore_preserves_newlines() {
        let mut buf = TerminalRingBuffer::new(10);
        buf.replace_from_snapshot_lines(&[
            "line one".to_string(),
            "line two".to_string(),
            "line three\n".to_string(),
        ]);
        assert_eq!(buf.to_string(), "line one\nline two\nline three\n");
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_line_byte_ceiling_preserves_utf8_boundaries() {
        let mut buf = TerminalRingBuffer::new(5);
        // 4-byte emoji repeated 5000 times = 20,000 bytes without newlines
        let long_chunk = "🚀".repeat(5000);
        buf.push_str(&long_chunk);
        assert_eq!(buf.len(), 1);
        let stored = buf.lines_vec().pop().unwrap();
        assert!(stored.len() <= 16 * 1024);
        assert_eq!(stored.len() % 4, 0);

        // Appending more to the partial line does not exceed the ceiling
        buf.push_str("more");
        let stored_after = buf.lines_vec().pop().unwrap();
        assert!(stored_after.len() <= 16 * 1024);
    }
}
