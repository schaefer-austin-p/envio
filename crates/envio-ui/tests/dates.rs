use envio_ui::{DateBucket, bucket};

// 2027-01-15 12:00:00 UTC
const NOW: i64 = 1_800_014_400;
const H: i64 = 3_600;
const D: i64 = 86_400;

#[test]
fn same_local_day_is_time() {
    assert_eq!(bucket(NOW, Some(NOW - 5 * H), 0), DateBucket::Time);
}

#[test]
fn the_utc_offset_decides_the_day() {
    // 23:00 UTC on the 14th: yesterday at UTC, the 15th at +2h.
    let date = NOW - 13 * H;
    assert_eq!(bucket(NOW, Some(date), 0), DateBucket::Weekday);
    assert_eq!(bucket(NOW, Some(date), 2 * 3_600), DateBucket::Time);
    // 09:00 UTC is 01:00 on the 15th at -8h, and now is 04:00 there.
    assert_eq!(bucket(NOW, Some(NOW - 3 * H), -8 * 3_600), DateBucket::Time);
}

#[test]
fn six_days_back_is_weekday() {
    assert_eq!(bucket(NOW, Some(NOW - 6 * D), 0), DateBucket::Weekday);
    assert_eq!(bucket(NOW, Some(NOW - 7 * D), 0), DateBucket::DayMonth);
}

#[test]
fn same_year_is_day_month() {
    // 2027-01-02
    assert_eq!(bucket(NOW, Some(NOW - 13 * D), 0), DateBucket::DayMonth);
}

#[test]
fn older_is_full() {
    assert_eq!(bucket(NOW, Some(NOW - 20 * D), 0), DateBucket::Full);
    assert_eq!(bucket(NOW, Some(0), 0), DateBucket::Full);
}

#[test]
fn no_date_is_none() {
    assert_eq!(bucket(NOW, None, 0), DateBucket::None);
}
