//! Real-signer vectors: the canonicalisation must give the exact bytes that a
//! signer other than this crate signed (see `canon_vectors`).
use super::canon::{body, body_hash, header_fields, signed_data};
use super::canon_vectors::*;
use super::signature::signature;
use base64::Engine;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};

fn check(header: &str) {
    let b64 = base64::engine::general_purpose::STANDARD;
    let raw = format!("{header}{MSG}").into_bytes();
    let fields = header_fields(&raw);
    let sig_field = fields[0];
    assert!(sig_field.starts_with(b"DKIM-Signature:"));
    let value = std::str::from_utf8(&sig_field[b"DKIM-Signature:".len()..]).unwrap();
    let sig = signature(value).expect("the signature header parses");
    // The body hash is the one the signer wrote.
    assert_eq!(
        body_hash(&body(&raw), sig.body_canon, sig.body_length).map(|h| h.to_vec()),
        Some(sig.body_hash.clone()),
        "body hash"
    );
    // The signed data verifies under the signer's public key.
    let data = signed_data(&fields, 0, &sig).expect("index in range");
    let key = b64.decode(PUB).unwrap();
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key)
        .verify(&data, &sig.signature)
        .expect("signed_data is what the signer signed");
}

#[test]
fn dkimpy_relaxed_b_last() {
    check(RELAXED);
}

#[test]
fn dkimpy_simple_b_last() {
    check(SIMPLE);
}

// b= folded right after the `=`, last tag. The signer hashed `b=` with nothing
// after it (RFC 6376 3.5: value deleted with its surrounding whitespace).
#[test]
fn dkimpy_relaxed_b_folded_after_equals() {
    check(RELAXED_FOLD);
}

#[test]
fn dkimpy_simple_b_folded_after_equals() {
    check(SIMPLE_FOLD);
}

// b= in the middle: folded after `=`, then `;` and another tag.
#[test]
fn signed_relaxed_b_folded_in_the_middle() {
    check(RELAXED_MID);
}

#[test]
fn signed_simple_b_folded_in_the_middle() {
    check(SIMPLE_MID);
}
