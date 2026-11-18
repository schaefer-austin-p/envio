//! The words for meeting invites: card states, the reply's subject and body.

use crate::api::command::Answer;
use crate::i18n::{fill, tr};

/// The reply's subject: "Accepted: Planning".
pub fn subject(answer: Answer, summary: &str) -> String {
    let template = match answer {
        Answer::Accepted => tr("Accepted: {summary}"),
        Answer::Tentative => tr("Tentative: {summary}"),
        Answer::Declined => tr("Declined: {summary}"),
    };
    fill(&template, &[("summary", summary)])
}

/// The reply's body when the user wrote no note. `name` is who answers.
pub fn reply_text(answer: Answer, name: &str) -> String {
    let template = match answer {
        Answer::Accepted => tr("{name} has accepted your invitation."),
        Answer::Tentative => tr("{name} has tentatively accepted your invitation."),
        Answer::Declined => tr("{name} has declined your invitation."),
    };
    fill(&template, &[("name", name)])
}

/// The card's line once the user has answered.
pub fn you_answered(answer: Answer) -> String {
    match answer {
        Answer::Accepted => tr("You accepted"),
        Answer::Tentative => tr("You said maybe"),
        Answer::Declined => tr("You declined"),
    }
}

/// The card's line for somebody's answer to our own invitation.
pub fn attendee_answered(name: &str, answer: Option<Answer>) -> String {
    let template = match answer {
        Some(Answer::Accepted) => tr("{name} accepted"),
        Some(Answer::Tentative) => tr("{name} said maybe"),
        Some(Answer::Declined) => tr("{name} declined"),
        None => tr("{name} replied"),
    };
    fill(&template, &[("name", name)])
}

/// The request changed after the user answered.
pub fn updated() -> String {
    tr("Updated")
}

pub fn cancelled() -> String {
    tr("Cancelled")
}

/// The reader's Undo toast after an answer.
pub fn answer_offer(answer: Answer) -> String {
    match answer {
        Answer::Accepted => tr("Accepting the invitation"),
        Answer::Tentative => tr("Answering maybe to the invitation"),
        Answer::Declined => tr("Declining the invitation"),
    }
}

/// The message holds no invite that can be answered.
pub fn cannot_answer() -> String {
    tr("This invitation can't be answered")
}

/// The reply could not be queued; nothing was changed.
pub fn not_queued() -> String {
    tr("Could not answer the invitation. Nothing was changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_name_the_answer_and_the_event() {
        assert_eq!(subject(Answer::Accepted, "Planning"), "Accepted: Planning");
        assert_eq!(
            subject(Answer::Tentative, "Planning"),
            "Tentative: Planning"
        );
        assert_eq!(subject(Answer::Declined, "Planning"), "Declined: Planning");
    }

    #[test]
    fn the_default_body_names_who_answers() {
        assert_eq!(
            reply_text(Answer::Declined, "Grace"),
            "Grace has declined your invitation."
        );
    }

    #[test]
    fn attendee_lines() {
        assert_eq!(
            attendee_answered("Grace", Some(Answer::Accepted)),
            "Grace accepted"
        );
        assert_eq!(attendee_answered("Grace", None), "Grace replied");
    }
}
