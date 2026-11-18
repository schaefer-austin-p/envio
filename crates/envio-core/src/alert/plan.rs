//! What a new-mail alert says, or whether there is none.

use envio_store::Settings;

use super::Alert;
use crate::api::event::NewMail;
use crate::text;

/// The alert for new Inbox mail, or `None` when the account's switch is off
/// or the app is focused (the user is looking at the mail). Without
/// "Show sender and subject" the card names the account and counts.
pub fn plan(
    mail: &NewMail,
    account_name: &str,
    settings: &Settings,
    focused: bool,
) -> Option<Alert> {
    if focused || mail.count == 0 || settings.notify_off.contains(&mail.account) {
        return None;
    }
    let shown = match (&mail.first, settings.notify_show_text, mail.count) {
        (Some(first), true, 1) => Some(first),
        _ => None,
    };
    let (summary, body) = match shown {
        Some((sender, subject)) => (sender.clone(), subject.clone()),
        None => (
            account_name.to_owned(),
            text::count::new_mail(u64::from(mail.count)),
        ),
    };
    Some(Alert {
        tag: format!("new-{}", mail.account),
        summary,
        body,
        account: mail.account.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(count: u32, first: Option<(&str, &str)>) -> NewMail {
        NewMail {
            account: "1700".into(),
            count,
            first: first.map(|(a, b)| (a.to_owned(), b.to_owned())),
        }
    }

    fn text_on() -> Settings {
        Settings {
            notify_show_text: true,
            ..Settings::default()
        }
    }

    #[test]
    fn switch_off_means_no_alert() {
        let off = Settings {
            notify_off: ["1700".to_owned()].into(),
            ..Settings::default()
        };
        assert_eq!(plan(&mail(2, None), "Work", &off, false), None);
        // Another account's switch does not matter.
        let other = Settings {
            notify_off: ["1701".to_owned()].into(),
            ..Settings::default()
        };
        assert!(plan(&mail(2, None), "Work", &other, false).is_some());
    }

    #[test]
    fn focused_means_no_alert() {
        assert_eq!(
            plan(&mail(2, None), "Work", &Settings::default(), true),
            None
        );
    }

    #[test]
    fn many_shows_account_and_count() {
        let alert = plan(&mail(3, None), "Work", &text_on(), false).unwrap();
        assert_eq!(alert.summary, "Work");
        assert_eq!(alert.body, text::count::new_mail(3));
        assert_eq!(alert.body, "3 new messages");
    }

    #[test]
    fn one_with_text_off_shows_account_and_count() {
        let one = mail(1, Some(("Ada", "Lunch")));
        let alert = plan(&one, "Work", &Settings::default(), false).unwrap();
        assert_eq!(alert.summary, "Work");
        assert_eq!(alert.body, "1 new message");
    }

    #[test]
    fn one_with_text_on_shows_sender_and_subject() {
        let one = mail(1, Some(("Ada", "Lunch")));
        let alert = plan(&one, "Work", &text_on(), false).unwrap();
        assert_eq!(alert.summary, "Ada");
        assert_eq!(alert.body, "Lunch");
    }

    #[test]
    fn one_with_text_on_but_no_text_falls_back_to_the_count() {
        let alert = plan(&mail(1, None), "Work", &text_on(), false).unwrap();
        assert_eq!(alert.summary, "Work");
        assert_eq!(alert.body, "1 new message");
    }

    #[test]
    fn the_tag_is_per_account() {
        let a = plan(&mail(1, None), "Work", &Settings::default(), false).unwrap();
        let mut other = mail(1, None);
        other.account = "1701".into();
        let b = plan(&other, "Home", &Settings::default(), false).unwrap();
        assert_eq!(a.tag, "new-1700");
        assert_ne!(a.tag, b.tag);
        assert_eq!(a.account, "1700");
    }

    #[test]
    fn nothing_new_means_no_alert() {
        assert_eq!(
            plan(&mail(0, None), "Work", &Settings::default(), false),
            None
        );
    }
}
