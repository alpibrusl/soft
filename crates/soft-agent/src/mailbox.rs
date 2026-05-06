//! In-process A2A mailbox.
//!
//! v0 uses `std::sync::mpsc` for in-process delivery. Cross-process A2A
//! (HTTP transport, agent cards, etc.) lives in [`soft-a2a`](../../../crates/soft-a2a)
//! and is deferred to Phase 2.

use std::sync::mpsc;

use serde_json::Value;

use crate::Error;

/// An A2A message sitting in an agent's inbox.
#[derive(Clone, Debug)]
pub struct A2aMessage {
    pub from: String,
    pub topic: String,
    pub payload: Value,
}

/// Receiving end of an agent's mailbox.
pub struct Mailbox {
    rx: mpsc::Receiver<A2aMessage>,
}

/// Sending end (held by peers / the runner).
#[derive(Clone)]
pub struct MailboxSender {
    tx: mpsc::Sender<A2aMessage>,
}

impl Mailbox {
    pub fn new() -> (Mailbox, MailboxSender) {
        let (tx, rx) = mpsc::channel();
        (Mailbox { rx }, MailboxSender { tx })
    }

    pub fn recv(&self) -> Result<A2aMessage, Error> {
        self.rx.recv().map_err(|_| Error::MailboxClosed)
    }

    pub fn try_recv(&self) -> Option<A2aMessage> {
        self.rx.try_recv().ok()
    }
}

impl MailboxSender {
    pub fn send(&self, msg: A2aMessage) -> Result<(), Error> {
        self.tx.send(msg).map_err(|_| Error::MailboxClosed)
    }
}
