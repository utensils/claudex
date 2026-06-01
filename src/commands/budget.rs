use anyhow::{Result, bail};
use chrono::{Datelike, Local, NaiveTime, TimeZone, Timelike, Utc};

use crate::cli::ResolvedFilter;
use crate::index::IndexStore;
use crate::providers::enabled_default;
use crate::time_utils::local_day_start_ms;
use crate::ui;

pub fn run(monthly: f64, json: bool, filter: &ResolvedFilter) -> Result<()> {
    if monthly <= 0.0 {
        bail!("--monthly must be greater than 0");
    }
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;
    idx.ensure_fresh(&providers)?;

    let today = Local::now().date_naive();
    let default_start = today
        .with_day(1)
        .expect("every calendar month has a first day");
    let mut scoped = filter.clone();
    let period_start = scoped.since_ms.map(date_from_ms).unwrap_or(default_start);
    if scoped.since_ms.is_none() {
        scoped.since_ms = Some(local_day_start_ms(period_start));
    }
    let period_end = scoped.until_ms.map(date_from_ms).unwrap_or(today);
    let days_elapsed = (period_end - period_start).num_days().max(0) + 1;
    let days_in_month = days_in_month(period_end.year(), period_end.month()) as i64;

    let summary = idx.query_summary(&scoped)?;
    let spent = summary.total_cost;
    let projected = if days_elapsed > 0 {
        spent / days_elapsed as f64 * days_in_month as f64
    } else {
        spent
    };
    let remaining = monthly - spent;
    let pct = spent / monthly * 100.0;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "monthly_budget_usd": monthly,
                "period_start": period_start.to_string(),
                "period_end": period_end.to_string(),
                "days_elapsed": days_elapsed,
                "days_in_month": days_in_month,
                "spent_usd": spent,
                "remaining_usd": remaining,
                "used_percent": pct,
                "projected_month_end_usd": projected,
                "projected_over_budget_usd": (projected - monthly).max(0.0),
                "sessions": summary.total_sessions,
            }))?
        );
        return Ok(());
    }

    let mut table = ui::table();
    table.set_header(ui::header([
        "Budget",
        "Spent",
        "Remaining",
        "Used",
        "Projected",
    ]));
    ui::right_align(&mut table, &[0, 1, 2, 3, 4]);
    table.add_row([
        ui::cell_cost(monthly),
        ui::cell_cost(spent),
        ui::cell_cost(remaining),
        ui::cell_plain(format!("{pct:.1}%")),
        ui::cell_cost(projected),
    ]);
    println!("{table}");
    Ok(())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap();
    let last = next - chrono::Duration::days(1);
    last.day()
}

fn date_from_ms(ms: i64) -> chrono::NaiveDate {
    let utc = Utc
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now);
    let time = utc.time();
    if time == NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        || (time.hour(), time.minute(), time.second(), time.nanosecond())
            == (23, 59, 59, 999_000_000)
    {
        return utc.date_naive();
    }

    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
        .date_naive()
}
