//! Graph's sendMail reads the recipients from the MIME headers, there is no
//! separate envelope. A Bcc recipient (in the envelope but in neither To nor
//! Cc) therefore goes into a `Bcc:` header of the copy that is sent. The
//! stored draft keeps no such header, so this runs only at send time.

/// The bare, lower-cased addresses of an address list (`Name <a@b>, c@d`).
/// Commas inside quotes or angle brackets do not split.
pub fn addresses(list: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut quoted, mut angle, mut start) = (false, false, 0);
    for (i, c) in list.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ',' if !quoted && !angle => {
                parts.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts
        .into_iter()
        .filter_map(|p| {
            let bare = match (p.rfind('<'), p.rfind('>')) {
                (Some(a), Some(b)) if a < b => &p[a + 1..b],
                _ => p,
            };
            let bare = bare.trim().to_ascii_lowercase();
            (!bare.is_empty()).then_some(bare)
        })
        .collect()
}

/// `eml` with a `Bcc:` header naming the envelope recipients that To and Cc
/// do not. An existing `Bcc:` header is dropped first. With no such
/// recipient the bytes come back unchanged.
pub fn with_bcc(eml: &[u8], envelope: &[String]) -> Vec<u8> {
    let (head, rest) = split(eml);
    let nl: &[u8] = if head.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    let text = String::from_utf8_lossy(head);
    let mut kept = String::new();
    let mut known = Vec::new();
    // The header the current line belongs to (a folded line continues it).
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        let mut value = line;
        if !line.starts_with([' ', '\t']) {
            current = line.split(':').next().unwrap_or("").to_ascii_lowercase();
            value = line.get(current.len() + 1..).unwrap_or("");
        }
        if current == "to" || current == "cc" {
            known.extend(addresses(value));
        }
        if current != "bcc" {
            kept.push_str(line);
        }
    }
    let mut bcc: Vec<String> = Vec::new();
    for a in envelope.iter().flat_map(|e| addresses(e)) {
        if !known.contains(&a) && !bcc.contains(&a) {
            bcc.push(a);
        }
    }
    if bcc.is_empty() {
        return eml.to_vec();
    }
    let mut out = kept.into_bytes();
    if !out.ends_with(b"\n") && !out.is_empty() {
        out.extend_from_slice(nl);
    }
    out.extend_from_slice(&fold(&bcc, nl));
    out.extend_from_slice(rest);
    out
}

/// The header block and the rest (the blank line and the body).
fn split(eml: &[u8]) -> (&[u8], &[u8]) {
    for (i, w) in eml.windows(2).enumerate() {
        if w == b"\n\n" {
            return (&eml[..=i], &eml[i + 1..]);
        }
        if w == b"\n\r" && eml.get(i + 2) == Some(&b'\n') {
            return (&eml[..=i], &eml[i + 1..]);
        }
    }
    (eml, &[])
}

/// `Bcc: a, b, ...` folded below 78 columns, each line ending with `nl`.
fn fold(addrs: &[String], nl: &[u8]) -> Vec<u8> {
    let mut out = String::from("Bcc:");
    let mut width = out.len();
    for (i, a) in addrs.iter().enumerate() {
        let item = if i + 1 < addrs.len() {
            format!("{a},")
        } else {
            a.clone()
        };
        if width + 1 + item.len() > 76 && width > 4 {
            out.push_str(&String::from_utf8_lossy(nl));
            out.push(' ');
            width = 1;
        } else {
            out.push(' ');
            width += 1;
        }
        out.push_str(&item);
        width += item.len();
    }
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(nl);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn run(eml: &str, env: &[&str]) -> String {
        String::from_utf8(with_bcc(eml.as_bytes(), &s(env))).unwrap()
    }

    const MSG: &str = "From: me@example.org\r\nTo: a@example.org\r\nCc: b@example.org\r\nSubject: hi\r\n\r\nbody\r\n";

    #[test]
    fn only_recipients_outside_to_and_cc_become_bcc() {
        let out = run(MSG, &["a@example.org", "b@example.org", "c@example.org"]);
        assert!(out.contains("Subject: hi\r\nBcc: c@example.org\r\n\r\nbody\r\n"));
    }

    #[test]
    fn no_bcc_recipient_leaves_the_bytes_alone() {
        assert_eq!(run(MSG, &["a@example.org", "b@example.org"]), MSG);
        assert_eq!(run(MSG, &[]), MSG);
    }

    #[test]
    fn comparison_ignores_case_and_display_names() {
        let eml = "To: Ann Lee <A@Example.org>, \"Doe, Jo\" <jo@example.org>\r\n\r\nx";
        assert_eq!(run(eml, &["a@example.org", "JO@example.org"]), eml);
        let out = run(eml, &["Bob <bob@example.org>"]);
        assert!(out.contains("Bcc: bob@example.org\r\n"));
    }

    #[test]
    fn an_existing_bcc_is_replaced() {
        let eml = "To: a@example.org\r\nBcc: old@example.org,\r\n more@example.org\r\nSubject: s\r\n\r\nx";
        let out = run(eml, &["a@example.org", "new@example.org"]);
        assert!(!out.contains("old@example.org") && !out.contains("more@example.org"));
        assert!(out.contains("Bcc: new@example.org\r\n\r\nx"));
        assert_eq!(out.matches("Bcc:").count(), 1);
    }

    #[test]
    fn a_long_list_is_folded() {
        let env: Vec<String> = (0..12).map(|i| format!("person{i}@example.org")).collect();
        let out = String::from_utf8(with_bcc(MSG.as_bytes(), &env)).unwrap();
        let block = out.split("\r\n\r\n").next().unwrap();
        let bcc: Vec<&str> = block
            .split("\r\n")
            .skip_while(|l| !l.starts_with("Bcc:"))
            .collect();
        assert!(bcc.len() > 1);
        assert!(bcc.iter().all(|l| l.len() <= 78));
        assert!(bcc[1..].iter().all(|l| l.starts_with(' ')));
        assert_eq!(addresses(&bcc.join(" ")[4..]).len(), 12);
    }

    #[test]
    fn headers_only_and_bare_lf_mail_work() {
        let out = run("To: a@example.org\nSubject: s", &["z@example.org"]);
        assert_eq!(out, "To: a@example.org\nSubject: s\nBcc: z@example.org\n");
        let lf = run("To: a@example.org\n\nbody", &["z@example.org"]);
        assert_eq!(lf, "To: a@example.org\nBcc: z@example.org\n\nbody");
    }

    #[test]
    fn a_folded_from_does_not_hide_a_self_bcc() {
        let eml = "From: =?utf-8?B?TcOp?=\r\n <me@example.org>\r\nTo: a@example.org\r\n\r\nx";
        let out = run(eml, &["a@example.org", "me@example.org"]);
        assert!(out.contains("Bcc: me@example.org\r\n\r\nx"), "{out}");
    }

    #[test]
    fn a_folded_to_or_cc_still_counts() {
        let eml = "To: a@example.org,\r\n b@example.org\r\nCc: c@example.org,\r\n\td@example.org\r\n\r\nx";
        let all = [
            "a@example.org",
            "b@example.org",
            "c@example.org",
            "d@example.org",
        ];
        assert_eq!(run(eml, &all), eml);
    }

    #[test]
    fn the_input_is_not_modified() {
        let eml = MSG.as_bytes().to_vec();
        let _ = with_bcc(&eml, &s(&["c@example.org"]));
        assert_eq!(eml, MSG.as_bytes());
    }
}
