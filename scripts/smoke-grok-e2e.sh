#!/usr/bin/env bash
# End-to-end smoke: cocli Channel → Agent(grok) → real Runtime driver.
# Uses the local grok CLI discovered on PATH (not --fake-runtime).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${COCLI_BIN:-$ROOT/target/debug/cocli}"
PORT="${COCLI_SMOKE_PORT:-18090}"
BASE="http://127.0.0.1:${PORT}"
DATA="${COCLI_SMOKE_DATA:-$(mktemp -d -t cocli-grok-smoke.XXXXXX)}"
# Empty model = let cocli/server pick the first discovered launchable model.
MODEL="${COCLI_GROK_MODEL:-}"
PROMPT="${COCLI_SMOKE_PROMPT:-You are working inside the cocli repository. Reply in one short sentence confirming you can see this is the cocli multi-agent project and that Channel→Agent delivery works.}"
TIMEOUT_SEC="${COCLI_SMOKE_TIMEOUT:-180}"

if [[ ! -x "$BIN" ]]; then
  echo "building cocli…"
  (cd "$ROOT" && cargo build -p cocli --bin cocli)
fi

if ! command -v grok >/dev/null 2>&1; then
  echo "grok CLI not found on PATH" >&2
  exit 1
fi

echo "data dir: $DATA"
echo "bind:     $BASE"
echo "model:    ${MODEL:-<discovered first>}"

"$BIN" --bind "127.0.0.1:${PORT}" --data-dir "$DATA" >"$DATA/server.log" 2>&1 &
PID=$!
cleanup() {
  if kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

for _ in $(seq 1 80); do
  if curl -sf "$BASE/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
curl -sf "$BASE/healthz" >/dev/null

echo "— runtimes —"
RUNTIMES=$(curl -sf "$BASE/api/runtimes")
echo "$RUNTIMES" | python3 -c 'import json,sys; data=json.load(sys.stdin); grok=[r for r in data if r["name"]=="grok"];
print(json.dumps(grok[0] if grok else {"error":"grok missing","all":[r["name"] for r in data]}, indent=2));
assert grok and grok[0].get("installed"), "grok runtime not installed/discovered";
models=grok[0].get("models") or [];
assert models, "grok models list empty";
assert models[0] not in ("grok-composer-2.5-fast",), "stale grok model list: %r" % (models,)'
DISCOVERED_MODEL=$(echo "$RUNTIMES" | python3 -c 'import json,sys; data=json.load(sys.stdin); grok=next(r for r in data if r["name"]=="grok"); print((grok.get("models") or [""])[0])')
if [[ -z "$MODEL" ]]; then
  MODEL="$DISCOVERED_MODEL"
fi
echo "using model: $MODEL"

CHANNEL=$(curl -sf -X POST "$BASE/api/channels" \
  -H 'content-type: application/json' \
  -d '{"name":"grok-smoke","description":"real Runtime e2e"}')
CHANNEL_ID=$(echo "$CHANNEL" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
echo "channel: $CHANNEL_ID"

AGENT=$(curl -sf -X POST "$BASE/api/agents" \
  -H 'content-type: application/json' \
  -d "{\"channel_id\":\"$CHANNEL_ID\",\"name\":\"grok-bootstrap\",\"runtime\":\"grok\",\"model\":\"$MODEL\",\"instructions\":\"Prefer short answers. You are dogfooding cocli.\"}")
AGENT_ID=$(echo "$AGENT" | python3 -c 'import json,sys; a=json.load(sys.stdin); print(a["id"]); assert a["status"]=="running" and a["lifecycle_status"]=="active"')
echo "agent:   $AGENT_ID"

POST=$(curl -sf -X POST "$BASE/api/channels/${CHANNEL_ID}/messages" \
  -H 'content-type: application/json' \
  -d "$(python3 -c 'import json,sys; print(json.dumps({"content":sys.argv[1]}))' "$PROMPT")")
echo "— post response (truncated) —"
echo "$POST" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("replies", len(d.get("replies") or [])); print("pending", [(p.get("state"), p.get("attempts")) for p in d.get("pending_deliveries") or []])'

MSG_ID=$(echo "$POST" | python3 -c 'import json,sys; print(json.load(sys.stdin)["message"]["id"])')
echo "message: $MSG_ID"

deadline=$((SECONDS + TIMEOUT_SEC))
while (( SECONDS < deadline )); do
  MSGS=$(curl -sf "$BASE/api/channels/${CHANNEL_ID}/messages")
  COUNT=$(echo "$MSGS" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
  if (( COUNT >= 2 )); then
    echo "— messages —"
    echo "$MSGS" | python3 -c 'import json,sys; 
for m in json.load(sys.stdin):
  role=m["role"]; content=(m.get("content") or "").replace("\n"," ")[:200]
  print(f"{role}: {content}")'
    ASSISTANT=$(echo "$MSGS" | python3 -c 'import json,sys; ms=json.load(sys.stdin); print(next((m["content"] for m in ms if m["role"]=="assistant"), ""))')
    if [[ -n "$ASSISTANT" ]]; then
      STATS=$(curl -sf "$BASE/api/deliveries/stats")
      echo "— delivery stats —"
      echo "$STATS"
      echo "PASS: grok Runtime delivered an assistant reply"
      exit 0
    fi
  fi
  sleep 1
done

echo "FAIL: timed out waiting for grok reply" >&2
echo "— server log (tail) —" >&2
tail -80 "$DATA/server.log" >&2 || true
echo "— messages so far —" >&2
curl -sf "$BASE/api/channels/${CHANNEL_ID}/messages" >&2 || true
echo >&2
exit 1
