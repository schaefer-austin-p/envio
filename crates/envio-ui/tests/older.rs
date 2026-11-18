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

fn reset(folder: i64) -> Event {
    Event::OlderReset {
        folder: envio_core::FolderId(folder),
        inbox: false,
    }
}

fn inbox_reset(folder: i64) -> Event {
    Event::OlderReset {
        folder: envio_core::FolderId(folder),
        inbox: true,
    }
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
    // Search results are their own page with no row; a search leaves the
    // row of the list behind it alone, so Back finds it still there.
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let (_, outs) = ui.search(SearchScope::All, "zebra".into(), false);
    assert!(rows(&outs).is_empty());
    let late = feed(&mut ui, page(F1, None, vec![msg(1, 10)], true));
    assert!(rows(&late).is_empty());
    assert!(ui.load_older().is_some());
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

fn changed() -> Event {
    Event::MessagesChanged {
        folders: vec![envio_core::FolderId(1)],
    }
}

/// The `ListMessages` pages the outputs ask after a cursor.
fn asked(outs: &[Output]) -> Vec<Option<envio_core::Cursor>> {
    outs.iter()
        .filter_map(|o| match o {
            Output::Send(Command::ListMessages { after, .. }) => Some(*after),
            _ => None,
        })
        .collect()
}

#[test]
fn the_ui_pages_in_the_rows_load_older_fetched_itself() {
    for (exhausted, last) in [(false, OlderRow::Ready), (true, OlderRow::Done)] {
        let mut ui = Ui::new();
        opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
        let _ = ui.load_older().unwrap();
        let outs = feed(&mut ui, loaded(50, exhausted));
        assert_eq!(rows(&outs), [OlderRow::Hidden]);
        // The refresh after MessagesChanged; the view never calls more().
        let mut seen = asked(&outs);
        let outs = feed(&mut ui, changed());
        seen.extend(asked(&outs));
        let outs = feed(&mut ui, page(F1, None, vec![msg(2, 20), msg(1, 10)], false));
        seen.extend(asked(&outs));
        assert!(seen.contains(&Some(cursor(1, 10))), "{seen:?}");
        // The older rows arrive at the end of the list.
        let page_in = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
        assert_eq!(rows(&feed(&mut ui, page_in)), [last]);
    }
}

#[test]
fn a_batch_loaded_during_a_search_pages_in_once() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    // A search starts before the batch is reported: the list behind it
    // still takes the batch's rows, once.
    let _ = ui.search(SearchScope::All, "zebra".into(), false);
    let outs = feed(&mut ui, loaded(9, false));
    assert_eq!(asked(&outs), [Some(cursor(1, 10))]);
    let page_in = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, page_in)), [OlderRow::Ready]);
    // Later refreshes of the list ask for nothing extra.
    let outs = feed(&mut ui, page(F1, None, vec![msg(2, 20), msg(1, 10)], true));
    assert!(asked(&outs).is_empty());
    assert!(asked(&feed(&mut ui, changed())).iter().all(Option::is_none));
}

#[test]
fn an_older_reset_brings_the_load_older_row_back_from_done() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    assert_eq!(rows(&feed(&mut ui, loaded(0, true))), [OlderRow::Done]);
    // The sync window changed: the older rows go, so more can be loaded.
    assert_eq!(rows(&feed(&mut ui, reset(1))), [OlderRow::Ready]);
    assert!(ui.load_older().is_some());
}

#[test]
fn an_older_reset_of_another_folder_leaves_the_list_alone() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    assert_eq!(rows(&feed(&mut ui, loaded(0, true))), [OlderRow::Done]);
    // Folder 2 was reset; the shown folder 1 still has nothing older.
    assert!(rows(&feed(&mut ui, reset(2))).is_empty());
    assert!(ui.load_older().is_none());
    // The unified inbox shows the inboxes only: another folder's reset is
    // not its business, an inbox's is.
    let mut ui = Ui::new();
    let inboxes = envio_core::ListRef::Inboxes;
    opened(&mut ui, inboxes, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    let done = Event::OlderLoaded {
        list: inboxes,
        fetched: 0,
        exhausted: true,
    };
    assert_eq!(rows(&feed(&mut ui, done)), [OlderRow::Done]);
    assert!(rows(&feed(&mut ui, reset(2))).is_empty());
    assert_eq!(rows(&feed(&mut ui, inbox_reset(2))), [OlderRow::Ready]);
}

#[test]
fn an_inbox_reset_during_a_unified_load_keeps_it_from_ending_done() {
    let mut ui = Ui::new();
    let inboxes = envio_core::ListRef::Inboxes;
    opened(&mut ui, inboxes, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    // One account's inbox is reset while the batch loads; the batch's
    // "exhausted" may stem from before the reset.
    let _ = feed(&mut ui, inbox_reset(2));
    let done = Event::OlderLoaded {
        list: inboxes,
        fetched: 0,
        exhausted: true,
    };
    assert_eq!(rows(&feed(&mut ui, done)), [OlderRow::Ready]);
}

// Reaching the end of the list loads older mail by itself (#130).

fn offline() -> Event {
    Event::OlderFailed {
        list: F1,
        text: "offline".into(),
    }
}

#[test]
fn reaching_the_end_loads_one_older_page() {
    let mut ui = Ui::new();
    // Not at the end of the list yet: the row is hidden, nothing is asked.
    let _ = ui.open_list(F1);
    let _ = feed(&mut ui, page(F1, None, vec![msg(2, 20), msg(1, 10)], false));
    assert!(ui.load_older_near_end().is_none());
    let _ = ui.more().unwrap();
    let last = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, last)), [OlderRow::Ready]);
    // One scroll can fire several times (a fling): exactly one page is asked.
    let (command, outs) = ui.load_older_near_end().unwrap();
    assert!(matches!(command, Command::LoadOlder { list } if list == F1));
    assert_eq!(rows(&outs), [OlderRow::Loading]);
    assert!(ui.load_older_near_end().is_none());
    assert!(ui.load_older_near_end().is_none());
}

#[test]
fn a_page_that_arrived_allows_the_next_automatic_load() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    let _ = feed(&mut ui, loaded(50, false));
    let page_in = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, page_in)), [OlderRow::Ready]);
    assert!(ui.load_older_near_end().is_some());
}

#[test]
fn a_failed_automatic_load_is_not_retried_but_the_row_still_works() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    assert_eq!(rows(&feed(&mut ui, offline())), [OlderRow::Ready]);
    // Scrolling again does not loop; tapping the row is the fallback.
    assert!(ui.load_older_near_end().is_none());
    assert!(ui.load_older().is_some());
}

#[test]
fn an_empty_automatic_batch_is_not_asked_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    // Nothing cached and the server may have more: no automatic loop.
    assert_eq!(rows(&feed(&mut ui, loaded(0, false))), [OlderRow::Ready]);
    assert!(ui.load_older_near_end().is_none());
}

fn other() -> envio_core::ListRef {
    envio_core::ListRef::Folder(envio_core::FolderId(2))
}

#[test]
fn a_reopened_list_the_server_has_no_more_for_is_not_auto_loaded_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    assert_eq!(rows(&feed(&mut ui, loaded(0, true))), [OlderRow::Done]);
    // Back to the list: the row is Ready again, but scrolling asks nothing.
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert!(ui.load_older_near_end().is_none());
    // The row still works when tapped.
    assert!(ui.load_older().is_some());
    // Another list is not affected.
    opened(&mut ui, other(), vec![msg(5, 50)], true);
    assert!(ui.load_older_near_end().is_some());
}

#[test]
fn a_reopened_list_whose_last_load_failed_is_not_auto_loaded_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    let _ = feed(&mut ui, offline());
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert!(
        ui.load_older_near_end().is_none(),
        "no toast nobody asked for"
    );
    assert!(ui.load_older().is_some());
}

#[test]
fn a_tap_that_brings_rows_allows_auto_loading_the_list_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    let _ = feed(&mut ui, offline());
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older().unwrap();
    let _ = feed(&mut ui, loaded(20, false));
    let page_in = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(rows(&feed(&mut ui, page_in)), [OlderRow::Ready]);
    assert!(ui.load_older_near_end().is_some());
}

#[test]
fn a_dropped_older_window_allows_auto_loading_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    let _ = ui.load_older_near_end().unwrap();
    let _ = feed(&mut ui, offline());
    assert!(ui.load_older_near_end().is_none());
    // The sync window changed (the older rows went): the state is new.
    let _ = feed(&mut ui, reset(1));
    assert!(ui.load_older_near_end().is_some());
    // The same after Done, and for a list that was blocked while not shown.
    let _ = feed(&mut ui, loaded(0, true));
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert!(ui.load_older_near_end().is_none());
    let _ = feed(&mut ui, reset(1));
    assert!(ui.load_older_near_end().is_some());
}
