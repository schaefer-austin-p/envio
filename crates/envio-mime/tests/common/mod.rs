//! An independent model of the font size Qt 5 paints each element of the
//! sanitised HTML at, for the font-size tests. Written from Qt's source and
//! checked against a real `QTextDocument` (default font 16px), not from
//! `fontsize.rs`. It assumes the worst: relative sizes (`%`, `em`) count as
//! CSS says, although Qt 5.15 ignores them.
//!
//! - `h1`..`h5`, `big`, `small`, `<font size>` and the keywords `small` to
//!   `xx-large` set an adjustment: `round(scale * 16)` with scale 0.7, 0.8,
//!   1, 1.2, 1.5, 2, 2.4 for -2..=4 (qtextformat.cpp). It is inherited,
//!   and beats `px` and `pt` on the same element and below.
//! - `sub` and `sup` are painted at `size * 2 / 3`, integer division
//!   (qtextengine.cpp), also for what is inside them.

pub const MAX_PX: i64 = 26;
pub const MIN_PX: i64 = 1;

#[derive(Clone, Copy)]
struct State {
    adjustment: Option<i32>,
    /// Computed size when no adjustment applies.
    px: f64,
    scripted: bool,
}

impl State {
    /// The size, in whole px, before the `sub`/`sup` scaling.
    fn size(self) -> i64 {
        match self.adjustment {
            Some(a) => {
                let scale = [0.7, 0.8, 1.0, 1.2, 1.5, 2.0, 2.4][(a + 2).clamp(0, 6) as usize];
                (scale * 16.0_f64).round() as i64
            }
            None => self.px.round() as i64,
        }
    }

    fn painted(self) -> i64 {
        if self.scripted {
            self.size() * 2 / 3
        } else {
            self.size()
        }
    }
}

fn tag_adjustment(name: &str) -> Option<i32> {
    Some(match name {
        "h1" => 3,
        "h2" => 2,
        "h3" | "big" => 1,
        "h4" => 0,
        "h5" | "small" => -1,
        _ => return None,
    })
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let len = tag[at..].find('"')?;
    Some(&tag[at..at + len])
}

fn declaration(state: &mut State, parent: State, value: &str) {
    let value = value
        .split('!')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let value = value.strip_prefix('+').unwrap_or(&value);
    match value {
        "small" => state.adjustment = Some(-1),
        "medium" => state.adjustment = Some(0),
        "large" => state.adjustment = Some(1),
        "x-large" => state.adjustment = Some(2),
        "xx-large" => state.adjustment = Some(3),
        "xxx-large" => state.px = 48.0,
        "larger" => state.px = (parent.size() as f64) * 1.2,
        "smaller" => state.px = (parent.size() as f64) / 1.2,
        "x-small" => state.px = 10.0,
        "xx-small" => state.px = 9.0,
        _ => {
            let end = value
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(value.len());
            let Ok(n) = value[..end].parse::<f64>() else {
                return;
            };
            match &value[end..] {
                "px" => state.px = n,
                "pt" => state.px = n * 4.0 / 3.0,
                "rem" => state.px = n * 16.0,
                "em" => state.px = n * (parent.size() as f64),
                "%" => state.px = n * (parent.size() as f64) / 100.0,
                _ => {}
            }
        }
    }
}

/// The painted size of every element in `html`, with its tag.
pub fn painted_sizes(html: &str) -> Vec<(String, i64)> {
    const VOID: [&str; 6] = ["br", "img", "wbr", "hr", "col", "input"];
    let mut open: Vec<(String, State)> = Vec::new();
    let mut sizes = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let raw = &rest[start..start + len + 1];
        rest = &rest[start + len + 1..];
        if let Some(name) = raw.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim().to_lowercase();
            if let Some(at) = open.iter().rposition(|(n, _)| *n == name) {
                open.truncate(at);
            }
            continue;
        }
        let name: String = raw[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if name.is_empty() || raw.starts_with("<!") {
            continue;
        }
        let parent = open.last().map_or(
            State {
                adjustment: None,
                px: 16.0,
                scripted: false,
            },
            |(_, s)| *s,
        );
        let mut state = parent;
        if let Some(a) = tag_adjustment(&name) {
            state.adjustment = Some(a);
        }
        if matches!(name.as_str(), "sub" | "sup") {
            state.scripted = true;
        }
        if name == "font"
            && let Some(size) = attr(raw, "size")
            && let Ok(n) = size.trim_start_matches('+').parse::<i64>()
        {
            let n = if size.starts_with('+') || size.starts_with('-') {
                n.clamp(-9_999, 9_999)
            } else {
                n - 3
            };
            state.adjustment = Some(n as i32);
        }
        if let Some(style) = attr(raw, "style") {
            for d in style.split(';') {
                if let Some((prop, value)) = d.split_once(':')
                    && prop.trim().eq_ignore_ascii_case("font-size")
                {
                    declaration(&mut state, parent, value);
                }
            }
        }
        sizes.push((name.clone(), state.painted()));
        if !VOID.contains(&name.as_str()) {
            open.push((name, state));
        }
    }
    sizes
}

/// The elements of `html` painted over [`MAX_PX`] or under [`MIN_PX`].
pub fn out_of_range(html: &str) -> Vec<(String, i64)> {
    painted_sizes(html)
        .into_iter()
        .filter(|(_, px)| !(MIN_PX..=MAX_PX).contains(px))
        .collect()
}
