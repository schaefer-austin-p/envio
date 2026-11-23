#!/bin/sh
# The D-Bus glue test (#442): tests/notify/main.cpp against a fake
# org.freedesktop.Notifications on a private session bus. Runs in the
# ut-app-ci image (tools/ci/app.sh). Never touches the real session bus:
# dbus-run-session starts its own and exports its address.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
build=${BUILD_DIR:-$root/build-ci}

[ -f "$build/CMakeCache.txt" ] || cmake -S "$root" -B "$build" -DCMAKE_BUILD_TYPE=Release \
  ${FETCHCONTENT_BASE_DIR:+"-DFETCHCONTENT_BASE_DIR=$FETCHCONTENT_BASE_DIR"}
cmake "$build" -DENVIO_NOTIFYTEST=ON >/dev/null
cmake --build "$build" --target envio-notifytest -j "$(nproc)"

home=$(mktemp -d)
trap 'rm -rf "$home"' EXIT
mkdir -m 700 "$home/run"
HOME="$home" XDG_RUNTIME_DIR="$home/run" QT_QPA_PLATFORM=offscreen \
  dbus-run-session -- "$build/envio-notifytest"
