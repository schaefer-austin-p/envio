//! The logger is global, so this binary owns it: one shared writer at Trace.

use envio_core::logging;
use log::{Level, LevelFilter};
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

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

fn captured() -> String {
    static BUF: OnceLock<Shared> = OnceLock::new();
    let shared = BUF.get_or_init(|| {
        let shared = Shared::default();
        logging::init_with_writer(Box::new(shared.clone()), LevelFilter::Trace).unwrap();
        shared
    });
    String::from_utf8(shared.0.lock().unwrap().clone()).unwrap()
}

#[test]
fn trace_level_never_records_async_imap_or_imap_proto() {
    captured();
    log::trace!(target: "async_imap::client", "LOGIN \"u\" \"pw-marker-1\"");
    log::debug!(target: "imap_proto::parser", "line-marker-2");
    log::warn!(target: "async_imap::client", "warn-marker-3");
    let out = captured();
    assert!(!out.contains("pw-marker-1"), "{out}");
    assert!(!out.contains("line-marker-2"), "{out}");
    assert!(out.contains("warn-marker-3"), "{out}");
}

#[test]
fn own_targets_are_recorded() {
    captured();
    log::info!(target: "envio_core::worker", "own-marker-4");
    assert!(captured().contains("own-marker-4"));
}

#[test]
fn levels_follow_the_chosen_maximum() {
    assert!(!logging::allowed(
        "envio_core",
        Level::Debug,
        LevelFilter::Info
    ));
    assert!(logging::allowed(
        "envio_core",
        Level::Warn,
        LevelFilter::Info
    ));
    assert!(!logging::allowed(
        "async_imap",
        Level::Info,
        LevelFilter::Trace
    ));
    assert!(logging::allowed(
        "async_imap::x",
        Level::Error,
        LevelFilter::Trace
    ));
    assert!(logging::allowed(
        "async_imap_other",
        Level::Trace,
        LevelFilter::Trace
    ));
}

#[test]
fn the_log_file_rotates_at_one_mib() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.log");
    std::fs::write(&path, vec![b'x'; logging::ROTATE_BYTES as usize]).unwrap();
    logging::rotate_if_needed_for_test(&path);
    assert!(!path.exists());
    let rotated = std::fs::read(dir.path().join("envio.log.1")).unwrap();
    assert_eq!(rotated.len() as u64, logging::ROTATE_BYTES);

    std::fs::write(&path, b"small").unwrap();
    logging::rotate_if_needed_for_test(&path);
    assert!(path.exists());
}
