//! Network test: needs a working system resolver. `cargo test -p envio-net
//! --test dns -- --ignored`.

use envio_net::dns::{SystemTxt, TxtAnswer, TxtLookup};

#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_name_without_a_key_record_is_no_record() {
    let answer = SystemTxt::new()
        .txt("brisbane._domainkey.football.example.com")
        .await;
    assert_eq!(answer, TxtAnswer::NoRecord);
}

/// The `p=` value of a DKIM key record text, up to the next `;`.
fn key_of(text: &str) -> &str {
    let rest = &text[text.find("p=").expect("a p= tag") + 2..];
    rest.split(';').next().unwrap()
}

/// Looked up on 2026-11-19: `selector2._domainkey.microsoft.com` is a CNAME
/// to `selector2-microsoft-com._domainkey.microsoft.onmicrosoft.com`, which
/// holds one TXT record (how Microsoft 365 publishes DKIM selectors).
#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_cname_delegated_selector_gives_the_key_record() {
    let answer = SystemTxt::new()
        .txt("selector2._domainkey.microsoft.com")
        .await;
    let TxtAnswer::Records { texts, ttl } = answer else {
        panic!("expected Records, got {answer:?}");
    };
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(texts[0].starts_with("v=DKIM1"), "{}", texts[0]);
    assert!(!key_of(&texts[0]).is_empty(), "{}", texts[0]);
    assert!(ttl > 0);
}

/// Looked up on 2026-11-19: `selector1._domainkey.outlook.com` is a CNAME
/// to `selector1._domainkey.outbound.protection.outlook.com`, whose one TXT
/// record holds a 2048-bit key in two character-strings (the first is 255
/// bytes), so this covers CNAME following and string joining on real data.
#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_key_split_in_several_strings_is_one_continuous_run() {
    use base64::Engine;
    let answer = SystemTxt::new()
        .txt("selector1._domainkey.outlook.com")
        .await;
    let TxtAnswer::Records { texts, .. } = answer else {
        panic!("expected Records, got {answer:?}");
    };
    assert_eq!(texts.len(), 1, "{texts:?}");
    let text = &texts[0];
    assert!(text.starts_with("v=DKIM1"), "{text}");
    assert!(text.len() > 255, "not joined: {} bytes", text.len());
    assert!(
        !text.contains(|c: char| c == '"' || c.is_whitespace()),
        "gap or quote in {text}"
    );
    let key = key_of(text);
    assert!(
        key.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')),
        "not one base64 run: {key}"
    );
    let der = base64::engine::general_purpose::STANDARD
        .decode(key)
        .expect("valid base64");
    // A 2048-bit RSA SubjectPublicKeyInfo is 294 bytes (the 1024-bit one is
    // 162), so a key cut at the string boundary would not decode to it.
    assert_eq!(der.len(), 294);
    assert!(der.starts_with(&[0x30, 0x82, 0x01, 0x22]));
}
