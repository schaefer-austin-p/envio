#!/usr/bin/env bash
# Starts a throwaway Stalwart for envio-net's integration tests and prints
# the environment the tests read (eval "$(up.sh)"). Works against the local
# Docker daemon and against CI's dind daemon (DOCKER_HOST set): the tests
# reach the published ports on STALWART_HOST.
#
# Stalwart 1.0 starts in bootstrap mode without a config file, so: bootstrap
# through its JMAP API, restart, add the STARTTLS listeners and two users,
# restart again. The container removes itself after 2 hours, a backstop for
# runs that never reach their cleanup step.
set -euo pipefail

image=${STALWART_IMAGE:-stalwartlabs/stalwart:v1.0.3}
name=${STALWART_NAME:-envio-stalwart}
host=${STALWART_HOST:-127.0.0.1}
admin="admin:envio-admin-$$"

log() { echo "up.sh: $*" >&2; }

docker rm -f "$name" >/dev/null 2>&1 || true
docker run -d --rm --name "$name" \
  -e STALWART_RECOVERY_ADMIN="$admin" \
  -p 993 -p 143 -p 465 -p 587 -p 8080 \
  --entrypoint timeout "$image" 7200 \
  /usr/local/bin/stalwart --config /etc/stalwart/config.json >/dev/null

port() { docker port "$name" "$1/tcp" | head -n 1 | sed 's/.*://'; }
api_port=$(port 8080)
api="http://$host:$api_port/jmap/"

wait_api() {
  for _ in $(seq 1 60); do
    if curl -fsS -o /dev/null -u "$admin" "http://$host:$api_port/jmap/session" 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  log "Stalwart's API did not come up"; docker logs "$name" >&2 || true; exit 1
}

# One JMAP call; fails on an HTTP error or a "notCreated"/"notUpdated" reply.
jmap() {
  local reply
  reply=$(curl -fsS -u "$admin" -H 'content-type: application/json' --data "$1" "$api")
  if grep -q '"not\(Created\|Updated\)"\|"error"' <<<"$reply"; then
    log "JMAP call failed: $reply"; exit 1
  fi
}

wait_ports() {
  # A port answers once the listener is up; mail ports greet on connect.
  for p in 993 143 465 587; do
    local hp; hp=$(port "$p")
    for _ in $(seq 1 60); do
      if (exec 3<>"/dev/tcp/$host/$hp") 2>/dev/null; then break; fi
      sleep 1
    done
  done
}

restart() {
  docker restart "$name" >/dev/null
  api_port=$(port 8080)
  api="http://$host:$api_port/jmap/"
  wait_api
}

using='"using":["urn:ietf:params:jmap:core","urn:stalwart:jmap"]'

wait_api
log "bootstrapping"
jmap '{'"$using"',"methodCalls":[["x:Bootstrap/set",{"accountId":"d333333","update":{"singleton":{
  "defaultHostname":"mail.envio.test","defaultDomain":"envio.test",
  "requestTlsCertificate":false,"generateDkimKeys":false}}},"0"]]}'
restart

log "adding listeners and users"
domain=$(curl -fsS -u "$admin" -H 'content-type: application/json' --data \
  '{'"$using"',"methodCalls":[["x:Domain/query",{"accountId":"d333333","filter":{"name":"envio.test"}},"d"]]}' \
  "$api" | sed -n 's/.*"ids":\["\([^"]*\)".*/\1/p')
[ -n "$domain" ] || { log "no envio.test domain after bootstrap"; exit 1; }
user() {
  printf '"%s":{"@type":"User","name":"%s","domainId":"%s","credentials":{"0":{"@type":"Password","secret":"%s"}}}' \
    "$1" "$1" "$domain" "$2"
}
jmap '{'"$using"',"methodCalls":[
 ["x:NetworkListener/set",{"accountId":"d333333","create":{
   "imap":{"name":"imap","bind":{"[::]:143":true},"protocol":"imap","useTls":true,"implicitTls":false},
   "submission":{"name":"submission","bind":{"[::]:587":true},"protocol":"smtp","useTls":true,"implicitTls":false}}},"l"],
 ["x:Account/set",{"accountId":"d333333","create":{'"$(user alice alice-secret-1),$(user bob bob-secret-1)"'}},"a"]]}'
restart
wait_ports
log "ready"

cat <<EOF
export STALWART_HOST=$host
export STALWART_IMAPS_PORT=$(port 993)
export STALWART_IMAP_PORT=$(port 143)
export STALWART_SUBMISSIONS_PORT=$(port 465)
export STALWART_SUBMISSION_PORT=$(port 587)
EOF
