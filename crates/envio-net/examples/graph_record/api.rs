//! The recorder's Graph calls: one request type, delta walks and the undo
//! list. Errors name the step and the status, never a path, body or token.

use envio_net::graph::http::{Http, Request};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

/// Pages kept per recorded reply, to keep the fixtures small.
pub const KEEP: usize = 5;
pub const DRAFT_SUBJECT: &str = "envio fixture recorder (temporary)";

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

/// The path and query of a link Graph gave; only graph.microsoft.com links.
pub fn path_of(link: &str) -> Result<String, String> {
    link.strip_prefix("https://graph.microsoft.com")
        .map(str::to_owned)
        .ok_or_else(|| "a link pointed away from graph.microsoft.com".to_owned())
}

/// One request to make.
pub struct Q<'a> {
    step: &'a str,
    method: &'static str,
    path: &'a str,
    prefer: Option<&'a str>,
    body: Option<Value>,
}

impl<'a> Q<'a> {
    pub fn new(step: &'a str, method: &'static str, path: &'a str) -> Self {
        Self {
            step,
            method,
            path,
            prefer: None,
            body: None,
        }
    }

    pub fn prefer(mut self, p: &'a str) -> Self {
        self.prefer = Some(p);
        self
    }

    pub fn body(mut self, b: Value) -> Self {
        self.body = Some(b);
        self
    }
}

pub struct Api {
    pub http: Http,
    pub token: SecretString,
}

impl Api {
    pub async fn call(&mut self, q: Q<'_>) -> Result<(u16, Value), String> {
        let mut headers = vec![(
            "Authorization",
            format!("Bearer {}", self.token.expose_secret()),
        )];
        headers.extend(q.prefer.map(|p| ("Prefer", p.to_owned())));
        let body = q
            .body
            .map(|b| ("application/json", b.to_string().into_bytes()));
        let req = Request {
            method: q.method,
            path: q.path,
            headers,
            body,
        };
        let res = self
            .http
            .send(&req, false, q.method == "GET")
            .await
            .map_err(|e| format!("{}: request failed: {}", q.step, e.into_error()))?;
        let json = serde_json::from_slice(&res.body).unwrap_or(Value::Null);
        Ok((res.status, json))
    }

    /// A call that must succeed (2xx).
    pub async fn ok(&mut self, q: Q<'_>) -> Result<Value, String> {
        let step = q.step.to_owned();
        let (status, v) = self.call(q).await?;
        if (200..300).contains(&status) {
            return Ok(v);
        }
        let code = v["error"]["code"].as_str().unwrap_or("-");
        Err(format!("{step}: HTTP {status}, Graph code {code}"))
    }

    pub async fn get(&mut self, step: &str, path: &str) -> Result<Value, String> {
        self.ok(Q::new(step, "GET", path)).await
    }
}

/// The pages of a delta walk worth keeping, its delta link and the ids seen.
pub struct Walk {
    pub first: Value,
    pub second: Option<Value>,
    pub last: Value,
    pub link: String,
    /// Non-draft message ids with their read flag.
    pub ids: Vec<(String, bool)>,
}

fn collect_ids(page: &Value, ids: &mut Vec<(String, bool)>) {
    for item in page["value"].as_array().into_iter().flatten() {
        if let Some(id) = item["id"].as_str().filter(|_| item["isDraft"] != true) {
            ids.push((id.to_owned(), item["isRead"].as_bool().unwrap_or(true)));
        }
    }
}

enum Step {
    Next(String),
    Done(String),
}

fn step_of(page: &Value, step: &str) -> Result<Step, String> {
    if let Some(next) = page["@odata.nextLink"].as_str() {
        return path_of(next).map(Step::Next);
    }
    match page["@odata.deltaLink"].as_str() {
        Some(link) => Ok(Step::Done(link.to_owned())),
        None => Err(format!("{step}: a page had no next or delta link")),
    }
}

/// Follows a delta from `path` (the first request) to its delta link. The
/// first two pages are small so there are next links to record.
pub async fn walk(api: &mut Api, step: &str, path: &str) -> Result<Walk, String> {
    let (mut path, mut pages, mut ids) = (path.to_owned(), Vec::<Value>::new(), Vec::new());
    for n in 0..100 {
        let prefer = format!("odata.maxpagesize={}", if n < 2 { 3 } else { 200 });
        let page = api.ok(Q::new(step, "GET", &path).prefer(&prefer)).await?;
        collect_ids(&page, &mut ids);
        let next = step_of(&page, step)?;
        pages.push(page);
        match next {
            Step::Next(p) => path = p,
            Step::Done(link) => {
                let second = pages.get(1).filter(|_| pages.len() > 2).cloned();
                let last = pages.last().cloned().unwrap_or(Value::Null);
                let first = pages.swap_remove(0);
                return Ok(Walk {
                    first,
                    second,
                    last,
                    link,
                    ids,
                });
            }
        }
    }
    Err(format!("{step}: over 100 pages; use a smaller mailbox"))
}

/// At most `KEEP` items of a collection, `must` (an id) first.
pub fn trunc(mut v: Value, must: Option<&str>) -> Value {
    if let Some(items) = v["value"].as_array_mut() {
        if let Some(id) = must {
            items.sort_by_key(|i| i["id"].as_str() != Some(id));
        }
        items.truncate(KEEP);
    }
    v
}

/// What is left to undo if the run stops early.
#[derive(Default)]
pub struct Undo {
    pub unread_flip: Option<(String, bool)>,
    pub draft: Option<String>,
}

impl Undo {
    pub async fn run(&mut self, api: &mut Api) {
        if let Some((id, was_read)) = self.unread_flip.take() {
            let path = format!("/v1.0/me/messages/{}", enc(&id));
            let q = Q::new("restore read state", "PATCH", &path).body(json!({"isRead": was_read}));
            match api.ok(q).await {
                Ok(_) => eprintln!("restored: the message's read state"),
                Err(e) => eprintln!("WARNING: {e}; one Inbox message's read state may be flipped"),
            }
        }
        if let Some(id) = self.draft.take() {
            let path = format!("/v1.0/me/messages/{}/permanentDelete", enc(&id));
            match api.ok(Q::new("delete draft", "POST", &path)).await {
                Ok(_) => eprintln!("restored: the temporary draft is deleted"),
                Err(e) => eprintln!(
                    "WARNING: {e}; delete the draft \"{DRAFT_SUBJECT}\" in Drafts by hand"
                ),
            }
        }
    }
}
