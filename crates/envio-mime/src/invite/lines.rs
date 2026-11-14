//! iCalendar content lines (RFC 5545 §3.1): unfold, then split into name,
//! parameters and value. Pure and lenient; nothing here panics.

/// One content line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Prop {
    /// Upper case.
    pub name: String,
    /// Parameter names upper case; values unquoted, as written.
    pub params: Vec<(String, String)>,
    /// Still text-escaped; see [`unescape`].
    pub value: String,
}

impl Prop {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Lines with CRLF or LF folds (a newline then one space or tab) removed.
/// `None` when there are more than `max_lines` physical lines.
pub(super) fn unfold(text: &str, max_lines: usize) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for (n, line) in text.split('\n').enumerate() {
        if n >= max_lines {
            return None;
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (line.strip_prefix([' ', '\t']), out.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => out.push(line.to_owned()),
        }
    }
    Some(out)
}

/// Splits `NAME;P1=a;P2="b;c":value`; `None` for a line without a colon
/// outside quotes or with an empty name.
pub(super) fn split(line: &str) -> Option<Prop> {
    let mut in_quotes = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_unquoted(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            // A multi-valued parameter keeps its first value.
            let first = split_unquoted(v.trim(), ',')
                .into_iter()
                .next()
                .unwrap_or("");
            Some((
                k.trim().to_ascii_uppercase(),
                first.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect();
    Some(Prop {
        name,
        params,
        value: value.to_owned(),
    })
}

fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_quotes = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == sep && !in_quotes {
            out.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

/// TEXT value: `\n \N \, \; \\` resolved, control characters dropped (a
/// newline stays one), at most `max_chars` characters.
pub(super) fn unescape(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if count >= max_chars {
            break;
        }
        let c = if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => '\n',
                Some(other) => other,
                None => break,
            }
        } else {
            c
        };
        if c == '\n' || !c.is_control() {
            out.push(c);
            count += 1;
        }
    }
    out
}
