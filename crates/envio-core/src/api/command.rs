//! What the app asks the core to do.

#[derive(Debug)]
pub enum Command {
    /// Flush unsent work and stop the worker.
    Shutdown,
    /// The app came to the foreground (`true`) or went to the background.
    AppActive(bool),
    /// Sync now: one account, or all. Ends an Offline wait.
    Refresh { account: Option<String> },
    /// Panic the worker, to test the restart.
    #[cfg(feature = "test-hooks")]
    Crash,
}

impl Command {
    /// The variant's name, for logs (never the payload).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Shutdown => "Shutdown",
            Self::AppActive(_) => "AppActive",
            Self::Refresh { .. } => "Refresh",
            #[cfg(feature = "test-hooks")]
            Self::Crash => "Crash",
        }
    }
}
