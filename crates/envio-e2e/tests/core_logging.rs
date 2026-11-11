//! The logger is global, so this binary owns it: a real sign-in and first
//! sync against Stalwart at Trace, and the log must not hold the password
//! (#37). Skips without Stalwart, unless ENVIO_REQUIRE_STALWART=1.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex};

use envio_core::{AccountStatus, Event, FolderRole, logging};
use log::LevelFilter;
use support::core_run::{CoreRun, NET};
use support::stalwart::Stalwart;

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn trace_logging_of_a_real_sign_in_never_records_the_password() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let buffer = Shared::default();
    logging::init_with_writer(Box::new(buffer.clone()), LevelFilter::Trace).unwrap();
    let run = CoreRun::start(LevelFilter::Trace);
    let account = run.add_account(&server, "heidi");
    run.folder(&account, FolderRole::Inbox);
    run.wait(NET, |event| match event {
        Event::AccountStatus {
            status: AccountStatus::Ready,
            ..
        } => Some(()),
        _ => None,
    });
    drop(run);
    let log = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(log.lines().count() > 5, "hardly any log: {log}");
    assert!(
        !log.contains("heidi-secret-1"),
        "the password is in the log"
    );
    assert!(!log.contains("LOGIN"), "a LOGIN command is in the log");
}
