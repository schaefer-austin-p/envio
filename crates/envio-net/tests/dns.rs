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

/// What the code guarantees for any real answer, whatever key a sender
/// publishes today: at least one record, each joined into one string with a
/// `p=` tag, and the whole answer under the 8 KiB cap. The tests do not pin
/// the current key, its size or the number of records, so a key rotation
/// does not break them.
fn assert_key_records(answer: TxtAnswer) {
    let TxtAnswer::Records { texts, .. } = answer else {
        panic!("expected Records, got {answer:?}");
    };
    assert!(!texts.is_empty());
    assert!(texts.len() <= 8, "{} records", texts.len());
    assert!(
        texts.iter().map(String::len).sum::<usize>() <= 8 * 1024,
        "over the 8 KiB cap"
    );
    for text in &texts {
        assert!(text.contains("p="), "no p= tag in {text}");
    }
}

/// `selector2._domainkey.microsoft.com` is a CNAME to a name under
/// `microsoft.onmicrosoft.com` (how Microsoft 365 publishes DKIM
/// selectors, checked 2026-11-19), so this covers CNAME following.
#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_cname_delegated_selector_gives_the_key_record() {
    assert_key_records(
        SystemTxt::new()
            .txt("selector2._domainkey.microsoft.com")
            .await,
    );
}

/// `selector1._domainkey.outlook.com` is a CNAME to a name under
/// `outbound.protection.outlook.com` whose TXT record was split into two
/// character-strings (checked 2026-11-19), so this covers CNAME following
/// and string joining. Whatever the split, the joined text has no quote or
/// whitespace in its `p=` value, which is one run of base64.
#[tokio::test]
#[ignore = "needs network access to a DNS resolver"]
async fn a_joined_key_is_one_continuous_base64_run() {
    let answer = SystemTxt::new()
        .txt("selector1._domainkey.outlook.com")
        .await;
    assert_key_records(answer.clone());
    let TxtAnswer::Records { texts, .. } = answer else {
        unreachable!();
    };
    for text in &texts {
        let rest = &text[text.find("p=").unwrap() + 2..];
        let key = rest.split(';').next().unwrap();
        assert!(
            key.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')),
            "not one base64 run: {key}"
        );
    }
}
