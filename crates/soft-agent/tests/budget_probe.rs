//! Probe: does soft-agent on lex 0.3 honor `[budget(N)]` on a Lex
//! function? If yes, we can plumb a `--budget` CLI flag into
//! soft-runner; if no, this test surfaces the failure mode.
//!
//! Tests both the happy path (budget high enough) and the rejection
//! path (budget exhausted). Setup is deliberately tiny — a single
//! function annotated with `[budget(N)]` and called via `LexHost`.

use soft_agent::{LexHost, Policy};

const SOURCE: &str = r#"
fn step() -> [budget(10)] Int { 1 }
fn run_three() -> [budget(10)] Int {
  step() + step() + step()
}
"#;

fn build_host_with_budget(ceiling: Option<u64>) -> LexHost {
    let host = LexHost::from_source(SOURCE).expect("compile");
    let mut policy = Policy::permissive();
    policy.budget = ceiling;
    host.with_policy(policy)
}

#[test]
fn budget_unbounded_allows_any_calls() {
    let host = build_host_with_budget(None);
    let result = host.call("run_three", vec![]).expect("call ok");
    assert_eq!(result.value.to_json().as_i64(), Some(3));
}

#[test]
fn budget_high_enough_allows_call() {
    // run_three calls step() three times; each [budget(10)] call
    // deducts cost ≥1 from the pool. Ceiling 1000 should be plenty.
    let host = build_host_with_budget(Some(1000));
    let result = host.call("run_three", vec![]).expect("call ok");
    assert_eq!(result.value.to_json().as_i64(), Some(3));
}

#[test]
fn budget_exhausted_surfaces_error() {
    // Set ceiling to 0 — even the first call should be rejected.
    let host = build_host_with_budget(Some(0));
    let result = host.call("run_three", vec![]);
    let err = result.expect_err("expected budget-exceeded");
    let msg = format!("{err}");
    assert!(
        msg.contains("budget exceeded"),
        "expected budget-exceeded error, got: {msg}"
    );
}

/// The lex 0.3 type checker requires every Lex caller of a
/// `[budget(N)]` function to declare a compatible budget effect on
/// its own return type. Pins the contract.
#[test]
fn calling_budgeted_callee_from_unbudgeted_lex_caller_is_a_type_error() {
    let src = r#"
        fn budgeted() -> [budget(10)] Int { 1 }
        fn entry() -> Int { budgeted() }
    "#;
    let result = LexHost::from_source(src);
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected typecheck failure, but compile succeeded"),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("budget(10)") && msg.contains("EffectNotDeclared"),
        "expected EffectNotDeclared for budget(10), got: {msg}"
    );
}

/// Practical consequence for soft-agent: when soft-runner invokes a
/// `[budget(N)]`-annotated lex handler from Rust (via
/// `host.call(fn_name, args)`), the runtime currently does NOT charge
/// N for the entry call itself — only for `Op::Call`s the handler
/// makes downstream. So `[budget(50)]` on a top-level handler bounds
/// the *helper-function* + *stdlib-call* depth inside the handler,
/// not the act of dispatching to it.
///
/// This test pins the behavior so we'd notice if it changes upstream.
#[test]
fn rust_caller_into_budgeted_handler_with_internal_calls_deducts_correctly() {
    // lex 0.3 budget effects don't subsume — caller and callee
    // declare the same `[budget(N)]` value (mirrors the upstream
    // budget_runtime.rs test pattern).
    let src = r#"
        fn helper() -> [budget(10)] Int { 1 }
        fn handler() -> [budget(10)] Int {
            helper() + helper() + helper()
        }
    "#;
    let host = LexHost::from_source(src).expect("compile");

    // Ceiling 100 — plenty for 3 × 5 = 15 deductions plus overhead.
    let mut p = Policy::permissive();
    p.budget = Some(100);
    let h = host.with_policy(p);
    let v = h.call("handler", vec![]).expect("ceiling 100 ok");
    assert_eq!(v.value.to_json().as_i64(), Some(3));

    // Ceiling 1 — first internal helper() call should fail.
    let host = LexHost::from_source(src).expect("compile");
    let mut p = Policy::permissive();
    p.budget = Some(1);
    let h = host.with_policy(p);
    let err = h
        .call("handler", vec![])
        .expect_err("ceiling 1 should fail");
    assert!(
        format!("{err}").contains("budget exceeded"),
        "expected budget-exceeded, got: {err}"
    );
}
