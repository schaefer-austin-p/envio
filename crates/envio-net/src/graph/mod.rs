//! Microsoft Graph: the HTTP/1.1 client with keep-alive (`http`), the
//! incremental reply reader (`reply`), the parsers (`parse`, over `model`'s
//! types), the status table (`error`) and, behind `test-mock`, a scripted
//! TLS server for tests here and in envio-sync and envio-core.

pub mod error;
pub mod http;
#[cfg(feature = "test-mock")]
pub mod mock;
pub mod model;
pub mod parse;
mod reply;
mod time;
