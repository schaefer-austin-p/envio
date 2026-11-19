//! DKIM (RFC 6376, RFC 8463) in pure Rust: the pieces that read a
//! `DKIM-Signature` header and a key record from DNS, canonicalise, and
//! verify ([`key_queries`], [`verify`]). No I/O: the caller does the DNS
//! lookup and hands the TXT text in.

mod canon;
#[cfg(test)]
mod canon_signer_tests;
#[cfg(test)]
mod canon_tests;
#[cfg(test)]
mod canon_vectors;
mod der;
mod key;
mod signature;
mod tags;
mod verify;

pub(crate) use canon::{body, body_hash, header_fields, signed_data};
pub(crate) use key::{KeyRecord, PublicKey, key_record};
pub(crate) use signature::{Algorithm, Canon, Signature, signature};
pub(crate) use verify::field_named;
pub use verify::{DkimVerdict, KeyAnswer, key_queries, verify};
