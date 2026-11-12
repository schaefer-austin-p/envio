//! Which way a row's date is shown. QML formats the bucket with the Qt
//! locale; nothing here builds a date string.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateBucket {
    /// Today: the time.
    Time,
    /// The last six days: the weekday.
    Weekday,
    /// This year: day and month.
    DayMonth,
    /// Older: with the year.
    Full,
    /// No usable date.
    None,
}

impl DateBucket {
    /// The QML role value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Weekday => "weekday",
            Self::DayMonth => "daymonth",
            Self::Full => "full",
            Self::None => "none",
        }
    }
}

const DAY: i64 = 86_400;

/// The year of a day count since 1970-01-01 (proleptic Gregorian).
fn year_of(days: i64) -> i64 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let y = yoe + era * 400;
    if mp >= 10 { y + 1 } else { y }
}

/// `utc_offset_s` is the local offset east of UTC at `now_s`.
pub fn bucket(now_s: i64, date_s: Option<i64>, utc_offset_s: i32) -> DateBucket {
    let Some(date_s) = date_s else {
        return DateBucket::None;
    };
    let off = i64::from(utc_offset_s);
    let today = (now_s + off).div_euclid(DAY);
    let day = (date_s + off).div_euclid(DAY);
    match today - day {
        0 => DateBucket::Time,
        1..=6 => DateBucket::Weekday,
        _ if year_of(today) == year_of(day) => DateBucket::DayMonth,
        _ => DateBucket::Full,
    }
}
