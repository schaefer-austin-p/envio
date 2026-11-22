use super::*;

fn e(status: u16, code: &str) -> GraphError {
    GraphError {
        status,
        code: code.into(),
        retry_after_s: None,
    }
}

fn kind(m: Mapped) -> &'static str {
    match m {
        Mapped::Err(NetError::Busy(_)) => "busy",
        Mapped::Err(NetError::Refused(_)) => "refused",
        Mapped::Err(NetError::Unconfirmed(_)) => "unconfirmed",
        Mapped::Err(_) => "other",
        Mapped::Gone => "gone",
        Mapped::Expired => "expired",
        Mapped::Retry401 => "retry401",
    }
}

fn k(status: u16, code: &str, op: Op) -> &'static str {
    kind(to_net(&e(status, code), op))
}

#[test]
fn the_spec_table_for_reads_and_writes() {
    for op in [Op::Read, Op::Write, Op::Delta, Op::Create] {
        assert_eq!(k(401, "", op), "retry401", "{op:?}");
        assert_eq!(k(403, "", op), "refused", "{op:?}");
        for s in [409, 423, 429, 500, 502, 503, 504] {
            assert_eq!(k(s, "", op), "busy", "{s} {op:?}");
        }
        for s in [400, 413, 507, 418, 301] {
            assert_eq!(k(s, "", op), "refused", "{s} {op:?}");
        }
    }
}

#[test]
fn a_404_is_gone_when_reading_or_changing_but_refused_when_creating() {
    for op in [Op::Read, Op::Write, Op::Delta] {
        assert_eq!(k(404, "", op), "gone", "{op:?}");
    }
    assert_eq!(k(404, "", Op::Create), "refused");
}

#[test]
fn delta_expires_on_410_or_the_sync_state_code() {
    assert_eq!(k(410, "", Op::Delta), "expired");
    assert_eq!(k(400, "syncStateNotFound", Op::Delta), "expired");
    assert_eq!(k(410, "", Op::Read), "refused");
    assert_eq!(k(400, "syncStateNotFound", Op::Read), "refused");
}

#[test]
fn a_403_on_send_is_busy_only_for_a_send_limit_code() {
    assert_eq!(k(403, "ErrorExceededMessageLimit", Op::Send), "busy");
    assert_eq!(k(403, "SendAsDenied", Op::Send), "refused");
    assert_eq!(k(403, "ErrorExceededMessageLimit", Op::Write), "refused");
}

#[test]
fn send_statuses_in_the_confirmed_set_map_as_the_table() {
    assert_eq!(k(401, "", Op::Send), "retry401");
    for s in [400, 413, 507, 403] {
        assert_eq!(k(s, "", Op::Send), "refused", "{s}");
    }
    for s in [409, 423, 429, 503] {
        assert_eq!(k(s, "", Op::Send), "busy", "{s}");
    }
}

#[test]
fn every_other_send_status_is_unconfirmed() {
    for s in [500, 502, 504, 404, 408, 410, 418, 302, 599] {
        assert_eq!(k(s, "", Op::Send), "unconfirmed", "{s}");
    }
}

#[test]
fn messages_hold_status_and_code_only() {
    let Mapped::Err(err) = to_net(&e(429, "TooManyRequests"), Op::Read) else {
        panic!("not an error");
    };
    assert_eq!(
        err.to_string(),
        "server busy: HTTP 429, Graph code TooManyRequests"
    );
    let Mapped::Err(err) = to_net(&e(418, ""), Op::Read) else {
        panic!("not an error");
    };
    assert_eq!(err.to_string(), "refused by the server: HTTP 418");
}
