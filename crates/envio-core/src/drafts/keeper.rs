//! The thread that saves drafts (`envio-drafts`). It outlives worker
//! restarts: it is owned by `Core`, not by the supervisor.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::DraftBook;
use crate::api::command::{ComposeId, DraftInput};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::core::EventSink;

/// `Core::send` waits this long at most for a flush.
const FLUSH_WAIT: Duration = Duration::from_millis(500);

enum KeeperMsg {
    Changed {
        compose: ComposeId,
        draft: Box<DraftInput>,
        at: i64,
    },
    Close {
        compose: ComposeId,
        discard: bool,
    },
    Flush(Sender<()>),
    /// Look at the clock again: the book changed behind the thread's back.
    Wake,
    Stop,
}

pub(crate) struct DraftKeeper {
    book: Arc<DraftBook>,
    tx: Sender<KeeperMsg>,
    thread: Option<JoinHandle<()>>,
    /// Attachment copies, one at a time on their own thread (`envio-attach`)
    /// so a big file never delays a text save. Not joined on drop: it ends
    /// after the copy it is in, and a stuck source must not hold up quitting.
    attach_tx: Sender<(ComposeId, PathBuf)>,
}

impl DraftKeeper {
    pub fn start(book: Arc<DraftBook>, sink: Arc<dyn EventSink>) -> io::Result<Self> {
        let (tx, rx) = channel();
        let waker = tx.clone();
        book.set_waker(move || {
            let _ = waker.send(KeeperMsg::Wake);
        });
        let (attach_tx, attach_rx) = channel();
        let attach_book = Arc::clone(&book);
        let kept = Arc::clone(&book);
        let attach_sink = Arc::clone(&sink);
        thread::Builder::new()
            .name("envio-attach".into())
            .spawn(move || run_attach(&attach_book, &attach_rx, &*attach_sink))?;
        let thread = thread::Builder::new()
            .name("envio-drafts".into())
            .spawn(move || run(&book, &rx, &*sink))?;
        Ok(Self {
            book: kept,
            tx,
            thread: Some(thread),
            attach_tx,
        })
    }

    pub fn changed(&self, compose: ComposeId, draft: DraftInput, at: i64) {
        self.post(KeeperMsg::Changed {
            compose,
            draft: Box::new(draft),
            at,
        });
    }

    pub fn close(&self, compose: ComposeId, discard: bool) {
        // Here and now, on the caller's thread: the worker is told next and
        // must not depend on the draft thread (or a flush) being done.
        self.book.closing(compose);
        self.post(KeeperMsg::Close { compose, discard });
    }

    pub fn attach(&self, compose: ComposeId, path: PathBuf) {
        if self.attach_tx.send((compose, path)).is_err() {
            log::warn!("the attach thread has stopped; dropped an attachment");
        }
    }

    /// Saves everything pending and waits for it, for at most 500 ms.
    pub fn flush(&self) {
        let (ack, done) = channel();
        if self.tx.send(KeeperMsg::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(FLUSH_WAIT);
        }
    }

    fn post(&self, msg: KeeperMsg) {
        if self.tx.send(msg).is_err() {
            log::warn!("the draft thread has stopped; dropped a draft message");
        }
    }
}

impl Drop for DraftKeeper {
    fn drop(&mut self) {
        let _ = self.tx.send(KeeperMsg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn emit_all(sink: &dyn EventSink, events: Vec<Event>) {
    for event in events {
        sink.emit(event);
    }
}

/// Keeps going after a panic (the app's sink, or the store, may panic): the
/// book is plain data and the channel is still there.
fn run(book: &DraftBook, rx: &Receiver<KeeperMsg>, sink: &dyn EventSink) {
    loop {
        match panic::catch_unwind(AssertUnwindSafe(|| step(book, rx, sink))) {
            Ok(true) => {}
            Ok(false) => return,
            Err(_) => log::error!("the draft thread panicked; going on"),
        }
    }
}

/// Copies the attachments in the order they were asked for. A panic fails
/// that one attachment, not the ones after it.
fn run_attach(book: &DraftBook, rx: &Receiver<(ComposeId, PathBuf)>, sink: &dyn EventSink) {
    while let Ok((compose, path)) = rx.recv() {
        let done = panic::catch_unwind(AssertUnwindSafe(|| {
            book.attach_emit(compose, &path, &mut |event| sink.emit(event));
        }));
        if done.is_err() {
            log::error!("the attach thread panicked; going on");
        }
    }
}

/// One message (or due time) handled; false when the thread should end.
fn step(book: &DraftBook, rx: &Receiver<KeeperMsg>, sink: &dyn EventSink) -> bool {
    {
        let wait = book
            .next_due()
            .map(|due| u64::try_from((due - now_ms()).max(0)).unwrap_or(0));
        let msg = match wait {
            Some(ms) => rx.recv_timeout(Duration::from_millis(ms)),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(KeeperMsg::Changed { compose, draft, at }) => book.changed(compose, *draft, at),
            Ok(KeeperMsg::Close { compose, discard }) => {
                emit_all(sink, book.close(compose, discard));
            }
            Ok(KeeperMsg::Flush(ack)) => {
                emit_all(sink, book.flush_all());
                let _ = ack.send(());
            }
            Ok(KeeperMsg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                emit_all(sink, book.flush_all());
                return false;
            }
            Ok(KeeperMsg::Wake) | Err(RecvTimeoutError::Timeout) => {}
        }
        emit_all(sink, book.save_due(now_ms()));
    }
    true
}
