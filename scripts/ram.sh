#!/usr/bin/env bash
# RAM-Messung (Linux): chatty mit isoliertem Profil gegen den Testserver starten, PSS/RSS des Prozessbaums
# summieren und den Hauptprozess aufschlüsseln.
#
#   scripts/ram.sh [--bin PFAD] [--wait-key] [sekunden=60]
#
# Voraussetzung: scripts/mm-test-server.sh lief (src-tauri/target/mm-test.env), Release-Build vorhanden.
# --wait-key: statt fester Wartezeit auf Enter warten (z. B. nach Kanal öffnen und Hochscrollen).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=$ROOT/src-tauri/target/release/chatty
WAIT_KEY=0
SECS=60
while [ $# -gt 0 ]; do
  case $1 in
    --bin) BIN=$2; shift 2 ;;
    --wait-key) WAIT_KEY=1; shift ;;
    *) SECS=$1; shift ;;
  esac
done
[ -x "$BIN" ] || { echo "Binary fehlt: $BIN" >&2; exit 1; }

set -a; . "$ROOT/src-tauri/target/mm-test.env"; set +a
ACCOUNT=chatty-ram-test
PROFILE=$(mktemp -d)
cleanup() {
  [ -n "${APP:-}" ] && kill -- -"$APP" 2>/dev/null || true
  (cd "$ROOT/src-tauri" && cargo run -q -p chatty-core --example mm_login -- "$ACCOUNT" --delete >/dev/null 2>&1) || true
  rm -rf "$PROFILE"
}
trap cleanup EXIT

(cd "$ROOT/src-tauri" && cargo run -q -p chatty-core --example mm_login -- "$ACCOUNT" >/dev/null)
mkdir -p "$PROFILE/config/chatty" "$PROFILE/data"
cat >"$PROFILE/config/chatty/accounts.json" <<EOF
[{"id":"$ACCOUNT","kind":"mattermost","server":"$CHATTY_MM_PROXY_URL","label":"ram-test","login":"$CHATTY_MM_USER"}]
EOF

XDG_CONFIG_HOME=$PROFILE/config XDG_DATA_HOME=$PROFILE/data setsid "$BIN" >"$PROFILE/app.log" 2>&1 </dev/null &
APP=$!
if [ "$WAIT_KEY" = 1 ]; then
  read -r -p "Messen mit Enter … " _
else
  sleep "$SECS"
fi

tree() { echo "$1"; for c in $(pgrep -P "$1"); do tree "$c"; done; }
kb() { awk -v k="$1:" '$1==k {print $2}' "$2"; }
total_pss=0; total_rss=0
for p in $(tree "$APP"); do
  [ -r "/proc/$p/smaps_rollup" ] || continue
  pss=$(kb Pss "/proc/$p/smaps_rollup"); rss=$(kb Rss "/proc/$p/smaps_rollup")
  printf '%-8s %-24s PSS %5d MB  RSS %5d MB\n' "$p" "$(cat "/proc/$p/comm")" $((pss / 1024)) $((rss / 1024))
  total_pss=$((total_pss + pss)); total_rss=$((total_rss + rss))
done
echo "SUMME PSS $((total_pss / 1024)) MB  RSS $((total_rss / 1024)) MB"

# Hauptprozess: eigener Speicher (Heap, anonym) gegenüber Bibliotheken (geteilt).
awk '
  /^[0-9a-f]+-[0-9a-f]+ / { name = (NF >= 6) ? $6 : "[anon]"; kind = (name ~ /\.so/) ? "lib" : (name ~ /^\[heap\]|^\[anon/ ? "heap/anon" : "sonst") }
  /^Pss:/ { pss[kind] += $2 }
  /^Private_Dirty:/ { dirty[kind] += $2 }
  END { for (k in pss) printf "  chatty %-10s PSS %5d MB  Private_Dirty %5d MB\n", k, pss[k] / 1024, dirty[k] / 1024 }
' "/proc/$APP/smaps"
