//! Autonomous agent runtime for the soft project.
//!
//! Sits on lex-lang v0.2's primitives (effects, types, trace, store,
//! spec-checker) and provides the actor-shaped abstractions LLM-driven
//! agents need.
//!
//! See `docs/crates/soft-agent.md` in the repo root for the full scope.
//!
//! # Status
//!
//! v1 — core types + builder + mailbox + runner + spec gate + trace.
//! Action execution is still pluggable via [`ActionExecutor`]; the default
//! [`MockExecutor`] just records calls. Real execution against the
//! `std.agent` builtins (driving a Lex VM under the trace recorder)
//! lands in the next slice.

#![forbid(unsafe_code)]

pub mod action;
pub mod agent;
pub mod bindings;
pub mod effect;
pub mod error;
pub mod executor;
pub mod gate;
pub mod lex_dsl;
pub mod lex_host;
pub mod mailbox;
pub mod metrics;
pub mod replay;
pub mod router;
pub mod runner;
pub mod trace;

pub use action::Action;
pub use agent::{Agent, AgentConfig, AgentId};
pub use bindings::default_float_bindings;
pub use effect::{Effect, EffectSet};
pub use error::Error;
pub use executor::{ActionExecutor, ExecError, MockExecutor};
pub use gate::{Gate, Verdict};
pub use lex_dsl::{parse_lex_config, LexAgentSetup, DSL_PREAMBLE};
pub use lex_host::{HandlerFactory, LexCall, LexHost};
pub use mailbox::{A2aMessage, Mailbox, MailboxSender};
pub use metrics::Metrics;
pub use router::{InProcessExecutor, InProcessRouter};
pub use runner::{BindingsFn, DrainReport, Handler, Runner, RunnerBuilder, StepReport};
pub use trace::TraceWriter;
