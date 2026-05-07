//! Anthropic Messages API as an `EffectHandler` wrapper.
//!
//! Wraps [`lex_runtime::DefaultHandler`] and intercepts
//! `agent.cloud_complete` (which `DefaultHandler` would otherwise route
//! to OpenAI-shape chat-completions). All other effects fall through
//! unchanged — `agent.local_complete`, `agent.call_mcp`, `agent.send_a2a`,
//! plus `[net]`, `[io]`, etc.
//!
//! Per PR #203's design: this is the "EffectHandler escape hatch" the
//! lex-lang team called out for non-OpenAI backends. Anthropic's
//! `/v1/messages` endpoint differs from OpenAI's enough (different
//! request shape, `x-api-key` header, required `max_tokens`, content
//! returned as content-blocks) that translation at this layer is
//! cleaner than overloading `LEX_LLM_CLOUD_BASE_URL`.
//!
//! ## Configuration (env vars)
//!
//! - `ANTHROPIC_API_KEY` — required. Get one from the Anthropic
//!   Console (this is *not* the same as a Claude.ai or Claude Code
//!   subscription).
//! - `ANTHROPIC_BASE_URL` — defaults to `https://api.anthropic.com`.
//! - `ANTHROPIC_MODEL` — defaults to `claude-sonnet-4-5-20250929`.
//! - `ANTHROPIC_MAX_TOKENS` — defaults to `1024`.
//! - `ANTHROPIC_VERSION` — defaults to `2023-06-01` (Anthropic's stable
//!   header value).

use std::time::Duration;

use lex_bytecode::{vm::EffectHandler, Value};
use lex_runtime::{DefaultHandler, Policy};

const ANTHROPIC_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub struct AnthropicCloudHandler {
    inner: DefaultHandler,
    agent: ureq::Agent,
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: u32,
    api_version: String,
}

impl AnthropicCloudHandler {
    /// Build from environment. Returns `None` if `ANTHROPIC_API_KEY`
    /// isn't set — the caller should warn the user and fall back to the
    /// default handler.
    pub fn from_env(policy: Policy) -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        let base_url = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".into());
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-5-20250929".into());
        let max_tokens = std::env::var("ANTHROPIC_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024);
        let api_version =
            std::env::var("ANTHROPIC_VERSION").unwrap_or_else(|_| "2023-06-01".into());

        let agent = ureq::AgentBuilder::new()
            .timeout(ANTHROPIC_HTTP_TIMEOUT)
            .build();

        Some(Self {
            inner: DefaultHandler::new(policy),
            agent,
            api_key,
            base_url,
            model,
            max_tokens,
            api_version,
        })
    }
}

impl EffectHandler for AnthropicCloudHandler {
    fn dispatch(&mut self, kind: &str, op: &str, args: Vec<Value>) -> Result<Value, String> {
        if kind == "agent" && op == "cloud_complete" {
            return Ok(self.call_anthropic(args));
        }
        self.inner.dispatch(kind, op, args)
    }
}

impl AnthropicCloudHandler {
    fn call_anthropic(&self, args: Vec<Value>) -> Value {
        let prompt = match args.first() {
            Some(Value::Str(s)) => s.clone(),
            _ => {
                return err(Value::Str(
                    "agent.cloud_complete(prompt): prompt must be Str".into(),
                ))
            }
        };

        let body = serde_json::json!({
            "model": &self.model,
            "max_tokens": self.max_tokens,
            "messages": [{"role": "user", "content": prompt}],
        });

        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let resp = self
            .agent
            .post(&url)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", &self.api_version)
            .set("content-type", "application/json")
            .send_json(body);

        match resp {
            Ok(r) => match r.into_json::<serde_json::Value>() {
                Ok(j) => extract_text(&j),
                Err(e) => err(Value::Str(format!("anthropic decode: {e}"))),
            },
            Err(ureq::Error::Status(code, r)) => err(Value::Str(format!(
                "anthropic {} {}",
                code,
                r.into_string().unwrap_or_default()
            ))),
            Err(e) => err(Value::Str(format!("anthropic transport: {e}"))),
        }
    }
}

/// Anthropic returns `{ "content": [ { "type": "text", "text": "..." }, ... ] }`.
/// We concatenate all `text` blocks (usually just one) and return as a Lex
/// `Result[Str, Str]::Ok`. Non-text blocks are skipped.
fn extract_text(j: &serde_json::Value) -> Value {
    let blocks = match j.get("content").and_then(|c| c.as_array()) {
        Some(b) => b,
        None => {
            return err(Value::Str(format!(
                "anthropic: response missing `content` array: {j}"
            )));
        }
    };
    let mut out = String::new();
    for b in blocks {
        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
    }
    if out.is_empty() {
        err(Value::Str(format!("anthropic: no text content in: {j}")))
    } else {
        ok(Value::Str(out))
    }
}

fn ok(v: Value) -> Value {
    Value::Variant {
        name: "Ok".into(),
        args: vec![v],
    }
}

fn err(v: Value) -> Value {
    Value::Variant {
        name: "Err".into(),
        args: vec![v],
    }
}
