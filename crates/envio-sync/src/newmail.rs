//! The new-mail rule: which of the messages an Inbox sync stored should
//! notify. Pure: the driver passes in what it stored and what it knew
//! before. The first sync of an account and a UIDVALIDITY reset never
//! notify.

use envio_mime::Address;

/// One message an Inbox sync stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRow {
    pub uid: u32,
    pub seen: bool,
    /// The first From address (its name is the sender's display name).
    pub from: Address,
    pub subject: String,
}

/// What an Inbox sync found that is new to the user.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NewMail {
    pub count: u32,
    /// (sender display name or address, subject); set only when
    /// `count == 1`.
    pub first: Option<(String, String)>,
}

/// New mail among `stored`: UIDs at or above `prev_uidnext`, not `\Seen`,
/// not from one of `own`. `prev_uidnext` is `None` before the folder's
/// first finished sync.
pub fn count(
    prev_uidnext: Option<u32>,
    uidvalidity_changed: bool,
    stored: &[NewRow],
    own: &[Address],
) -> NewMail {
    let Some(prev) = prev_uidnext.filter(|_| !uidvalidity_changed) else {
        return NewMail::default();
    };
    let fresh: Vec<&NewRow> = stored
        .iter()
        .filter(|row| row.uid >= prev && !row.seen && !is_own(&row.from, own))
        .collect();
    let first = match fresh[..] {
        [row] => Some((
            row.from
                .name
                .clone()
                .unwrap_or_else(|| row.from.email.clone()),
            row.subject.clone(),
        )),
        _ => None,
    };
    NewMail {
        count: u32::try_from(fresh.len()).unwrap_or(u32::MAX),
        first,
    }
}

fn is_own(from: &Address, own: &[Address]) -> bool {
    own.iter()
        .any(|mine| mine.email.eq_ignore_ascii_case(&from.email))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(name: Option<&str>, email: &str) -> Address {
        Address {
            name: name.map(str::to_owned),
            email: email.to_owned(),
        }
    }

    fn row(uid: u32, seen: bool, email: &str) -> NewRow {
        NewRow {
            uid,
            seen,
            from: addr(Some("Ada"), email),
            subject: format!("subject {uid}"),
        }
    }

    #[test]
    fn no_previous_uidnext_never_notifies() {
        let got = count(None, false, &[row(1, false, "a@x.org")], &[]);
        assert_eq!(got, NewMail::default());
    }

    #[test]
    fn a_uidvalidity_reset_never_notifies() {
        let got = count(Some(1), true, &[row(5, false, "a@x.org")], &[]);
        assert_eq!(got, NewMail::default());
    }

    #[test]
    fn seen_mail_is_not_new() {
        let got = count(Some(1), false, &[row(5, true, "a@x.org")], &[]);
        assert_eq!(got.count, 0);
    }

    #[test]
    fn mail_from_my_own_address_is_not_new() {
        let own = [addr(None, "Me@Example.org")];
        let got = count(Some(1), false, &[row(5, false, "me@example.org")], &own);
        assert_eq!(got.count, 0);
    }

    #[test]
    fn uids_below_the_old_uidnext_are_not_new() {
        let stored = [row(4, false, "a@x.org"), row(5, false, "a@x.org")];
        let got = count(Some(5), false, &stored, &[]);
        assert_eq!(got.count, 1);
    }

    #[test]
    fn one_new_message_carries_sender_and_subject() {
        let got = count(Some(1), false, &[row(5, false, "a@x.org")], &[]);
        assert_eq!(got.count, 1);
        assert_eq!(got.first, Some(("Ada".into(), "subject 5".into())));
    }

    #[test]
    fn three_new_messages_carry_no_text() {
        let stored = [
            row(5, false, "a@x.org"),
            row(6, false, "b@x.org"),
            row(7, false, "c@x.org"),
        ];
        let got = count(Some(1), false, &stored, &[]);
        assert_eq!(got.count, 3);
        assert_eq!(got.first, None);
    }
}
