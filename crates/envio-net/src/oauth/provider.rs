//! The provider table: endpoints, scopes, hosts, and which addresses use it.

use super::client_ids;
use crate::server::{Endpoint, Security};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Microsoft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub kind: ProviderKind,
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub scopes: &'static str,
    pub redirect_uri: String,
    pub imap: Endpoint,
    pub smtp: Endpoint,
}

impl Provider {
    /// Microsoft sign-in, or `None` while no client ID is registered.
    pub fn microsoft() -> Option<Provider> {
        if client_ids::MICROSOFT.is_empty() {
            return None;
        }
        Some(Self::microsoft_template().with_client_id(client_ids::MICROSOFT))
    }

    /// Microsoft's endpoints and hosts with an empty client ID. Tests and
    /// `envio-core::Config` fill it in with `with_client_id`.
    pub fn microsoft_template() -> Provider {
        const BASE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";
        Provider {
            kind: ProviderKind::Microsoft,
            client_id: String::new(),
            authorize_url: format!("{BASE}/authorize"),
            token_url: format!("{BASE}/token"),
            scopes: "https://outlook.office.com/IMAP.AccessAsUser.All \
                     https://outlook.office.com/SMTP.Send offline_access openid email profile",
            redirect_uri: "envio-oauth://callback".to_owned(),
            imap: Endpoint::new("outlook.office365.com", 993, Security::Tls),
            smtp: Endpoint::new("smtp.office365.com", 587, Security::StartTls),
        }
    }

    pub fn with_client_id(mut self, id: &str) -> Self {
        self.client_id = id.to_owned();
        self
    }
}

/// The provider that offers OAuth for this address's domain, if any. Pure
/// table; case-insensitive, a trailing dot is ignored. Custom domains hosted
/// at Microsoft are #221's MX check, which feeds this table's answer.
pub fn provider_kind_for(email: &str) -> Option<ProviderKind> {
    let (_, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    microsoft_domain(&domain).then_some(ProviderKind::Microsoft)
}

fn microsoft_domain(d: &str) -> bool {
    if matches!(d, "msn.com" | "windowslive.com" | "office365.com") {
        return true;
    }
    if d.strip_suffix(".onmicrosoft.com")
        .is_some_and(|t| !t.is_empty() && !t.contains('.'))
    {
        return true;
    }
    // outlook.*, hotmail.*, live.*: the rest must look like a country or
    // generic suffix ("com", "de", "co.uk", "com.au"), not a registrable domain.
    let Some((first, rest)) = d.split_once('.') else {
        return false;
    };
    matches!(first, "outlook" | "hotmail" | "live") && is_suffix(rest)
}

fn is_suffix(rest: &str) -> bool {
    let labels: Vec<&str> = rest.split('.').collect();
    let alpha = |l: &str, lo: usize, hi: usize| {
        (lo..=hi).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_lowercase())
    };
    match labels.as_slice() {
        [tld] => alpha(tld, 2, 3),
        [sld, tld] => matches!(*sld, "co" | "com") && alpha(tld, 2, 2),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
