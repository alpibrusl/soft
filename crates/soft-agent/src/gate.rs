//! Action gate — wraps `spec_checker::evaluate_gate_compiled`.
//!
//! soft-agent's policy is **Inconclusive → Deny by default** (fail-safe).
//! The verdict variant + `spec_name` + `reason` are recorded to the trace
//! alongside the Allow/Deny verdict so audit replay can distinguish the
//! two.

use indexmap::IndexMap;
use lex_bytecode::Value;
use spec_checker::{evaluate_gate_compiled, parse_spec, Spec};

use crate::{Action, Error};

pub use spec_checker::GateVerdict as Verdict;

/// A compiled gate: a list of specs plus the host Lex program that any
/// helper functions in spec bodies refer to.
pub struct Gate {
    specs: Vec<Spec>,
    program: lex_bytecode::Program,
}

impl Gate {
    /// Build a gate from spec sources plus a host Lex source.
    ///
    /// Each entry in `spec_sources` is parsed via
    /// [`spec_checker::parse_spec`]. The host source is parsed →
    /// canonicalised → type-checked → compiled to bytecode once; that
    /// compiled program is reused for every gate evaluation. For Phase 1
    /// specs that are purely arithmetic over bindings, the host can be a
    /// trivial stub like `fn _host() -> Int { 0 }`.
    pub fn from_sources(spec_sources: &[&str], host_lex_source: &str) -> Result<Self, Error> {
        let specs = spec_sources
            .iter()
            .map(|src| parse_spec(src).map_err(|e| Error::Spec(format!("parse: {e}"))))
            .collect::<Result<Vec<_>, _>>()?;

        let prog = lex_syntax::parse_source(host_lex_source)
            .map_err(|e| Error::Spec(format!("host parse: {e:?}")))?;
        let stages = lex_ast::canonicalize_program(&prog);
        if let Err(errs) = lex_types::check_program(&stages) {
            return Err(Error::Spec(format!("host typecheck: {errs:?}")));
        }
        let program = lex_bytecode::compile_program(&stages);

        Ok(Gate { specs, program })
    }

    /// Evaluate every spec against `bindings` and return the first
    /// non-Allow verdict (or Allow if all pass).
    pub fn evaluate(&self, bindings: &IndexMap<String, Value>) -> Verdict {
        evaluate_gate_compiled(&self.specs, bindings, &self.program)
    }

    /// Number of specs registered.
    pub fn spec_count(&self) -> usize {
        self.specs.len()
    }
}

pub(crate) fn action_to_json(action: &Action) -> serde_json::Value {
    match action {
        Action::CallMcp { server, tool, args } => serde_json::json!({
            "kind": "call_mcp", "server": server, "tool": tool, "args": args,
        }),
        Action::SendA2a { peer, payload } => serde_json::json!({
            "kind": "send_a2a", "peer": peer, "payload": payload,
        }),
        Action::LocalLlm { prompt } => serde_json::json!({
            "kind": "local_llm", "prompt": prompt,
        }),
        Action::CloudLlm { prompt } => serde_json::json!({
            "kind": "cloud_llm", "prompt": prompt,
        }),
    }
}
