//! Actions an agent may propose for execution.
//!
//! Each variant maps to a lex-lang v0.2 `std.agent` primitive:
//!
//! | Action      | Lex builtin              | Effect      |
//! |-------------|--------------------------|-------------|
//! | `CallMcp`   | `agent.call_mcp(...)`    | `mcp`       |
//! | `SendA2a`   | `agent.send_a2a(...)`    | `a2a`       |
//! | `LocalLlm`  | `agent.local_complete()` | `llm_local` |
//! | `CloudLlm`  | `agent.cloud_complete()` | `llm_cloud` |
//!
//! Each proposed action is checked against the spec gate (via
//! `spec_checker::evaluate_gate_compiled` in the next slice) before
//! execution. Both `Allow` and `Deny`/`Inconclusive` verdicts are recorded
//! to the trace; soft-agent treats `Inconclusive` as `Deny` by default.

use serde_json::Value;

use crate::Effect;

#[derive(Clone, Debug)]
pub enum Action {
    CallMcp {
        server: String,
        tool: String,
        args: Value,
    },
    SendA2a {
        peer: String,
        payload: Value,
    },
    LocalLlm {
        prompt: String,
    },
    CloudLlm {
        prompt: String,
    },
}

impl Action {
    /// The lex-lang effect this action would carry.
    pub fn effect(&self) -> Effect {
        match self {
            Action::CallMcp { .. } => Effect::Mcp,
            Action::SendA2a { .. } => Effect::A2a,
            Action::LocalLlm { .. } => Effect::LlmLocal,
            Action::CloudLlm { .. } => Effect::LlmCloud,
        }
    }
}
