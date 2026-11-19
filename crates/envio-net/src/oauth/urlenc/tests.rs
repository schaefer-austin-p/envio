use super::*;

#[test]
fn safe_code_keeps_plain_codes_up_to_64_characters() {
    assert_eq!(safe_code("server_error"), "server_error");
    assert_eq!(safe_code("."), ".");
    assert_eq!(safe_code(&"a".repeat(64)), "a".repeat(64));
}

#[test]
fn safe_code_hides_everything_else() {
    for s in [
        &"a".repeat(65),
        "",
        "a b",
        " ",
        "a@x",
        "a@example.org",
        "é",
        "a\n",
    ] {
        assert_eq!(safe_code(s), "unknown", "{s:?}");
    }
}

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
