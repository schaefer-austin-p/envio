//! A DKIM key record (RFC 6376 3.6.1, RFC 8463) from one DNS TXT string.

use super::der::rsa_public_key;
use super::tags::{base64_bytes, colon_list, tag_list};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicKey {
    /// A PKCS#1 RSAPublicKey in DER.
    Rsa(Vec<u8>),
    Ed25519([u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyRecord {
    pub key: PublicKey,
    /// `t=s`: the signature's `i=` domain must equal `d=`.
    pub strict: bool,
}

/// One TXT record (its strings already joined). None when revoked
/// (`p=` empty), testing (`t=y`), not for email (`s=`), not allowing
/// sha256 (`h=`), `k=` unknown, `v=` present and not DKIM1, or the key
/// does not parse.
pub(crate) fn key_record(txt: &str) -> Option<KeyRecord> {
    let tags = tag_list(txt)?;
    let get = |name: &str| {
        tags.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    if get("v").is_some_and(|v| v != "DKIM1") {
        return None;
    }
    if get("h").is_some_and(|h| !colon_list(h).any(|a| a == "sha256")) {
        return None;
    }
    if get("s").is_some_and(|s| !colon_list(s).any(|t| t == "*" || t == "email")) {
        return None;
    }
    let flags = get("t").unwrap_or("");
    if colon_list(flags).any(|f| f == "y") {
        return None;
    }
    let strict = colon_list(flags).any(|f| f == "s");
    let p = base64_bytes(get("p")?).filter(|p| !p.is_empty())?;
    let key = match get("k").unwrap_or("rsa") {
        "rsa" => PublicKey::Rsa(rsa_public_key(&p)?.to_vec()),
        "ed25519" => PublicKey::Ed25519(p.try_into().ok()?),
        _ => return None,
    };
    Some(KeyRecord { key, strict })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const ED_P: &str = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";
    const RSA_P: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDkHlOQoBTzWRiGs5V6NpP3idY6Wk08a5qhdR6wy5bdOKb2jLQiY/J16JYi0Qvx/byYzCNb3W91y3FutACDfzwQ/BC/e/8uBsCR+yz1Lxj+PL6lHvqMKrM3rG4hstT5QjvHO9PzoxZyVYLzBfO2EeC3Ip3G+2kryOTIKT+l/K4w3QIDAQAB";

    fn b64(bytes: &[u8]) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn ed(extra: &str) -> String {
        format!("v=DKIM1; k=ed25519; {extra} p={ED_P}")
    }

    #[test]
    fn the_rfc_8463_keys_parse() {
        let r = key_record(&format!("v=DKIM1; k=ed25519; p={ED_P}")).unwrap();
        assert!(!r.strict);
        let PublicKey::Ed25519(k) = r.key else {
            panic!("not ed25519")
        };
        // RFC 8032 section 7.1 test 1.
        assert_eq!(k[..4], [0xd7, 0x5a, 0x98, 0x01]);

        // The strings of the TXT record joined, with the key split by spaces.
        let spaced = format!("v=DKIM1; k=rsa; p={} {}", &RSA_P[..60], &RSA_P[60..]);
        let r = key_record(&spaced).unwrap();
        let PublicKey::Rsa(der) = r.key else {
            panic!("not rsa")
        };
        assert_eq!(der[..3], [0x30, 0x81, 0x89]);
        assert_eq!(der.len(), 140);
    }

    #[test]
    fn k_defaults_to_rsa_and_v_is_optional() {
        assert!(matches!(
            key_record(&format!("p={RSA_P}")).unwrap().key,
            PublicKey::Rsa(_)
        ));
        assert!(key_record(&format!("v=DKIM2; p={RSA_P}")).is_none());
        assert!(key_record(&format!("k=dsa; p={RSA_P}")).is_none());
    }

    #[test]
    fn a_bare_rsa_public_key_parses() {
        let PublicKey::Rsa(der) = key_record(&format!("p={RSA_P}")).unwrap().key else {
            panic!()
        };
        let r = key_record(&format!("k=rsa; p={}", b64(&der))).unwrap();
        assert_eq!(r.key, PublicKey::Rsa(der));
    }

    #[test]
    fn revoked_testing_and_unusable_records_are_none() {
        assert!(key_record("v=DKIM1; k=rsa; p=").is_none());
        assert!(key_record("v=DKIM1; k=rsa").is_none());
        assert!(key_record(&ed("t=y;")).is_none());
        assert!(key_record(&ed("t=y:s;")).is_none());
        assert!(key_record(&ed("t=s:y;")).is_none());
        assert!(key_record(&ed("s=other;")).is_none());
        assert!(key_record(&ed("s=email:other;")).is_some());
        assert!(key_record(&ed("h=sha1;")).is_none());
        assert!(key_record(&ed("h=sha1:sha256;")).is_some());
    }

    #[test]
    fn flags_and_service_types() {
        assert!(key_record(&ed("t=s;")).unwrap().strict);
        assert!(!key_record(&ed("t=other;")).unwrap().strict);
        assert!(key_record(&ed("s=*;")).is_some());
        assert!(key_record(&ed("s=email;")).is_some());
        assert!(key_record(&ed("s = email : * ;")).is_some());
    }

    #[test]
    fn bad_key_bytes_are_none() {
        assert!(key_record(&format!("k=ed25519; p={}", b64(&[7; 31]))).is_none());
        assert!(key_record(&format!("k=ed25519; p={}", b64(&[7; 33]))).is_none());
        assert!(key_record("k=ed25519; p=!!!!").is_none());
        // Trailing garbage after the SPKI.
        let mut der = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(RSA_P)
                .unwrap()
        };
        der.push(0);
        assert!(key_record(&format!("k=rsa; p={}", b64(&der))).is_none());
        // An Ed25519 key under k=rsa, and the reverse.
        assert!(key_record(&format!("k=rsa; p={ED_P}")).is_none());
        assert!(key_record(&format!("k=ed25519; p={RSA_P}")).is_none());
    }

    proptest! {
        #[test]
        fn never_panics(s in any::<String>()) {
            let _ = key_record(&s);
        }

        #[test]
        fn never_panics_on_tag_soup(
            s in "(v=DKIM1|k=(rsa|ed25519|x)|p=[A-Za-z0-9+/=]{0,60}|t=[ysx:]{0,5}|s=[a-z*:]{0,8}|h=[a-z0-9:]{0,12}|;| )*",
        ) {
            let _ = key_record(&s);
        }

        /// A record that parses is not testing, allows sha256 and email.
        #[test]
        fn a_parse_implies_the_invariants(
            v in prop::sample::select(vec!["", "v=DKIM1;", "v=DKIM2;"]),
            k in prop::sample::select(vec!["", "k=ed25519;", "k=rsa;", "k=x;"]),
            t in prop::sample::select(vec!["", "t=y;", "t=s;", "t=y:s;", "t=s:y;", "t=Y;"]),
            s in prop::sample::select(vec!["", "s=*;", "s=email;", "s=other;", "s=email:x;"]),
            h in prop::sample::select(vec!["", "h=sha256;", "h=sha1;", "h=sha1:sha256;"]),
        ) {
            let txt = format!("{v} {k} {t} {s} {h} p={ED_P}");
            if let Some(rec) = key_record(&txt) {
                let has = |tag: &str, want: &str| {
                    tag_list(&txt).unwrap().iter().any(|(n, val)| {
                        n == tag && colon_list(val).any(|e| e == want)
                    })
                };
                prop_assert!(!has("t", "y"));
                prop_assert_eq!(rec.strict, has("t", "s"));
                if txt.contains("h=") { prop_assert!(has("h", "sha256")); }
                if txt.contains("s=") { prop_assert!(has("s", "*") || has("s", "email")); }
                prop_assert!(!txt.contains("v=DKIM2"));
                prop_assert!(matches!(rec.key, PublicKey::Ed25519(_)) == !k.contains("rsa"));
            }
        }
    }
}
