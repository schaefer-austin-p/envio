use super::*;
use secrecy::ExposeSecret;

const URI: &str = "envio-oauth://callback";

fn provider() -> Provider {
    Provider::microsoft_template().with_client_id("cid-1")
}

fn query_of(url: &str) -> (String, Vec<(String, String)>) {
    let (base, q) = url.split_once('?').unwrap();
    let pairs = q
        .split('&')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (k.to_owned(), v.to_owned())
        })
        .collect();
    (base.to_owned(), pairs)
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    let hits: Vec<_> = pairs.iter().filter(|(k, _)| k == key).collect();
    assert_eq!(hits.len(), 1, "{key}");
    &hits[0].1
}

fn err(url: &str, state: &str) -> Option<RedirectError> {
    parse_redirect(url, state, URI).err()
}

#[test]
fn authorize_url_has_every_parameter_encoded() {
    let pkce = Pkce::from_parts("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "st4te");
    let url = authorize_url(&provider(), &pkce, "a+b@example.org");
    let (base, q) = query_of(&url);
    assert_eq!(
        base,
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
    );
    assert_eq!(q.len(), 8);
    assert_eq!(get(&q, "response_type"), "code");
    assert_eq!(get(&q, "client_id"), "cid-1");
    assert_eq!(get(&q, "redirect_uri"), "envio-oauth%3A%2F%2Fcallback");
    let scope = get(&q, "scope");
    assert!(scope.starts_with("https%3A%2F%2Foutlook.office.com%2FIMAP.AccessAsUser.All%20"));
    assert!(!scope.contains(' '));
    assert_eq!(get(&q, "state"), "st4te");
    assert_eq!(
        get(&q, "code_challenge"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert_eq!(get(&q, "code_challenge_method"), "S256");
    assert_eq!(get(&q, "login_hint"), "a%2Bb%40example.org");
}

#[test]
fn a_hint_cannot_inject_parameters() {
    let pkce = Pkce::from_parts("v", "s");
    let url = authorize_url(&provider(), &pkce, "x&state=evil#frag y");
    let (_, q) = query_of(&url);
    assert_eq!(get(&q, "state"), "s");
    assert_eq!(get(&q, "login_hint"), "x%26state%3Devil%23frag%20y");
    assert!(!url.contains('#'));
}

#[test]
fn a_good_redirect_parses() {
    let r = parse_redirect("envio-oauth://callback?code=ab%20c&state=xyz", "xyz", URI).unwrap();
    assert_eq!(r.code.expose_secret(), "ab c");
    assert_eq!(r.state, "xyz");
}

#[test]
fn a_wrong_state_is_a_mismatch() {
    for url in [
        "envio-oauth://callback?code=c&state=xyZ",
        "envio-oauth://callback?code=c&state=xy",
        "envio-oauth://callback?code=c&state=xyzz",
        "envio-oauth://callback?code=c",
    ] {
        assert_eq!(err(url, "xyz"), Some(RedirectError::Mismatch), "{url}");
    }
}

#[test]
fn a_wrong_scheme_or_host_is_malformed() {
    for url in [
        "envio-oauth://evil?code=c&state=s",
        "envio-oauth://callback.evil?code=c&state=s",
        "envio-oauth://callbackx?code=c&state=s",
        "other://callback?code=c&state=s",
        "https://callback?code=c&state=s",
        "envio-oauth://callback/path?code=c&state=s",
        "mailto:a@example.org",
        "",
    ] {
        assert_eq!(err(url, "s"), Some(RedirectError::Malformed), "{url}");
    }
}

#[test]
fn access_denied_is_denied() {
    let url =
        "envio-oauth://callback?error=access_denied&error_description=user%40example.org&state=s";
    assert_eq!(err(url, "s"), Some(RedirectError::Denied));
}

#[test]
fn another_error_keeps_only_the_code() {
    let url = "envio-oauth://callback?error=server_error&error_description=for%20user%40example.org&state=s";
    let e = err(url, "s").unwrap();
    assert_eq!(e, RedirectError::Server("server_error".to_owned()));
    assert!(!format!("{e:?}").contains("example.org"));
}

#[test]
fn a_missing_code_is_malformed() {
    for url in [
        "envio-oauth://callback?state=s",
        "envio-oauth://callback?code=&state=s",
        "envio-oauth://callback",
    ] {
        assert_eq!(err(url, "s"), Some(RedirectError::Malformed), "{url}");
    }
}

#[test]
fn repeated_parameters_are_malformed() {
    for url in [
        "envio-oauth://callback?code=a&code=b&state=s",
        "envio-oauth://callback?code=a&state=s&state=s",
    ] {
        assert_eq!(err(url, "s"), Some(RedirectError::Malformed), "{url}");
    }
}

#[test]
fn a_huge_query_is_malformed() {
    let url = format!("envio-oauth://callback?code={}&state=s", "a".repeat(20_000));
    assert_eq!(err(&url, "s"), Some(RedirectError::Malformed));
}

#[test]
fn debug_hides_the_code() {
    let r = parse_redirect("envio-oauth://callback?code=CODE-MARKER&state=s", "s", URI).unwrap();
    assert!(!format!("{r:?}").contains("CODE-MARKER"));
}
