//! Envio's network side: IMAP, SMTP, account autoconfig, and TLS with
//! certificate pins. Async on tokio; core drives it from its worker
//! thread's runtime. Decisions (trust, reply parsing, query building,
//! autoconfig order) are pure functions; the I/O around them is thin.

pub mod autoconfig;
mod connect;
pub mod error;
pub mod imap;
pub mod server;
pub mod smtp;
pub mod tls;
pub mod trust;
mod verifier;

pub use connect::{COMMAND_TIMEOUT, CONNECT_TIMEOUT};
pub use error::{NetError, Result, Untrusted};
pub use server::{Credentials, Endpoint, Fingerprint, Security};
pub use tls::Roots;
