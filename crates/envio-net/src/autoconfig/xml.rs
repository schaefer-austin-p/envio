//! Thunderbird's autoconfig format (config-v1.1.xml): the IMAP and SMTP
//! servers in it, with the %EMAIL...% placeholders filled in. Servers
//! without TLS ("plain") and other protocols (POP3) are skipped.

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;

use super::plan::{Candidates, Setting, is_hostname};
use crate::server::{Endpoint, Security};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Imap,
    Smtp,
}

#[derive(Default)]
struct Server {
    hostname: String,
    port: String,
    socket_type: String,
    username: String,
}

/// The servers in `xml` for `email`; None if it isn't an autoconfig file.
pub fn parse(xml: &str, email: &str) -> Option<Candidates> {
    // No trim_text: it would also trim the spaces either side of an entity
    // (`a &amp; b` became `a&b`). setting() trims each field once instead.
    let mut reader = Reader::from_str(xml);
    let mut out = Candidates::default();
    let mut saw_root = false;
    let mut server: Option<(Kind, Server)> = None;
    let mut field: Option<String> = None;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_owned();
                saw_root |= name == "clientConfig";
                let kind = e.try_get_attribute("type").ok().flatten().and_then(|a| {
                    a.normalized_value(XmlVersion::Implicit1_0)
                        .ok()
                        .map(|v| v.into_owned())
                });
                match (name.as_str(), kind.as_deref()) {
                    ("incomingServer", Some("imap")) => {
                        server = Some((Kind::Imap, Server::default()))
                    }
                    ("outgoingServer", Some("smtp")) => {
                        server = Some((Kind::Smtp, Server::default()))
                    }
                    _ => field = Some(name),
                }
            }
            Event::Text(t) => push_text(&mut server, field.as_deref(), &t.xml10_content()),
            Event::GeneralRef(r) => {
                let text = match r.resolve_char_ref().ok().flatten() {
                    Some(c) => c.to_string(),
                    None => resolve_predefined_entity(&r).unwrap_or("").to_owned(),
                };
                push_text(&mut server, field.as_deref(), &text);
            }
            Event::End(e) => {
                let name = e.local_name();
                if matches!(name.as_ref(), "incomingServer" | "outgoingServer")
                    && let Some((kind, s)) = server.take()
                    && let Some(setting) = setting(&s, email)
                {
                    match kind {
                        Kind::Imap => out.imap.push(setting),
                        Kind::Smtp => out.smtp.push(setting),
                    }
                }
                field = None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    saw_root.then_some(out)
}

fn push_text(server: &mut Option<(Kind, Server)>, field: Option<&str>, text: &str) {
    let Some((_, s)) = server.as_mut() else {
        return;
    };
    let target = match field {
        Some("hostname") => &mut s.hostname,
        Some("port") => &mut s.port,
        Some("socketType") => &mut s.socket_type,
        Some("username") => &mut s.username,
        _ => return,
    };
    target.push_str(text);
}

fn setting(s: &Server, email: &str) -> Option<Setting> {
    let security = match s.socket_type.trim() {
        "SSL" => Security::Tls,
        "STARTTLS" => Security::StartTls,
        _ => return None,
    };
    let host = fill(s.hostname.trim(), email).to_ascii_lowercase();
    let port: u16 = s.port.trim().parse().ok().filter(|&p| p != 0)?;
    if !is_hostname(&host) {
        return None;
    }
    Some(Setting {
        endpoint: Endpoint::new(&host, port, security),
        username: fill(s.username.trim(), email),
    })
}

/// The placeholders Thunderbird defines.
fn fill(template: &str, email: &str) -> String {
    let email = email.trim();
    let (local, domain) = email.rsplit_once('@').unwrap_or((email, ""));
    template
        .replace("%EMAILADDRESS%", email)
        .replace("%EMAILLOCALPART%", local)
        .replace("%EMAILDOMAIN%", domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="example.org">
    <domain>example.org</domain>
    <incomingServer type="pop3">
      <hostname>pop.example.org</hostname><port>995</port>
      <socketType>SSL</socketType><username>%EMAILADDRESS%</username>
    </incomingServer>
    <incomingServer type="imap">
      <hostname>imap.%EMAILDOMAIN%</hostname><port>143</port>
      <socketType>plain</socketType><username>%EMAILLOCALPART%</username>
    </incomingServer>
    <incomingServer type="imap">
      <hostname>IMAP.example.org</hostname><port>993</port>
      <socketType>SSL</socketType><username>%EMAILLOCALPART%</username>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.example.org</hostname><port>587</port>
      <socketType>STARTTLS</socketType><username>%EMAILADDRESS%</username>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

    #[test]
    fn imap_and_smtp_servers_come_out_with_placeholders_filled() {
        let found = parse(CONFIG, "jo@example.org").unwrap();
        assert_eq!(found.imap.len(), 1, "pop3 and plain are skipped");
        assert_eq!(
            found.imap[0].endpoint,
            Endpoint::new("imap.example.org", 993, Security::Tls)
        );
        assert_eq!(found.imap[0].username, "jo");
        assert_eq!(
            found.smtp[0].endpoint,
            Endpoint::new("smtp.example.org", 587, Security::StartTls)
        );
        assert_eq!(found.smtp[0].username, "jo@example.org");
    }

    #[test]
    fn entities_are_resolved_in_values() {
        let xml = r#"<clientConfig><emailProvider><outgoingServer type="smtp">
          <hostname>smtp.example.org</hostname><port>465</port><socketType>SSL</socketType>
          <username>a&amp;b&#64;example.org</username></outgoingServer></emailProvider></clientConfig>"#;
        assert_eq!(
            parse(xml, "x@example.org").unwrap().smtp[0].username,
            "a&b@example.org"
        );
    }

    #[test]
    fn spaces_around_an_entity_are_kept() {
        let xml = r#"<clientConfig><emailProvider><outgoingServer type="smtp">
          <hostname>smtp.example.org</hostname><port>465</port><socketType>SSL</socketType>
          <username> a &amp; b </username></outgoingServer></emailProvider></clientConfig>"#;
        assert_eq!(
            parse(xml, "x@example.org").unwrap().smtp[0].username,
            "a & b"
        );
    }

    #[test]
    fn bad_hosts_ports_and_documents_are_refused() {
        let server = |host: &str, port: &str| {
            format!(
                "<clientConfig><incomingServer type=\"imap\"><hostname>{host}</hostname>\
                 <port>{port}</port><socketType>SSL</socketType></incomingServer></clientConfig>"
            )
        };
        assert!(
            parse(&server("evil host", "993"), "a@b.c")
                .unwrap()
                .imap
                .is_empty()
        );
        assert!(
            parse(&server("imap.b.c", "0"), "a@b.c")
                .unwrap()
                .imap
                .is_empty()
        );
        assert!(
            parse(&server("imap.b.c", "99999"), "a@b.c")
                .unwrap()
                .imap
                .is_empty()
        );
        assert_eq!(parse("<html><body>Not found</body></html>", "a@b.c"), None);
        assert_eq!(
            parse("<clientConfig><unclosed>", "a@b.c").map(|c| c.imap.len()),
            Some(0)
        );
        assert_eq!(parse("not xml at all <", "a@b.c"), None);
    }
}
