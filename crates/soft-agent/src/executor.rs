//! Action executor abstraction.
//!
//! v1 ships a [`MockExecutor`] that records what would have been executed
//! and returns canned responses. Real execution via lex-lang's `std.agent`
//! primitives lives in a later slice (and requires the runner to drive a
//! Lex VM, which it doesn't today).

use serde_json::{json, Value};

use crate::Action;

#[derive(Debug)]
pub enum ExecError {
    NotPermitted(String),
    Failed(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::NotPermitted(why) => write!(f, "not permitted: {why}"),
            ExecError::Failed(why) => write!(f, "execution failed: {why}"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Pluggable executor for proposed actions. Real impls call out to MCP
/// servers / LLMs / A2A wire; [`MockExecutor`] just records the calls.
pub trait ActionExecutor: Send {
    fn execute(&mut self, action: &Action) -> Result<Value, ExecError>;
}

/// In-memory executor that records every action and returns
/// `{"mock": true}`. Used by tests and the depot demo.
#[derive(Default)]
pub struct MockExecutor {
    pub log: Vec<Action>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self { log: Vec::new() }
    }
}

impl ActionExecutor for MockExecutor {
    fn execute(&mut self, action: &Action) -> Result<Value, ExecError> {
        self.log.push(action.clone());
        Ok(json!({"mock": true}))
    }
}
