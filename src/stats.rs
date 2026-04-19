pub fn percentile_sorted(sorted: &[i64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p * sorted.len()).saturating_sub(1) / 100;
    sorted[idx.min(sorted.len() - 1)] as f64
}

#[cfg(test)]
mod tests {
    use super::percentile_sorted;

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
}
