# Soft → lex-lang: build report after v0.2.0

**Date:** 2026-05-06 (later in the day; v0.2.0 published this morning).
**Re:** post-build follow-up to [`expectations.md`](./expectations.md).

A short status update. Soft built `soft-agent` v0–v3 and `soft-a2a` v0 against `lex-lang = "0.2"` over the past few hours. This doc reports what worked, what we worked around, and what we'd like next. Two issue drafts ready to file are in [`issues/drafts/`](./issues/drafts/).

## 1. Status snapshot

- Workspace pinned at `lex-lang = "0.2"` (currently resolves to **v0.2.1** for `spec-checker` and `lex-trace` after their tracer-hook patch landed; rest of the family at 0.2.0/0.2.1). `cargo check --workspace` clean.
- **`soft-agent` v3** — agent type + builder + mailbox; runner with Rust *and* Lex handler dispatch; spec gate via `spec-checker::evaluate_gate_compiled`; trace via `lex_trace::Recorder` + `lex_store::Store::save_trace`; `LexHost` for invoking Lex code under our own `Vm + Recorder`.
- **`soft-a2a` v0** — `tiny_http` server + `ureq` client, A2A `Message` / `Part` / `AgentCard` schema pinned at `A2A_VERSION = "0.2"`, agent-card endpoint, mailbox forwarding.
- Tests: 28 Rust tests + 10 Python pytest, all green. End-to-end integration test (`crates/soft-a2a/tests/integration.rs`) drives an external A2A client → soft-a2a server → soft-agent mailbox → Lex handler → executor → persisted trace.

## 2. What worked end-to-end

- **`agent.call_mcp`** drives our Phase 1 mock MCP servers through real stdio JSON-RPC. Spawn-per-call is fine for our load profile so far.
- **`spec-checker::evaluate_gate_compiled`** returns `Allow | Deny | Inconclusive` at single-digit ms on the Phase 1 specs. We treat `Inconclusive` as `Deny` by default and record the variant + reason in the trace.
- **`lex-trace::Recorder` + `lex-store::Store::save_trace`** — events round-trip cleanly (write → finalize → save → reload → assert).
- **`LexHost` with our own `Vm + Recorder`** captures nested Lex helper calls (`under_budget` → `projected_load`, `budget_total`) in the resulting `TraceTree`.
- **`lex_bytecode::Value::from_json` / `to_json`** handles the JSON ↔ Lex-Value round-trip we use to bridge soft-agent's JSON state into spec bindings and Lex handler args.

## 3. What we worked around

Two real friction points; drafts ready to file in `issues/drafts/`.

### 3.1 `spec-checker`'s eval `Vm` has no `Tracer` hook (resolved upstream in v0.2.1)

Originally drafted as a high-priority ask. Shipped same day as `spec_checker::evaluate_gate_compiled_traced(specs, bindings, bc, new_tracer: Fn() -> Box<dyn Tracer>)`, plus `impl Tracer for lex_trace::Handle` so multiple tracers can share recorder state via `Arc<Mutex>`. Wired into `soft_agent::Gate::evaluate_traced` and the runner's per-action gate path; verified by [`crates/soft-agent/tests/gate_traced.rs`](../../crates/soft-agent/tests/gate_traced.rs) (spec body's helper calls — `projected_load`, `budget_total` — appear as nested calls in the recorder's tree). Draft preserved at [`issues/drafts/spec-checker-tracer-hook.md`](./issues/drafts/spec-checker-tracer-hook.md) for historical context.

### 3.2 Parser rejects underscore-prefixed identifiers (low priority)

`fn _host()` and `let _ := state.ok` both fail (`expected identifier ..., got Some(Underscore)`). Workarounds (rename to `host`, drop the unused-binding) are minor but recurring whenever Lex code is embedded in a Rust context that wants stub host programs or ignored params.

Draft: [`issues/drafts/parser-underscore-handling.md`](./issues/drafts/parser-underscore-handling.md). Either accept `_name` everywhere as a non-warning identifier, or accept `_` as a discard pattern in `let` and as a function-name marker.

## 4. Open follow-ups (your side, per `expectations.md`)

| Issue | Status from soft |
|---|---|
| [#196](https://github.com/alpibrusl/lex-lang/issues/196) — LLM provider config | Will comment with concrete shape: env vars (`OLLAMA_HOST`, `OPENAI_API_KEY`) for common cases + `lex_runtime::EffectHandler` impl as escape hatch, plus trace-record fields `model_snapshot_hash` and `sampling_seed`. |
| [#197](https://github.com/alpibrusl/lex-lang/issues/197) — `agent.call_mcp` connection cache | Awaiting our benchmarks. Will produce them once Phase 1 has a meaningful per-message profile. |
| [#198](https://github.com/alpibrusl/lex-lang/issues/198) — Edge runtime aarch64 binaries | Will smoke-test the v0.2.0 release binaries on Jetson + Mac Studio mini once we have hardware access. |

## 5. Open follow-ups (soft side)

For your visibility, not blocking you:

- `examples/phase1/*.lex` are still aspirational — they describe an `agent.*` DSL we don't ship in `soft-agent` (Lex builtins for `agent.new`, `agent.handle`, etc.). Either rewrite them to match the action-record convention `soft-agent` actually consumes, or add a thin Lex-side DSL on top of our existing `LexHost`. Probably the former; we'll decide as the agent count grows.
- A Phase 1 demo that actually runs a multi-agent scenario over A2A across processes (today the integration test runs in-process with the server in a thread).
- Wire up #196's surface in `soft-agent` once that issue resolves.

## 6. Asks

1. Triage the two drafts in [`issues/drafts/`](./issues/drafts/). The tracer hook is the one we'd actually use this quarter.
2. Roughly: any plans for v0.3 you'd like soft to factor in? We're a downstream consumer with active code, so bumping deps and reporting breakage is straightforward when you cut new releases.
