mod support;

use envio_core::{Command, Event, SearchScope};
use envio_ui::{OlderRow, Output, Signal, Ui};
use support::*;

/// The `OlderRow` states the outputs announce, in order.
fn rows(outs: &[Output]) -> Vec<OlderRow> {
    signals(outs)
        .into_iter()
        .filter_map(|s| match s {
            Signal::OlderRow(row) => Some(*row),
            _ => None,
        })
        .collect()
}

fn loaded(fetched: usize, exhausted: bool) -> Event {
    Event::OlderLoaded {
        list: F1,
        fetched,
        exhausted,
    }
}

#[test]
fn older_row_is_ready_at_the_end_of_a_folder() {
    let mut ui = Ui::new();
    let _ = ui.open_list(F1);
    // Not at the end yet: nothing to show.
    let first = feed(&mut ui, page(F1, None, vec![msg(2, 20), msg(1, 10)], false));
    assert!(rows(&first).is_empty());
    let _ = ui.more().unwrap();
    let last = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, last)), [OlderRow::Ready]);
    // The unified inbox has it too.
    let mut ui = Ui::new();
    let inboxes = envio_core::ListRef::Inboxes;
    let outs = opened(&mut ui, inboxes, vec![msg(1, 10)], true);
    assert_eq!(rows(&outs), [OlderRow::Ready]);
}

#[test]
fn older_row_is_hidden_for_search() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let (_, outs) = ui.search(SearchScope::All, "zebra".into(), false);
    assert_eq!(rows(&outs), [OlderRow::Hidden]);
    // A late page of the list behind the search does not bring it back.
    let late = feed(&mut ui, page(F1, None, vec![msg(1, 10)], true));
    assert!(rows(&late).is_empty());
    assert!(ui.load_older().is_none());
    // Opening a list again does.
    let outs = opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert_eq!(rows(&outs), [OlderRow::Ready]);
}

#[test]
fn load_older_shows_loading_then_done_when_exhausted() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let (command, outs) = ui.load_older().unwrap();
    assert!(matches!(command, Command::LoadOlder { list } if list == F1));
    assert_eq!(rows(&outs), [OlderRow::Loading]);
    // Pressed again while loading: nothing.
    assert!(ui.load_older().is_none());
    // Nothing new was cached: nothing to page in.
    assert_eq!(rows(&feed(&mut ui, loaded(0, true))), [OlderRow::Done]);
    assert!(ui.load_older().is_none());
}

#[test]
fn older_rows_are_paged_in_before_the_row_returns() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    // Rows arrived: the list is not at its end until they are shown.
    assert_eq!(rows(&feed(&mut ui, loaded(9, false))), [OlderRow::Hidden]);
    assert!(ui.more().is_some());
    let last = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, last)), [OlderRow::Ready]);
}

#[test]
fn older_failure_returns_to_ready() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    let outs = feed(
        &mut ui,
        Event::OlderFailed {
            list: F1,
            text: "offline".into(),
        },
    );
    assert_eq!(rows(&outs), [OlderRow::Ready]);
    assert!(
        signals(&outs)
            .iter()
            .any(|s| matches!(s, Signal::Notice(t) if t == "offline"))
    );
    // And it can be tried again.
    assert!(ui.load_older().is_some());
}
