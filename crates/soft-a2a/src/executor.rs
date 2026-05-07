//! [`A2aRoutedExecutor`] — `ActionExecutor` that delivers `SendA2a`
//! actions to remote peers via HTTP using [`A2aClient`].
//!
//! Sibling of `soft_agent::InProcessRouter` for cross-process
//! deployments. Build with a `peer_name → base_url` map at construction;
//! every outbound `SendA2a` looks up the peer and POSTs an A2A
//! [`Message`] to `<base_url>/a2a/messages`.
//!
//! Other action kinds (`CallMcp`, `LocalLlm`, `CloudLlm`) are stubbed —
//! they return canned JSON. Real MCP integration belongs in lex-runtime
//! via `agent.call_mcp` (already shipped); LLM integration is open
//! upstream as [#196](https://github.com/alpibrusl/lex-lang/issues/196).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use soft_agent::{Action, ActionExecutor, ExecError};

use crate::wire::{Message, MessageMetadata, Part, Role};
use crate::A2aClient;

pub struct A2aRoutedExecutor {
    client: A2aClient,
    peers: HashMap<String, String>,
    source: String,
}

impl A2aRoutedExecutor {
    /// `source` is this agent's name — stamped onto every outbound
    /// message's [`MessageMetadata::from`]. `peers` maps each peer name
    /// (as referenced by the Lex handler in `SendA2a.peer`) to a base
    /// URL like `http://depot:8002` (no trailing slash).
    pub fn new(source: impl Into<String>, peers: HashMap<String, String>) -> Self {
        Self {
            client: A2aClient::new(),
            peers,
            source: source.into(),
        }
    }
}

impl ActionExecutor for A2aRoutedExecutor {
    fn execute(&mut self, action: &Action) -> Result<Value, ExecError> {
        match action {
            Action::SendA2a {
                peer,
                topic,
                payload,
            } => {
                let url = self.peers.get(peer).ok_or_else(|| {
                    ExecError::NotPermitted(format!("no URL configured for peer `{peer}`"))
                })?;
                let message = Message {
                    message_id: format!("{}-{}", self.source, monotonic_ns()),
                    role: Role::Agent,
                    parts: vec![Part::Data {
                        data: payload.clone(),
                    }],
                    task_id: None,
                    metadata: Some(MessageMetadata {
                        from: self.source.clone(),
                        topic: topic.clone(),
                    }),
                };
                self.client
                    .send(url, &message)
                    .map_err(|e| ExecError::Failed(format!("a2a send to `{peer}` ({url}): {e}")))?;
                Ok(json!({"delivered_to": peer, "topic": topic, "url": url}))
            }
            Action::CallMcp { server, tool, .. } => {
                Ok(json!({"stubbed": "mcp", "server": server, "tool": tool}))
            }
            Action::LocalLlm { .. } => Ok(json!({"stubbed": "llm_local"})),
            Action::CloudLlm { .. } => Ok(json!({"stubbed": "llm_cloud"})),
        }
    }
}

fn monotonic_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
