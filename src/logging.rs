use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A high-performance, line-based fixed-capacity ring buffer for in-memory terminal lines.
/// Avoids repeated full-string allocations and drains by storing individual lines in a VecDeque.
#[derive(Debug, Clone)]
pub struct TerminalRingBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
    has_partial_line: bool,
}

impl TerminalRingBuffer {
    pub fn new(max_lines: usize) -> Self {
        let max_lines = max_lines.max(1);
        Self {
            lines: VecDeque::with_capacity(max_lines + 1),
            max_lines,
            has_partial_line: false,
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
    }

    /// Appends incoming text (which may contain multiple lines or partial lines) to the ring buffer.
    pub fn push_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let cleaned = clean_terminal_chunk(text);
        if cleaned.is_empty() {
            return;
        }

        let mut start = 0;
        let bytes = cleaned.as_bytes();

        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                let piece = &cleaned[start..=i];
                start = i + 1;

                if self.has_partial_line {
                    if let Some(last) = self.lines.back_mut() {
                        last.push_str(piece);
                    }
                    self.has_partial_line = false;
                } else {
                    self.lines.push_back(piece.to_string());
                }
                self.trim();
            }
        }

        // Remaining text after last newline is a partial line
        if start < cleaned.len() {
            let remainder = &cleaned[start..];
            if self.has_partial_line {
                if let Some(last) = self.lines.back_mut() {
                    last.push_str(remainder);
                }
            } else {
                self.lines.push_back(remainder.to_string());
                self.has_partial_line = true;
                self.trim();
            }
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

/// Reads the last `max_lines` from the given file without loading excessive data into memory.
pub fn read_last_lines(path: &Path, max_lines: usize) -> std::io::Result<String> {
    if !path.exists() || max_lines == 0 {
        return Ok(String::new());
    }

    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_len = metadata.len();

    if file_len == 0 {
        return Ok(String::new());
    }

    let chunk_size: u64 = 64 * 1024;
    let max_read: u64 = 512 * 1024;
    let mut pos = file_len;
    let mut chunks = Vec::new();
    let mut newlines = 0;
    let mut total_read = 0;

    while pos > 0 && newlines <= max_lines && total_read < max_read {
        let read_size = std::cmp::min(chunk_size, pos);
        pos -= read_size;
        file.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)?;

        for &b in chunk.iter().rev() {
            if b == b'\n' {
                newlines += 1;
            }
        }
        chunks.push(chunk);
        total_read += read_size;
    }

    let mut combined = Vec::with_capacity(total_read as usize);
    for chunk in chunks.into_iter().rev() {
        combined.extend_from_slice(&chunk);
    }

    let raw = String::from_utf8_lossy(&combined).into_owned();
    let mut result = clean_terminal_chunk(&raw);
    trim_terminal_buffer(&mut result, max_lines, max_read as usize);
    Ok(result)
}

/// Trims an in-memory buffer to retain only the most recent lines and maximum byte limit.
/// Guarantees UTF-8 validity and avoids unbounded UI text allocations.
pub fn trim_terminal_buffer(buf: &mut String, max_lines: usize, max_bytes: usize) {
    if buf.is_empty() {
        return;
    }

    let bytes = buf.as_bytes();
    let mut newlines = 0;
    let mut keep_start = 0;

    for (i, &b) in bytes.iter().enumerate().rev() {
        if b == b'\n' {
            newlines += 1;
            if newlines > max_lines {
                keep_start = i + 1;
                break;
            }
        }
    }

    if buf.len() - keep_start > max_bytes {
        keep_start = buf.len() - max_bytes;
        while keep_start < buf.len() && !buf.is_char_boundary(keep_start) {
            keep_start += 1;
        }
        if let Some(next_newline) = buf[keep_start..].find('\n') {
            keep_start += next_newline + 1;
        }
    }

    if keep_start > 0 && keep_start < buf.len() {
        buf.drain(..keep_start);
    } else if keep_start >= buf.len() {
        buf.clear();
    }
}

use tokio::sync::mpsc;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
pub struct AppMakeWriter {
    tx: mpsc::Sender<String>,
}

impl std::io::Write for AppMakeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf).into_owned();
        let _ = self.tx.try_send(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for AppMakeWriter {
    type Writer = AppMakeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Sanitizes incoming console/terminal chunks:
/// - Replaces tabs (\t) with 4 spaces to prevent broken `][` missing-glyph boxes in Slint TextEdit.
/// - Normalizes carriage returns (\r\n and \r to \n).
/// - Strips ANSI color and cursor control codes.
pub fn clean_terminal_chunk(text: &str) -> String {
    let without_tabs = text.replace('\t', "    ");
    let without_cr = without_tabs.replace("\r\n", "\n").replace('\r', "\n");
    crate::engine::strip_ansi_codes(&without_cr)
}

pub fn init_tracing(terminal_tx: mpsc::Sender<String>) {
    let default_filter = "benchhub=info,winit=warn,sctk=warn,calloop=warn,i_slint_backend_winit=warn,i_slint_core=warn,warn";
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let writer = AppMakeWriter { tx: terminal_tx };

    let fmt_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let mut buf = TerminalRingBuffer::new(3);
        buf.push_str("line 1\n");
        buf.push_str("line 2\n");
        buf.push_str("line 3\n");
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.to_string(), "line 1\nline 2\nline 3\n");

        buf.push_str("line 4\n");
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.to_string(), "line 2\nline 3\nline 4\n");
    }

    #[test]
    fn test_ring_buffer_partial_lines() {
        let mut buf = TerminalRingBuffer::new(3);
        buf.push_str("part1 ");
        buf.push_str("part2\n");
        assert_eq!(buf.to_string(), "part1 part2\n");

        buf.push_str("another partial");
        assert_eq!(buf.to_string(), "part1 part2\nanother partial");

        buf.push_str(" completed\n");
        assert_eq!(buf.to_string(), "part1 part2\nanother partial completed\n");
    }

    #[test]
    fn test_ring_buffer_resize() {
        let mut buf = TerminalRingBuffer::new(5);
        for i in 1..=5 {
            buf.push_str(&format!("line {}\n", i));
        }
        assert_eq!(buf.len(), 5);

        buf.set_max_lines(2);
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.to_string(), "line 4\nline 5\n");
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut buf = TerminalRingBuffer::new(5);
        buf.push_str("some lines\nhere\n");
        assert!(!buf.is_empty());
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.to_string(), "");
    }
}
