//! A2A protocol surface for soft agents.
//!
//! Pins one A2A version (see [`A2A_VERSION`]). Server side wraps
//! `tiny_http` and bridges incoming messages into a
//! [`soft_agent::MailboxSender`]; client side wraps `ureq` for
//! peer-to-peer sends. Cross-process A2A across processes works today;
//! Phase 1 of soft only requires in-process and uses the
//! `soft_agent::Mailbox` directly without going through this crate.
//!
//! See `docs/crates/soft-a2a.md` in the repo root for scope.
//!
//! # Status
//!
//! v0 — minimal Message + Part + AgentCard schema, POST `/a2a/messages`
//! and GET `/a2a/agent-card` endpoints, an HTTP client. Task lifecycle,
//! SSE streaming, signatures, and authentication are deferred.

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod server;
pub mod wire;

pub use client::A2aClient;
pub use error::Error;
pub use server::A2aServer;
pub use wire::{
    AgentCard, Capabilities, Message, MessageMetadata, Part, Role, Skill, A2A_VERSION,
};
