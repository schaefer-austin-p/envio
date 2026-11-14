#!/bin/sh
# tools/stamp.sh: what a development build is made from, as About and the
# start-up log show it (#198): `branch@hash`, with `-dirty` for uncommitted
# changes to tracked files, only the hash on a detached HEAD (a PR's branch
# comes from GITHUB_HEAD_REF), "unknown" without git.
# qt/stamp/build.rs runs it. The cargo make tasks and tools/ci/click.sh run
# it on the host and hand the result into the container as ENVIO_GIT: in
# there a worktree's .git points at a folder that isn't mounted.
cd "$(dirname "$0")/.." || exit 0
# The container's user doesn't own the mounted checkout.
g() { git -c safe.directory='*' "$@"; }
hash=$(g rev-parse --short=10 HEAD 2>/dev/null) || { echo unknown; exit 0; }
branch=${GITHUB_HEAD_REF:-$(g symbolic-ref --short -q HEAD)}
# No index refresh: a rewritten index would make the next build stamp again.
dirty=$(GIT_OPTIONAL_LOCKS=0 g status --porcelain --untracked-files=no 2>/dev/null)
echo "${branch:+$branch@}$hash${dirty:+-dirty}"
