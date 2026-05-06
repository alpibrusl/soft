//! Autonomous agent runtime for the soft project.
//!
//! Sits on lex-lang v0.2's primitives (effects, types, trace, store,
//! spec-checker) and provides the actor-shaped abstractions LLM-driven
//! agents need.
//!
//! See `docs/crates/soft-agent.md` in the repo root for the full scope.
//!
//! # v0 status
//!
//! This first slice provides the core types and the in-process mailbox.
//! It does not yet integrate `lex-trace`, `spec-checker`, or any of the
//! `std.agent` builtins from lex-lang v0.2 — those land in the next slice
//! together with the runtime loop.

#![forbid(unsafe_code)]

pub mod action;
pub mod agent;
pub mod effect;
pub mod error;
pub mod mailbox;

pub use action::Action;
pub use agent::{Agent, AgentConfig, AgentId};
pub use effect::{Effect, EffectSet};
pub use error::Error;
pub use mailbox::{A2aMessage, Mailbox, MailboxSender};
