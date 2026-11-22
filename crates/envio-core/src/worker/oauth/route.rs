//! Which sign-in a token belongs to: the tenant (`tid`) of the id token
//! decides between Graph and IMAP, whatever the user picked. Pure.

use envio_net::oauth::{CONSUMER_TENANT, Identity, ProviderKind};

/// Who the first token named; the second sign-in must name the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    who: Option<String>,
    tid: Option<String>,
}

impl Seen {
    pub(crate) fn of(identity: Option<&Identity>) -> Self {
        Self {
            who: identity
                .and_then(|i| i.email.clone().or_else(|| i.preferred_username.clone()))
                .map(|w| w.to_ascii_lowercase()),
            tid: identity.and_then(|i| i.tid.clone()),
        }
    }

    pub(crate) fn who(&self) -> Option<&str> {
        self.who.as_deref()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// The token fits the picked path.
    Fits,
    /// The other kind of account: `personal` says which kind it is.
    Other { personal: bool },
    /// Nothing is saved: a second token for someone else, or no tenant.
    Refuse,
}

/// `first`: what the first token named, once a mismatch was shown. There is
/// no third sign-in, so with `first` set the answer is `Fits` or `Refuse`.
pub(crate) fn route(
    kind: ProviderKind,
    first: Option<&Seen>,
    identity: Option<&Identity>,
) -> Route {
    let seen = Seen::of(identity);
    let personal = seen.tid.as_deref() == Some(CONSUMER_TENANT);
    let fits = match kind {
        ProviderKind::MicrosoftGraph => personal,
        // No `tid` was always taken on the IMAP path.
        ProviderKind::Microsoft => !personal,
    };
    match first {
        Some(first) if fits && *first == seen => Route::Fits,
        Some(_) => Route::Refuse,
        None if fits => Route::Fits,
        // A Graph pick that names no tenant cannot be told from a work account.
        None if seen.tid.is_none() => Route::Refuse,
        None => Route::Other { personal },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: &str = "11111111-2222-3333-4444-555555555555";

    fn id(who: &str, tid: Option<&str>) -> Identity {
        Identity {
            email: Some(who.into()),
            preferred_username: None,
            tid: tid.map(Into::into),
        }
    }

    #[test]
    fn the_tenant_decides_not_the_pick() {
        let graph = ProviderKind::MicrosoftGraph;
        let imap = ProviderKind::Microsoft;
        let me = id("anna@example.org", Some(CONSUMER_TENANT));
        let work = id("anna@example.org", Some(WORK));
        assert_eq!(route(graph, None, Some(&me)), Route::Fits);
        assert_eq!(route(imap, None, Some(&work)), Route::Fits);
        assert_eq!(
            route(graph, None, Some(&work)),
            Route::Other { personal: false }
        );
        assert_eq!(
            route(imap, None, Some(&me)),
            Route::Other { personal: true }
        );
    }

    #[test]
    fn no_tenant_is_taken_for_imap_and_refused_for_graph() {
        let none = id("anna@example.org", None);
        assert_eq!(
            route(ProviderKind::Microsoft, None, Some(&none)),
            Route::Fits
        );
        assert_eq!(
            route(ProviderKind::MicrosoftGraph, None, Some(&none)),
            Route::Refuse
        );
        assert_eq!(
            route(ProviderKind::MicrosoftGraph, None, None),
            Route::Refuse
        );
    }

    #[test]
    fn the_second_token_must_name_the_same_account_and_never_asks_again() {
        let first = Seen::of(Some(&id("Anna@example.org", Some(WORK))));
        let imap = ProviderKind::Microsoft;
        let same = id("anna@EXAMPLE.org", Some(WORK));
        assert_eq!(route(imap, Some(&first), Some(&same)), Route::Fits);
        let other_who = id("bob@example.org", Some(WORK));
        assert_eq!(route(imap, Some(&first), Some(&other_who)), Route::Refuse);
        let other_tid = id(
            "anna@example.org",
            Some("99999999-2222-3333-4444-555555555555"),
        );
        assert_eq!(route(imap, Some(&first), Some(&other_tid)), Route::Refuse);
        // The tenant is the wrong kind again: refused, not asked a third time.
        let personal = id("anna@example.org", Some(CONSUMER_TENANT));
        assert_eq!(route(imap, Some(&first), Some(&personal)), Route::Refuse);
    }
}
