# soft

A runtime for autonomous LLM-driven agents that **earn their authority**:
every outbound action passes through a spec gate before execution, every
gate verdict is recorded in a deterministic trace, and the trace is
replayable as a post-hoc audit.

Built on [lex-lang](https://github.com/alpibrusl/lex-lang) — agents are
written in Lex (typed effects, structural records, runtime spec
checking), driven from a small Rust runner.

> **Status.** Phase 1 is shippable: a four-agent EV-charging fleet
> demo (vehicle / depot / tms / pv) runs end-to-end via either
> `deploy/run-local.sh` or `docker compose up`. Workspace pinned to
> lex-lang 0.3. CI exercises `cargo {fmt,clippy,test}` plus a docker
> build. See [the proposal](docs/proposal.md) for the longer arc.

## How it fits together

```
┌────────────────────────────┐
│  vehicle.lex     pv.lex    │   ← agent definitions in Lex (DSL on
│  depot.lex       tms.lex   │     top of lex-lang) + sibling .spec
│                            │     files for runtime invariants
└──────────────┬─────────────┘
               │ compiled by soft-agent
               ▼
┌────────────────────────────┐
│  soft-run <agent.lex>      │   ← one OS process per agent. Real
│   ├─ A2A HTTP listener     │     A2A HTTP between processes (same
│   │   /a2a/messages        │     wire format whether colocated or
│   │   /a2a/agent-card      │     cross-host).
│   │   /healthz  /metrics   │
│   ├─ mailbox + Runner      │
│   ├─ spec gate (lex-3 #208)│   ← every action checked against
│   ├─ Prometheus counters   │     a record-typed invariant
│   │   + latency histograms │
│   └─ lex-store trace sink  │
└──────────────┬─────────────┘
               │ /traces volume
               ▼
┌────────────────────────────┐
│  soft-trace-viewer         │   ← collapsible HTML tree per run
│  http://…:8080/            │
└────────────────────────────┘
```

## Quickstart

### Local (no containers)

```bash
./deploy/run-local.sh
```

Builds `soft-run` + `soft-replay`, brings up the four-agent fleet on
`127.0.0.1:8001..8004` with a shared trace store under `/tmp/soft-store-…`,
sends a `Dispatch` to vehicle and a `RequestSession` to depot, gracefully
shuts down via `POST /shutdown`, and replays the persisted traces.
Expect the run to end with `4 trace(s) replayed; no violations.`

### Docker compose

```bash
docker compose up --build
```

Brings up the same fleet plus a trace viewer container. Service-name
DNS is the only difference: peer URLs become `http://depot:8002` etc.
instead of localhost. Ports:

| Service        | URL                      |
|---              |---                       |
| vehicle        | http://localhost:8001    |
| depot          | http://localhost:8002    |
| tms            | http://localhost:8003    |
| pv             | http://localhost:8004    |
| trace-viewer   | http://localhost:8080    |

```bash
docker compose down --timeout 5    # graceful shutdown, traces flush
```

### One agent by hand

```bash
cargo build -p soft-runner
./target/debug/soft-run agents/depot.lex \
  --port 8002 \
  --state-json '{"current_kw":30.0,"budget_kw":100.0,"pv_kw":0.0,"requested_kw":50.0}'

curl -fsS http://127.0.0.1:8002/a2a/agent-card | jq
curl -fsS http://127.0.0.1:8002/healthz
curl -fsS http://127.0.0.1:8002/metrics | head -20
```

Full CLI reference + the LLM-driven agent recipe: [`deploy/README.md`](deploy/README.md).

## The agents (`agents/`)

| File                  | Role                                                                                |
|---                    |---                                                                                  |
| `vehicle.lex`         | Autonomous truck. Receives Dispatch; falls over to a backup depot on first denial.  |
| `depot.lex`           | Charging depot. Owns its grid budget. Receives `PvUpdate` ticks from the pv agent.  |
| `tms.lex`             | Transport-management sink. Receives `Complete`/`Failed` from vehicle.               |
| `pv.lex`              | PV (solar) feed. Ticks every `2s` and broadcasts `PvUpdate` to depot.               |
| `llm_driver.lex`      | Demo: a Lex handler that calls `agent.local_complete` (Ollama) and forwards out.    |
| `depot.spec`          | Grid-budget invariant: `current_kw + power_kw ≤ budget_kw + pv_kw`.                 |
| `vehicle.spec`        | SOC-reserve invariant: `soc - energy_needed ≥ reserve`.                             |

Spec quantifiers use lex 0.3 record types and field access, so a
single binding-fn forwards state and action-payload JSON straight
through:

```text
spec depot_grid_budget {
  forall s :: { current_kw :: Float, budget_kw :: Float, pv_kw :: Float },
         a :: { power_kw :: Float }:
    s.current_kw + a.power_kw <= s.budget_kw + s.pv_kw
}
```

## Crates (`crates/`)

| Crate                | Binary               | What it does                                                              |
|---                   |---                   |---                                                                        |
| `soft-agent`         | `soft-replay`        | Core runtime: Agent, Mailbox, Runner, Gate, BindingsFn, Metrics, Trace.   |
| `soft-a2a`           | —                    | HTTP wire for inter-agent messaging + `/healthz`, `/metrics`, `/shutdown`.|
| `soft-runner`        | `soft-run`           | CLI launcher: one process per `.lex` agent.                               |
| `soft-trace-viewer`  | `soft-trace-viewer`  | Tiny HTML viewer over the persisted trace store.                          |

Per-crate detail: [`docs/crates/`](docs/crates/).

## Observability

Every `soft-run` exposes Prometheus on its A2A port:

- **Counters**: `soft_messages_received_total`, `soft_actions_proposed_total`,
  `soft_actions_allowed_total`, `soft_actions_denied_total{reason=gate|mcp_allowlist}`,
  `soft_gate_verdict_total`, `soft_tick_fires_total`, `soft_steps_total`.
- **Histograms** (`*_seconds`, standard latency buckets): `soft_step_duration_seconds`,
  `soft_action_execute_duration_seconds`.

`soft-trace-viewer` browses persisted runs from a shared `/traces`
volume. Click a run id to expand the call/effect tree.

## Documentation

| File                                                | Topic                                                       |
|---                                                  |---                                                          |
| [`docs/proposal.md`](docs/proposal.md)              | Project goals, scope, the audit story.                      |
| [`deploy/README.md`](deploy/README.md)              | Multi-process deploy, full CLI ref, LLM-driven recipe.      |
| [`docs/architecture/lex-vs-rust.md`](docs/architecture/lex-vs-rust.md) | Where the boundary sits and why.       |
| [`docs/lex-lang-0.3-upgrade.md`](docs/lex-lang-0.3-upgrade.md) | Upstream 0.3 audit + what soft adopted.       |
| [`docs/lex-lang/v0.2.2-followup.md`](docs/lex-lang/v0.2.2-followup.md) | Follow-up notes from the 0.2.2 cycle.      |
| [`docs/lex-lang/issues/`](docs/lex-lang/issues/)    | Filed and drafted lex-lang asks.                            |

## Building

Requires Rust 1.80+ (workspace MSRV). Standard cargo:

```bash
cargo build --workspace --all-targets
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

CI runs the same set on every PR plus a `docker build` smoke. See
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## License

[EUPL-1.2](https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
