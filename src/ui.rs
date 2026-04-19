//! Presentation layer — one place owns the palette, table style, terminal
//! width, color detection, and progress indicators. Commands call semantic
//! helpers (`project`, `timestamp`, `emphasis`, …) rather than raw
//! `owo_colors` methods so the palette can change in one place.

use std::io::IsTerminal;
use std::sync::OnceLock;
use std::time::Duration;

use clap::ValueEnum;
use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ContentArrangement, Table, TableComponent,
    presets::NOTHING,
};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ColorChoice {
    /// Colorize when stdout is a TTY and NO_COLOR is unset (default)
    Auto,
    /// Always emit ANSI color escapes
    Always,
    /// Never emit ANSI color escapes
    Never,
}

static COLOR_ON: OnceLock<bool> = OnceLock::new();

pub fn apply_color_choice(choice: ColorChoice) {
    let on = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        }
    };
    let _ = COLOR_ON.set(on);
}

fn color_on() -> bool {
    *COLOR_ON
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

// --- Table builder ---

/// Preconfigured comfy-table with a minimal style: no outer box, no vertical
/// dividers, no per-row separators — just a horizontal rule under the header.
/// Dynamic arrangement fits content to the current terminal width.
pub fn table() -> Table {
    let mut t = Table::new();
    t.load_preset(NOTHING);
    t.set_style(TableComponent::HeaderLines, '─');
    t.set_content_arrangement(ContentArrangement::Dynamic);
    if let Some((w, _)) = terminal_size::terminal_size() {
        t.set_width(w.0);
    }
    if color_on() {
        t.enforce_styling();
    } else {
        t.force_no_tty();
    }
    t
}

/// Build bold + cyan header cells. Comfy-table renders these as ANSI when
/// the table's styling is enabled; `force_no_tty` (called by `table()` under
/// `--color never`) suppresses them cleanly.
pub fn header<I, S>(cells: I) -> Vec<Cell>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cells
        .into_iter()
        .map(|c| {
            Cell::new(c.into())
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan)
        })
        .collect()
}

/// Right-align the specified column indices. Use for numeric columns so
/// digits line up on the decimal.
pub fn right_align(table: &mut Table, cols: &[usize]) {
    for &i in cols {
        if let Some(col) = table.column_mut(i) {
            col.set_cell_alignment(CellAlignment::Right);
        }
    }
}

/// Build a bold summary row (e.g. "TOTAL"). Cells inherit column alignment.
pub fn total_row<I, S>(cells: I) -> Vec<Cell>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cells
        .into_iter()
        .map(|c| Cell::new(c.into()).add_attribute(Attribute::Bold))
        .collect()
}

// --- Cell builders for table rows ---
//
// Use these instead of passing raw `String` values to `add_row` so every
// command gets the same palette. Comfy-table strips styling automatically
// when the table is in non-TTY mode (`--color never` calls `force_no_tty`).

pub fn cell_project(s: &str) -> Cell {
    Cell::new(s).fg(Color::Cyan)
}

pub fn cell_cost(usd: f64) -> Cell {
    Cell::new(fmt_cost(usd)).fg(Color::Green)
}

pub fn cell_count(n: u64) -> Cell {
    Cell::new(fmt_count(n))
}

pub fn cell_model(s: &str) -> Cell {
    Cell::new(s).fg(Color::Yellow)
}

pub fn cell_tool(s: &str) -> Cell {
    Cell::new(s).fg(Color::Magenta)
}

pub fn cell_dim(s: &str) -> Cell {
    Cell::new(s).fg(Color::DarkGrey)
}

pub fn cell_plain(s: impl Into<String>) -> Cell {
    Cell::new(s.into())
}

// --- Number formatting ---

/// Format a cost as `$12,345.67` — two decimals, thousands separator. Negative
/// values render as `-$5.00`, not `$-5.00`.
pub fn fmt_cost(usd: f64) -> String {
    let sign = if usd < 0.0 { "-" } else { "" };
    let total_cents = (usd.abs() * 100.0).round() as u64;
    let whole = total_cents / 100;
    let cents = total_cents % 100;
    format!("{}${}.{:02}", sign, group_thousands_u64(whole), cents)
}

/// Format an integer with comma thousands separators: `12,345`.
pub fn fmt_count(n: u64) -> String {
    group_thousands_u64(n)
}

fn group_thousands_u64(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let first = bytes.len() % 3;
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && i >= first && (i - first) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

// --- Palette ---
//
// Each helper returns an owned `String` so call sites stay simple. Allocation
// cost is negligible for report output. Keep helpers semantic, not color-named.

pub fn project(s: &str) -> String {
    if color_on() {
        s.bright_blue().to_string()
    } else {
        s.to_string()
    }
}

pub fn project_headline(s: &str) -> String {
    if color_on() {
        s.bright_blue().bold().to_string()
    } else {
        s.to_string()
    }
}

pub fn session_id(s: &str) -> String {
    if color_on() {
        s.dimmed().to_string()
    } else {
        s.to_string()
    }
}

pub fn timestamp(s: &str) -> String {
    if color_on() {
        s.dimmed().to_string()
    } else {
        s.to_string()
    }
}

pub fn tool_name(s: &str) -> String {
    if color_on() {
        s.cyan().to_string()
    } else {
        s.to_string()
    }
}

pub fn model_name(s: &str) -> String {
    if color_on() {
        s.yellow().to_string()
    } else {
        s.to_string()
    }
}

pub fn role(s: &str) -> String {
    if color_on() {
        s.bright_yellow().to_string()
    } else {
        s.to_string()
    }
}

pub fn section_title(s: &str) -> String {
    if color_on() {
        s.bold().to_string()
    } else {
        s.to_string()
    }
}

pub fn emphasis(s: &str) -> String {
    if color_on() {
        s.bold().to_string()
    } else {
        s.to_string()
    }
}

pub fn match_highlight(s: &str) -> String {
    if color_on() {
        s.bright_red().bold().to_string()
    } else {
        s.to_string()
    }
}

pub fn banner(s: &str) -> String {
    if color_on() {
        s.bright_yellow().to_string()
    } else {
        s.to_string()
    }
}

/// Colored cost for non-table contexts (summary). Green dollar figure.
pub fn cost(usd: f64) -> String {
    if color_on() {
        fmt_cost(usd).green().to_string()
    } else {
        fmt_cost(usd)
    }
}

/// Colored count for non-table contexts. No special color; just formatted.
pub fn count(n: u64) -> String {
    fmt_count(n)
}

// Log-level helpers used by `watch`.

pub fn level_error(s: &str) -> String {
    if color_on() {
        s.red().bold().to_string()
    } else {
        s.to_string()
    }
}

pub fn level_warn(s: &str) -> String {
    if color_on() {
        s.yellow().to_string()
    } else {
        s.to_string()
    }
}

pub fn level_debug(s: &str) -> String {
    if color_on() {
        s.dimmed().to_string()
    } else {
        s.to_string()
    }
}

/// Color a session-record `type` string — green for user, blue for assistant,
/// dimmed for system, yellow for other.
pub fn record_type(ty: &str) -> String {
    if !color_on() {
        return ty.to_string();
    }
    match ty {
        "user" => ty.bright_green().bold().to_string(),
        "assistant" => ty.bright_blue().bold().to_string(),
        "system" => ty.dimmed().to_string(),
        _ => ty.bright_yellow().to_string(),
    }
}

/// Color a plain text log line based on keywords it contains (fallback for
/// non-JSON `watch` lines).
pub fn classify_text_line(line: &str) -> String {
    if !color_on() {
        return line.to_string();
    }
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

// --- Progress spinner ---
//
// TTY-gated: when stderr isn't a terminal (CI, pipes), construction returns a
// no-op guard so output stays clean. Spinner draws to stderr so `--json` on
// stdout is never contaminated.

pub struct Spinner(Option<ProgressBar>);

impl Spinner {
    pub fn start(message: impl Into<String>) -> Self {
        if !std::io::stderr().is_terminal() {
            return Self(None);
        }
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.set_message(message.into());
        pb.enable_steady_tick(Duration::from_millis(100));
        Self(Some(pb))
    }

    pub fn finish(self) {
        if let Some(pb) = self.0 {
            pb.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_thousands_edge_cases() {
        assert_eq!(group_thousands_u64(0), "0");
        assert_eq!(group_thousands_u64(999), "999");
        assert_eq!(group_thousands_u64(1_000), "1,000");
        assert_eq!(group_thousands_u64(12_345), "12,345");
        assert_eq!(group_thousands_u64(1_234_567), "1,234,567");
        assert_eq!(group_thousands_u64(1_000_000_000), "1,000,000,000");
    }

    #[test]
    fn fmt_cost_rounds_to_two_decimals() {
        assert_eq!(fmt_cost(12735.6563), "$12,735.66");
        assert_eq!(fmt_cost(0.0), "$0.00");
        assert_eq!(fmt_cost(0.125), "$0.13"); // banker's vs half-up — .round() is half-away-from-zero
        assert_eq!(fmt_cost(1_234_567.89), "$1,234,567.89");
    }

    #[test]
    fn fmt_cost_negative_sign_outside_dollar() {
        assert_eq!(fmt_cost(-5.5), "-$5.50");
    }

    #[test]
    fn fmt_count_formats_big_numbers() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(12), "12");
        assert_eq!(fmt_count(326_347), "326,347");
        assert_eq!(fmt_count(17_596_000_000), "17,596,000,000");
    }
}
