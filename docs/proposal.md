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

## 5. Lex extensions required

**No grammar changes.** The ≤500-token grammar constraint stays intact. Each extension below maps to a draft issue in §11.

| # | Item | Shape | Existing infra it builds on |
|---|---|---|---|
| 1 | `lex-agent` crate | New crate. Agent type, mailbox, peer config, handler dispatch. | `std.flow` for the receive→propose→gate→execute loop. |
| 2 | New effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]` | Runtime handlers in `lex-runtime`. | Existing effect-typing pipeline. No type-system change. |
| 3 | MCP server wrapper | Registers MCP servers as Tool Registry entries with effect manifests. | Existing Tool Registry, ACLI compliance. |
| 4 | A2A endpoints | New surface on existing transport. | `lex-api` HTTP/JSON server. |
| 5 | Spec-checker as runtime action gate | New wiring: proposed effect → spec check against state → allow/deny. | `spec-checker` (random property + SMT-LIB export). |
| 6 | `lex-trace` ↔ `lex-vcs` integration | Open question — see §10. | `lex-trace`, `lex-store`, `lex-vcs store-merge`. |

The cumulative net-new code is one crate plus four runtime handlers plus wiring — not a new framework.

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

Is `lex-trace`'s output already a first-class entity inside `lex-vcs` branches, or is integration the "tier-2" work the original draft referenced as upstream-pending? Affects whether edge audit sync is "free" or requires upstream work. Issue 6 in §11 asks this.

### 10.2 LLM-internal vs LLM-external

Does the LLM run inside the Lex agent process as an `[llm-*]` effect handler, or does the agent talk to an external LLM service? External-by-default is simpler and lets us swap providers; the effect handler is a thin shim. Internal would be more auditable but couples the runtime to a specific inference stack. **Recommendation:** external-by-default, with a thin `lex-runtime` handler abstraction so we can move the boundary later if it matters.

### 10.3 A2A versioning

A2A is emerging-standard. Breaking changes are a real risk. **Mitigation:** pin a version, isolate the wire-format adapter in a single module so a future migration is a contained change.

### 10.4 Spec-checker ergonomics as a runtime gate

`spec-checker` today is built around randomized property checking and SMT-LIB export. That is fine for tests; it may not be the right ergonomics for a per-action runtime gate that must return a verdict in milliseconds. Worth a spike before committing the wiring shape. Issue 5 in §11 covers this.

### 10.5 Edge LLM determinism

`[llm-local]` replay requires seeded sampling and a pinned model snapshot. Replay only reproduces decisions if both are stable across runs. **Mitigation:** include the model snapshot hash and sampling seed in each trace record. This is part of what `lex-trace` should capture.

## 11. Draft issue bodies for `alpibrusl/lex-lang`

The GitHub MCP scope of the session that produced this proposal is restricted to `alpibrusl/soft`, so these issues need to be filed by hand in `lex-lang`. Each is written so that it stands on its own.

---

### Issue 1 — New crate `lex-agent` for autonomous agent runtime

**Summary.** Add a new crate `lex-agent` that provides the runtime primitives for autonomous agents in Lex. Driven by the `soft` proposal in `alpibrusl/soft:docs/proposal.md`.

**Scope.**

- An `agent` type carrying: identity (content-addressed), state, declared peer list, declared MCP tool allowlist, declared effect set, system prompt and goal (for LLM-driven agents).
- A mailbox abstraction over A2A messages (in-process for Phase 1; transport-pluggable).
- Handler dispatch built on `std.flow`: receive → propose → gate → execute.
- Action-gate hook between "LLM proposed action" and "effect executes" that calls into `spec-checker` and writes to `lex-trace`.

**Out of scope.**

- A2A wire transport (separate issue, Issue 4).
- MCP wire transport (Issue 3).
- Effect handlers themselves (Issue 2).

**Acceptance.** Phase 1 of `soft` can run a three-agent demo (`vehicle`, `depot`, `tms`) in-process using `lex-agent`.

---

### Issue 2 — Add effect tags `[a2a]`, `[mcp]`, `[llm-local]`, `[llm-cloud]`

**Summary.** Add four new runtime effects to `lex-runtime` so agent effect signatures can express what they may do.

**Why these four are distinct.**

- `[a2a]` and `[mcp]` are different protocols with different security and replay properties.
- `[llm-local]` and `[llm-cloud]` are different deployment surfaces with different latency, cost, and determinism properties. An edge `vehicle-agent` typed `[llm-local, telemetry-read, a2a-out]` must not be able to accidentally call a cloud LLM.

**Scope.** Effect tags, runtime handlers, policy-gate integration, `--allow-effects` plumbing.

**Acceptance.** A Lex program that declares `[llm-local]` cannot reach `[llm-cloud]` and vice versa. Each effect is loggable to `lex-trace` with enough information to replay.

---

### Issue 3 — MCP server wrapper for Tool Registry

**Summary.** Allow MCP servers to be registered as Tool Registry entries with effect manifests, so a Lex program can call MCP tools through the existing tool-registry surface.

**Scope.**

- A wrapper that consumes an MCP server's tool list and produces Tool Registry entries.
- Effect manifest derivation: each MCP tool is annotated with an effect (e.g. `[mcp(ocpp), net]`).
- Wire-format glue.

**Open questions.**

- How much of this is already covered by ACLI compliance? May reduce scope.
- Authentication and credentials handling at the MCP boundary.

**Acceptance.** A Python service exposed as an MCP server can be called from a Lex agent via the Tool Registry, and the call is correctly typed and audited.

---

### Issue 4 — A2A endpoints exposed via `lex-api`

**Summary.** Expose A2A agent endpoints via the existing `lex-api` HTTP/JSON server. Agents become A2A-discoverable services with agent cards.

**Scope.**

- A2A protocol adapter: Messages, Tasks, Parts, agent cards, identity.
- Pin a specific A2A version. Isolate the adapter in one module so a future upgrade is contained.
- Streaming via SSE where A2A specifies it.

**Out of scope.**

- Cross-process A2A is acceptable target for this issue, but Phase 1 of `soft` only requires in-process.

**Acceptance.** A `lex-agent` instance is reachable as an A2A endpoint, returns a valid agent card, and round-trips a Message with a structured Part.

---

### Issue 5 — Wire `spec-checker` as a runtime action gate

**Summary.** Reuse `spec-checker` as a runtime invariant gate that runs between an agent's proposed action and its execution.

**Scope.**

- An API: `gate(state, proposed_effect, specs) -> Allow | Deny(reason)`.
- Performance target: per-action verdict in single-digit milliseconds for Phase 1's spec set.
- Trace integration: every gate decision is recorded to `lex-trace`.

**Open questions.**

- `spec-checker` today is randomized-property + SMT-LIB. Is the existing implementation suitable for synchronous per-action gating, or is a different evaluation mode needed? Possibly the gate uses a deterministic sub-mode and the SMT path is reserved for offline verification.
- Spec authoring ergonomics. Library-first, no grammar change. The constraint that grammar fits in ≤500 tokens is intact.

**Acceptance.** Phase 1's two safety specs (site grid budget, SoC reserve) can be expressed and gate proposed effects in the demo.

---

### Issue 6 — Clarify `lex-trace` and `lex-vcs` integration

**Summary.** Question issue. Document and (if needed) implement the integration between `lex-trace` and `lex-vcs` so that an edge agent's local trace can be merged into a cloud audit log via `store-merge` on reconnect.

**Specifics.**

- Is each trace currently a `lex-vcs` branch, or is it a separate artifact in `lex-store`?
- If separate, what is the recommended path to land the integration the original soft draft (`lex-lang:docs/proposals/soft.md`) referenced as "tier-2"?
- Does `store-merge` already handle three-way merge of trace data, or does that need a custom resolver?

**Acceptance.** A clear answer documented, plus any required implementation work scoped as a follow-up issue.

---

## 12. Path forward

1. File the six issues above against `alpibrusl/lex-lang`.
2. Build Phase 1 in this repo, importing `lex-agent` (as it lands) and the new effect handlers.
3. Validate the headline deliverable (audit replay across cloud + edge proves no spec violation).
4. Use what we learn to scope Phase 2 honestly.

If at the end of Phase 1 any of the four Lex features (effects, capabilities, spec-checker as gate, trace replay) does not earn its keep on the demo, that is itself a useful finding about Lex.
