//! The QObjects QML uses, registered in the `Envio 1.0` QML module. They
//! translate between Qt and envio-core/envio-ui types and hold no logic.

// QML invokables and signals take flat arguments (their signatures are the
// QML API), so the argument-count gate does not apply to this crate.
#![allow(clippy::too_many_arguments)]

mod build_info;
mod calls;
mod convert;
mod core_object;
mod models;
mod sink;
mod variant;
