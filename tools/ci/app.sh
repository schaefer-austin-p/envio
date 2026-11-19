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
find "$root/qml" -path "$root/qml/bindle" -prune -o -name '*.qml' -print0 | xargs -0 -r "$qmllint"

section "lint: bindle.qrc lists every bindle QML file (#229)"
(cd "$root" && cargo xtask bindle-qrc)

section "lint: QML formats no counts (#3)"
"$root/tools/ci/no-qml-plurals.sh" --self-test

section "lint: the app's Config uses the OAuth providers (#219)"
"$root/tools/ci/oauth-wired.sh" --self-test

section "lint: qt/ Rust"
cargo clippy --manifest-path "$root/qt/Cargo.toml" --all-targets --locked -- -D warnings

section "tests: qt/ Rust (the pure conversions)"
cargo test --manifest-path "$root/qt/Cargo.toml" --locked

section "build info: an official release has no git data or build time (#198)"
# Builds the stamp with a probe source and time, as a development and as a
# release build; only the first may hold them.
stamped() {
  ENVIO_RELEASE=$1 ENVIO_GIT=envio-probe-source SOURCE_DATE_EPOCH=1234567890 \
    cargo build -q --manifest-path "$root/qt/Cargo.toml" -p envio-stamp --locked
  cat "$CARGO_TARGET_DIR"/debug/deps/libenvio_stamp-*.rlib | grep -aq -e envio-probe-source -e 1234567890
}
stamped 0 || { echo "the probe isn't in a development build, so this check sees nothing"; exit 1; }
if stamped 1; then
  echo "a release build carries the stamp"
  exit 1
fi

section "build: CMake, warnings as errors"
cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release -DENVIO_WERROR=ON \
  ${FETCHCONTENT_BASE_DIR:+"-DFETCHCONTENT_BASE_DIR=$FETCHCONTENT_BASE_DIR"}
cmake --build "$build" -j "$(nproc)"

section "tests: QML"
"$root/tests/qml/run.sh"
