use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::ui;

pub fn run(raw: bool, follow: Option<&str>) -> Result<()> {
    let path = match follow {
        Some(p) => PathBuf::from(p),
        None => default_debug_log()?,
    };

    eprintln!(
        "Watching {} (Ctrl-C to exit)",
        ui::timestamp(&path.display().to_string())
    );
    if !path.exists() {
        eprintln!(
            "{}  {}",
            ui::timestamp("waiting for"),
            ui::timestamp("claude --debug-file <path>"),
        );
    }

    let mut pos: u64 = file_len(&path);
    // Accumulate raw bytes; split on '\n' at the byte level so multi-byte
    // UTF-8 codepoints that straddle a read boundary stay intact.
    let mut leftover: Vec<u8> = Vec::new();

    loop {
        let len = file_len(&path);
        if len < pos {
            eprintln!(
                "\n{}  {}",
                ui::banner("─── new session"),
                ui::timestamp(&path.display().to_string())
            );
            pos = 0;
            leftover.clear();
        }
        if len > pos
            && let Ok(mut f) = fs::File::open(&path)
            && f.seek(SeekFrom::Start(pos)).is_ok()
        {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                pos += buf.len() as u64;
                leftover.extend_from_slice(&buf);
                for line in lines_from_leftover(&mut leftover, raw) {
                    println!("{line}");
                }
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}

fn default_debug_log() -> Result<PathBuf> {
    let dir = crate::claudex_dir()?.join("debug");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("latest.log"))
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Extract every complete (`\n`-terminated) line from `buf` and return them
/// formatted for display. Any trailing partial line stays in `buf` for the
/// next poll. Operates on raw bytes so multi-byte UTF-8 codepoints that
/// straddle chunk boundaries survive intact.
fn lines_from_leftover(buf: &mut Vec<u8>, raw: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..buf.len() {
        if buf[i] == b'\n' {
            let line = String::from_utf8_lossy(&buf[start..i]);
            if !line.trim().is_empty() {
                out.push(if raw {
                    line.into_owned()
                } else {
                    format_line(&line)
                });
            }
            start = i + 1;
        }
    }
    if start > 0 {
        buf.drain(..start);
    }
    out
}

fn format_line(line: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(line) {
        format_json_line(&v, line)
    } else {
        ui::classify_text_line(line)
    }
}

fn format_json_line(v: &Value, raw_line: &str) -> String {
    let ts = v["timestamp"]
        .as_str()
        .or_else(|| v["ts"].as_str())
        .unwrap_or("");
    let ts_short = shorten_ts(ts);

    // Session JSONL record style (has a "type" field)
    if let Some(record_type) = v["type"].as_str() {
        if record_type == "system" {
            let dur = v["durationMs"].as_u64().unwrap_or(0);
            let suffix = if dur > 0 {
                format!(" {dur}ms")
            } else {
                String::new()
            };
            return format!(
                "{} [{}]{}",
                ui::timestamp(ts_short),
                ui::level_debug("system"),
                ui::level_debug(&suffix)
            );
        }
        return format!(
            "{} [{}]",
            ui::timestamp(ts_short),
            ui::record_type(record_type)
        );
    }

    // Structured log style (level + message)
    let level = v["level"]
        .as_str()
        .or_else(|| v["severity"].as_str())
        .unwrap_or("info");
    let msg = v["message"]
        .as_str()
        .or_else(|| v["msg"].as_str())
        .unwrap_or(raw_line);

    let (level_s, msg_s) = match level.to_lowercase().as_str() {
        "error" | "fatal" | "critical" => (ui::level_error(level), ui::level_error(msg)),
        "warn" | "warning" => (ui::level_warn(level), ui::level_warn(msg)),
        "debug" | "trace" => (ui::level_debug(level), ui::level_debug(msg)),
        _ => (level.to_string(), msg.to_string()),
    };

    if ts_short.is_empty() {
        format!("[{level_s}] {msg_s}")
    } else {
        format!("{} [{}] {}", ui::timestamp(ts_short), level_s, msg_s)
    }
}

fn shorten_ts(ts: &str) -> &str {
    // Extract HH:MM:SS from "YYYY-MM-DDTHH:MM:SS..." or "YYYY-MM-DD HH:MM:SS..."
    if ts.len() >= 19 && ts.is_char_boundary(11) && ts.is_char_boundary(19) {
        &ts[11..19]
    } else if ts.len() >= 8 && ts.is_char_boundary(8) {
        &ts[..8]
    } else {
        ts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn shorten_ts_iso_with_t_separator() {
        assert_eq!(shorten_ts("2026-04-18T12:34:56.789Z"), "12:34:56");
    }

    #[test]
    fn shorten_ts_iso_with_space_separator() {
        assert_eq!(shorten_ts("2026-04-18 12:34:56.789"), "12:34:56");
    }

    #[test]
    fn shorten_ts_short_input_returned_as_is() {
        assert_eq!(shorten_ts("12:34:56"), "12:34:56");
        assert_eq!(shorten_ts(""), "");
        assert_eq!(shorten_ts("abc"), "abc");
    }

    #[test]
    fn format_line_handles_structured_json_log() {
        let line = r#"{"timestamp":"2026-04-18T12:34:56.000Z","level":"error","message":"boom"}"#;
        let out = strip_ansi(&format_line(line));
        assert!(out.contains("12:34:56"), "got: {out}");
        assert!(out.contains("error"), "got: {out}");
        assert!(out.contains("boom"), "got: {out}");
    }

    #[test]
    fn format_line_handles_alt_field_names() {
        let line = r#"{"ts":"2026-04-18T09:00:00.000Z","severity":"warn","msg":"slow"}"#;
        let out = strip_ansi(&format_line(line));
        assert!(out.contains("09:00:00"));
        assert!(out.contains("warn"));
        assert!(out.contains("slow"));
    }

    #[test]
    fn format_line_missing_timestamp_omits_ts_prefix() {
        let line = r#"{"level":"info","message":"hello"}"#;
        let out = strip_ansi(&format_line(line));
        assert!(out.starts_with("[info] hello"), "got: {out}");
    }

    #[test]
    fn format_line_session_record_types() {
        for (record_type, ts) in [
            ("user", "2026-04-18T12:00:00.000Z"),
            ("assistant", "2026-04-18T12:00:01.000Z"),
            ("other", "2026-04-18T12:00:02.000Z"),
        ] {
            let line = format!(r#"{{"type":"{record_type}","timestamp":"{ts}"}}"#);
            let out = strip_ansi(&format_line(&line));
            assert!(out.contains(record_type), "type={record_type} got: {out}");
            assert!(out.contains(&ts[11..19]), "type={record_type} got: {out}");
        }
    }

    #[test]
    fn format_line_system_record_includes_duration() {
        let line = r#"{"type":"system","timestamp":"2026-04-18T12:00:00.000Z","durationMs":250}"#;
        let out = strip_ansi(&format_line(line));
        assert!(out.contains("system"));
        assert!(out.contains("250ms"), "got: {out}");
    }

    #[test]
    fn format_line_system_record_omits_zero_duration() {
        let line = r#"{"type":"system","timestamp":"2026-04-18T12:00:00.000Z"}"#;
        let out = strip_ansi(&format_line(line));
        assert!(out.contains("system"));
        assert!(!out.contains("ms"), "got: {out}");
    }

    #[test]
    fn format_line_falls_back_to_classify_for_non_json() {
        assert_eq!(
            strip_ansi(&format_line("plain log line")),
            strip_ansi(&ui::classify_text_line("plain log line")),
        );
    }

    #[test]
    fn lines_from_leftover_splits_complete_lines() {
        let mut buf = b"a\nb\nc\n".to_vec();
        let lines = lines_from_leftover(&mut buf, true);
        assert_eq!(lines, vec!["a", "b", "c"]);
        assert!(buf.is_empty());
    }

    #[test]
    fn lines_from_leftover_buffers_partial_line() {
        let mut buf = b"hello wor".to_vec();
        let first = lines_from_leftover(&mut buf, true);
        assert!(first.is_empty());
        assert_eq!(buf, b"hello wor");

        buf.extend_from_slice(b"ld\nnext\n");
        let second = lines_from_leftover(&mut buf, true);
        assert_eq!(second, vec!["hello world", "next"]);
        assert!(buf.is_empty());
    }

    #[test]
    fn lines_from_leftover_preserves_utf8_across_chunks() {
        // "é" is 0xC3 0xA9. If the reader hands us the first byte alone, we
        // must not insert a replacement character — the codepoint needs to
        // reassemble after the next chunk arrives.
        let mut buf = vec![b'a', 0xC3];
        let first = lines_from_leftover(&mut buf, true);
        assert!(first.is_empty());
        buf.push(0xA9);
        buf.push(b'\n');
        let second = lines_from_leftover(&mut buf, true);
        assert_eq!(second, vec!["aé"]);
    }

    #[test]
    fn lines_from_leftover_skips_blank_lines() {
        let mut buf = b"a\n\n   \nb\n".to_vec();
        let lines = lines_from_leftover(&mut buf, true);
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn lines_from_leftover_raw_vs_formatted() {
        let json = br#"{"level":"error","message":"boom"}
"#;
        let mut buf1 = json.to_vec();
        let raw = lines_from_leftover(&mut buf1, true);
        let mut buf2 = json.to_vec();
        let formatted = lines_from_leftover(&mut buf2, false);

        assert_eq!(raw.len(), 1);
        assert_eq!(formatted.len(), 1);
        assert!(raw[0].contains("{\"level\""));
        assert!(!raw[0].contains('\x1b'));
        assert!(
            strip_ansi(&formatted[0]).contains("boom"),
            "got: {}",
            formatted[0]
        );
    }

    #[test]
    fn lines_from_leftover_trailing_newline() {
        let mut buf = b"a\n".to_vec();
        lines_from_leftover(&mut buf, true);
        assert!(buf.is_empty());

        buf.extend_from_slice(b"b");
        lines_from_leftover(&mut buf, true);
        assert_eq!(buf, b"b");

        buf.extend_from_slice(b"\n");
        let lines = lines_from_leftover(&mut buf, true);
        assert_eq!(lines, vec!["b"]);
        assert!(buf.is_empty());
    }

    #[test]
    fn file_len_missing_file_returns_zero() {
        let p = Path::new("/definitely/not/a/real/path/claudex-watch-test-xyz");
        assert_eq!(file_len(p), 0);
    }

    #[test]
    fn file_len_reports_size() {
        use std::io::Write;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut f = std::fs::File::create(tmp.path()).unwrap();
        f.write_all(b"hello").unwrap();
        assert_eq!(file_len(tmp.path()), 5);
    }

    #[test]
    fn default_debug_log_creates_dir_and_returns_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let _guard = HomeGuard::set(tmp.path());
        let path = default_debug_log().unwrap();
        assert_eq!(path, tmp.path().join(".claudex/debug/latest.log"));
        assert!(path.parent().unwrap().is_dir());
    }

    struct HomeGuard {
        prev: Option<std::ffi::OsString>,
    }
    impl HomeGuard {
        fn set(path: &Path) -> Self {
            let prev = std::env::var_os("HOME");
            // SAFETY: env mutation is not thread-safe; callers ensure
            // this is used from a single test thread.
            unsafe { std::env::set_var("HOME", path) };
            Self { prev }
        }
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            // SAFETY: see `set` above.
            unsafe {
                match self.prev.take() {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }
}
