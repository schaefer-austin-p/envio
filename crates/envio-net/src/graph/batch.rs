//! Summary reads through `POST /$batch`: up to 20 per request.
//!
//! Decision (fixtures README): `$batch` answered 200 with a reply per request
//! on a consumer account, so there is no per-id GET fallback; the spec only
//! asks for one if the fixture check had failed.

use serde_json::{Value, json};

use super::client::{GraphClient, Outcome, Spec};
use super::error::{Mapped, Op, to_net};
use super::model::{GSummary, SubReply};
use super::parse;
use super::path::enc;
use crate::{NetError, Result};

/// Graph's limit on sub-requests in one `$batch`.
const CHUNK: usize = 20;

fn sub_request(n: usize, id: &str) -> Value {
    json!({
        "id": n.to_string(),
        "method": "GET",
        "url": format!(
            "/me/messages/{}?$select=internetMessageHeaders,receivedDateTime&$expand=singleValueExtendedProperties($filter=id eq 'Integer 0x0E08')",
            enc(id)
        ),
        "headers": {"Prefer": "IdType=\"ImmutableId\""},
    })
}

/// What a chunk came to.
enum Chunk {
    Done(Vec<GSummary>),
    /// A sub-reply said 401: the token is stale.
    Unauthorized,
}

/// Turns sub-replies into summaries, in the order of `ids`. A sub-404 drops
/// that id; replies are matched by their `id`, never by position.
fn collect(ids: &[&str], replies: &[SubReply]) -> Result<Chunk> {
    let mut out = Vec::new();
    for (n, _) in ids.iter().enumerate() {
        let Some(r) = replies.iter().find(|r| r.id == (n + 1).to_string()) else {
            return Err(NetError::Protocol("Graph batch lacks a reply".into()));
        };
        if r.status == 200 {
            out.push(parse::summary(&r.body)?);
            continue;
        }
        let e = super::model::GraphError {
            status: r.status,
            code: String::new(),
            retry_after_s: r.retry_after_s,
        };
        match to_net(&e, Op::Read) {
            Mapped::Gone => {}
            Mapped::Retry401 => return Ok(Chunk::Unauthorized),
            Mapped::Err(e) => return Err(e),
            Mapped::Expired => unreachable!("a read has no expiry"),
        }
    }
    Ok(Chunk::Done(out))
}

impl GraphClient {
    /// Summaries (size, headers, received time) of `ids`. A message that is
    /// gone is left out. Any other sub-failure fails the call: 429 and 503
    /// as `Busy`, a second 401 as `SignIn`.
    pub async fn summaries(&mut self, ids: &[String]) -> Result<Vec<GSummary>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            let chunk: Vec<&str> = chunk.iter().map(String::as_str).collect();
            let mut forced = false;
            loop {
                match self.batch_chunk(&chunk).await? {
                    Chunk::Done(list) => {
                        out.extend(list);
                        break;
                    }
                    Chunk::Unauthorized if forced => {
                        return Err(NetError::SignIn("Graph refused the new token".into()));
                    }
                    Chunk::Unauthorized => {
                        forced = true;
                        self.force_next = true;
                    }
                }
            }
        }
        Ok(out)
    }

    async fn batch_chunk(&mut self, ids: &[&str]) -> Result<Chunk> {
        let requests: Vec<Value> = ids
            .iter()
            .enumerate()
            .map(|(n, id)| sub_request(n + 1, id))
            .collect();
        let mut spec = Spec::json(
            Op::Read,
            "POST",
            "/v1.0/$batch".into(),
            &json!({"requests": requests}),
        );
        // A POST, but only reads: safe to repeat on a dead reused connection.
        spec.idempotent = true;
        let Outcome::Ok(resp) = self.exec(&spec).await? else {
            return Err(NetError::Refused("HTTP 404, batch".into()));
        };
        collect(ids, &parse::batch(&resp.body)?)
    }
}
