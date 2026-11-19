//! Chunked replies as `parse_response` and `response_complete` read them.

use super::*;
use tokio::io::AsyncWriteExt;

const HEAD: &str = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";

fn wire(body: &str) -> Vec<u8> {
    format!("{HEAD}{body}").into_bytes()
}

/// `read_response`, failing (not hanging) if it waits for a close.
async fn read_within<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut R,
) -> (zeroize::Zeroizing<Vec<u8>>, bool) {
    tokio::time::timeout(std::time::Duration::from_secs(5), read_response(stream))
        .await
        .expect("read_response waited for a close")
        .unwrap()
}

fn decoded(body: &str) -> Result<(u16, Vec<u8>), BadReply> {
    parse_response(&wire(body), true)
}

#[test]
fn a_chunked_response_is_decoded_and_keeps_its_status() {
    assert_eq!(
        decoded("5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"),
        Ok((200, b"hello world".to_vec()))
    );
    let err = "HTTP/1.1 400 Bad\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
    assert_eq!(
        parse_response(err.as_bytes(), true),
        Ok((400, b"{}".to_vec()))
    );
    // Hex sizes in either case, the header name and value in any case.
    let raw = "HTTP/1.1 200 OK\r\ntransfer-encoding: Chunked\r\n\r\nA\r\n0123456789\r\n0\r\n\r\n";
    assert_eq!(
        parse_response(raw.as_bytes(), true),
        Ok((200, b"0123456789".to_vec()))
    );
}

#[test]
fn chunk_extensions_and_trailers_are_skipped() {
    assert_eq!(
        decoded("3;name=value;x\r\nabc\r\n2;q\r\nde\r\n0;last\r\nX-Sum: 1\r\nY: 2\r\n\r\n"),
        Ok((200, b"abcde".to_vec()))
    );
    // A space before the ';' is not part of a hex size.
    assert_eq!(
        decoded("2 ;ignored\r\nde\r\n0\r\n\r\n"),
        Err(BadReply::Malformed)
    );
}

#[test]
fn a_chunked_body_at_the_cap_is_read_and_one_byte_more_is_not() {
    let at = |n: usize| format!("{n:x}\r\n{}\r\n0\r\n\r\n", "a".repeat(n));
    assert_eq!(decoded(&at(MAX_BODY)).unwrap().1.len(), MAX_BODY);
    assert_eq!(decoded(&at(MAX_BODY + 1)), Err(BadReply::TooBig));
}

#[test]
fn chunks_that_add_up_past_the_cap_are_too_big() {
    let half = "a".repeat(MAX_BODY / 2 + 1);
    let n = half.len();
    let body = format!("{n:x}\r\n{half}\r\n{n:x}\r\n{half}\r\n0\r\n\r\n");
    assert_eq!(decoded(&body), Err(BadReply::TooBig));
}

#[test]
fn a_chunk_size_is_refused_before_its_data_arrives() {
    // Only the size line is here: the verdict does not wait for the data.
    let raw = wire(&format!("{:x}\r\n", MAX_BODY + 1));
    assert!(response_complete(&raw));
    assert_eq!(parse_response(&raw, false), Err(BadReply::TooBig));
    // A size too big for usize is too big, not a parse failure.
    let raw = wire("FFFFFFFFFFFFFFFFFFFFFFFF\r\n");
    assert!(response_complete(&raw));
    assert_eq!(parse_response(&raw, true), Err(BadReply::TooBig));
}

#[test]
fn a_truncated_chunked_body_is_not_complete() {
    let whole = wire("5\r\nhello\r\n0\r\n\r\n");
    // Every proper prefix of a valid reply is incomplete, so the reader
    // keeps reading, and none of them decodes.
    for cut in HEAD.len()..whole.len() {
        let part = &whole[..cut];
        assert!(!response_complete(part), "cut at {cut}");
        // Ended by a dropped socket: worth a retry. Ended by a clean
        // close: the server's own fault, so final.
        assert_eq!(parse_response(part, false), Err(BadReply::Unreadable));
        assert_eq!(parse_response(part, true), Err(BadReply::Malformed));
    }
    assert!(response_complete(&whole));
}

#[test]
fn broken_chunk_framing_is_malformed() {
    for body in [
        "zz\r\nhi\r\n0\r\n\r\n",
        "\r\nhi\r\n0\r\n\r\n",
        "-1\r\nhi\r\n0\r\n\r\n",
        "0x2\r\nhi\r\n0\r\n\r\n",
        "2\r\nhiXX0\r\n\r\n",
        "2\r\nhello\r\n0\r\n\r\n",
        ";ext\r\nhi\r\n0\r\n\r\n",
    ] {
        assert_eq!(decoded(body), Err(BadReply::Malformed), "{body:?}");
        assert!(response_complete(&wire(body)), "{body:?}");
    }
}

#[test]
fn a_size_line_or_trailers_without_end_are_bounded() {
    // No line end within the limit: malformed, without waiting for more.
    let long = wire(&format!("5;{}", "x".repeat(MAX_CHUNK_LINE + 1)));
    assert!(response_complete(&long));
    assert_eq!(parse_response(&long, false), Err(BadReply::Malformed));
    let trailers = format!("0\r\n{}", "T: v\r\n".repeat(MAX_HEAD / 6 + 2));
    assert!(response_complete(&wire(&trailers)));
    assert_eq!(decoded(&trailers), Err(BadReply::Malformed));
    // A single trailer line over the limit too.
    let one = format!("0\r\nT: {}", "v".repeat(MAX_HEAD + 1));
    assert_eq!(decoded(&one), Err(BadReply::Malformed));
}

#[test]
fn an_encoding_other_than_a_lone_chunked_is_malformed() {
    for value in ["gzip, chunked", "chunked, chunked", "gzip", "identity"] {
        let raw =
            format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: {value}\r\n\r\n2\r\n{{}}\r\n0\r\n\r\n");
        assert!(response_complete(raw.as_bytes()), "{value}");
        assert_eq!(
            parse_response(raw.as_bytes(), true),
            Err(BadReply::Malformed),
            "{value}"
        );
    }
}

#[test]
fn a_content_length_reply_still_works() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
    assert!(response_complete(raw));
    assert_eq!(parse_response(raw, false), Ok((200, b"{}".to_vec())));
    // Cut short of its length: still a retry, not final.
    let cut = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{}";
    assert!(!response_complete(cut));
    assert_eq!(parse_response(cut, true), Err(BadReply::Unreadable));
    // Neither length nor chunked: the body runs to the close.
    assert_eq!(
        parse_response(b"HTTP/1.0 200 OK\r\n\r\n{}", true),
        Ok((200, b"{}".to_vec()))
    );
}

#[test]
fn transfer_encoding_wins_over_a_content_length() {
    let raw = "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
    assert_eq!(
        parse_response(raw.as_bytes(), true),
        Ok((200, b"{}".to_vec()))
    );
}

#[tokio::test]
async fn read_response_stops_at_the_last_chunk_without_waiting_for_a_close() {
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    server
        .write_all(&wire("5\r\nhello\r\n0\r\nT: 1\r\n\r\n"))
        .await
        .unwrap();
    // `server` stays open: only the framing can end the read.
    let (raw, clean) = read_within(&mut client).await;
    assert!(clean);
    assert_eq!(parse_response(&raw, true), Ok((200, b"hello".to_vec())));
}

#[tokio::test]
async fn read_response_stops_at_a_bad_chunk_without_waiting_for_a_close() {
    let (mut client, mut server) = tokio::io::duplex(64 * 1024);
    server.write_all(&wire("zz\r\n")).await.unwrap();
    let (raw, _) = read_within(&mut client).await;
    assert_eq!(parse_response(&raw, true), Err(BadReply::Malformed));
}

#[test]
fn the_shared_autoconfig_reader_still_refuses_chunked() {
    let raw = wire("5\r\nhello\r\n0\r\n\r\n");
    assert_eq!(parse_reply(&raw), Reply::Chunked);
    assert!(reply_complete(&raw));
}

#[test]
fn a_size_line_over_the_limit_is_malformed_even_with_its_line_end() {
    let line = format!("5;{}\r\nhello\r\n0\r\n\r\n", "x".repeat(MAX_CHUNK_LINE + 1));
    assert_eq!(decoded(&line), Err(BadReply::Malformed));
    let ok = format!("5;{}\r\nhello\r\n0\r\n\r\n", "x".repeat(MAX_CHUNK_LINE - 2));
    assert_eq!(decoded(&ok), Ok((200, b"hello".to_vec())));
}

#[test]
fn a_wrong_byte_after_the_data_is_refused_at_once() {
    // Only one byte follows the data, and it is not a CR.
    let raw = wire("5\r\nhelloX");
    assert!(response_complete(&raw));
    assert_eq!(parse_response(&raw, false), Err(BadReply::Malformed));
    // A CR alone is a valid prefix.
    assert!(!response_complete(&wire("5\r\nhello\r")));
}

#[test]
fn a_repeated_transfer_encoding_header_is_malformed() {
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
    assert_eq!(
        parse_response(raw.as_bytes(), true),
        Err(BadReply::Malformed)
    );
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked, gzip\r\n\r\n";
    assert_eq!(
        parse_response(raw.as_bytes(), true),
        Err(BadReply::Malformed)
    );
}

#[test]
fn the_autoconfig_reader_refuses_any_transfer_encoding() {
    for value in ["chunked", "gzip, chunked", "gzip"] {
        let raw = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: {value}\r\n\r\n<a/>");
        assert_eq!(parse_reply(raw.as_bytes()), Reply::Chunked, "{value}");
        assert!(reply_complete(raw.as_bytes()), "{value}");
    }
}
