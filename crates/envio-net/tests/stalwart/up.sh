#!/usr/bin/env bash
# Starts a throwaway Stalwart for envio-net's integration tests and prints
# the environment the tests read (eval "$(up.sh)"). Works against the local
# Docker daemon and against CI's dind daemon (DOCKER_HOST set): the tests
# reach the published ports on STALWART_HOST.
#
# Stalwart 1.0 starts in bootstrap mode without a config file, so: bootstrap
# through its JMAP API, restart, add the STARTTLS listeners and the test users (alice and bob for envio-net,
# carol to frank for envio-e2e's sync tests, grace to ivan for its core tests,
# judy to olga for its older-mail tests,
# one per test),
# restart again. The container removes itself after 2 hours, a backstop for
# runs that never reach their cleanup step.
set -euo pipefail

image=${STALWART_IMAGE:-stalwartlabs/stalwart:v1.0.3@sha256:73c684a84c24f5b4206ec266a8deb8986fe44a9f623997e07f084388d943c8bd}
name=${STALWART_NAME:-envio-stalwart}
host=${STALWART_HOST:-127.0.0.1}
admin="admin:envio-admin-$$"

log() { echo "up.sh: $*" >&2; }

# Containers of killed local runs (their labelled owner is gone) go first.
. "$(dirname "$0")/clean.sh"
clean_stale_stalwart
labels=()
while IFS= read -r l; do labels+=("$l"); done < <(stalwart_owner_labels)
docker rm -f "$name" >/dev/null 2>&1 || true
docker run -d --rm --name "$name" ${labels[@]+"${labels[@]}"} \
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
    local open=0
    for _ in $(seq 1 60); do
      if (exec 3<>"/dev/tcp/$host/$hp") 2>/dev/null; then open=1; break; fi
      sleep 1
    done
    if [ "$open" -ne 1 ]; then log "port $p ($host:$hp) never opened"; exit 1; fi
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
# Stalwart's default "Sender IP throttle" lets one IP open 5 SMTP sessions a
# second, and every session of these tests comes from the one Docker gateway
# IP. Adding two accounts takes four (each probe opens SMTP once to be asked
# about the certificate and once to log in), so a busy second gets one dropped
# with "peer closed connection without sending TLS close_notify" and the
# wizard says "Could not reach the server" (#105). The limit of 25 mails an
# hour per recipient stays.
throttles=$(curl -fsS -u "$admin" -H 'content-type: application/json' --data \
  '{'"$using"',"methodCalls":[["x:MtaInboundThrottle/get",{"accountId":"d333333"},"t"]]}' \
  "$api")
# One list object a line, so the id is the one of the object that carries the
# description, whatever the order of its fields; two ids or none is no match.
# `|| true`: a miss ends at the message below, not silently in pipefail.
ip_throttle=$(sed 's/},{/}\n{/g' <<<"$throttles" | grep '"description":"Sender IP throttle"' \
  | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)
case "$ip_throttle" in
  "" | *[!a-z0-9]*) log "no single Sender IP throttle to relax in: $throttles"; exit 1 ;;
esac
user() {
  printf '"%s":{"@type":"User","name":"%s","domainId":"%s","credentials":{"0":{"@type":"Password","secret":"%s"}}}' \
    "$1" "$1" "$domain" "$2"
}
jmap '{'"$using"',"methodCalls":[
 ["x:MtaInboundThrottle/set",{"accountId":"d333333","update":{"'"$ip_throttle"'":{"rate":{"count":1000,"period":1000}}}},"t"],
 ["x:NetworkListener/set",{"accountId":"d333333","create":{
   "imap":{"name":"imap","bind":{"[::]:143":true},"protocol":"imap","useTls":true,"implicitTls":false},
   "submission":{"name":"submission","bind":{"[::]:587":true},"protocol":"smtp","useTls":true,"implicitTls":false}}},"l"],
 ["x:Account/set",{"accountId":"d333333","create":{'"$(user alice alice-secret-1),$(user bob bob-secret-1),$(user carol carol-secret-1),$(user dave dave-secret-1),$(user erin erin-secret-1),$(user frank frank-secret-1),$(user grace grace-secret-1),$(user heidi heidi-secret-1),$(user ivan pässwort-ivan-1),$(user judy judy-secret-1),$(user karl karl-secret-1),$(user liam liam-secret-1),$(user mona mona-secret-1),$(user nina nina-secret-1),$(user olga olga-secret-1)"'}},"a"]]}'
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
