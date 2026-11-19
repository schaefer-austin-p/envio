//! The per-signature check on the RFC 8463 Appendix A.3 message, whose `h=`
//! has no `content-type`, so the public API rightly ignores it.
use super::*;

const MSG: &[u8] = include_bytes!("../../../tests/fixtures/dkim/rfc8463.eml");
const ED_RECORD: &str = "v=DKIM1; k=ed25519; p=11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
const RSA_RECORD: &str = "v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDkHlOQoBTzWRiGs5V6NpP3idY6Wk08a5qhdR6wy5bdOKb2jLQiY/J16JYi0Qvx/byYzCNb3W91y3FutACDfzwQ/BC/e/8uBsCR+yz1Lxj+PL6lHvqMKrM3rG4hstT5QjvHO9PzoxZyVYLzBfO2EeC3Ip3G+2kryOTIKT+l/K4w3QIDAQAB";
const ED_NAME: &str = "brisbane._domainkey.football.example.com";
const RSA_NAME: &str = "test._domainkey.football.example.com";

fn answers(ed: bool, rsa: bool) -> Vec<(&'static str, KeyAnswer)> {
    let mut v = Vec::new();
    if ed {
        v.push((ED_NAME, KeyAnswer::Records(vec![ED_RECORD.into()])));
    }
    if rsa {
        v.push((RSA_NAME, KeyAnswer::Records(vec![RSA_RECORD.into()])));
    }
    v
}

/// The verdict on the RFC message with the content-type rule off.
fn verdict(ed: bool, rsa: bool) -> DkimVerdict {
    let fields = header_fields(MSG);
    let cands = candidates(&fields, "football.example.com", false);
    assert_eq!(cands.len(), 2);
    let list = answers(ed, rsa);
    let lookup = |n: &str| list.iter().find(|(k, _)| *k == n).map(|(_, a)| a);
    let input = Input {
        raw: MSG,
        fields: &fields,
        now: 0,
    };
    decide(&input, &cands, &lookup)
}

#[test]
fn the_rfc_8463_message_passes_with_either_key_or_both() {
    assert_eq!(verdict(false, true), DkimVerdict::Pass);
    assert_eq!(verdict(true, false), DkimVerdict::Pass);
    assert_eq!(verdict(true, true), DkimVerdict::Pass);
}

#[test]
fn the_rfc_8463_message_without_a_key_needs_both() {
    assert_eq!(
        verdict(false, false),
        DkimVerdict::NeedKeys(vec![ED_NAME.into(), RSA_NAME.into()])
    );
}

#[test]
fn each_algorithm_checks_its_own_input() {
    let fields = header_fields(MSG);
    let cands = candidates(&fields, "football.example.com", false);
    let (ed, rsa) = (&cands[0], &cands[1]);
    assert_eq!(ed.sig.algorithm, Algorithm::Ed25519Sha256);
    let ed_key = key_record(ED_RECORD).unwrap();
    let rsa_key = key_record(RSA_RECORD).unwrap();
    assert!(signature_ok(&fields, ed.index, &ed.sig, &ed_key));
    assert!(signature_ok(&fields, rsa.index, &rsa.sig, &rsa_key));
    // A key of the other type, and a signature over other data, fail.
    assert!(!signature_ok(&fields, ed.index, &ed.sig, &rsa_key));
    assert!(!signature_ok(&fields, rsa.index, &rsa.sig, &ed_key));
    let data = signed_data(&fields, ed.index, &ed.sig).unwrap();
    assert!(check(
        ed.sig.algorithm,
        &ed_key.key,
        &data,
        &ed.sig.signature
    ));
    assert!(!check(
        rsa.sig.algorithm,
        &ed_key.key,
        &data,
        &ed.sig.signature
    ));
}

#[test]
fn field_named_ignores_case_and_space_before_the_colon() {
    assert!(field_named(b"DKIM-Signature : v=1", b"dkim-signature"));
    assert!(!field_named(b"X-DKIM-Signature: v=1", b"dkim-signature"));
    assert!(!field_named(b"no colon", b"dkim-signature"));
}
