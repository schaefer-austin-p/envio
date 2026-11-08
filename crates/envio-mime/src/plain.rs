//! Plain text to Qt rich text: format=flowed, quotes, links, escaping.

/// Wraps the output so Qt keeps runs of spaces and wraps long lines.
const BLOCK_STYLE: &str = "white-space:pre-wrap";
/// Deeper quote levels render at this depth.
const MAX_QUOTE_DEPTH: usize = 10;
const SCHEMES: [&str; 4] = ["https://", "http://", "mailto:", "www."];

/// One displayed line and its quote depth.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    depth: usize,
    text: String,
}

/// Plain text as Qt rich text. `flowed` and `delsp` are the RFC 3676
/// parameters of the text part; quote levels become nested blockquotes and
/// http, https, mailto and `www.` addresses become links.
pub fn plain_to_rich(text: &str, flowed: bool, delsp: bool) -> String {
    let lines = if flowed {
        unflow(text, delsp)
    } else {
        text.lines().map(fixed_line).collect()
    };
    let mut out = format!("<div style=\"{BLOCK_STYLE}\">");
    let mut depth = 0;
    let mut first = true;
    for line in &lines {
        if line.depth != depth {
            change_depth(&mut out, depth, line.depth);
            depth = line.depth;
            first = true;
        }
        if !first {
            out.push_str("<br>");
        }
        linkify(&line.text, &mut out);
        first = false;
    }
    change_depth(&mut out, depth, 0);
    out.push_str("</div>");
    out
}

/// HTML-escapes `text` into `out`.
pub(crate) fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn change_depth(out: &mut String, from: usize, to: usize) {
    for _ in to..from {
        out.push_str("</blockquote>");
    }
    for _ in from..to {
        out.push_str("<blockquote style=\"");
        out.push_str(BLOCK_STYLE);
        out.push_str("\">");
    }
}

/// Counts leading `>` marks (spaces between them allowed) and strips them
/// plus one following space.
fn split_quote(line: &str) -> (usize, &str) {
    let mut depth = 0;
    let mut rest = line;
    loop {
        if let Some(r) = rest.strip_prefix('>') {
            depth += 1;
            rest = r;
        } else if depth > 0 && rest.starts_with(" >") {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    let rest = if depth > 0 {
        rest.strip_prefix(' ').unwrap_or(rest)
    } else {
        rest
    };
    (depth.min(MAX_QUOTE_DEPTH), rest)
}

fn fixed_line(line: &str) -> Line {
    let (depth, text) = split_quote(line);
    Line {
        depth,
        text: text.to_owned(),
    }
}

/// RFC 3676: a line ending in a space continues on the next line of the
/// same quote depth. With delsp=yes that space is removed. A leading space
/// on an unquoted line is space-stuffing and is removed.
fn unflow(text: &str, delsp: bool) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut open = false;
    for raw in text.lines() {
        let (depth, body) = split_quote(raw);
        let body = if depth == 0 {
            body.strip_prefix(' ').unwrap_or(body)
        } else {
            body
        };
        let soft = body.ends_with(' ') && body != "-- ";
        let piece = if soft && delsp {
            &body[..body.len() - 1]
        } else {
            body
        };
        match out.last_mut() {
            Some(last) if open && last.depth == depth => last.text.push_str(piece),
            _ => out.push(Line {
                depth,
                text: piece.to_owned(),
            }),
        }
        open = soft;
    }
    out
}

/// Escapes `line` into `out`, turning addresses into links. The link text
/// is the address exactly as written; `www.` links get `https://`.
fn linkify(line: &str, out: &mut String) {
    let lower = line.to_ascii_lowercase();
    let mut pos = 0;
    while let Some((start, end)) = next_link(line, &lower, pos) {
        escape_into(&line[pos..start], out);
        let url = &line[start..end];
        let href = if lower[start..].starts_with("www.") {
            format!("https://{url}")
        } else {
            url.to_owned()
        };
        out.push_str("<a href=\"");
        escape_into(&href, out);
        out.push_str("\">");
        escape_into(url, out);
        out.push_str("</a>");
        pos = end;
    }
    escape_into(&line[pos..], out);
}

/// The byte range of the next link at or after `from`. A link starts at a
/// word boundary and needs at least one character after its scheme.
fn next_link(line: &str, lower: &str, from: usize) -> Option<(usize, usize)> {
    let mut search = from;
    loop {
        let (start, scheme) = SCHEMES
            .iter()
            .filter_map(|s| lower[search..].find(s).map(|i| (i + search, *s)))
            .min_by_key(|(i, _)| *i)?;
        let at_boundary = line[..start]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || "(<[\"'".contains(c));
        let end = url_end(line, start);
        if at_boundary && end > start + scheme.len() {
            return Some((start, end));
        }
        // Schemes are ASCII, so the next byte is a char boundary.
        search = start + 1;
    }
}

/// Where the address starting at `start` ends: at whitespace, `<`, `>` or
/// `"`, minus trailing punctuation and an unbalanced `)`.
fn url_end(line: &str, start: usize) -> usize {
    let rest = &line[start..];
    let len = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
        .unwrap_or(rest.len());
    let mut url = &rest[..len];
    loop {
        let mut trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'']);
        if trimmed.ends_with(')') && !trimmed.contains('(') {
            trimmed = &trimmed[..trimmed.len() - 1];
        }
        if trimmed.len() == url.len() {
            break;
        }
        url = trimmed;
    }
    start + url.len()
}
