//! Whether to trust a server certificate: pure, so every case is a unit
//! test. A pin, when there is one, is the only thing that counts; without
//! one, the system roots decide and a failure becomes the user's prompt.

use crate::error::Untrusted;
use crate::server::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    /// Ask the user (the self-signed prompt).
    Untrusted(Untrusted),
    /// Stop: the pinned certificate was replaced.
    Changed {
        pinned: Fingerprint,
    },
}

/// `system` runs the roots check; it is only called when there is no pin.
pub fn decide(
    pin: Option<&Fingerprint>,
    presented: &Fingerprint,
    system: impl FnOnce() -> Result<(), Untrusted>,
) -> Verdict {
    match pin {
        Some(pinned) if pinned == presented => Verdict::Accept,
        Some(pinned) => Verdict::Changed { pinned: *pinned },
        None => match system() {
            Ok(()) => Verdict::Accept,
            Err(reason) => Verdict::Untrusted(reason),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_bytes([byte; 32])
    }

    #[test]
    fn a_matching_pin_is_accepted_without_asking_the_roots() {
        let verdict = decide(Some(&fp(1)), &fp(1), || panic!("roots consulted"));
        assert_eq!(verdict, Verdict::Accept);
    }

    #[test]
    fn a_different_certificate_on_a_pinned_server_is_a_change() {
        let verdict = decide(Some(&fp(1)), &fp(2), || Ok(()));
        assert_eq!(verdict, Verdict::Changed { pinned: fp(1) });
    }

    #[test]
    fn without_a_pin_the_roots_decide() {
        assert_eq!(decide(None, &fp(1), || Ok(())), Verdict::Accept);
        assert_eq!(
            decide(None, &fp(1), || Err(Untrusted::UnknownIssuer)),
            Verdict::Untrusted(Untrusted::UnknownIssuer)
        );
    }
}
