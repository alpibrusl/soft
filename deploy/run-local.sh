#!/usr/bin/env bash
#
# Spins up the four-agent demo as four background processes. No
# orchestrator dependency (foreman / overmind / docker not required).
# Run from the repo root:
#
#   ./deploy/run-local.sh
#
# Sends a Dispatch to vehicle, sleeps a couple of seconds, gracefully
# shuts each agent down via `POST /shutdown`, then runs `soft-replay`
# against the store of accumulated per-agent traces.

set -euo pipefail

if [[ ! -x ./target/debug/soft-run ]]; then
  echo "building soft-run..."
  cargo build -p soft-runner
fi
if [[ ! -x ./target/debug/soft-replay ]]; then
  cargo build -p soft-agent --bin soft-replay
fi

LOG_DIR="${LOG_DIR:-/tmp/soft-deploy-$$}"
STORE_DIR="${STORE_DIR:-/tmp/soft-store-$$}"
mkdir -p "$LOG_DIR" "$STORE_DIR"
echo "logs in $LOG_DIR; store in $STORE_DIR"

# All four runners share the same store, so soft-replay sees every
# agent's per-run trace in one place at the end.

cleanup() {
  trap - EXIT INT TERM
  echo
  echo "=== requesting graceful shutdown ==="
  for port in 8001 8002 8003 8004; do
    curl -sf -X POST "http://127.0.0.1:${port}/shutdown" >/dev/null 2>&1 || true
  done
  # Give the runners a moment to finalize their traces before forcing.
  sleep 1.0
  for pid in "$tms_pid" "$depot_pid" "$vehicle_pid" "$pv_pid"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

./target/debug/soft-run agents/tms.lex --port 8003 \
  --store "$STORE_DIR" \
  --state-json '{"running":true}' \
  > "$LOG_DIR/tms.log" 2>&1 &
tms_pid=$!

./target/debug/soft-run agents/depot.lex --port 8002 \
  --peer vehicle=http://127.0.0.1:8001 \
  --store "$STORE_DIR" \
  --state-json '{"current_kw":30.0,"budget_kw":100.0,"pv_kw":0.0,"requested_kw":50.0}' \
  > "$LOG_DIR/depot.log" 2>&1 &
depot_pid=$!

./target/debug/soft-run agents/vehicle.lex --port 8001 \
  --peer depot=http://127.0.0.1:8002 \
  --peer depot2=http://127.0.0.1:8002 \
  --peer tms=http://127.0.0.1:8003 \
  --store "$STORE_DIR" \
  --state-json '{"soc":0.85,"reserve":0.20,"energy_needed":0.30,"tried":0}' \
  > "$LOG_DIR/vehicle.log" 2>&1 &
vehicle_pid=$!

./target/debug/soft-run agents/pv.lex --port 8004 \
  --peer depot=http://127.0.0.1:8002 \
  --tick Tick=2s \
  --store "$STORE_DIR" \
  --state-json '{"pv_kw":25}' \
  > "$LOG_DIR/pv.log" 2>&1 &
pv_pid=$!

sleep 0.5

echo "=== sending Dispatch to vehicle ==="
curl -sf -X POST http://127.0.0.1:8001/a2a/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "message_id":"m-1",
    "role":"user",
    "parts":[{"kind":"data","data":{"delivery_id":"d-1"}}],
    "metadata":{"from":"tester","topic":"Dispatch"}
  }'
echo

echo "=== sending RequestSession to depot (exercises grid_budget gate) ==="
curl -sf -X POST http://127.0.0.1:8002/a2a/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "message_id":"m-2",
    "role":"user",
    "parts":[{"kind":"data","data":{"vehicle_id":"v-1","power_kw":50}}],
    "metadata":{"from":"vehicle","topic":"RequestSession"}
  }'
echo
sleep 3

# `cleanup` (trap) shuts down via POST /shutdown so each runner finalizes
# its trace cleanly. Then run soft-replay against the shared store.
cleanup
trap - EXIT INT TERM

echo
echo "=== logs ==="
for name in vehicle depot tms pv; do
  echo "--- $name ---"
  cat "$LOG_DIR/$name.log"
  echo
done

echo "=== replay ==="
./target/debug/soft-replay "$STORE_DIR"
