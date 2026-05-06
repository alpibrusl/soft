# Phase 1 demo: a depot day on Lex

This directory sketches the Phase 1 demo described in `docs/proposal.md` §7.

> **Status.** lex-lang v0.2.0 ships the four primitives the agents call:
> `agent.local_complete`, `agent.cloud_complete`, `agent.send_a2a`,
> `agent.call_mcp` (the `[llm_local]`, `[llm_cloud]`, `[a2a]`, `[mcp]`
> typed builtins under `std.agent`). The handler-dispatch pattern shown
> in these `.lex` files (`agent.handle("Topic", fn)`, `agent.specs(...)`,
> `agent.mcp_servers(...)`, etc.) is provided by
> [`soft-agent`](../../crates/soft-agent) — that crate is the Phase 1
> implementation target. As written, these files describe what soft-agent
> will let agent authors write.

## Files

| File | Role |
|---|---|
| `messages.lex` | A2A topics and Part schemas shared across agents. |
| `vehicle.lex` | Edge agent. Runs on truck hardware. `[llm_local, mcp, a2a]`. |
| `depot.lex` | Cloud agent. Owns grid budget. `[llm_cloud, mcp, a2a]`. Granting sessions is depot-only via runtime registry scoping (lex 0.2 ships flat `[mcp]`). |
| `tms.lex` | Cloud agent. Dispatches deliveries; calls Python optimizer. `[llm_cloud, mcp, a2a, time]`. |
| `specs/grid_budget.spec` | Site grid budget never exceeded. |
| `specs/soc_reserve.spec` | Vehicle SoC never below configured reserve. |

## Read in this order

1. `messages.lex` — the shared vocabulary; note the typed Part shapes.
2. `vehicle.lex` — note the effect signature `[llm_local, mcp, a2a]`. Compile-time: cannot reach a cloud LLM. Runtime (via soft-agent's tool registry): MCP allowlist limited to `telemetry` and `local_planner`.
3. `depot.lex` — only this agent's MCP allowlist contains `ocpp`. Plus only this agent registers a `Topic.GrantSession` outgoing topic in soft-agent's dispatcher.
4. `specs/*.spec` — what `spec_checker::evaluate_gate_compiled` evaluates against each proposed action before the effect executes. Returns `GateVerdict::{Allow, Deny, Inconclusive}`; soft-agent treats `Inconclusive` as `Deny` by default and records the variant + reason in the trace.

## How it would run (target)

In-process, all three agents in one invocation:

```
soft run-demo examples/phase1/ \
  --duration 8h \
  --mcp optimizer="python mocks/optimizer.py" \
  --mcp ocpp="python mocks/ocpp.py" \
  --mcp telemetry="python mocks/telemetry.py" \
  --mcp tms_db="python mocks/tms_db.py" \
  --mcp local_planner="python mocks/local_planner.py" \
  --simulate-vehicles 5 \
  --simulate-chargers 3
```

Per-agent (cross-process, deferred to Phase 2):

```
soft run examples/phase1/depot.lex \
  --allow-effects llm_cloud,a2a,mcp \
  --allow-peers vehicle,tms \
  --peer vehicle=https://vehicle-1.fleet.local:8443/a2a \
  --peer tms=https://tms.example.com:443/a2a \
  --mcp ocpp="python mocks/ocpp.py" \
  --mcp optimizer="python mocks/optimizer.py"
```

The spec gate, the `agent.{local,cloud}_complete` call, and the trace
recording all happen inside the soft-agent runtime — handlers do not call
them directly.

## Headline deliverable

Replay the simulated day's trace from cloud + edge audit logs and prove
that no spec was ever violated.

Each agent run produces a trace at `<store>/traces/<run_id>/trace.json`.
Cross-store sync is a blob copy plus attestation entry; run IDs are
content-addressed so there's no merge step. `soft replay <run_id>`
reconstructs decisions across cloud and edge.
