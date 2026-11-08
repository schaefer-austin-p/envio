//! A hard check that the html5ever the guard was fuzzed against is the one
//! the workspace builds with.

/// The html5ever version the long fuzz run last passed on. Update it only
/// after that run shows 0 under-counts.
const FUZZED_HTML5EVER: &str = "0.40.1";

/// Versions of every `html5ever` package in the workspace Cargo.lock.
fn locked_versions(lock: &str) -> Vec<String> {
    lock.split("[[package]]")
        .filter(|block| block.lines().any(|l| l.trim() == r#"name = "html5ever""#))
        .filter_map(|block| {
            block
                .lines()
                .find_map(|l| l.strip_prefix("version = \"")?.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn html5ever_is_the_fuzzed_version() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock");
    let lock = std::fs::read_to_string(path).expect("workspace Cargo.lock");
    let versions = locked_versions(&lock);
    assert_eq!(
        versions,
        [FUZZED_HTML5EVER],
        "html5ever in Cargo.lock is {versions:?}, but the nesting guard was fuzzed against \
         {FUZZED_HTML5EVER}. Run `NESTFUZZ_CASES=500000 cargo test -p envio-mime --release -- \
         --ignored nesting_fuzz`, and once it shows 0 under-counts set FUZZED_HTML5EVER in \
         src/nesting/fuzz_baseline.rs to the new version."
    );
}

#[test]
fn lock_parser_reads_versions() {
    let lock = "[[package]]\nname = \"a\"\nversion = \"1\"\n\n[[package]]\nname = \"html5ever\"\nversion = \"0.9.1\"\nsource = \"x\"\n";
    assert_eq!(locked_versions(lock), ["0.9.1"]);
}
