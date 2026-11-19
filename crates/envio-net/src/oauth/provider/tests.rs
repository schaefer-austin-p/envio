use super::*;
use crate::server::Security;

#[test]
fn microsoft_is_on_with_the_registered_client_id() {
    // A UUID: 8-4-4-4-12 hex digits.
    let parts: Vec<&str> = client_ids::MICROSOFT.split('-').collect();
    assert_eq!(
        parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12]
    );
    assert!(
        parts
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
    );
    let p = Provider::microsoft().expect("Microsoft is offered");
    assert_eq!(p.client_id, client_ids::MICROSOFT);
}

#[test]
fn with_client_id_fills_the_id_and_nothing_else() {
    let t = Provider::microsoft_template();
    assert_eq!(t.client_id, "");
    let p = t.clone().with_client_id("cid");
    assert_eq!(p.client_id, "cid");
    assert_eq!(
        Provider {
            client_id: String::new(),
            ..p
        },
        t
    );
}

#[test]
fn microsoft_values() {
    let p = Provider::microsoft_template();
    assert_eq!(p.kind, ProviderKind::Microsoft);
    assert_eq!(
        p.authorize_url,
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
    );
    assert_eq!(
        p.token_url,
        "https://login.microsoftonline.com/common/oauth2/v2.0/token"
    );
    assert_eq!(
        p.scopes,
        "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send \
         offline_access openid email profile"
    );
    assert_eq!(p.redirect_uri, "enviooauth://callback");
    assert_eq!(
        p.imap,
        Endpoint::new("outlook.office365.com", 993, Security::Tls)
    );
    assert_eq!(
        p.smtp,
        Endpoint::new("smtp.office365.com", 587, Security::StartTls)
    );
}

fn ms(email: &str) -> bool {
    provider_kind_for(email) == Some(ProviderKind::Microsoft)
}

#[test]
fn microsoft_domains_are_found() {
    for e in [
        "a@outlook.com",
        "a@Outlook.COM",
        "a@outlook.com.",
        " a@outlook.com ",
        "a@hotmail.com",
        "a@hotmail.co.uk",
        "a@live.nl",
        "a@hotmail.com.br",
        "a@live.com.au",
        "a@outlook.com.br",
        "a@hotmail.com.mx",
        "a@live.co.uk",
        "a@live.com",
        "a@msn.com",
        "a@outlook.de",
        "a@windowslive.com",
        "a@office365.com",
        "a@contoso.onmicrosoft.com",
    ] {
        assert!(ms(e), "{e}");
    }
}

#[test]
fn other_domains_are_not() {
    for e in [
        "a@example.org",
        "a@outlook.com.example.org",
        "a@hotmail.example.org",
        "a@live.org",
        "a@outlook.org",
        "a@hotmail.biz",
        "a@hotmail.com.xx",
        "a@live.com.br",
        "a@outlook.co.uk",
        "a@notoutlook.com",
        "a@live.example.org",
        "a@msn.com.example.org",
        "a@onmicrosoft.com.example.org",
        "outlook.com",
        "a@",
        "",
    ] {
        assert!(!ms(e), "{e}");
    }
}

#[test]
fn every_redirect_scheme_is_letters_and_digits_only() {
    // Ubuntu Touch's lomiri-url-dispatcher only parses schemes matching
    // `[a-z][a-z0-9]*` and refuses anything else (BadURL), so a scheme with a
    // hyphen never gets the sign-in back to the app (#367).
    // Add each new built-in provider to this list.
    let providers = [Provider::microsoft_template()];
    for p in &providers {
        let (scheme, _) = p.redirect_uri.split_once("://").expect("has a scheme");
        let mut chars = scheme.chars();
        let first = chars.next().expect("scheme is not empty");
        assert!(first.is_ascii_lowercase(), "{scheme}");
        assert!(
            chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "{scheme}"
        );
    }
}
