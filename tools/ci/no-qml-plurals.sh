#!/bin/sh
# QML never formats a count (#3): counted text comes from Rust (tr_n)
# through EnvioCore.countText. Fails if a .qml file under the given
# directory (default qml/) has a three-argument qsTr(...) (the plural
# form) or a %n placeholder.
#
#   tools/ci/no-qml-plurals.sh [dir]
#   tools/ci/no-qml-plurals.sh --self-test    # the checker on its fixture
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)

check() {
  dir=$1
  # A qsTr call with two top-level commas has a third (the count) argument.
  # Strings are removed first so commas and parentheses inside them don't count.
  bad=$(find "$dir" -name '*.qml' | LC_ALL=C sort | while read -r f; do
    sed -E 's/"([^"\\]|\\.)*"/S/g; s/'"'"'([^'"'"'\\]|\\.)*'"'"'/S/g' "$f" |
      grep -nE 'qsTr\([^,)]*,[^,)]*,' | sed "s|^|$f:|" || true
    grep -nF '%n' "$f" | sed "s|^|$f:|" || true
  done)
  if [ -n "$bad" ]; then
    echo "QML formats a count (use EnvioCore.countText, see #3):" >&2
    echo "$bad" >&2
    return 1
  fi
}

if [ "${1:-}" = --self-test ]; then
  if check "$root/tests/ci/plurals" 2>/dev/null; then
    echo "no-qml-plurals: the fixture was not rejected" >&2
    exit 1
  fi
  check "$root/qml"
  exit 0
fi
check "${1:-$root/qml}"
