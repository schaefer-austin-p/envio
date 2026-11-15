//! The meeting card of an invite message. Pure: the worker passes what it
//! knows (the remembered answer, the Outbox state of our reply).

use envio_mime::{Invite, InviteMethod, NaiveParts, PartStat, Person, When, same_email};
use envio_store::InviteAnswer;

use crate::api::command::Answer;
use crate::api::view::{CardMethod, CardState, InviteCard, ReplyState};
use crate::text;

fn answer_of(stat: PartStat) -> Option<Answer> {
    match stat {
        PartStat::Accepted => Some(Answer::Accepted),
        PartStat::Tentative => Some(Answer::Tentative),
        PartStat::Declined => Some(Answer::Declined),
        PartStat::NeedsAction => None,
    }
}

/// The partstat a user's answer is sent as.
pub(crate) fn part_stat(answer: Answer) -> PartStat {
    match answer {
        Answer::Accepted => PartStat::Accepted,
        Answer::Tentative => PartStat::Tentative,
        Answer::Declined => PartStat::Declined,
    }
}

fn person_label(person: &Person) -> String {
    if person.name.is_empty() {
        person.email.clone()
    } else {
        person.name.clone()
    }
}

fn naive(p: NaiveParts) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        p.year, p.month, p.day, p.hour, p.minute, p.second
    )
}

fn date((y, m, d): (i32, u8, u8)) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Whether the user can answer: a request, not cancelled, with an organizer
/// to send the answer to.
pub(crate) fn can_answer(invite: &Invite) -> bool {
    invite.method == InviteMethod::Request && !invite.cancelled && invite.organizer.is_some()
}

/// The state, its text and the answer it shows.
fn state_of(
    invite: &Invite,
    remembered: Option<&InviteAnswer>,
    me: &str,
) -> (CardState, String, Option<Answer>) {
    match invite.method {
        InviteMethod::Cancel => (CardState::Cancelled, text::invite::cancelled(), None),
        InviteMethod::Reply => {
            let who = invite.attendees.iter().find(|a| a.partstat.is_some());
            let answer = who.and_then(|a| a.partstat).and_then(answer_of);
            let line = match (who, answer) {
                (Some(a), Some(answer)) if same_email(&a.email, me) => {
                    text::invite::you_answered(answer)
                }
                (Some(a), answer) => text::invite::attendee_answered(&person_label(a), answer),
                (None, answer) => text::invite::attendee_answered("", answer),
            };
            (CardState::Replied, line, answer)
        }
        InviteMethod::Request if invite.cancelled => {
            (CardState::Cancelled, text::invite::cancelled(), None)
        }
        InviteMethod::Request => match remembered {
            None => (CardState::Open, String::new(), None),
            Some(r) if invite.sequence > r.sequence => {
                (CardState::Updated, text::invite::updated(), None)
            }
            Some(r) => match Answer::parse(&r.answer) {
                Some(answer) => (
                    CardState::Answered,
                    text::invite::you_answered(answer),
                    Some(answer),
                ),
                None => (CardState::Open, String::new(), None),
            },
        },
    }
}

/// `remembered` is the stored answer for this invite's (account, uid), `reply`
/// where our reply to it stands, `me` the address of the account that got
/// the mail. `ics_index` is left at -1 for the caller to set.
pub(crate) fn card(
    invite: &Invite,
    remembered: Option<&InviteAnswer>,
    reply: ReplyState,
    me: &str,
) -> InviteCard {
    let (state, state_text, answer) = state_of(invite, remembered, me);
    let answered = state == CardState::Answered;
    let (mut start, mut end, mut start_date, mut end_date) = (0, 0, String::new(), String::new());
    let (mut all_day, mut floating, mut zone_unknown) = (false, false, false);
    match &invite.when {
        When::Timed {
            start: s,
            end: e,
            zone_unknown: z,
        } => {
            (start, end, zone_unknown) = (*s, *e, *z);
        }
        When::AllDay { start: s, end: e } => {
            all_day = true;
            (start_date, end_date) = (date(*s), date(*e));
        }
        When::Floating { start: s, end: e } => {
            floating = true;
            (start_date, end_date) = (naive(*s), naive(*e));
        }
    }
    InviteCard {
        method: match invite.method {
            InviteMethod::Request => CardMethod::Request,
            InviteMethod::Cancel => CardMethod::Cancel,
            InviteMethod::Reply => CardMethod::Reply,
        },
        state,
        state_text,
        answer,
        sending: answered && reply == ReplyState::Sending,
        send_failed: answered && reply == ReplyState::Failed,
        summary: invite.summary.clone(),
        location: invite.location.clone(),
        all_day,
        floating,
        start,
        end,
        start_date,
        end_date,
        zone_unknown,
        repeats: invite.repeats,
        organizer: invite
            .organizer
            .as_ref()
            .map(person_label)
            .unwrap_or_default(),
        can_answer: can_answer(invite),
        ics_index: -1,
    }
}

#[cfg(test)]
mod card_unit_tests {
    use super::*;

    const ME: &str = "me@example.org";

    fn person(email: &str, name: &str, partstat: Option<PartStat>) -> Person {
        Person {
            email: email.into(),
            name: name.into(),
            partstat,
        }
    }

    fn invite(method: InviteMethod) -> Invite {
        Invite {
            method,
            uid: "u1@example.org".into(),
            sequence: 0,
            summary: "Planning".into(),
            location: "Room 4".into(),
            when: When::Timed {
                start: 1_800_000_000,
                end: 1_800_003_600,
                zone_unknown: false,
            },
            dtstart_line: "DTSTART:20270115T080000Z".into(),
            recurrence_id: String::new(),
            recurrence_id_line: None,
            organizer_line: Some("ORGANIZER:mailto:ada@example.org".into()),
            organizer: Some(person("ada@example.org", "Ada", None)),
            attendees: Vec::new(),
            repeats: false,
            cancelled: method == InviteMethod::Cancel,
            raw: String::new(),
        }
    }

    fn remembered(sequence: u32, answer: &str) -> InviteAnswer {
        InviteAnswer {
            account: "a".into(),
            uid: "u1@example.org".into(),
            recurrence_id: String::new(),
            sequence,
            answer: answer.into(),
            answered_at_ms: 1,
            event_end_unix: 1_800_003_600,
            reply_message_id: "<r@example.org>".into(),
        }
    }

    #[test]
    fn an_unanswered_request_is_open_with_buttons() {
        let c = card(&invite(InviteMethod::Request), None, ReplyState::Gone, ME);
        assert_eq!((c.method, c.state), (CardMethod::Request, CardState::Open));
        assert!(c.can_answer && c.state_text.is_empty() && c.answer.is_none());
        assert_eq!((c.start, c.end), (1_800_000_000, 1_800_003_600));
        assert_eq!(c.organizer, "Ada");
    }

    #[test]
    fn an_answered_request_says_what_the_user_said() {
        let r = remembered(0, "tentative");
        let c = card(
            &invite(InviteMethod::Request),
            Some(&r),
            ReplyState::Gone,
            ME,
        );
        assert_eq!(c.state, CardState::Answered);
        assert_eq!(c.answer, Some(Answer::Tentative));
        assert_eq!(c.state_text, "You said maybe");
        assert!(c.can_answer && !c.sending && !c.send_failed);
    }

    #[test]
    fn a_newer_sequence_is_updated_and_the_same_one_is_answered() {
        let r = remembered(1, "accepted");
        let mut i = invite(InviteMethod::Request);
        i.sequence = 2;
        let c = card(&i, Some(&r), ReplyState::Sending, ME);
        assert_eq!(c.state, CardState::Updated);
        assert_eq!(c.state_text, "Updated");
        assert!(c.answer.is_none() && !c.sending);
        i.sequence = 1;
        assert_eq!(
            card(&i, Some(&r), ReplyState::Gone, ME).state,
            CardState::Answered
        );
        i.sequence = 0;
        assert_eq!(
            card(&i, Some(&r), ReplyState::Gone, ME).state,
            CardState::Answered
        );
    }

    #[test]
    fn the_outbox_state_shows_beside_the_answer() {
        let r = remembered(0, "accepted");
        let i = invite(InviteMethod::Request);
        let sending = card(&i, Some(&r), ReplyState::Sending, ME);
        assert!(sending.sending && !sending.send_failed);
        let failed = card(&i, Some(&r), ReplyState::Failed, ME);
        assert!(!failed.sending && failed.send_failed);
        assert_eq!(failed.state, CardState::Answered);
        // Nothing remembered: the Outbox state says nothing.
        let none = card(&i, None, ReplyState::Sending, ME);
        assert!(!none.sending);
    }

    #[test]
    fn a_cancel_has_no_buttons() {
        let r = remembered(0, "accepted");
        let c = card(
            &invite(InviteMethod::Cancel),
            Some(&r),
            ReplyState::Gone,
            ME,
        );
        assert_eq!(
            (c.state, c.method),
            (CardState::Cancelled, CardMethod::Cancel)
        );
        assert!(!c.can_answer);
        assert_eq!(c.state_text, "Cancelled");
    }

    #[test]
    fn a_request_marked_cancelled_reads_as_cancelled() {
        let mut i = invite(InviteMethod::Request);
        i.cancelled = true;
        let c = card(&i, None, ReplyState::Gone, ME);
        assert_eq!(c.state, CardState::Cancelled);
        assert!(!c.can_answer);
    }

    #[test]
    fn a_request_without_an_organizer_cannot_be_answered() {
        let mut i = invite(InviteMethod::Request);
        i.organizer = None;
        i.organizer_line = None;
        let c = card(&i, None, ReplyState::Gone, ME);
        assert_eq!(c.state, CardState::Open);
        assert!(!c.can_answer);
        assert_eq!(c.organizer, "");
    }

    #[test]
    fn a_reply_names_the_attendee() {
        let mut i = invite(InviteMethod::Reply);
        i.attendees = vec![person(
            "grace@example.org",
            "Grace",
            Some(PartStat::Accepted),
        )];
        let c = card(&i, None, ReplyState::Gone, ME);
        assert_eq!((c.state, c.method), (CardState::Replied, CardMethod::Reply));
        assert_eq!(c.state_text, "Grace accepted");
        assert_eq!(c.answer, Some(Answer::Accepted));
        assert!(!c.can_answer);
        i.attendees = vec![person("grace@example.org", "", Some(PartStat::Declined))];
        assert_eq!(
            card(&i, None, ReplyState::Gone, ME).state_text,
            "grace@example.org declined"
        );
    }

    #[test]
    fn a_reply_from_the_user_says_you() {
        let mut i = invite(InviteMethod::Reply);
        i.attendees = vec![person("ME@example.org", "Me", Some(PartStat::Accepted))];
        assert_eq!(
            card(&i, None, ReplyState::Gone, ME).state_text,
            "You accepted"
        );
    }

    #[test]
    fn times_cross_as_the_contract_says() {
        let mut i = invite(InviteMethod::Request);
        i.when = When::AllDay {
            start: (2027, 1, 15),
            end: (2027, 1, 16),
        };
        let c = card(&i, None, ReplyState::Gone, ME);
        assert!(c.all_day && !c.floating);
        assert_eq!((c.start, c.end), (0, 0));
        assert_eq!(
            (c.start_date.as_str(), c.end_date.as_str()),
            ("2027-01-15", "2027-01-16")
        );
        let p = |h| NaiveParts {
            year: 2027,
            month: 1,
            day: 15,
            hour: h,
            minute: 30,
            second: 0,
        };
        i.when = When::Floating {
            start: p(9),
            end: p(10),
        };
        let c = card(&i, None, ReplyState::Gone, ME);
        assert!(c.floating && !c.all_day);
        assert_eq!(c.start_date, "2027-01-15T09:30:00");
        assert_eq!(c.end_date, "2027-01-15T10:30:00");
        i.when = When::Timed {
            start: 5,
            end: 5,
            zone_unknown: true,
        };
        let c = card(&i, None, ReplyState::Gone, ME);
        assert!(c.zone_unknown && c.start_date.is_empty());
        i.repeats = true;
        assert!(card(&i, None, ReplyState::Gone, ME).repeats);
    }
}
