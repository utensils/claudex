use chrono::{Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

pub fn local_day_start_ms(date: NaiveDate) -> i64 {
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).expect("valid time");
    let local_midnight = NaiveDateTime::new(date, midnight);
    match Local.from_local_datetime(&local_midnight) {
        LocalResult::Single(dt) => dt.timestamp_millis(),
        LocalResult::Ambiguous(a, b) => a.min(b).timestamp_millis(),
        LocalResult::None => local_midnight.and_utc().timestamp_millis(),
    }
}
