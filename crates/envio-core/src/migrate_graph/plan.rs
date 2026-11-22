//! The #342 migration's next move, from the saved step and what the account
//! holds. Pure: the driver (`worker/migrate_graph.rs`) does the I/O.
//!
//! Steps, as saved in `Migration::step`:
//! 1. replay the Journal on IMAP, and file the `Accepted` Outbox items;
//! 2. clear the account's cache rows and bodies;
//! 3. switch the backend (drop the IMAP token and servers);
//! 4. is in memory only (`Live` reset) and runs right after 3.

use envio_store::Migration;

/// Failed runs (replay, folder list, filing) after which "Switch anyway" is
/// offered.
pub const TRIES: u32 = 3;

/// Filing of an `Accepted` item is asked for again at most this often: a
/// refused Sent copy is no error, so each try needs a real delay or the 3
/// tries to Stuck would pass within a second (#439).
pub const FILE_TICK_MS: i64 = 10_000;

/// When the move may next ask for filing, after it did at `armed_ms`.
pub fn file_due_ms(armed_ms: i64) -> i64 {
    armed_ms.saturating_add(FILE_TICK_MS)
}

/// Filing may be asked for now: the tick has come, or the clock went back
/// past the last ask (a step back must not starve the item).
pub fn file_due(armed_ms: i64, now: i64) -> bool {
    now < armed_ms || now >= file_due_ms(armed_ms)
}

pub const REPLAY: u8 = 1;
pub const CLEAR: u8 = 2;
pub const SWITCH: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStep {
    /// Ask for a replay (and filing) on IMAP.
    Replay,
    /// Nothing to do now: a run is failing and backs off, or an `Accepted`
    /// item is still to be filed.
    Wait,
    /// Replay or filing failed `TRIES` times: the user chooses Keep waiting or
    /// Switch anyway. Nothing is asked for meanwhile.
    Stuck,
    /// Run step 2.
    Clear,
    /// Run step 3, then step 4.
    Switch,
    /// Not a migration step (an unknown saved step): clear the marker.
    Done,
}

/// `replay_failed`: the last replay run failed and has not run again.
pub fn next(
    m: &Migration,
    journal_empty: bool,
    accepted_left: bool,
    replay_failed: bool,
) -> MigrationStep {
    match m.step {
        REPLAY => match (journal_empty, accepted_left) {
            (true, false) => MigrationStep::Clear,
            // The Sent copy of an accepted item is refused again and again
            // (quota, Sent not selectable): the mail was sent, so Switch
            // anyway drops nothing.
            (true, true) if m.failures >= TRIES => MigrationStep::Stuck,
            (true, true) => MigrationStep::Wait,
            (false, _) if m.failures >= TRIES => MigrationStep::Stuck,
            (false, _) if replay_failed => MigrationStep::Wait,
            (false, _) => MigrationStep::Replay,
        },
        CLEAR => MigrationStep::Clear,
        SWITCH => MigrationStep::Switch,
        _ => MigrationStep::Done,
    }
}

/// "Switch anyway" is offered only at step 1 after `TRIES` failed runs.
pub fn can_switch_anyway(m: &Migration) -> bool {
    m.step == REPLAY && m.failures >= TRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(step: u8, failures: u32) -> Migration {
        Migration { step, failures }
    }

    #[test]
    fn an_empty_journal_and_nothing_to_file_goes_on_to_clearing() {
        assert_eq!(next(&at(1, 0), true, false, false), MigrationStep::Clear);
    }

    #[test]
    fn filing_is_asked_for_once_per_tick() {
        assert_eq!(file_due_ms(5), 5 + FILE_TICK_MS);
    }

    #[test]
    fn an_accepted_item_still_to_file_holds_step_one() {
        assert_eq!(next(&at(1, 0), true, true, false), MigrationStep::Wait);
    }

    #[test]
    fn an_item_that_stays_unfiled_for_three_tries_is_stuck_not_waiting_forever() {
        assert_eq!(next(&at(1, 2), true, true, false), MigrationStep::Wait);
        for failed in [true, false] {
            assert_eq!(next(&at(1, 3), true, true, failed), MigrationStep::Stuck);
        }
        assert!(can_switch_anyway(&at(1, 3)));
    }

    #[test]
    fn pending_actions_are_replayed_first() {
        assert_eq!(next(&at(1, 0), false, false, false), MigrationStep::Replay);
    }

    #[test]
    fn a_failed_run_backs_off_instead_of_asking_again() {
        assert_eq!(next(&at(1, 1), false, false, true), MigrationStep::Wait);
    }

    #[test]
    fn three_failures_are_stuck_whatever_the_last_run_did() {
        for failed in [true, false] {
            assert_eq!(next(&at(1, 3), false, false, failed), MigrationStep::Stuck);
        }
        assert_eq!(next(&at(1, 2), false, false, false), MigrationStep::Replay);
    }

    #[test]
    fn later_steps_resume_where_they_were_saved() {
        // The journal and the Outbox no longer matter once step 1 is over.
        assert_eq!(next(&at(2, 0), false, true, true), MigrationStep::Clear);
        assert_eq!(next(&at(3, 0), false, true, true), MigrationStep::Switch);
        assert_eq!(next(&at(9, 0), true, false, false), MigrationStep::Done);
    }

    #[test]
    fn switch_anyway_needs_step_one_and_three_failures() {
        assert!(!can_switch_anyway(&at(1, 2)));
        assert!(can_switch_anyway(&at(1, 3)));
        assert!(!can_switch_anyway(&at(2, 3)));
    }
}
