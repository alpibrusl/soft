//! Compile-time effect set declaration.
//!
//! Mirrors lex-lang v0.2's four agent effects (`[llm_local]`, `[llm_cloud]`,
//! `[a2a]`, `[mcp]`) plus a `Time` effect for periodic ticks. soft-agent
//! uses this set for the agent's declared capabilities; per-server MCP
//! scoping is a separate runtime concern (see `Agent::allows_mcp_server`).

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    LlmLocal,
    LlmCloud,
    A2a,
    Mcp,
    Time,
}

impl Effect {
    /// Canonical name as written in a Lex effect signature.
    pub fn name(self) -> &'static str {
        match self {
            Effect::LlmLocal => "llm_local",
            Effect::LlmCloud => "llm_cloud",
            Effect::A2a => "a2a",
            Effect::Mcp => "mcp",
            Effect::Time => "time",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectSet(BTreeSet<Effect>);

impl EffectSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, effect: Effect) -> bool {
        self.0.contains(&effect)
    }

    pub fn iter(&self) -> impl Iterator<Item = Effect> + '_ {
        self.0.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<Effect> for EffectSet {
    fn from_iter<I: IntoIterator<Item = Effect>>(iter: I) -> Self {
        EffectSet(iter.into_iter().collect())
    }
}
