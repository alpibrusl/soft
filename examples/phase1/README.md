# Phase 1 demo: a depot day on Lex

This directory sketches the Phase 1 demo described in `docs/proposal.md` §7.

> **Illustrative.** The APIs shown (`agent.*`, `a2a.*`, `llm.*`, the `spec`
> form) do not exist in Lex yet — they correspond to the six issues drafted
> in proposal §11. Read this directory as the concrete target those issues
> are working toward, not as runnable code.

## Files

| File | Role |
|---|---|
| `messages.lex` | A2A topics and Part schemas shared across agents. |
| `vehicle.lex` | Edge agent. Runs on truck hardware. `[llm-local]`. |
| `depot.lex` | Cloud agent. Owns grid budget. Only agent with `mcp(ocpp)`. |
| `tms.lex` | Cloud agent. Dispatches deliveries; calls Python optimizer. |
| `specs/grid_budget.spec` | Site grid budget never exceeded. |
| `specs/soc_reserve.spec` | Vehicle SoC never below configured reserve. |

## Read in this order

1. `messages.lex` — the shared vocabulary; note the typed Part shapes.
2. `vehicle.lex` — note the effect signature. The agent *cannot* call a cloud
   LLM, *cannot* talk to OCPP, and *cannot* grant a charging session. The
   type system blocks all three.
3. `depot.lex` — note the capability separation: only this agent carries
   `mcp(ocpp)` and the `Topic.GrantSession` emit capability.
4. `specs/*.spec` — what the runtime gate evaluates against each proposed
   action before the effect executes.

## How it would run (target)

In-process, all three agents in one invocation:

```
soft run-demo examples/phase1/ \
  --duration 8h \
  --mcp optimizer="python -m soft.bridge.optimizer" \
  --mcp ocpp="python -m soft.bridge.ocpp" \
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
  --mcp ocpp=https://ocpp.local:9000 \
  --mcp optimizer=https://opt.local:9001
```

The spec gate, the LLM call, and the trace recording all happen inside the
`agent` runtime — handlers do not call them directly.

## Headline deliverable

Replay the simulated day's trace from cloud + edge audit logs and prove
that no spec was ever violated.

The trace is a `lex-vcs` branch per process; cloud audit is the merge of
all of them. `soft replay <trace-id>` reconstructs decisions across cloud
and edge.
