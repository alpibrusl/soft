# Soft → lex-lang: feedback on #184–#192 and what we'd appreciate next

**Date:** 2026-05-06
**Re:** PRs [#189](https://github.com/alpibrusl/lex-lang/pull/189), [#190](https://github.com/alpibrusl/lex-lang/pull/190), [#191](https://github.com/alpibrusl/lex-lang/pull/191), [#192](https://github.com/alpibrusl/lex-lang/pull/192) — the acceptance set for our soft proposal.

A short follow-up. Soft is the first downstream consumer of all four primitives; this doc captures what we'll align with on our side, what's open, and what'd help us next.

Cross-references: [`scope-decision.md`](./scope-decision.md) (the prior side of this conversation) and the four issue mirrors in [`issues/`](./issues/).

## 1. What works

- **Real wires, not stubs.** `agent.call_mcp` doing actual JSON-RPC over stdio means soft's Phase 1 mocks (under [`mocks/`](../../mocks/)) are exercisable end-to-end from a Lex program today. That's the integration story we asked for.
- **Spec gate is fast enough.** 5 ms mean for 1k iterations on the Phase 1 specs is on the right side of "milliseconds." We'll wire `evaluate_gate` into soft-agent's action-gate hook against that budget.
- **The trace-vs-vcs answer is better than what we proposed.** We assumed three-way merge of trace branches; union semantics on content-addressed run IDs is simpler and avoids new resolver work entirely. Updating soft's docs.

## 2. What we're aligning on the soft side

- **Effect tag spelling.** Our proposal used kebab-case (`llm-local`); shipped form is snake (`llm_local`, `llm_cloud`). Aligning across `docs/proposal.md`, `examples/phase1/`, and our issue mirrors.
- **Direct-call builtins, not handler dispatch.** Our Phase 1 sketches show `agent.handle("Topic", on_dispatch)`-style dispatch — that pattern lives in soft-agent (our crate), not in lex-lang. The four primitives sit *under* it as the effect-typed calls the handlers ultimately make. Updating the sketches to reflect that.
- **`GateVerdict::Inconclusive`.** Defaulting to fail-safe (treat as Deny) in soft-agent, with the verdict + reason recorded in the trace. Open to making it a per-spec policy if a fail-open use case appears later.

## 3. Stability surface we'd appreciate

Not blockers — soft can build against branch HEAD — but useful:

- **A tag** (`v0.x` or whatever your scheme is) right after #189–#192 so soft-agent and soft-a2a can pin against it. Branch-HEAD git deps make our `cargo check` non-deterministic.
- **One-liner CHANGELOG** naming what landed (the four PRs plus #175, #176, #188, etc.). Even per-PR bullets are enough.
- **Crate publication path.** Plans for `lex-types`, `lex-runtime`, `lex-trace`, `lex-vcs`, `lex-api` to land on crates.io? Alpha versions would unblock external pinning. If git-deps-only is the long-term answer, that's also fine — we'll commit to it.

## 4. Open questions on the four APIs

### 4.1 Tracing — writer-side

[#192](https://github.com/alpibrusl/lex-lang/pull/192) documented where traces *live* (`<store>/traces/<run_id>/trace.json`); the writer-side Rust API isn't obvious to us:

- How does soft-agent obtain a `RunId` for an agent execution?
- What's the call to append a trace event (proposed action, gate verdict, effect result)?
- Is the trace JSON schema documented somewhere we can reference for the eventual `soft replay` CLI?

A five-line example in `lex-trace`'s docs would close this.

### 4.2 Spec gate — bindings marshalling

`evaluate_gate(specs, bindings, lex_source)` — what's the Rust shape of `bindings`? Quantifier-name → some `Value` type? Soft-agent will marshal from its own `DepotState` / `VehicleState` types on every action, so the shape decides whether we need a derive macro or hand-coding works.

For `evaluate_gate_compiled`: who owns the compiled-program lifetime? Cached per agent type, per spec, or per call?

### 4.3 A2A — receiving side

`agent.send_a2a(peer, payload)` is enough to send. For *receiving*, the scope-decision said `lex-api::serve_on(server, state)` is exposed and that soft-a2a should use it as a library. Could you confirm:

- The entry point still exists in the post-#189–#192 version?
- The handler shape (signature, req/res types) we'd register against?

A pointer to a `lex-api` doc page or example is enough.

### 4.4 MCP — server-name resolution

`agent.call_mcp(server, tool, args_json)` — how does `server` (e.g. `"optimizer"`) resolve to a concrete stdio command at runtime? Our [`mocks/README.md`](../../mocks/README.md) assumes a future `--mcp optimizer="python mocks/optimizer.py"` flag on a hypothetical `soft run-demo`; is there an existing registry or config inside lex we should use instead, or is wiring it in soft the expected answer?

If "wire it up in soft" — fine, we'll do that in soft-agent.

### 4.5 LLM provider configuration

Where's the inference backend configured for `[llm_local]` and `[llm_cloud]`? Per-call args, environment variables, a config file? For soft we'd want:

- `[llm_local]` bound to one of {Ollama, llama.cpp, MLX} per-deploy.
- `[llm_cloud]` bound to a provider with API key from env.
- Trace records including model snapshot hash + sampling seed (per #189's trace-replay requirement).

If the config shape isn't fully decided yet, soft has opinions here.

## 5. Edge runtime

Vehicle-agents target Jetson Orin (aarch64-linux) and Mac Studio mini (aarch64-darwin) per [`proposal.md`](../proposal.md) §6.

- Pre-built binaries or simple cross-compilation paths for those targets — in the plan, or downstream's job?
- Known-good local-LLM backend per target? Ollama works on both; llama.cpp + CUDA on Jetson is the obvious other path.

## 6. Roadmap visibility

- What's queued after #184–#192 that would affect soft-agent's design?
- Plans for a connection cache in `agent.call_mcp`? PR #190 called this out as v2 pending benchmarks — we may produce those.
- Plans to formalize the JSON shape `agent.call_mcp` returns? Today: "server's `result` JSON serialized to a Lex `Str`," which is fine pragmatically; if a typed-deserialization path is coming, useful to know.

## 7. Integration testing

Soft will host an integration test that exercises all four primitives against the mock MCP servers. Goal: a reproducible "this version of lex + this version of soft works together" check. Happy to mirror it upstream if useful, or keep it downstream — your call.

## 8. What stays on us

For completeness — these don't need anything from upstream:

- The agent runtime itself (mailbox, handler dispatch, action-gate orchestration). [`crates/soft-agent`](../../crates/soft-agent/).
- The A2A wire surface and version pin. [`crates/soft-a2a`](../../crates/soft-a2a/).
- The Phase 1 demo, mocks, and audit-replay tooling.

We expect to be the busiest consumer of #184–#192 over the next quarter — happy to file follow-up issues as we hit edges.
