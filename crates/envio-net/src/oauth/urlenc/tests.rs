use super::*;

#[test]
fn encodes_everything_but_the_unreserved_set() {
    assert_eq!(encode("aZ09-._~"), "aZ09-._~");
    assert_eq!(
        encode("a b+c@d/e:f&g=h#i%"),
        "a%20b%2Bc%40d%2Fe%3Af%26g%3Dh%23i%25"
    );
    assert_eq!(encode("é"), "%C3%A9");
}

#[test]
fn decodes_percent_and_plus() {
    assert_eq!(decode("a%20b+c%2B%C3%A9").as_deref(), Some("a b c+é"));
    assert_eq!(decode("%zz"), None);
    assert_eq!(decode("%4"), None);
    assert_eq!(decode("%FF"), None);
}

#[test]
fn form_joins_encoded_pairs() {
    assert_eq!(form(&[("a", "1 2"), ("b&c", "d=e")]), "a=1%202&b%26c=d%3De");
}
