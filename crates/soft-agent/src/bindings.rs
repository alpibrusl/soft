//! Default `BindingsFn` for spec gates.
//!
//! [`default_float_bindings`] walks an agent's state plus the action
//! being evaluated and emits one `LexValue::Float` binding per
//! top-level numeric field. Action-derived bindings override
//! state-derived bindings on collision so a spec can reason about a
//! field both agents might carry under the same name (e.g. `power_kw`
//! in state vs in an outgoing GrantSession payload — the latter is
//! the value the gate should validate, since it's what the agent is
//! committing to).
//!
//! Action sources by variant:
//!   - [`Action::SendA2a`]: walks the `payload` object.
//!   - [`Action::CallMcp`]: walks the `args` object.
//!   - [`Action::LocalLlm`] / [`Action::CloudLlm`]: no numeric scalars.
//!
//! Only top-level fields are extracted; nested objects are ignored.
//! Use a custom [`BindingsFn`](crate::BindingsFn) when a spec needs
//! deeper structure or non-Float types.

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::Value;

use crate::Action;

/// Default `BindingsFn` for soft-runner-style deployments. See module
/// docs for semantics.
pub fn default_float_bindings(state: &Value, action: &Action) -> IndexMap<String, LexValue> {
    let mut out = IndexMap::new();
    if let Some(obj) = state.as_object() {
        for (k, v) in obj {
            if let Some(f) = v.as_f64() {
                out.insert(k.clone(), LexValue::Float(f));
            }
        }
    }
    let action_obj = match action {
        Action::SendA2a { payload, .. } => payload.as_object(),
        Action::CallMcp { args, .. } => args.as_object(),
        Action::LocalLlm { .. } | Action::CloudLlm { .. } => None,
    };
    if let Some(obj) = action_obj {
        for (k, v) in obj {
            if let Some(f) = v.as_f64() {
                // Action wins on collision: re-insert overrides.
                out.insert(k.clone(), LexValue::Float(f));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn float(b: &IndexMap<String, LexValue>, k: &str) -> Option<f64> {
        match b.get(k)? {
            LexValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    #[test]
    fn pulls_top_level_floats_from_state() {
        let state = json!({"current_kw": 30.0, "budget_kw": 100.0, "name": "depot"});
        let action = Action::LocalLlm {
            prompt: "irrelevant".into(),
        };
        let b = default_float_bindings(&state, &action);
        assert_eq!(float(&b, "current_kw"), Some(30.0));
        assert_eq!(float(&b, "budget_kw"), Some(100.0));
        assert!(b.get("name").is_none(), "non-numeric fields skipped");
    }

    #[test]
    fn pulls_top_level_floats_from_send_a2a_payload() {
        let state = json!({"current_kw": 30.0});
        let action = Action::SendA2a {
            peer: "vehicle".into(),
            topic: "GrantSession".into(),
            payload: json!({"power_kw": 50, "charger_id": "c-1"}),
        };
        let b = default_float_bindings(&state, &action);
        assert_eq!(float(&b, "current_kw"), Some(30.0));
        assert_eq!(float(&b, "power_kw"), Some(50.0));
        assert!(b.get("charger_id").is_none());
    }

    #[test]
    fn pulls_top_level_floats_from_call_mcp_args() {
        let state = json!({"soc": 0.85});
        let action = Action::CallMcp {
            server: "telemetry".into(),
            tool: "report".into(),
            args: json!({"voltage": 240.0, "label": "main"}),
        };
        let b = default_float_bindings(&state, &action);
        assert_eq!(float(&b, "soc"), Some(0.85));
        assert_eq!(float(&b, "voltage"), Some(240.0));
        assert!(b.get("label").is_none());
    }

    #[test]
    fn action_overrides_state_on_key_collision() {
        let state = json!({"power_kw": 10.0, "budget_kw": 100.0});
        let action = Action::SendA2a {
            peer: "v".into(),
            topic: "Grant".into(),
            payload: json!({"power_kw": 50.0}),
        };
        let b = default_float_bindings(&state, &action);
        // Action's power_kw shadows state's.
        assert_eq!(float(&b, "power_kw"), Some(50.0));
        // State-only fields still come through.
        assert_eq!(float(&b, "budget_kw"), Some(100.0));
    }

    #[test]
    fn empty_state_and_non_numeric_action_yields_empty_bindings() {
        let state = json!({});
        let action = Action::SendA2a {
            peer: "x".into(),
            topic: "Y".into(),
            payload: json!({"only": "strings"}),
        };
        let b = default_float_bindings(&state, &action);
        assert!(b.is_empty());
    }

    #[test]
    fn local_and_cloud_llm_actions_contribute_no_bindings() {
        let state = json!({"k": 1.0});
        let local = Action::LocalLlm { prompt: "p".into() };
        let cloud = Action::CloudLlm { prompt: "p".into() };
        let bl = default_float_bindings(&state, &local);
        let bc = default_float_bindings(&state, &cloud);
        assert_eq!(bl.len(), 1);
        assert_eq!(bc.len(), 1);
    }
}
