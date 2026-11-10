//! The network side of an autoconfig fetch: TLS to the host, one GET per
//! redirect hop, and every hop under one deadline. The rules are in `http`.

use std::future::Future;
use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::http::{self, MAX_BODY, MAX_HEAD, MAX_REDIRECTS, Reply, Url};
use crate::connect;
use crate::error::{NetError, Result};
use crate::server::{Endpoint, Security};
use crate::tls::{self, Roots};

/// All of one fetch, redirects included.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// The body at `url`, following redirects, all within `limit`. Ok(None)
/// when there is no file there.
pub async fn get(url: &str, roots: &Roots, limit: Duration) -> Result<Option<String>> {
    follow(url, limit, |hop| fetch_once(hop, roots)).await
}

/// Follows redirects by calling `hop` once per URL, within `limit` in all.
/// A redirect target must pass `http::allowed_redirect_host`, and there are
/// at most `MAX_REDIRECTS` of them; a loop or a longer chain is an error.
pub async fn follow<F, Fut>(url: &str, limit: Duration, mut hop: F) -> Result<Option<String>>
where
    F: FnMut(Url) -> Fut,
    Fut: Future<Output = Result<Reply>>,
{
    connect::within_for(limit, async {
        let mut url = url.to_owned();
        for index in 0..=MAX_REDIRECTS {
            let parsed = http::parse_url(&url)
                .ok_or_else(|| NetError::Refused(format!("not an https URL: {url}")))?;
            if index > 0 && !http::allowed_redirect_host(&parsed.host) {
                return Err(NetError::Refused(format!("redirect to {}", parsed.host)));
            }
            match hop(parsed).await? {
                Reply::Body(body) => return Ok(Some(body)),
                Reply::Nothing => return Ok(None),
                Reply::Chunked => {
                    return Err(NetError::Protocol("chunked autoconfig reply".into()));
                }
                Reply::Redirect(next) => url = next,
            }
        }
        Err(NetError::Refused("too many redirects".into()))
    })
    .await
}

async fn fetch_once(url: Url, roots: &Roots) -> Result<Reply> {
    // No pin: autoconfig hosts must have a certificate the system trusts.
    let endpoint = Endpoint::new(&url.host, url.port, Security::Tls);
    let mut stream = tls::connect(&endpoint, roots).await?;
    let raw = connect::within(async {
        stream.write_all(http::request(&url).as_bytes()).await?;
        Ok(read_reply(&mut stream).await?)
    })
    .await?;
    Ok(http::parse_reply(&raw))
}

/// Reads until the reply is complete, the peer closes, or the reply is
/// over the cap. A peer that drops TCP without close_notify also ends it.
async fn read_reply<R: AsyncRead + Unpin>(stream: &mut R) -> io::Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    while !http::reply_complete(&raw) && raw.len() <= MAX_HEAD + MAX_BODY {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// Runs `follow` with one scripted reply per hop; returns the result
    /// and how many hops were made.
    async fn scripted(replies: Vec<Reply>) -> (Result<Option<String>>, usize) {
        let calls = Cell::new(0usize);
        let mut replies = replies.into_iter();
        let result = follow("https://a.example.org/", Duration::from_secs(5), |_| {
            calls.set(calls.get() + 1);
            let reply = replies.next().unwrap_or(Reply::Nothing);
            async move { Ok(reply) }
        })
        .await;
        (result, calls.get())
    }

    fn redirect(to: &str) -> Reply {
        Reply::Redirect(to.into())
    }

    #[tokio::test]
    async fn a_chain_within_the_cap_is_followed() {
        let mut replies = vec![redirect("https://a.example.org/1"); MAX_REDIRECTS];
        replies.push(Reply::Body("found".into()));
        let (result, hops) = scripted(replies).await;
        assert_eq!(result.unwrap(), Some("found".into()));
        assert_eq!(hops, MAX_REDIRECTS + 1);
    }

    #[tokio::test]
    async fn a_chain_over_the_cap_is_an_error() {
        let mut replies = vec![redirect("https://a.example.org/1"); MAX_REDIRECTS + 1];
        replies.push(Reply::Body("too far".into()));
        let (result, hops) = scripted(replies).await;
        assert!(matches!(result, Err(NetError::Refused(_))));
        assert_eq!(hops, MAX_REDIRECTS + 1);
    }

    #[tokio::test]
    async fn a_redirect_loop_is_an_error_after_the_cap() {
        let (result, hops) = scripted(vec![redirect("https://a.example.org/"); 10]).await;
        assert!(matches!(result, Err(NetError::Refused(_))));
        assert_eq!(hops, MAX_REDIRECTS + 1);
    }

    #[tokio::test]
    async fn a_redirect_to_localhost_or_plain_http_is_refused_before_a_hop() {
        for target in [
            "https://localhost:8443/x",
            "http://example.org/x",
            "https://10.0.0.1/x",
        ] {
            let (result, hops) = scripted(vec![redirect(target), Reply::Body("no".into())]).await;
            assert!(matches!(result, Err(NetError::Refused(_))), "{target}");
            assert_eq!(hops, 1, "{target}");
        }
    }

    #[tokio::test]
    async fn the_limit_covers_every_hop_not_each_one() {
        // Each hop takes 200 ms; four hops would take 800 ms, the limit is 300.
        let result = follow(
            "https://a.example.org/",
            Duration::from_millis(300),
            |_| async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(Reply::Redirect("https://a.example.org/next".into()))
            },
        )
        .await;
        assert!(matches!(result, Err(NetError::Timeout)));
    }
}
