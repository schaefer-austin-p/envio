use super::*;

#[test]
fn the_verifier_draws_on_all_64_symbols() {
    use secrecy::ExposeSecret;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..2000 {
        seen.extend(Pkce::new().verifier.expose_secret().bytes());
    }
    assert_eq!(seen.len(), 64);
    assert!(ALPHABET.iter().all(|b| seen.contains(b)));
}
use base64::Engine;
use secrecy::ExposeSecret;

#[test]
fn rfc7636_appendix_b_vector() {
    let p = Pkce::from_parts("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "s");
    assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    assert_eq!(p.state, "s");
}

#[test]
fn verifier_is_64_url_safe_characters() {
    let p = Pkce::new();
    let v = p.verifier.expose_secret();
    assert_eq!(v.len(), 64);
    assert!(
        v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
}

#[test]
fn challenge_matches_the_verifier() {
    let p = Pkce::new();
    let again = Pkce::from_parts(p.verifier.expose_secret(), &p.state);
    assert_eq!(p.challenge, again.challenge);
    assert_eq!(p.challenge.len(), 43);
}

#[test]
fn state_is_32_random_bytes_in_base64url() {
    let p = Pkce::new();
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&p.state)
        .unwrap();
    assert_eq!(raw.len(), 32);
}

#[test]
fn every_attempt_differs() {
    let (a, b) = (Pkce::new(), Pkce::new());
    assert_ne!(a.verifier.expose_secret(), b.verifier.expose_secret());
    assert_ne!(a.state, b.state);
}

#[test]
fn debug_hides_the_verifier() {
    let p = Pkce::from_parts("VERIFIER-MARKER", "state-marker");
    let text = format!("{p:?}");
    assert!(!text.contains("VERIFIER-MARKER"));
    assert!(!text.contains(&p.challenge));
}
