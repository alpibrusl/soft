# lex-lang 0.3 upgrade — soft-agent impact

**Status**: lex-lang [PR #230](https://github.com/alpibrusl/lex-lang/pull/230) merged
2026-05-07. Once a `0.3.0` is published, soft can bump its workspace pin
from `0.2` to `0.3`.

This document captures what changed upstream, what soft-agent has to
do (almost nothing), and what soft-agent *can* do to take advantage of
the new capabilities.

## TL;DR

`soft-agent`'s public surface usage of lex-lang is **unchanged**.
Compiling against `0.3` requires a one-line workspace bump and no
code changes. There is one optional refactor in `agent.rs` that
`#207` makes possible — a runtime allowlist becomes a compile-time
fact — and one mandatory consideration around the new effect
declarations in user-supplied `.lex` agent files.

## What landed upstream (10 commits)

### Stdlib batch — all closed

| Issue | Module / op | Commit |
|---|---|---|
| #216 | `std.env` + new `[env]` effect + `--allow-env` CLI flag | `093fbd3` |
| #218 | `std.math` extended (trig, log/exp, rounding, min/max/clamp) | `3e48a3c` |
| #219 | `std.random` — pure seeded SplitMix64 RNG | `1205223` |
| #217 | `std.parser` — structural combinators | `71559e4` |
| #221 | `parser.map` / `parser.and_then` | `5518c45` |
| #212 | `result.or_else` / `option.or_else` / `option.and_then` sig fix | `037a6e5`, `8e1f4ee` |

### Language-level — one of four shipped

| Issue | What | Commit |
|---|---|---|
| #207 | Per-capability effect parameterization (type system + runtime policy walk) | `984c25f` |
| #223 | `lex-vcs` preserves effect args in `EffectSet` end-to-end | `e867e6d` |

### Foundations

| Issue | What | Commit |
|---|---|---|
| #222 | Content-addressed closure identities (`Value::Closure { body_hash }`) | `b5d8866` |

## What soft-agent has to do

### Mandatory: nothing on the API side

The lex-lang surface that soft-agent consumes is unchanged:

- `lex_syntax::parse_source` (`lex_host.rs`, `gate.rs`)
- `lex_ast::canonicalize_program` (`lex_host.rs`, `gate.rs`)
- `lex_types::check_program` (`lex_host.rs`, `gate.rs`)
- `lex_bytecode::compile_program`, `Program`, `Value`, `Value::to_json` /
  `Value::from_json` (shape unchanged for everything except the closure
  debug string, which `soft-agent` doesn't introspect)
- `lex_bytecode::vm::{Vm, EffectHandler}` — `Vm::with_handler`, `vm.call`,
  `vm.set_tracer`
- `lex_runtime::{DefaultHandler, Policy}` — `DefaultHandler::new`,
  `Policy::permissive`
- `lex_trace::{Recorder, Handle, RunId, TraceTree, TraceNode, TraceNodeKind}`
  (the recorder, the `Tracer` impl on `Handle`, and the tree/node types
  consumed by `TraceWriter` and `replay::scan_trace`)
- `lex_store::Store::{open, save_trace, load_trace, list_traces}`
  (`trace.rs`, `bin/soft_replay.rs`)
- `spec_checker::{evaluate_gate_compiled, evaluate_gate_compiled_traced,
  parse_spec, Spec, GateVerdict}` (`gate.rs`)

### Mandatory: workspace bump

When lex-lang `0.3.0` ships, update `soft/Cargo.toml`:

```toml
[workspace.dependencies]
lex-syntax   = "0.3"
lex-ast      = "0.3"
lex-types    = "0.3"
lex-bytecode = "0.3"
lex-runtime  = "0.3"
lex-store    = "0.3"
lex-trace    = "0.3"
lex-api      = "0.3"
spec-checker = "0.3"
```

That's the entire mandatory delta.

## What soft-agent could do (optional)

### Drop the runtime MCP allowlist now that effects can carry per-server scope

`crates/soft-agent/src/agent.rs:131` has:

```rust
/// Runtime-side MCP allowlist check. Per-server scoping is a soft-agent
/// concern because lex-lang v0.2 ships the flat `[mcp]` effect; future
/// parameterized effects (`[mcp(<server>)]`) could promote this to a
/// compile-time fact.
pub fn allows_mcp_server(&self, server: &str) -> bool {
    self.config.mcp_servers.contains(server)
}
```

That comment is now overtaken by events. With lex-lang `0.3`, agent
authors can write:

```lex
fn dispatch_to_optimizer() -> [mcp("optimizer")] Result[Plan, Str] {
  agent.call_mcp("optimizer", "compute_plan", "{}")
}
```

The compiler catches `mcp("ocpp")` calls from inside an
`[mcp("optimizer")]`-declared function before the program ever runs.
The runtime gate also honors per-server grants:

```bash
lex run --allow-effects mcp:optimizer my-agent.lex
# fails if my-agent.lex declares [mcp("ocpp")]
```

`Agent::allows_mcp_server` becomes belt-and-braces — useful for the
rare case where a user *insists* on a flat `[mcp]` declaration but
the agent config still wants per-server scope. Keep it for that, or
delete it; either is defensible.

### Adopt `std.env` instead of host-shimmed env reads

If anywhere in the soft examples a Lex agent currently shells out to
the host runtime to read an env var, `std.env.get :: Str -> [env] Option[Str]`
is now a first-class effect. Tighter capability story, no shim.

### Adopt `std.random` for seed-deterministic agent decisions

Replay-deterministic randomness (retry jitter, exploration sampling)
falls out of `std.random.seed(N)` + threading the `Rng` through call
sites. The seed appears in the trace at construction time, so replay
reproduces the sequence exactly. See the §10.5 `[llm_local]` replay
note in the soft proposal — this satisfies the "seeded sampling"
requirement called out there.

## SemVer details — what makes this 0.3

Two surface changes warrant the major bump:

1. **`lex_bytecode::Value::Closure` gained a `body_hash` field**
   (`b5d8866`, #222). Pattern-matches like
   `Value::Closure { fn_id, captures }` need a `, ..` added.
   `soft-agent` doesn't pattern-match on `Value::Closure` anywhere —
   it only consumes `LexValue::to_json()` results — so this lands
   transparently.

2. **`lex_types::EffectSet.concrete` changed from `BTreeSet<String>`
   to `BTreeSet<EffectKind>`** (`984c25f`, #207). Direct string
   iteration breaks. `soft-agent` defines its own private
   `EffectSet` enum (`crates/soft-agent/src/effect.rs:33`) and
   never imports `lex_types::EffectSet`, so this also lands
   transparently.

The closure debug string `<closure fn_N>` became `<closure HEX8>`
where `HEX8` is the first 8 hex chars of the body hash. Trace
stability *improves*: equivalent closures produced from different
source locations now render identically across compiles. If any
test fixture in `soft-agent` matches against the literal string
`<closure fn_`, it will need updating — a quick `grep -r "<closure
fn_" tests/` will find them.

## Closure-call hook — the `parser.run` case study

`#221` introduced a small VM-internal pattern that `soft-agent` may
find useful for its own host-side helpers: `Vm::run_to(base_depth)`
plus `Vm::invoke_closure_value(closure, args)` makes the Vm
re-entrant for closure invocation from inside an effect dispatch.
The trait `lex_bytecode::parser_runtime::ClosureCaller` is the
intended extension point, currently used only by the parser
interpreter but generic over any caller that needs to invoke
closures while the Vm is mid-run. If `soft-agent` ever wants to
intercept an effect (say, an MCP call) and post-process the result
through a Lex closure provided in the agent config, this is the
hook to use.

## Open follow-ups upstream

Three language-track items still open on the meta tracker (#220):

- #208 — spec-checker quantification over records / lists / ADTs.
  This is the one that eliminates `soft-agent`'s `BindingsFn`
  flattening hack. The plumbing lives in
  `crates/soft-agent/src/runner.rs:57` (the `BindingsFn` type alias),
  with the per-action call site in `Runner::dispatch` and the builder
  hook at `RunnerBuilder::bindings_fn`. Once #208 lands, the closure
  can be replaced by passing the action record straight through to
  `evaluate_gate_compiled_traced` and letting the spec body destructure
  it.
- #209 — refinement types on function signatures (depends on #208).
- #206 — canonical AST as primary compilation surface (largest lift).

These are noted here so the soft team has the upstream roadmap
visible. None of them is urgent for soft-agent's current
deployments.
