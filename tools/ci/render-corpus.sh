#!/bin/sh
# Renders envio-mime's corpus, attack files and a few generated shapes to Qt
# rich text (cargo run -p envio-mime --example render-corpus) into
# $1, default $BUILD_DIR/corpus (git-ignored), for tests/qml/tst_rich_text.qml.
# tests/qml/run.sh runs it first, inside the CI image. Prints the directory.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-${BUILD_DIR:-$root/build-ci}/corpus}
rm -rf "$out"
cargo run -q --manifest-path "$root/Cargo.toml" -p envio-mime --example render-corpus -- "$out" >&2
echo "$out"
