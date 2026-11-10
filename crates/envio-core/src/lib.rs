//! Envio's logic. Qt-free: the app talks to it through qt/.

pub mod build_info;
pub mod health;
pub mod logging;
pub mod problem;
pub mod secrets;
pub mod servers;

// The helpers below are consumed by later modules of this crate.
mod fsutil;
#[allow(dead_code)]
mod i18n;
#[allow(dead_code)]
mod mask;
#[allow(dead_code)]
mod text;
