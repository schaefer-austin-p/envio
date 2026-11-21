//! Microsoft Graph: the HTTP/1.1 client with keep-alive (`http`), the
//! incremental reply reader (`reply`) and, behind `test-mock`, a scripted TLS
//! server for tests here and in envio-sync and envio-core.

pub mod http;
#[cfg(feature = "test-mock")]
pub mod mock;
mod reply;
