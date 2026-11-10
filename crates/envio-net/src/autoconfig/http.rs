//! Just enough HTTPS to fetch an autoconfig file: GET over HTTP/1.0 (so
//! no chunked replies), no cookies, no User-Agent, a size cap and at most
//! three redirects, all to https. The request and response handling is
//! pure; `get` only moves bytes.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::connect;
use crate::error::{NetError, Result};
use crate::server::{Endpoint, Security};
use crate::tls::{self, Roots};

/// Autoconfig files are a few KiB; anything bigger isn't one.
pub const MAX_BODY: usize = 256 * 1024;
pub const MAX_REDIRECTS: usize = 3;

#[derive(Debug, PartialEq, Eq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Body(String),
    Redirect(String),
    /// Any other status: there is no file here.
    Nothing,
}

/// `https://host[:port]/path`; anything else (http, user info) is None.
pub fn parse_url(url: &str) -> Option<Url> {
    let rest = url.strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (authority, 443),
    };
    let ok_path = path.bytes().all(|b| b.is_ascii_graphic());
    (super::plan::is_hostname(host) && ok_path).then(|| Url {
        host: host.to_ascii_lowercase(),
        port,
        path: path.to_owned(),
    })
}

pub fn request(url: &Url) -> String {
    format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nAccept: application/xml, text/xml\r\nConnection: close\r\n\r\n",
        url.path, url.host
    )
}

/// The whole response, read to EOF (HTTP/1.0).
pub fn parse_reply(raw: &[u8]) -> Reply {
    let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Reply::Nothing;
    };
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let header = |wanted: &str| {
        head.split("\r\n").skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(wanted)
                .then(|| value.trim().to_owned())
        })
    };
    match status {
        200 => match String::from_utf8(raw[split + 4..].to_vec()) {
            Ok(body) => Reply::Body(body),
            Err(_) => Reply::Nothing,
        },
        301 | 302 | 303 | 307 | 308 => header("location").map_or(Reply::Nothing, Reply::Redirect),
        _ => Reply::Nothing,
    }
}

/// The file at `url`, following redirects; Ok(None) when there is none.
pub async fn get(url: &str, roots: &Roots) -> Result<Option<String>> {
    let mut url = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let parsed =
            parse_url(&url).ok_or_else(|| NetError::Refused(format!("not an https URL: {url}")))?;
        let raw = fetch_once(&parsed, roots).await?;
        if raw.len() > MAX_BODY {
            return Ok(None);
        }
        match parse_reply(&raw) {
            Reply::Body(body) => return Ok(Some(body)),
            Reply::Nothing => return Ok(None),
            Reply::Redirect(next) => url = next,
        }
    }
    Ok(None)
}

async fn fetch_once(url: &Url, roots: &Roots) -> Result<Vec<u8>> {
    // No pin: autoconfig hosts must have a certificate the system trusts.
    let endpoint = Endpoint::new(&url.host, url.port, Security::Tls);
    let mut stream = tls::connect(&endpoint, roots).await?;
    connect::within(async {
        stream.write_all(request(url).as_bytes()).await?;
        let mut raw = Vec::new();
        (&mut stream)
            .take(MAX_BODY as u64 + 8 * 1024)
            .read_to_end(&mut raw)
            .await?;
        Ok(raw)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_urls_parse() {
        assert_eq!(
            parse_url("https://Autoconfig.Example.org/mail/config-v1.1.xml?emailaddress=x"),
            Some(Url {
                host: "autoconfig.example.org".into(),
                port: 443,
                path: "/mail/config-v1.1.xml?emailaddress=x".into(),
            })
        );
        assert_eq!(parse_url("https://example.org:8443").unwrap().port, 8443);
        assert_eq!(parse_url("https://example.org").unwrap().path, "/");
        for bad in [
            "http://example.org/",
            "https://user@example.org/",
            "https://ex ample.org/",
            "https://example.org/a b",
        ] {
            assert_eq!(parse_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_request_is_a_bare_http_1_0_get() {
        let url = parse_url("https://example.org/a").unwrap();
        let text = request(&url);
        assert!(text.starts_with("GET /a HTTP/1.0\r\nHost: example.org\r\n"));
        assert!(!text.contains("User-Agent"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn replies_become_a_body_a_redirect_or_nothing() {
        assert_eq!(
            parse_reply(b"HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\n\r\n<clientConfig/>"),
            Reply::Body("<clientConfig/>".into())
        );
        assert_eq!(
            parse_reply(b"HTTP/1.1 301 Moved\r\nlocation: https://b.example/x\r\n\r\n"),
            Reply::Redirect("https://b.example/x".into())
        );
        assert_eq!(
            parse_reply(b"HTTP/1.1 404 Not Found\r\n\r\nnope"),
            Reply::Nothing
        );
        assert_eq!(parse_reply(b"HTTP/1.1 302 Found\r\n\r\n"), Reply::Nothing);
        assert_eq!(parse_reply(b"garbage"), Reply::Nothing);
        assert_eq!(
            parse_reply(b"HTTP/1.1 200 OK\r\n\r\n\xff\xfe"),
            Reply::Nothing
        );
    }
}
