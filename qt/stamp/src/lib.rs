//! The stamp `build.rs` leaves: both None in an official release, so its
//! binary holds no git data or time.

/// From tools/stamp.sh: `branch@hash`, `-dirty`, or "unknown".
pub const SOURCE: Option<&str> = option_env!("ENVIO_STAMP_SOURCE");
/// When it was compiled, in Unix seconds (SOURCE_DATE_EPOCH if set).
pub const BUILT: Option<&str> = option_env!("ENVIO_STAMP_BUILT");
