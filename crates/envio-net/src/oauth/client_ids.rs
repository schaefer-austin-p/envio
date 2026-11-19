//! The OAuth client IDs. A public client's ID is not a secret; it is one
//! checked-in constant, identical in every build. Production code reaches
//! it only through `Provider::microsoft()`.

/// The Microsoft Entra application (client) ID. Empty until the owner
/// registers the app (#219 T10); while empty, no Microsoft sign-in is offered.
pub const MICROSOFT: &str = "";
