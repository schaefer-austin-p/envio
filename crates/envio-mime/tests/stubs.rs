//! A message whose attachment was left on the server: envio-net's stub
//! headers (`X-Envio-Section`, `X-Envio-Size`) say where it is and how big.

use envio_mime::{Message, attachment_bytes, attachments_bytes, decode_part, parse};

const WITH_STUB: &str = "From: a@x.test\r\nSubject: Big\r\nMIME-Version: 1.0\r\n\
    Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n\
    --b\r\nContent-Type: text/plain\r\n\r\nhello\r\n\
    --b\r\nContent-Type: application/pdf; name=\"big.pdf\"\r\n\
    Content-Disposition: attachment; filename=\"big.pdf\"\r\n\
    Content-Transfer-Encoding: base64\r\n\
    X-Envio-Section: 2\r\nX-Envio-Size: 31457280\r\n\r\n\r\n\
    --b\r\nContent-Type: application/zip\r\n\
    Content-Disposition: attachment; filename=\"small.zip\"\r\n\
    Content-Transfer-Encoding: base64\r\n\r\nUEsDBA==\r\n\
    --b--\r\n";

fn attachments(raw: &str) -> Vec<envio_mime::AttachmentInfo> {
    match parse(raw.as_bytes()) {
        Message::Parsed { attachments, .. } => attachments,
        Message::Raw(_) => panic!("did not parse"),
    }
}

#[test]
fn a_stub_reports_its_section_and_the_real_size() {
    let list = attachments(WITH_STUB);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].section.as_deref(), Some("2"));
    assert_eq!(list[0].size, 31_457_280);
    assert_eq!(list[0].filename.as_deref(), Some("big.pdf"));
    assert_eq!(list[1].section, None);
    assert_eq!(list[1].size, 4);
}

#[test]
fn a_stub_has_no_bytes_but_a_fetched_part_does() {
    assert_eq!(attachment_bytes(WITH_STUB.as_bytes(), 0), None);
    assert_eq!(
        attachment_bytes(WITH_STUB.as_bytes(), 1).as_deref(),
        Some(&b"PK\x03\x04"[..])
    );
    let many = attachments_bytes(WITH_STUB.as_bytes(), &[0, 1]);
    assert_eq!(many[0], None);
    assert!(many[1].is_some());
}

#[test]
fn a_part_fetched_alone_decodes() {
    let part =
        "Content-Type: application/pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nUEsDBA==\r\n";
    assert_eq!(
        decode_part(part.as_bytes()).as_deref(),
        Some(&b"PK\x03\x04"[..])
    );
    let plain =
        "Content-Type: text/plain\r\nContent-Disposition: attachment; filename=a.txt\r\n\r\nhi\r\n";
    assert_eq!(
        decode_part(plain.as_bytes()).as_deref(),
        Some(&b"hi\r\n"[..])
    );
}

#[test]
fn a_single_part_stub_is_listed() {
    let raw = "From: a@x.test\r\nMIME-Version: 1.0\r\nContent-Type: application/pdf\r\n\
        X-Envio-Section: 1\r\nX-Envio-Size: 9000000\r\n\r\n";
    let list = attachments(raw);
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0].section.as_deref(), Some("1"));
    assert_eq!(list[0].size, 9_000_000);
}
