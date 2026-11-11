#!/bin/sh
# QML rules checked by tools/ci/qml-lint.pl:
#  - QML never formats a count (#3): counted text comes from Rust (tr_n)
#    through EnvioCore.countText. Fails on a qsTr/qsTranslate/qsTrId call
#    with a count argument (also across lines, or `qsTr (`) and on %n inside
#    one of their strings. Comments are ignored.
#  - No swipe actions on list rows: fails on SwipeDelegate, leadingActions
#    and trailingActions.
#
#   tools/ci/no-qml-plurals.sh [dir]     check a directory (default qml/)
#   tools/ci/no-qml-plurals.sh --self-test
#       each file in tests/ci/bad/ must be rejected, tests/ci/ok/ and qml/ pass
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
lint="$root/tools/ci/qml-lint.pl"

check() {
  find "$1" -name '*.qml' | LC_ALL=C sort | xargs -r perl "$lint"
}

if [ "${1:-}" = --self-test ]; then
  status=0
  for f in "$root"/tests/ci/bad/*.qml; do
    if perl "$lint" "$f" >/dev/null; then
      echo "no-qml-plurals: not rejected: $f" >&2
      status=1
    fi
  done
  check "$root/tests/ci/ok" || { echo "no-qml-plurals: the ok fixtures were rejected" >&2; status=1; }
  check "$root/qml" || status=1
  exit $status
fi
check "${1:-$root/qml}"
