//! A logger installed before envio's stays, and envio says its filter is off.
//! Its own test binary: the logger is process-wide.

use std::sync::Mutex;

use envio_core::logging;

static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Foreign;

impl log::Log for Foreign {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        SEEN.lock().unwrap().push(record.args().to_string());
    }
    fn flush(&self) {}
}

#[test]
fn init_warns_once_when_another_logger_is_installed() {
    log::set_logger(&Foreign).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let dir = tempfile::tempdir().unwrap();
    logging::init(dir.path(), log::LevelFilter::Info).unwrap();
    logging::init(dir.path(), log::LevelFilter::Info).unwrap();
    let seen = SEEN.lock().unwrap();
    let warnings = seen.iter().filter(|m| m.contains("another logger")).count();
    assert_eq!(warnings, 1, "{seen:?}");
}
