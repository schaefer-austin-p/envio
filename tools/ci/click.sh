#!/usr/bin/env bash
# Builds the arm64 .click in clickable's cross-compile image and reviews it;
# the package and the review output land in build-click/. CI runs this on the
# runner's dind daemon (see dind.sh). Locally, point DOCKER_HOST at your
# daemon (unix:///var/run/docker.sock), or use `cargo make click` instead.
#
# clickable normally bind-mounts the project, but a job container's paths
# don't exist on the dind daemon, so the source goes in with `docker cp`.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# docker.io/clickable/amd64-ut24.04-1.x-arm64 (image_version 13).
image=clickable/amd64-ut24.04-1.x-arm64@sha256:2ea7f5e79c257a3928adcf7b45d13e8bc72ab9c7c0367d94f1214e5a0c58c817
clickable_version=8.10.0
out=build-click

# shellcheck source=tools/ci/dind.sh
source tools/ci/dind.sh

# No reap of earlier runs' containers: two jobs can overlap on one daemon.
# A cancelled run's container ends by itself when its sleep does (--rm).
c=$(docker create --rm --label envio-ci=click --label "envio-ci-run=${GITHUB_RUN_ID:-local}" \
  "$image" sleep 7200)
trap 'docker rm -f "$c" >/dev/null 2>&1 || true' EXIT
docker start "$c" >/dev/null
git archive --prefix=src/ HEAD | docker cp - "$c:/"
# git archive leaves submodules out: add bindle (qml/bindle) so CMake finds bindle.qrc.
# Archive the commit the gitlink pins, not whatever the submodule has checked out.
git -C qml/bindle archive --prefix=src/qml/bindle/ "$(git rev-parse HEAD:qml/bindle)" | docker cp - "$c:/"

# Which build this is (#198), taken here: the container gets no .git. The
# v* tag build is the official release, which carries no stamp; ENVIO_GIT is
# then a sentinel that only a stamped build would hold, checked below.
sentinel=envio-release-build-must-not-be-stamped
case "${GITHUB_REF:-}" in
  refs/tags/v*) release=1 stamp=$sentinel ;;
  *) release='' stamp=$(sh tools/stamp.sh) ;;
esac
if [ -n "$release" ]; then
  echo "build stamp: none (official release)"
else
  echo "build stamp: $stamp"
fi

run() {
  docker exec -w /src -e CLICKABLE_ENV_ENVIO_RELEASE="$release" -e CLICKABLE_ENV_ENVIO_GIT="$stamp" \
    "$c" bash -euo pipefail -c "$1"
}

# The image has click-review but not clickable. A prebuilt image with
# clickable in it would remove this step. --ignore-installed: pip would
# otherwise uninstall the image's apt `click` (the package tool) to make room
# for the `click` library clickable needs.
# A stalled mirror once hung apt-get for 43 min (#451); fail well before the
# job's 45-min limit.
run "timeout 15m apt-get update && timeout 15m apt-get install -y --no-install-recommends python3-pip && pip install --break-system-packages --ignore-installed clickable-ut==$clickable_version"

# --skip-review: the review below decides (errors fail, warnings print);
# the build's own review would fail on a warning and skip the report.
run 'clickable build --arch arm64 --container-mode --skip-review'

# Only compiled code: cargo's fingerprints record every env var a build
# script watches, the sentinel included, without it reaching the binary.
if [ -n "$release" ] && docker exec "$c" grep -rqa --include='*.rlib' --include='*.a' \
  --include='*.so' --include=envio "$sentinel" /src/build; then
  echo "the release build is stamped: ENVIO_RELEASE didn't reach it" >&2
  exit 1
fi

rm -rf "$out"
mkdir -p "$out"
click=$(docker exec "$c" bash -c 'find /src/build -name "*.click" | head -n1')
[ -n "$click" ] || { echo "no .click was built" >&2; exit 1; }
docker cp "$c:$click" "$out/"

# click-review: errors fail the script, warnings are printed only.
run 'clickable review --arch arm64 --container-mode --accept-warnings' 2>&1 | tee "$out/click-review.txt"
ls -l "$out"
