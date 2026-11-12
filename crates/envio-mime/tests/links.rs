//! Link targets: the sanitiser keeps them honest, `link_parts` splits them.

use envio_mime::{RenderOptions, sanitize_html};

fn clean(html: &str) -> String {
    sanitize_html(html, &RenderOptions::default()).html
}

#[test]
fn format_characters_in_a_link_path_are_percent_encoded() {
    for (href, want) in [
        (
            "https://a.example/x\u{202E}gpj",
            "https://a.example/x%E2%80%AEgpj",
        ),
        (
            "https://a.example/?q=\u{200B}1#\u{2066}f",
            "https://a.example/?q=%E2%80%8B1#%E2%81%A6f",
        ),
        (
            "mailto:a@b.example?subject=\u{FEFF}hi",
            "mailto:a@b.example?subject=%EF%BB%BFhi",
        ),
        (
            "https://a.example/\u{061C}\u{200E}\u{200F}",
            "https://a.example/%D8%9C%E2%80%8E%E2%80%8F",
        ),
    ] {
        let html = format!(r#"<a href="{href}">x</a>"#);
        assert_eq!(
            clean(&html),
            format!(r#"<a href="{want}">x</a>"#),
            "{href:?}"
        );
    }
}

#[test]
fn format_characters_in_a_link_host_or_scheme_lose_the_href() {
    for href in [
        "https://bank.example\u{200B}.evil.example/",
        "https://\u{202E}moc.evil.example/",
        "https://user\u{2067}@evil.example/",
        "https\u{200B}://a.example/",
        "https://a.example\u{200D}",
    ] {
        let html = format!(r#"<a href="{href}">x</a>"#);
        assert_eq!(clean(&html), "<a>x</a>", "{href:?}");
    }
}

#[test]
fn ordinary_unicode_links_are_untouched() {
    let out = clean("<a href=\"https://例え.jp/パス?q=é\">x</a>");
    assert!(out.contains("例え.jp/パス?q=é"), "{out}");
}

use envio_mime::{LinkParts, link_parts};

fn parts(url: &str) -> LinkParts {
    link_parts(url).unwrap_or_else(|| panic!("{url:?}"))
}

#[test]
fn plain_https() {
    assert_eq!(
        parts("HTTPS://Example.com:8443/a/b?q=1#f"),
        LinkParts {
            scheme: "https".into(),
            userinfo: None,
            host: "example.com".into(),
            punycode: None,
            port: Some("8443".into()),
            rest: "/a/b?q=1#f".into(),
        }
    );
}

#[test]
fn userinfo_trick_shows_the_real_host() {
    let p = parts("https://bank.example@evil.example/login");
    assert_eq!(p.userinfo.as_deref(), Some("bank.example"));
    assert_eq!(p.host, "evil.example");
    assert_eq!(p.rest, "/login");
}

#[test]
fn idn_host_shows_unicode_and_punycode() {
    let p = parts("https://例え.jp/x");
    assert_eq!(p.host, "例え.jp");
    assert_eq!(p.punycode.as_deref(), Some("xn--r8jz45g.jp"));
    let p = parts("https://xn--r8jz45g.jp/x");
    assert_eq!(p.host, "例え.jp");
    assert_eq!(p.punycode.as_deref(), Some("xn--r8jz45g.jp"));
}

#[test]
fn bidi_in_the_path_stays_in_rest_and_in_the_host_is_refused() {
    let p = parts("https://a.example/doc\u{202E}fdp.exe");
    assert_eq!(p.host, "a.example");
    assert_eq!(p.rest, "/doc\u{202E}fdp.exe");
    assert_eq!(link_parts("https://a\u{200B}.example/"), None);
    assert_eq!(link_parts("https://\u{202E}a.example/"), None);
}

#[test]
fn mailto_splits_at_the_address() {
    let p = parts("mailto:jane@例え.jp?subject=Hi");
    assert_eq!(p.scheme, "mailto");
    assert_eq!(p.userinfo.as_deref(), Some("jane"));
    assert_eq!(p.host, "例え.jp");
    assert_eq!(p.rest, "?subject=Hi");
}

#[test]
fn bracketed_hosts_must_be_ipv6_literals() {
    for url in [
        "https://[bank.example]/",
        "https://[bank.example",
        "https://[::1]evil.example/",
        "https://[::1]:80x/",
    ] {
        assert_eq!(link_parts(url), None, "{url}");
    }
    let p = parts("https://[::1]:8080/x");
    assert_eq!(
        (p.host.as_str(), p.port.as_deref()),
        ("[::1]", Some("8080"))
    );
}

#[test]
fn other_or_broken_links_have_no_parts() {
    for url in [
        "ftp://a.example/",
        "https:///x",
        "https://",
        "mailto:nobody",
        "x",
    ] {
        assert_eq!(link_parts(url), None, "{url}");
    }
    assert_eq!(parts("http://[::1]:80/").host, "[::1]");
}

#[test]
fn plain_text_links_get_the_same_href_rules() {
    use envio_mime::plain_to_rich;
    let out = plain_to_rich("see https://\u{202E}moc.evil.example/ now", false, false);
    assert!(!out.contains("<a"), "{out}");
    let out = plain_to_rich("see https://a.example/x\u{202E}y now", false, false);
    assert!(
        out.contains("href=\"https://a.example/x%E2%80%AEy\""),
        "{out}"
    );
}

#[test]
fn backslash_in_the_authority_is_refused() {
    assert_eq!(link_parts("https://evil.example\\@bank.example/"), None);
    assert_eq!(
        clean("<a href=\"https://evil.example\\@bank.example/\">x</a>"),
        "<a>x</a>"
    );
}

#[test]
fn format_characters_in_userinfo_or_port_are_refused() {
    assert_eq!(
        link_parts("https://bank.example\u{202E}@evil.example/"),
        None
    );
    assert_eq!(link_parts("https://a.example:8\u{200B}0/"), None);
}

#[test]
fn percent_encoded_hosts_show_the_host_that_opens() {
    assert_eq!(parts("https://b%61nk.example/").host, "bank.example");
    assert_eq!(link_parts("https://a%E2%80%AEb.example/"), None);
}

#[test]
fn names_lose_bidi_controls_but_keep_joiners() {
    use envio_mime::{Address, strip_bidi_controls};
    let persian = "\u{0645}\u{0647}\u{062F}\u{06CC}\u{200C}\u{0632}\u{0627}\u{062F}\u{0647}";
    let emoji = "\u{1F469}\u{200D}\u{1F4BB} Ana";
    let name = |n| Address::new(Some(n), "a@x.example").name;
    assert_eq!(name(persian).as_deref(), Some(persian));
    assert_eq!(name(emoji).as_deref(), Some(emoji));
    assert_eq!(name("J\u{202E}ane\u{2066}").as_deref(), Some("Jane"));
    assert_eq!(
        strip_bidi_controls("a\u{061C}\u{200E}\u{200F}\u{202A}\u{2069}b\u{200B}"),
        "ab\u{200B}"
    );
    assert_eq!(Address::parse("a@x\u{200D}.example"), None);
}

#[test]
fn decoded_hosts_with_forbidden_code_points_are_refused() {
    for url in [
        "https://bank.example%40evil.example/",
        "https://evil.example%2Fbank.example/",
        "https://%5B::1%5D/",
        "https://a%3Ab.example/",
        "https://a%5Cb.example/",
        "https://a%3Fb.example/",
        "https://a%23b.example/",
        "https://a%25b.example/",
        "https://a%20b.example/",
        "https://a%3Cb.example/",
    ] {
        assert_eq!(link_parts(url), None, "{url}");
    }
}

#[test]
fn the_format_character_set_is_complete() {
    for c in ["\u{0890}", "\u{110CD}", "\u{13430}", "\u{1BCA1}"] {
        let out = clean(&format!("<a href=\"https://a.example/x{c}y\">x</a>"));
        assert!(!out.contains(c), "{c:?}");
    }
}

#[test]
fn fullwidth_at_and_slash_do_not_forge_a_host() {
    assert_eq!(link_parts("https://paypal.com\u{FF20}evil.example/"), None);
    assert_eq!(link_parts("https://evil.example\u{FF0F}x/"), None);
}

#[test]
fn numeric_ipv4_hosts_are_shown_as_the_browser_reaches_them() {
    for (url, host) in [
        ("http://3232235777/", "192.168.1.1"),
        ("http://0x7f.1/", "127.0.0.1"),
        ("https://0300.0250.1.1/", "192.168.1.1"),
        ("http://192.168.1.1./", "192.168.1.1"),
        ("http://10.0.0.1:8080/", "10.0.0.1"),
    ] {
        assert_eq!(
            link_parts(url).unwrap_or_else(|| panic!("{url}")).host,
            host,
            "{url}"
        );
    }
    // Looks numeric but is not an address, or does not fit.
    for url in [
        "http://4294967296/",
        "http://1.2.3.4.5/",
        "http://256.1.1.1/",
        "http://1.2.3.999999/",
    ] {
        assert_eq!(link_parts(url), None, "{url}");
    }
    assert_eq!(link_parts("http://1.example/").unwrap().host, "1.example");
}

#[test]
fn a_port_above_65535_is_malformed() {
    assert_eq!(link_parts("https://a.example:65536/"), None);
    assert!(link_parts("https://a.example:65535/").is_some());
}
