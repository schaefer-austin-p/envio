//! The Graph client: one request pipeline (token, 401 rule, in-call retry,
//! status table) and the calls built on it.

use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};

use super::bearer::Bearer;
use super::error::{Mapped, Op, to_net};
use super::http::{Http, Request, Response, Sent};
use super::model::GraphError;
use super::parse;
use super::path::enc;
use crate::tls::Roots;
use crate::{NetError, Result};

pub(super) const HOST: &str = "graph.microsoft.com";
/// A `Retry-After` longer than this is not waited for: the caller backs off.
const MAX_WAIT_S: u64 = 10;
/// A change to a message's flags; `None` leaves it alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch {
    pub read: Option<bool>,
    pub flagged: Option<bool>,
    pub categories: Option<Vec<String>>,
}

/// What a call asks of the pipeline.
pub(super) struct Spec {
    pub op: Op,
    pub method: &'static str,
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<(&'static str, Vec<u8>)>,
    /// Always a new connection (sends and creates).
    pub fresh: bool,
    /// May be repeated on a new connection if a reused one was dead.
    pub idempotent: bool,
}

impl Spec {
    pub fn get(op: Op, path: String) -> Self {
        Self {
            op,
            method: "GET",
            path,
            headers: vec![],
            body: None,
            fresh: false,
            idempotent: true,
        }
    }

    pub fn json(op: Op, method: &'static str, path: String, body: &Value) -> Self {
        Self {
            op,
            method,
            path,
            headers: vec![],
            body: Some(("application/json", body.to_string().into_bytes())),
            fresh: false,
            idempotent: false,
        }
    }
}

/// What a request came to, past the status table.
pub(super) enum Outcome {
    Ok(Response),
    Gone,
    Expired,
}

pub struct GraphClient {
    pub(super) http: Http,
    pub(super) bearer: Box<dyn Bearer>,
    /// The next token fetch is forced (a sub-reply said 401).
    pub(super) force_next: bool,
}

impl GraphClient {
    pub fn new(roots: Roots, bearer: Box<dyn Bearer>) -> Self {
        Self::with_host(HOST, 443, roots, bearer)
    }

    #[doc(hidden)]
    pub fn with_host(host: &str, port: u16, roots: Roots, bearer: Box<dyn Bearer>) -> Self {
        Self {
            http: Http::new(host, port, roots),
            bearer,
            force_next: false,
        }
    }

    /// Whether `status` is worth one in-call retry for this request.
    ///
    /// A write that is not idempotent (a move) is repeated only on 429 and
    /// 503, which say it was not acted on: after a 5xx it may have landed,
    /// and the repeat would then see a 404 and call the message gone.
    fn may_retry(spec: &Spec, status: u16) -> bool {
        match spec.op {
            Op::Create => false,
            Op::Send => matches!(status, 429 | 503),
            _ if !spec.idempotent => matches!(status, 429 | 503),
            _ => matches!(status, 429 | 500 | 502 | 503 | 504),
        }
    }

    /// Sends `spec`: a fresh token on a 401 once, one retry of a short
    /// `Retry-After`, then the status table.
    pub(super) async fn exec(&mut self, spec: &Spec) -> Result<Outcome> {
        let mut forced = std::mem::take(&mut self.force_next);
        let mut retried = false;
        loop {
            let token = self.bearer.bearer(forced).await?;
            let mut headers = spec.headers.clone();
            headers.push(("Authorization", bearer_header(&token)));
            let req = Request {
                method: spec.method,
                path: &spec.path,
                headers,
                body: spec.body.clone(),
            };
            let resp = self
                .http
                .send(&req, spec.fresh, spec.idempotent)
                .await
                .map_err(|s| lost(spec.op, s))?;
            if (200..300).contains(&resp.status) {
                return Ok(Outcome::Ok(resp));
            }
            let err = parse::error(resp.status, &resp.body, resp.header("retry-after"));
            match to_net(&err, spec.op) {
                Mapped::Retry401 if forced => {
                    return Err(NetError::SignIn("Graph refused the new token".into()));
                }
                Mapped::Retry401 => forced = true,
                Mapped::Gone => return Ok(Outcome::Gone),
                Mapped::Expired => return Ok(Outcome::Expired),
                Mapped::Err(e) => {
                    if retried || !Self::may_retry(spec, resp.status) || !short_wait(&err) {
                        return Err(e);
                    }
                    retried = true;
                    let wait = err.retry_after_s.unwrap_or(0);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
            }
        }
    }

    /// The reply body, or `None` if the thing is gone.
    pub(super) async fn body(&mut self, spec: &Spec) -> Result<Option<Vec<u8>>> {
        Ok(match self.exec(spec).await? {
            Outcome::Ok(r) => Some(r.body),
            Outcome::Gone => None,
            Outcome::Expired => return Err(NetError::Protocol("unexpected 410".into())),
        })
    }

    /// Flags, read state and categories. A message that is gone is `Ok`.
    pub async fn patch(&mut self, id: &str, patch: &Patch) -> Result<()> {
        let mut body = serde_json::Map::new();
        if let Some(read) = patch.read {
            body.insert("isRead".into(), json!(read));
        }
        if let Some(flagged) = patch.flagged {
            let state = if flagged { "flagged" } else { "notFlagged" };
            body.insert("flag".into(), json!({"flagStatus": state}));
        }
        if let Some(categories) = &patch.categories {
            body.insert("categories".into(), json!(categories));
        }
        let path = format!("/v1.0/me/messages/{}", enc(id));
        let mut spec = Spec::json(Op::Write, "PATCH", path, &Value::Object(body));
        spec.idempotent = true;
        self.exec(&spec).await.map(|_| ())
    }

    /// Moves a message; its new id, or `None` if it is gone. Not repeated
    /// on a dead reused connection: the move may have happened.
    pub async fn move_to(&mut self, id: &str, folder_id: &str) -> Result<Option<String>> {
        let path = format!("/v1.0/me/messages/{}/move", enc(id));
        let spec = Spec::json(
            Op::Write,
            "POST",
            path,
            &json!({"destinationId": folder_id}),
        );
        match self.body(&spec).await? {
            Some(body) => Ok(Some(new_id(&body)?)),
            None => Ok(None),
        }
    }

    /// Deletes for good. A message that is gone is `Ok`.
    pub async fn permanent_delete(&mut self, id: &str) -> Result<()> {
        let path = format!("/v1.0/me/messages/{}/permanentDelete", enc(id));
        let mut spec = Spec::json(Op::Write, "POST", path, &json!({}));
        // A repeat is safe: a message already gone is `Ok`.
        spec.idempotent = true;
        self.exec(&spec).await.map(|_| ())
    }

    /// Creates a message in `folder_id` from MIME; its id.
    pub async fn create_mime(&mut self, folder_id: &str, mime: &[u8]) -> Result<String> {
        let path = format!("/v1.0/me/mailFolders/{}/messages", enc(folder_id));
        let mut spec = mime_post(Op::Create, path, mime);
        spec.idempotent = false;
        let body = self.body(&spec).await?;
        new_id(&body.unwrap_or_default())
    }

    /// Sends the MIME. A lost reply, or a status that does not prove Graph
    /// refused it, is `Unconfirmed`.
    pub async fn send_mime(&mut self, mime: &[u8]) -> Result<()> {
        let spec = mime_post(Op::Send, "/v1.0/me/sendMail".into(), mime);
        self.exec(&spec).await.map(|_| ())
    }
}

fn bearer_header(token: &secrecy::SecretString) -> String {
    use secrecy::ExposeSecret;
    format!("Bearer {}", token.expose_secret())
}

fn mime_post(op: Op, path: String, mime: &[u8]) -> Spec {
    let body = base64::engine::general_purpose::STANDARD.encode(mime);
    Spec {
        op,
        method: "POST",
        path,
        headers: vec![],
        body: Some(("text/plain", body.into_bytes())),
        fresh: true,
        idempotent: false,
    }
}

fn new_id(body: &[u8]) -> Result<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .ok_or_else(|| NetError::Protocol("Graph reply has no message id".into()))
}

fn short_wait(e: &GraphError) -> bool {
    e.retry_after_s.is_some_and(|s| s <= MAX_WAIT_S)
}

/// A request that failed without a status. A lost reply to a send or a
/// create may have been acted on.
fn lost(op: Op, s: Sent) -> NetError {
    match (op, s) {
        (Op::Send | Op::Create, Sent::After(e)) => NetError::Unconfirmed(e.to_string()),
        (_, s) => s.into_error(),
    }
}
