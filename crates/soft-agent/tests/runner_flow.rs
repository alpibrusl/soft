//! End-to-end runner tests: mailbox → handler → (optional gate) →
//! executor → trace, persisted to a temp lex_store::Store.

use serde_json::json;
use soft_agent::{
    A2aMessage, Action, AgentConfig, Effect, Mailbox, MockExecutor, Runner,
};
use tempfile::tempdir;

fn build_test_agent(name: &str) -> soft_agent::Agent {
    AgentConfig::new(name)
        .peers(["peer"])
        .effects([Effect::A2a])
        .build()
        .expect("config valid")
}

#[test]
fn runner_dispatches_and_executes_without_gate() {
    let (mailbox, sender) = Mailbox::new();
    let executor = Box::<MockExecutor>::default();

    let mut runner = Runner::builder()
        .agent(build_test_agent("ping_pong"))
        .state(json!({}))
        .mailbox(mailbox)
        .executor(executor)
        .handle("Ping", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "Pong".into(),
                payload: json!({"reply": "pong"}),
            }]
        })
        .build()
        .expect("runner builds");

    sender
        .send(A2aMessage {
            from: "peer".into(),
            topic: "Ping".into(),
            payload: json!(null),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    assert_eq!(report.total_allowed, 1);
    assert_eq!(report.total_denied, 0);

    // Persist + reload trace, verify shape.
    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).expect("store");
    let run_id = runner.finalize(&store).expect("finalize");

    let tree = store.load_trace(&run_id.0).expect("trace reloads");
    assert_eq!(tree.run_id, run_id.0);
    assert!(
        tree.nodes.iter().any(|n| n.target == "a2a.received"),
        "trace must contain a2a.received"
    );
    assert!(
        tree.nodes.iter().any(|n| n.target == "action.proposed"),
        "trace must contain action.proposed"
    );
    assert!(
        tree.nodes.iter().any(|n| n.target == "action.executed"),
        "trace must contain action.executed"
    );
}

#[test]
fn runner_denies_unknown_mcp_server() {
    let agent = AgentConfig::new("vehicle")
        .peers(["depot"])
        .mcp_servers(["telemetry"]) // ocpp NOT allowed
        .effects([Effect::Mcp, Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .handle("TryOcpp", |_state, _msg| {
            vec![Action::CallMcp {
                server: "ocpp".into(),
                tool: "remote_start_transaction".into(),
                args: json!({}),
            }]
        })
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "depot".into(),
            topic: "TryOcpp".into(),
            payload: json!(null),
        })
        .unwrap();

    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    assert_eq!(report.total_allowed, 0);
    assert_eq!(report.total_denied, 1, "ocpp should be blocked by allowlist");
}

#[test]
fn runner_returns_handler_not_registered() {
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(build_test_agent("strict"))
        .mailbox(mailbox)
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "peer".into(),
            topic: "Unknown".into(),
            payload: json!(null),
        })
        .unwrap();

    let err = runner.step().unwrap_err();
    assert!(matches!(err, soft_agent::Error::HandlerNotRegistered(_)));
}

#[test]
fn runner_idle_when_mailbox_empty() {
    let (mailbox, _sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(build_test_agent("idle"))
        .mailbox(mailbox)
        .build()
        .unwrap();

    let step = runner.step().unwrap();
    assert!(matches!(step, soft_agent::StepReport::Idle));
}
