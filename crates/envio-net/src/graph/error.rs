//! The spec's status table: a Graph error reply to what the caller does.

use super::model::GraphError;
use crate::NetError;

/// What the request was, which decides what a status means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// A read of one message or a list.
    Read,
    /// A patch, move or delete.
    Write,
    /// A delta page.
    Delta,
    /// `sendMail`: stricter, see [`to_net`].
    Send,
    /// A draft or message created from MIME.
    Create,
}

#[derive(Debug)]
pub enum Mapped {
    Err(NetError),
    /// The thing is gone: callers turn this into `None`, `Ok(())` or a
    /// dropped id, as their call says.
    Gone,
    /// A delta link that is no longer valid.
    Expired,
    /// A 401: get a fresh token once and repeat; a second 401 is `SignIn`.
    Retry401,
}

/// Graph error codes on a 403 that mean "send limit", not "not allowed".
/// `SendAsDenied` is not one of them.
const SEND_LIMIT_CODES: [&str; 1] = ["ErrorExceededMessageLimit"];

/// `sendMail` statuses where Graph certainly did not take the message.
/// Anything else may have been taken, so a requeue could send twice.
const SEND_NOT_SENT: [u16; 9] = [400, 401, 403, 409, 413, 423, 429, 503, 507];

fn text(e: &GraphError) -> String {
    match e.code.as_str() {
        "" => format!("HTTP {}", e.status),
        code => format!("HTTP {}, Graph code {code}", e.status),
    }
}

fn is_send_limit(e: &GraphError) -> bool {
    SEND_LIMIT_CODES
        .iter()
        .any(|c| c.eq_ignore_ascii_case(&e.code))
}

/// Maps a non-2xx reply. `Op::Send` maps a status outside
/// [`SEND_NOT_SENT`] to `NetError::Unconfirmed`.
pub fn to_net(e: &GraphError, op: Op) -> Mapped {
    if op == Op::Send && !SEND_NOT_SENT.contains(&e.status) {
        return Mapped::Err(NetError::Unconfirmed(text(e)));
    }
    let msg = text(e);
    match (e.status, op) {
        (401, _) => Mapped::Retry401,
        (404, Op::Read | Op::Write | Op::Delta) => Mapped::Gone,
        (410, Op::Delta) => Mapped::Expired,
        (_, Op::Delta) if e.code.eq_ignore_ascii_case("syncStateNotFound") => Mapped::Expired,
        (403, Op::Send) if is_send_limit(e) => Mapped::Err(NetError::Busy(msg)),
        (409 | 423 | 429 | 500 | 502 | 503 | 504, _) => Mapped::Err(NetError::Busy(msg)),
        _ => Mapped::Err(NetError::Refused(msg)),
    }
}

#[cfg(test)]
mod tests;
