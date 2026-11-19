#!/bin/sh
# The app must build its Config's OAuth providers with
# envio_core::default_oauth_providers() (#219): an empty list in qt/ would
# switch Microsoft sign-in off for good, and nothing else would fail.
# qt/ is its own workspace with no Qt-free test, so this checks the source.
#
#   tools/ci/oauth-wired.sh [dir]     check the .rs files under dir (default qt/src)
#   tools/ci/oauth-wired.sh --self-test
#       a fixture that reverts the line must be rejected; qt/src must pass.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)

check() {
  if [ ! -d "$1" ]; then
    echo "oauth-wired: not a directory: $1" >&2
    return 1
  fi
  bad=$(grep -rn --include='*.rs' -E 'oauth:[[:space:]]*' "$1" | grep -v 'default_oauth_providers()' || true)
  if [ -n "$bad" ]; then
    echo "oauth-wired: the Config must use envio_core::default_oauth_providers():" >&2
    echo "$bad" >&2
    return 1
  fi
  if ! grep -rq --include='*.rs' 'oauth: envio_core::default_oauth_providers()' "$1"; then
    echo "oauth-wired: no Config in $1 sets oauth from default_oauth_providers()" >&2
    return 1
  fi
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  printf 'fn f() { Config { oauth: Vec::new(), } }\n' >"$tmp/lifecycle.rs"
  if check "$tmp" 2>/dev/null; then
    echo "oauth-wired: self-test: a reverted line was accepted" >&2
    exit 1
  fi
  check "$root/qt/src"
  echo "oauth-wired: ok"
  exit 0
fi

check "${1:-$root/qt/src}"
