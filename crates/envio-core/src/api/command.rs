//! What the app asks the core to do.

use envio_net::autoconfig::Source;
use secrecy::SecretString;

use crate::servers::{ServerAddr, Service};

/// What the wizard has settled on for a new account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSetup {
    pub name: String,
    pub email: String,
    pub username: String,
    pub imap: ServerAddr,
    pub smtp: ServerAddr,
    pub source: Option<Source>,
    /// The servers came from unauthenticated DNS: the user must agree
    /// to the hosts before the password goes to them.
    pub needs_confirm: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub setup: AccountSetup,
    pub password: SecretString,
    /// The user agreed to the hosts shown (needed when `needs_confirm`).
    pub servers_confirmed: bool,
    /// Hex fingerprints of certificates the user accepted.
    pub accepted: Vec<(Service, String)>,
}

#[derive(Debug)]
pub enum Command {
    /// Flush unsent work and stop the worker.
    Shutdown,
    /// The app came to the foreground (`true`) or went to the background.
    AppActive(bool),
    /// Sync now: one account, or all. Ends an Offline wait.
    Refresh { account: Option<String> },
    /// Look up the servers for an address (only its domain leaves the phone).
    Detect { email: String },
    /// Check the servers and password, then add the account.
    Connect(Box<ConnectRequest>),
    /// A new password for an account whose sign-in failed.
    UpdatePassword {
        account: String,
        password: SecretString,
    },
    /// Trust the certificate the account's server presented.
    AcceptCertificate {
        account: String,
        service: Service,
        fingerprint: String,
    },
    /// Remove an account and everything kept for it. Asks first when
    /// work is unsent, unless `confirmed`.
    RemoveAccount { account: String, confirmed: bool },
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
            Self::Detect { .. } => "Detect",
            Self::Connect(_) => "Connect",
            Self::UpdatePassword { .. } => "UpdatePassword",
            Self::AcceptCertificate { .. } => "AcceptCertificate",
            Self::RemoveAccount { .. } => "RemoveAccount",
            #[cfg(feature = "test-hooks")]
            Self::Crash => "Crash",
        }
    }
}
