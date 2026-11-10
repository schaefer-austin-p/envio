//! Envio's logic. Qt-free: the app talks to it through qt/.

pub mod build_info;
pub mod logging;

// The helpers below are consumed by later modules of this crate.
#[allow(dead_code)]
mod fsutil;
#[allow(dead_code)]
mod i18n;
#[allow(dead_code)]
mod mask;
#[allow(dead_code)]
mod text;
