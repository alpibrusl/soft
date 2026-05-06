//! A2A wire schema. Pinned to [`A2A_VERSION`].
//!
//! Intentionally a minimal subset: the Message envelope, Part union,
//! and AgentCard are enough to round-trip an intent + structured payload
//! between two soft agents over HTTP. Tasks, streaming, and signatures
//! are deferred.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A2A protocol version this crate is pinned to. Bump when adopting an
/// upstream-breaking change to the wire format.
pub const A2A_VERSION: &str = "0.2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub message_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// soft-a2a routing convention: tells the receiving agent who sent the
    /// message and which topic handler to dispatch it to. Required when
    /// posting to `/a2a/messages` against a soft-a2a server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    Text { text: String },
    Data { data: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageMetadata {
    pub from: String,
    pub topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    pub url: String,
    pub a2a_version: String,
    pub capabilities: Capabilities,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Capabilities {
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl AgentCard {
    /// Convenience constructor with sensible defaults for a soft agent
    /// that exposes one or more topic handlers as A2A skills.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        AgentCard {
            name: name.into(),
            description: description.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            url: url.into(),
            a2a_version: A2A_VERSION.to_string(),
            capabilities: Capabilities::default(),
            skills: Vec::new(),
        }
    }

    pub fn with_skills(mut self, skills: Vec<Skill>) -> Self {
        self.skills = skills;
        self
    }
}

/// Collapse a Message's parts into a single JSON payload for soft-agent.
///
/// Convention: the first `Part::Data` wins; if there's none, all
/// `Part::Text` are joined with `\n` under a `text` field; otherwise
/// `null`.
pub fn parts_to_payload(parts: &[Part]) -> Value {
    if let Some(Part::Data { data }) = parts.iter().find(|p| matches!(p, Part::Data { .. })) {
        return data.clone();
    }
    let texts: Vec<&str> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        Value::Null
    } else {
        serde_json::json!({ "text": texts.join("\n") })
    }
}
