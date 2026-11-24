#!/usr/bin/env bash
# Runs a headless world on a throwaway database, closes N ticks through the admin API, then
# prints the recent chronicle and what every seat sees through MCP `look`. Given a seat and a
# JSON list of orders, that seat acts through MCP `act` before the first tick closes.
# Usage: scripts/verify-world.sh [scenario] [ticks] [seed] [seat orders]
set -euo pipefail

scenario=${1:-scenarios/rome.yaml}
ticks=${2:-4}
seed=${3:-2026}
actor=${4:-}
orders=${5:-[]}
port=$((20000 + RANDOM % 20000))
api=http://127.0.0.1:$port

cargo build -q -p sim-server
dir=$(mktemp -d)
target/debug/sim-server run --db "$dir/world.db" --scenario "$scenario" --seed "$seed" \
  --bind "127.0.0.1:$port" --tick-seconds 86400 >"$dir/out" 2>"$dir/log" &
pid=$!
trap 'kill $pid 2>/dev/null; rm -rf "$dir"' EXIT

for i in $(seq 101); do
  grep -q '^Seat tokens:' "$dir/out" && curl -s -o /dev/null "$api/mcp" && break
  if [ "$i" = 101 ] || ! kill -0 $pid 2>/dev/null; then
    echo "the world did not start on $api" >&2
    cat "$dir/log" >&2
    exit 1
  fi
  sleep 0.1
done
admin=$(sed -n 's/^Admin token (for \/api): //p' "$dir/out")
seats=$(sed -n '/^Seat tokens:/,/^$/p' "$dir/out" | tail -n +2)

mcp() {
  curl -sf "$api/mcp" -H 'content-type: application/json' \
    -H 'accept: application/json, text/event-stream' "$@"
}

# Calls one tool in a fresh MCP session and prints its result.
tool() {
  session=$(mcp -D - -o /dev/null -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"verify","version":"0"}}}' |
    tr -d '\r' | sed -n 's/^mcp-session-id: //Ip')
  mcp -H "mcp-session-id: $session" -o /dev/null \
    -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  mcp -H "mcp-session-id: $session" \
    -d "$(jq -nc --arg name "$1" --argjson args "$2" '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$name,arguments:$args}}')" |
    sed -n 's/^data: //p' | jq '.result.structuredContent // .result // .error'
}

if [ -n "$actor" ]; then
  token=$(awk -v s="$actor" '$1 == s { print $2 }' <<<"$seats")
  echo "== $actor acts"
  tool act "$(jq -nc --arg t "$token" --argjson o "$orders" '{seat_token:$t,tick:0,orders:$o}')"
fi

for _ in $(seq "$ticks"); do
  curl -sf -X POST -H "Authorization: Bearer $admin" "$api/api/tick" >/dev/null
done

echo "== recent chronicle after $ticks ticks (seed $seed); the full log is at /api/events"
curl -sf -H "Authorization: Bearer $admin" "$api/api/overview" | jq -r '.chronicle[]'

while read -r seat token _; do
  [ -n "$seat" ] || continue
  echo "== $seat sees"
  tool look "$(jq -nc --arg t "$token" '{seat_token:$t}')"
done <<<"$seats"
