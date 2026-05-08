//! Multi-agent integration: an `InProcessRouter` routes outbound
//! `SendA2a` actions from one agent's executor into another agent's
//! mailbox. Two Lex handlers, two `Runner`s, one router.

use serde_json::json;
use soft_agent::{A2aMessage, AgentConfig, Effect, InProcessRouter, LexHost, Mailbox, Runner};

/// Vehicle has two handlers:
/// - `on_dispatch`: TMS sends a Dispatch → vehicle requests charging from depot.
/// - `on_grant`: depot replies with GrantSession → vehicle does nothing
///   (terminal in this test).
const VEHICLE_LEX: &str = include_str!("fixtures/multi_agent_vehicle.lex");

/// Depot has one handler: receive RequestSession → reply GrantSession to
/// the requesting vehicle (echoed via `msg.from`).
const DEPOT_LEX: &str = include_str!("fixtures/multi_agent_depot.lex");

#[test]
fn router_delivers_send_a2a_between_two_agents() {
    let router = InProcessRouter::new();

    // -------- Vehicle agent --------
    let vehicle_host = LexHost::from_source(VEHICLE_LEX).expect("vehicle lex compiles");
    let vehicle_agent = AgentConfig::new("vehicle")
        .peers(["depot", "tms"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (vehicle_mailbox, vehicle_sender) = Mailbox::new();
    router.register("vehicle", vehicle_sender.clone());

    let mut vehicle_runner = Runner::builder()
        .agent(vehicle_agent)
        .state(json!({"ready": true}))
        .mailbox(vehicle_mailbox)
        .lex_host(vehicle_host)
        .handle_lex("Dispatch", "on_dispatch")
        .handle_lex("GrantSession", "on_grant")
        .executor(Box::new(router.executor("vehicle")))
        .build()
        .unwrap();

    // -------- Depot agent --------
    let depot_host = LexHost::from_source(DEPOT_LEX).expect("depot lex compiles");
    let depot_agent = AgentConfig::new("depot")
        .peers(["vehicle"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (depot_mailbox, depot_sender) = Mailbox::new();
    router.register("depot", depot_sender);

    let mut depot_runner = Runner::builder()
        .agent(depot_agent)
        .state(json!({"ok": true}))
        .mailbox(depot_mailbox)
        .lex_host(depot_host)
        .handle_lex("RequestSession", "on_request_session")
        .executor(Box::new(router.executor("depot")))
        .build()
        .unwrap();

    // -------- Step 1: kick off with a Dispatch from a "tms" tester --------
    vehicle_sender
        .send(A2aMessage {
            from: "tms".into(),
            topic: "Dispatch".into(),
            payload: json!({"delivery_id": "d-1"}),
        })
        .unwrap();

    // Vehicle drains → handler proposes SendA2a("depot", "RequestSession") →
    // InProcessExecutor delivers into depot's mailbox.
    let r1 = vehicle_runner.drain().expect("vehicle drain ok");
    assert_eq!(r1.messages, 1);
    assert_eq!(
        r1.total_allowed, 1,
        "vehicle's SendA2a to depot should execute"
    );

    // Depot drains → handler proposes SendA2a("vehicle", "GrantSession") →
    // executor delivers back into vehicle's mailbox.
    let r2 = depot_runner.drain().expect("depot drain ok");
    assert_eq!(r2.messages, 1, "depot should have received RequestSession");
    assert_eq!(
        r2.total_allowed, 1,
        "depot's SendA2a to vehicle should execute"
    );

    // Vehicle drains again → handler is on_grant, returns [] → no further send.
    let r3 = vehicle_runner.drain().expect("vehicle second drain ok");
    assert_eq!(r3.messages, 1, "vehicle should have received GrantSession");
    assert_eq!(r3.total_allowed, 0, "on_grant returns empty action list");
}

#[test]
fn router_unknown_peer_records_skip() {
    let router = InProcessRouter::new();

    let agent = AgentConfig::new("loner")
        .peers(["ghost"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (mailbox, sender) = Mailbox::new();
    router.register("loner", sender.clone());
    // Note: "ghost" is NOT registered.

    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .handle("PingGhost", |_state, _msg| {
            vec![soft_agent::Action::SendA2a {
                peer: "ghost".into(),
                topic: "Pong".into(),
                payload: json!({}),
            }]
        })
        .executor(Box::new(router.executor("loner")))
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "tester".into(),
            topic: "PingGhost".into(),
            payload: json!(null),
        })
        .unwrap();

    // The runner records action.executed with an Err outcome since the
    // executor returns NotPermitted for unknown peers. That counts as
    // "allowed" by the gate but failed at execution; allowed is the
    // gate-level count, not the executor outcome.
    let report = runner.drain().expect("drain ok");
    assert_eq!(report.messages, 1);
    assert_eq!(report.total_allowed, 1, "gate allowed; executor failed");
}
