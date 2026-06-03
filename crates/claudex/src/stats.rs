pub fn percentile_sorted(sorted: &[i64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p * sorted.len()).saturating_sub(1) / 100;
    sorted[idx.min(sorted.len() - 1)] as f64
}

pub fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "-".to_string();
    }
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::{format_duration, percentile_sorted};

    #[test]
    fn percentile_sorted_handles_empty_input() {
        assert_eq!(percentile_sorted(&[], 50), 0.0);
    }

    #[test]
    fn percentile_sorted_returns_expected_cutoffs() {
        let values = [10, 20, 30, 40];
        assert_eq!(percentile_sorted(&values, 50), 20.0);
        assert_eq!(percentile_sorted(&values, 95), 40.0);
    }

    #[test]
    fn duration_formats_compactly() {
        assert_eq!(format_duration(0), "-");
        assert_eq!(format_duration(45_000), "45s");
        assert_eq!(format_duration(90_000), "1m30s");
        assert_eq!(format_duration(3_661_000), "1h1m");
    }
}
