//! Default `BindingsFn`s for spec gates.
//!
//! [`record_bindings`] is the recommended default for soft-agent on
//! lex-lang 0.3+: it forwards an agent's state and the action's
//! payload (or args) as two record-typed bindings — `s` and `a` —
//! that a spec can destructure directly via field access. This
//! relies on lex-lang #208 (spec-checker quantifiers over user-defined
//! ADTs and structural records).
//!
//! Spec authors write:
//!
//! ```text
//! spec my_invariant {
//!   forall s :: { current_kw :: Float, budget_kw :: Float },
//!          a :: { power_kw :: Float }:
//!     s.current_kw + a.power_kw <= s.budget_kw
//! }
//! ```
//!
//! [`default_float_bindings`] is the legacy flattening helper kept
//! for backward compatibility — specs that still quantify over flat
//! scalar bindings (`forall current_kw :: Float, ...`) continue to
//! work. New specs should prefer [`record_bindings`] for the stronger
//! type contract and clearer scoping.

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::Value;

use crate::Action;

/// Forwards `state` and the action's payload/args as two record-typed
/// bindings — `s` and `a` — for use with lex-lang 0.3+ specs that
/// quantify over records.
///
/// Action sources by variant:
///   - [`Action::SendA2a`]: forwards `payload`.
///   - [`Action::CallMcp`]: forwards `args`.
///   - [`Action::LocalLlm`] / [`Action::CloudLlm`]: forwards an empty
///     record (the spec body's field accesses on `a` will be
///     undefined; specs that intentionally apply only to a/2a or MCP
///     should add an explicit guard, or omit `a` from the quantifier).
///
/// Non-object JSON values (null, scalar, array) are forwarded as
/// `LexValue::Unit`; the spec evaluator treats field access on Unit
/// as Inconclusive (soft-Deny under the runner's policy).
pub fn record_bindings(state: &Value, action: &Action) -> IndexMap<String, LexValue> {
    let mut out = IndexMap::new();
    out.insert("s".to_string(), json_to_record_value(state));

    let action_obj = match action {
        Action::SendA2a { payload, .. } => payload.clone(),
        Action::CallMcp { args, .. } => args.clone(),
        Action::LocalLlm { .. } | Action::CloudLlm { .. } => Value::Object(Default::default()),
    };
    out.insert("a".to_string(), json_to_record_value(&action_obj));
    out
}

fn json_to_record_value(v: &Value) -> LexValue {
    match v {
        Value::Object(map) => {
            let mut fields = IndexMap::new();
            for (k, val) in map {
                fields.insert(k.clone(), json_scalar_to_lex(val));
            }
            LexValue::Record(fields)
        }
        _ => LexValue::Unit,
    }
}

fn json_scalar_to_lex(v: &Value) -> LexValue {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                LexValue::Float(f)
            } else if let Some(i) = n.as_i64() {
                LexValue::Int(i)
            } else {
                LexValue::Unit
            }
        }
        Value::Bool(b) => LexValue::Bool(*b),
        Value::String(s) => LexValue::Str(s.clone()),
        Value::Object(_) => json_to_record_value(v),
        // Lists/null/etc. — not currently consumed by any deploy spec.
        // If a future spec needs them, extend here (LexValue::List, Unit).
        _ => LexValue::Unit,
    }
}

/// Legacy flat-scalar `BindingsFn` for soft-agent on lex-lang 0.2 and
/// for specs that prefer explicit scalar quantifiers. Walks state's
/// top-level Float fields, then merges in floats from the action
/// source: SendA2a payload, CallMcp args. Action bindings override
/// state bindings on key collision.
///
/// New specs should prefer [`record_bindings`] paired with
/// record-typed quantifiers — same contract, fewer name collisions,
/// clearer state-vs-action provenance.
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

    fn record_field(b: &IndexMap<String, LexValue>, rec: &str, field: &str) -> Option<LexValue> {
        let LexValue::Record(map) = b.get(rec)? else {
            return None;
        };
        map.get(field).cloned()
    }

    // ---------- record_bindings ----------

    #[test]
    fn record_bindings_forwards_state_as_s() {
        let state = json!({"current_kw": 30.0, "budget_kw": 100.0});
        let action = Action::LocalLlm { prompt: "p".into() };
        let b = record_bindings(&state, &action);
        assert_eq!(
            record_field(&b, "s", "current_kw"),
            Some(LexValue::Float(30.0))
        );
        assert_eq!(
            record_field(&b, "s", "budget_kw"),
            Some(LexValue::Float(100.0))
        );
    }

    #[test]
    fn record_bindings_forwards_send_a2a_payload_as_a() {
        let state = json!({});
        let action = Action::SendA2a {
            peer: "vehicle".into(),
            topic: "GrantSession".into(),
            payload: json!({"power_kw": 50, "charger_id": "c-1"}),
        };
        let b = record_bindings(&state, &action);
        assert_eq!(
            record_field(&b, "a", "power_kw"),
            Some(LexValue::Float(50.0))
        );
        assert_eq!(
            record_field(&b, "a", "charger_id"),
            Some(LexValue::Str("c-1".into()))
        );
    }

    #[test]
    fn record_bindings_forwards_call_mcp_args_as_a() {
        let state = json!({});
        let action = Action::CallMcp {
            server: "telemetry".into(),
            tool: "report".into(),
            args: json!({"voltage": 240}),
        };
        let b = record_bindings(&state, &action);
        assert_eq!(
            record_field(&b, "a", "voltage"),
            Some(LexValue::Float(240.0))
        );
    }

    #[test]
    fn record_bindings_for_llm_action_yields_empty_a() {
        let state = json!({"k": 1.0});
        let action = Action::LocalLlm { prompt: "p".into() };
        let b = record_bindings(&state, &action);
        let LexValue::Record(map) = b.get("a").unwrap() else {
            panic!("expected Record for `a`");
        };
        assert!(map.is_empty(), "LLM actions carry no record-shaped data");
    }

    #[test]
    fn record_bindings_non_object_state_yields_unit_s() {
        let state = json!("not an object");
        let action = Action::LocalLlm { prompt: "p".into() };
        let b = record_bindings(&state, &action);
        assert!(matches!(b.get("s"), Some(LexValue::Unit)));
    }

    // ---------- default_float_bindings (regression) ----------

    #[test]
    fn legacy_default_float_bindings_still_works() {
        let state = json!({"current_kw": 30.0});
        let action = Action::SendA2a {
            peer: "v".into(),
            topic: "Grant".into(),
            payload: json!({"power_kw": 50.0}),
        };
        let b = default_float_bindings(&state, &action);
        assert_eq!(float(&b, "current_kw"), Some(30.0));
        assert_eq!(float(&b, "power_kw"), Some(50.0));
    }

    #[test]
    fn legacy_action_overrides_state_on_collision() {
        let state = json!({"power_kw": 10.0});
        let action = Action::SendA2a {
            peer: "v".into(),
            topic: "G".into(),
            payload: json!({"power_kw": 50.0}),
        };
        let b = default_float_bindings(&state, &action);
        assert_eq!(float(&b, "power_kw"), Some(50.0));
    }
}
