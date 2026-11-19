//! Just enough DER to accept the two spellings of an RSA key in a DKIM `p=`.

/// The RSAPublicKey inside a SubjectPublicKeyInfo for rsaEncryption, or
/// `der` itself when it already is a bare RSAPublicKey (a SEQUENCE of two
/// INTEGERs). Strict DER lengths; nothing after the outer SEQUENCE.
pub(super) fn rsa_public_key(der: &[u8]) -> Option<&[u8]> {
    const RSA_OID: &[u8] = &[
        0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01,
    ];
    if is_rsa_public_key(der) {
        return Some(der);
    }
    // SPKI: SEQUENCE { SEQUENCE { OID rsaEncryption, NULL? }, BIT STRING }.
    let (outer, rest) = tlv(der, 0x30)?;
    if !rest.is_empty() {
        return None;
    }
    let (alg, rest) = tlv(outer, 0x30)?;
    let params = alg.strip_prefix(RSA_OID)?;
    if !params.is_empty() && params != [0x05, 0x00] {
        return None;
    }
    let (bits, rest) = tlv(rest, 0x03)?;
    if !rest.is_empty() {
        return None;
    }
    let (unused, key) = bits.split_first()?;
    (*unused == 0 && is_rsa_public_key(key)).then_some(key)
}

/// A bare RSAPublicKey: SEQUENCE { INTEGER n, INTEGER e }, nothing after it.
fn is_rsa_public_key(der: &[u8]) -> bool {
    let Some((outer, rest)) = tlv(der, 0x30) else {
        return false;
    };
    if !rest.is_empty() {
        return false;
    }
    let Some((n, after_n)) = tlv(outer, 0x02) else {
        return false;
    };
    let Some((e, after_e)) = tlv(after_n, 0x02) else {
        return false;
    };
    !n.is_empty() && !e.is_empty() && after_e.is_empty()
}

/// One DER TLV with the expected tag: (contents, the bytes after it).
fn tlv(input: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (&t, rest) = input.split_first()?;
    if t != tag {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = match first {
        n if n < 0x80 => (usize::from(n), rest),
        0x81 => {
            let (&n, rest) = rest.split_first()?;
            (n >= 0x80).then_some((usize::from(n), rest))?
        }
        0x82 => {
            let (bytes, rest) = rest.split_at_checked(2)?;
            let n = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
            (n >= 0x100).then_some((n, rest))?
        }
        _ => return None, // keys over 64 KiB are not DKIM keys
    };
    rest.split_at_checked(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// RFC 8463 A.2 `test._domainkey` key, base64.
    const SPKI_B64: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDkHlOQoBTzWRiGs5V6NpP3idY6Wk08a5qhdR6wy5bdOKb2jLQiY/J16JYi0Qvx/byYzCNb3W91y3FutACDfzwQ/BC/e/8uBsCR+yz1Lxj+PL6lHvqMKrM3rG4hstT5QjvHO9PzoxZyVYLzBfO2EeC3Ip3G+2kryOTIKT+l/K4w3QIDAQAB";

    fn spki() -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(SPKI_B64)
            .unwrap()
    }

    #[test]
    fn spki_gives_the_inner_rsa_public_key() {
        let der = spki();
        let inner = rsa_public_key(&der).unwrap();
        // The 1024-bit key: SEQUENCE (0x30 0x81 0x89) { INTEGER n, INTEGER e }.
        assert_eq!(&inner[..3], &[0x30, 0x81, 0x89]);
        assert_eq!(inner.len(), 0x89 + 3);
        assert!(der.ends_with(inner));
    }

    #[test]
    fn a_bare_rsa_public_key_is_returned_whole() {
        let der = spki();
        let inner = rsa_public_key(&der).unwrap().to_vec();
        assert_eq!(rsa_public_key(&inner), Some(&inner[..]));
    }

    #[test]
    fn trailing_garbage_or_truncation_is_rejected() {
        let mut der = spki();
        let inner = rsa_public_key(&der).unwrap().to_vec();
        der.push(0);
        assert_eq!(rsa_public_key(&der), None);
        let mut bare = inner.clone();
        bare.push(0);
        assert_eq!(rsa_public_key(&bare), None);
        assert_eq!(rsa_public_key(&inner[..inner.len() - 1]), None);
        assert_eq!(rsa_public_key(&spki()[..40]), None);
    }

    #[test]
    fn other_algorithms_and_shapes_are_rejected() {
        // The SPKI with the OID's last byte changed (not rsaEncryption).
        let mut der = spki();
        let at = der
            .windows(3)
            .position(|w| w == [0x01, 0x01, 0x01])
            .unwrap();
        der[at + 2] = 0x02;
        assert_eq!(rsa_public_key(&der), None);
        // Something other than NULL after the OID in the AlgorithmIdentifier.
        let mut der = spki();
        let at = der.windows(2).position(|w| w == [0x05, 0x00]).unwrap();
        der[at] = 0x04;
        assert_eq!(rsa_public_key(&der), None);
        // A non-minimal length (0x81 0x05).
        assert_eq!(rsa_public_key(&[0x30, 0x81, 0x05, 1, 2, 3, 4, 5]), None);
        assert_eq!(rsa_public_key(&[]), None);
        assert_eq!(rsa_public_key(&[0x30, 0x00]), None);
    }

    #[test]
    fn the_bit_string_must_have_no_unused_bits() {
        let mut der = spki();
        let at = der
            .windows(4)
            .position(|w| w == [0x03, 0x81, 0x8d, 0x00])
            .unwrap();
        assert!(rsa_public_key(&der).is_some());
        der[at + 3] = 0x01;
        assert_eq!(rsa_public_key(&der), None);
    }

    #[test]
    fn lengths_must_be_minimal() {
        // SEQUENCE { INTEGER 1, INTEGER 3 } is a valid bare key.
        assert!(rsa_public_key(&[0x30, 0x06, 2, 1, 1, 2, 1, 3]).is_some());
        // The same with the outer length in the long forms, which are not minimal.
        assert_eq!(rsa_public_key(&[0x30, 0x81, 0x06, 2, 1, 1, 2, 1, 3]), None);
        assert_eq!(
            rsa_public_key(&[0x30, 0x82, 0x00, 0x06, 2, 1, 1, 2, 1, 3]),
            None
        );
        // And a minimal two-byte length is still fine (a 256-byte modulus).
        let mut key = vec![0x30, 0x82, 0x01, 0x07, 2, 0x82, 0x01, 0x00];
        key.extend([0x80; 256]);
        key.extend([2, 1, 3]);
        assert!(rsa_public_key(&key).is_some());
    }

    #[test]
    fn integers_must_not_be_empty() {
        assert_eq!(rsa_public_key(&[0x30, 0x05, 2, 0, 2, 1, 3]), None);
        assert_eq!(rsa_public_key(&[0x30, 0x05, 2, 1, 1, 2, 0]), None);
        assert_eq!(rsa_public_key(&[0x30, 0x04, 2, 0, 2, 0]), None);
    }

    proptest! {
        #[test]
        fn never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
            let _ = rsa_public_key(&bytes);
        }

        #[test]
        fn never_panics_on_mutated_keys(at in 0usize..162, byte in any::<u8>()) {
            let mut der = spki();
            der[at] = byte;
            let _ = rsa_public_key(&der);
        }
    }
}
