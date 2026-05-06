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

## Driving a mock manually

### MCP Inspector (browser UI)

The fastest way to poke a mock by hand. The inspector spawns the mock as a stdio child and opens a browser tab where you can discover tools, call them, and see results.

```
npx @modelcontextprotocol/inspector python mocks/optimizer.py
```

### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "soft-optimizer": {
      "command": "python",
      "args": ["/absolute/path/to/soft/mocks/optimizer.py"]
    }
  }
}
```

Restart Claude Desktop; the mock's tools appear in the tool palette.

### Programmatic (Python)

```python
import asyncio
from mcp import ClientSession
from mcp.client.stdio import StdioServerParameters, stdio_client

async def main():
    params = StdioServerParameters(command="python", args=["mocks/optimizer.py"])
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            print(await session.list_tools())
            print(await session.call_tool(
                "schedule_session",
                {
                    "vehicle_id": "v-1",
                    "arrival_time": "2026-05-06T10:00:00",
                    "energy_kwh": 30.0,
                    "deadline": "2026-05-06T16:00:00",
                },
            ))

asyncio.run(main())
```

## Run from the demo runner (hypothetical, lands with `soft-agent`)

```
soft run-demo ../examples/phase1/ \
  --mcp optimizer="python mocks/optimizer.py" \
  --mcp ocpp="python mocks/ocpp.py" \
  --mcp telemetry="python mocks/telemetry.py" \
  --mcp tms_db="python mocks/tms_db.py" \
  --mcp local_planner="python mocks/local_planner.py"
```

## Tests

End-to-end smoke tests for each mock via the MCP stdio transport live in [`../tests/test_mocks.py`](../tests/test_mocks.py).

```
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt -r ../tests/requirements.txt
pytest
```

The suite spawns each mock as a subprocess, connects an MCP client, calls each tool, and asserts the response shape. Passes today (10/10).

## Replacing with real services

Each mock is a single file with no shared state and a small surface. Swapping for the real Python module is a per-server replacement: keep the tool signatures stable, change the implementation to call the real `scheduling-optimizer` / `ocpp` / `csms` / `telemetry` modules.
