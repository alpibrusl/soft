# Phase 1 mock MCP servers

Five Python MCP servers that simulate the cloud and edge services Phase 1 agents call. Deliberately unsophisticated — the goal is to give the agent runtime real MCP endpoints to exercise during the demo before any real Python service is plumbed in.

## Servers

| Server | Used by | Tools |
|---|---|---|
| `optimizer.py` | `depot-agent`, `tms-agent` | `schedule_session`, `plan_route` |
| `ocpp.py` | `depot-agent` | `remote_start_transaction`, `remote_stop_transaction`, `get_charger_status` |
| `telemetry.py` | `vehicle-agent` (and depots, for fleet view) | `get_vehicle_status`, `get_pv_output` |
| `tms_db.py` | `tms-agent` | `list_pending_deliveries`, `mark_delivery_status` |
| `local_planner.py` | `vehicle-agent` (edge) | `estimate_energy`, `find_charging_stop` |

## Setup

Requires Python 3.10+.

```
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
```

## Run a single mock (stdio)

```
python optimizer.py
```

You can drive it with [Claude Desktop](https://www.anthropic.com/claude) or any MCP client.

## Run from the demo runner (hypothetical, lands with `soft-agent`)

```
soft run-demo ../examples/phase1/ \
  --mcp optimizer="python mocks/optimizer.py" \
  --mcp ocpp="python mocks/ocpp.py" \
  --mcp telemetry="python mocks/telemetry.py" \
  --mcp tms_db="python mocks/tms_db.py" \
  --mcp local_planner="python mocks/local_planner.py"
```

## Replacing with real services

Each mock is a single file with no shared state and a small surface. Swapping for the real Python module is a per-server replacement: keep the tool signatures stable, change the implementation to call the real `scheduling-optimizer` / `ocpp` / `csms` / `telemetry` modules.
