#!/usr/bin/env bash
# Removes leaked per-run Stalwart containers: every `envio-stalwart-<digits>`
# whose PID is no longer a running process on this host (a killed run never
# reached its EXIT trap, #270). Other names, including the plain
# `envio-stalwart` and CI's `envio-stalwart-<run>-<attempt>`, are left alone.
# Source it for clean_stale_stalwart, or run it directly.
set -euo pipefail

clean_stale_stalwart() {
  local name pid
  while read -r name; do
    [[ $name =~ ^envio-stalwart-([0-9]+)$ ]] || continue
    pid=${BASH_REMATCH[1]}
    if [ ! -d "/proc/$pid" ]; then
      echo "clean.sh: removing leaked $name" >&2
      docker rm -f "$name" >/dev/null 2>&1 || true
    fi
  done < <(docker ps -a --filter name=envio-stalwart- --format '{{.Names}}' 2>/dev/null || true)
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then clean_stale_stalwart; fi
