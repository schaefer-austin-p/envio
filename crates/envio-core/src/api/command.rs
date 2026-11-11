//! What the app asks the core to do.

#[derive(Debug)]
pub enum Command {
    /// Flush unsent work and stop the worker.
    Shutdown,
    /// Panic the worker, to test the restart.
    #[cfg(feature = "test-hooks")]
    Crash,
}

impl Command {
    /// The variant's name, for logs (never the payload).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shutdown => "Shutdown",
            #[cfg(feature = "test-hooks")]
            Self::Crash => "Crash",
        }
    }
}
