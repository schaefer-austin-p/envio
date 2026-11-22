//! #384 Decision 13: a known consumer domain goes straight to Graph, where
//! the Graph provider is set up. No network: Detect and the start of a
//! sign-in never reach the token endpoint.

mod support;

use std::time::Instant;

use envio_core::{Command, Event, OAuthFailure, OAuthOutcome, ProviderKind};
use envio_net::oauth::Provider;
use support::Harness;

const GRAPH_SCOPE: &str = "Mail.ReadWrite";
const IMAP_SCOPE: &str = "IMAP.AccessAsUser.All";

fn provider(kind: ProviderKind) -> Provider {
    let template = match kind {
        ProviderKind::Microsoft => Provider::microsoft_template(),
        ProviderKind::MicrosoftGraph => Provider::microsoft_graph_template(),
    };
    template.with_client_id("test-client-id")
}

fn core(kinds: &[ProviderKind]) -> Harness {
    let mut h = Harness::unstarted();
    h.oauth = kinds.iter().map(|k| provider(*k)).collect();
    h.start_core();
    h
}

struct Offer {
    servers: String,
    text: String,
    provider: String,
}

fn detect(h: &Harness, email: &str) -> Offer {
    h.send(Command::Detect {
        email: email.into(),
    });
    h.wait(|e| match e {
        Event::Detected {
            servers,
            text,
            oauth: Some(offer),
            ..
        } => Some(Offer {
            servers: servers.clone(),
            text: text.clone(),
            provider: offer.provider.clone(),
        }),
        _ => None,
    })
}

fn start(h: &Harness, email: &str, kind: Option<ProviderKind>) -> String {
    h.send(Command::StartOAuth {
        email: email.into(),
        account: None,
        kind,
    });
    h.wait(|e| match e {
        Event::OAuthBrowser { url, .. } => Some(url.clone()),
        _ => None,
    })
}

#[test]
fn detect_of_a_consumer_address_offers_graph_with_no_servers() {
    let h = core(&[ProviderKind::Microsoft, ProviderKind::MicrosoftGraph]);
    let offer = detect(&h, "ann@outlook.com");
    assert_eq!(offer.provider, "Microsoft");
    assert_eq!(offer.servers, "", "Graph has no IMAP/SMTP hosts to show");
    assert_eq!(offer.text, "Sign in with Microsoft to add this account");
}

#[test]
fn detect_of_a_consumer_address_with_only_imap_is_the_imap_offer() {
    let h = core(&[ProviderKind::Microsoft]);
    let offer = detect(&h, "ann@outlook.com");
    assert!(
        offer.servers.contains("outlook.office365.com"),
        "{}",
        offer.servers
    );
}

#[test]
fn detect_of_a_work_address_stays_imap_with_both_providers() {
    let h = core(&[ProviderKind::Microsoft, ProviderKind::MicrosoftGraph]);
    let offer = detect(&h, "ann@office365.com");
    assert!(
        offer.servers.contains("outlook.office365.com"),
        "{}",
        offer.servers
    );
}

#[test]
fn start_without_a_pick_builds_a_graph_url_for_a_consumer_address() {
    let h = core(&[ProviderKind::Microsoft, ProviderKind::MicrosoftGraph]);
    let url = start(&h, "ann@outlook.com", None);
    assert!(url.contains(GRAPH_SCOPE), "{url}");
    assert!(!url.contains(IMAP_SCOPE), "{url}");
}

#[test]
fn start_without_a_pick_for_a_consumer_address_with_only_imap_is_imap() {
    let h = core(&[ProviderKind::Microsoft]);
    let url = start(&h, "ann@outlook.com", None);
    assert!(url.contains(IMAP_SCOPE), "{url}");
}

#[test]
fn start_for_a_work_address_stays_imap() {
    let h = core(&[ProviderKind::Microsoft, ProviderKind::MicrosoftGraph]);
    let url = start(&h, "ann@office365.com", None);
    assert!(url.contains(IMAP_SCOPE), "{url}");
}

#[test]
fn an_explicit_pick_wins_over_the_default() {
    let h = core(&[ProviderKind::Microsoft, ProviderKind::MicrosoftGraph]);
    let url = start(&h, "ann@outlook.com", Some(ProviderKind::Microsoft));
    assert!(url.contains(IMAP_SCOPE), "{url}");
}

#[test]
fn only_the_graph_provider_does_not_refuse_a_consumer_address() {
    let h = core(&[ProviderKind::MicrosoftGraph]);
    let offer = detect(&h, "ann@outlook.com");
    assert_eq!(offer.provider, "Microsoft");
}

#[test]
fn with_neither_provider_a_consumer_address_is_refused_and_cannot_sign_in() {
    let h = core(&[]);
    h.send(Command::Detect {
        email: "ann@outlook.com".into(),
    });
    h.wait(|e| matches!(e, Event::SetupRefused { .. }).then_some(()));
    h.send(Command::StartOAuth {
        email: "ann@outlook.com".into(),
        account: None,
        kind: None,
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match h.events.recv_timeout(left) {
            Ok(Event::OAuthDone { outcome, .. }) => {
                assert_eq!(outcome, OAuthOutcome::Failed(OAuthFailure::Unavailable));
                break;
            }
            Ok(Event::OAuthBrowser { .. }) => panic!("a sign-in began"),
            Ok(_) => {}
            Err(_) => panic!("no answer"),
        }
    }
}
