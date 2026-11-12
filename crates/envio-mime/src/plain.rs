//! Plain text to Qt rich text: format=flowed, quotes, links, escaping.

use crate::preview::is_signature_line;

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
        let soft = body.ends_with(' ') && !is_signature_line(body);
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

/// format=flowed text joined into whole lines for quoting: soft breaks are
/// removed per [`unflow`], quote depth prefixes (`> `) are put back, every
/// line ends in `\n`.
pub(crate) fn unflow_text(text: &str, delsp: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for line in unflow(text, delsp) {
        for _ in 0..line.depth {
            out.push('>');
        }
        if line.depth > 0 && !line.text.is_empty() {
            out.push(' ');
        }
        out.push_str(&line.text);
        out.push('\n');
    }
    out
}

/// Escapes `line` into `out`, turning addresses into links. The link text
/// is the address exactly as written; `www.` links get `https://`.
fn linkify(line: &str, out: &mut String) {
    let lower = line.to_ascii_lowercase();
    let mut finder = Finder::new(&lower);
    let mut pos = 0;
    while let Some((start, end)) = finder.next_link(line, pos) {
        escape_into(&line[pos..start], out);
        let url = &line[start..end];
        let href = if lower[start..].starts_with("www.") {
            format!("https://{url}")
        } else {
            url.to_owned()
        };
        if let Some(href) = crate::link::safe_href(&href) {
            out.push_str("<a href=\"");
            escape_into(&href, out);
            out.push_str("\">");
            escape_into(url, out);
            out.push_str("</a>");
        } else {
            escape_into(url, out);
        }
        pos = end;
    }
    escape_into(&line[pos..], out);
}

/// Finds scheme candidates in one pass over the line: each scheme's next
/// position is cached and searched again only once the scan has passed it.
struct Finder<'a> {
    lower: &'a str,
    /// `None` once a scheme does not occur again.
    next: [Option<usize>; 4],
}

impl<'a> Finder<'a> {
    fn new(lower: &'a str) -> Self {
        Self {
            lower,
            next: SCHEMES.map(|s| lower.find(s)),
        }
    }

    /// The first scheme at or after `from`, and its position.
    fn candidate(&mut self, from: usize) -> Option<(usize, &'static str)> {
        for (next, scheme) in self.next.iter_mut().zip(SCHEMES) {
            if next.is_some_and(|i| i < from) {
                *next = self.lower[from..].find(scheme).map(|i| i + from);
            }
        }
        self.next
            .iter()
            .zip(SCHEMES)
            .filter_map(|(next, scheme)| next.map(|i| (i, scheme)))
            .min_by_key(|(i, _)| *i)
    }

    /// The byte range of the next link at or after `from`. A link starts at
    /// a word boundary and needs at least one character after its scheme.
    fn next_link(&mut self, line: &str, from: usize) -> Option<(usize, usize)> {
        let mut search = from;
        loop {
            let (start, scheme) = self.candidate(search)?;
            let at_boundary = line[..start]
                .chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || "(<[\"'".contains(c));
            if at_boundary {
                let end = url_end(line, start);
                if end > start + scheme.len() {
                    return Some((start, end));
                }
            }
            // Schemes are ASCII, so the next byte is a char boundary.
            search = start + 1;
        }
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
    // Trimming never removes a `(`, so look for one once.
    let has_open = url.contains('(');
    loop {
        let mut trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '\'']);
        if trimmed.ends_with(')') && !has_open {
            trimmed = &trimmed[..trimmed.len() - 1];
        }
        if trimmed.len() == url.len() {
            break;
        }
        url = trimmed;
    }
    start + url.len()
}
