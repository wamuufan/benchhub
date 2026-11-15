use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub use benchhub_types::{clean_terminal_chunk, TerminalRingBuffer};

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
    let mut ring = TerminalRingBuffer::new(max_lines);
    ring.push_str(&raw);
    let mut result = ring.to_string();
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
