//! Verifies `Gate::evaluate_traced` (new in spec-checker 0.2.1) folds
//! spec-body helper calls into a caller-supplied trace tree.

use indexmap::IndexMap;
use lex_bytecode::Value;
use soft_agent::{Gate, Verdict};

const HOST_WITH_HELPERS: &str = r#"
fn projected_load(current :: Float, delta :: Float) -> Float {
  current + delta
}

fn budget_total(grid :: Float, pv :: Float) -> Float {
  grid + pv
}

fn under_budget(current :: Float, delta :: Float, grid :: Float, pv :: Float) -> Bool {
  projected_load(current, delta) <= budget_total(grid, pv)
}
"#;

const SPEC_USING_HELPERS: &str = r#"
spec grid_budget {
  forall current :: Float, delta :: Float, grid :: Float, pv :: Float:
    under_budget(current, delta, grid, pv)
}
"#;

fn collect_call_names(nodes: &[lex_trace::TraceNode]) -> Vec<String> {
    let mut out = Vec::new();
    for n in nodes {
        if matches!(n.kind, lex_trace::TraceNodeKind::Call) {
            out.push(n.target.clone());
        }
        out.extend(collect_call_names(&n.children));
    }
    out
}

fn budget_bindings(current: f64, delta: f64, grid: f64, pv: f64) -> IndexMap<String, Value> {
    let mut m = IndexMap::new();
    m.insert("current".into(), Value::Float(current));
    m.insert("delta".into(), Value::Float(delta));
    m.insert("grid".into(), Value::Float(grid));
    m.insert("pv".into(), Value::Float(pv));
    m
}

fn run_traced(gate: &Gate, bindings: &IndexMap<String, Value>) -> (Verdict, lex_trace::TraceTree) {
    let recorder = lex_trace::Recorder::new();
    let handle = recorder.handle();
    let h_for_closure = handle.clone();
    let verdict = gate.evaluate_traced(bindings, move || {
        Box::new(h_for_closure.clone()) as Box<dyn lex_bytecode::vm::Tracer>
    });
    let tree = handle.finalize(
        "test".to_string(),
        serde_json::Value::Null,
        None,
        None,
        0,
        0,
    );
    (verdict, tree)
}

#[test]
fn evaluate_traced_records_helper_calls_into_handle() {
    // The Vm fires `enter_call` for bytecode Call ops only (the top-level
    // `Vm::call` entry point becomes the trace's root; its name is set
    // by whatever `Handle::finalize` is given). So the spec body's call
    // into `under_budget` is the entry point, and the trace records the
    // calls *inside* it: `projected_load` and `budget_total`.
    let gate = Gate::from_sources(&[SPEC_USING_HELPERS], HOST_WITH_HELPERS).unwrap();
    let (verdict, tree) = run_traced(&gate, &budget_bindings(80.0, 30.0, 100.0, 20.0));
    assert!(matches!(verdict, Verdict::Allow));
    let names = collect_call_names(&tree.nodes);
    assert!(
        names.iter().any(|n| n == "projected_load"),
        "expected projected_load call in spec trace, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "budget_total"),
        "expected budget_total call in spec trace, got: {names:?}"
    );
}

#[test]
fn evaluate_traced_returns_deny_when_predicate_fails() {
    let gate = Gate::from_sources(&[SPEC_USING_HELPERS], HOST_WITH_HELPERS).unwrap();
    // 80 + 50 = 130 > 100 + 20 = 120 → Deny
    let (verdict, tree) = run_traced(&gate, &budget_bindings(80.0, 50.0, 100.0, 20.0));
    match verdict {
        Verdict::Deny { spec_name, .. } => assert_eq!(spec_name, "grid_budget"),
        other => panic!("expected Deny, got {other:?}"),
    }
    // The trace still captured the helper calls during evaluation.
    let names = collect_call_names(&tree.nodes);
    assert!(
        names.iter().any(|n| n == "projected_load"),
        "expected projected_load in deny path's trace, got: {names:?}"
    );
}
