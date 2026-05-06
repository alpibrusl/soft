# Multi-process deployment

Each agent runs in its own OS process. Inter-agent communication is real
HTTP A2A — exactly the same wire format as cross-host deployment, just
all four processes happen to be on `127.0.0.1`.

The four processes:

| Agent | Port | Source | Peers |
|---|---|---|---|
| `tms`     | 8003 | `agents/tms.lex`     | (terminal sink) |
| `depot`   | 8002 | `agents/depot.lex`   | `vehicle=8001` |
| `vehicle` | 8001 | `agents/vehicle.lex` | `depot=8002`, `depot2=8002` (same target — falls back through the Lex `tried` counter), `tms=8003` |
| `pv`      | 8004 | `agents/pv.lex`      | `depot=8002`; ticks every `2s` to broadcast `PvUpdate` |

## Quickest path

```bash
./deploy/run-local.sh
```

Builds `soft-run` if needed, spawns all four agents in the background,
sends a Dispatch to vehicle, prints each agent's log, then tears down.
Output looks like:

```
=== sending Dispatch to vehicle ===
{"accepted":"m-1"}

=== logs ===
--- vehicle ---
soft-run: agent `vehicle` on http://127.0.0.1:8001  peers={...}
[vehicle] step: allowed=1 denied=0    # vehicle sends RequestSession to depot
[vehicle] step: allowed=1 denied=0    # vehicle receives GrantSession, sends Complete to tms

--- depot ---
soft-run: agent `depot` on http://127.0.0.1:8002  peers={vehicle=>http://127.0.0.1:8001}
[depot] step: allowed=1 denied=0       # depot grants the session

--- tms ---
[tms] step: allowed=0 denied=0          # terminal — no further action
...
```

## Running by hand

```bash
cargo build -p soft-runner

# One terminal each.
./target/debug/soft-run agents/tms.lex     --port 8003 --state-json '{"running":true}'
./target/debug/soft-run agents/depot.lex   --port 8002 --peer vehicle=http://127.0.0.1:8001 --state-json '{"current_kw":30,"budget_kw":100,"pv_kw":0,"requested_kw":50}'
./target/debug/soft-run agents/vehicle.lex --port 8001 --peer depot=http://127.0.0.1:8002 --peer depot2=http://127.0.0.1:8002 --peer tms=http://127.0.0.1:8003 --state-json '{"soc":0.30,"reserve":0.20,"energy_needed":0.30,"tried":0}'
./target/debug/soft-run agents/pv.lex      --port 8004 --peer depot=http://127.0.0.1:8002 --tick Tick=2s --state-json '{"pv_kw":25}'
```

Then trigger the flow:

```bash
curl -X POST http://127.0.0.1:8001/a2a/messages \
  -H 'Content-Type: application/json' \
  -d '{
    "message_id":"m-1",
    "role":"user",
    "parts":[{"kind":"data","data":{"delivery_id":"d-1"}}],
    "metadata":{"from":"tester","topic":"Dispatch"}
  }'
```

Or fetch any agent's card:

```bash
curl http://127.0.0.1:8002/a2a/agent-card | jq
```

## With foreman / overmind

```bash
foreman  start -f deploy/Procfile
overmind start -f deploy/Procfile
```

Same effect, with one log per process.

## CLI reference (`soft-run`)

```text
soft-run <agent.lex> --port <N>
    [--bind <addr>]                   # default 127.0.0.1
    [--peer <name>=<url>]...          # e.g. --peer depot=http://localhost:8002
    [--tick <topic>=<duration>]...    # e.g. --tick Tick=2s   (units: ms|s|m)
    [--state-json <json>]             # initial state for the agent
```

The `<agent.lex>` file must define a top-level `fn config() -> AgentConfig`
using the soft-agent DSL (`agent_new`, `agent_peers`, `agent_handles`, ...).
The DSL preamble is auto-prepended.

## Why no Docker yet

Docker / `docker-compose.yml` is the obvious next layer once the binary
is solid; it'd be a one-screen file pointing at this same binary plus
mounting `agents/` into each container. We're holding off until there's
a concrete reason to take the container hit (CI, multi-host deployment,
production parity). Native processes are faster to iterate against, and
the same A2A wire is exercised either way.

Sketch for when we want it (`deploy/docker-compose.yml`):

```yaml
services:
  vehicle:
    build: .
    command: ["soft-run", "agents/vehicle.lex", "--port", "8001",
              "--peer", "depot=http://depot:8002",
              "--peer", "tms=http://tms:8003",
              "--state-json", '{"soc":0.30,"reserve":0.20,"energy_needed":0.30,"tried":0}']
    ports: ["8001:8001"]
  depot: # ...same shape
  tms:   # ...
  pv:    # ...with --tick Tick=2s
```

A 30-line follow-up when needed.
