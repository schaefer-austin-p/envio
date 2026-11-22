//! Which accounts are offered "Update sign-in" (#342). Pure.

use envio_net::oauth::{CONSUMER_TENANT, is_consumer_domain};
use envio_store::{AccountSettings, BackendKind};

/// Whether `settings` is an Outlook.com account still on IMAP that can move
/// to Graph. `identity_tid` is the tenant a token named, when one is known:
/// a consumer `tid` qualifies, any other `tid` never does. Without one (old
/// accounts store no identity) the narrow consumer-domain list decides; a
/// custom domain without a `tid` stays on IMAP. The `tid` of the sign-in is
/// the final filter (`worker/oauth.rs`).
pub fn eligible(settings: &AccountSettings, identity_tid: Option<&str>) -> bool {
    if settings.backend != BackendKind::Imap || !settings.is_oauth() || settings.migration.is_some()
    {
        return false;
    }
    match identity_tid {
        Some(tid) => tid == CONSUMER_TENANT,
        None => is_consumer_domain(&settings.email),
    }
}

#[cfg(test)]
mod tests {
    use envio_store::{Migration, Security, Server};

    use super::*;

    const WORK: &str = "11111111-2222-3333-4444-555555555555";

    fn account(email: &str) -> AccountSettings {
        let server = || Server {
            host: "example.org".into(),
            port: 993,
            security: Security::Tls,
            pin: None,
        };
        AccountSettings {
            name: "A".into(),
            email: email.into(),
            username: email.into(),
            backend: BackendKind::Imap,
            imap: Some(server()),
            smtp: Some(server()),
            auth: Some("microsoft".into()),
            migration: None,
            dropped_note: None,
        }
    }

    #[test]
    fn each_consumer_domain_qualifies_without_a_tid() {
        for domain in [
            "outlook.com",
            "outlook.fr",
            "hotmail.com",
            "hotmail.co.uk",
            "live.com",
            "live.nl",
            "msn.com",
            "windowslive.com",
        ] {
            assert!(eligible(&account(&format!("a@{domain}")), None), "{domain}");
        }
    }

    #[test]
    fn work_domains_and_other_domains_never_qualify_by_name() {
        for domain in [
            "office365.com",
            "contoso.onmicrosoft.com",
            "example.org",
            "live.org",
        ] {
            assert!(
                !eligible(&account(&format!("a@{domain}")), None),
                "{domain}"
            );
        }
    }

    #[test]
    fn a_custom_domain_needs_a_consumer_tid() {
        let custom = account("a@example.org");
        assert!(!eligible(&custom, None), "no tid, custom domain");
        assert!(eligible(&custom, Some(CONSUMER_TENANT)));
        assert!(!eligible(&custom, Some(WORK)));
    }

    #[test]
    fn a_work_tid_wins_over_a_consumer_domain() {
        assert!(!eligible(&account("a@outlook.com"), Some(WORK)));
    }

    #[test]
    fn only_an_oauth_imap_account_not_already_moving() {
        let mut password = account("a@outlook.com");
        password.auth = None;
        assert!(!eligible(&password, None));
        let mut graph = account("a@outlook.com");
        graph.backend = BackendKind::Graph;
        assert!(!eligible(&graph, None));
        let mut moving = account("a@outlook.com");
        moving.migration = Some(Migration {
            step: 1,
            failures: 0,
        });
        assert!(!eligible(&moving, None));
    }
}
