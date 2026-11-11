mod support;

use envio_core::{
    AccountSetup, Command, ComposeId, Event, ServerAddr, Settings, UndoId, UserAction,
};
use envio_net::Security;
use envio_ui::{Model, Op, Row, Signal, Ui};
use support::*;

fn selected(op: &Op) -> bool {
    match op {
        Op::Update {
            row: Row::Message(m),
            ..
        } => m.selected,
        _ => panic!("{op:?}"),
    }
}

#[test]
fn selection_survives_a_refresh() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20), msg(1, 10)], true);
    assert!(selected(changes(&ui.select(2, true), Model::Messages)[0]));
    assert!(selected(changes(&ui.select(1, true), Model::Messages)[0]));
    feed(
        &mut ui,
        Event::MessagesChanged {
            folders: vec![envio_core::FolderId(1)],
        },
    );
    // 4 arrives, 1 is deleted; 2 stays selected, 1 leaves the selection.
    let outs = feed(
        &mut ui,
        page(F1, None, vec![msg(4, 40), msg(3, 30), msg(2, 20)], true),
    );
    let ops = changes(&outs, Model::Messages);
    assert!(ops.iter().all(|op| !matches!(op, Op::Reset(_))), "{ops:?}");
    let (cmd, _) = ui.act_on_selection(UserAction::Archive).unwrap();
    let Command::Act { ids, .. } = cmd else {
        panic!("{cmd:?}")
    };
    assert_eq!(ids, [envio_core::MessageRowId(2)]);
}

#[test]
fn acting_on_the_selection_clears_it() {
    let mut ui = Ui::new();
    assert!(ui.act_on_selection(UserAction::Delete).is_none());
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
    ui.select(1, true);
    ui.select(2, true);
    let (cmd, outs) = ui.act_on_selection(UserAction::MarkRead(true)).unwrap();
    let Command::Act { ids, action } = cmd else {
        panic!("{cmd:?}")
    };
    assert_eq!(ids.len(), 2);
    assert_eq!(action, UserAction::MarkRead(true));
    let ops = changes(&outs, Model::Messages);
    assert_eq!(ops.len(), 2);
    assert!(ops.iter().all(|op| !selected(op)));
    assert!(ui.act_on_selection(UserAction::Delete).is_none());
}

fn setup(confirm: bool) -> AccountSetup {
    let addr = ServerAddr {
        host: "mail.example.org".into(),
        port: 993,
        security: Security::Tls,
    };
    AccountSetup {
        name: "Ada".into(),
        email: "ada@example.org".into(),
        username: "ada".into(),
        imap: addr.clone(),
        smtp: addr,
        source: None,
        needs_confirm: confirm,
    }
}

#[test]
fn one_shot_events_become_signals() {
    let mut ui = Ui::new();
    let c = ComposeId(4);
    let events = vec![
        Event::Notice { text: "n".into() },
        Event::StoreProblem { text: "p".into() },
        Event::Detected {
            email: "ada@example.org".into(),
            setup: setup(true),
            text: "t".into(),
        },
        Event::UndoOffer {
            undo: UndoId(1),
            text: "Archived".into(),
            expires_at_ms: 0,
        },
        Event::Queued {
            compose: c,
            item: "x".into(),
            text: "Sent".into(),
        },
        Event::Settings(Settings::default()),
    ];
    let mut got = Vec::new();
    for e in events {
        let outs = feed(&mut ui, e);
        let s = signals(&outs);
        assert_eq!(s.len(), 1, "{outs:?}");
        got.push(s[0].clone());
    }
    assert_eq!(got[0], Signal::Notice("n".into()));
    assert!(matches!(
        &got[2],
        Signal::Detected {
            needs_confirm: true,
            ..
        }
    ));
    assert!(
        matches!(got[4], Signal::Queued { compose, ref text } if compose == c && text == "Sent")
    );
    assert!(matches!(got[5], Signal::Settings(_)));
}

#[test]
fn a_search_failure_shows_only_for_the_current_search() {
    let mut ui = Ui::new();
    let all = envio_core::SearchScope::All;
    let failed = |q: &str| Event::SearchFailed {
        scope: envio_core::SearchScope::All,
        query: q.into(),
        text: "down".into(),
    };
    assert!(feed(&mut ui, failed("q")).is_empty());
    let _ = ui.search(all, "q".into(), true);
    assert!(feed(&mut ui, failed("old")).is_empty());
    let outs = feed(&mut ui, failed("q"));
    assert_eq!(signals(&outs), [&Signal::SearchFailed("down".into())]);
}

#[test]
fn switching_lists_or_searching_clears_the_marks() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
    ui.select(2, true);
    let (_, outs) = ui.open_list(envio_core::ListRef::Inboxes);
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Update { .. }]) && !selected(ops[0]));
    assert!(ui.act_on_selection(UserAction::Delete).is_none());

    opened(&mut ui, F1, vec![msg(2, 20)], true);
    ui.select(2, true);
    let (_, outs) = ui.search(envio_core::SearchScope::All, "q".into(), false);
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Update { .. }]) && !selected(ops[0]));
    assert!(ui.act_on_selection(UserAction::Delete).is_none());
}

fn two_accounts(ui: &mut Ui) {
    feed(
        ui,
        Event::Started {
            accounts: vec![info("a"), info("b")],
            cache: envio_core::CacheState::Existing,
        },
    );
}

fn server_results(query: &str) -> Event {
    Event::SearchResults {
        scope: envio_core::SearchScope::All,
        query: query.into(),
        server: true,
        rows: vec![msg(1, 10)],
    }
}

#[test]
fn a_server_search_is_done_with_its_results_or_when_every_account_failed() {
    let mut ui = Ui::new();
    two_accounts(&mut ui);
    let failed = |text: &str| Event::SearchFailed {
        scope: envio_core::SearchScope::All,
        query: "q".into(),
        text: text.into(),
    };
    let _ = ui.search(envio_core::SearchScope::All, "q".into(), true);
    let outs = feed(&mut ui, server_results("q"));
    assert_eq!(signals(&outs), [&Signal::SearchDone]);

    // One account failing is not the end: the other still answers.
    let _ = ui.search(envio_core::SearchScope::All, "q".into(), true);
    let outs = feed(&mut ui, failed("a down"));
    assert_eq!(signals(&outs), [&Signal::SearchFailed("a down".into())]);
    let outs = feed(&mut ui, failed("b down"));
    assert_eq!(
        signals(&outs),
        [&Signal::SearchFailed("b down".into()), &Signal::SearchDone]
    );

    // A local search is never "done" in this sense.
    let _ = ui.search(envio_core::SearchScope::All, "q".into(), false);
    let outs = feed(
        &mut ui,
        Event::SearchResults {
            scope: envio_core::SearchScope::All,
            query: "q".into(),
            server: false,
            rows: Vec::new(),
        },
    );
    assert!(signals(&outs).is_empty());
}

#[test]
fn a_cancelled_server_search_drops_late_results() {
    let mut ui = Ui::new();
    two_accounts(&mut ui);
    let _ = ui.search(envio_core::SearchScope::All, "q".into(), true);
    // Cancel: the page asks the same question locally.
    let _ = ui.search(envio_core::SearchScope::All, "q".into(), false);
    let outs = feed(&mut ui, server_results("q"));
    assert!(outs.is_empty());
}
