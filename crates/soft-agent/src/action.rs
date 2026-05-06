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
        topic: String,
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

    /// Decode an action from a JSON record produced by a Lex handler.
    ///
    /// Convention (v3): the record carries a `kind` discriminator
    /// (`call_mcp`, `send_a2a`, `local_llm`, `cloud_llm`) plus the fields
    /// relevant to that kind. Unrelated fields are ignored; missing
    /// fields default to empty.
    pub fn from_json(j: &Value) -> Result<Action, crate::Error> {
        let kind = j
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::Spec("action record missing `kind`".into()))?;
        let s = |key| {
            j.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let parse_json_str_field = |key: &str| -> Value {
            let raw = j.get(key);
            match raw {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(other) => other.clone(),
                None => Value::Null,
            }
        };
        match kind {
            "call_mcp" => Ok(Action::CallMcp {
                server: s("server"),
                tool: s("tool"),
                args: parse_json_str_field("args_json"),
            }),
            "send_a2a" => Ok(Action::SendA2a {
                peer: s("peer"),
                topic: s("a2a_topic"),
                payload: parse_json_str_field("payload_json"),
            }),
            "local_llm" => Ok(Action::LocalLlm { prompt: s("prompt") }),
            "cloud_llm" => Ok(Action::CloudLlm { prompt: s("prompt") }),
            other => Err(crate::Error::Spec(format!(
                "unknown action kind: `{other}`"
            ))),
        }
    }

    /// Decode from a Lex `Value` produced by a Lex handler.
    pub fn from_lex_value(v: &lex_bytecode::Value) -> Result<Action, crate::Error> {
        Self::from_json(&v.to_json())
    }
}
