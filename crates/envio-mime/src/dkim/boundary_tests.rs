use super::boundary::{agrees, reader_body_start, well_formed};
use super::canon::body_start;
use proptest::prelude::*;

/// Line-ending mixes between the headers and the body, and inside them.
const TABLE: &[&str] = &[
    "From: a@b\r\nSubject: x\r\n\r\nbody\r\n",
    "From: a@b\nSubject: x\n\nbody\n",
    "From: a@b\r\nSubject: x\r\n\nbody\r\n",
    "From: a@b\r\nSubject: x\n\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\r\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\r\r\r\nbody\r\n",
    "From: a@b\r\n\r\r\nPHISH: send money\r\nSubject: x\r\n\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\r\n\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\t\r\n\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\x0c\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\r \r\nbody\r\n",
    "From: a@b\r\r\nSubject: x\r\n\r\nbody\r\n",
    "From: a@b\r\nSubject: x\r\n\r\n",
    "From: a@b\r\nSubject: x\r\n",
    "From: a@b\r\nSubject: x",
    "From: a@b\r\nSubject: x\r\n continued\r\n\r\nbody\r\n",
    "\r\nbody only\r\n",
    "\r\r\nFrom: a@b\r\n\r\nbody\r\n",
    " \r\nFrom: a@b\r\n\r\nbody\r\n",
];

/// Header blocks the guard refuses, whatever the offsets say: a line that
/// is no field, or a field line starting with CR, FF or a control byte.
const MALFORMED: &[&str] = &[
    "From: a@b\r\nSubject: x\r\n\rbody\r\n\r\nmore\r\n",
    "From: a@b\r\nSubject: x\r\n \r\nbody\r\n\r\nmore\r\n",
    "From: a@b\r\nSubject: x\r\n \r\r\nbody\r\n",
    "From: a@b\r\nno colon here\r\n\r\nbody\r\n",
    "junk line\r\nFrom: a@b\r\n\r\nbody\r\n",
    "From: a@b\r\nz\n \n body\r\n",
    "From: a@b\r\n\rFrom: evil@x\r\n\r\nbody\r\n",
    "From: a@b\r\n\x0cFrom: evil@x\r\n\r\nbody\r\n",
    " lead: continuation first\r\n\r\nbody\r\n",
];

#[test]
fn a_header_block_with_a_line_that_is_no_field_is_not_trusted() {
    for case in MALFORMED {
        assert!(!agrees(case.as_bytes()), "{case:?}");
    }
}

#[test]
fn dkim_and_the_reader_split_every_line_ending_mix_at_the_same_offset() {
    for case in TABLE {
        let raw = case.as_bytes();
        let ours = body_start(raw).unwrap_or(raw.len());
        assert_eq!(
            reader_body_start(raw),
            Some(ours),
            "{case:?}: the reader and DKIM disagree on where the body starts"
        );
        assert!(well_formed(raw) && agrees(raw), "{case:?}");
    }
}

proptest! {
    /// Whatever the mix of line-ending and whitespace bytes after a field,
    /// DKIM and the reader split at the same offset or the guard refuses.
    #[test]
    fn random_line_ending_mixes_split_at_the_same_offset(
        mix in proptest::collection::vec(
            prop_oneof![
                Just("\r"), Just("\n"), Just(" "), Just("\t"), Just("\x0c"),
                Just("X: y"), Just("z"),
            ],
            0..14,
        ),
    ) {
        let raw = format!("From: a@b\r\n{}body\r\n", mix.concat());
        let raw = raw.as_bytes();
        let ours = body_start(raw).unwrap_or(raw.len());
        // Where the header block is well formed the two offsets are equal;
        // where it is not, the guard refuses the message.
        prop_assert!(
            !well_formed(raw) && !agrees(raw) || reader_body_start(raw) == Some(ours),
            "{:?}",
            raw
        );
    }
}
