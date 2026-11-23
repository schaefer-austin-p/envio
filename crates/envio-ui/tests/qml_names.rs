//! The outcome and kind names cross into QML as plain strings. These tests
//! pin them to `envio_ui`'s table and check that the QML files and the QML
//! tests only use names from it (#426).

use std::fs;
use std::path::{Path, PathBuf};

use envio_core::{OAuthFailure, OAuthOutcome, ProviderKind};
use envio_ui::{KIND_NAMES, OUTCOME_NAMES, kind_name, outcome_name, parse_kind};

const FAILURES: [OAuthFailure; 12] = [
    OAuthFailure::Timeout,
    OAuthFailure::Offline,
    OAuthFailure::Blocked,
    OAuthFailure::Other,
    OAuthFailure::Duplicate,
    OAuthFailure::NotOAuth,
    OAuthFailure::Unavailable,
    OAuthFailure::NotSaved,
    OAuthFailure::Admin,
    OAuthFailure::Mismatch,
    OAuthFailure::NoTenant,
    OAuthFailure::NotPersonal,
];

#[test]
fn every_outcome_has_a_name_in_the_table_and_the_table_has_no_spare() {
    let mut all = vec![
        OAuthOutcome::Added,
        OAuthOutcome::Resumed,
        OAuthOutcome::Updated,
        OAuthOutcome::Cancelled,
    ];
    all.extend(FAILURES.map(OAuthOutcome::Failed));
    let mut used: Vec<&str> = Vec::new();
    for o in &all {
        let name = outcome_name(o);
        assert!(OUTCOME_NAMES.contains(&name), "{o:?} -> {name}");
        used.push(name);
    }
    for name in OUTCOME_NAMES {
        assert!(used.contains(&name), "{name} is named by no outcome");
    }
}

#[test]
fn kinds_round_trip() {
    for kind in [ProviderKind::Microsoft, ProviderKind::MicrosoftGraph] {
        let name = kind_name(kind);
        assert!(KIND_NAMES.contains(&name));
        assert_eq!(parse_kind(name), Some(kind));
    }
    for name in KIND_NAMES {
        assert!(parse_kind(name).is_some(), "{name}");
    }
    assert_eq!(parse_kind(""), None);
    assert_eq!(parse_kind("grpah"), None);
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "bindle") {
                continue;
            }
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "qml" || x == "js") {
            out.push(p);
        }
    }
}

type Uses = Vec<(String, String)>;

/// The string literal that starts `s` (after spaces), if any.
fn string_after(s: &str) -> Option<&str> {
    let rest = s.trim_start().strip_prefix('"')?;
    rest.split('"').next()
}

/// The string literal that ends `s` (before spaces), if any.
fn string_before(s: &str) -> Option<&str> {
    let rest = s.trim_end().strip_suffix('"')?;
    rest.rsplit('"').next()
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// `outcome` as a whole word at the end of `s`.
fn ends_with_outcome(s: &str) -> bool {
    s.trim_end()
        .strip_suffix("outcome")
        .is_some_and(|before| !before.ends_with(is_ident))
}

/// `outcome` as a whole word at the start of `s`.
fn starts_with_outcome(s: &str) -> bool {
    s.trim_start()
        .strip_prefix("outcome")
        .is_some_and(|after| !after.starts_with(is_ident))
}

/// The strings of a `[...]` list in `s` (up to the first `]`).
fn list_strings(s: &str) -> Vec<&str> {
    let inner = s.split(']').next().unwrap_or(s);
    let inner = inner.rsplit('[').next().unwrap_or(inner);
    inner.split(',').filter_map(string_after).collect()
}

/// The outcome names one (trimmed) line compares against or sends: the
/// string next to `outcome` with `===`, `!==`, `==` or `!=` (either side),
/// the list of `[...].indexOf(outcome)` and `for (const outcome of [...])`,
/// and the second argument of `oauthDone(...)`.
fn outcomes_on(t: &str) -> Vec<&str> {
    let mut v = Vec::new();
    // "===" and "!==" are read at their "==" and "!=": one position each.
    for (at, op) in t.match_indices("==").chain(t.match_indices("!=")) {
        if op == "!=" && t[at + 2..].starts_with('=') {
            continue; // the "!==" is read at its "=="
        }
        let left = &t[..at];
        let right = &t[at + op.len()..];
        let right = right.strip_prefix('=').unwrap_or(right);
        let left = left.strip_suffix('!').unwrap_or(left);
        if ends_with_outcome(left) {
            v.extend(string_after(right));
        }
        if starts_with_outcome(right) {
            v.extend(string_before(left));
        }
    }
    if let Some((before, _)) = t.split_once(".indexOf(outcome)") {
        v.extend(list_strings(before.rsplit_once('[').map_or("", |(_, l)| l)));
    }
    if let Some((_, after)) = t.split_once("const outcome of ") {
        v.extend(list_strings(after));
    }
    // oauthDone(email, "outcome", text): the second argument.
    if let Some(rest) = t.split("oauthDone(").nth(1) {
        v.extend(rest.split(',').nth(1).and_then(string_after));
    }
    v
}

/// The kind strings one (trimmed) line passes or sets: `oauthKind = "x"` or
/// `oauthKind = cond ? "x" : "y"`, a `startOAuth` call's third argument in a
/// QML test, and `kind: "x"` set directly on an `OAuthSession` (`direct`).
fn kinds_on(t: &str, direct: bool) -> Vec<&str> {
    let mut v = Vec::new();
    if let Some(rest) = t.split("oauthKind =").nth(1) {
        match rest.rsplit_once('?') {
            Some((_, values)) => v.extend(values.split(':').filter_map(string_after)),
            None => v.extend(string_after(rest)),
        }
    }
    if t.contains("startOAuth")
        && let Some(rest) = t.split("args[2],").nth(1)
    {
        v.extend(string_after(rest));
    }
    if direct && let Some(rest) = t.strip_prefix("kind:") {
        v.extend(string_after(rest));
    }
    v
}

fn brace_change(t: &str) -> i32 {
    t.matches('{').count() as i32 - t.matches('}').count() as i32
}

/// The outcome names and kinds one QML file's text uses. An `OAuthSession`
/// block is found by counting braces, so `OAuthSession{` and any
/// indentation work.
fn scan(text: &str) -> (Vec<String>, Vec<String>) {
    let (mut outcomes, mut kinds) = (Vec::new(), Vec::new());
    let (mut depth, mut session) = (0, None::<i32>);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if session.is_none()
            && t.strip_prefix("OAuthSession")
                .is_some_and(|r| r.trim_start().starts_with('{'))
        {
            session = Some(depth);
        }
        let direct = session.is_some_and(|d| depth == d + 1);
        outcomes.extend(outcomes_on(t).into_iter().map(String::from));
        kinds.extend(kinds_on(t, direct).into_iter().map(String::from));
        depth += brace_change(t);
        if session.is_some_and(|d| depth <= d) {
            session = None;
        }
    }
    (outcomes, kinds)
}

/// What QML and its tests use as an outcome name and as a kind, with the
/// file of each.
fn used_names() -> (Uses, Uses) {
    let mut all = Vec::new();
    files(&root().join("qml"), &mut all);
    files(&root().join("tests/qml"), &mut all);
    let (mut outcomes, mut kinds) = (Uses::new(), Uses::new());
    for path in all {
        let (o, k) = scan(&fs::read_to_string(&path).unwrap());
        let at = path.display().to_string();
        outcomes.extend(o.into_iter().map(|s| (at.clone(), s)));
        kinds.extend(k.into_iter().map(|s| (at.clone(), s)));
    }
    (outcomes, kinds)
}

#[test]
fn outcomes_are_read_next_to_the_operator_only() {
    assert_eq!(
        outcomes_on(r#"if (outcome === "cancelled") return { "kind": "info" }"#),
        ["cancelled"]
    );
    assert_eq!(outcomes_on(r#"if (outcome !== "admin") x()"#), ["admin"]);
    assert_eq!(outcomes_on(r#"if (outcome == "failed") x()"#), ["failed"]);
    assert_eq!(outcomes_on(r#"if (outcome != "work") x()"#), ["work"]);
    assert_eq!(
        outcomes_on(r#"if ("mismatch" === outcome) x()"#),
        ["mismatch"]
    );
    assert_eq!(outcomes_on(r#"if ("added" !== outcome) x()"#), ["added"]);
    assert_eq!(outcomes_on(r#"if ("resumed" == outcome) x()"#), ["resumed"]);
    assert_eq!(outcomes_on(r#"if ("updated" != outcome) x()"#), ["updated"]);
    assert_eq!(
        outcomes_on(r#"outcome === "admin" || outcome === "work""#),
        ["admin", "work"]
    );
    assert_eq!(
        outcomes_on(r#"if (["added", "admin"].indexOf(outcome) >= 0)"#),
        ["added", "admin"]
    );
    assert_eq!(
        outcomes_on(r#"for (const outcome of ["admin", "mismatch"]) {"#),
        ["admin", "mismatch"]
    );
    assert_eq!(
        outcomes_on(r#"core.oauthDone("a@b", "failed", "text")"#),
        ["failed"]
    );
    assert!(outcomes_on(r#"x === "other""#).is_empty());
    assert!(outcomes_on(r#"myoutcome === "other""#).is_empty());
}

#[test]
fn kinds_are_read_next_to_their_setter() {
    assert_eq!(kinds_on(r#"kind: "graph""#, true), ["graph"]);
    assert!(kinds_on(r#"kind: "warning""#, false).is_empty());
    assert_eq!(
        kinds_on(r#"oauthKind = id === "outlook" ? "graph" : "imap""#, false),
        ["graph", "imap"]
    );
    assert_eq!(kinds_on(r#"oauthKind = "imap""#, false), ["imap"]);
    assert_eq!(
        kinds_on(
            r#"compare(core.called("startOAuth")[0].args[2], "imap")"#,
            false
        ),
        ["imap"]
    );
}

#[test]
fn a_session_block_is_found_whatever_its_spacing_or_indent() {
    for opener in ["OAuthSession {", "OAuthSession{", "        OAuthSession  {"] {
        let text = format!(
            "Page {{\n{opener}\n  id: s\n\t\tkind: \"graph\"\n  onDeclined: function(o) {{\n kind: \"nested\"\n }}\n}}\nkind: \"after\"\n}}\n"
        );
        let (_, kinds) = scan(&text);
        assert_eq!(kinds, ["graph"], "{opener:?}");
    }
}

#[test]
fn qml_and_its_tests_use_only_pinned_outcome_names() {
    let (outcomes, _) = used_names();
    assert!(outcomes.len() >= 10, "scan found too little: {outcomes:?}");
    for (file, name) in &outcomes {
        assert!(
            OUTCOME_NAMES.contains(&name.as_str()),
            "{file} uses outcome \"{name}\", not in envio-ui's OUTCOME_NAMES"
        );
    }
}

/// Limit: only string literals are read. A kind or outcome built by an
/// expression other than the `cond ? "x" : "y"` form (a variable, a
/// concatenation) is not seen.
#[test]
fn qml_and_its_tests_use_only_pinned_kind_names() {
    let (_, kinds) = used_names();
    assert!(kinds.len() >= 4, "scan found too little: {kinds:?}");
    for (file, name) in &kinds {
        assert!(
            name.is_empty() || KIND_NAMES.contains(&name.as_str()),
            "{file} uses kind \"{name}\", not in envio-ui's KIND_NAMES"
        );
    }
    for want in KIND_NAMES {
        assert!(
            kinds.iter().any(|(_, k)| k == want),
            "no QML uses \"{want}\""
        );
    }
}
