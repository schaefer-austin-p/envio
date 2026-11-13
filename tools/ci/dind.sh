# Sourced by CI scripts (not run): installs the pinned docker CLI if missing
# and points DOCKER_HOST at the runner's dind daemon. Sets DIND_IP.
#
# The job container has no docker CLI and no DOCKER_HOST; the dind daemon
# is its default gateway, listening on 2375, and ports or files it holds
# live on that daemon, not in the job container.
if ! command -v docker >/dev/null; then
  # docker 27.5.1's static tarball, checked before it is unpacked.
  case "$(uname -m)" in
    x86_64) docker_sha=4f798b3ee1e0140eab5bf30b0edc4e84f4cdb53255a429dc3bbae9524845d640 ;;
    aarch64) docker_sha=e6b53725a73763ab3f988c73f8772eaed429754c1a579db5ff11f21990fd1817 ;;
    *) echo "no pinned docker CLI for $(uname -m)" >&2; exit 1 ;;
  esac
  tmp=$(mktemp -d)
  curl -fsSL -o "$tmp/docker.tgz" \
    "https://download.docker.com/linux/static/stable/$(uname -m)/docker-27.5.1.tgz"
  echo "$docker_sha  $tmp/docker.tgz" | sha256sum -c -
  tar -xz -C /usr/local/bin --strip-components=1 -f "$tmp/docker.tgz" docker/docker
  rm -rf "$tmp"
fi
gw=$(awk '$2 == "00000000" { print $3; exit }' /proc/net/route)
DIND_IP=$(printf '%d.%d.%d.%d' "0x${gw:6:2}" "0x${gw:4:2}" "0x${gw:2:2}" "0x${gw:0:2}")
export DIND_IP
export DOCKER_HOST="${DOCKER_HOST:-tcp://$DIND_IP:2375}"
