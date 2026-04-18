use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;

pub fn run(raw: bool) -> Result<()> {
    let home = dirs::home_dir().context("could not find home directory")?;
    let latest = home.join(".claude").join("debug").join("latest");

    if !latest.exists() {
        anyhow::bail!("~/.claude/debug/latest does not exist — is debug logging enabled?");
    }

    let mut current_target = resolve_target(&latest)?;
    let mut file = fs::File::open(&current_target)
        .with_context(|| format!("opening {}", current_target.display()))?;
    file.seek(SeekFrom::End(0))?;
    let mut reader = BufReader::new(file);

    eprintln!(
        "{}",
        format!("watching {}", current_target.display()).dimmed()
    );

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                if let Ok(new_target) = resolve_target(&latest) {
                    if new_target != current_target {
                        eprintln!(
                            "{}",
                            format!("\n--- session switched: {} ---\n", new_target.display())
                                .dimmed()
                        );
                        current_target = new_target;
                        let new_file = fs::File::open(&current_target)
                            .with_context(|| format!("opening {}", current_target.display()))?;
                        reader = BufReader::new(new_file);
                        continue;
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\n', '\r']);
                if raw {
                    println!("{trimmed}");
                } else {
                    print_formatted(trimmed);
                }
            }
            Err(e) => {
                eprintln!("{}", format!("read error: {e}").red());
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

fn resolve_target(path: &Path) -> Result<PathBuf> {
    match fs::read_link(path) {
        Ok(target) => {
            if target.is_relative() {
                let parent = path.parent().unwrap_or(Path::new("."));
                Ok(parent.join(target))
            } else {
                Ok(target)
            }
        }
        Err(_) => Ok(path.to_path_buf()),
    }
}

fn print_formatted(line: &str) {
    if line.trim().is_empty() {
        return;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(v) => format_json_record(&v),
        Err(_) => println!("{line}"),
    }
}

fn format_json_record(v: &Value) {
    let ts = v["timestamp"].as_str().or(v["ts"].as_str()).unwrap_or("");
    let level = v["level"].as_str().unwrap_or("");
    let msg = v["message"].as_str().or(v["msg"].as_str()).unwrap_or("");
    let record_type = v["type"].as_str().unwrap_or("");

    let ts_prefix = if ts.is_empty() {
        String::new()
    } else {
        format!("{} ", ts.dimmed())
    };

    let tag = match record_type {
        "assistant" => format!("[{}] ", "assistant".bright_green().bold()),
        "user" => format!("[{}] ", "user".bright_blue().bold()),
        "tool_use" => format!("[{}] ", "tool_use".bright_yellow().bold()),
        "tool_result" => format!("[{}] ", "tool_result".bright_yellow().bold()),
        _ => {
            let level_str = match level {
                "error" | "ERROR" => level.bright_red().bold().to_string(),
                "warn" | "WARN" | "warning" | "WARNING" => level.bright_yellow().to_string(),
                "info" | "INFO" => level.bright_cyan().to_string(),
                "debug" | "DEBUG" => level.dimmed().to_string(),
                other if !other.is_empty() => other.to_string(),
                _ => String::new(),
            };
            if level_str.is_empty() {
                String::new()
            } else {
                format!("[{level_str}] ")
            }
        }
    };

    if msg.is_empty() && tag.is_empty() {
        println!(
            "{ts_prefix}{}",
            serde_json::to_string(v).unwrap_or_default().dimmed()
        );
        return;
    }

    let msg_colored = if level.eq_ignore_ascii_case("error")
        || record_type == "error"
        || msg.to_lowercase().contains("error")
    {
        msg.bright_red().to_string()
    } else {
        msg.to_string()
    };

    println!("{ts_prefix}{tag}{msg_colored}");
}
