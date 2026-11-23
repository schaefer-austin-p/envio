//! Who hosts a domain's mail, from its MX records (#384). Pure: the
//! lookup is `mx.rs`.

#[cfg(test)]
mod tests;

/// One MX record: lower preference is tried first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxRecord {
    pub preference: u16,
    pub host: String,
}

/// What the MX records say about the domain's mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosting {
    Unknown,
    Microsoft365,
    /// Google Workspace (or Gmail): its mail wants an app password (#221).
    Google,
}

/// `Microsoft365` when the most-preferred MX records are all Exchange
/// Online hosts, `Google` when they are all Google's, else `Unknown`.
/// Strict on purpose: a false positive sends the user to a sign-in that
/// cannot work for them (or a hint that does not apply).
pub fn hosting_of(records: &[MxRecord]) -> Hosting {
    let Some(best) = records.iter().map(|r| r.preference).min() else {
        return Hosting::Unknown;
    };
    let mut best_records = records.iter().filter(|r| r.preference == best);
    if best_records.clone().all(|r| exchange_online(&r.host)) {
        Hosting::Microsoft365
    } else if best_records.all(|r| google_mail(&r.host)) {
        Hosting::Google
    } else {
        Hosting::Unknown
    }
}

/// Google's MX hosts: `aspmx.l.google.com`, `altN.aspmx.l.google.com`,
/// `aspmx[N].googlemail.com` and `smtp.google.com` (the single-record
/// setup).
fn google_mail(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if matches!(host.as_str(), "aspmx.l.google.com" | "smtp.google.com") {
        return true;
    }
    if let Some(alt) = host.strip_suffix(".aspmx.l.google.com") {
        return alt
            .strip_prefix("alt")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    }
    host.strip_suffix(".googlemail.com").is_some_and(|label| {
        label
            .strip_prefix("aspmx")
            .is_some_and(|n| n.len() <= 1 && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// `<one label>.mail.protection.outlook.com`, or the DNSSEC form under
/// `mx.microsoft`. Microsoft's documentation (Exchange Team's "Modernizing
/// DNS security for Exchange Online mail flow",
/// https://techcommunity.microsoft.com/blog/exchange/modernizing-dns-security-for-exchange-online-mail-flow/4514248,
/// and the DANE page, https://learn.microsoft.com/exchange/security-and-compliance/how-dane-secures-email)
/// says only "dedicated subdomains within mx.microsoft", "the MX record
/// ending in mx.microsoft", without a hostname example. Field reports
/// show `<domain-with-hyphens>.<tag>.mx.microsoft` (two labels), so one
/// or two labels are accepted there.
fn exchange_online(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if let Some(label) = host.strip_suffix(".mail.protection.outlook.com") {
        return label_ok(label);
    }
    match host.strip_suffix(".mx.microsoft") {
        Some(labels) => labels.split('.').count() <= 2 && labels.split('.').all(label_ok),
        None => false,
    }
}

/// One non-empty label, no dots.
fn label_ok(label: &str) -> bool {
    !label.is_empty() && !label.contains('.')
}
