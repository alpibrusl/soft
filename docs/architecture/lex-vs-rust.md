# Lex vs Rust: where the boundary actually sits

> **TL;DR.** A 100% Lex system isn't realistic — *something* has to host the Lex VM, expose effect handlers to the OS, and integrate with external libraries (HTTP, MCP wire, JSON, content-addressed storage). That host is Rust today. **Everything above the host can be Lex** and is, in soft-agent v7: agent config, topic→handler mapping, handler decision logic, specs, and spec helpers all live in `.lex` source.

This doc breaks down what's left in Rust and why.

## Inventory: where each piece lives today

| Piece | Authored in | Why |
|---|---|---|
| Agent config (peers, effects, MCP allowlist, system prompt) | **Lex** (DSL preamble + `fn config()`) | Pure data — record-builder pattern. |
| Topic → handler mapping | **Lex** (`agent_handles([...])`) | Same. |
| Handler decision logic (per-topic) | **Lex** (`fn on_dispatch(state, msg)`) | Effect-typed; gated; traced. The natural home. |
| Spec language (predicates) | **Lex** (`spec body`) | spec-checker takes Lex source. |
| Spec helper functions | **Lex** (`fn under_budget(...)`) | Compiled into the gate's host program. |
| Mailbox + MailboxSender | Rust | `std::sync::mpsc`. |
| Runner loop (recv → dispatch → gate → execute → trace) | Rust | Hot path; per-message coordination; calls into Lex multiple times. |
| Action executor (MockExecutor, InProcessExecutor) | Rust | Owns external-side effects (mailbox delivery, future MCP/A2A wires). |
| Trace writer | Rust | Builds `lex_trace::TraceTree` (the type itself is Rust). |
| Content-addressed store | Rust | `lex-store` filesystem layout. |
| A2A wire (server, client, schema) | Rust | Wraps `tiny_http` + `ureq` + `serde`. |
| Lex VM | Rust | The runtime that interprets Lex bytecode. |
| FFI / marshalling (LexValue ↔ JSON ↔ Action) | Rust | Sits *at* the boundary by definition. |

## Categorization

The remaining Rust falls into four buckets. Each tells a different story about whether it could move.

### 1. The Lex runtime itself (immovable)

`lex_runtime::Vm`, `lex_bytecode::compile_program`, `lex_syntax::parse_source`, `lex_trace::Recorder`, `lex_store::Store`. *Something* has to be the host that interprets Lex bytecode and exposes effect handlers to the OS. That host is Rust in lex-lang's design. Self-hosting (a Lex VM written in Lex) is theoretically possible but wouldn't change the fact that *some* lower-level runtime has to back it.

**Verdict: stays Rust forever.**

### 2. External-library wrappers (immovable in practice)

`tiny_http`, `ureq`, `serde_json`. These are Rust crates with no Lex equivalent. soft-a2a's `wire.rs`, `server.rs`, `client.rs` are thin adapters around them.

We could expose these as Lex effects (`[http_server]`, `[json]`) — and a few are already exposed (`agent.call_mcp`, `agent.send_a2a`). But the *implementation* of the effect handler stays in Rust because it has to call into the underlying crate.

**Verdict: stays Rust. Lex sees the effect surface, Rust implements the handler.**

### 3. Hot-path runtime infrastructure (could move, but shouldn't)

The runner's `step()` and `drain()` loop, the trace writer's bookkeeping, action conversion at the FFI boundary. These could be Lex if we exposed `mailbox.try_recv` as an effect, dispatched into Lex per loop iteration, etc.

But each loop iteration crosses the FFI boundary multiple times. Per-message Lex VM round-trips are slow compared to a tight Rust loop. And there's no policy or capability content to express here — it's just plumbing.

**Verdict: could move; would be slower and add no expressiveness. Stays Rust by choice.**

### 4. Glue and bindings (shrinks as #3 stays)

`AgentConfig::from_lex_value`, `Action::from_json`, `BindingsFn` closures, the `bindings_to_json` trace helper. Marshaling between Rust and Lex types.

This bucket grows when the boundary is "wide" (lots of types crossing) and shrinks when the boundary is "narrow" (Lex returns a single typed result the runner consumes). Today it's small.

**Verdict: shrinks naturally as the boundary stabilizes; not something to optimize away.**

## Lines-of-code distribution after v7

| Layer | Where | Lines |
|---|---|---|
| Lex (compiling, exercised) | inline `r#"..."#` in tests/examples | ~480 (was 315 before DSL + multi_depot refactor) |
| Lex (aspirational, not compiling) | `examples/phase1/*.lex` standalone files | 374 |
| Rust runtime infrastructure | soft-agent `src/` excluding lex-side glue | ~2,400 |
| Rust glue (FFI, parsers, action conversion) | soft-agent `src/{lex_dsl, lex_host, gate}.rs` etc. | ~600 |
| Rust A2A wire | soft-a2a `src/` | ~290 |
| Rust tests + examples | both crates `tests/` + `examples/` | ~1,400 |

The user-authored part of any new agent (config + handlers + spec helpers + specs) is **100% Lex**. The Rust is project-side infrastructure.

## What 100% Lex would actually require

To eliminate Rust from the soft project entirely:

1. **A Lex VM in Lex.** Self-hosting. Possible but circular (something has to host the host).
2. **Lex bindings to all OS / network primitives.** Lex would need `std.net.tcp_listen`, `std.io.spawn_thread`, etc. Each implemented in *something* lower-level (Rust again).
3. **A Lex-native A2A SDK, MCP client, content-addressed store.** All currently Rust crates we wrap.

In practice, "100% Lex" means "100% Lex above an even thinner Rust host." Today our Rust host is wide because it covers convenience. With more upstream investment in lex-runtime (more effect handlers, richer stdlib), it could shrink toward a minimal interpreter + I/O layer. But it can't go to zero.

## What this means for the project

The dogfooding goal — "see how far Lex can carry the agent decision layer" — has been answered: **all the way**. Agent definitions, decision logic, gates, helpers are Lex. The 13:1 Rust:Lex ratio reflected our *infrastructure* burden, not Lex's expressiveness ceiling.

For Phase 2 the boundary could move further:
- More `lex-runtime` effect handlers shrink soft-agent's Rust glue.
- Richer Lex stdlib (record-update sugar, list helpers) shortens our DSL preamble.
- Upstreaming the agent DSL itself (proposed [#199](#) eventually) makes soft a thinner consumer.

None of those changes the answer to "is 100% Lex realistic?" — no, and that's fine.
