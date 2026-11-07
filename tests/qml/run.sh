#!/bin/sh
# QML unit tests (tests/qml/tst_*.qml), offscreen. Runs in the ut-app-ci
# image (tools/ci/app.sh, cargo make qml-test).
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
runner=$(command -v qmltestrunner || echo /usr/lib/qt5/bin/qmltestrunner)
QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen} exec "$runner" -input "$root/tests/qml" "$@"
