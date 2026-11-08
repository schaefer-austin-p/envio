//! Differential fuzz: the guard in [`super`] must never say "fine" for
//! markup that html5ever really nests deeper than [`MAX_DEPTH`].
//!
//! The guard models html5ever 0.40.1 (pinned in Cargo.toml). Any bump of
//! that pin must rerun the long test, see the `Cargo.toml` comment.

use super::fuzz_gen::{Rng, generate};
use super::fuzz_sink::real_depth;
use super::{MAX_DEPTH, too_deep};

/// The tree holds the document and `html` above the fragment's elements,
/// so real depth may exceed [`MAX_DEPTH`] by this much for a guard that
/// counts elements only.
const ROOT_SLACK: usize = 3;

/// Pieces per case in the long run; the quick run uses fewer to stay fast.
const LONG_REPEATS: usize = 400;
const QUICK_REPEATS: usize = 100;

/// Whether the guard under-counts `html`: it passes it, yet html5ever
/// builds a tree deeper than the limit.
fn under_counts(html: &str) -> bool {
    if too_deep(html) {
        return false;
    }
    let (real, live) = real_depth(html);
    real.max(live) > MAX_DEPTH + ROOT_SLACK
}

/// Greedy delta debugging over tag-sized chunks: drops runs of tokens
/// while the guard still under-counts.
fn shrink(html: &str) -> String {
    let mut tokens: Vec<String> = html.split_inclusive('>').map(str::to_owned).collect();
    let mut chunk = tokens.len() / 2;
    while chunk >= 1 {
        let mut i = 0;
        while i < tokens.len() {
            let mut trial = tokens.clone();
            trial.drain(i..(i + chunk).min(tokens.len()));
            if under_counts(&trial.concat()) {
                tokens = trial;
            } else {
                i += chunk;
            }
        }
        chunk /= 2;
    }
    tokens.concat()
}

/// Runs `cases` cases from `seed`; returns the shrunk inputs that the guard
/// under-counted (at most `keep`) and how many there were.
fn run(cases: u64, seed: u64, repeats: usize, keep: usize) -> (Vec<String>, u64) {
    let mut rng = Rng::new(seed);
    let mut shrunk = Vec::new();
    let mut bad = 0;
    for _ in 0..cases {
        let html = generate(&mut rng, repeats);
        if under_counts(&html) {
            bad += 1;
            if shrunk.len() < keep {
                shrunk.push(shrink(&html));
            }
        }
    }
    (shrunk, bad)
}

fn assert_no_under_counts(cases: u64, seed: u64, repeats: usize) {
    let (shrunk, bad) = run(cases, seed, repeats, 3);
    assert!(
        bad == 0,
        "{bad} of {cases} cases (seed {seed}) under-counted; shrunk inputs:\n{shrunk:#?}"
    );
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Quick, deterministic: part of every test run.
#[test]
fn nesting_fuzz_seeded() {
    assert_no_under_counts(1500, 0x5EED_0001, QUICK_REPEATS);
}

/// The long run: `NESTFUZZ_CASES` cases (default 500000) from
/// `NESTFUZZ_SEED` (default 1). Run in release:
/// `cargo test -p envio-mime --release -- --ignored nesting_fuzz`.
#[test]
#[ignore = "long; run on html5ever bumps and guard changes"]
fn nesting_fuzz_long() {
    let cases = env_u64("NESTFUZZ_CASES", 500_000);
    let seed = env_u64("NESTFUZZ_SEED", 1);
    assert_no_under_counts(cases, seed, LONG_REPEATS);
    println!("nesting fuzz: {cases} cases from seed {seed}, 0 under-counts");
}

/// The harness itself must be able to fail: a guard-free depth check on a
/// known-deep input has to see the depth.
#[test]
fn nesting_fuzz_sink_measures_depth() {
    let html = "<div>".repeat(40);
    let (real, _) = real_depth(&html);
    assert!(real >= 40, "real depth {real}");
    assert!(!too_deep(&html));
}
