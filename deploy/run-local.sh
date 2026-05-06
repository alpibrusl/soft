#!/usr/bin/env bash
#
# Spins up the four-agent demo as four background processes. No
# orchestrator dependency (foreman / overmind / docker not required).
# Run from the repo root:
#
#   ./deploy/run-local.sh
#
# Sends a Dispatch to vehicle, sleeps a couple of seconds, prints each
# agent's stderr, then tears everything down.

set -euo pipefail

if [[ ! -x ./target/debug/soft-run ]]; then
  echo "building soft-run..."
  cargo build -p soft-runner
fi

LOG_DIR="${LOG_DIR:-/tmp/soft-deploy-$$}"
mkdir -p "$LOG_DIR"
echo "logs in $LOG_DIR"

cleanup() {
  trap - EXIT INT TERM
  echo
  echo "=== shutting down ==="
  for pid in "$tms_pid" "$depot_pid" "$vehicle_pid" "$pv_pid"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

./target/debug/soft-run agents/tms.lex --port 8003 \
  --state-json '{"running":true}' \
  > "$LOG_DIR/tms.log" 2>&1 &
tms_pid=$!

./target/debug/soft-run agents/depot.lex --port 8002 \
  --peer vehicle=http://127.0.0.1:8001 \
  --state-json '{"current_kw":30,"budget_kw":100,"pv_kw":0,"requested_kw":50}' \
  > "$LOG_DIR/depot.log" 2>&1 &
depot_pid=$!

./target/debug/soft-run agents/vehicle.lex --port 8001 \
  --peer depot=http://127.0.0.1:8002 \
  --peer depot2=http://127.0.0.1:8002 \
  --peer tms=http://127.0.0.1:8003 \
  --state-json '{"soc":0.30,"reserve":0.20,"energy_needed":0.30,"tried":0}' \
  > "$LOG_DIR/vehicle.log" 2>&1 &
vehicle_pid=$!

./target/debug/soft-run agents/pv.lex --port 8004 \
  --peer depot=http://127.0.0.1:8002 \
  --tick Tick=2s \
  --state-json '{"pv_kw":25}' \
  > "$LOG_DIR/pv.log" 2>&1 &
pv_pid=$!

sleep 0.5

echo "=== sending Dispatch to vehicle ==="
curl -s -X POST http://127.0.0.1:8001/a2a/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "message_id":"m-1",
    "role":"user",
    "parts":[{"kind":"data","data":{"delivery_id":"d-1"}}],
    "metadata":{"from":"tester","topic":"Dispatch"}
  }'
echo
sleep 3

echo
echo "=== logs ==="
for name in vehicle depot tms pv; do
  echo "--- $name ---"
  cat "$LOG_DIR/$name.log"
  echo
done
