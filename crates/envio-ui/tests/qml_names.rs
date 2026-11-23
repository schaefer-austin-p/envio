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

/// The double-quoted strings on a line.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

type Uses = Vec<(String, String)>;

/// The outcome names one (trimmed) line compares against or sends.
fn outcomes_on(t: &str) -> Vec<&str> {
    let mut v = Vec::new();
    if t.contains("outcome ===")
        || t.contains("outcome !==")
        || t.contains(".indexOf(outcome)")
        || t.contains("const outcome of")
    {
        v.extend(quoted(t));
    }
    // oauthDone(email, "outcome", text): the second argument.
    if let Some(rest) = t.split("oauthDone(").nth(1)
        && let Some(s) = rest
            .split(',')
            .nth(1)
            .and_then(|a| quoted(a).first().copied())
    {
        v.push(s);
    }
    v
}

/// The kind strings one (trimmed) line passes or sets.
fn kinds_on(t: &str, in_session: bool) -> Vec<&str> {
    let mut v = Vec::new();
    if let Some(rest) = t.split("oauthKind =").nth(1) {
        // `oauthKind = id === "x" ? "graph" : "imap"`: only the values.
        v.extend(quoted(rest.split('?').next_back().unwrap_or(rest)));
    }
    if t.contains("startOAuth")
        && let Some(rest) = t.split("args[2],").nth(1)
    {
        v.extend(quoted(rest));
    }
    if in_session && t.starts_with("kind: \"") {
        v.extend(quoted(t));
    }
    v
}

/// What QML and its tests use as an outcome name and as a kind.
fn used_names() -> (Uses, Uses) {
    let mut all = Vec::new();
    files(&root().join("qml"), &mut all);
    files(&root().join("tests/qml"), &mut all);
    let (mut outcomes, mut kinds) = (Uses::new(), Uses::new());
    for path in all {
        let text = fs::read_to_string(&path).unwrap();
        // Indent of the open `OAuthSession {` block, if inside one.
        let mut session: Option<usize> = None;
        let at = path.display().to_string();
        for line in text.lines() {
            let t = line.trim();
            let indent = line.len() - line.trim_start().len();
            if session.is_some_and(|i| !t.is_empty() && indent <= i) {
                session = None;
            }
            if t.starts_with("OAuthSession {") {
                session = Some(indent);
            }
            if t.starts_with("//") {
                continue;
            }
            let in_session = session.is_some_and(|i| indent == i + 4);
            for s in outcomes_on(t) {
                outcomes.push((at.clone(), s.to_string()));
            }
            for s in kinds_on(t, in_session) {
                kinds.push((at.clone(), s.to_string()));
            }
        }
    }
    (outcomes, kinds)
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
