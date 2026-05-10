# lex-lang 0.5 upgrade — soft-agent impact

**Status**: 📝 evaluation only. lex-lang `0.5.0` published 2026-05-09;
soft is currently pinned to `0.3` (`Cargo.toml:18`). This document
audits the `0.4.0` + `0.5.0` changelogs against soft's actual call
sites and proposes adoption work, but no code has been changed yet.
Workspace bump and CI verification are follow-ups (see §"Next steps").

The audit follows the same structure as `docs/lex-lang-0.3-upgrade.md`:
what landed upstream, what soft *has* to do, what soft *can* do.

## TL;DR

soft's consumed lex-lang surface is **unchanged** between 0.3 and 0.5.
Every API soft calls today (the list in
`docs/lex-lang-0.3-upgrade.md` §"Mandatory: nothing on the API side")
keeps its signature and semantics. The mandatory delta is the
one-line workspace bump.

The two breaking surfaces called out in the upstream changelog —
`OperationFormat` versioning rotating every `OpId` (#244, 0.4) and
the `rand 0.10` / `getrandom 0.4` trait import rotation (0.5) — both
miss soft entirely: soft doesn't write ops to the lex op log, and
doesn't import `rand`/`getrandom` directly (verified against
`crates/soft-agent/Cargo.toml`).

The interesting question is what soft *should* adopt. The op-log
performance work (#261 packfiles + GC + delta encoding) and the
multi-writer CAS branch advance (#262) are both future-relevant but
inert today, because soft only writes traces under
`<store>/traces/<run_id>/trace.json` via `Store::save_trace` — never
ops. The single most-valuable adoption is wiring **Trace
attestations** (#246, completed by #257) into `TraceWriter::finalize`
so soft's runs become first-class objects in the lex-vcs attestation
graph. That's a small, contained lift; everything else can wait until
soft adopts the op-log-as-decision-log model (out of scope for this
upgrade).

## What landed upstream

Two minor releases between soft's pin and the new tip. Both are
data-layer-additive: the on-disk shape gains optional fields and new
file kinds, but pre-existing readers keep working.

### 0.4.0 — agent-VCS roadmap (2026-05-08)

Closes the seven gaps from the lex-vcs review. Every change is
additive at the data layer (Option fields with
`skip_serializing_if`, new enum variants, new endpoints).

| Issue | What | Soft-relevant? |
|---|---|---|
| #245 | Machine-checkable branch advancement gates (`policy.required_attestations`) | Future — when soft commits ops |
| #246 | `AttestationKind::Trace { run_id, root_target }` + `by-run` index + `lex run --trace` auto-emits | **Yes** — direct fit for `TraceWriter::finalize` |
| #247 | Cost accounting on ops (`AddFunction.budget_cost`, `ModifyBody.from/to_budget`, `ChangeEffectSig.from/to_budget`) | Future — when soft commits ops |
| #248 | Retroactive producer quarantine (`AttestationKind::ProducerBlock`/`ProducerUnblock`) + write-time gate | Future — when soft emits attestations |
| #242 | Append-only sync (push side): `POST /v1/ops/batch`, `/v1/attestations/batch`, `lex op push`, `lex attest push` | Indirect — soft doesn't run `lex serve` |
| #244 | `OperationFormat` enum + `lex store migrate-ops` | No — soft writes no ops |
| #243 | OpId stability spec (golden tests + canonical-form invariants) | No — internal to lex-vcs |

### 0.5.0 — performance + 0.4 follow-ups (2026-05-09)

The op-log performance roadmap and the post-0.4.0 limitation
follow-ups.

| Issue | What | Soft-relevant? |
|---|---|---|
| #257 | Ops-during-run Trace attestations (`Store::record_op_trace`, `record_run_committed_ops_since`) — populates `op_id` on the variant #246 added | **Yes** — completes the trace-attestation story |
| #260 | Append-only sync (pull side): `lex op pull`, `lex attest pull`, `GET /v1/ops/since`, `/v1/attestations/since` | Indirect — same shape as #242 |
| #262 | Multi-writer CAS branch advance (`Store::set_branch_head_op_cas`, `StoreError::Contention`, 503 + `Retry-After` on the HTTP API) | Future — when soft commits ops |
| #258 | Attestation-cascade migration (`migrate::plan_attestation_migration`, `apply_attestation_migration`) | No — soft has no attestations to cascade |
| #256 | Walk-back producer-block gate (`Branch.last_gate_checkpoint`, `invalidate_gate_checkpoints`) | Future — when soft emits attestations |
| #261 slice 1 | Op-log packfiles (`OpLog::repack`, `<store>/ops/pack-<hash>.{pack,idx}`) | No — soft writes no ops |
| #261 slice 2 | Predicate-driven op-log GC (`Store::plan_gc`, `apply_gc`, `lex op gc`) | No — soft writes no ops |
| #261 slice 3 | Delta-encoded stage bytes (`<stage_id>.delta.json`, `DELTA_RATIO_THRESHOLD = 50%`, `DELTA_CHAIN_CAP = 32`) | No — soft writes no stages |
| deps | `getrandom 0.2→0.4`, `rand 0.9→0.10`, plus `toml`, `rusqlite`, `notify`, `action-gh-release` | No — soft doesn't import these directly |

## What soft-agent has to do

### Mandatory: nothing on the API side

Same surface that 0.3 documented. Re-listed here so a future audit
can diff:

- `lex_syntax::parse_source` (`crates/soft-agent/src/lex_host.rs:65`,
  `crates/soft-agent/src/gate.rs:38`)
- `lex_ast::canonicalize_program` (same files)
- `lex_types::check_program` (same files)
- `lex_bytecode::compile_program`, `Program`, `Value`,
  `Value::to_json` / `Value::from_json` (`lex_host.rs:71`,
  `runner.rs:271`)
- `lex_bytecode::vm::{Vm, EffectHandler}` — `Vm::with_handler`,
  `vm.call`, `vm.set_tracer` (`lex_host.rs:111-122`)
- `lex_runtime::{DefaultHandler, Policy}` — `DefaultHandler::new`,
  `Policy::permissive` (`lex_host.rs:51`)
- `lex_trace::{Recorder, Handle, RunId, TraceTree, TraceNode,
  TraceNodeKind}` (`crates/soft-agent/src/trace.rs:13`,
  `crates/soft-agent/src/lex_host.rs:31`,
  `crates/soft-agent/src/replay.rs:11`)
- `lex_store::Store::{open, save_trace, load_trace, list_traces}`
  (`trace.rs:80`, `bin/soft_replay.rs`)
- `spec_checker::{evaluate_gate_compiled,
  evaluate_gate_compiled_traced, parse_spec, Spec, GateVerdict}`
  (`gate.rs:11`)

None of these signatures changed in 0.4 or 0.5. The `lex-store` and
`lex-vcs` additions are net-new methods (`record_op_trace`, `plan_gc`,
`apply_gc`, `set_branch_head_op_cas`, `record_run_committed_ops_since`)
on existing types.

### Mandatory: workspace bump

```toml
[workspace.dependencies]
lex-syntax   = "0.5"
lex-ast      = "0.5"
lex-types    = "0.5"
lex-bytecode = "0.5"
lex-runtime  = "0.5"
lex-store    = "0.5"
lex-trace    = "0.5"
lex-api      = "0.5"
spec-checker = "0.5"
```

The pin comment in `Cargo.toml:13-16` should be rewritten to point
at this document.

### Verification checklist

Run after the bump, before merging:

- `cargo check --workspace --all-targets` — picks up any silent
  trait-resolution change pulled in transitively from the 0.5 dep
  bumps (`getrandom 0.4`, `rand 0.10`).
- `cargo test --workspace` — the existing soft-agent suite covers
  every consumed lex API.
- `./deploy/run-local.sh` — end-to-end on the four-agent fleet,
  same gate as the 0.3 upgrade. Expected to end with
  `4 trace(s) replayed; no violations.`
- `docker compose up --build && docker compose down --timeout 5` —
  the docker path also verifies the trace volume layout still
  parses through `Store::load_trace` after the bump.

If `cargo check` surfaces a `getrandom`/`rand` trait-import error in
soft's own deps (it shouldn't — `crates/soft-agent/Cargo.toml`
doesn't import either), the fix is whatever the upstream changelog
prescribes: `OsRng → SysRng`, `TryRngCore → TryRng`,
`getrandom::getrandom → getrandom::fill`.

## What soft-agent could do (optional)

### Adopt `AttestationKind::Trace` in `TraceWriter::finalize` — recommended

This is the one optional adoption with a meaningful payoff for soft's
audit story.

Today, `crates/soft-agent/src/trace.rs:71-86` finalizes a run by
writing the trace JSON to `<store>/traces/<run_id>/trace.json` and
returning the `RunId`:

```rust
pub fn finalize(self, store: &Store) -> Result<RunId, Error> {
    let tree = TraceTree { /* ... */ };
    store
        .save_trace(&tree)
        .map_err(|e| Error::Trace(format!("save_trace: {e}")))?;
    Ok(self.run_id)
}
```

There's no link in the lex-vcs attestation graph back to that
trace — `lex attest filter --run <run_id>` returns nothing for soft
runs because nothing emitted a `Trace` attestation. Same story for
the lex-tea web UI's per-run drill-down.

With 0.4, `AttestationKind::Trace { run_id, root_target }` exists
specifically for this. With 0.5 (#257), `op_id` on the variant gets
populated when ops are committed during the run, so the by-run index
gives `lex attest filter --run <id>` actual hits. The right shape for
soft is to emit a single `Trace` attestation per run, anchored to
the agent's entry-handler stage_id, with `root_target` set from
`TraceTree.root_target` and `result` mirroring whether `scan_trace`
found a violation:

```rust
pub fn finalize(self, store: &Store) -> Result<RunId, Error> {
    let tree = TraceTree { /* ... */ };
    let run_id = self.run_id.clone();
    store.save_trace(&tree).map_err(/* ... */)?;
    // New: anchor the trace in the attestation graph.
    let counts = crate::replay::scan_trace(&tree);
    let result = if counts.has_violation() {
        AttestationResult::Failed
    } else {
        AttestationResult::Passed
    };
    store
        .record_trace_attestation(/* stage_id */, &run_id, &tree.root_target, result)
        .map_err(/* ... */)?;
    Ok(run_id)
}
```

The blocker is that soft doesn't have a `stage_id` to anchor against:
`LexHost::from_source` (`lex_host.rs:64`) compiles the agent program
in-memory but never publishes it to the store, so there's no stage to
attach the attestation to. Two ways to close that:

1. **Publish the agent program to the store at startup.** One
   `Store::publish_program(&program)` per agent process during
   `LexHost::from_source` (or its caller in `soft-runner`); the
   returned root stage_id becomes the anchor for every subsequent
   `Trace` attestation from that agent. Side-effect bonus: soft
   would also start receiving `TypeCheck` attestations for free
   (`apply_operation_checked` in #245 emits one per op).
2. **Synthesise a synthetic stage per agent name.** Less correct
   but simpler — the by-run index works either way; the
   stage anchor is mostly useful for the lex-tea UI's
   stage-centric views.

Option 1 is the cleaner story; it's also the natural on-ramp into
the broader op-log-as-decision-log model (see §"Out of scope"
below). Recommend doing it as a separate, follow-up slice rather
than bundling it into the 0.5 bump — the bump itself should land
mechanically, with code changes deferred.

### Drop the `BindingsFn` flattening hack — already overdue

This was the optional adoption flagged in `docs/lex-lang-0.3-upgrade.md`
§"Open follow-ups upstream". Spec-checker over records / lists / ADTs
shipped as #208 in 0.3.0 (slices 2 + 3 in the 0.3 changelog), so the
flattening helper at `crates/soft-agent/src/runner.rs:69` is
retire-able today already — the 0.5 bump doesn't change the picture
either way. Worth noting here only because the comment in `runner.rs`
still references the lex-0.2 limitation; rewrite the comment to point
at the 0.3 capability (or do the refactor) when the 0.5 bump lands.

### Don't adopt the op-log/sync surface yet

These all assume soft has committed to the op-log-as-decision-log
model. Worth reading; not worth implementing now:

- **`lex op push` / `lex op pull` (#242, #260)** — content-addressed
  bidirectional sync. Useful future story for cross-machine soft
  fleets, but trace blobs themselves don't flow through these
  endpoints (per `docs/design/trace-vs-vcs.md` upstream — traces
  stay out of the op log; sync is metadata via attestations + a
  separate blob copy). Until soft attaches Trace attestations
  (recommendation above), there's nothing to sync.
- **Multi-writer CAS branch advance (#262)** — only applies on
  `Store::apply_operation`. soft's `Store::save_trace` writes under
  `<store>/traces/<run_id>/trace.json`, and run_ids are unique per
  start, so the four-agent shared-volume deploy has no contention
  hazard today.
- **Op-log packfiles + GC + delta-encoded stages (#261)** — only
  meaningful past ~10k ops. soft writes zero ops.

### Don't adopt the producer-quarantine surface yet

Same gating: requires soft to be emitting attestations under known
`tool_id`s before the retro-block / walk-back gates have anything to
act on. The story is the right one for a future where soft
delegates action proposals to LLMs and wants per-model quarantine —
file an issue and revisit when that becomes concrete.

## SemVer details — what makes this a 0.5 minor bump

Two breaking surfaces in the upstream changelog. Neither hits soft.

1. **`OperationFormat` versioning rotates every `OpId` on V1→V2
   migration** (#244, 0.4). Today only `OperationFormat::V1` is in
   production, so existing stores are unaffected. soft writes no
   ops, so the migration path is moot regardless.

2. **`rand 0.10` / `getrandom 0.4` trait import rotation** (0.5).
   `OsRng → SysRng`, `TryRngCore → TryRng`,
   `getrandom::getrandom → getrandom::fill`. Affects callers of
   `lex_vcs::Keypair::generate` or the `crypto.random` runtime
   handler that took out their own RNG. soft does neither —
   `crates/soft-agent/Cargo.toml` lists only `indexmap`, `serde`,
   `serde_json`, `tiny_http`, `ureq`, plus the lex-* crates.
   `cargo check` after the bump is the verification.

Everything else in 0.4 + 0.5 is additive: new enum variants under
`AttestationKind`, new optional fields on `OperationKind` variants
(behind `skip_serializing_if = "Option::is_none"` so canonical
bytes are unchanged for pre-existing ops), new `Store` methods, new
HTTP endpoints, new file kinds (`pack-<hash>.{pack,idx}`,
`<stage_id>.delta.json`) that loose-file readers ignore.

The minor-version designation is justified — pre-1.0 lex-lang
allows breaking changes on minor bumps, but 0.4 and 0.5 didn't
exercise that allowance for surfaces soft consumes.

## Out of scope (called out for follow-up)

- **Op-log-as-decision-log adoption.** The natural follow-up to
  the Trace-attestation work above: model agent state mutations as
  ops in the lex op log, get TypeCheck/Spec/EffectAudit
  attestations for free, get cross-agent sync via push/pull, get
  retro-block / walk-back gates. Significant design lift; not a
  prerequisite for the 0.5 bump itself. Track separately.
- **`lex-tea` integration for soft runs.** Once Trace attestations
  emit, `lex-tea`'s per-run views become useful for soft. The web
  UI is read-only over `lex-store`, so no soft-side code is needed
  beyond the attestation emit — but a doc pass on
  `deploy/README.md` showing the lex-tea URL alongside the existing
  `soft-trace-viewer` URL would be worthwhile.
- **`spec-checker`'s record/ADT quantifiers refactor**
  (`runner.rs:69` `BindingsFn`). Carried over from
  `docs/lex-lang-0.3-upgrade.md` §"Open follow-ups upstream"; the
  capability shipped in 0.3 (#208 slices 2 + 3) and the hack
  remains. Independent of 0.5; do it when convenient.

## Next steps

1. **Bump the workspace** (`Cargo.toml:18-26`) from `0.3` to `0.5`.
   Rewrite the pin comment at `Cargo.toml:13-16` to reference this
   document instead of the 0.3 upgrade doc.
2. **Run the verification checklist** above. If green, that's the
   bump shipped.
3. **File an issue for the Trace-attestation adoption** with a
   pointer to §"Adopt `AttestationKind::Trace` in
   `TraceWriter::finalize`". Decide between option 1 (publish
   programs to the store) and option 2 (synthetic stage per agent)
   in the issue, not in the bump PR.
4. **Update `README.md`** §"Status" once the bump lands — the
   "Workspace pinned to lex-lang 0.3" line moves to 0.5.
