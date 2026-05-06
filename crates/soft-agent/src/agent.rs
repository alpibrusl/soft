//! Agent type and configuration builder.
//!
//! Mirrors the `agent.*` DSL shown in `examples/phase1/*.lex`. Phase 1
//! agents are built up via the builder methods on [`AgentConfig`] and
//! finalised into an [`Agent`] via [`AgentConfig::build`].

use std::collections::BTreeSet;

use crate::{Effect, EffectSet, Error};

/// Content-addressed agent identity.
///
/// v0 uses the agent's declared name as identity. v1 will hash the AST of
/// the agent's Lex source via `lex_ast::SigId` so identity survives
/// renames and tracks behaviour.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(name: impl Into<String>) -> Self {
        AgentId(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Builder for agent configuration. Mirrors the .lex builder DSL.
#[derive(Clone, Debug)]
pub struct AgentConfig {
    name: String,
    peers: BTreeSet<String>,
    mcp_servers: BTreeSet<String>,
    effects: EffectSet,
    system_prompt: Option<String>,
    spec_paths: Vec<String>,
}

impl AgentConfig {
    pub fn new(name: impl Into<String>) -> Self {
        AgentConfig {
            name: name.into(),
            peers: BTreeSet::new(),
            mcp_servers: BTreeSet::new(),
            effects: EffectSet::new(),
            system_prompt: None,
            spec_paths: Vec::new(),
        }
    }

    pub fn peers(mut self, peers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.peers = peers.into_iter().map(Into::into).collect();
        self
    }

    pub fn mcp_servers(
        mut self,
        servers: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.mcp_servers = servers.into_iter().map(Into::into).collect();
        self
    }

    pub fn effects(mut self, effects: impl IntoIterator<Item = Effect>) -> Self {
        self.effects = effects.into_iter().collect();
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn specs(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.spec_paths = paths.into_iter().map(Into::into).collect();
        self
    }

    pub fn build(self) -> Result<Agent, Error> {
        if self.effects.is_empty() {
            return Err(Error::InvalidConfig(
                "agent effects set must not be empty".into(),
            ));
        }
        if self.effects.contains(Effect::LlmLocal) && self.effects.contains(Effect::LlmCloud) {
            return Err(Error::InvalidConfig(
                "agent cannot declare both [llm_local] and [llm_cloud]".into(),
            ));
        }
        if self.effects.contains(Effect::Mcp) && self.mcp_servers.is_empty() {
            return Err(Error::InvalidConfig(
                "agent declares [mcp] but no servers in its allowlist".into(),
            ));
        }
        Ok(Agent {
            id: AgentId::new(&self.name),
            config: self,
        })
    }
}

/// A built agent, ready to be handed to a Runner (next slice).
#[derive(Clone, Debug)]
pub struct Agent {
    id: AgentId,
    config: AgentConfig,
}

impl Agent {
    pub fn id(&self) -> &AgentId {
        &self.id
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn effects(&self) -> &EffectSet {
        &self.config.effects
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt.as_deref()
    }

    pub fn spec_paths(&self) -> &[String] {
        &self.config.spec_paths
    }

    /// Runtime-side MCP allowlist check. Per-server scoping is a soft-agent
    /// concern because lex-lang v0.2 ships the flat `[mcp]` effect; future
    /// parameterized effects (`[mcp(<server>)]`) could promote this to a
    /// compile-time fact.
    pub fn allows_mcp_server(&self, server: &str) -> bool {
        self.config.mcp_servers.contains(server)
    }

    pub fn knows_peer(&self, peer: &str) -> bool {
        self.config.peers.contains(peer)
    }
}
