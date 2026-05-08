# Multi-process deployment

Each agent runs in its own OS process. Inter-agent communication is real
HTTP A2A — exactly the same wire format as cross-host deployment, just
all four processes happen to be on `127.0.0.1` (or, with compose, on
service-name DNS inside a container network).

## Docker Compose (recommended for cross-host realism)

```bash
docker compose up --build
```

Brings up vehicle / depot / tms / pv plus a `trace-viewer` companion.
Peers resolve via service-name DNS (`http://depot:8002` etc.). Each
service has a `/healthz` healthcheck and a 5s graceful-shutdown grace
period, so `docker compose down` flushes traces correctly via
`POST /shutdown` (the runner picks up SIGTERM where the platform
delivers it; the orchestrator sends `/shutdown` as a fallback).

| Service | Host port | Notes |
|---|---|---|
| `vehicle`       | 8001 | A2A + `/healthz` + `/metrics` |
| `depot`         | 8002 | same |
| `tms`           | 8003 | same |
| `pv`            | 8004 | same; ticks `2s` |
| `trace-viewer`  | 8080 | HTML browser for the shared `/traces` volume |

`SOFT_SHUTDOWN_TOKEN` defaults to `localdev` — change it when binding
non-loopback. `ANTHROPIC_API_KEY` is forwarded through if set in your
shell env.

```bash
docker compose down --timeout 5    # graceful, traces flush
```

The four processes:

| Agent | Port | Source | Peers |
|---|---|---|---|
| `tms`     | 8003 | `agents/tms.lex`     | (terminal sink) |
| `depot`   | 8002 | `agents/depot.lex`   | `vehicle=8001`; receives `RequestSession` and `PvUpdate` |
| `vehicle` | 8001 | `agents/vehicle.lex` | `depot=8002`, `depot2=8002` (same target — falls back through the Lex `tried` counter), `tms=8003` |
| `pv`      | 8004 | `agents/pv.lex`      | `depot=8002`; ticks every `2s` to broadcast `PvUpdate` |

## Quickest path

```bash
./deploy/run-local.sh
```

Builds `soft-run` and `soft-replay` if needed, spawns all four agents in
the background with a shared trace store, sends a `Dispatch` to vehicle,
gracefully shuts each process down via `POST /shutdown`, then runs
`soft-replay` against the per-agent traces. Output looks like:

```
=== sending Dispatch to vehicle ===
{"accepted":"m-1"}

=== requesting graceful shutdown ===

=== logs ===
[vehicle] step: allowed=1 denied=0     # vehicle sends RequestSession to depot
[vehicle] step: allowed=1 denied=0     # vehicle receives GrantSession, sends Complete to tms
[vehicle] shutdown signal received; finalizing trace
[vehicle] trace saved: /tmp/soft-store-NNN/traces/<run_id>/trace.json
[depot]   step: allowed=1 denied=0     # depot grants the session
[depot]   step: allowed=0 denied=0     # depot's on_pv_update mutates state, no actions
[tms]     step: allowed=0 denied=0     # terminal
[pv]      step: allowed=1 denied=0     # tick → SendA2a("depot","PvUpdate")

=== replay ===
run_id...         proposed  allowed  denied  inconclusive  executed  skipped
<...>                   2        0       0             0         2        0   ← VIOLATION: executed > allowed
```

> **Note on ungated agents.** `soft-replay` only flags runs as violations
> when the gate was actually invoked (i.e. some `gate.verdict` event was
> recorded) AND `executed > allowed`. Runs without a configured spec gate
> — like the pv agent, which just emits ticks — get a separate
> `(ungated — no spec gate)` annotation but are not counted as
> violations: there's no Deny to bypass. See
> [`Counts::is_gated`](../crates/soft-agent/src/replay.rs) for the
> precise predicate.

## Running by hand

```bash
cargo build -p soft-runner
cargo build -p soft-agent --bin soft-replay

# One terminal each. --store is optional but recommended.
./target/debug/soft-run agents/tms.lex     --port 8003 --store /tmp/soft-store --state-json '{"running":true}'
./target/debug/soft-run agents/depot.lex   --port 8002 --store /tmp/soft-store --peer vehicle=http://127.0.0.1:8001 --state-json '{"current_kw":30.0,"budget_kw":100.0,"pv_kw":0.0,"requested_kw":50.0}'
./target/debug/soft-run agents/vehicle.lex --port 8001 --store /tmp/soft-store --peer depot=http://127.0.0.1:8002 --peer depot2=http://127.0.0.1:8002 --peer tms=http://127.0.0.1:8003 --state-json '{"soc":0.30,"reserve":0.20,"energy_needed":0.30,"tried":0}'
./target/debug/soft-run agents/pv.lex      --port 8004 --store /tmp/soft-store --peer depot=http://127.0.0.1:8002 --tick Tick=2s --state-json '{"pv_kw":25.0}'
```

Trigger the flow:

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

Stop each agent gracefully (finalises its trace before exiting):

```bash
curl -X POST http://127.0.0.1:8001/shutdown
curl -X POST http://127.0.0.1:8002/shutdown
curl -X POST http://127.0.0.1:8003/shutdown
curl -X POST http://127.0.0.1:8004/shutdown
```

Replay all four traces:

```bash
./target/debug/soft-replay /tmp/soft-store
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

Same effect, with one log per process. Procfile uses `--store /tmp/soft-store`.

## CLI reference (`soft-run`)

```text
soft-run <agent.lex> --port <N>
    [--bind <addr>]                    default 127.0.0.1
    [--peer <name>=<url>]...           e.g. --peer depot=http://localhost:8002
    [--tick <topic>=<duration>]...     e.g. --tick Tick=2s   (units: ms|s|m)
    [--state-json <json>]              initial state for the agent
    [--store <path>]                   if set, finalize the trace here on shutdown
    [--llm-cloud-provider <name>]      `default` (OpenAI-shape, lex-runtime builtin)
                                       or `anthropic` (custom EffectHandler)
    [--shutdown-token <T>]             or SOFT_SHUTDOWN_TOKEN env. Required
                                       when --bind is non-loopback. When set,
                                       POST /shutdown requires a matching
                                       X-Shutdown-Token header.
```

The `<agent.lex>` file must define a top-level `fn config() -> AgentConfig`
using the soft-agent DSL (`agent_new`, `agent_peers`, `agent_handles`, ...).
The DSL preamble is auto-prepended.

### Graceful shutdown

Two paths, in priority order:

1. **`POST /shutdown`** — the reliable orchestrator-driven path. Each
   `soft-run` exposes this on its A2A port and finalises its trace
   before exiting. Use this in Docker `preStop` hooks, Kubernetes
   `preStop` lifecycle, Procfile teardown scripts.
2. **SIGINT / SIGTERM** — best-effort `ctrlc` handler. Works in real
   terminals; some sandboxed parents intercept signals before they
   reach our process. If the trace doesn't get persisted, fall back to
   the `POST /shutdown` path.

`POST /shutdown` has no auth by default — safe only because the
default `--bind` is `127.0.0.1`. If you bind to a non-loopback
interface (e.g. `0.0.0.0` for cross-host A2A), `soft-run` will refuse
to start without a `--shutdown-token` (or `SOFT_SHUTDOWN_TOKEN` env).
Callers must then send `X-Shutdown-Token: <T>`:

```bash
curl -X POST -H "X-Shutdown-Token: $TOKEN" http://host:port/shutdown
```

## Observability

Every `soft-run` exposes two extra routes alongside its A2A endpoints:

- **`GET /healthz`** — `{"ok": true}`. Cheap liveness check; used by
  the compose healthchecks. Doesn't depend on the mailbox or executor.
- **`GET /metrics`** — Prometheus text exposition format. Counters:
  - `soft_messages_received_total{topic}`
  - `soft_actions_proposed_total{kind}`
  - `soft_actions_allowed_total{kind}` (passed gate + executor accepted)
  - `soft_actions_denied_total{kind, reason}` (`reason` ∈ `gate`, `mcp_allowlist`)
  - `soft_gate_verdict_total{verdict}` (`Allow`, `Deny`, `Inconclusive`)
  - `soft_tick_fires_total{topic}`
  - `soft_steps_total{outcome}` (`Idle`, `Processed`, `Error`)

Wire a Prometheus scraper at `http://<host>:<port>/metrics`. The agent
name appears as a single `# HELP soft_agent <name>` comment so you can
disambiguate scrapes when service-discovery doesn't already label them.

## Trace viewer (`soft-trace-viewer`)

A small standalone binary that serves the persisted `lex-store` traces
as an interactive HTML page.

```bash
cargo run -p soft-trace-viewer -- --store /tmp/soft-store --bind 127.0.0.1:8080
# open http://127.0.0.1:8080/
```

In compose it runs alongside the agents on port `8080`, mounted on the
shared `traces` volume — clicking a run-id in the sidebar loads the
full call/effect tree for that agent's run, with collapsible sections
per `Call` / `Effect` node and JSON-pretty-printed args/results. No JS
framework, ~200 lines of vanilla.

API surface (used by the embedded HTML):

| Method | Path | Body |
|---|---|---|
| GET | `/`               | `index.html` |
| GET | `/api/traces`     | `{"traces": ["<run-id>", ...]}` |
| GET | `/api/trace/<id>` | full `TraceTree` JSON |

## Driving an LLM-backed handler

`agent.local_complete` and `agent.cloud_complete` are real HTTP calls in
lex-lang v0.2.2. Two providers shipped end-to-end today:

### Default (OpenAI-shape, ships in lex-runtime)

```bash
# Pull a local model first.
ollama serve &
ollama pull llama3
export OLLAMA_HOST=http://localhost:11434
export LEX_LLM_LOCAL_MODEL=llama3

# OR for cloud (anything OpenAI-compatible: OpenAI, Mistral, Groq, ...).
export LEX_LLM_CLOUD_BASE_URL=https://api.openai.com
export LEX_LLM_CLOUD_API_KEY=sk-...
export LEX_LLM_CLOUD_MODEL=gpt-4o-mini

./target/debug/soft-run agents/llm_driver.lex --port 8005 \
  --peer logger=http://127.0.0.1:8003 \
  --state-json '{"prompt":"Should we accept this delivery?"}'
```

Send a `Decide`:

```bash
curl -X POST http://127.0.0.1:8005/a2a/messages \
  -H 'Content-Type: application/json' \
  -d '{"message_id":"m-1","role":"user","parts":[{"kind":"data","data":{}}],"metadata":{"from":"tester","topic":"Decide"}}'
```

The `agent.local_complete` (or `cloud_complete`) call hits the
configured backend; the resulting text is forwarded as a `SendA2a` to
the `logger` peer. With Ollama not running, the call returns
`Err(...)` and the handler's `unwrap_or` falls back gracefully.

### Anthropic (custom EffectHandler in soft-runner)

`agent.cloud_complete` is OpenAI-shape upstream; `--llm-cloud-provider
anthropic` swaps in a custom `EffectHandler` (in
`crates/soft-runner/src/anthropic.rs`) that calls Anthropic's
`/v1/messages` API instead. Get an API key from
[Anthropic Console](https://console.anthropic.com/) (separate from a
Claude.ai or Claude Code subscription), then:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
# Optional: ANTHROPIC_BASE_URL, ANTHROPIC_MODEL (default claude-3-5-sonnet),
#           ANTHROPIC_MAX_TOKENS (default 1024), ANTHROPIC_VERSION

./target/debug/soft-run agents/llm_driver.lex --port 8005 \
  --llm-cloud-provider anthropic \
  --peer logger=http://127.0.0.1:8003 \
  --state-json '{"prompt":"Should we accept this delivery?"}'
```

Same `Decide` curl as above. Now `agent.cloud_complete(...)` routes
through Anthropic's Messages API; the assistant's text comes back as
the `Result[Str, Str]::Ok(text)` value.

The wrapper falls through to lex-runtime's `DefaultHandler` for every
other effect (`agent.local_complete`, `agent.send_a2a`,
`agent.call_mcp`), so an agent can mix Anthropic for cloud LLM with
Ollama for local LLM and the lex-runtime default for everything else.

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
              "--store", "/data/store",
              "--peer", "depot=http://depot:8002",
              "--peer", "tms=http://tms:8003",
              "--state-json", '{"soc":0.30,"reserve":0.20,"energy_needed":0.30,"tried":0}']
    stop_grace_period: 5s   # gives /shutdown enough time to finalize
    ports: ["8001:8001"]
    volumes: ["./data:/data"]
  depot: # ...same shape
  tms:   # ...
  pv:    # ...with --tick Tick=2s
```

A 30-line follow-up when needed.
