use crate::events::KNOWN_OUTAGES;
use crate::models::Sensor;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Historical sensor anomaly or outage flag attached to affected calendar months.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityFlag {
    /// Terra MODIS record before 2000-11: incomplete / problematic early mission period
    /// ([11, section 6]; [26, section 2.2.1]).
    TerraEarlyRecord,
    /// Terra MOD14 full data outages: 2000-08-06..2000-08-17, 2001-06-16..2001-07-02,
    /// and 2002-03-21..2002-03-29 ([18, section 4]), derived from `events::KNOWN_OUTAGES`.
    TerraOutage,
    /// S-NPP VIIRS before 2012-04-03 (Mx5.3): ~50 K nadir brightness temperature bias
    /// ([13, section 4.2]).
    SnppEarlyCalibration,
    /// S-NPP VIIRS before 2012-10-15 (Mx6.3): spurious scan lines ([13, section 4.2]).
    SnppSpuriousScanLines,
    /// Terra and Aqua MODIS orbital drift from 2021 onward: equator-crossing time drift
    /// causing roughly -2.8% to -5.5% calibration deviation (fire-harmony, FireHarmonize).
    ModisOrbitalDrift,
}

#[derive(Clone, Copy)]
enum PlatformScope {
    Terra,
    Snpp,
    AnyModis,
}

impl PlatformScope {
    fn matches(self, platforms: &[Sensor]) -> bool {
        match self {
            Self::Terra => platforms.contains(&Sensor::ModisTerra),
            Self::Snpp => platforms.contains(&Sensor::ViirsSNPP),
            Self::AnyModis => platforms.iter().any(|s| s.is_modis()),
        }
    }
}

type Ymd = (i32, u32, u32);

struct PeriodRule {
    flag: QualityFlag,
    scope: PlatformScope,
    start: Option<Ymd>,
    end: Option<Ymd>,
}

const STATIC_PERIODS: &[PeriodRule] = &[
    PeriodRule {
        flag: QualityFlag::TerraEarlyRecord,
        scope: PlatformScope::Terra,
        start: None,
        end: Some((2000, 10, 31)),
    },
    PeriodRule {
        flag: QualityFlag::SnppEarlyCalibration,
        scope: PlatformScope::Snpp,
        start: None,
        end: Some((2012, 4, 2)),
    },
    PeriodRule {
        flag: QualityFlag::SnppSpuriousScanLines,
        scope: PlatformScope::Snpp,
        start: None,
        end: Some((2012, 10, 14)),
    },
    PeriodRule {
        flag: QualityFlag::ModisOrbitalDrift,
        scope: PlatformScope::AnyModis,
        start: Some((2021, 1, 1)),
        end: None,
    },
];

fn ymd_to_date((y, m, d): Ymd) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid quality period date")
}

fn month_bounds(year: i32, month: u32) -> Option<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::from_ymd_opt(year, month, 1)?;
    let (next_year, next_month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    let end = NaiveDate::from_ymd_opt(next_year, next_month, 1)?.pred_opt()?;
    Some((start, end))
}

fn overlaps_month(
    month_start: NaiveDate,
    month_end: NaiveDate,
    start: Option<Ymd>,
    end: Option<Ymd>,
) -> bool {
    let after_start = start.is_none_or(|s| ymd_to_date(s) <= month_end);
    let before_end = end.is_none_or(|e| month_start <= ymd_to_date(e));
    after_start && before_end
}

/// Return all quality flags whose anomaly or outage period overlaps any day of `(year, month)`
/// and whose affected sensor family appears in `platforms`, in deterministic sorted order.
pub fn month_quality_flags(year: i32, month: u32, platforms: &[Sensor]) -> Vec<QualityFlag> {
    let Some((month_start, month_end)) = month_bounds(year, month) else {
        return Vec::new();
    };

    let mut flags = Vec::new();
    for rule in STATIC_PERIODS {
        if rule.scope.matches(platforms)
            && overlaps_month(month_start, month_end, rule.start, rule.end)
        {
            flags.push(rule.flag);
        }
    }

    if PlatformScope::Terra.matches(platforms)
        && KNOWN_OUTAGES
            .iter()
            .any(|&(start, end)| overlaps_month(month_start, month_end, Some(start), Some(end)))
    {
        flags.push(QualityFlag::TerraOutage);
    }

    flags.sort_unstable();
    flags.dedup();
    flags
}

/// Human-readable one-sentence English description of `flag` for API warning messages.
pub fn flag_description(flag: QualityFlag) -> &'static str {
    match flag {
        QualityFlag::TerraEarlyRecord => {
            "Terra MODIS records before 2000-11 are incomplete and problematic."
        }
        QualityFlag::TerraOutage => {
            "Terra MOD14 experienced complete data outages on known dates during this period."
        }
        QualityFlag::SnppEarlyCalibration => {
            "S-NPP VIIRS data before 2012-04-03 (Mx5.3) exhibit an ~50 K nadir bias."
        }
        QualityFlag::SnppSpuriousScanLines => {
            "S-NPP VIIRS data before 2012-10-15 (Mx6.3) contain spurious scan lines."
        }
        QualityFlag::ModisOrbitalDrift => {
            "Terra and Aqua MODIS orbital drift from 2021 onward introduces roughly -2.8% to -5.5% calibration deviation."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_flag_requires_matching_platform_and_period() {
        assert_eq!(
            month_quality_flags(2000, 10, &[Sensor::ModisTerra]),
            vec![QualityFlag::TerraEarlyRecord]
        );
        assert!(month_quality_flags(2000, 10, &[Sensor::ModisAqua]).is_empty());

        assert_eq!(
            month_quality_flags(2001, 6, &[Sensor::ModisTerra]),
            vec![QualityFlag::TerraOutage]
        );
        assert!(month_quality_flags(2001, 6, &[Sensor::ModisAqua]).is_empty());

        assert_eq!(
            month_quality_flags(2012, 3, &[Sensor::ViirsSNPP]),
            vec![
                QualityFlag::SnppEarlyCalibration,
                QualityFlag::SnppSpuriousScanLines,
            ]
        );
        assert!(month_quality_flags(2012, 3, &[Sensor::ViirsNOAA20]).is_empty());
        assert!(month_quality_flags(2012, 3, &[Sensor::ModisTerra]).is_empty());

        assert_eq!(
            month_quality_flags(2012, 9, &[Sensor::ViirsSNPP]),
            vec![QualityFlag::SnppSpuriousScanLines]
        );
        assert!(month_quality_flags(2012, 9, &[Sensor::ModisAqua]).is_empty());

        assert_eq!(
            month_quality_flags(2022, 7, &[Sensor::ModisTerra]),
            vec![QualityFlag::ModisOrbitalDrift]
        );
        assert_eq!(
            month_quality_flags(2022, 7, &[Sensor::ModisAqua]),
            vec![QualityFlag::ModisOrbitalDrift]
        );
        assert!(month_quality_flags(2022, 7, &[Sensor::ViirsSNPP]).is_empty());
    }

    #[test]
    fn boundary_months_match_exact_cutoffs() {
        // Terra early record: before 2000-11
        assert!(
            month_quality_flags(2000, 10, &[Sensor::ModisTerra])
                .contains(&QualityFlag::TerraEarlyRecord)
        );
        assert!(
            !month_quality_flags(2000, 11, &[Sensor::ModisTerra])
                .contains(&QualityFlag::TerraEarlyRecord)
        );

        // S-NPP Mx5.3 ~50 K bias: before 2012-04-03 (April 1-2 fall inside)
        assert!(
            month_quality_flags(2012, 3, &[Sensor::ViirsSNPP])
                .contains(&QualityFlag::SnppEarlyCalibration)
        );
        assert!(
            month_quality_flags(2012, 4, &[Sensor::ViirsSNPP])
                .contains(&QualityFlag::SnppEarlyCalibration)
        );
        assert!(
            !month_quality_flags(2012, 5, &[Sensor::ViirsSNPP])
                .contains(&QualityFlag::SnppEarlyCalibration)
        );

        // S-NPP Mx6.3 spurious scan lines: before 2012-10-15 (Oct 1-14 fall inside)
        assert!(
            month_quality_flags(2012, 10, &[Sensor::ViirsSNPP])
                .contains(&QualityFlag::SnppSpuriousScanLines)
        );
        assert!(
            !month_quality_flags(2012, 11, &[Sensor::ViirsSNPP])
                .contains(&QualityFlag::SnppSpuriousScanLines)
        );

        // MODIS orbital drift: 2021 onward
        assert!(
            !month_quality_flags(2020, 12, &[Sensor::ModisTerra, Sensor::ModisAqua])
                .contains(&QualityFlag::ModisOrbitalDrift)
        );
        assert!(
            month_quality_flags(2021, 1, &[Sensor::ModisAqua])
                .contains(&QualityFlag::ModisOrbitalDrift)
        );
    }

    #[test]
    fn terra_outage_matches_known_outages_windows() {
        let mut flagged_months = Vec::new();
        for year in 2000..=2003 {
            for month in 1..=12 {
                if month_quality_flags(year, month, &[Sensor::ModisTerra])
                    .contains(&QualityFlag::TerraOutage)
                {
                    flagged_months.push((year, month));
                }
            }
        }
        assert_eq!(
            flagged_months,
            vec![(2000, 8), (2001, 6), (2001, 7), (2002, 3)]
        );
    }

    #[test]
    fn flags_are_sorted_deduped_and_descriptions_non_empty() {
        let flags = month_quality_flags(
            2000,
            8,
            &[
                Sensor::ModisTerra,
                Sensor::ModisTerra,
                Sensor::ViirsSNPP,
            ],
        );
        assert_eq!(
            flags,
            vec![
                QualityFlag::TerraEarlyRecord,
                QualityFlag::TerraOutage,
                QualityFlag::SnppEarlyCalibration,
                QualityFlag::SnppSpuriousScanLines,
            ]
        );
        for flag in flags {
            assert!(!flag_description(flag).is_empty());
        }
        assert!(!flag_description(QualityFlag::ModisOrbitalDrift).is_empty());
    }
}
