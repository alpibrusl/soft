//! End-to-end: spin up an A2aServer on an ephemeral port, send a Message
//! via A2aClient, verify it lands in the soft-agent Mailbox.

use serde_json::json;
use soft_a2a::{
    A2aClient, A2aServer, AgentCard, Message, MessageMetadata, Part, Role, A2A_VERSION,
};
use soft_agent::Mailbox;
use std::time::Duration;

fn fresh_card() -> AgentCard {
    AgentCard::new(
        "test-agent",
        "soft-a2a round-trip test",
        "http://127.0.0.1:0/",
    )
}

#[test]
fn server_forwards_message_to_mailbox() {
    let (mailbox, sender) = Mailbox::new();
    let server = A2aServer::bind("127.0.0.1:0", fresh_card(), sender).expect("server binds");
    let addr = server.local_addr().expect("has addr");
    let _handle = server.spawn();

    // Give the listener a moment to be ready.
    std::thread::sleep(Duration::from_millis(50));

    let client = A2aClient::new();
    let msg = Message {
        message_id: "msg-1".into(),
        role: Role::User,
        parts: vec![Part::Data {
            data: json!({"vehicle_id": "v-1", "power_kw": 30.0}),
        }],
        task_id: None,
        metadata: Some(MessageMetadata {
            from: "tester".into(),
            topic: "RequestSession".into(),
        }),
    };
    client
        .send(&format!("http://{addr}"), &msg)
        .expect("client send ok");

    let received = mailbox.recv().expect("message lands in mailbox");
    assert_eq!(received.from, "tester");
    assert_eq!(received.topic, "RequestSession");
    assert_eq!(received.payload["vehicle_id"], "v-1");
    assert_eq!(received.payload["power_kw"], 30.0);
}

#[test]
fn missing_metadata_returns_400() {
    let (_mailbox, sender) = Mailbox::new();
    let server = A2aServer::bind("127.0.0.1:0", fresh_card(), sender).unwrap();
    let addr = server.local_addr().unwrap();
    let _handle = server.spawn();
    std::thread::sleep(Duration::from_millis(50));

    let bad = Message {
        message_id: "msg-x".into(),
        role: Role::User,
        parts: vec![Part::Text { text: "hi".into() }],
        task_id: None,
        metadata: None,
    };
    let result = A2aClient::new().send(&format!("http://{addr}"), &bad);
    let err = result.expect_err("should be 400");
    let msg = err.to_string();
    assert!(msg.contains("400"), "expected 400, got {msg:?}");
}

#[test]
fn agent_card_endpoint() {
    let (_mailbox, sender) = Mailbox::new();
    let server = A2aServer::bind("127.0.0.1:0", fresh_card(), sender).unwrap();
    let addr = server.local_addr().unwrap();
    let _handle = server.spawn();
    std::thread::sleep(Duration::from_millis(50));

    let card = A2aClient::new()
        .fetch_agent_card(&format!("http://{addr}"))
        .expect("fetch ok");
    assert_eq!(card.name, "test-agent");
    assert_eq!(card.a2a_version, A2A_VERSION);
}

#[test]
fn text_parts_are_concatenated_into_payload() {
    let (mailbox, sender) = Mailbox::new();
    let server = A2aServer::bind("127.0.0.1:0", fresh_card(), sender).unwrap();
    let addr = server.local_addr().unwrap();
    let _handle = server.spawn();
    std::thread::sleep(Duration::from_millis(50));

    let msg = Message {
        message_id: "msg-text".into(),
        role: Role::User,
        parts: vec![
            Part::Text {
                text: "hello".into(),
            },
            Part::Text {
                text: "world".into(),
            },
        ],
        task_id: None,
        metadata: Some(MessageMetadata {
            from: "tester".into(),
            topic: "Greet".into(),
        }),
    };
    A2aClient::new()
        .send(&format!("http://{addr}"), &msg)
        .expect("send ok");

    let received = mailbox.recv().unwrap();
    assert_eq!(received.payload["text"], "hello\nworld");
}
