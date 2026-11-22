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
}

/// `Microsoft365` when the most-preferred MX records are all Exchange
/// Online hosts, else `Unknown`. Strict on purpose: a false positive
/// sends the user to a sign-in that cannot work for them.
pub fn hosting_of(records: &[MxRecord]) -> Hosting {
    let Some(best) = records.iter().map(|r| r.preference).min() else {
        return Hosting::Unknown;
    };
    let all = records
        .iter()
        .filter(|r| r.preference == best)
        .all(|r| exchange_online(&r.host));
    if all {
        Hosting::Microsoft365
    } else {
        Hosting::Unknown
    }
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
