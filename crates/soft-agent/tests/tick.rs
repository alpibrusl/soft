//! Tick mechanism: periodic self-messages drive a registered handler
//! without an external sender.

use serde_json::json;
use soft_agent::{Action, AgentConfig, Effect, Mailbox, Runner};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn tick_fires_periodically_and_runs_handler() {
    let agent = AgentConfig::new("ticker")
        .peers(["self"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let counter = Arc::new(Mutex::new(0usize));
    let counter_for_handler = Arc::clone(&counter);

    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .tick(Duration::from_millis(20), "Tick", sender.clone())
        .handle("Tick", move |_state, _msg| {
            *counter_for_handler.lock().unwrap() += 1;
            Vec::new()
        })
        .build()
        .unwrap();

    // Wait for ~5 ticks then drain.
    std::thread::sleep(Duration::from_millis(120));
    let report = runner.drain().unwrap();
    let n = *counter.lock().unwrap();
    assert!(
        (4..=8).contains(&n),
        "expected 4..=8 ticks in ~120ms with 20ms interval, got {n}"
    );
    assert_eq!(report.messages, n);

    drop(runner); // Closes mailbox; tick thread exits on next send.
}

#[test]
fn tick_handler_can_emit_actions() {
    let agent = AgentConfig::new("self_pinger")
        .peers(["self", "peer"])
        .effects([Effect::A2a])
        .build()
        .unwrap();

    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .tick(Duration::from_millis(20), "Tick", sender.clone())
        .handle("Tick", |_state, _msg| {
            vec![Action::SendA2a {
                peer: "peer".into(),
                topic: "Notice".into(),
                payload: json!({"hi": "world"}),
            }]
        })
        .build()
        .unwrap();

    std::thread::sleep(Duration::from_millis(60));
    let report = runner.drain().unwrap();
    assert!(report.messages >= 1);
    assert_eq!(
        report.total_allowed, report.messages,
        "every tick emits one allowed action"
    );
}
