#!/usr/bin/env bash
# CI only: starts Stalwart on the runner's shared dind daemon for
# envio-net's and envio-e2e's integration tests and appends what the tests read to
# $GITHUB_ENV. Locally, use `cargo make net-it` or `cargo make e2e-it` instead.
#
# The job container has no docker CLI and no DOCKER_HOST; the dind daemon
# is its default gateway, listening on 2375, and ports Stalwart publishes
# live on that daemon, so the tests reach them on the gateway address.
set -euo pipefail

# shellcheck source=tools/ci/dind.sh
source "$(dirname "${BASH_SOURCE[0]}")/dind.sh"
export STALWART_HOST="$DIND_IP"
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
