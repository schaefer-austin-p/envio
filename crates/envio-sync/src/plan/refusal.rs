//! Why a draft upload was refused, and how long to wait before the next
//! try. Pure.

use envio_store::RefusalKind;

/// The kind of a refusal, from `NetError::Refused`'s text (of the form
/// `code: <Option<Code>>, info: <Option<str>>`). Case-insensitive, first hit
/// wins: quota, then read-only, then too large, else `Other`. A best guess:
/// the server's wording is its own.
pub fn classify(refusal_text: &str) -> RefusalKind {
    let text = refusal_text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
    if has(&["overquota", "quota"]) {
        RefusalKind::OverQuota
    } else if has(&["readonly", "read-only", "noperm", "permission"]) {
        RefusalKind::ReadOnly
    } else if has(&["toobig", "too large", "too big", "size"]) {
        RefusalKind::TooLarge
    } else {
        RefusalKind::Other
    }
}

const FIRST_WAIT_MS: i64 = 5 * 60 * 1_000;
const LONGEST_WAIT_MS: i64 = 60 * 60 * 1_000;

/// The wait after the n-th failure of one save: 5 minutes, doubling, at
/// most an hour. No overflow for any count.
pub fn retry_delay_ms(failures: u32) -> i64 {
    // 5 min * 2^4 is already over the cap, so the shift never needs more.
    let doublings = failures.saturating_sub(1).min(4);
    (FIRST_WAIT_MS << doublings).min(LONGEST_WAIT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use RefusalKind::*;

    #[test]
    fn classify_sorts_the_texts_a_server_sends() {
        let cases = [
            ("NO [OVERQUOTA] Mailbox is full", OverQuota),
            ("code: None, info: Some(\"OVERQUOTA\")", OverQuota),
            ("code: None, info: Some(\"NOPERM no access\")", ReadOnly),
            ("code: Some(ReadOnly), info: None", ReadOnly),
            ("code: None, info: Some(\"TOOBIG\")", TooLarge),
            ("code: None, info: Some(\"message too large\")", TooLarge),
            ("code: None, info: Some(\"[LIMIT] busy\")", Other),
            ("code: None, info: None", Other),
            ("code: Some(TryCreate), info: None", Other),
            ("No [OverQuota]", OverQuota),
            ("noPerm", ReadOnly),
            ("TooBig", TooLarge),
            ("READONLY", ReadOnly),
        ];
        for (text, kind) in cases {
            assert_eq!(classify(text), kind, "{text}");
        }
    }

    #[test]
    fn the_first_hit_wins() {
        assert_eq!(classify("quota: permission denied"), OverQuota);
    }

    #[test]
    fn the_delay_doubles_from_five_minutes_and_stops_at_an_hour() {
        let min = 60_000;
        let got: Vec<_> = (1..=7).map(retry_delay_ms).collect();
        let want = [5, 10, 20, 40, 60, 60, 60].map(|m| m * min);
        assert_eq!(got, want);
        assert_eq!(retry_delay_ms(u32::MAX), 60 * min);
        assert_eq!(retry_delay_ms(0), 5 * min);
    }
}
