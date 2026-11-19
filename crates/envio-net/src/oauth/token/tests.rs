use super::*;

#[test]
fn invalid_grant_at_other_statuses_is_refused_not_revoked() {
    for s in [401, 403, 404] {
        let e = parse_token_reply(s, br#"{"error":"invalid_grant"}"#).err();
        assert_eq!(
            e,
            Some(TokenError::Refused("invalid_grant".to_owned())),
            "{s}"
        );
    }
}

#[test]
fn only_2xx_is_success() {
    let body = br#"{"access_token":"AT","expires_in":60}"#;
    for s in [100, 300, 302, 399] {
        assert!(
            matches!(parse_token_reply(s, body), Err(TokenError::Refused(_))),
            "{s}"
        );
    }
    assert!(parse_token_reply(299, body).is_ok());
}

#[test]
fn an_empty_refresh_token_is_none() {
    let body = br#"{"access_token":"AT","refresh_token":"","expires_in":60}"#;
    assert!(parse_token_reply(200, body).unwrap().refresh.is_none());
}

#[test]
fn a_padded_payload_is_accepted() {
    use base64::Engine;
    let payload = r#"{"email":"a@example.org","x":"12"}"#;
    assert_ne!(payload.len() % 3, 0);
    let padded = base64::engine::general_purpose::URL_SAFE.encode(payload);
    assert!(padded.ends_with('='));
    let got = identity_of(&format!("h.{padded}.s"));
    assert_eq!(got.and_then(|i| i.email), Some("a@example.org".to_owned()));
}

#[test]
fn the_standard_alphabet_is_rejected() {
    use base64::Engine;
    let payload = r#"{"email":"a@example.org","x":"???>>>"}"#;
    let std_enc = base64::engine::general_purpose::STANDARD_NO_PAD.encode(payload);
    assert!(std_enc.contains('+') || std_enc.contains('/'), "{std_enc}");
    assert_eq!(identity_of(&format!("h.{std_enc}.s")), None);
    // The same claims in base64url do parse.
    let url_enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
    assert!(identity_of(&format!("h.{url_enc}.s")).is_some());
}
use base64::Engine;
use secrecy::ExposeSecret;

fn jwt(payload: &str) -> String {
    let b = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
    format!("{}.{}.sig", b(r#"{"alg":"none"}"#), b(payload))
}

fn reply(status: u16, body: &str) -> Result<Tokens, TokenError> {
    parse_token_reply(status, body.as_bytes())
}

fn id(email: Option<&str>, upn: Option<&str>) -> Identity {
    Identity {
        email: email.map(str::to_owned),
        preferred_username: upn.map(str::to_owned),
    }
}

#[test]
fn a_full_reply_parses() {
    let idt = jwt(r#"{"email":"a@example.org","preferred_username":"a@envio.test"}"#);
    let body = format!(
        r#"{{"token_type":"Bearer","access_token":"AT","refresh_token":"RT","expires_in":3599,"id_token":"{idt}"}}"#
    );
    let t = reply(200, &body).unwrap();
    assert_eq!(t.access.expose_secret(), "AT");
    assert_eq!(t.refresh.as_ref().unwrap().expose_secret(), "RT");
    assert_eq!(t.expires_in, 3599);
    assert_eq!(
        t.identity,
        Some(id(Some("a@example.org"), Some("a@envio.test")))
    );
}

#[test]
fn a_reply_without_refresh_token_or_id_token_parses() {
    let t = reply(200, r#"{"access_token":"AT","expires_in":60}"#).unwrap();
    assert!(t.refresh.is_none());
    assert!(t.identity.is_none());
    assert_eq!(t.expires_in, 60);
}

#[test]
fn a_negative_expires_in_means_already_expired() {
    let t = reply(200, r#"{"access_token":"AT","expires_in":-5}"#).unwrap();
    assert_eq!(t.expires_in, 0);
}

#[test]
fn a_reply_without_an_access_token_is_refused() {
    assert!(matches!(
        reply(200, r#"{"expires_in":60}"#),
        Err(TokenError::Refused(_))
    ));
    let empty = r#"{"access_token":"","expires_in":60}"#;
    assert!(matches!(reply(200, empty), Err(TokenError::Refused(_))));
}

#[test]
fn the_revoking_errors_are_revoked() {
    for code in [
        "invalid_grant",
        "interaction_required",
        "consent_required",
        "invalid_client",
    ] {
        let body = format!(r#"{{"error":"{code}","error_description":"x"}}"#);
        assert!(
            matches!(reply(400, &body), Err(TokenError::Revoked)),
            "{code}"
        );
    }
}

#[test]
fn other_client_errors_are_refused_with_the_code_only() {
    let body = r#"{"error":"invalid_request","error_description":"for a@example.org"}"#;
    assert_eq!(
        reply(400, body).err(),
        Some(TokenError::Refused("invalid_request".to_owned()))
    );
    assert!(matches!(reply(401, "{}"), Err(TokenError::Refused(_))));
    assert!(matches!(reply(404, "nope"), Err(TokenError::Refused(_))));
}

#[test]
fn server_errors_and_throttling_are_temporary() {
    for s in [500, 502, 503, 429] {
        let r = reply(s, r#"{"error":"invalid_grant"}"#);
        assert!(matches!(r, Err(TokenError::Temporary)), "{s}");
    }
}

#[test]
fn a_html_body_never_reaches_the_error() {
    let html = "<html><body>SECRET-MARKER captive portal</body></html>";
    for s in [200, 400, 403] {
        let e = reply(s, html).err().unwrap();
        assert!(!format!("{e:?}").contains("SECRET-MARKER"), "{s}");
    }
    assert!(matches!(reply(502, html), Err(TokenError::Temporary)));
}

#[test]
fn an_error_code_that_is_not_a_code_is_not_echoed() {
    let e = reply(400, r#"{"error":"bad thing for a@example.org"}"#)
        .err()
        .unwrap();
    assert!(!format!("{e:?}").contains("example.org"));
}

#[test]
fn tokens_debug_hides_every_secret() {
    let body = r#"{"access_token":"AT-MARKER","refresh_token":"RT-MARKER","expires_in":1}"#;
    let text = format!("{:?}", reply(200, body).unwrap());
    assert!(!text.contains("AT-MARKER") && !text.contains("RT-MARKER"));
}

#[test]
fn the_code_request_body() {
    let p = Provider::microsoft_template().with_client_id("cid 1");
    let body = token_request_body(
        &p,
        &Grant::Code {
            code: "c&d=e",
            verifier: "v+w",
        },
    );
    assert_eq!(
        body.expose_secret(),
        "client_id=cid%201&grant_type=authorization_code&code=c%26d%3De\
         &redirect_uri=envio-oauth%3A%2F%2Fcallback&code_verifier=v%2Bw"
    );
}

#[test]
fn the_refresh_request_body() {
    let p = Provider::microsoft_template().with_client_id("cid");
    let body = token_request_body(&p, &Grant::Refresh { token: "r t" });
    let text = body.expose_secret();
    assert!(text.starts_with("client_id=cid&grant_type=refresh_token&refresh_token=r%20t&"));
    assert!(text.contains("&scope=https%3A%2F%2Foutlook.office.com%2FIMAP.AccessAsUser.All%20"));
}

#[test]
fn identity_of_reads_both_claims() {
    let t = jwt(r#"{"email":"A@Example.org","preferred_username":"a@envio.test","sub":"x"}"#);
    assert_eq!(
        identity_of(&t),
        Some(id(Some("A@Example.org"), Some("a@envio.test")))
    );
}

#[test]
fn identity_of_reads_only_preferred_username() {
    let t = jwt(r#"{"preferred_username":"a@envio.test"}"#);
    assert_eq!(identity_of(&t), Some(id(None, Some("a@envio.test"))));
}

#[test]
fn identity_of_survives_garbage() {
    for t in [
        "",
        "x",
        "a.b.c",
        "a..c",
        "a.!!!.c",
        &jwt("not json"),
        &jwt("[1]"),
    ] {
        assert_eq!(identity_of(t), None, "{t}");
    }
    assert_eq!(
        identity_of(&jwt(r#"{"email":"","preferred_username":"  "}"#)),
        None
    );
    assert_eq!(identity_of(&jwt(r#"{"email":5}"#)), None);
}

#[test]
fn matches_the_email_claim() {
    assert!(identity_matches(
        "a@example.org",
        &id(Some("a@example.org"), None)
    ));
}

#[test]
fn matches_the_preferred_username_ignoring_case_and_spaces() {
    assert!(identity_matches(
        "  A@Envio.TEST ",
        &id(None, Some("a@envio.test"))
    ));
    assert!(identity_matches(
        "a@envio.test",
        &id(Some("b@example.org"), Some("A@ENVIO.test"))
    ));
}

#[test]
fn a_differing_upn_still_matches_on_the_email_claim() {
    let claims = id(Some("a@example.org"), Some("a@tenant.envio.test"));
    assert!(identity_matches("a@example.org", &claims));
}

#[test]
fn no_match_cases() {
    assert!(!identity_matches(
        "a@example.org",
        &id(Some("b@example.org"), Some("c@example.org"))
    ));
    assert!(!identity_matches("a@example.org", &id(None, None)));
    assert!(!identity_matches(
        "a@example.org",
        &id(Some("aa@example.org"), None)
    ));
    assert!(!identity_matches(
        "a@example.org",
        &id(Some("a@example.org.x"), None)
    ));
    assert!(!identity_matches(
        "example.org",
        &id(Some("a@example.org"), None)
    ));
    assert!(!identity_matches("", &id(Some(""), None)));
}

#[test]
fn a_2xx_body_that_is_not_token_json_is_temporary() {
    let bodies: [&[u8]; 4] = [
        b"<html>Sign in to the network</html>",
        b"",
        br#"{"access_token":"AT","expires_in":"#,
        b"null",
    ];
    for status in [200, 204, 299] {
        for body in bodies {
            assert_eq!(
                parse_token_reply(status, body).err(),
                Some(TokenError::Temporary),
                "{status} {body:?}"
            );
        }
    }
}

#[test]
fn json_that_lacks_a_token_or_a_non_2xx_page_is_still_refused() {
    assert_eq!(
        parse_token_reply(200, b"{}").err(),
        Some(TokenError::Refused("no_access_token".to_owned()))
    );
    for s in [302, 400, 401, 403] {
        assert_eq!(
            parse_token_reply(s, b"<html>nope</html>").err(),
            Some(TokenError::Refused("unreadable reply".to_owned())),
            "{s}"
        );
    }
}
