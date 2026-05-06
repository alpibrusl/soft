# Soft: Capability-Bounded LLM Agents on Lex

**Status:** Draft proposal for Phase 1.
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

### 3.2 Soft agent layer (new, in Lex)

LLM-driven agents that own the coordination decisions currently centralised:

| Agent | Where | Purpose |
|---|---|---|
| `vehicle-agent` | edge (per truck) | Requests routes; requests charging sessions; reports state. |
| `depot-agent` | cloud (per site) | Owns local grid budget; accepts/rejects sessions; schedules across chargers. |
| `tms-agent` | cloud | Dispatches deliveries; queries vehicles; reassigns when needed. |
| `pv-agent` | cloud (per site, optional) | Negotiates PV availability with the depot. |

Each agent has a system prompt, a goal, a declared peer list, a declared MCP tool allowlist, and an effect signature. The LLM interprets incoming intents and proposes the next action. Lex then decides whether the action is allowed.

### 3.3 Lex runtime (extended)

The substrate that surrounds the agent layer:

- **Capability types** decide which MCP tools and A2A peers an agent can reach.
- **Effect gates** intercept every proposed action; `spec-checker` runs as a runtime invariant; `lex-trace` records the full chain.
- **Audit replay** spans cloud and edge by treating each truck's local trace as a `lex-vcs` branch that merges into the cloud audit on reconnect.

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

Each Python service exposes an MCP server. Tools are registered in Lex's existing **Tool Registry** with effect manifests. Each agent has a typed allowlist of tools it can call. The Tool Registry already pairs tools with effect manifests, so the integration shape is incremental rather than novel.

### 4.3 Why split A2A and MCP

A2A is for agent-to-agent; MCP is for tools. Forcing one to do both either bloats the agent protocol with tool semantics or strips MCP of its discovery and schema affordances. Keeping them separate gives interop on day one with both ecosystems.

## 5. Where each piece lives

**No grammar changes.** The ≤500-token grammar constraint stays intact. Work splits across two repos: `lex-lang` adds language-level primitives; `soft` owns the agent runtime and the A2A surface. See [`docs/lex-lang/scope-decision.md`](./lex-lang/scope-decision.md) for the upstream team's reasoning on the boundary.

### 5.1 Upstream (`lex-lang`)

| Issue | Item | Shape | Existing infra it builds on |
|---|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | Effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` | Runtime handlers in `lex-runtime`. | Existing effect-typing pipeline. No type-system change. |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | MCP server wrapper for Tool Registry | Registers MCP servers as Tool Registry entries with effect manifests. | Existing Tool Registry, ACLI compliance. |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | `spec-checker` as runtime action gate | New wiring: proposed effect → spec check against state → allow/deny. | `spec-checker` (random property + SMT-LIB export). |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | `lex-trace` ↔ `lex-vcs` integration | Open question — see §10. | `lex-trace`, `lex-store`, `lex-vcs store-merge`. |

### 5.2 In-repo (`soft`)

| Crate | Role | Depends on |
|---|---|---|
| [`soft-agent`](./crates/soft-agent.md) | Agent runtime: identity, mailbox, handler dispatch, action-gate hook. | `lex-{vcs,types,trace}`, #184–#187. |
| [`soft-a2a`](./crates/soft-a2a.md) | A2A wire surface; pins A2A version; uses `lex-api` as a library. | `lex-api`, `soft-agent`, #184. |

The cumulative net-new code is two soft-internal crates plus four lex-lang runtime handlers plus wiring — not a new framework.

## 6. Edge deployment

Vehicle agents run on the truck. Depot, TMS, and PV agents run in the cloud.

### 6.1 Hardware target

NVIDIA Jetson Orin or Mac Studio mini per truck. The Lex stack-VM bytecode interpreter is small and portable enough for both.

### 6.2 Local LLM

`[llm-local]` is bound to a local inference runtime — Ollama, llama.cpp, or MLX depending on hardware. Seeded sampling is required for trace reproducibility.

### 6.3 Effect typing as deployment fact

Effect signatures encode where each agent runs and what it can do:

```
vehicle-agent : [llm-local, telemetry-read, a2a-out, a2a-in]
depot-agent   : [llm-cloud, mcp(ocpp), mcp(optimizer), a2a-in, a2a-out]
tms-agent     : [llm-cloud, mcp(optimizer), mcp(tms-db), a2a-in, a2a-out]
```

These are compile-time facts:

- `vehicle-agent` cannot call a cloud LLM, cannot write to OCPP, cannot grant a charging session — the effect set forbids it.
- Only `depot-agent` can issue OCPP `RemoteStartTransaction`, because only `depot-agent` carries `mcp(ocpp)`.

### 6.4 Offline behaviour

A truck loses connectivity mid-route. The vehicle-agent keeps operating on local decisions, queues outbound A2A messages, and writes to a local trace branch on the truck's `lex-store`.

### 6.5 Reconnect

`lex-vcs store-merge` reconciles the truck's trace branch into the cloud audit. Three-way merge handles divergence. This uses existing `lex-vcs` functionality; no new sync protocol is required for Phase 1.

## 7. Phase 1 demo

### 7.1 Scope

- One simulated depot.
- ~5 simulated vehicles, simulated chargers, simulated PV.
- Recorded or generated TMS deliveries.
- The **real** Python `scheduling-optimizer` reached via MCP.
- Three agent types (`vehicle-agent`, `depot-agent`, `tms-agent`).
- In-process A2A. HTTP transport deferred to Phase 2.

### 7.2 Specs as runtime gates (two)

1. Site grid budget never exceeded.
2. Vehicle SoC never below configured reserve.

Each is a `spec-checker` invariant evaluated against state plus the proposed effect, before the effect executes.

### 7.3 Capability separations (two)

1. Vehicle can *request* a charging session; only the depot can *grant*. Request and grant are different effects, carried by different agents.
2. TMS can *dispatch* a delivery; only a vehicle can *acknowledge* and *complete*. Same pattern.

### 7.4 Failure-mode demos (two)

1. Charger drops mid-session. Depot agent observes the disconnect, the system recovers, no spec is violated, and the audit reflects the reality.
2. Vehicle goes offline mid-route, then reconnects. Local trace branch on the truck is merged into the cloud audit on reconnect; no spec violations on either side.

### 7.5 Headline deliverable

> Replay the simulated day's trace from cloud and edge audit logs, and prove that no spec was ever violated.

This is the one-sentence summary worth shipping a demo to defend.

## 8. Phase 2 sketch (deferred)

- Multi-depot with cross-depot session handoff (vehicle reroutes mid-trip).
- A2A over HTTP across processes — cloud-to-edge and depot-to-depot.
- OCPP/OCPI bridges via MCP. Python keeps the wire protocols at external boundaries; the agents speak A2A internally and call the bridge as a tool.
- `soft replay <trace-id>` CLI — reconstructs decisions across cloud + edge audit logs.
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

## 10. Open questions

These need resolution before or during Phase 1.

### 10.1 lex-trace ↔ lex-vcs integration

Is `lex-trace`'s output already a first-class entity inside `lex-vcs` branches, or is integration the "tier-2" work the original draft referenced as upstream-pending? Affects whether edge audit sync is "free" or requires upstream work. [#187](https://github.com/alpibrusl/lex-lang/issues/187) asks this.

### 10.2 LLM-internal vs LLM-external

Does the LLM run inside the Lex agent process as an `[llm-*]` effect handler, or does the agent talk to an external LLM service? External-by-default is simpler and lets us swap providers; the effect handler is a thin shim. Internal would be more auditable but couples the runtime to a specific inference stack. **Recommendation:** external-by-default, with a thin `lex-runtime` handler abstraction so we can move the boundary later if it matters.

### 10.3 A2A versioning

A2A is emerging-standard. Breaking changes are a real risk. **Mitigation:** pin a version, isolate the wire-format adapter in a single module so a future migration is a contained change.

### 10.4 Spec-checker ergonomics as a runtime gate

`spec-checker` today is built around randomized property checking and SMT-LIB export. That is fine for tests; it may not be the right ergonomics for a per-action runtime gate that must return a verdict in milliseconds. Worth a spike before committing the wiring shape. [#186](https://github.com/alpibrusl/lex-lang/issues/186) covers this.

### 10.5 Edge LLM determinism

`[llm-local]` replay requires seeded sampling and a pinned model snapshot. Replay only reproduces decisions if both are stable across runs. **Mitigation:** include the model snapshot hash and sampling seed in each trace record. This is part of what `lex-trace` should capture.

## 11. Issue and crate index

Four issues filed in `lex-lang`; two declined and now soft-internal crates. Detailed bodies live in their own files for traceability.

### 11.1 lex-lang issues

| # | File mirror | Title |
|---|---|---|
| [#184](https://github.com/alpibrusl/lex-lang/issues/184) | [`docs/lex-lang/issues/184-effect-tags.md`](./lex-lang/issues/184-effect-tags.md) | Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` |
| [#185](https://github.com/alpibrusl/lex-lang/issues/185) | [`docs/lex-lang/issues/185-mcp-tool-registry-wrapper.md`](./lex-lang/issues/185-mcp-tool-registry-wrapper.md) | MCP server wrapper for Tool Registry |
| [#186](https://github.com/alpibrusl/lex-lang/issues/186) | [`docs/lex-lang/issues/186-spec-checker-runtime-gate.md`](./lex-lang/issues/186-spec-checker-runtime-gate.md) | Wire `spec-checker` as a runtime action gate |
| [#187](https://github.com/alpibrusl/lex-lang/issues/187) | [`docs/lex-lang/issues/187-lex-trace-vcs-integration.md`](./lex-lang/issues/187-lex-trace-vcs-integration.md) | Clarify `lex-trace` ↔ `lex-vcs` integration |

### 11.2 Soft-internal crates

| Crate | Scoping doc | Origin |
|---|---|---|
| `soft-agent` | [`docs/crates/soft-agent.md`](./crates/soft-agent.md) | Originally drafted as upstream Issue 1; declined — agent runtime is above the language. |
| `soft-a2a` | [`docs/crates/soft-a2a.md`](./crates/soft-a2a.md) | Originally drafted as upstream Issue 4; declined — A2A version pin belongs with the consumer. |

See [`docs/lex-lang/scope-decision.md`](./lex-lang/scope-decision.md) for the upstream team's reasoning.

## 12. Path forward

**Lex-lang issues filed.** [#184](https://github.com/alpibrusl/lex-lang/issues/184), [#185](https://github.com/alpibrusl/lex-lang/issues/185), [#186](https://github.com/alpibrusl/lex-lang/issues/186), [#187](https://github.com/alpibrusl/lex-lang/issues/187) are tracked upstream.

**Next steps.**

1. Stand up `crates/soft-agent` and `crates/soft-a2a` in this repo as the assembly layer; track upstream landings to wire each lex-lang primitive in as it ships.
2. Build Phase 1 in `examples/phase1/` against the real APIs as they land.
3. Validate the headline deliverable (audit replay across cloud + edge proves no spec violation).
4. Use what we learn to scope Phase 2 honestly.

If at the end of Phase 1 any of the four Lex features (effects, capabilities, spec-checker as gate, trace replay) does not earn its keep on the demo, that is itself a useful finding about Lex.
