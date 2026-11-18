//! A background connect for IDLE that hangs does not hold up commands (#239).

mod support;

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use envio_core::{Command, Event};
use envio_store::FolderRole;
use support::Harness;

/// A server that accepts connections and never says a word: the TLS
/// handshake hangs until the client's own timeout (20 s).
fn hanging_server() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&accepted);
    thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            // The second connection is the foreground's: hang up on it and
            // on the first, so the core is not left in a 20 s timeout.
            if count.fetch_add(1, Ordering::SeqCst) >= 1 {
                held.clear();
                continue;
            }
            held.push(stream);
        }
    });
    (port, accepted)
}

fn wait_for(what: &str, within: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(20));
    }
}

/// A started background core whose first IDLE connect is hanging.
fn hanging() -> (Harness, String, Arc<AtomicUsize>) {
    let (port, accepted) = hanging_server();
    let mut h = Harness::unstarted();
    let key = h.seed_account_at("me@example.org", Some("pw"), port);
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[]);
    h.start_core();
    wait_for(
        "the background connect began",
        Duration::from_secs(5),
        || accepted.load(Ordering::SeqCst) == 1,
    );
    (h, key, accepted)
}

#[test]
fn a_command_is_handled_while_a_background_connect_hangs() {
    let (h, key, _) = hanging();
    h.send(Command::ListFolders { account: key });
    // The connect times out after 20 s: a prompt answer is the fix.
    let started = Instant::now();
    h.wait(|e| matches!(e, Event::Folders { .. }).then_some(()));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn app_active_takes_over_from_a_hanging_background_connect() {
    let (h, _, accepted) = hanging();
    h.send(Command::AppActive(true));
    // The foreground sync connects itself at once instead of waiting out
    // the background connect.
    wait_for(
        "the foreground did not connect while the background connect hung",
        Duration::from_secs(3),
        || accepted.load(Ordering::SeqCst) >= 2,
    );
}
