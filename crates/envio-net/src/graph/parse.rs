//! Pure parsers for Graph replies. Bad JSON, or a reply of the wrong shape,
//! is `NetError::Protocol` with a fixed text: no part of a body (addresses,
//! subjects) reaches an error.

use serde_json::Value;

use super::model::{DeltaNext, DeltaPage, GChange, GFolder, GMeta, GSummary, GraphError, SubReply};
use super::time::rfc3339_ms;
use crate::{NetError, Result};

/// The `Integer 0x0E08` property (message size), as Graph spells it back.
const SIZE_PROPERTY: &str = "integer 0xe08";

fn json(body: &[u8]) -> Result<Value> {
    serde_json::from_slice(body).map_err(|_| NetError::Protocol("Graph reply is not JSON".into()))
}

fn bad(what: &str) -> NetError {
    NetError::Protocol(format!("Graph reply has no usable {what}"))
}

fn items(v: &Value) -> Result<&Vec<Value>> {
    v["value"].as_array().ok_or_else(|| bad("value list"))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_owned)
}

fn folder(v: &Value) -> Result<GFolder> {
    Ok(GFolder {
        id: text(v, "id").ok_or_else(|| bad("folder id"))?,
        parent_id: text(v, "parentFolderId"),
        display_name: text(v, "displayName").unwrap_or_default(),
        well_known: text(v, "wellKnownName"),
        hidden: v["isHidden"].as_bool().unwrap_or(false),
        class: text(v, "folderClass"),
        child_count: v["childFolderCount"]
            .as_u64()
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
    })
}

/// A page of folders and the next link, if any.
pub fn folders(body: &[u8]) -> Result<(Vec<GFolder>, Option<String>)> {
    let v = json(body)?;
    let list = items(&v)?.iter().map(folder).collect::<Result<_>>()?;
    Ok((list, text(&v, "@odata.nextLink")))
}

fn meta(v: &Value) -> Result<GMeta> {
    let received = v["receivedDateTime"].as_str().unwrap_or_default();
    Ok(GMeta {
        id: text(v, "id").ok_or_else(|| bad("message id"))?,
        received_ms: rfc3339_ms(received).ok_or_else(|| bad("receivedDateTime"))?,
        is_read: v["isRead"].as_bool().unwrap_or(false),
        flagged: v["flag"]["flagStatus"].as_str() == Some("flagged"),
        categories: v["categories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_owned))
            .collect(),
        is_draft: v["isDraft"].as_bool().unwrap_or(false),
    })
}

/// A delta item: only what the reply carries.
fn change(v: &Value) -> Result<GChange> {
    let received_ms = match v["receivedDateTime"].as_str() {
        Some(s) => Some(rfc3339_ms(s).ok_or_else(|| bad("receivedDateTime"))?),
        None => None,
    };
    Ok(GChange {
        id: text(v, "id").ok_or_else(|| bad("message id"))?,
        received_ms,
        is_read: v["isRead"].as_bool(),
        flagged: v["flag"]["flagStatus"].as_str().map(|s| s == "flagged"),
        categories: v["categories"].as_array().map(|list| {
            list.iter()
                .filter_map(|c| c.as_str().map(str::to_owned))
                .collect()
        }),
        is_draft: v["isDraft"].as_bool(),
    })
}

/// True if the body is Graph's "this delta link is dead" error.
fn is_sync_state_not_found(v: &Value) -> bool {
    v["error"]["code"]
        .as_str()
        .is_some_and(|c| c.eq_ignore_ascii_case("syncStateNotFound"))
}

/// A delta page. A 410, or a `syncStateNotFound` error body, is `Expired`.
pub fn delta(status: u16, body: &[u8]) -> Result<DeltaPage> {
    if status == 410 {
        return Ok(DeltaPage::Expired);
    }
    let v = json(body)?;
    if is_sync_state_not_found(&v) {
        return Ok(DeltaPage::Expired);
    }
    let next = match (text(&v, "@odata.nextLink"), text(&v, "@odata.deltaLink")) {
        (Some(link), _) => DeltaNext::More(link),
        (None, Some(link)) => DeltaNext::Done(link),
        (None, None) => return Err(bad("next or delta link")),
    };
    let (mut changed, mut removed) = (Vec::new(), Vec::new());
    for item in items(&v)? {
        if item.get("@removed").is_some() {
            removed.push(text(item, "id").ok_or_else(|| bad("message id"))?);
        } else {
            changed.push(change(item)?);
        }
    }
    Ok(DeltaPage::Page {
        changed,
        removed,
        next,
    })
}

/// A page of `$search` or `$filter` results and the next link, if any.
pub fn search(body: &[u8]) -> Result<(Vec<GMeta>, Option<String>)> {
    let v = json(body)?;
    let list = items(&v)?.iter().map(meta).collect::<Result<_>>()?;
    Ok((list, text(&v, "@odata.nextLink")))
}

/// The size from the extended properties, 0 if absent or unreadable.
fn size_of(v: &Value) -> u32 {
    v["singleValueExtendedProperties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| {
            p["id"]
                .as_str()
                .is_some_and(|id| id.eq_ignore_ascii_case(SIZE_PROPERTY) || is_padded_size(id))
        })
        .and_then(|p| p["value"].as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// `Integer 0x0E08` spelled with leading zeros (`0x00E08`).
fn is_padded_size(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower
        .strip_prefix("integer 0x")
        .is_some_and(|h| h.trim_start_matches('0') == "e08")
}

/// One message's summary: the `$batch` sub-body.
pub fn summary(body: &[u8]) -> Result<GSummary> {
    let v = json(body)?;
    let headers = v["internetMessageHeaders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| Some((text(h, "name")?, text(h, "value")?)))
        .collect();
    Ok(GSummary {
        id: text(&v, "id").ok_or_else(|| bad("message id"))?,
        received_ms: rfc3339_ms(v["receivedDateTime"].as_str().unwrap_or_default())
            .ok_or_else(|| bad("receivedDateTime"))?,
        size: size_of(&v),
        headers,
    })
}

/// A header value in a sub-reply's `headers` object, any case.
fn sub_header<'a>(h: &'a Value, name: &str) -> Option<&'a str> {
    h.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.as_str())
}

/// `Retry-After` as seconds; an HTTP date (or junk) is `None`.
pub fn retry_after(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| v.trim().parse().ok())
}

/// The sub-replies of a `$batch`, in the order Graph gave them. Match them
/// to requests by `id`, never by position.
pub fn batch(body: &[u8]) -> Result<Vec<SubReply>> {
    let v = json(body)?;
    let list = v["responses"]
        .as_array()
        .ok_or_else(|| bad("responses list"))?;
    list.iter()
        .map(|r| {
            let status = r["status"]
                .as_u64()
                .and_then(|s| u16::try_from(s).ok())
                .ok_or_else(|| bad("sub-status"))?;
            let id = match &r["id"] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(bad("sub-request id")),
            };
            Ok(SubReply {
                id,
                status,
                retry_after_s: retry_after(sub_header(&r["headers"], "Retry-After")),
                body: serde_json::to_vec(&r["body"]).unwrap_or_default(),
            })
        })
        .collect()
}

/// A Graph error reply reduced to status and code. The code is kept only if
/// it is a plain identifier, so nothing else can ride along into a log.
pub fn error(status: u16, body: &[u8], retry_after: Option<&str>) -> GraphError {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| text(&v["error"], "code"))
        .filter(|c| {
            c.len() <= 64
                && c.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        .unwrap_or_default();
    GraphError {
        status,
        code,
        retry_after_s: self::retry_after(retry_after),
    }
}

#[cfg(test)]
mod tests;
