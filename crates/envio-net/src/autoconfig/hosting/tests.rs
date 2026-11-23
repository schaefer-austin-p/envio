use super::*;

fn mx(preference: u16, host: &str) -> MxRecord {
    MxRecord {
        preference,
        host: host.to_owned(),
    }
}

fn hosting(records: &[(u16, &str)]) -> Hosting {
    let records: Vec<_> = records.iter().map(|(p, h)| mx(*p, h)).collect();
    hosting_of(&records)
}

const M365: &str = "contoso-com.mail.protection.outlook.com";

#[test]
fn exchange_online_hosts_count() {
    assert_eq!(hosting(&[(0, M365)]), Hosting::Microsoft365);
    assert_eq!(
        hosting(&[(0, "contoso-com.mx.microsoft")]),
        Hosting::Microsoft365
    );
    assert_eq!(
        hosting(&[(0, "contoso-com.o-v1.mx.microsoft")]),
        Hosting::Microsoft365
    );
}

#[test]
fn case_and_the_trailing_dot_do_not_matter() {
    assert_eq!(
        hosting(&[(0, "Contoso-Com.Mail.Protection.Outlook.COM.")]),
        Hosting::Microsoft365
    );
}

#[test]
fn only_the_most_preferred_records_count() {
    assert_eq!(
        hosting(&[(0, M365), (0, "b-com.mail.protection.outlook.com")]),
        Hosting::Microsoft365
    );
    assert_eq!(
        hosting(&[(10, M365), (20, "mx.mimecast.com")]),
        Hosting::Microsoft365
    );
    assert_eq!(
        hosting(&[(10, "mx.mimecast.com"), (20, M365)]),
        Hosting::Unknown
    );
    assert_eq!(
        hosting(&[(10, M365), (10, "mx.mimecast.com")]),
        Hosting::Unknown
    );
}

#[test]
fn nothing_to_go_on_is_unknown() {
    assert_eq!(hosting(&[]), Hosting::Unknown);
    assert_eq!(hosting(&[(0, ".")]), Hosting::Unknown);
}

#[test]
fn the_host_must_be_the_right_depth_under_the_suffix() {
    for host in [
        "x.y.mail.protection.outlook.com",
        "mail.protection.outlook.com",
        ".mail.protection.outlook.com",
        "evil-mail.protection.outlook.com.example.org",
        "contoso-com.mx.microsoft.example.org",
        "mx.microsoft",
        "a.b.c.mx.microsoft",
    ] {
        assert_eq!(hosting(&[(0, host)]), Hosting::Unknown, "{host}");
    }
}

#[test]
fn consumer_and_sovereign_clouds_are_not_matched() {
    for host in [
        "contoso-com.olc.protection.outlook.com",
        "contoso-com.mail.protection.outlook.us",
        "contoso-com.mail.protection.office365.us",
        "contoso-com.mail.protection.partner.outlook.cn",
    ] {
        assert_eq!(hosting(&[(0, host)]), Hosting::Unknown, "{host}");
    }
}

#[test]
fn google_mx_hosts_count() {
    for host in [
        "aspmx.l.google.com",
        "alt1.aspmx.l.google.com",
        "ALT4.aspmx.l.google.com.",
        "aspmx.googlemail.com",
        "aspmx2.googlemail.com",
        "aspmx5.googlemail.com",
        "smtp.google.com",
    ] {
        assert_eq!(hosting(&[(1, host)]), Hosting::Google, "{host}");
    }
}

#[test]
fn a_google_set_with_alternates_is_google() {
    assert_eq!(
        hosting(&[
            (1, "aspmx.l.google.com"),
            (5, "alt1.aspmx.l.google.com"),
            (10, "alt3.aspmx.l.google.com"),
        ]),
        Hosting::Google
    );
    assert_eq!(
        hosting(&[
            (1, "aspmx.l.google.com"),
            (1, "alt1.aspmx.l.google.com"),
            (1, "aspmx2.googlemail.com"),
        ]),
        Hosting::Google
    );
}

#[test]
fn google_must_hold_for_every_best_record() {
    assert_eq!(
        hosting(&[(1, "aspmx.l.google.com"), (1, "mx.example.org")]),
        Hosting::Unknown
    );
    assert_eq!(
        hosting(&[(10, "mx.example.org"), (20, "aspmx.l.google.com")]),
        Hosting::Unknown
    );
    assert_eq!(
        hosting(&[(1, "aspmx.l.google.com"), (1, M365)]),
        Hosting::Unknown
    );
}

#[test]
fn lookalike_google_hosts_are_not_matched() {
    for host in [
        "google.com",
        "aspmx.l.google.com.example.org",
        "evil-aspmx.l.google.com",
        "alt.aspmx.l.google.com",
        "altx.aspmx.l.google.com",
        "x.alt1.aspmx.l.google.com",
        "aspmx12.googlemail.com",
        "aspmxa.googlemail.com",
        "mail.googlemail.com",
        "aspmx.googlemail.com.example.org",
        "mail.smtp.google.com",
    ] {
        assert_eq!(hosting(&[(1, host)]), Hosting::Unknown, "{host}");
    }
}
