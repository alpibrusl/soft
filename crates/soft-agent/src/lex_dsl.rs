//! Lex-side DSL for declaring an agent config.
//!
//! soft-agent ships a small Lex preamble (`DSL_PREAMBLE`) defining
//! `AgentConfig`, helper constructors (`agent_new`, `agent_peers`,
//! `agent_handles`, ...), and the canonical `ActionRecord` shape. Users
//! write their agent as a regular Lex file with a top-level `config()`
//! function:
//!
//! ```text
//! fn config() -> AgentConfig {
//!   agent_new("vehicle")
//!   |> agent_peers(["depot", "tms"])
//!   |> agent_effects(["llm_local", "a2a"])
//!   |> agent_handles([
//!        { topic: "Dispatch", fn_name: "on_dispatch" },
//!      ])
//! }
//!
//! fn on_dispatch(state, msg) -> List[ActionRecord] { ... }
//! ```
//!
//! [`Runner::from_lex_source`](crate::Runner::from_lex_source) prepends
//! the preamble, compiles the combined source via [`LexHost`], invokes
//! `config()`, parses the returned record into a real
//! [`AgentConfig`] + handler registrations, and returns a partially-built
//! [`RunnerBuilder`] the caller finishes with `.mailbox(...)`,
//! `.state(...)`, etc. The preamble itself is pure Lex — no upstream
//! lex-runtime change required.

use lex_bytecode::Value as LexValue;

use crate::{AgentConfig, Effect, Error};

/// Lex preamble shipped with soft-agent. Prepended to user agent source
/// before compilation. Pure Lex — defines record types and pure-function
/// builders. Each builder copies the whole record because lex-syntax 0.2
/// has no record-update sugar yet (probably worth filing as a small
/// papercut alongside the underscore one).
///
/// Sourced from the sibling `dsl_preamble.lex` so the file is browsable
/// as Lex (LSP/syntax highlighting works) and the line count attributes
/// to Lex rather than a Rust raw string.
pub const DSL_PREAMBLE: &str = include_str!("dsl_preamble.lex");

/// What [`Runner::from_lex_source`](crate::Runner::from_lex_source)
/// extracts from a user's `config()` Lex value: the structured config
/// plus a list of `(topic, fn_name)` pairs to register as Lex handlers.
pub struct LexAgentSetup {
    pub config: AgentConfig,
    pub handlers: Vec<(String, String)>,
}

/// Parse a `LexValue::Record` matching the preamble's `AgentConfig` shape
/// into a Rust [`AgentConfig`] + handler registrations.
pub fn parse_lex_config(value: &LexValue) -> Result<LexAgentSetup, Error> {
    let json = value.to_json();
    let get_str = |key: &str| json.get(key).and_then(|v| v.as_str()).map(String::from);
    let get_str_list = |key: &str| -> Vec<String> {
        json.get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };

    let name = get_str("name")
        .ok_or_else(|| Error::InvalidConfig("config record missing `name`".into()))?;

    let peers = get_str_list("peers");
    let mcp_servers = get_str_list("mcp_servers");

    let effect_names = get_str_list("effects");
    let mut effects = Vec::new();
    for n in &effect_names {
        match effect_from_name(n) {
            Some(e) => effects.push(e),
            None => {
                return Err(Error::InvalidConfig(format!(
                    "unknown effect tag in config: `{n}` (expected one of \
                    llm_local, llm_cloud, a2a, mcp, time)"
                )))
            }
        }
    }

    let handlers: Vec<(String, String)> = json
        .get("handlers")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|h| {
                    let topic = h.get("topic")?.as_str()?.to_string();
                    let fn_name = h.get("fn_name")?.as_str()?.to_string();
                    Some((topic, fn_name))
                })
                .collect()
        })
        .unwrap_or_default();

    let spec_paths = get_str_list("spec_paths");
    let system_prompt = get_str("system_prompt").unwrap_or_default();

    let mut builder = AgentConfig::new(name);
    if !peers.is_empty() {
        builder = builder.peers(peers);
    }
    if !mcp_servers.is_empty() {
        builder = builder.mcp_servers(mcp_servers);
    }
    if !effects.is_empty() {
        builder = builder.effects(effects);
    }
    if !spec_paths.is_empty() {
        builder = builder.specs(spec_paths);
    }
    if !system_prompt.is_empty() {
        builder = builder.system_prompt(system_prompt);
    }

    Ok(LexAgentSetup {
        config: builder,
        handlers,
    })
}

fn effect_from_name(s: &str) -> Option<Effect> {
    match s {
        "llm_local" => Some(Effect::LlmLocal),
        "llm_cloud" => Some(Effect::LlmCloud),
        "a2a" => Some(Effect::A2a),
        "mcp" => Some(Effect::Mcp),
        "time" => Some(Effect::Time),
        _ => None,
    }
}
