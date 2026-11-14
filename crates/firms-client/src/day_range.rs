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
}
