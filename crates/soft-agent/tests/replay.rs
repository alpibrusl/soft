//! Tests `soft_agent::replay::scan_trace` against real traces produced by
//! the runner. Verifies the invariant logic catches gate-bypass attempts.

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::json;
use soft_agent::{
    replay::scan_trace, A2aMessage, Action, AgentConfig, Effect, Gate, Mailbox, Runner,
};
use tempfile::tempdir;

const ALWAYS_DENY_SPEC: &str = "
spec deny_all {
  forall x :: Int:
    false
}
";
const HOST: &str = "fn host() -> Int { 0 }";

#[test]
fn scan_counts_allow_runs_correctly() {
    // No gate → every proposed action is executed; allowed count is 0
    // (no gate verdict ever recorded), executed count > allowed → would
    // be flagged as a violation. We use this test to confirm the count
    // logic, not the invariant — see next test for the invariant case.
    let agent = AgentConfig::new("nogate")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .handle("Ping", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "Pong".into(),
                payload: json!({}),
            }]
        })
        .build()
        .unwrap();
    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "Ping".into(),
            payload: json!(null),
        })
        .unwrap();
    runner.drain().unwrap();

    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).unwrap();
    let run_id = runner.finalize(&store).unwrap();
    let tree = store.load_trace(&run_id.0).unwrap();

    let c = scan_trace(&tree);
    assert_eq!(c.proposed, 1);
    assert_eq!(c.executed, 1);
    assert_eq!(c.allowed, 0, "no gate ⇒ no verdicts recorded");
    assert_eq!(c.denied, 0);
    assert!(!c.is_gated(), "no gate.verdict events ⇒ ungated");
    assert!(
        !c.has_violation(),
        "ungated runs are not violations — there's no Deny to bypass"
    );
}

#[test]
fn scan_counts_a_real_deny_run() {
    let gate = Gate::from_sources(&[ALWAYS_DENY_SPEC], HOST).unwrap();
    let agent = AgentConfig::new("gated")
        .peers(["caller"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .gate(gate)
        .bindings_fn(Box::new(|_state, _action| {
            let mut m = IndexMap::new();
            m.insert("x".into(), LexValue::Int(0));
            m
        }))
        .handle("Ping", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "Pong".into(),
                payload: json!({}),
            }]
        })
        .build()
        .unwrap();
    sender
        .send(A2aMessage {
            from: "caller".into(),
            topic: "Ping".into(),
            payload: json!(null),
        })
        .unwrap();
    runner.drain().unwrap();

    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).unwrap();
    let run_id = runner.finalize(&store).unwrap();
    let tree = store.load_trace(&run_id.0).unwrap();

    let c = scan_trace(&tree);
    assert_eq!(c.proposed, 1);
    assert_eq!(c.denied, 1, "spec returned false");
    assert_eq!(c.allowed, 0);
    assert_eq!(c.executed, 0, "denied actions must not execute");
    assert!(!c.has_violation(), "executed (0) <= allowed (0)");
}
