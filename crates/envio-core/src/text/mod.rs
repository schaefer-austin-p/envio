//! User-facing core text. `tr`/`tr_n` are called only from this folder, so
//! xgettext reads only these files (see tools/i18n/update.sh).

pub mod compose;
pub mod count;
pub mod link;
pub mod notice;
pub mod outbox;
pub mod read;
pub mod setup;
pub mod status;
pub mod undo;

#[cfg(test)]
mod tests {
    // This file is scanned too, so the scanner spells its characters as
    // ASCII codes instead of char literals.
    const DOUBLE_QUOTE: char = 34 as char;
    const SINGLE_QUOTE: char = 39 as char;
    const BACKSLASH: char = 92 as char;
    const SLASH: char = 47 as char;
    const NEWLINE: char = 10 as char;

    /// Removes string literals and `//` comments so only code remains.
    fn code_only(src: &str) -> String {
        let mut out = String::new();
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            if c == DOUBLE_QUOTE {
                while let Some(d) = chars.next() {
                    if d == BACKSLASH {
                        chars.next();
                    } else if d == DOUBLE_QUOTE {
                        break;
                    }
                }
            } else if c == SLASH && chars.peek() == Some(&SLASH) {
                for d in chars.by_ref() {
                    if d == NEWLINE {
                        out.push(NEWLINE);
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    // xgettext's C scanner reads a single quote as a character literal and
    // loses the rest of the file, so text files use none (and no lifetimes).
    #[test]
    fn the_scanner_flags_code_quotes_but_not_quotes_in_text() {
        let q = SINGLE_QUOTE;
        let lifetime = format!("fn f(x: &{q}a str) {{}}");
        let char_literal = format!("let c = {q}x{q};");
        let in_string = format!("tr(\"don{q}t\") // it{q}s fine");
        assert!(code_only(&lifetime).contains(q));
        assert!(code_only(&char_literal).contains(q));
        assert!(!code_only(&in_string).contains(q));
    }

    #[test]
    fn text_files_have_no_char_literals_or_lifetimes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/text");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                let code = code_only(&std::fs::read_to_string(&path).unwrap());
                assert!(
                    !code.contains(SINGLE_QUOTE),
                    "{} has a single quote",
                    path.display()
                );
            }
        }
    }
}
