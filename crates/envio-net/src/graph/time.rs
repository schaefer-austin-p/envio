//! RFC 3339 UTC timestamps, as Graph writes them (`2026-11-21T22:39:44Z`).

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn num(s: &str) -> Option<i64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// Milliseconds since the epoch for `YYYY-MM-DDTHH:MM:SS[.fff]Z`; `None` for
/// anything else (other offsets included: Graph sends UTC).
pub fn rfc3339_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day) = (num(d.next()?)?, num(d.next()?)?, num(d.next()?)?);
    let (clock, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut t = clock.split(':');
    let (h, min, sec) = (num(t.next()?)?, num(t.next()?)?, num(t.next()?)?);
    let valid = d.next().is_none()
        && t.next().is_none()
        && (1..=12).contains(&m)
        && (1..=31).contains(&day)
        && h < 24
        && min < 60
        && sec < 61;
    if !valid {
        return None;
    }
    let ms: i64 = match frac {
        "" => 0,
        f => format!("{:0<3}", &f[..f.len().min(3)]).parse().ok()?,
    };
    Some(((days(y, m, day) * 24 + h) * 60 + min) * 60_000 + sec * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_ms("2026-11-21T22:39:44Z"), Some(1_795_300_784_000));
        assert_eq!(rfc3339_ms("2000-02-29T12:00:00.5Z"), Some(951_825_600_500));
        assert_eq!(rfc3339_ms("1969-12-31T23:59:59Z"), Some(-1000));
    }

    #[test]
    fn other_shapes_are_none() {
        for s in [
            "",
            "2026-11-21",
            "2026-11-21T22:39:44",
            "2026-11-21T22:39:44+01:00",
            "2026-13-01T00:00:00Z",
            "x-y-zT00:00:00Z",
        ] {
            assert_eq!(rfc3339_ms(s), None, "{s}");
        }
    }
}
