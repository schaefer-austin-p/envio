//! #221: `Detected` carries the app-password hint for Gmail, iCloud, Yahoo
//! and AOL addresses and for Google Workspace domains found by MX, and none
//! for any other address. A Workspace domain that autoconfig could not find
//! gets Gmail's servers.

mod support;

use envio_core::hooks::{SetupFake, fake_setup};
use envio_core::{AccountSetup, AppPasswordHint, Command, Event};
use envio_net::autoconfig::{Found, Hosting, Setting, Source};
use envio_net::{Endpoint, Security};
use support::Harness;

fn ispdb_found(domain: &str) -> Found {
    let setting = |host: String| Setting {
        endpoint: Endpoint::new(&host, 993, Security::Tls),
        username: "me".into(),
    };
    Found {
        imap: setting(format!("imap.{domain}")),
        smtp: setting(format!("smtp.{domain}")),
        source: Source::Ispdb,
        confirm: false,
    }
}

/// Detect `email` with the fake's `found` and MX verdict; the setup and
/// hint of the `Detected` event.
fn detected(
    email: &str,
    found: Option<Found>,
    hosting: Hosting,
) -> (AccountSetup, Option<AppPasswordHint>, String) {
    let h = Harness::start();
    fake_setup(
        email,
        SetupFake {
            found,
            hosting: Some(hosting),
            ..Default::default()
        },
    );
    h.send(Command::Detect {
        email: email.into(),
    });
    h.wait(|e| match e {
        Event::Detected {
            setup,
            app_password,
            text,
            ..
        } => Some((setup.clone(), app_password.clone(), text.clone())),
        _ => None,
    })
}

#[test]
fn a_gmail_address_found_through_ispdb_gets_the_google_hint() {
    let (_, hint, _) = detected(
        "ann@gmail.com",
        Some(ispdb_found("gmail.com")),
        Hosting::Unknown,
    );
    let hint = hint.expect("a hint");
    assert_eq!(
        hint.text,
        "Google needs an app password, not your normal password."
    );
    assert_eq!(hint.url, "https://myaccount.google.com/apppasswords");
}

#[test]
fn icloud_yahoo_and_aol_addresses_name_their_provider() {
    for (email, domain, name, url) in [
        (
            "ann@icloud.com",
            "icloud.com",
            "iCloud",
            "https://account.apple.com/account/manage",
        ),
        (
            "ann@yahoo.com",
            "yahoo.com",
            "Yahoo",
            "https://login.yahoo.com/account/security",
        ),
        (
            "ann@aol.com",
            "aol.com",
            "AOL",
            "https://login.aol.com/account/security",
        ),
    ] {
        let (_, hint, _) = detected(email, Some(ispdb_found(domain)), Hosting::Unknown);
        let hint = hint.unwrap_or_else(|| panic!("a hint for {email}"));
        assert_eq!(
            hint.text,
            format!("{name} needs an app password, not your normal password.")
        );
        assert_eq!(hint.url, url);
    }
}

#[test]
fn an_address_of_another_provider_has_no_hint() {
    let (_, hint, _) = detected(
        "ann@example.org",
        Some(ispdb_found("example.org")),
        Hosting::Unknown,
    );
    assert_eq!(hint, None);
}

#[test]
fn a_workspace_domain_found_by_mx_gets_the_hint_and_gmails_servers() {
    // No autoconfig answer: the plan's guess, `imap.studio.example`.
    let (setup, hint, text) = detected("ann@studio.example", None, Hosting::Google);
    let hint = hint.expect("a hint");
    assert_eq!(
        hint.text,
        "Google needs an app password, not your normal password."
    );
    assert_eq!(hint.url, "https://myaccount.google.com/apppasswords");
    assert_eq!(setup.imap.host, "imap.gmail.com");
    assert_eq!(setup.smtp.host, "smtp.gmail.com");
    assert_eq!(setup.source, Some(Source::Provider));
    assert_eq!(text, "Settings found at your mail provider");
}

#[test]
fn a_workspace_domain_with_its_own_autoconfig_keeps_those_servers() {
    // Another address than the test above: the fakes are keyed by address.
    let (setup, hint, _) = detected(
        "ann@own.example",
        Some(ispdb_found("own.example")),
        Hosting::Google,
    );
    assert!(hint.is_some());
    assert_eq!(setup.imap.host, "imap.own.example");
}

#[test]
fn an_unknown_domain_without_an_answer_stays_a_guess_with_no_hint() {
    let (setup, hint, _) = detected("ann@nowhere.example", None, Hosting::Unknown);
    assert_eq!(hint, None);
    assert_eq!(setup.imap.host, "imap.nowhere.example");
    assert_eq!(setup.source, Some(Source::Guess));
}
