#!/usr/bin/env bash
# CI only: starts Stalwart on the runner's shared dind daemon for
# envio-net's integration tests and appends what the tests read to
# $GITHUB_ENV. Locally, use `cargo make net-it` instead.
#
# The job container has no docker CLI and no DOCKER_HOST; the dind daemon
# is its default gateway, listening on 2375, and ports Stalwart publishes
# live on that daemon, so the tests reach them on the gateway address.
set -euo pipefail

if ! command -v docker >/dev/null; then
  curl -fsSL "https://download.docker.com/linux/static/stable/$(uname -m)/docker-27.5.1.tgz" |
    tar -xz -C /usr/local/bin --strip-components=1 docker/docker
fi
gw=$(awk '$2 == "00000000" { print $3; exit }' /proc/net/route)
ip=$(printf '%d.%d.%d.%d' "0x${gw:6:2}" "0x${gw:4:2}" "0x${gw:2:2}" "0x${gw:0:2}")
export DOCKER_HOST="${DOCKER_HOST:-tcp://$ip:2375}"
export STALWART_HOST="$ip"
# One daemon serves every job: name the container per run.
export STALWART_NAME="envio-stalwart-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}"

# Written first, so the cleanup step can remove a half-started container.
{
  echo "DOCKER_HOST=$DOCKER_HOST"
  echo "STALWART_NAME=$STALWART_NAME"
} >>"$GITHUB_ENV"
settings=$(crates/envio-net/tests/stalwart/up.sh)
{
  echo "ENVIO_REQUIRE_STALWART=1"
  sed 's/^export //' <<<"$settings"
} >>"$GITHUB_ENV"
