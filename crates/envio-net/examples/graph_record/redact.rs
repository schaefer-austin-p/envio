//! Redaction of recorded Graph replies: addresses, names, subjects, ids,
//! links and headers are replaced by stable fakes (the same input always
//! gets the same fake, across files), bodies are dropped, and free text is
//! scrubbed of addresses, GUIDs and IP literals. Pure functions, no I/O.

use std::collections::HashMap;
use std::ops::Range;

use serde_json::Value;

/// `internetMessageHeaders` names that are kept; every other header
/// (Received, X-MS-Exchange-*, Authentication-Results, ...) is dropped.
const KEEP_HEADERS: [&str; 10] = [
    "From",
    "To",
    "Cc",
    "Subject",
    "Date",
    "Message-ID",
    "In-Reply-To",
    "References",
    "MIME-Version",
    "Content-Type",
];

/// Folder names that stay as they are; any other name becomes "Folder N".
const WELL_KNOWN_FOLDERS: [&str; 10] = [
    "Inbox",
    "Drafts",
    "Sent Items",
    "Deleted Items",
    "Junk Email",
    "Archive",
    "Outbox",
    "Conversation History",
    "Notes",
    "Sync Issues",
];

/// Fields dropped outright: message content and links that embed ids.
const DROPPED_FIELDS: [&str; 4] = ["body", "bodyPreview", "uniqueBody", "webLink"];

/// Fields whose string value is an id of some kind.
const ID_FIELDS: [&str; 6] = [
    "id",
    "changeKey",
    "conversationId",
    "parentFolderId",
    "conversationIndex",
    "@odata.etag",
];

const KNOWN_SEGMENTS: [&str; 14] = [
    "me",
    "mailFolders",
    "messages",
    "delta",
    "childFolders",
    "$value",
    "$batch",
    "inbox",
    "drafts",
    "sentitems",
    "deleteditems",
    "archive",
    "junkemail",
    "v1.0",
];

#[derive(Default)]
pub struct Redactor {
    maps: HashMap<&'static str, HashMap<String, usize>>,
}

impl Redactor {
    /// The stable number of `key` in the `kind` namespace, from 1.
    fn number(&mut self, kind: &'static str, key: &str) -> usize {
        let map = self.maps.entry(kind).or_default();
        let next = map.len() + 1;
        *map.entry(key.to_owned()).or_insert(next)
    }

    /// `alice@example.org`, `bob@example.org`, then `user3@example.org`...
    pub fn address(&mut self, addr: &str) -> String {
        match self.number("address", &addr.to_ascii_lowercase()) {
            1 => "alice@example.org".into(),
            2 => "bob@example.org".into(),
            n => format!("user{n}@example.org"),
        }
    }

    pub fn name_for(&mut self, addr: &str) -> String {
        match self.number("address", &addr.to_ascii_lowercase()) {
            1 => "Alice".into(),
            2 => "Bob".into(),
            n => format!("User {n}"),
        }
    }

    pub fn id(&mut self, id: &str) -> String {
        format!("AAMk-{}", self.number("id", id))
    }

    pub fn message_id(&mut self, id: &str) -> String {
        format!("<fake-{}@example.org>", self.number("mid", id.trim()))
    }

    fn subject(&mut self, s: &str) -> String {
        format!("Subject {}", self.number("subject", s))
    }

    fn folder_name(&mut self, name: &str) -> String {
        if WELL_KNOWN_FOLDERS.contains(&name) {
            name.to_owned()
        } else {
            format!("Folder {}", self.number("folder", name))
        }
    }

    fn token(&mut self, t: &str) -> String {
        format!("fake-{}", self.number("token", t))
    }

    /// Free text with every address, GUID and IP literal replaced.
    pub fn text(&mut self, s: &str) -> String {
        enum Hit {
            Email,
            Guid,
            Ip,
        }
        let mut hits: Vec<(Range<usize>, Hit)> = emails(s)
            .into_iter()
            .map(|r| (r, Hit::Email))
            .chain(guids(s).into_iter().map(|r| (r, Hit::Guid)))
            .chain(ip_literals(s).into_iter().map(|r| (r, Hit::Ip)))
            .collect();
        hits.sort_by_key(|(r, _)| r.start);
        let mut out = String::new();
        let mut at = 0;
        for (range, hit) in hits {
            if range.start < at {
                continue;
            }
            out.push_str(&s[at..range.start]);
            let found = &s[range.clone()];
            match hit {
                Hit::Email => out.push_str(&self.address(found)),
                Hit::Guid => {
                    let n = self.number("guid", &found.to_ascii_lowercase());
                    out.push_str(&format!("00000000-0000-4000-8000-{n:012}"));
                }
                Hit::Ip => out.push_str("[ip]"),
            }
            at = range.end;
        }
        out.push_str(&s[at..]);
        out
    }

    /// Keeps the allow-listed headers and redacts their values.
    pub fn headers(&mut self, list: &[Value]) -> Vec<Value> {
        let mut out = Vec::new();
        for item in list {
            let (Some(name), Some(value)) = (
                item.get("name").and_then(Value::as_str),
                item.get("value").and_then(Value::as_str),
            ) else {
                continue;
            };
            let Some(kept) = KEEP_HEADERS.iter().find(|k| k.eq_ignore_ascii_case(name)) else {
                continue;
            };
            let value = match kept.to_ascii_lowercase().as_str() {
                "from" | "to" | "cc" => self.address_header(value),
                "subject" => self.subject(value),
                "message-id" | "in-reply-to" | "references" => self.id_header(value),
                "content-type" => self.content_type(value),
                _ => self.text(value),
            };
            out.push(serde_json::json!({ "name": name, "value": value }));
        }
        out
    }

    /// `Name <addr>, ...` rebuilt as `Alice <alice@example.org>, ...`.
    fn address_header(&mut self, value: &str) -> String {
        let mut parts = Vec::new();
        let (mut depth_quote, mut depth_angle, mut start) = (false, false, 0);
        for (i, c) in value.char_indices() {
            match c {
                '"' => depth_quote = !depth_quote,
                '<' if !depth_quote => depth_angle = true,
                '>' if !depth_quote => depth_angle = false,
                ',' if !depth_quote && !depth_angle => {
                    parts.push(&value[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&value[start..]);
        let rebuilt: Vec<String> = parts
            .iter()
            .filter_map(|part| {
                let range = emails(part).into_iter().next()?;
                let addr = &part[range];
                Some(format!("{} <{}>", self.name_for(addr), self.address(addr)))
            })
            .collect();
        rebuilt.join(", ")
    }

    /// Each `<id>` in the value mapped; a value with none is mapped whole.
    fn id_header(&mut self, value: &str) -> String {
        let mut ids = Vec::new();
        let mut rest = value;
        while let Some(open) = rest.find('<') {
            let Some(len) = rest[open..].find('>') else {
                break;
            };
            ids.push(self.message_id(&rest[open..=open + len]));
            rest = &rest[open + len + 1..];
        }
        if ids.is_empty() && !value.trim().is_empty() {
            ids.push(self.message_id(value));
        }
        ids.join(" ")
    }

    /// The media type and parameters, but the boundary (Exchange builds it
    /// from host names) replaced.
    fn content_type(&mut self, value: &str) -> String {
        let parts: Vec<String> = value
            .split(';')
            .map(|part| {
                if part.trim().to_ascii_lowercase().starts_with("boundary=") {
                    " boundary=\"fixture-boundary\"".to_owned()
                } else {
                    self.text(part)
                }
            })
            .collect();
        parts.join(";")
    }

    /// An `@odata` link with ids and tokens faked and only known query
    /// keys kept. `@odata.context` links are scrubbed instead.
    pub fn link(&mut self, link: &str, context: bool) -> String {
        const BASE: &str = "https://graph.microsoft.com/v1.0/";
        if context {
            let inner = self.parens(link);
            return self.text(&inner);
        }
        let rest = link.strip_prefix(BASE).unwrap_or("");
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|seg| {
                if KNOWN_SEGMENTS.contains(&seg) {
                    seg.to_owned()
                } else if seg.starts_with("users(") {
                    "users('fake-user')".to_owned()
                } else if seg.contains('(') {
                    self.parens(seg)
                } else {
                    self.id(seg)
                }
            })
            .collect();
        let params: Vec<String> = query
            .split('&')
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                match k {
                    "$skiptoken" | "$deltatoken" => Some(format!("{k}={}", self.token(v))),
                    "$skip" | "$top" | "$select" => Some(format!("{k}={v}")),
                    "$search" | "$filter" => Some(format!("{k}=%22redacted%22")),
                    _ => None,
                }
            })
            .collect();
        let mut out = format!("{BASE}{}", segments.join("/"));
        if !params.is_empty() {
            out.push('?');
            out.push_str(&params.join("&"));
        }
        out
    }

    /// Every `('...')` in `s` with its content mapped like an id.
    fn parens(&mut self, s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(open) = rest.find("('") {
            let Some(close) = rest[open + 2..].find("')") else {
                break;
            };
            let inner = &rest[open + 2..open + 2 + close];
            out.push_str(&rest[..open + 2]);
            out.push_str(&self.id(inner));
            out.push_str("')");
            rest = &rest[open + 2 + close + 2..];
        }
        out.push_str(rest);
        out
    }

    /// Redacts a whole JSON tree in place.
    pub fn value(&mut self, v: &mut Value) {
        match v {
            Value::Object(map) => {
                for k in DROPPED_FIELDS {
                    map.remove(k);
                }
                for (k, child) in map.iter_mut() {
                    self.field(k, child);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|i| self.value(i)),
            Value::String(s) => *s = self.text(s),
            _ => {}
        }
    }

    fn field(&mut self, key: &str, child: &mut Value) {
        let text = child.as_str().map(str::to_owned);
        match (key, text) {
            ("internetMessageHeaders", _) => {
                let list = child.as_array().cloned().unwrap_or_default();
                *child = Value::Array(self.headers(&list));
            }
            ("subject", Some(s)) => *child = Value::String(self.subject(&s)),
            ("internetMessageId", Some(s)) => *child = Value::String(self.message_id(&s)),
            ("displayName", Some(s)) => *child = Value::String(self.folder_name(&s)),
            ("categories", _) => {
                if let Value::Array(items) = child {
                    for item in items.iter_mut() {
                        let n = self.number("category", item.as_str().unwrap_or_default());
                        *item = Value::String(format!("Category {n}"));
                    }
                }
            }
            ("emailAddress", _) => self.email_address(child),
            ("@odata.context", Some(s)) => *child = Value::String(self.link(&s, true)),
            (k, Some(s)) if k.starts_with("@odata.") && s.starts_with("https://") => {
                *child = Value::String(self.link(&s, false));
            }
            // A property id such as "Integer 0x0E08" stays; ids go through the map.
            (k, Some(s)) if ID_FIELDS.contains(&k) => {
                if !s.contains("0x") {
                    *child = Value::String(self.id(&s));
                }
            }
            // A batch response's `id` is the request id; only its body is redacted.
            ("responses", _) => {
                if let Value::Array(items) = child {
                    for item in items.iter_mut() {
                        let Some(map) = item.as_object_mut() else {
                            continue;
                        };
                        // `id` is the request id and `body` the reply; the
                        // object walker would map the one and drop the other.
                        let request_id = map.remove("id");
                        let mut reply = map.remove("body");
                        if let Some(r) = reply.as_mut() {
                            self.value(r);
                        }
                        self.value(item);
                        if let Some(map) = item.as_object_mut() {
                            map.extend(request_id.map(|v| ("id".to_owned(), v)));
                            map.extend(reply.map(|v| ("body".to_owned(), v)));
                        }
                    }
                }
            }
            _ => self.value(child),
        }
    }

    fn email_address(&mut self, v: &mut Value) {
        let Some(map) = v.as_object_mut() else {
            return self.value(v);
        };
        let addr = map
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match addr {
            Some(a) => {
                let (name, fake) = (self.name_for(&a), self.address(&a));
                map.insert("name".into(), Value::String(name));
                map.insert("address".into(), Value::String(fake));
            }
            None => {
                map.remove("name");
            }
        }
    }
}

fn is_local(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"._%+-'".contains(&b)
}

/// Byte ranges of email addresses in `s`.
pub fn emails(s: &str) -> Vec<Range<usize>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'@' {
            i += 1;
            continue;
        }
        let mut start = i;
        while start > 0 && is_local(b[start - 1]) {
            start -= 1;
        }
        let mut end = i + 1;
        while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'.' || b[end] == b'-')
        {
            end += 1;
        }
        while end > i + 1 && (b[end - 1] == b'.' || b[end - 1] == b'-') {
            end -= 1;
        }
        if start < i && s[i + 1..end].contains('.') {
            out.push(start..end);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn is_guid(w: &[u8]) -> bool {
    w.len() == 36
        && w.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Byte ranges of GUID-shaped strings (8-4-4-4-12 hex).
pub fn guids(s: &str) -> Vec<Range<usize>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 36 <= b.len() {
        let flank = |j: Option<usize>| {
            j.and_then(|j| b.get(j))
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'-')
        };
        if is_guid(&b[i..i + 36]) && !flank(i.checked_sub(1)) && !flank(Some(i + 36)) {
            out.push(i..i + 36);
            i += 36;
        } else {
            i += 1;
        }
    }
    out
}

/// Byte ranges of IPv4 and IPv6 literals.
pub fn ip_literals(s: &str) -> Vec<Range<usize>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // IPv4: a run of digits and dots.
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let run = s[start..i].trim_end_matches('.');
            let parts: Vec<&str> = run.split('.').collect();
            if parts.len() == 4
                && parts
                    .iter()
                    .all(|p| (1..=3).contains(&p.len()) && p.parse::<u8>().is_ok())
            {
                out.push(start..start + run.len());
            }
            continue;
        }
        i += 1;
    }
    // IPv6: a run of hex digits and colons with "::" or four or more colons.
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_hexdigit() || b[i] == b':' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_hexdigit() || b[i] == b':') {
                i += 1;
            }
            let run = &s[start..i];
            let colons = run.matches(':').count();
            let groups_ok = run.split(':').all(|g| g.len() <= 4);
            if groups_ok && run.len() > 2 && (run.contains("::") || colons >= 4) {
                out.push(start..i);
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "redact/tests.rs"]
mod tests;
