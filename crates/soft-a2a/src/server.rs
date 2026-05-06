//! HTTP server bridging A2A messages into a [`soft_agent::MailboxSender`].
//!
//! Routes:
//!
//! | Method | Path               | Body                      | Effect                                              |
//! |--------|--------------------|---------------------------|-----------------------------------------------------|
//! | GET    | `/a2a/agent-card`  | -                         | Returns the agent's [`AgentCard`] as JSON.          |
//! | POST   | `/a2a/messages`    | A2A [`Message`] (JSON)    | Forwards as a soft-agent `A2aMessage` to the inbox. |
//! | -      | -                  | -                         | All other paths → 404.                              |
//!
//! `metadata.from` and `metadata.topic` on the incoming Message are
//! mandatory — they tell soft-agent who sent the message and which topic
//! handler to dispatch into.

use std::thread::{self, JoinHandle};

use serde_json::json;
use soft_agent::MailboxSender;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::wire::{parts_to_payload, AgentCard, Message};
use crate::Error;

pub struct A2aServer {
    server: Server,
    agent_card: AgentCard,
    sender: MailboxSender,
}

impl A2aServer {
    /// Bind a server to `addr` (e.g. `"127.0.0.1:0"` for an ephemeral port).
    pub fn bind(
        addr: &str,
        agent_card: AgentCard,
        sender: MailboxSender,
    ) -> Result<Self, Error> {
        let server = Server::http(addr).map_err(|e| Error::Bind(e.to_string()))?;
        Ok(A2aServer { server, agent_card, sender })
    }

    /// The local socket address the server is listening on.
    pub fn local_addr(&self) -> Option<String> {
        self.server.server_addr().to_ip().map(|a| a.to_string())
    }

    /// Run the server loop in this thread. Blocks until the underlying
    /// listener is dropped.
    pub fn run(self) {
        let A2aServer { server, agent_card, sender } = self;
        for req in server.incoming_requests() {
            handle(req, &agent_card, &sender);
        }
    }

    /// Spawn the run loop in a thread.
    pub fn spawn(self) -> JoinHandle<()> {
        thread::spawn(move || self.run())
    }
}

fn handle(req: Request, card: &AgentCard, sender: &MailboxSender) {
    match (req.method(), req.url()) {
        (Method::Get, "/a2a/agent-card") => respond_json(req, card, 200),
        (Method::Post, "/a2a/messages") => handle_message(req, sender),
        _ => respond_text(req, "not found", 404),
    }
}

fn handle_message(mut req: Request, sender: &MailboxSender) {
    let mut body = String::new();
    if req.as_reader().read_to_string(&mut body).is_err() {
        respond_text(req, "read error", 400);
        return;
    }
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            respond_text(req, &format!("bad json: {e}"), 400);
            return;
        }
    };
    let meta = match msg.metadata.clone() {
        Some(m) => m,
        None => {
            respond_text(req, "metadata.from + metadata.topic required", 400);
            return;
        }
    };
    let payload = parts_to_payload(&msg.parts);
    let a2a = soft_agent::A2aMessage {
        from: meta.from,
        topic: meta.topic,
        payload,
    };
    if sender.send(a2a).is_err() {
        respond_text(req, "mailbox closed", 503);
        return;
    }
    respond_json(req, &json!({"accepted": msg.message_id}), 202);
}

fn respond_json<T: serde::Serialize>(req: Request, body: &T, status: u16) {
    let json_str = match serde_json::to_string(body) {
        Ok(s) => s,
        Err(e) => return respond_text(req, &format!("encode error: {e}"), 500),
    };
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static content-type header is well-formed");
    let resp = Response::from_string(json_str)
        .with_header(header)
        .with_status_code(status);
    let _ = req.respond(resp);
}

fn respond_text(req: Request, body: &str, status: u16) {
    let resp = Response::from_string(body).with_status_code(status);
    let _ = req.respond(resp);
}
