#!/bin/sh
# QML rules checked by tools/ci/qml-lint.pl:
#  - QML never formats a count (#3): counted text comes from Rust (tr_n)
#    through EnvioCore.countText. Fails on a qsTr/qsTranslate/qsTrId call
#    with a count argument (also across lines, or `qsTr (`) and on %n inside
#    one of their strings. Comments are ignored.
#  - fontMono is read only by qml/platform/Theme.qml (#67).
#  - A radius outside qml/platform/ is a Theme role or a circle (#68).
#  - No swipe actions on list rows: fails on SwipeDelegate, leadingActions
#    and trailingActions.
#
#   tools/ci/no-qml-plurals.sh [dir]     check a directory (default qml/)
#   tools/ci/no-qml-plurals.sh --self-test
#       each file in tests/ci/bad/ (there must be some, each with a first line
#       `// expect: <text>`) must be rejected with exit status 1 and a
#       file:line line containing <text>; tests/ci/ok/ and qml/ must pass.
#       A missing or empty directory fails.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
lint="$root/tools/ci/qml-lint.pl"

# A directory that is missing or holds no .qml file is an error, never a pass.
check() {
  if [ ! -d "$1" ]; then
    echo "no-qml-plurals: not a directory: $1" >&2
    return 1
  fi
  files=$(find "$1" -name '*.qml' | LC_ALL=C sort)
  if [ -z "$files" ]; then
    echo "no-qml-plurals: no .qml files in $1" >&2
    return 1
  fi
  # shellcheck disable=SC2086
  perl "$lint" $files
}

# Every file in $1 must be rejected with exit status 1 and a "file:line: ..."
# line containing the text of its first-line "// expect: <text>" comment.
self_test_bad() {
  bad="$1"
  if [ ! -d "$bad" ]; then
    echo "no-qml-plurals: the bad fixtures are missing: $bad" >&2
    return 1
  fi
  count=0
  st=0
  for f in "$bad"/*.qml; do
    [ -f "$f" ] || continue
    count=$((count + 1))
    want=$(sed -n '1s|^// expect: ||p' "$f")
    if [ -z "$want" ]; then
      echo "no-qml-plurals: no '// expect:' first line in $f" >&2
      st=1
      continue
    fi
    rc=0
    out=$(perl "$lint" "$f") || rc=$?
    if [ "$rc" -ne 1 ]; then
      echo "no-qml-plurals: expected exit status 1, got $rc: $f" >&2
      st=1
    elif ! printf '%s\n' "$out" | grep -E -- "^.+:[0-9]+: " | grep -F -q -- "$want"; then
      echo "no-qml-plurals: $f was not rejected for '$want'; got: $out" >&2
      st=1
    fi
  done
  if [ "$count" -eq 0 ]; then
    echo "no-qml-plurals: no bad fixtures in $bad" >&2
    return 1
  fi
  return $st
}

if [ "${1:-}" = --self-test ]; then
  status=0
  self_test_bad "${BAD_DIR:-$root/tests/ci/bad}" || status=1
  check "$root/tests/ci/ok" || { echo "no-qml-plurals: the ok fixtures were rejected" >&2; status=1; }
  check "$root/qml" || status=1
  exit $status
fi
check "${1:-$root/qml}"
