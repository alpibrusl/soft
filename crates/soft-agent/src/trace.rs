//! Trace writer — builds a [`lex_trace::TraceTree`] as the runner
//! executes, and persists it via [`lex_store::Store::save_trace`].
//!
//! v1 records a flat sequence of effect events (received messages,
//! proposed actions, gate verdicts, executed actions). The
//! [`lex_trace::TraceNode`] tree shape supports nesting; we'll add
//! hierarchy when the runner drives Lex code under a `Vm + Tracer` in a
//! later slice.

use std::time::{SystemTime, UNIX_EPOCH};

use lex_store::Store;
use lex_trace::{RunId, TraceNode, TraceNodeKind, TraceTree};
use serde_json::Value;

use crate::Error;

/// Trace recorder for one agent run.
pub struct TraceWriter {
    run_id: RunId,
    root_target: String,
    root_input: Value,
    started_at: u64,
    nodes: Vec<TraceNode>,
}

impl TraceWriter {
    /// Start a new trace.
    ///
    /// `seed` is hashed (with current time, per [`lex_trace::RunId::new`])
    /// into the run ID. Use a stable identifier — e.g. the agent name —
    /// so traces cluster sensibly on disk and across replays.
    pub fn new(seed: &str, root_target: impl Into<String>, root_input: Value) -> Self {
        Self {
            run_id: RunId::new(seed),
            root_target: root_target.into(),
            root_input,
            started_at: now_ms(),
            nodes: Vec::new(),
        }
    }

    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Append an effect event.
    pub fn record_effect(
        &mut self,
        target: impl Into<String>,
        input: Value,
        outcome: Result<Value, String>,
    ) {
        let now = now_ms();
        let target = target.into();
        let (output, error) = match outcome {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        self.nodes.push(TraceNode {
            node_id: format!("{}.evt-{}", target, self.nodes.len()),
            kind: TraceNodeKind::Effect,
            target,
            input,
            output,
            error,
            started_at: now,
            ended_at: now,
            children: Vec::new(),
        });
    }

    /// Persist the trace under `<store_root>/traces/<run_id>/trace.json`.
    pub fn finalize(self, store: &Store) -> Result<RunId, Error> {
        let tree = TraceTree {
            run_id: self.run_id.0.clone(),
            root_target: self.root_target,
            root_input: self.root_input,
            root_output: None,
            root_error: None,
            started_at: self.started_at,
            ended_at: now_ms(),
            nodes: self.nodes,
        };
        store
            .save_trace(&tree)
            .map_err(|e| Error::Trace(format!("save_trace: {e}")))?;
        Ok(self.run_id)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
