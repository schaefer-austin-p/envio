//! What the app asks the core to do.

#[derive(Debug)]
pub enum Command {
    /// Flush unsent work and stop the worker.
    Shutdown,
    /// Panic the worker, to test the restart.
    #[cfg(feature = "test-hooks")]
    Crash,
}
