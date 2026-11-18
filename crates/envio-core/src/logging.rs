//! The file logger. It never records `async_imap` or `imap_proto` below
//! Warn: their Debug and Trace lines carry the IMAP LOGIN command with the
//! password.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

pub const QUIET_TARGETS: [&str; 2] = ["async_imap", "imap_proto"];
pub const ROTATE_BYTES: u64 = 1 << 20;

/// Whether a record may be written: `max` applies, except that the
/// credential-bearing targets are capped at Warn.
pub fn allowed(target: &str, level: log::Level, max: log::LevelFilter) -> bool {
    let quiet = QUIET_TARGETS.iter().any(|q| {
        target == *q
            || target
                .strip_prefix(q)
                .is_some_and(|rest| rest.starts_with("::"))
    });
    let cap = if quiet {
        max.min(log::LevelFilter::Warn)
    } else {
        max
    };
    level <= cap
}

enum Sink {
    File { path: PathBuf, file: Option<File> },
    Writer(Box<dyn Write + Send>),
}

impl Sink {
    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        match self {
            Self::Writer(w) => w.write_all(line),
            Self::File { path, file } => {
                if rotate_if_needed(path) {
                    *file = None;
                }
                if file.is_none() {
                    *file = Some(
                        OpenOptions::new()
                            .create(true)
                            .append(true)
                            .mode(0o600)
                            .open(&*path)?,
                    );
                }
                file.as_mut().map_or(Ok(()), |f| f.write_all(line))
            }
        }
    }
}

struct FileLog {
    max: log::LevelFilter,
    out: Mutex<Sink>,
}

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        allowed(m.target(), m.level(), self.max)
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        // A message must not be able to start a line of its own.
        let message = r
            .args()
            .to_string()
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        let line = format!("{} {} {}: {message}\n", now_ms(), r.level(), r.target());
        if let Ok(mut sink) = self.out.lock() {
            let _ = sink.write_line(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Renames a log of at least `ROTATE_BYTES` to `<name>.1`, replacing an
/// older one. True when it rotated.
fn rotate_if_needed(path: &Path) -> bool {
    let big = fs::metadata(path).is_ok_and(|m| m.len() >= ROTATE_BYTES);
    if !big {
        return false;
    }
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(".1");
    fs::rename(path, rotated).is_ok()
}

#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub fn rotate_if_needed_for_test(path: &Path) -> bool {
    rotate_if_needed(path)
}

fn install(sink: Sink, max: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    let logger = FileLog {
        max,
        out: Mutex::new(sink),
    };
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(max);
    OURS.store(true, Ordering::Relaxed);
    Ok(())
}

/// Set once the envio logger is the installed one.
static OURS: AtomicBool = AtomicBool::new(false);
static WARNED: Once = Once::new();

/// Installs the file logger (`envio.log` in `logs_dir`) once per process.
/// A second call, or a logger installed by `init_with_writer`, is `Ok` and
/// keeps the first. A logger some other code installed first stays, and
/// then the envio filter (the password-bearing targets capped at Warn) is
/// not active: hosts must not install their own logger before `Core::start`.
pub fn init(logs_dir: &Path, max: log::LevelFilter) -> io::Result<()> {
    envio_store::create_private_dir(logs_dir)?;
    let sink = Sink::File {
        path: logs_dir.join("envio.log"),
        file: None,
    };
    if install(sink, max).is_err() && !OURS.load(Ordering::Relaxed) {
        WARNED.call_once(|| {
            log::warn!("another logger is installed; the envio log filter is not active");
        });
    }
    Ok(())
}

/// Installs a logger writing to `writer` (tests).
pub fn init_with_writer(
    writer: Box<dyn Write + Send>,
    max: log::LevelFilter,
) -> Result<(), log::SetLoggerError> {
    install(Sink::Writer(writer), max)
}
