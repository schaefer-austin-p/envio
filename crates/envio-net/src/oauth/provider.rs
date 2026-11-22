//! The provider table: endpoints, scopes, hosts, and which addresses use it.

use super::client_ids;
use crate::server::{Endpoint, Security};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderKind {
    /// Microsoft sign-in for IMAP and SMTP with OAuth.
    Microsoft,
    /// Microsoft sign-in with Graph scopes (Outlook.com accounts, #382).
    MicrosoftGraph,
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
    #[doc(hidden)]
    pub fn microsoft_template() -> Provider {
        const BASE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";
        Provider {
            kind: ProviderKind::Microsoft,
            client_id: String::new(),
            authorize_url: format!("{BASE}/authorize"),
            token_url: format!("{BASE}/token"),
            scopes: "https://outlook.office.com/IMAP.AccessAsUser.All \
                     https://outlook.office.com/SMTP.Send offline_access openid email profile",
            redirect_uri: "enviooauth://callback".to_owned(),
            imap: Endpoint::new("outlook.office365.com", 993, Security::Tls),
            smtp: Endpoint::new("smtp.office365.com", 587, Security::StartTls),
        }
    }

    /// Microsoft sign-in with Graph scopes, or `None` while no client ID is
    /// registered. Same app registration and endpoints as `microsoft()`.
    pub fn microsoft_graph() -> Option<Provider> {
        if client_ids::MICROSOFT.is_empty() {
            return None;
        }
        Some(Self::microsoft_graph_template().with_client_id(client_ids::MICROSOFT))
    }

    /// The Graph provider with an empty client ID, like `microsoft_template`.
    /// The Graph backend reads `/me/...` mail paths only, which
    /// `Mail.ReadWrite` covers, so `User.Read` is not requested. The
    /// `imap` and `smtp` endpoints are unused here.
    #[doc(hidden)]
    pub fn microsoft_graph_template() -> Provider {
        Provider {
            kind: ProviderKind::MicrosoftGraph,
            scopes: "Mail.ReadWrite Mail.Send offline_access openid email profile",
            ..Self::microsoft_template()
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
    let Some((first, rest)) = d.split_once('.') else {
        return false;
    };
    let suffixes = match first {
        "outlook" => OUTLOOK,
        "hotmail" => HOTMAIL,
        "live" => LIVE,
        _ => return false,
    };
    suffixes.contains(&rest)
}

// Microsoft's consumer domains by country. An explicit list on purpose: a
// missing domain only means the password path (safe); a false positive
// would offer the wrong provider (live.org, hotmail.biz are not Microsoft's).
const OUTLOOK: &[&str] = &[
    "com", "at", "be", "cl", "cz", "de", "dk", "es", "fr", "hu", "ie", "in", "it", "jp", "kr",
    "lv", "my", "ph", "pt", "sa", "sg", "sk", "co.id", "co.il", "co.th", "com.ar", "com.au",
    "com.br", "com.gr", "com.tr", "com.vn",
];
const HOTMAIL: &[&str] = &[
    "com", "at", "be", "ca", "ch", "cl", "cz", "de", "dk", "es", "fi", "fr", "gr", "hu", "it",
    "kz", "lt", "lv", "nl", "no", "pt", "ro", "rs", "se", "sg", "sk", "co.il", "co.jp", "co.nz",
    "co.th", "co.uk", "com.ar", "com.au", "com.br", "com.gr", "com.mx", "com.pe", "com.tr",
    "com.vn",
];
const LIVE: &[&str] = &[
    "com", "at", "be", "ca", "cl", "cn", "de", "dk", "fi", "fr", "hk", "ie", "in", "it", "jp",
    "nl", "no", "ru", "se", "co.uk", "co.za", "com.ar", "com.au", "com.mx", "com.pt",
];

#[cfg(test)]
mod tests;
