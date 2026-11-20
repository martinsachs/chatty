#!/usr/bin/env bash
# Lokaler Mattermost-Testserver für Integrationstest, UI-Prüfung und RAM-Messung.
#
#   scripts/mm-test-server.sh [--messages N] [--channels N] [--delay]   Server neu aufsetzen
#   scripts/mm-test-server.sh --send N                                  N Nachrichten als anna in „Dev“
#
# Container: chatty-mm (Port 8065), chatty-mm-proxy (nginx, Port 8066 → 8065, für Trenn-Tests).
# Zugangsdaten landen in src-tauri/target/mm-test.env (nicht in build/, das der Frontend-Build leert).
# Passwörter werden zufällig erzeugt und nie ausgegeben.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ENV_FILE=$ROOT/src-tauri/target/mm-test.env
URL=http://localhost:8065
PROXY_URL=http://localhost:8066
API=$URL/api/v4

MESSAGES=0
CHANNELS=0
DELAY=0
SEND=0
while [ $# -gt 0 ]; do
  case $1 in
    --messages) MESSAGES=$2; shift 2 ;;
    --channels) CHANNELS=$2; shift 2 ;;
    --delay) DELAY=1; shift ;;
    --send) SEND=$2; shift 2 ;;
    *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
  esac
done

json() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }
login() { # benutzer passwort → token
  curl -sf -i -X POST "$API/users/login" -H 'Content-Type: application/json' \
    -d "{\"login_id\":\"$1\",\"password\":\"$2\"}" | tr -d '\r' | awk -F': ' 'tolower($1)=="token"{print $2}'
}
api() { # token methode pfad [body]
  curl -sf -X "$2" "$API$3" -H "Authorization: Bearer $1" -H 'Content-Type: application/json' ${4:+-d "$4"}
}
send_messages() { # token kanal anzahl
  local i
  for i in $(seq 1 "$3"); do
    api "$1" POST /posts "{\"channel_id\":\"$2\",\"message\":\"Nachricht $i von anna ($(date +%T))\"}" >/dev/null
  done
}

if [ "$SEND" -gt 0 ]; then
  set -a; . "$ENV_FILE"; set +a
  send_messages "$(login "$CHATTY_MM_USER2" "$CHATTY_MM_PASSWORD2")" "$CHATTY_MM_DEV" "$SEND"
  echo "$SEND Nachrichten gesendet"
  exit 0
fi

docker rm -f chatty-mm chatty-mm-proxy >/dev/null 2>&1 || true
docker run -d --name chatty-mm -p 8065:8065 mattermost/mattermost-preview:latest >/dev/null
echo "Warte auf Mattermost …"
until curl -sf "$API/system/ping" >/dev/null; do sleep 3; done

NGINX_CONF=$(mktemp)
LIMIT=""
[ "$DELAY" = 1 ] && LIMIT="location ~ ^/api/v4/channels/[^/]+/posts { limit_rate 2k; proxy_pass http://127.0.0.1:8065; proxy_set_header Host \$host; }"
cat >"$NGINX_CONF" <<EOF
server {
  listen 8066;
  client_max_body_size 50M;
  location ~ /api/v[0-9]+/(users/)?websocket\$ {
    proxy_set_header Upgrade \$http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host \$host;
    proxy_http_version 1.1;
    proxy_read_timeout 600s;
    proxy_pass http://127.0.0.1:8065;
  }
  $LIMIT
  location / {
    proxy_set_header Host \$host;
    proxy_http_version 1.1;
    proxy_pass http://127.0.0.1:8065;
  }
}
EOF
chmod 644 "$NGINX_CONF"
docker run -d --name chatty-mm-proxy --network host \
  -v "$NGINX_CONF:/etc/nginx/conf.d/default.conf:ro" nginx:stable >/dev/null

pw() { python3 -c 'import secrets; print("Ct-" + secrets.token_urlsafe(18))'; }
P1=$(pw); P2=$(pw); P3=$(pw)
# Der erste Nutzer darf sich selbst anlegen (wird Systemadmin), weitere legt er an.
curl -sf -X POST "$API/users" -H 'Content-Type: application/json' \
  -d "{\"email\":\"tester@example.com\",\"username\":\"tester\",\"password\":\"$P1\"}" >/dev/null
T=$(login tester "$P1")
for u in "anna:$P2" "bert:$P3"; do
  name=${u%%:*}; pass=${u#*:}
  api "$T" POST /users "{\"email\":\"$name@example.com\",\"username\":\"$name\",\"password\":\"$pass\"}" >/dev/null
done
ME=$(api "$T" GET /users/me | json "['id']")
ANNA=$(api "$T" GET /users/username/anna | json "['id']")
BERT=$(api "$T" GET /users/username/bert | json "['id']")
T1=$(api "$T" POST /teams '{"name":"team1","display_name":"Team Eins","type":"O"}' | json "['id']")
api "$T" POST /teams '{"name":"team2","display_name":"Team Zwei","type":"O"}' >/dev/null
for u in "$ANNA" "$BERT"; do
  api "$T" POST "/teams/$T1/members" "{\"team_id\":\"$T1\",\"user_id\":\"$u\"}" >/dev/null
done
DEV=$(api "$T" POST /channels "{\"team_id\":\"$T1\",\"name\":\"dev\",\"display_name\":\"Dev\",\"type\":\"O\"}" | json "['id']")
api "$T" POST "/channels/$DEV/members" "{\"user_id\":\"$ANNA\"}" >/dev/null
api "$T" POST /channels/direct "[\"$ME\",\"$ANNA\"]" >/dev/null
for i in $(seq 1 "$CHANNELS"); do
  api "$T" POST /channels "{\"team_id\":\"$T1\",\"name\":\"c-$i\",\"display_name\":\"Kanal $i\",\"type\":\"O\"}" >/dev/null
done
[ "$MESSAGES" -gt 0 ] && send_messages "$(login anna "$P2")" "$DEV" "$MESSAGES"

mkdir -p "$(dirname "$ENV_FILE")"
umask 077
cat >"$ENV_FILE" <<EOF
CHATTY_MM_URL=$URL
CHATTY_MM_PROXY_URL=$PROXY_URL
CHATTY_MM_USER=tester
CHATTY_MM_PASSWORD=$P1
CHATTY_MM_USER2=anna
CHATTY_MM_PASSWORD2=$P2
CHATTY_MM_USER3=bert
CHATTY_MM_PASSWORD3=$P3
CHATTY_MM_DEV=$DEV
EOF
echo "Testserver bereit: $URL (Proxy $PROXY_URL), Kanal Dev=$DEV, Zugangsdaten in $ENV_FILE"
