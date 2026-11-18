#!/usr/bin/env bash
# Owner labels and leak cleanup for per-run Stalwart containers (#270).
#
# up.sh sources this file. A run that exports STALWART_OWNER_PID gets its
# container labelled with that PID, the PID's start time (guards against a
# reused PID), our PID namespace and our hostname. A killed run never reached
# its EXIT trap, so clean_stale_stalwart removes a container only when it
# carries envio.owner-pid, was made by our user in our host and PID namespace, and its
# owner is gone (no such process, a zombie, or a different start time).
# Unlabelled containers (a plain or hand-named STALWART_NAME, CI's names) are
# never touched. It does nothing when our own /proc cannot be read (macOS,
# hidepid). Sourcing sets no shell options; running it directly cleans.

# Prints "<state> <starttime>" of a PID from /proc/<pid>/stat; fails if unreadable.
_stalwart_proc_stat() {
  local s rest
  s=$(cat "/proc/$1/stat" 2>/dev/null) || return 1
  rest=${s##*) } # skip "pid (comm) ", comm may hold spaces
  set -- $rest
  [ -n "${20:-}" ] || return 1
  echo "$1 ${20}"
}

# Prints the docker run label arguments for STALWART_OWNER_PID, or nothing.
stalwart_owner_labels() {
  local pid=${STALWART_OWNER_PID:-} st ns
  [ -n "$pid" ] || return 0
  st=$(_stalwart_proc_stat "$pid") || return 0
  ns=$(readlink /proc/self/ns/pid 2>/dev/null) || return 0
  printf '%s\n' --label "envio.owner-pid=$pid" --label "envio.owner-start=${st#* }" \
    --label "envio.owner-pidns=$ns" --label "envio.owner-host=$(hostname)" \
    --label "envio.owner-uid=$(id -u)"
}

_stalwart_label() {
  docker inspect -f "{{index .Config.Labels \"$2\"}}" "$1" 2>/dev/null
}

clean_stale_stalwart() {
  local ns host name pid start st
  ns=$(readlink /proc/self/ns/pid 2>/dev/null) || return 0
  _stalwart_proc_stat self >/dev/null || return 0
  host=$(hostname)
  while read -r name; do
    [ -n "$name" ] || continue
    pid=$(_stalwart_label "$name" envio.owner-pid) || continue
    [[ $pid =~ ^[0-9]+$ ]] || continue
    [ "$(_stalwart_label "$name" envio.owner-pidns)" = "$ns" ] || continue
    [ "$(_stalwart_label "$name" envio.owner-host)" = "$host" ] || continue
    # Another user's owner PID may be hidden from us (hidepid), so only reap our own.
    [ "$(_stalwart_label "$name" envio.owner-uid)" = "$(id -u)" ] || continue
    start=$(_stalwart_label "$name" envio.owner-start) || continue
    if st=$(_stalwart_proc_stat "$pid") && [ "${st%% *}" != Z ] && [ "${st#* }" = "$start" ]; then
      continue # the owner is alive
    fi
    echo "clean.sh: removing leaked $name" >&2
    docker rm -f "$name" >/dev/null 2>&1 || true
  done < <(docker ps -a --filter label=envio.owner-pid --format '{{.Names}}' 2>/dev/null || true)
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then clean_stale_stalwart; fi
