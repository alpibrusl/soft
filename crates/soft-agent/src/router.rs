//! In-process A2A router + executor.
//!
//! For multi-agent runs that don't need cross-process A2A, [`InProcessRouter`]
//! lets you wire each agent's outbound `SendA2a` directly into the next
//! agent's mailbox. The cross-process A2A path lives in [`soft_a2a`] and uses
//! `tiny_http` + `ureq`.
//!
//! Usage sketch:
//!
//! ```ignore
//! let router = InProcessRouter::new();
//! let (vehicle_in, vehicle_send) = Mailbox::new();
//! let (depot_in,   depot_send)   = Mailbox::new();
//! router.register("vehicle", vehicle_send.clone());
//! router.register("depot",   depot_send.clone());
//!
//! let vehicle_runner = Runner::builder()
//!     .agent(vehicle_agent).mailbox(vehicle_in)
//!     .executor(Box::new(router.executor("vehicle")))
//!     .build()?;
//! let depot_runner = Runner::builder()
//!     .agent(depot_agent).mailbox(depot_in)
//!     .executor(Box::new(router.executor("depot")))
//!     .build()?;
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::executor::{ActionExecutor, ExecError};
use crate::mailbox::{A2aMessage, MailboxSender};
use crate::Action;

/// Shared registry of `peer name → MailboxSender`.
#[derive(Clone, Default)]
pub struct InProcessRouter {
    senders: Arc<Mutex<HashMap<String, MailboxSender>>>,
}

impl InProcessRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a peer's mailbox sender. Last write wins per peer name.
    pub fn register(&self, peer: impl Into<String>, sender: MailboxSender) {
        self.senders.lock().unwrap().insert(peer.into(), sender);
    }

    /// Build an executor for an agent named `source`. Outbound `SendA2a`
    /// actions look up the target peer in the router and deliver to that
    /// mailbox. `CallMcp`/`LocalLlm`/`CloudLlm` are handled as no-ops in
    /// v0 (deterministic Lex handler demos don't need them).
    pub fn executor(&self, source: impl Into<String>) -> InProcessExecutor {
        InProcessExecutor {
            senders: Arc::clone(&self.senders),
            source: source.into(),
        }
    }
}

/// `ActionExecutor` that delivers `SendA2a` to peer mailboxes via the
/// shared router. Other action kinds are stubbed.
pub struct InProcessExecutor {
    senders: Arc<Mutex<HashMap<String, MailboxSender>>>,
    source: String,
}

impl ActionExecutor for InProcessExecutor {
    fn execute(&mut self, action: &Action) -> Result<Value, ExecError> {
        match action {
            Action::SendA2a { peer, topic, payload } => {
                let senders = self.senders.lock().unwrap();
                let sender = senders.get(peer).ok_or_else(|| {
                    ExecError::NotPermitted(format!("router has no peer `{peer}`"))
                })?;
                sender
                    .send(A2aMessage {
                        from: self.source.clone(),
                        topic: topic.clone(),
                        payload: payload.clone(),
                    })
                    .map_err(|_| ExecError::Failed(format!("delivery to `{peer}` failed")))?;
                Ok(json!({"delivered_to": peer, "topic": topic}))
            }
            Action::CallMcp { server, tool, .. } => {
                // v0 stub — wire to lex_runtime's agent.call_mcp from a Lex
                // handler instead of through this executor.
                Ok(json!({"stubbed": "mcp", "server": server, "tool": tool}))
            }
            Action::LocalLlm { .. } => Ok(json!({"stubbed": "llm_local"})),
            Action::CloudLlm { .. } => Ok(json!({"stubbed": "llm_cloud"})),
        }
    }
}
