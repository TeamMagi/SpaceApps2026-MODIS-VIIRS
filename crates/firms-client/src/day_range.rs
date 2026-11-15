use std::fmt;

use anyhow::bail;
use chrono::{Duration, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DayRange(u8);

impl DayRange {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 5;

    pub fn new(days: u8) -> anyhow::Result<Self> {
        if !(Self::MIN..=Self::MAX).contains(&days) {
            bail!(
                "day range must be between {} and {} days inclusive, got {days}",
                Self::MIN,
                Self::MAX
            );
        }

        Ok(Self(days))
    }

    pub fn clamped(days: u32) -> Self {
        let days = days.clamp(u32::from(Self::MIN), u32::from(Self::MAX));
        Self(days as u8)
    }

    pub fn get(self) -> u8 {
        self.0
    }

    pub fn as_u32(self) -> u32 {
        u32::from(self.0)
    }
}

impl Default for DayRange {
    fn default() -> Self {
        Self(Self::MIN)
    }
}

impl fmt::Display for DayRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn date_windows(start: NaiveDate, end: NaiveDate) -> Vec<(NaiveDate, DayRange)> {
    if end < start {
        return Vec::new();
    }

    let mut windows = Vec::new();
    let mut window_start = start;

    loop {
        let remaining = (end - window_start).num_days() + 1;
        let length = remaining.min(i64::from(DayRange::MAX)) as u8;

        windows.push((window_start, DayRange(length)));

        if i64::from(length) >= remaining {
            break;
        }

        window_start += Duration::days(i64::from(length));
    }

    windows
}

pub fn recent_window(today: NaiveDate, days: DayRange) -> (NaiveDate, DayRange) {
    let offset = i64::from(days.get() - 1);
    (today - Duration::days(offset), days)
}

pub fn transaction_cost(windows: &[(NaiveDate, DayRange)]) -> u32 {
    windows.iter().map(|(_, days)| days.as_u32()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn lengths(windows: &[(NaiveDate, DayRange)]) -> Vec<u8> {
        windows.iter().map(|(_, days)| days.get()).collect()
    }

    #[test]
    fn new_rejects_zero_and_six() {
        assert!(DayRange::new(0).is_err());
        assert!(DayRange::new(6).is_err());
    }

    #[test]
    fn clamped_saturates_into_valid_range() {
        assert_eq!(DayRange::clamped(0), DayRange::new(1).unwrap());
        assert_eq!(DayRange::clamped(999), DayRange::new(5).unwrap());
    }

    #[test]
    fn date_windows_splits_exact_multiple_of_five() {
        let windows = date_windows(date(2026, 1, 1), date(2026, 1, 10));

        assert_eq!(lengths(&windows), vec![5, 5]);
        assert_eq!(
            windows.iter().map(|(start, _)| *start).collect::<Vec<_>>(),
            vec![date(2026, 1, 1), date(2026, 1, 6)]
        );
    }

    #[test]
    fn date_windows_splits_remainder() {
        let windows = date_windows(date(2026, 1, 1), date(2026, 1, 12));

        assert_eq!(lengths(&windows), vec![5, 5, 2]);
        assert_eq!(
            windows.iter().map(|(start, _)| *start).collect::<Vec<_>>(),
            vec![date(2026, 1, 1), date(2026, 1, 6), date(2026, 1, 11)]
        );
    }

    #[test]
    fn date_windows_handles_single_day() {
        let day = date(2026, 1, 1);
        let windows = date_windows(day, day);

        assert_eq!(windows, vec![(day, DayRange::new(1).unwrap())]);
    }

    #[test]
    fn date_windows_returns_empty_for_reversed_range() {
        assert!(date_windows(date(2026, 1, 2), date(2026, 1, 1)).is_empty());
    }

    #[test]
    fn recent_window_ends_today() {
        let today = date(2026, 11, 14);
        let days = DayRange::new(5).unwrap();

        assert_eq!(recent_window(today, days), (date(2026, 11, 10), days));
    }

    #[test]
    fn date_windows_spans_leap_day() {
        let windows = date_windows(date(2024, 2, 26), date(2024, 3, 2));

        assert_eq!(lengths(&windows), vec![5, 1]);
        assert_eq!(
            windows.iter().map(|(start, _)| *start).collect::<Vec<_>>(),
            vec![date(2024, 2, 26), date(2024, 3, 2)]
        );
    }

    #[test]
    fn new_accepts_exact_limits_and_rejects_outside_with_message() {
        // 3-value BVA around MIN=1 and MAX=5, plus the u8 extreme.
        for ok in [1u8, 2, 4, 5] {
            assert_eq!(DayRange::new(ok).unwrap().get(), ok);
        }
        for bad in [0u8, 6, u8::MAX] {
            let err = DayRange::new(bad).unwrap_err().to_string();
            assert!(err.contains("between 1 and 5") && err.contains(&format!("got {bad}")), "{err}");
        }
    }

    #[test]
    fn clamped_maps_every_input_class_into_range() {
        let cases = [(0, 1), (1, 1), (2, 2), (4, 4), (5, 5), (6, 5), (u32::MAX, 5)];
        for (input, expected) in cases {
            assert_eq!(DayRange::clamped(input).get(), expected, "input {input}");
        }
    }

    #[test]
    fn default_display_and_conversions_agree() {
        assert_eq!(DayRange::default().get(), 1);
        assert_eq!(DayRange::clamped(3).to_string(), "3");
        assert_eq!(DayRange::clamped(4).as_u32(), 4);
        assert!(DayRange::clamped(2) < DayRange::clamped(3));
    }

    #[test]
    fn date_windows_boundary_lengths_around_the_five_day_limit() {
        // 4, 5 and 6 days: just below, on, and just above one window.
        let start = date(2026, 3, 1);
        for (days, expected) in [(4i64, vec![4u8]), (5, vec![5]), (6, vec![5, 1])] {
            let windows = date_windows(start, start + Duration::days(days - 1));
            assert_eq!(lengths(&windows), expected, "{days} days");
        }
    }

    #[test]
    fn date_windows_are_contiguous_and_cover_every_day_exactly_once() {
        // Invariant over lengths 1..=40, crossing a year boundary.
        let start = date(2025, 12, 20);
        for len in 1..=40i64 {
            let end = start + Duration::days(len - 1);
            let windows = date_windows(start, end);
            assert_eq!(windows[0].0, start);
            let mut cursor = start;
            for (win_start, days) in &windows {
                assert_eq!(*win_start, cursor, "gap or overlap for {len} days");
                assert!(days.get() <= DayRange::MAX);
                cursor += Duration::days(i64::from(days.get()));
            }
            assert_eq!(cursor, end + Duration::days(1));
            assert_eq!(i64::from(transaction_cost(&windows)), len);
        }
    }

    #[test]
    fn transaction_cost_of_no_windows_is_zero() {
        assert_eq!(transaction_cost(&[]), 0);
    }

    #[test]
    fn recent_window_with_one_day_starts_today_and_crosses_month_boundary() {
        let one = DayRange::new(1).unwrap();
        assert_eq!(recent_window(date(2026, 3, 1), one), (date(2026, 3, 1), one));

        let five = DayRange::new(5).unwrap();
        assert_eq!(recent_window(date(2026, 3, 2), five).0, date(2026, 2, 26));
    }
}
