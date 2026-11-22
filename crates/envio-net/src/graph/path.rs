//! Building request paths: percent-encoding and the paths of Graph's links.

use crate::{NetError, Result};

/// Percent-encodes everything but RFC 3986's unreserved characters.
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The path and query of a link Graph gave (`@odata.nextLink`, delta link).
/// The host is dropped: the client only ever talks to its own endpoint, so
/// the token cannot be sent elsewhere by a link.
pub fn link_path(link: &str) -> Result<String> {
    let rest = link
        .strip_prefix("https://")
        .ok_or_else(|| NetError::Protocol("Graph link is not https".into()))?;
    match rest.find('/') {
        Some(i) => Ok(rest[i..].to_owned()),
        None => Err(NetError::Protocol("Graph link has no path".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(enc("AAMk-1_x.~"), "AAMk-1_x.~");
        assert_eq!(enc("a b'<>=/"), "a%20b%27%3C%3E%3D%2F");
    }

    #[test]
    fn a_link_keeps_path_and_query_and_drops_the_host() {
        let got = link_path("https://graph.microsoft.com/v1.0/me/x?$skiptoken=a%3D").unwrap();
        assert_eq!(got, "/v1.0/me/x?$skiptoken=a%3D");
        assert!(link_path("http://graph.microsoft.com/v1.0/x").is_err());
        assert!(link_path("https://graph.microsoft.com").is_err());
        assert!(link_path("/v1.0/x").is_err());
    }
}
