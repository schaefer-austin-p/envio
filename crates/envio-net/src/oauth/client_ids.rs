//! The OAuth client IDs. A public client's ID is not a secret; it is one
//! checked-in constant, identical in every build. Production code reaches
//! it only through `Provider::microsoft()`.

/// The Microsoft Entra application (client) ID: a public client registered
/// for personal and organisational accounts (#219 T10). Never add a client
/// secret. While this is empty, no Microsoft sign-in is offered.
pub const MICROSOFT: &str = "65905258-cf99-4019-b7a1-a0c401a115c6";
