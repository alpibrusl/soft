//! Unit tests for [`Gate`] in isolation — no runner, no mailbox, just
//! parse a spec and evaluate against hand-built bindings.

use indexmap::IndexMap;
use lex_bytecode::Value;
use soft_agent::{Gate, Verdict};

const HOST: &str = "fn host() -> Int { 0 }";

const ALWAYS_ALLOW: &str = "
spec always_allow {
  forall x :: Int:
    true
}
";

const ALWAYS_DENY: &str = "
spec always_deny {
  forall x :: Int:
    false
}
";

const POSITIVE_ONLY: &str = "
spec positive_only {
  forall x :: Int:
    x >= 0
}
";

#[test]
fn gate_allows_when_spec_holds() {
    let gate = Gate::from_sources(&[ALWAYS_ALLOW], HOST).expect("gate must build");
    let mut bindings = IndexMap::new();
    bindings.insert("x".to_string(), Value::Int(7));
    assert!(matches!(gate.evaluate(&bindings), Verdict::Allow));
}

#[test]
fn gate_denies_when_spec_fails() {
    let gate = Gate::from_sources(&[ALWAYS_DENY], HOST).expect("gate must build");
    let mut bindings = IndexMap::new();
    bindings.insert("x".to_string(), Value::Int(7));
    let v = gate.evaluate(&bindings);
    match v {
        Verdict::Deny { spec_name, .. } => assert_eq!(spec_name, "always_deny"),
        other => panic!("expected Deny, got {other:?}"),
    }
}

#[test]
fn gate_evaluates_predicate() {
    let gate = Gate::from_sources(&[POSITIVE_ONLY], HOST).expect("gate must build");

    let mut ok = IndexMap::new();
    ok.insert("x".to_string(), Value::Int(3));
    assert!(matches!(gate.evaluate(&ok), Verdict::Allow));

    let mut bad = IndexMap::new();
    bad.insert("x".to_string(), Value::Int(-1));
    assert!(matches!(gate.evaluate(&bad), Verdict::Deny { .. }));
}

#[test]
fn gate_first_failing_spec_wins() {
    let gate = Gate::from_sources(&[ALWAYS_ALLOW, ALWAYS_DENY], HOST).expect("gate must build");
    let mut bindings = IndexMap::new();
    bindings.insert("x".to_string(), Value::Int(0));
    let v = gate.evaluate(&bindings);
    // ALWAYS_ALLOW passes, then ALWAYS_DENY denies.
    match v {
        Verdict::Deny { spec_name, .. } => assert_eq!(spec_name, "always_deny"),
        other => panic!("expected Deny from second spec, got {other:?}"),
    }
}
