//! Runner — drives an agent through its mailbox, applying the gate and
//! recording each step to the trace.
//!
//! Pipeline per inbound message:
//!
//! 1. Trace `a2a.received`.
//! 2. Dispatch to the topic handler; collect proposed actions.
//! 3. For each action:
//!    a. Trace `action.proposed`.
//!    b. If a gate is configured, evaluate; trace `gate.verdict`.
//!       Inconclusive is treated as Deny by default.
//!    c. If the action is `CallMcp`, check `Agent::allows_mcp_server`
//!       (per-server runtime allowlist; lex 0.2 ships flat `[mcp]`).
//!    d. Execute via the configured [`ActionExecutor`]; trace
//!       `action.executed`.
//!
//! Trace records are flushed only at [`Runner::finalize`].

use std::collections::HashMap;

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::Value;

use crate::{
    Action, A2aMessage, Agent, Error, Mailbox,
    executor::{ActionExecutor, MockExecutor},
    gate::{action_to_json, Gate, Verdict},
    trace::TraceWriter,
};

pub type Handler = Box<dyn FnMut(&mut Value, &A2aMessage) -> Vec<Action> + Send>;

/// Builds scalar spec bindings for a (state, action) pair.
///
/// `spec-checker` 0.2 quantifies over scalar types only (`Int`, `Float`,
/// `Bool`, `Str`); record-shaped bindings aren't supported yet. Each agent
/// configures a `BindingsFn` that pulls the scalars its specs need out of
/// state and action. Default is empty — gates without a `bindings_fn` will
/// fail to bind any quantifier and (typically) return Inconclusive.
pub type BindingsFn = Box<dyn Fn(&Value, &Action) -> IndexMap<String, LexValue> + Send>;

pub struct Runner {
    agent: Agent,
    state: Value,
    mailbox: Mailbox,
    handlers: HashMap<String, Handler>,
    gate: Option<Gate>,
    bindings_fn: BindingsFn,
    executor: Box<dyn ActionExecutor>,
    trace: TraceWriter,
}

impl Runner {
    pub fn builder() -> RunnerBuilder {
        RunnerBuilder::default()
    }

    /// Process at most one inbound message. Returns [`StepReport::Idle`]
    /// if the mailbox is empty.
    pub fn step(&mut self) -> Result<StepReport, Error> {
        let msg = match self.mailbox.try_recv() {
            Some(m) => m,
            None => return Ok(StepReport::Idle),
        };
        self.trace.record_effect(
            "a2a.received",
            serde_json::json!({
                "from": msg.from, "topic": msg.topic, "payload": msg.payload,
            }),
            Ok(Value::Null),
        );

        let handler = self
            .handlers
            .get_mut(&msg.topic)
            .ok_or_else(|| Error::HandlerNotRegistered(msg.topic.clone()))?;
        let proposed = handler(&mut self.state, &msg);

        let mut allowed = 0usize;
        let mut denied = 0usize;

        for action in &proposed {
            let summary = action_to_json(action);
            self.trace
                .record_effect("action.proposed", summary.clone(), Ok(Value::Null));

            // Spec gate
            let gate_ok = match &self.gate {
                Some(gate) => {
                    let bindings = (self.bindings_fn)(&self.state, action);
                    let verdict = gate.evaluate(&bindings);
                    let verdict_json =
                        serde_json::to_value(&verdict).unwrap_or(Value::Null);
                    self.trace
                        .record_effect("gate.verdict", summary.clone(), Ok(verdict_json));
                    matches!(verdict, Verdict::Allow)
                }
                None => true,
            };
            if !gate_ok {
                denied += 1;
                continue;
            }

            // Per-server MCP allowlist (runtime check; lex 0.2 ships flat [mcp])
            if let Action::CallMcp { server, .. } = action {
                if !self.agent.allows_mcp_server(server) {
                    self.trace.record_effect(
                        "action.skipped",
                        summary.clone(),
                        Err(format!("mcp server `{server}` not in allowlist")),
                    );
                    denied += 1;
                    continue;
                }
            }

            // Execute
            let outcome = self
                .executor
                .execute(action)
                .map_err(|e| e.to_string());
            self.trace.record_effect("action.executed", summary, outcome);
            allowed += 1;
        }

        Ok(StepReport::Processed { allowed, denied })
    }

    /// Drain the mailbox, processing each pending message.
    pub fn drain(&mut self) -> Result<DrainReport, Error> {
        let mut total_allowed = 0;
        let mut total_denied = 0;
        let mut messages = 0;
        loop {
            match self.step()? {
                StepReport::Idle => break,
                StepReport::Processed { allowed, denied } => {
                    total_allowed += allowed;
                    total_denied += denied;
                    messages += 1;
                }
            }
        }
        Ok(DrainReport { messages, total_allowed, total_denied })
    }

    /// Persist the trace to `store` and return the run ID.
    pub fn finalize(self, store: &lex_store::Store) -> Result<lex_trace::RunId, Error> {
        self.trace.finalize(store)
    }

    pub fn state(&self) -> &Value {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut Value {
        &mut self.state
    }

    pub fn agent(&self) -> &Agent {
        &self.agent
    }
}

#[derive(Debug)]
pub enum StepReport {
    Idle,
    Processed { allowed: usize, denied: usize },
}

#[derive(Debug)]
pub struct DrainReport {
    pub messages: usize,
    pub total_allowed: usize,
    pub total_denied: usize,
}

#[derive(Default)]
pub struct RunnerBuilder {
    agent: Option<Agent>,
    state: Option<Value>,
    mailbox: Option<Mailbox>,
    handlers: HashMap<String, Handler>,
    gate: Option<Gate>,
    bindings_fn: Option<BindingsFn>,
    executor: Option<Box<dyn ActionExecutor>>,
}

impl RunnerBuilder {
    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn state(mut self, state: Value) -> Self {
        self.state = Some(state);
        self
    }

    pub fn mailbox(mut self, mbox: Mailbox) -> Self {
        self.mailbox = Some(mbox);
        self
    }

    pub fn handle(mut self, topic: impl Into<String>, handler: Handler) -> Self {
        self.handlers.insert(topic.into(), handler);
        self
    }

    pub fn gate(mut self, g: Gate) -> Self {
        self.gate = Some(g);
        self
    }

    pub fn bindings_fn(mut self, f: BindingsFn) -> Self {
        self.bindings_fn = Some(f);
        self
    }

    pub fn executor(mut self, e: Box<dyn ActionExecutor>) -> Self {
        self.executor = Some(e);
        self
    }

    pub fn build(self) -> Result<Runner, Error> {
        let agent = self
            .agent
            .ok_or_else(|| Error::InvalidConfig("missing agent".into()))?;
        let state = self.state.unwrap_or(Value::Null);
        let mailbox = self
            .mailbox
            .ok_or_else(|| Error::InvalidConfig("missing mailbox".into()))?;
        let executor = self
            .executor
            .unwrap_or_else(|| Box::new(MockExecutor::new()));
        let bindings_fn = self
            .bindings_fn
            .unwrap_or_else(|| Box::new(|_, _| IndexMap::new()));
        let trace = TraceWriter::new(
            agent.id().as_str(),
            agent.id().as_str().to_string(),
            state.clone(),
        );
        Ok(Runner {
            agent,
            state,
            mailbox,
            handlers: self.handlers,
            gate: self.gate,
            bindings_fn,
            executor,
            trace,
        })
    }
}
