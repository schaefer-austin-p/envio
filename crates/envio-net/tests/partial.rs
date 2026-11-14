//! Fetching a big message in parts against a real server: only the text
//! comes with the message, an attachment is fetched on its own. Skips
//! without Stalwart, like stalwart.rs.

mod support;

use envio_net::imap::Flag;
use support::stalwart::{ALICE, BOB, Stalwart, alice, message_id};

const ATTACHMENT_LINES: usize = 40_000;

/// A message with a text body, an inline image and a base64 attachment of
/// about 3 MB on the wire.
fn big_message(id: &str) -> (Vec<u8>, String) {
    let line = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0\r\n";
    let attachment = line.repeat(ATTACHMENT_LINES);
    let eml = format!(
        "From: Alice <{ALICE}>\r\nTo: Bob <{BOB}>\r\nSubject: Big\r\nMessage-ID: {id}\r\n\
         Date: Sun, 08 Nov 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"outer\"\r\n\r\n\
         --outer\r\nContent-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
         --alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello plain\r\n\
         --alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Hello html</p>\r\n\
         --alt--\r\n\
         --outer\r\nContent-Type: application/pdf; name=\"big.pdf\"\r\n\
         Content-Disposition: attachment; filename=\"big.pdf\"\r\n\
         Content-Transfer-Encoding: base64\r\n\r\n{attachment}\r\n\
         --outer--\r\n"
    );
    (eml.into_bytes(), attachment)
}

#[tokio::test]
async fn only_the_text_comes_with_a_big_message_and_an_attachment_comes_on_demand() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    let (eml, attachment) = big_message(&id);
    session.append("INBOX", &[Flag::Seen], &eml).await.unwrap();
    session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];

    let partial = session.fetch_partial(uid).await.unwrap().unwrap();
    let text = String::from_utf8(partial.clone()).unwrap();
    assert!(partial.len() < 5_000, "{} bytes", partial.len());
    assert!(text.contains("Hello plain") && text.contains("<p>Hello html</p>"));
    assert!(text.contains("Subject: Big"));
    assert!(text.contains("X-Envio-Section: 2\r\n"), "{text}");
    assert!(!text.contains("QUJDREVG"), "attachment bytes came along");

    let part = session
        .fetch_section(uid, "2", attachment.len())
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8(part).unwrap();
    let (header, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(header.contains("application/pdf"), "{header}");
    assert_eq!(body.trim_end(), attachment.trim_end());

    assert!(session.fetch_section(uid, "9", 10).await.unwrap().is_none());
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_single_part_message_is_a_stub_or_its_text() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    session.select("INBOX").await.unwrap();
    let mut uids = Vec::new();
    for (ctype, body) in [("text/plain", "just text"), ("application/pdf", "QUJD")] {
        let id = message_id();
        let eml = format!(
            "From: Alice <{ALICE}>\r\nTo: Bob <{BOB}>\r\nSubject: One\r\nMessage-ID: {id}\r\n\
             Date: Sun, 08 Nov 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\n\
             Content-Type: {ctype}\r\n\r\n{body}\r\n"
        );
        session.append("INBOX", &[], eml.as_bytes()).await.unwrap();
        session.select("INBOX").await.unwrap();
        uids.push(session.search_message_id(&id).await.unwrap()[0]);
    }
    let text = String::from_utf8(session.fetch_partial(uids[0]).await.unwrap().unwrap()).unwrap();
    assert!(text.ends_with("just text\r\n"), "{text}");
    let stub = String::from_utf8(session.fetch_partial(uids[1]).await.unwrap().unwrap()).unwrap();
    assert!(
        stub.contains("X-Envio-Section: 1\r\n") && !stub.contains("QUJD"),
        "{stub}"
    );
    let part = session
        .fetch_section(uids[1], "1", 10)
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8(part).unwrap().ends_with("QUJD\r\n"));
    session.logout().await.unwrap();
}

/// A message of `parts` (each a header block and a body) under `extra`
/// message headers.
fn multipart(id: &str, extra: &str, parts: &[(&str, &str)]) -> Vec<u8> {
    let mut eml = format!(
        "From: Alice <{ALICE}>\r\nTo: Bob <{BOB}>\r\nSubject: Crafted\r\nMessage-ID: {id}\r\n\
         Date: Sun, 08 Nov 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\n{extra}\
         Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n"
    );
    for (header, body) in parts {
        eml.push_str(&format!("--b\r\n{header}\r\n\r\n{body}\r\n"));
    }
    eml.push_str("--b--\r\n");
    eml.into_bytes()
}

#[tokio::test]
async fn a_sender_cannot_pass_its_own_stub_marks_off_as_envios() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    let eml = multipart(
        &id,
        "X-Envio-Section: 7\r\n",
        &[
            (
                "Content-Type: text/plain\r\nX-Envio-Section: 1\r\nX-Envio-Size: 99",
                "hello",
            ),
            (
                "Content-Type: application/zip\r\nContent-Disposition: attachment; filename=a.zip\r\n\
                 Content-Transfer-Encoding: base64\r\nx-envio-section: 5\r\nX-Envio-Size: 99",
                "UEsDBA==",
            ),
        ],
    );
    session.append("INBOX", &[], &eml).await.unwrap();
    session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];

    let whole = session.fetch_body(uid, eml.len()).await.unwrap().unwrap();
    let text = String::from_utf8(whole).unwrap().to_ascii_lowercase();
    assert!(!text.contains("\nx-envio-"), "{text}");
    assert!(text.contains("x-untrusted-envio-section: 7"), "{text}");

    let partial = session.fetch_partial(uid).await.unwrap().unwrap();
    let text = String::from_utf8(partial).unwrap();
    // The zip is not text, so its stub mark is envio's own (section 2); the
    // sender's marks, on the text, the zip and the message, are not.
    assert_eq!(text.matches("\nX-Envio-Section: ").count(), 1, "{text}");
    assert!(text.contains("\nX-Envio-Section: 2\r\n"), "{text}");
    assert!(text.contains("\r\n\r\nhello"), "{text}");
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_text_part_cut_at_the_cap_says_so() {
    let Some(server) = Stalwart::find().await else {
        return;
    };
    let mut session = server.imap(&alice()).await;
    let id = message_id();
    let long = "abcdefghij".repeat(220_000);
    let eml = multipart(&id, "", &[("Content-Type: text/plain", &long)]);
    session.append("INBOX", &[], &eml).await.unwrap();
    session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];
    let text = String::from_utf8(session.fetch_partial(uid).await.unwrap().unwrap()).unwrap();
    assert!(text.contains("\nX-Envio-Truncated: "), "{}", &text[..400]);
    assert!(text.len() < 2_200_000, "{} bytes", text.len());

    let id = message_id();
    let eml = multipart(&id, "", &[("Content-Type: text/plain", "short")]);
    session.append("INBOX", &[], &eml).await.unwrap();
    session.select("INBOX").await.unwrap();
    let uid = session.search_message_id(&id).await.unwrap()[0];
    let text = String::from_utf8(session.fetch_partial(uid).await.unwrap().unwrap()).unwrap();
    assert!(!text.contains("X-Envio-Truncated"), "{text}");
    session.logout().await.unwrap();
}
