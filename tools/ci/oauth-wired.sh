#!/bin/sh
# The app must build its Config's OAuth providers with
# envio_core::default_oauth_providers() (#219): an empty list in qt/ would
# switch Microsoft sign-in off for good, and nothing else would fail.
# qt/ is its own workspace with no Qt-free test, so this checks the source.
# Only a field init on its own line (`oauth: ...`) counts; the value may be
# on the next line (rustfmt) and the function may be imported.
#
#   tools/ci/oauth-wired.sh [dir]     check the .rs files under dir (default qt/src)
#   tools/ci/oauth-wired.sh --self-test
#       fixtures that revert the line must be rejected, ones that merely
#       look alike must pass; qt/src must pass.
#
# CI runs the self-test from tools/ci/app.sh and `cargo make ci`.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)

# Prints OK for each good field init and `file:line: text` for each bad one.
scan() {
  find "$1" -name '*.rs' | LC_ALL=C sort | xargs awk '
    FNR == 1 { pending = 0 }
    pending {
      if ($0 ~ /^[ \t]*(envio_core::)?default_oauth_providers\(\)/) print "OK"
      else print FILENAME ":" pl ": " pt
      pending = 0
      next
    }
    /^[ \t]*oauth:/ {
      rest = $0
      sub(/^[ \t]*oauth:[ \t]*/, "", rest)
      if (rest ~ /^(envio_core::)?default_oauth_providers\(\)/) print "OK"
      else if (rest == "") { pending = 1; pl = FNR; pt = $0 }
      else print FILENAME ":" FNR ": " $0
    }
  '
}

check() {
  if [ ! -d "$1" ]; then
    echo "oauth-wired: not a directory: $1" >&2
    return 1
  fi
  found=$(scan "$1")
  bad=$(printf '%s\n' "$found" | grep -v '^OK$' | grep -v '^$' || true)
  if [ -n "$bad" ]; then
    echo "oauth-wired: the Config must use envio_core::default_oauth_providers():" >&2
    echo "$bad" >&2
    return 1
  fi
  if ! printf '%s\n' "$found" | grep -q '^OK$'; then
    echo "oauth-wired: no Config in $1 sets oauth from default_oauth_providers()" >&2
    return 1
  fi
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  mkdir "$tmp/bad" "$tmp/ok1" "$tmp/ok2" "$tmp/ok3"
  # Reverted: must be rejected.
  printf 'fn f() { Config {\n    oauth: Vec::new(),\n} }\n' >"$tmp/bad/a.rs"
  # Reverted next to a good one elsewhere: still rejected.
  printf 'fn g() { C {\n    oauth: envio_core::default_oauth_providers(),\n} }\n' >"$tmp/bad/b.rs"
  # Look-alikes beside a good line: accepted.
  printf '// oauth: Vec::new() was the old value\nfn h() { C {\n    x_oauth: Vec::new(),\n    pub_oauth: 1,\n    oauth: envio_core::default_oauth_providers(),\n} }\n' >"$tmp/ok1/a.rs"
  # Imported function, and a value wrapped onto the next line.
  printf 'use envio_core::default_oauth_providers;\nfn h() { C {\n    oauth: default_oauth_providers(),\n} }\n' >"$tmp/ok2/a.rs"
  printf 'fn h() { C {\n    oauth:\n        envio_core::default_oauth_providers(),\n} }\n' >"$tmp/ok3/a.rs"
  if check "$tmp/bad" 2>/dev/null; then
    echo "oauth-wired: self-test: a reverted line was accepted" >&2
    exit 1
  fi
  for d in ok1 ok2 ok3; do
    if ! check "$tmp/$d" 2>/dev/null; then
      echo "oauth-wired: self-test: a good fixture ($d) was rejected" >&2
      exit 1
    fi
  done
  # A directory with no Config at all is an error, never a pass.
  mkdir "$tmp/none"
  printf 'fn f() {}\n' >"$tmp/none/a.rs"
  if check "$tmp/none" 2>/dev/null; then
    echo "oauth-wired: self-test: a directory with no Config passed" >&2
    exit 1
  fi
  check "$root/qt/src"
  echo "oauth-wired: ok"
  exit 0
fi

check "${1:-$root/qt/src}"
