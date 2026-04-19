use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(raw: bool, follow: Option<&str>) -> Result<()> {
    let path = match follow {
        Some(p) => PathBuf::from(p),
        None => default_debug_log()?,
    };

    eprintln!(
        "Watching {} (Ctrl-C to exit)",
        path.display().to_string().dimmed()
    );
    if !path.exists() {
        eprintln!(
            "{}  {}",
            "waiting for".dimmed(),
            "claude --debug-file <path>".dimmed()
        );
    }

    let mut pos: u64 = file_len(&path);
    let mut leftover = String::new();

    loop {
        let len = file_len(&path);
        if len < pos {
            eprintln!(
                "\n{}  {}",
                "─── new session".bright_yellow(),
                path.display().to_string().dimmed()
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
                for line in lines_from_chunk(&buf, &mut leftover, raw) {
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

fn lines_from_chunk(buf: &[u8], leftover: &mut String, raw: bool) -> Vec<String> {
    let chunk = String::from_utf8_lossy(buf);
    let combined = format!("{leftover}{chunk}");
    let ends_with_newline = combined.ends_with('\n');
    let mut parts: Vec<&str> = combined.split('\n').collect();

    if ends_with_newline {
        leftover.clear();
        if parts.last() == Some(&"") {
            parts.pop();
        }
    } else {
        *leftover = parts.pop().unwrap_or("").to_string();
    }

    parts
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            if raw {
                line.to_string()
            } else {
                format_line(line)
            }
        })
        .collect()
}

fn format_line(line: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(line) {
        format_json_line(&v, line)
    } else {
        format_text_line(line)
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
        let type_colored = match record_type {
            "user" => record_type.bright_green().bold().to_string(),
            "assistant" => record_type.bright_blue().bold().to_string(),
            "system" => {
                let dur = v["durationMs"].as_u64().unwrap_or(0);
                let suffix = if dur > 0 {
                    format!(" {dur}ms")
                } else {
                    String::new()
                };
                return format!(
                    "{} [{}]{}",
                    ts_short.dimmed(),
                    "system".dimmed(),
                    suffix.dimmed()
                );
            }
            _ => record_type.bright_yellow().to_string(),
        };
        return format!("{} [{}]", ts_short.dimmed(), type_colored);
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
        "error" | "fatal" | "critical" => (level.red().bold().to_string(), msg.red().to_string()),
        "warn" | "warning" => (level.yellow().to_string(), msg.yellow().to_string()),
        "debug" | "trace" => (level.dimmed().to_string(), msg.dimmed().to_string()),
        _ => (level.to_string(), msg.to_string()),
    };

    if ts_short.is_empty() {
        format!("[{level_s}] {msg_s}")
    } else {
        format!("{} [{}] {}", ts_short.dimmed(), level_s, msg_s)
    }
}

fn format_text_line(line: &str) -> String {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fatal") {
        line.red().to_string()
    } else if lower.contains("warn") {
        line.yellow().to_string()
    } else if lower.contains("tool_call") || lower.contains("tool_use") {
        line.cyan().to_string()
    } else if lower.contains("debug") || lower.contains("trace") {
        line.dimmed().to_string()
    } else {
        line.to_string()
    }
}

fn shorten_ts(ts: &str) -> &str {
    // Extract HH:MM:SS from "YYYY-MM-DDTHH:MM:SS..." or "YYYY-MM-DD HH:MM:SS..."
    if ts.len() >= 19 {
        &ts[11..19]
    } else if ts.len() >= 8 {
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
    fn format_text_line_classifies_keywords() {
        assert!(strip_ansi(&format_text_line("ERROR: boom")).contains("ERROR"));
        assert_ne!(format_text_line("ERROR: boom"), "ERROR: boom");
        assert_ne!(format_text_line("warn me"), "warn me");
        assert_ne!(format_text_line("tool_use: Read"), "tool_use: Read");
        assert_ne!(format_text_line("[DEBUG] x"), "[DEBUG] x");
        assert_eq!(format_text_line("plain text"), "plain text");
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
    fn format_line_falls_back_to_text_for_non_json() {
        assert_eq!(
            strip_ansi(&format_line("plain log line")),
            strip_ansi(&format_text_line("plain log line")),
        );
    }

    #[test]
    fn lines_from_chunk_splits_complete_lines() {
        let mut leftover = String::new();
        let lines = lines_from_chunk(b"a\nb\nc\n", &mut leftover, true);
        assert_eq!(lines, vec!["a", "b", "c"]);
        assert_eq!(leftover, "");
    }

    #[test]
    fn lines_from_chunk_buffers_partial_line() {
        let mut leftover = String::new();
        let first = lines_from_chunk(b"hello wor", &mut leftover, true);
        assert!(first.is_empty());
        assert_eq!(leftover, "hello wor");

        let second = lines_from_chunk(b"ld\nnext\n", &mut leftover, true);
        assert_eq!(second, vec!["hello world", "next"]);
        assert_eq!(leftover, "");
    }

    #[test]
    fn lines_from_chunk_skips_blank_lines() {
        let mut leftover = String::new();
        let lines = lines_from_chunk(b"a\n\n   \nb\n", &mut leftover, true);
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn lines_from_chunk_raw_vs_formatted() {
        let json = br#"{"level":"error","message":"boom"}
"#;
        let mut lo1 = String::new();
        let raw = lines_from_chunk(json, &mut lo1, true);
        let mut lo2 = String::new();
        let formatted = lines_from_chunk(json, &mut lo2, false);

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
    fn lines_from_chunk_handles_trailing_newline_correctly() {
        let mut leftover = String::new();
        lines_from_chunk(b"a\n", &mut leftover, true);
        assert_eq!(leftover, "");

        lines_from_chunk(b"b", &mut leftover, true);
        assert_eq!(leftover, "b");

        let lines = lines_from_chunk(b"\n", &mut leftover, true);
        assert_eq!(lines, vec!["b"]);
        assert_eq!(leftover, "");
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
        // SAFETY: tests in this module run sequentially via --test-threads=1
        // below when needed, but this test is self-contained — it sets HOME,
        // exercises default_debug_log, then drops the guard.
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
