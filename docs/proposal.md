# Soft: Capability-Bounded LLM Agents on Lex

**Status:** Phase 1 plan, building against [lex-lang v0.2.0](https://crates.io/crates/lex-runtime).
**Supersedes:** `alpibrusl/lex-lang:docs/proposals/soft.md` (earlier draft on the `claude/review-agent-vcs-architecture-9EZ74` branch).
**Branch:** `claude/review-soft-proposal-fubbK` in `alpibrusl/soft`.

## 1. Context

We operate a set of working Python modules in e-mobility logistics:

- `ocpi` — eMobility roaming protocol.
- `ocpp` — charge point communication.
- `csms` — charging station management.
- `scheduling-optimizer` — route and dispatch optimization.
- `telemetry` — vehicle and PV ingestion.

Today these modules stream data into a central system that holds all coordination decisions. The research project this proposal supports asks the inverse question: can coordination be lifted out of the centre into autonomous agents that talk to each other and call the existing modules as bounded capabilities?

Examples of decisions to lift:

- A vehicle requests a route when it needs one, or asks a depot for a charging session at a particular location.
- A depot accepts or rejects a charging session against its local grid budget.
- A TMS agent contacts a truck when a delivery needs reassignment.

The dogfooding goal is to build this in Lex and use the exercise to validate (and where necessary, extend) Lex itself.

## 2. What this is proving

The original soft draft framed the value as **"Lex's effect system plus verifier statically prove agent properties."** That framing assumes typed messages and formal pre-conditions on payloads, which does not fit LLM-driven agents whose messages are intent-shaped rather than type-shaped.

This draft reframes the value as:

> **Lex's effect system, runtime gates, and `lex-trace` replay keep LLM agents from doing anything they shouldn't — with full audit across cloud and edge.**

The LLM proposes; Lex disposes; the trace replays. That is a more honest claim for a real 2026 deployment, and it lines up with what Lex already ships.

## 3. Architecture

Three layers.

### 3.1 Python data plane (unchanged)

`ocpi`, `ocpp`, `csms`, `telemetry`, `scheduling-optimizer` keep speaking their wire protocols. Each is exposed as an MCP server. Agents do not embed protocol logic and do not replace these modules.

### 3.2 Soft agent layer (new)

LLM-driven agents that own the coordination decisions currently centralised:

| Agent | Where | Purpose |
|---|---|---|
| `vehicle-agent` | edge (per truck) | Requests routes; requests charging sessions; reports state. |
| `depot-agent` | cloud (per site) | Owns local grid budget; accepts/rejects sessions; schedules across chargers. |
| `tms-agent` | cloud | Dispatches deliveries; queries vehicles; reassigns when needed. |
| `pv-agent` | cloud (per site, optional) | Negotiates PV availability with the depot. |

Each agent has a system prompt, a goal, a declared peer list, a declared MCP allowlist (resolved by soft-agent's runtime registry), and an effect signature. The LLM interprets incoming intents and proposes the next action. Lex then decides whether the action is allowed.

The handler-dispatch layer (`agent.handle("Topic", fn)`) lives in `soft-agent`. The four typed primitives the handlers ultimately call — `agent.local_complete`, `agent.cloud_complete`, `agent.send_a2a`, `agent.call_mcp` — ship in lex-lang v0.2.0's `std.agent` module.

### 3.3 Lex runtime (extended, shipped in v0.2.0)

The substrate that surrounds the agent layer:

- **Capability types** decide which top-level effects an agent can use: `[llm_local]` xor `[llm_cloud]`, plus `[mcp]` and `[a2a]` if needed. Per-server MCP scoping is a soft-agent runtime check (parameterized effects like `[mcp(ocpp)]` are a possible future upstream ask).
- **Effect gates** intercept every proposed action; `spec_checker::evaluate_gate_compiled` runs as a runtime invariant returning `Allow | Deny | Inconclusive`. soft-agent records the verdict variant + spec name + reason to `lex_trace::Recorder`, and treats `Inconclusive` as `Deny` by default so audit replay can distinguish "couldn't tell" from "spec said no."
- **Audit replay** spans cloud and edge: each agent run produces a trace at `<store>/traces/<run_id>/trace.json`. Run IDs are content-addressed, so cross-store sync is union semantics — a blob copy plus attestation entry on reconnect, no resolver. (Earlier drafts assumed `lex-vcs` three-way merge of trace branches; that's not what shipped — see [`docs/lex-lang/issues/187-lex-trace-vcs-integration.md`](./lex-lang/issues/187-lex-trace-vcs-integration.md).)

## 4. Communication

Two protocols, used for what each is good at.

### 4.1 Inter-agent: A2A

Structured envelope, flexible payload. The envelope carries `from`, `to`, `intent_id`, `causal_parents`, `topic`, `timestamp`. The payload is a `Message` with content `Parts` (text, structured data, or files).

A2A is purpose-built for agent-to-agent communication: Tasks, Messages with typed Parts, agent-card discovery, identity, and streaming. It is emerging-standard rather than settled, but it is the most adopted candidate today and aligns with MCP's surface.

We considered three alternatives:

- **Pure free-form text.** Rejected — the ambiguity tax compounds across agents and produces a debugging nightmare at scale.
- **A bespoke envelope** (as in the original soft draft). Rejected — no interop, and we would be inventing a protocol that A2A already covers.
- **ACP** (Agent Communication Protocol). Considered — less adopted than A2A and has no clear advantage for our use case.

### 4.2 Agent-to-tool: MCP

Each Python service exposes an MCP server. lex-lang v0.2.0's `agent.call_mcp(server, tool, args_json)` is a real stdio JSON-RPC client (spawn-per-call, stateless). Soft-agent maintains the `server` name → command-line registry so agent code references servers by short name (e.g. `"optimizer"`).

### 4.3 Why split A2A and MCP

A2A is for agent-to-agent; MCP is for tools. Forcing one to do both either bloats the agent protocol with tool semantics or strips MCP of its discovery and schema affordances. Keeping them separate gives interop on day one with both ecosystems.

## 5. Where each piece lives

**No grammar changes.** The ≤500-token grammar constraint stays intact. lex-lang v0.2.0 added language-level primitives; `soft` owns the agent runtime and the A2A surface. See [`docs/lex-lang/scope-decision.md`](./lex-lang/scope-decision.md) for the upstream team's reasoning on the boundary.

### 5.1 Upstream (`lex-lang` v0.2.0)

| Issue → PR | Item | Shape |
|---|---|---|
| #184 → [#189](https://github.com/alpibrusl/lex-lang/pull/189) | Effect tags `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]` and `std.agent` builtins | Runtime handlers + four typed functions in `std.agent`. |
| #185 → [#190](https://github.com/alpibrusl/lex-lang/pull/190) | `agent.call_mcp` real stdio JSON-RPC client | Spawn-per-call, stateless; `Drop` reaps subprocess. |
| #186 → [#191](https://github.com/alpibrusl/lex-lang/pull/191) | `spec_checker::evaluate_gate` as runtime gate | Returns `GateVerdict::{Allow, Deny, Inconclusive}`; ~5 ms mean for 1k iterations on Phase 1 specs. |
| #187 → [#192](https://github.com/alpibrusl/lex-lang/pull/192) | `lex-trace` ↔ `lex-vcs` integration (docs only) | Traces are standalone `<store>/traces/<run_id>/` artifacts; cross-store sync is union semantics. |

### 5.2 In-repo (`soft`)

| Crate | Role | Depends on |
|---|---|---|
| [`soft-agent`](./crates/soft-agent.md) | Agent runtime: identity, mailbox, handler dispatch, action-gate hook, trace recording. | `lex-{runtime,trace,store,bytecode}` 0.2, `spec-checker` 0.2. |
| [`soft-a2a`](./crates/soft-a2a.md) | A2A wire surface; pins A2A version; uses `lex-api` 0.2 as a library. | `lex-api` 0.2, `soft-agent`. |

The cumulative net-new code is two soft-internal crates plus orchestration on top of the upstream primitives — not a new framework.

## 6. Edge deployment

Vehicle agents run on the truck. Depot, TMS, and PV agents run in the cloud.

### 6.1 Hardware target

NVIDIA Jetson Orin (aarch64-linux) or Mac Studio mini (aarch64-darwin) per truck. lex-lang v0.2.0's release workflow attaches pre-built binaries for both targets to every `v*` GitHub Release; verifying our use case on real hardware is tracked in [#198](https://github.com/alpibrusl/lex-lang/issues/198).

### 6.2 Local LLM

`[llm_local]` is bound to a local inference runtime — Ollama, llama.cpp, or MLX. Seeded sampling required for trace reproducibility. The provider configuration shape is open in [#196](https://github.com/alpibrusl/lex-lang/issues/196); soft will weigh in with an env-var-for-common-cases plus `lex_runtime::EffectHandler`-as-escape-hatch shape.

### 6.3 Effect typing as deployment fact

Effect signatures encode the load-bearing compile-time facts about each agent:

```
vehicle-agent : [llm_local, mcp, a2a]
depot-agent   : [llm_cloud, mcp, a2a]
tms-agent     : [llm_cloud, mcp, a2a]
```

Compile-time guarantees:

- `vehicle-agent` cannot reach a cloud LLM (no `[llm_cloud]`).
- `depot-agent` and `tms-agent` cannot reach a local LLM (no `[llm_local]`).

Runtime guarantees (enforced by soft-agent's tool registry, since lex 0.2 ships flat `[mcp]`):

- `vehicle-agent`'s MCP allowlist is `{ telemetry, local_planner }` only.
- `depot-agent`'s MCP allowlist is `{ ocpp, optimizer }` only — only the depot can issue `RemoteStartTransaction`.
- `tms-agent`'s MCP allowlist is `{ optimizer, tms_db }` only.

Promoting per-server scoping to a compile-time fact (parameterized effects like `[mcp(ocpp)]`) is a possible future upstream ask if the runtime check turns into a real source of bugs.

### 6.4 Offline behaviour

A truck loses connectivity mid-route. The vehicle-agent keeps operating on local decisions, queues outbound A2A messages, and accumulates a local trace at `<store>/traces/<run_id>/trace.json` on the truck's `lex-store`.

### 6.5 Reconnect

Cross-store sync of the trace is a blob copy plus an attestation entry. Run IDs are content-addressed, so there's no merge step; the cloud audit is the union of all per-run trace files received. This uses `lex-trace` and `lex-store` as shipped — no new sync protocol required for Phase 1.

## 7. Phase 1 demo

### 7.1 Scope

- One simulated depot.
- ~5 simulated vehicles, simulated chargers, simulated PV.
- Recorded or generated TMS deliveries.
- Mock Python services via MCP (under [`mocks/`](../mocks/)).
- Three agent types (`vehicle-agent`, `depot-agent`, `tms-agent`).
- In-process A2A. HTTP transport deferred to Phase 2.

### 7.2 Specs as runtime gates (two)

1. Site grid budget never exceeded.
2. Vehicle SoC never below configured reserve.

Each is a `spec-checker` invariant evaluated against state plus the proposed effect, before the effect executes. Verdicts are `Allow`, `Deny { spec_name, reason }`, or `Inconclusive { spec_name, reason }`; soft-agent's policy is `Inconclusive → Deny`, with the variant and reason recorded in the trace.

### 7.3 Capability separations (two)

1. Vehicle can *request* a charging session; only the depot can *grant*. Depot is the only agent whose MCP allowlist contains `ocpp`, and the only agent that registers `Topic.GrantSession` as an outgoing topic.
2. TMS can *dispatch* a delivery; only a vehicle can *acknowledge* and *complete*. Same pattern.

### 7.4 Failure-mode demos (two)

1. Charger drops mid-session. Depot agent observes the disconnect, the system recovers, no spec is violated, and the audit reflects the reality.
2. Vehicle goes offline mid-route, then reconnects. The truck's per-run trace files are blob-copied to the cloud audit on reconnect; no spec violations on either side.

### 7.5 Headline deliverable

> Replay the simulated day's trace from cloud and edge audit logs, and prove that no spec was ever violated.

This is the one-sentence summary worth shipping a demo to defend.

## 8. Phase 2 sketch (deferred)

- Multi-depot with cross-depot session handoff (vehicle reroutes mid-trip).
- A2A over HTTP across processes — cloud-to-edge and depot-to-depot.
- OCPP/OCPI bridges via MCP. Python keeps the wire protocols at external boundaries; the agents speak A2A internally and call the bridge as a tool.
- `soft replay <run_id>` CLI — reconstructs decisions across cloud + edge audit logs.
- Production deployment on real Jetson hardware in at least one truck.

Phase 2 is intentionally deferred until Phase 1 has demonstrated that the four Lex features (effects, capabilities, spec-checker as gate, trace replay) earn their keep on a single depot.

## 9. Non-goals

- Replacing Python data-plane modules.
- Speaking OCPP/OCPI from Lex directly. Python keeps the wire protocols at external boundaries.
- A new agent communication protocol. We adopt A2A.
- Formal pre-condition verification of LLM-generated message payloads. Specs are runtime gates on proposed actions, not message-shape preconditions.
- A built-in planner. The optimizer remains a Python service called via MCP.
- Multi-language agent runtime. Lex only.
- Hard real-time guarantees.
- Service-mesh replacement.
- LLM-as-a-language-primitive. LLM access is a runtime effect, not a Lex construct.

## 10. Open questions / status

### 10.1 lex-trace ↔ lex-vcs integration

**Resolved** by [PR #192](https://github.com/alpibrusl/lex-lang/pull/192) — see §3.3, §6.4, §6.5. Traces are standalone artifacts, not vcs branches. Cross-store sync is union semantics on content-addressed run IDs.

### 10.2 LLM-internal vs LLM-external

`[llm_local]` and `[llm_cloud]` ship as runtime effects in lex 0.2. Provider configuration (Ollama / llama.cpp / MLX bindings, cloud API keys) is open in [#196](https://github.com/alpibrusl/lex-lang/issues/196). **soft's preferred shape:** thin upstream surface for env-vars covering the common cases (`OLLAMA_HOST`, `OPENAI_API_KEY`), plus a `lex_runtime::EffectHandler` impl as the escape hatch.

### 10.3 A2A versioning

A2A is emerging-standard. Breaking changes are a real risk. **Mitigation:** pin a version, isolate the wire-format adapter inside `soft-a2a`. soft-a2a's release cycle is decoupled from `lex-api`'s.

### 10.4 Spec-checker ergonomics as a runtime gate

**Resolved** by [PR #191](https://github.com/alpibrusl/lex-lang/pull/191). Synchronous deterministic gating shipped (`evaluate_gate_compiled`); the SMT-LIB path is reserved for offline verification. Per-action verdicts at ~5 ms mean for 1k iterations on the Phase 1 spec set. soft-agent treats `GateVerdict::Inconclusive` as `Deny` by default and records both the variant and the `reason` in the trace.

### 10.5 Edge LLM determinism

`[llm_local]` replay requires seeded sampling and a pinned model snapshot. Both are part of the trace record per #189's design. Verifying the model-snapshot-hash + sampling-seed shape is stable across runs is on us; the configuration shape is tracked under [#196](https://github.com/alpibrusl/lex-lang/issues/196).

## 11. Issue and crate index

### 11.1 lex-lang issues — closed in v0.2.0

| Issue | Closed by | Mirror | Title |
|---|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | [#189](https://github.com/alpibrusl/lex-lang/pull/189) | [`184-effect-tags.md`](./lex-lang/issues/184-effect-tags.md) | Effect tags `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]` |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | [#190](https://github.com/alpibrusl/lex-lang/pull/190) | [`185-mcp-tool-registry-wrapper.md`](./lex-lang/issues/185-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | [#191](https://github.com/alpibrusl/lex-lang/pull/191) | [`186-spec-checker-runtime-gate.md`](./lex-lang/issues/186-spec-checker-runtime-gate.md) | Wire `spec-checker` as runtime action gate |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | [#192](https://github.com/alpibrusl/lex-lang/pull/192) | [`187-lex-trace-vcs-integration.md`](./lex-lang/issues/187-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

### 11.2 Open lex-lang follow-ups (we are tracking)

| Issue | Topic | Soft's stake |
|---|---|---|
| [#196](https://github.com/alpibrusl/lex-lang/issues/196) | LLM provider configuration | Will comment with the env-var + escape-hatch shape we want. |
| [#197](https://github.com/alpibrusl/lex-lang/issues/197) | `agent.call_mcp` connection cache | Will produce benchmarks (Phase 1 will include them). |
| [#198](https://github.com/alpibrusl/lex-lang/issues/198) | Edge runtime cross-compile + verified aarch64 binaries | Will smoke-test the v0.2.0 binaries on Jetson + Mac Studio mini and report. |

### 11.3 Soft-internal crates

| Crate | Scoping doc | Origin |
|---|---|---|
| `soft-agent` | [`docs/crates/soft-agent.md`](./crates/soft-agent.md) | Originally drafted as upstream Issue 1; declined — agent runtime is above the language. |
| `soft-a2a` | [`docs/crates/soft-a2a.md`](./crates/soft-a2a.md) | Originally drafted as upstream Issue 4; declined — A2A version pin belongs with the consumer. |

See [`docs/lex-lang/scope-decision.md`](./lex-lang/scope-decision.md) and [`docs/lex-lang/expectations.md`](./lex-lang/expectations.md) for the conversation around the boundary.

## 12. Path forward

**Status as of 2026-05-06:**

- lex-lang v0.2.0 published on crates.io; #184–#187 closed by PRs #189–#192.
- soft workspace pinned at `lex-lang = "0.2"` (semver); `cargo check --workspace` clean.
- Phase 1 mocks, smoke tests (10/10 passing), and Phase 1 sketches landed.
- Three open lex-lang follow-ups (#196–#198) tracked as soft asks.

**Next steps:**

1. Implement `crates/soft-agent`: agent type, mailbox, handler dispatch, action-gate hook with `evaluate_gate_compiled`, trace recording via `lex_trace::Recorder`.
2. Implement `crates/soft-a2a`: A2A protocol adapter on top of `lex-api::serve_on`.
3. Wire Phase 1 sketches in `examples/phase1/` against the real APIs as soft-agent grows.
4. Validate the headline deliverable (audit replay across cloud + edge proves no spec violation).
5. Close out #196 / #197 / #198 contributions as we hit them.

If at the end of Phase 1 any of the four Lex features (effects, capabilities, spec-checker as gate, trace replay) doesn't earn its keep on the demo, that is itself a useful finding about Lex.
