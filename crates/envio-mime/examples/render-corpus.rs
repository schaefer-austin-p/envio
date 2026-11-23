//! Renders envio-mime's test corpus the way the reader gets it (remote
//! images off) into a directory, one `.html` file each, plus `index.txt`
//! naming them. The QML test (tests/qml/tst_rich_text.qml) loads the files
//! into a Qt rich-text `Text`. Usage:
//!
//!     cargo run -p envio-mime --example render-corpus -- <out dir>

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use envio_mime::{RenderOptions, parse, render, sanitize_html};

fn options() -> RenderOptions {
    RenderOptions {
        allow_remote_images: false,
        cid_urls: HashMap::new(),
        cid_sizes: HashMap::new(),
        image_sizes: HashMap::new(),
        blocked_label: "[image]".to_owned(),
        max_image_width: 340,
        max_font_px: 0,
    }
}

/// A newsletter's usual shape: layout tables, a banner and a tracking pixel
/// from other hosts, buttons as links.
const NEWSLETTER: &str = r#"<table width="600" cellpadding="0" cellspacing="0"><tr><td>
<img src="https://cdn.example.com/banner.png" width="600" height="120" alt="Autumn routes">
<h1 style="color:#0f5a47">Three new routes</h1>
<p>The coastal path reopened, and we mapped two loops inland.</p>
<table><tr><td><a href="https://news.example.com/r?u=1">Read the routes</a></td></tr></table>
<p><a href="https://news.example.com/unsubscribe">Unsubscribe</a></p>
<img src="https://track.example.com/o.gif" width="1" height="1">
</td></tr></table>"#;

fn nested(tag: &str, depth: usize) -> String {
    format!(
        "{}<p>level {depth}</p>{}",
        format!("<{tag}>").repeat(depth),
        format!("</{tag}>").repeat(depth)
    )
}

fn files_in(dir: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    found.sort();
    Ok(found)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn main() -> io::Result<()> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or_else(|| io::Error::other("usage: render-corpus <out dir>"))?,
    );
    fs::create_dir_all(&out)?;
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut written: Vec<String> = Vec::new();
    let mut write = |name: String, html: &str| -> io::Result<()> {
        fs::write(out.join(&name), html)?;
        written.push(name);
        Ok(())
    };

    for path in files_in(&tests.join("corpus"), "eml")? {
        let rendered = render(&parse(&fs::read(&path)?), &options());
        write(format!("corpus-{}.html", stem(&path)), &rendered.html)?;
    }
    for path in files_in(&tests.join("attacks"), "html")? {
        let rendered = sanitize_html(&fs::read_to_string(&path)?, &options());
        write(format!("attack-{}.html", stem(&path)), &rendered.html)?;
    }
    let shapes = [
        ("newsletter", NEWSLETTER.to_owned()),
        ("quote-1", nested("blockquote", 1)),
        ("quote-3", nested("blockquote", 3)),
        // The nesting guard (MAX_DEPTH, 256) renders mail nested that deep
        // or deeper as plain text; 200 stays rich.
        ("nesting-200", nested("blockquote", 200)),
        ("nesting-256", nested("blockquote", 256)),
        ("nesting-300", nested("blockquote", 300)),
    ];
    for (name, html) in shapes {
        write(
            format!("{name}.html"),
            &sanitize_html(&html, &options()).html,
        )?;
    }
    fs::write(out.join("index.txt"), written.join("\n") + "\n")
}
