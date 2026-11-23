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
fn microsoft_graph_has_the_graph_scopes_and_kind() {
    let p = Provider::microsoft_graph().expect("offered");
    assert_eq!(p.client_id, client_ids::MICROSOFT);
    assert_eq!(p.kind, ProviderKind::MicrosoftGraph);
    assert_eq!(
        p.scopes,
        "Mail.ReadWrite Mail.Send offline_access openid email profile"
    );
    // The same sign-in endpoints and redirect as the IMAP provider.
    let imap = Provider::microsoft_template();
    let t = Provider::microsoft_graph_template();
    assert_eq!(t.client_id, "");
    assert_eq!(t.authorize_url, imap.authorize_url);
    assert_eq!(t.token_url, imap.token_url);
    assert_eq!(t.redirect_uri, imap.redirect_uri);
    assert_ne!(ProviderKind::Microsoft, ProviderKind::MicrosoftGraph);
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

#[test]
fn consumer_domains_are_the_narrow_list_never_work_domains() {
    for yes in [
        "a@outlook.com",
        "a@Hotmail.co.uk",
        "a@live.fr",
        "a@msn.com",
        "a@windowslive.com",
        "a@outlook.de.",
    ] {
        assert!(is_consumer_domain(yes), "{yes}");
    }
    for no in [
        "a@office365.com",
        "a@contoso.onmicrosoft.com",
        "a@example.org",
        "a@live.org",
        "a@hotmail.biz",
        "a@ outlook.com",
        "not an address",
    ] {
        assert!(!is_consumer_domain(no), "{no}");
    }
}

#[test]
fn consumer_microsoft_domain_is_the_narrow_list_on_a_bare_domain() {
    for yes in [
        "outlook.com",
        "Hotmail.CO.UK.",
        "live.fr",
        "msn.com",
        "windowslive.com",
    ] {
        assert!(consumer_microsoft_domain(yes), "{yes}");
    }
    for no in [
        "office365.com",
        "foo.onmicrosoft.com",
        "live.org",
        "hotmail.biz",
        "example.org",
        "",
    ] {
        assert!(!consumer_microsoft_domain(no), "{no}");
    }
}

#[test]
fn the_dispatcher_and_the_app_shell_declare_every_redirect_scheme() {
    // The scheme lives in three places: the provider's redirect_uri, the
    // click's url-dispatcher file and AppShell.qml's UriHandler. If one
    // drifts, the browser's redirect never reaches the app (#367).
    let dispatcher = include_str!("../../../../../envio.url-dispatcher");
    let shell = include_str!("../../../../../qml/pages/AppShell.qml");
    let providers = [Provider::microsoft_template()];
    for p in &providers {
        let (scheme, _) = p.redirect_uri.split_once("://").expect("has a scheme");
        assert!(
            dispatcher.contains(&format!("\"protocol\": \"{scheme}\"")),
            "envio.url-dispatcher lacks {scheme}"
        );
        assert!(
            shell.contains(&format!("schemes: [\"{scheme}\"]")),
            "AppShell.qml lacks {scheme}"
        );
    }
}
