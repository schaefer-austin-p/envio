//! DKIM (RFC 6376, RFC 8463) in pure Rust: the pieces that read a
//! `DKIM-Signature` header and a key record from DNS. Canonicalisation and
//! verification come in later modules. No I/O: the caller does the DNS
//! lookup and hands the TXT text in.

mod canon;
#[cfg(test)]
mod canon_tests;
mod der;
mod key;
mod signature;
mod tags;

pub(crate) use canon::{body, body_hash, header_fields, signed_data};
pub(crate) use key::{KeyRecord, PublicKey, key_record};
pub(crate) use signature::{Algorithm, Canon, Signature, signature};
pub(crate) use tags::tag_list;
