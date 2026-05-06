//! End-to-end LexHost tests: compile a small Lex program, call functions,
//! verify the trace tree captures the call.

use lex_bytecode::Value;
use soft_agent::LexHost;

const HELPERS: &str = r#"
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

#[test]
fn lex_host_calls_simple_function() {
    let host = LexHost::from_source(HELPERS).expect("compiles");
    let result = host
        .call("projected_load", vec![Value::Float(80.0), Value::Float(30.0)])
        .expect("call ok");
    assert_eq!(result.value, Value::Float(110.0));
    assert_eq!(result.tree.root_target, "projected_load");
}

#[test]
fn lex_host_call_records_nested_calls_in_trace() {
    let host = LexHost::from_source(HELPERS).expect("compiles");
    let result = host
        .call(
            "under_budget",
            vec![
                Value::Float(80.0),
                Value::Float(30.0),
                Value::Float(100.0),
                Value::Float(20.0),
            ],
        )
        .expect("call ok");
    assert_eq!(result.value, Value::Bool(true));
    // Nested helper calls (`projected_load`, `budget_total`) should appear
    // as nodes in the trace tree.
    let names = collect_call_names(&result.tree.nodes);
    assert!(
        names.iter().any(|n| n == "projected_load"),
        "expected projected_load in trace, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "budget_total"),
        "expected budget_total in trace, got: {names:?}"
    );
}

#[test]
fn lex_host_under_budget_denies_when_over() {
    let host = LexHost::from_source(HELPERS).expect("compiles");
    let result = host
        .call(
            "under_budget",
            vec![
                Value::Float(80.0),
                Value::Float(50.0), // 80 + 50 = 130
                Value::Float(100.0),
                Value::Float(20.0), // 100 + 20 = 120
            ],
        )
        .expect("call ok");
    assert_eq!(result.value, Value::Bool(false));
}

#[test]
fn lex_host_unknown_function_errors() {
    let host = LexHost::from_source(HELPERS).expect("compiles");
    let err = host.call("not_a_thing", vec![]).unwrap_err();
    assert!(matches!(err, soft_agent::Error::Spec(_)));
}

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
