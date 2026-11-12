//! Where core events enter Qt: the worker thread calls `emit`, the closure
//! runs on the Qt thread against the `EnvioCore` object.

use cxx_qt::CxxQtThread;
use envio_core::{Event, EventSink};

use crate::core_object::qobject::EnvioCore;

pub struct QtSink {
    thread: CxxQtThread<EnvioCore>,
}

impl QtSink {
    pub fn new(thread: CxxQtThread<EnvioCore>) -> Self {
        Self { thread }
    }
}

impl EventSink for QtSink {
    fn emit(&self, event: Event) {
        // The object is gone while the app shuts down; the core is
        // flushing and its last events have nobody to tell.
        if self.thread.queue(move |obj| obj.apply(event)).is_err() {
            log::info!("EnvioCore is gone; dropped an event");
        }
    }
}
