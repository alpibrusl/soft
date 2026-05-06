//! A2A protocol surface for soft agents.
//!
//! Uses `lex-api` as a library to register A2A routes alongside lex-native
//! routes. Pins one A2A version; the wire-format adapter is isolated in a
//! single module so future migrations stay contained.
//!
//! Scope: see `docs/crates/soft-a2a.md` in the repo root.
//!
//! # Status
//!
//! Skeleton. Depends on `soft-agent` (sibling crate) and on lex-lang
//! [#184](https://github.com/alpibrusl/lex-lang/issues/184) (`[a2a]` effect tag)
//! and [#187](https://github.com/alpibrusl/lex-lang/issues/187) (trace integration).

#![forbid(unsafe_code)]
