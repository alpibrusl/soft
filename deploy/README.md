# Multi-process deployment

Each agent runs in its own OS process. Inter-agent communication is real
HTTP A2A — exactly the same wire format as cross-host deployment, just
all four processes happen to be on `127.0.0.1`.

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

> **Note on the "violations".** `soft-replay`'s `has_violation` predicate is
> `executed > allowed`, which is conservative: an *ungated* run (one with no
> `gate.verdict` events) gets flagged because every executed action lacks a
> corresponding `Allow` verdict. The deploy agents in `agents/*.lex` don't
> currently configure spec gates (the gate path is wired in our test
> harness, not in `agents/`). Treat the flag as informational here — it
> means "you ran without a gate," not "an action bypassed Deny." Wiring a
> real gate into agents/depot.lex would be a follow-up.

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
