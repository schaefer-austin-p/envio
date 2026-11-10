//! The app job's image tag is written in three places: Makefile.toml's
//! APP_CI_IMAGE (local `cargo make app-docker`), the CI job's `image:`, and
//! the app build cache key. Bumping one and not the others builds locally
//! and in CI against different images, or reuses a cache from the old one.

use std::path::Path;

const IMAGE: &str = "code.austinschaefer.com/schaefera/ut-app-ci:";
const CACHE_KEY: &str = "app-build-v1-";

fn read(file: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// Every tag that follows `prefix` in `text`, up to a quote, space or `-$`.
fn tags_after(text: &str, prefix: &str, end: &[char]) -> Vec<String> {
    text.match_indices(prefix)
        .map(|(at, _)| {
            let rest = &text[at + prefix.len()..];
            let len = rest.find(end).unwrap_or(rest.len());
            rest[..len].to_owned()
        })
        .collect()
}

#[test]
fn the_app_image_tag_is_the_same_everywhere() {
    let make = tags_after(&read("Makefile.toml"), IMAGE, &['"', ' ', '\n']);
    let ci_text = read(".forgejo/workflows/ci.yml");
    let image = tags_after(&ci_text, IMAGE, &['"', ' ', '\n']);
    // The key is "app-build-v1-<tag>-${{ hashFiles(...) }}" and the restore
    // key "app-build-v1-<tag>-": the tag ends at the last '-' before '$'
    // or the line end.
    let keys: Vec<String> = tags_after(&ci_text, CACHE_KEY, &['$', '\n'])
        .into_iter()
        .map(|k| k.trim_end_matches('-').to_owned())
        .collect();

    assert_eq!(
        make.len(),
        1,
        "Makefile.toml names the image once: {make:?}"
    );
    assert!(!image.is_empty(), "ci.yml has no app image");
    assert!(
        keys.len() >= 2,
        "ci.yml has no app cache key and restore key: {keys:?}"
    );
    let tag = &make[0];
    for (place, found) in [("ci.yml image", &image), ("ci.yml cache key", &keys)] {
        for t in found {
            assert_eq!(t, tag, "{place} uses {t}, Makefile.toml uses {tag}");
        }
    }
}

#[test]
fn tags_are_cut_at_their_end() {
    let text = "image: x:1.2-3\nkey: k-1.2-3-${{ h }}\n  k-1.2-3-\n";
    assert_eq!(tags_after(text, "x:", &['"', ' ', '\n']), ["1.2-3"]);
    assert_eq!(tags_after(text, "k-", &['$', '\n']), ["1.2-3-", "1.2-3-"]);
}
