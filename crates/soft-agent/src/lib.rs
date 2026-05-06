//! Autonomous agent runtime for the soft project.
//!
//! Sits on lex-lang's primitives (effects, types, trace, vcs) and provides
//! actor-shaped abstractions for LLM-driven agents.
//!
//! Scope: see `docs/crates/soft-agent.md` in the repo root.
//!
//! # Status
//!
//! Skeleton. Awaiting lex-lang:
//!
//! - [#184](https://github.com/alpibrusl/lex-lang/issues/184) — effect tags
//! - [#185](https://github.com/alpibrusl/lex-lang/issues/185) — MCP wrapper
//! - [#186](https://github.com/alpibrusl/lex-lang/issues/186) — spec-checker runtime gate
//! - [#187](https://github.com/alpibrusl/lex-lang/issues/187) — lex-trace ↔ lex-vcs

#![forbid(unsafe_code)]
