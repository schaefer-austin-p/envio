//! PKCE (RFC 7636, S256): the verifier, its challenge and the `state`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use secrecy::SecretString;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// One sign-in attempt's secrets. Lives in memory only.
pub struct Pkce {
    pub verifier: SecretString,
    pub challenge: String,
    pub state: String,
}

fn random<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    SystemRandom::new()
        .fill(&mut buf)
        .expect("the system random source is unavailable");
    buf
}

impl Pkce {
    pub fn new() -> Self {
        // 64 symbols, so `b & 63` is uniform.
        let verifier: String = random::<64>()
            .iter()
            .map(|b| char::from(ALPHABET[usize::from(b & 63)]))
            .collect();
        Self::from_parts(&verifier, &B64.encode(random::<32>()))
    }

    pub fn from_parts(verifier: &str, state: &str) -> Self {
        let hash = digest(&SHA256, verifier.as_bytes());
        Self {
            verifier: SecretString::from(verifier),
            challenge: B64.encode(hash.as_ref()),
            state: state.to_owned(),
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkce").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
