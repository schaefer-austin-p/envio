mod support;

use envio_core::{
    CacheState, Event, MessageHeader, MessageRowId, MessageView, Service, Suggestion, UserAction,
};
use envio_ui::{Model, Row, Signal, Ui};
use support::*;

fn message_ids(ui: &Ui, model: Model) -> Vec<i64> {
    ui.rows(model)
        .iter()
        .map(|r| match r {
            Row::Message(m) => m.id,
            _ => panic!(),
        })
        .collect()
}

#[test]
fn rows_follow_every_change() {
    let mut ui = Ui::new();
    feed(
        &mut ui,
        Event::Started {
            accounts: vec![info("a"), info("b")],
            cache: CacheState::Existing,
        },
    );
    assert_eq!(ui.rows(Model::Accounts).len(), 2);

    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20), msg(1, 10)], true);
    assert_eq!(message_ids(&ui, Model::Messages), [3, 2, 1]);

    // A refresh inserts and removes; a selection updates a row.
    feed(
        &mut ui,
        Event::MessagesChanged {
            folders: vec![envio_core::FolderId(1)],
        },
    );
    feed(
        &mut ui,
        page(F1, None, vec![msg(4, 40), msg(3, 30), msg(2, 20)], true),
    );
    assert_eq!(message_ids(&ui, Model::Messages), [4, 3, 2]);
    ui.select(3, true);
    let Row::Message(m) = &ui.rows(Model::Messages)[1] else {
        panic!()
    };
    assert!(m.selected);

    // Acting clears the marks.
    ui.act_on_selection(UserAction::Archive).unwrap();
    let Row::Message(m) = &ui.rows(Model::Messages)[1] else {
        panic!()
    };
    assert!(!m.selected);

    feed(
        &mut ui,
        Event::Suggestions {
            query: "x".into(),
            items: vec![Suggestion {
                name: "N".into(),
                address: "n@example.org".into(),
                from_contacts: true,
                last_seen: None,
            }],
        },
    );
    assert_eq!(ui.rows(Model::Suggestions).len(), 1);
    // Each row says which query it answers.
    assert_eq!(
        ui.rows(Model::Suggestions)[0].value("query"),
        Some(envio_ui::RowValue::Str("x".into()))
    );
    assert!(ui.rows(Model::Outbox).is_empty());
}

#[test]
fn accepted_certificates_are_kept_until_a_new_setup() {
    let mut ui = Ui::new();
    ui.accept_setup_certificate(Service::Imap, "ab".into());
    assert_eq!(ui.accepted(), [(Service::Imap, "ab".to_string())]);
    ui.clear_accepted();
    assert!(ui.accepted().is_empty());
}

#[test]
fn a_setup_for_another_address_drops_the_acceptances() {
    let mut ui = Ui::new();
    ui.begin_setup("ann@example.org");
    ui.accept_setup_certificate(Service::Smtp, "ab".into());
    // The same address, in another spelling, keeps them (a retry).
    ui.begin_setup(" Ann@Example.org ");
    assert_eq!(ui.accepted().len(), 1);
    // Another address does not inherit them.
    ui.begin_setup("bob@example.net");
    assert!(ui.accepted().is_empty());
    ui.accept_setup_certificate(Service::Imap, "cd".into());
    // A new detect clears them even for the same address.
    ui.clear_accepted();
    ui.begin_setup("bob@example.net");
    assert!(ui.accepted().is_empty());
}

#[test]
fn an_opened_message_names_its_id() {
    let mut ui = Ui::new();
    let view = MessageView {
        header: MessageHeader {
            from: "f".into(),
            from_address: "f@example.org".into(),
            to: vec![],
            cc: vec![],
            subject: "s".into(),
            date: None,
            account: "a".into(),
            sender_known: false,
            sender_verified: false,
            sender_checkable: false,
        },
        html: String::new(),
        raw: false,
        blocked_images: 0,
        remote_images: vec![],
        own_colours: false,
        attachments: vec![],
        invite: None,
        is_draft: false,
    };
    let outs = feed(
        &mut ui,
        Event::MessageOpened {
            id: MessageRowId(7),
            view,
        },
    );
    assert!(matches!(
        signals(&outs)[..],
        [Signal::MessageOpened { id: 7, .. }]
    ));
}

#[test]
fn message_opened_carries_the_invite_card() {
    use envio_core::{CardMethod, CardState, InviteCard};
    let mut ui = Ui::new();
    let card = InviteCard {
        method: CardMethod::Request,
        state: CardState::Open,
        state_text: String::new(),
        answer: None,
        sending: false,
        send_failed: false,
        summary: "Standup".into(),
        location: String::new(),
        all_day: false,
        floating: false,
        start: 1_800_000_000,
        end: 1_800_003_600,
        start_date: String::new(),
        end_date: String::new(),
        zone_unknown: false,
        repeats: false,
        organizer: "Grace".into(),
        organizer_email: "grace@example.org".into(),
        can_answer: true,
        can_add_to_calendar: true,
        ics_index: 0,
    };
    let view = MessageView {
        header: MessageHeader {
            from: "f".into(),
            from_address: "f@example.org".into(),
            to: vec![],
            cc: vec![],
            subject: "s".into(),
            date: None,
            account: "a".into(),
            sender_known: false,
            sender_verified: false,
            sender_checkable: false,
        },
        html: String::new(),
        raw: false,
        blocked_images: 0,
        remote_images: vec![],
        own_colours: false,
        attachments: vec![],
        invite: Some(card.clone()),
        is_draft: false,
    };
    let outs = feed(
        &mut ui,
        Event::MessageOpened {
            id: MessageRowId(7),
            view,
        },
    );
    match &signals(&outs)[..] {
        [Signal::MessageOpened { view, .. }] => assert_eq!(view.invite, Some(card)),
        other => panic!("{other:?}"),
    }
}
