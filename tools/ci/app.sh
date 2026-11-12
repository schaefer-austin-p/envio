#!/bin/sh
# The Qt side, in the ut-app-ci image (clickable's amd64 image plus node,
# Xvfb, clang tools): translations, qt/ fmt and clippy, qmllint, a -Werror
# CMake build. Same script in CI and `cargo make app-docker`.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
build=${BUILD_DIR:-$root/build-ci}
export BUILD_DIR="$build"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target/ci-qt}

section() { echo "== $1"; }

section "translations: po/ is up to date with the sources"
command -v xgettext >/dev/null || { apt-get update -q && apt-get install -yq --no-install-recommends gettext; }
"$root/tools/i18n/update.sh" --check

section "format: qt/ Rust"
cargo fmt --manifest-path "$root/qt/Cargo.toml" --all --check

section "lint: QML"
qmllint=$(command -v qmllint || echo /usr/lib/qt5/bin/qmllint)
find "$root/qml" -name '*.qml' -exec "$qmllint" {} +

section "lint: QML formats no counts (#3)"
"$root/tools/ci/no-qml-plurals.sh" --self-test

section "lint: qt/ Rust"
cargo clippy --manifest-path "$root/qt/Cargo.toml" --all-targets --locked -- -D warnings

section "tests: qt/ Rust (the pure conversions)"
cargo test --manifest-path "$root/qt/Cargo.toml" --locked

section "build: CMake, warnings as errors"
cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release -DENVIO_WERROR=ON \
  ${FETCHCONTENT_BASE_DIR:+"-DFETCHCONTENT_BASE_DIR=$FETCHCONTENT_BASE_DIR"}
cmake --build "$build" -j "$(nproc)"

section "tests: QML"
"$root/tests/qml/run.sh"
