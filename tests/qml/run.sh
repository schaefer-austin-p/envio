#!/bin/sh
# QML unit tests (tests/qml/tst_*.qml), offscreen. Runs in the ut-app-ci
# image (tools/ci/app.sh, cargo make qml-test). The tests import the Rust
# `Envio` module, so they run in envio-qmltest (tests/qml/main.cpp), built
# here if the build dir lacks it, over a temporary XDG_* tree.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
build=${BUILD_DIR:-$root/build-ci}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target/ci-qt}

[ -f "$build/CMakeCache.txt" ] || cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release \
  ${FETCHCONTENT_BASE_DIR:+"-DFETCHCONTENT_BASE_DIR=$FETCHCONTENT_BASE_DIR"}
cmake "$build" -DENVIO_QMLTEST=ON >/dev/null
cmake --build "$build" --target envio-qmltest -j "$(nproc)"

home=$(mktemp -d)
trap 'rm -rf "$home"' EXIT
export HOME="$home" XDG_CACHE_HOME="$home/cache" XDG_DATA_HOME="$home/data"
QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen} "$build/envio-qmltest" -input "$root/tests/qml" "$@"
