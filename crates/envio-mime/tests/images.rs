//! Images and widths: remote, cid, trackers, scaling, wide tables.

use std::collections::HashMap;

use envio_mime::{RenderOptions, sanitize_html};

fn blocked() -> RenderOptions {
    RenderOptions {
        blocked_label: "[image]".to_owned(),
        max_image_width: 600,
        ..RenderOptions::default()
    }
}

fn allowed() -> RenderOptions {
    RenderOptions {
        allow_remote_images: true,
        ..blocked()
    }
}

fn clean(html: &str) -> String {
    sanitize_html(html, &blocked()).html
}

#[test]
fn blocks_remote_images_by_default() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" alt="A &amp; B"><img src="http://example.com/b.png">"#,
        &blocked(),
    );
    assert_eq!(
        r.html,
        r#"<span style="text-decoration:none">A &amp; B</span><span style="text-decoration:none">[image]</span>"#
    );
    assert_eq!(r.blocked_images, 2);
    assert_eq!(r.removed_trackers, 0);
}

#[test]
fn shows_remote_images_when_allowed() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" width="200" height="100">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="200" height="100">"#
    );
    assert_eq!(r.blocked_images, 0);
}

#[test]
fn lists_the_remote_images_it_shows_once_each() {
    let html = concat!(
        r#"<img src="https://example.com/a.png?x=1&amp;y=2">"#,
        r#"<img src="http://example.com/b.png" width="40">"#,
        r#"<img src="https://example.com/a.png?x=1&amp;y=2">"#,
        r#"<img src="https://example.com/p.gif" width="1" height="1">"#,
        r#"<img src="cid:logo@example.org">"#,
    );
    let options = RenderOptions {
        cid_urls: HashMap::from([(
            "logo@example.org".to_owned(),
            "file:///c/logo.png".to_owned(),
        )]),
        ..allowed()
    };
    let r = sanitize_html(html, &options);
    assert_eq!(
        r.remote_images,
        [
            "https://example.com/a.png?x=1&y=2",
            "http://example.com/b.png"
        ]
    );
}

#[test]
fn lists_no_remote_images_while_they_are_blocked() {
    let r = sanitize_html(r#"<img src="https://example.com/a.png">"#, &blocked());
    assert!(r.remote_images.is_empty());
}

#[test]
fn removes_tracking_pixels_even_when_allowed() {
    let html = concat!(
        r#"<img src="https://example.com/p.gif" width="1" height="1">"#,
        r#"<img src="https://example.com/q.gif" style="width:0;height:0">"#,
        r#"<img src="https://www.google-analytics.com/collect?v=1">"#,
        r#"<img src="https://mandrillapp.com/track/open.php">"#,
    );
    let r = sanitize_html(html, &allowed());
    assert_eq!(r.html, "");
    assert_eq!(r.removed_trackers, 4);
}

#[test]
fn tracker_hosts_match_whole_labels() {
    let r = sanitize_html(
        r#"<img src="https://notdoubleclick.net/a.png" width="10" height="10">"#,
        &allowed(),
    );
    assert_eq!(r.removed_trackers, 0);
    assert!(r.html.contains("notdoubleclick.net"));
}

#[test]
fn maps_cid_images_and_labels_unknown_ones() {
    let options = RenderOptions {
        cid_urls: HashMap::from([("logo@example.org".to_owned(), "image://envio/3".to_owned())]),
        ..blocked()
    };
    let r = sanitize_html(
        r#"<img src="cid:logo@example.org" alt="Logo"><img src="cid:other@example.org"> <img src="cid:gone@example.org" alt="Chart &amp; key">"#,
        &options,
    );
    // An unknown cid shows its alt text or the label, like a blocked
    // remote image, but loading remote images would not bring it back.
    assert_eq!(
        r.html,
        r#"<img src="image://envio/3" alt="Logo"><span style="text-decoration:none">[image]</span> <span style="text-decoration:none">Chart &amp; key</span>"#
    );
    assert_eq!(r.blocked_images, 0);
}

#[test]
fn scales_wide_images_down() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" width="1200" height="800">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="600" height="400">"#
    );
}

#[test]
fn image_size_from_style_becomes_attributes() {
    let r = sanitize_html(
        r#"<img src="https://example.com/a.png" style="width:300px;height:150px">"#,
        &allowed(),
    );
    assert_eq!(
        r.html,
        r#"<img src="https://example.com/a.png" width="300" height="150">"#
    );
}

/// Qt draws an image without a pixel width at the file's size, so a
/// responsive `width:100%` banner would overflow the reader.
#[test]
fn percentage_widths_become_a_share_of_the_reader() {
    for (attrs, width) in [
        (r#"style="width:100%;display:block""#, "600"),
        (r#"width="100%""#, "600"),
        (r#"width="50%" height="80""#, "300"),
        (r#"style="width: 25% !important""#, "150"),
        (r#"style="width:150%""#, "600"),
        (r#"style="width:100%;max-width:240px""#, "240"),
        (r#"style="width:100%;max-width:2000px""#, "600"),
    ] {
        let html = format!(r#"<img src="https://example.com/a.png" {attrs}>"#);
        assert_eq!(
            sanitize_html(&html, &allowed()).html,
            format!(r#"<img src="https://example.com/a.png" width="{width}">"#),
            "{attrs}"
        );
    }
    // A pixel width still wins, and nonsense stays without a width.
    for (attrs, out) in [
        (r#"width="320" style="width:100%""#, r#" width="320""#),
        (r#"style="width:0%""#, ""),
        (r#"style="width:abc%""#, ""),
    ] {
        let html = format!(r#"<img src="https://example.com/a.png" {attrs}>"#);
        assert_eq!(
            sanitize_html(&html, &allowed()).html,
            format!(r#"<img src="https://example.com/a.png"{out}>"#),
            "{attrs}"
        );
    }
}

/// A social icon in a 32 px cell must not grow to the reader's width.
#[test]
fn a_percentage_width_is_a_share_of_its_cell() {
    let html = r#"<table><tr><td width="32"><img src="https://example.com/a.png" style="width:100%"></td><td><img src="https://example.com/b.png" style="width:100%"></td></tr></table>"#;
    let out = sanitize_html(html, &allowed()).html;
    assert!(
        out.contains(r#"<img src="https://example.com/a.png" width="32">"#),
        "{out}"
    );
    // Outside the narrow cell, the share is of the reader again.
    assert!(
        out.contains(r#"<img src="https://example.com/b.png" width="600">"#),
        "{out}"
    );
    // A cell wider than the reader is capped by the reader.
    let html = r#"<table><tr><td width="580"><img src="https://example.com/a.png" width="50%"></td></tr></table>"#;
    assert!(
        sanitize_html(html, &allowed())
            .html
            .contains(r#"<img src="https://example.com/a.png" width="290">"#)
    );
    // An inner table's `</thead>` does not close the outer cell.
    let html = r#"<table><tr><td width="32"><table><thead><tr><th>x</th></tr></thead></table><img src="https://example.com/a.png" style="width:100%"></td></tr></table>"#;
    let out = sanitize_html(html, &allowed()).html;
    assert!(
        out.contains(r#"<img src="https://example.com/a.png" width="32">"#),
        "{out}"
    );
}

#[test]
fn a_percentage_width_is_left_alone_without_a_limit() {
    let options = RenderOptions {
        max_image_width: 0,
        ..allowed()
    };
    let html = r#"<img src="https://example.com/a.png" style="width:100%">"#;
    assert_eq!(
        sanitize_html(html, &options).html,
        r#"<img src="https://example.com/a.png">"#
    );
}

#[test]
fn drops_wide_table_widths() {
    assert_eq!(
        clean(
            r#"<table width="1200"><tr><td width="700">a</td><td width="100">b</td></tr></table>"#
        ),
        r#"<table><tbody><tr><td>a</td><td width="100">b</td></tr></tbody></table>"#
    );
}

#[test]
fn no_width_limit_when_zero() {
    let options = RenderOptions {
        max_image_width: 0,
        ..allowed()
    };
    let html = r#"<img src="https://example.com/a.png" width="5000">"#;
    assert_eq!(sanitize_html(html, &options).html, html);
}

#[test]
fn tracker_sizes_in_any_spelling() {
    for size in [
        "0",
        "0px",
        "1PX",
        "1Px",
        "1.0px",
        "0.5",
        "1pt",
        "1PT",
        "1px !important",
        " 1 px ",
        "2px",
        "0.75pt",
    ] {
        for html in [
            format!(r#"<img src="https://example.com/p.gif" width="{size}" height="50">"#),
            format!(r#"<img src="https://example.com/p.gif" style="height:{size};width:50px">"#),
        ] {
            let r = sanitize_html(&html, &allowed());
            assert_eq!(r.removed_trackers, 1, "{html}");
            assert_eq!(r.html, "", "{html}");
        }
    }
}

#[test]
fn real_images_are_not_trackers() {
    for size in ["3", "3px", "2.6px", "3pt", "100%", "auto", "10"] {
        let html = format!(r#"<img src="https://example.com/p.gif" width="{size}" height="50">"#);
        let r = sanitize_html(&html, &allowed());
        assert_eq!(r.removed_trackers, 0, "{html}");
    }
}

#[test]
fn tracker_host_with_trailing_dot_matches() {
    let r = sanitize_html(
        r#"<img src="https://t.yesware.com./o.gif"><img src="https://x.list-manage.com./o.gif">"#,
        &allowed(),
    );
    assert_eq!(r.removed_trackers, 2);
    assert_eq!(r.html, "");
}

#[test]
fn huge_table_widths_are_dropped() {
    let html =
        r#"<table width="99999999999999999999"><tr><td width="4294967296">x</td></tr></table>"#;
    let out = clean(html);
    assert!(!out.contains("width"), "{out}");
    assert!(out.contains("x"));
}

/// The most distinct remote images a message may load (#215).
const CAP: usize = 200;

fn many(n: usize) -> String {
    (0..n)
        .map(|i| format!(r#"<img src="https://example.com/{i}.png">"#))
        .collect()
}

#[test]
fn caps_the_distinct_remote_images() {
    let r = sanitize_html(&many(10_000), &allowed());
    assert_eq!(r.remote_images.len(), CAP);
    assert_eq!(r.remote_images[0], "https://example.com/0.png");
    assert_eq!(r.remote_images[CAP - 1], "https://example.com/199.png");
    // Past the cap an image is its label: nothing loads it.
    assert_eq!(r.html.matches("<img").count(), CAP);
    assert!(r.html.contains("[image]"));
    assert!(!r.html.contains("example.com/200.png"));
}

#[test]
fn a_repeat_of_a_listed_image_still_shows_past_the_cap() {
    let html = format!(r#"{}<img src="https://example.com/3.png">"#, many(CAP + 50));
    let r = sanitize_html(&html, &allowed());
    assert_eq!(r.remote_images.len(), CAP);
    assert_eq!(r.html.matches("<img").count(), CAP + 1);
}

#[test]
fn caps_the_images_that_wait_for_a_size() {
    // One URL written with a different height each time: each is its own
    // open size entry unless they are bounded.
    let html: String = (1..20_000)
        .map(|h| format!(r#"<img src="https://example.com/a.png" height="{h}">"#))
        .collect();
    let r = sanitize_html(&html, &allowed());
    assert_eq!(r.remote_images, ["https://example.com/a.png"]);
    assert!(r.unsized_images.len() <= PER_URL);
}

/// Hostile corpus: many distinct URLs (4x as many in the large run) must
/// come out capped at `CAP`, and `sanitize_html` must stay linear in the
/// input size. In particular there must be no unbounded per-URL bookkeeping
/// (such as a list of every URL seen, scanned per image): that takes
/// billions of compares at 100k URLs. A wall-clock limit flakes on a busy CI
/// runner (llvm-cov), so compare two sizes in one process, interleaved, and
/// take the fastest of 3 runs each: 4x the input should cost about 4x the
/// time (linear), not 16x (quadratic).
#[test]
fn many_distinct_urls_stay_capped_and_scale_linearly() {
    let (small_html, large_html) = (many(10_000), many(40_000));
    let time = |html: &str| {
        let start = std::time::Instant::now();
        let r = sanitize_html(html, &allowed());
        let took = start.elapsed();
        assert_eq!(r.remote_images.len(), CAP);
        assert!(r.unsized_images.len() <= CAP * PER_URL);
        took
    };
    let (mut small, mut large) = (std::time::Duration::MAX, std::time::Duration::MAX);
    for _ in 0..3 {
        small = small.min(time(&small_html));
        large = large.min(time(&large_html));
    }
    eprintln!("ratio {:.2}", large.as_secs_f64() / small.as_secs_f64());
    // 6x sits between linear (4x) and quadratic (16x); the floor keeps a very
    // fast small run from making the ratio meaningless.
    let limit = small * 6 + std::time::Duration::from_millis(100);
    assert!(
        large < limit,
        "40k URLs took {large:?}, 10k took {small:?}: not linear"
    );
}

/// Entries one URL may have waiting for a size (its widths and heights).
const PER_URL: usize = 8;

#[test]
fn repeats_of_one_url_cannot_use_up_the_size_slots_of_others() {
    // One URL written with a different height each time, then another.
    let mut html: String = (1..400)
        .map(|h| format!(r#"<img src="https://example.com/a.png" height="{h}">"#))
        .collect();
    html.push_str(r#"<img src="https://example.com/b.png">"#);
    let r = sanitize_html(&html, &allowed());
    assert!(r.unsized_images.iter().any(|u| u.url.ends_with("/b.png")));
    let of_a = r
        .unsized_images
        .iter()
        .filter(|u| u.url.ends_with("/a.png"));
    assert!(of_a.count() <= PER_URL);
}

#[test]
fn every_listed_url_keeps_a_size_slot() {
    let r = sanitize_html(&many(10_000), &allowed());
    assert_eq!(r.unsized_images.len(), CAP);
}

#[test]
fn counts_only_the_blocked_images_that_would_load() {
    // The count is what "Load images" brings back: at most the cap.
    let r = sanitize_html(&many(10_000), &blocked());
    assert_eq!(r.blocked_images, CAP);
    assert!(r.remote_images.is_empty());
    assert_eq!(r.html.matches("<img").count(), 0);
}

#[test]
fn counts_a_blocked_url_once() {
    let html = r#"<img src="https://example.com/a.png">"#.repeat(5);
    let r = sanitize_html(&html, &blocked());
    assert_eq!(r.blocked_images, 1);
    assert_eq!(r.html.matches("[image]").count(), 5);
}
