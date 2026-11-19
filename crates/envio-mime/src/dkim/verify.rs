//! DKIM verification (RFC 6376, RFC 8463) against keys the caller fetched.
//! Pure: `key_queries` says which DNS names could matter, the caller looks
//! them up, `verify` decides. Plan #285, decisions 6 and 9.

use ring::digest;
use ring::signature::{self, UnparsedPublicKey};

use super::{
    Algorithm, Canon, KeyRecord, PublicKey, Signature, body, body_hash, header_fields, key_record,
    signature as parse_signature, signed_data,
};
use crate::align::aligned;
use crate::from_field::single_from_domain;

/// Signatures tried, and signature checks made, per message.
const MAX_SIGNATURES: usize = 5;
/// Key names looked up per message.
const MAX_KEYS: usize = 3;

/// What a TXT lookup for one key name returned.
pub enum KeyAnswer {
    /// The records, each with its TXT strings already joined.
    Records(Vec<String>),
    NoRecord,
    /// Timeout or server failure.
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DkimVerdict {
    Pass,
    Fail,
    /// The key names `keys` has no answer for, when none passed yet.
    NeedKeys(Vec<String>),
}

/// A `DKIM-Signature` field that could vouch for the From domain.
struct Candidate {
    /// Its index among the header fields.
    index: usize,
    sig: Signature,
}

impl Candidate {
    fn key_name(&self) -> String {
        format!("{}._domainkey.{}", self.sig.selector, self.sig.domain)
    }
}

/// Whether `field` is named `name` (lowercase), case-insensitively, with
/// optional space before the colon.
pub(crate) fn field_named(field: &[u8], name: &[u8]) -> bool {
    let Some(colon) = field.iter().position(|&b| b == b':') else {
        return false;
    };
    field[..colon].trim_ascii_end().eq_ignore_ascii_case(name)
}

/// The lowercased domain of `from`, when the message has exactly one From
/// mailbox and it is that address (decision 6).
fn checked_from_domain(raw: &[u8], from: &str) -> Option<String> {
    let (_, domain) = from.trim().rsplit_once('@')?;
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    (single_from_domain(raw).as_deref() == Some(domain.as_str())).then_some(domain)
}

/// The first five signatures that parse, are aligned with `from_domain` and
/// (with `need_content_type`) sign Content-Type.
fn candidates(fields: &[&[u8]], from_domain: &str, need_content_type: bool) -> Vec<Candidate> {
    let wanted = |sig: &Signature| {
        (!need_content_type || sig.signs_content_type()) && aligned(&sig.domain, from_domain)
    };
    fields
        .iter()
        .enumerate()
        .filter(|(_, f)| field_named(f, b"dkim-signature"))
        .filter_map(|(index, f)| {
            let colon = f.iter().position(|&b| b == b':')?;
            let sig = parse_signature(&String::from_utf8_lossy(&f[colon + 1..]))?;
            wanted(&sig).then_some(Candidate { index, sig })
        })
        .take(MAX_SIGNATURES)
        .collect()
}

/// The first three distinct key names of the candidates.
fn names(cands: &[Candidate]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in cands {
        let name = c.key_name();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out.truncate(MAX_KEYS);
    out
}

/// The key names (`<s>._domainkey.<d>`, at most 3) whose answers could
/// verify `from` on this message. Empty means DKIM cannot verify it.
pub fn key_queries(raw: &[u8], from: &str) -> Vec<String> {
    let Some(domain) = checked_from_domain(raw, from) else {
        return Vec::new();
    };
    names(&candidates(&header_fields(raw), &domain, true))
}

/// Pass when one candidate signature (at most 5 tried) verifies with a key
/// from `keys`; NeedKeys with the names `keys` has no answer for when none
/// passed yet and some may; Fail otherwise. `now` is Unix seconds.
pub fn verify<'k>(
    raw: &[u8],
    from: &str,
    keys: &dyn Fn(&str) -> Option<&'k KeyAnswer>,
    now: u64,
) -> DkimVerdict {
    let Some(domain) = checked_from_domain(raw, from) else {
        return DkimVerdict::Fail;
    };
    let fields = header_fields(raw);
    let input = Input {
        raw,
        fields: &fields,
        now,
    };
    decide(&input, &candidates(&fields, &domain, true), keys)
}

/// What the decision reads from the message.
struct Input<'a> {
    raw: &'a [u8],
    fields: &'a [&'a [u8]],
    now: u64,
}

/// A body hash per distinct `c=` body mode and `l=`.
type BodyHashes = Vec<((Canon, Option<u64>), Option<[u8; 32]>)>;

fn decide<'k>(
    input: &Input,
    cands: &[Candidate],
    keys: &dyn Fn(&str) -> Option<&'k KeyAnswer>,
) -> DkimVerdict {
    let (fields, now) = (input.fields, input.now);
    let allowed = names(cands);
    let body = body(input.raw);
    let mut hashes = BodyHashes::new();
    let mut checks = 0;
    let mut missing: Vec<String> = Vec::new();
    for c in cands {
        let sig = &c.sig;
        if sig.expires.is_some_and(|x| x < now) {
            continue;
        }
        let kind = (sig.body_canon, sig.body_length);
        let hash = match hashes.iter().find(|(k, _)| *k == kind) {
            Some((_, h)) => *h,
            None => {
                let h = body_hash(&body, kind.0, kind.1);
                hashes.push((kind, h));
                h
            }
        };
        // A mismatch ends this candidate before any key is wanted.
        if hash.is_none_or(|h| h[..] != sig.body_hash[..]) {
            continue;
        }
        let name = c.key_name();
        if !allowed.contains(&name) {
            continue;
        }
        match keys(&name) {
            None => {
                if !missing.contains(&name) {
                    missing.push(name);
                }
            }
            Some(KeyAnswer::Records(records)) => {
                for record in records.iter().filter_map(|r| key_record(r)) {
                    if checks == MAX_SIGNATURES {
                        return DkimVerdict::Fail;
                    }
                    checks += 1;
                    if signature_ok(fields, c.index, sig, &record) {
                        return DkimVerdict::Pass;
                    }
                }
            }
            Some(KeyAnswer::NoRecord | KeyAnswer::Unavailable) => {}
        }
    }
    if missing.is_empty() {
        DkimVerdict::Fail
    } else {
        DkimVerdict::NeedKeys(missing)
    }
}

/// The cryptographic check of one signature with one key record: `t=s` and
/// the signature over the header data. The body hash is checked by the
/// caller.
fn signature_ok(fields: &[&[u8]], index: usize, sig: &Signature, record: &KeyRecord) -> bool {
    if record.strict && !agent_equals_domain(sig) {
        return false;
    }
    let Some(data) = signed_data(fields, index, sig) else {
        return false;
    };
    check(sig.algorithm, &record.key, &data, &sig.signature)
}

/// `i=` absent means `@d`; `signature()` already put `i=` inside `d=`.
fn agent_equals_domain(sig: &Signature) -> bool {
    sig.agent.as_ref().is_none_or(|a| {
        a.rsplit_once('@')
            .is_some_and(|(_, host)| host == sig.domain)
    })
}

/// The two algorithms take different inputs (RFC 8463): Ed25519 signs the
/// SHA-256 of the data, `ring`'s RSA verifier hashes the data itself.
fn check(alg: Algorithm, key: &PublicKey, data: &[u8], sig: &[u8]) -> bool {
    match (alg, key) {
        (Algorithm::RsaSha256, PublicKey::Rsa(der)) => UnparsedPublicKey::new(
            &signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
            der,
        )
        .verify(data, sig)
        .is_ok(),
        (Algorithm::Ed25519Sha256, PublicKey::Ed25519(raw)) => {
            let digest = digest::digest(&digest::SHA256, data);
            UnparsedPublicKey::new(&signature::ED25519, raw)
                .verify(digest.as_ref(), sig)
                .is_ok()
        }
        _ => false, // k= does not match a=
    }
}

#[cfg(test)]
mod tests;
