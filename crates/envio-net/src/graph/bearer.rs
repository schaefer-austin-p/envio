//! Where the Graph client gets its access token. Core implements this over
//! its token handle; tests use a fake.

use futures::future::LocalBoxFuture;
use secrecy::SecretString;

use crate::Result;

/// Non-`Send`, like core's `Refresher`: the worker is single-threaded.
pub trait Bearer {
    /// The access token. `force` asks for a fresh one: the last was refused
    /// with a 401. Core's problems map to net errors here: temporary and
    /// offline give `Busy`, a pause gives `SignIn`.
    fn bearer(&self, force: bool) -> LocalBoxFuture<'_, Result<SecretString>>;
}
