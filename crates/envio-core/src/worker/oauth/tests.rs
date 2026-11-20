use super::*;

fn id(email: Option<&str>, upn: Option<&str>) -> Identity {
    Identity {
        email: email.map(str::to_owned),
        preferred_username: upn.map(str::to_owned),
    }
}

#[test]
fn a_typed_address_equal_to_either_claim_is_a_match() {
    let both = id(Some("ada@outlook.example"), Some("ada@envio.test"));
    assert_eq!(judge("ada@outlook.example", Some(&both)), Verdict::Match);
    assert_eq!(judge(" ADA@envio.test ", Some(&both)), Verdict::Match);
}

#[test]
fn anything_else_asks_and_names_the_email_claim_first() {
    let both = id(Some("ada@outlook.example"), Some("ada@envio.test"));
    assert_eq!(
        judge("bob@outlook.example", Some(&both)),
        Verdict::Ask(Some("ada@outlook.example".into()))
    );
    let upn = id(None, Some("ada@envio.test"));
    assert_eq!(
        judge("bob@outlook.example", Some(&upn)),
        Verdict::Ask(Some("ada@envio.test".into()))
    );
    assert_eq!(judge("bob@outlook.example", None), Verdict::Ask(None));
    assert_eq!(
        judge("bob@outlook.example", Some(&id(None, None))),
        Verdict::Ask(None)
    );
}

#[test]
fn token_errors_become_the_failure_the_wizard_shows() {
    assert_eq!(failure_of(&TokenError::Temporary), OAuthFailure::Offline);
    assert_eq!(
        failure_of(&TokenError::Refused("x".into())),
        OAuthFailure::Blocked
    );
    assert_eq!(
        failure_of(&TokenError::Revoked("invalid_grant".into())),
        OAuthFailure::Other
    );
}
