//! Runner — drives an agent through its mailbox, applying the gate and
//! recording each step to the trace.
//!
//! Pipeline per inbound message:
//!
//! 1. Trace `a2a.received`.
//! 2. Dispatch to the topic handler (Rust closure or Lex function);
//!    collect proposed actions.
//! 3. For each action:
//!    a. Trace `action.proposed`.
//!    b. If a gate is configured, evaluate; trace `gate.verdict`.
//!       Inconclusive is treated as Deny by default.
//!    c. If the action is `CallMcp`, check `Agent::allows_mcp_server`
//!       (per-server runtime allowlist; lex 0.2 ships flat `[mcp]`).
//!    d. Execute via the configured [`ActionExecutor`]; trace
//!       `action.executed`.
//!
//! Trace records are flushed at [`Runner::finalize`].

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::{json, Value};

use crate::{
    Action, A2aMessage, Agent, Error, Mailbox,
    executor::{ActionExecutor, MockExecutor},
    gate::{action_to_json, Gate, Verdict},
    lex_host::LexHost,
    trace::TraceWriter,
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn bindings_to_json(b: &IndexMap<String, LexValue>) -> Value {
    let mut obj = serde_json::Map::new();
    for (k, v) in b {
        obj.insert(k.clone(), v.to_json());
    }
    Value::Object(obj)
}

/// Builds scalar spec bindings for a (state, action) pair.
///
/// `spec-checker` 0.2 quantifies over scalar types only (`Int`, `Float`,
/// `Bool`, `Str`); record-shaped bindings aren't supported yet. Each agent
/// configures a `BindingsFn` that pulls the scalars its specs need out of
/// state and action. Default is empty — gates without a `bindings_fn` will
/// fail to bind any quantifier and (typically) return Inconclusive.
pub type BindingsFn = Box<dyn Fn(&Value, &Action) -> IndexMap<String, LexValue> + Send>;

/// One topic handler. Either a Rust closure or the name of a Lex function
/// that the runner will invoke via its [`LexHost`].
pub enum Handler {
    /// Native Rust closure. Mutates state directly; returns proposed actions.
    Rust(Box<dyn FnMut(&mut Value, &A2aMessage) -> Vec<Action> + Send>),
    /// Name of a Lex function compiled into the runner's `LexHost`.
    /// The function is called with `(state, msg)` arguments and must
    /// return a list of action records (see [`Action::from_json`] for
    /// the expected record shape).
    Lex(String),
}

pub struct Runner {
    agent: Agent,
    state: Value,
    mailbox: Mailbox,
    handlers: HashMap<String, Handler>,
    lex_host: Option<LexHost>,
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
            json!({"from": msg.from, "topic": msg.topic, "payload": msg.payload}),
            Ok(Value::Null),
        );

        let proposed = dispatch(
            &mut self.state,
            &mut self.handlers,
            self.lex_host.as_ref(),
            &msg,
        )?;

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

                    // Per-action spec recorder so the spec body's
                    // helper calls (under_budget → projected_load + ...)
                    // show up nested in our trace via spec-checker
                    // 0.2.1's evaluate_gate_compiled_traced.
                    let spec_recorder = lex_trace::Recorder::new();
                    let spec_handle = spec_recorder.handle();
                    let h_for_closure = spec_handle.clone();
                    let spec_started = now_ms();
                    let verdict = gate.evaluate_traced(&bindings, move || {
                        Box::new(h_for_closure.clone())
                            as Box<dyn lex_bytecode::vm::Tracer>
                    });
                    let spec_ended = now_ms();
                    let spec_tree = spec_handle.finalize(
                        "spec.eval".to_string(),
                        bindings_to_json(&bindings),
                        Some(serde_json::to_value(&verdict).unwrap_or(Value::Null)),
                        None,
                        spec_started,
                        spec_ended,
                    );
                    let verdict_json = json!({
                        "verdict": serde_json::to_value(&verdict)
                            .unwrap_or(Value::Null),
                        "spec_trace": serde_json::to_value(&spec_tree)
                            .unwrap_or(Value::Null),
                    });
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
            let outcome = self.executor.execute(action).map_err(|e| e.to_string());
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

fn dispatch(
    state: &mut Value,
    handlers: &mut HashMap<String, Handler>,
    lex_host: Option<&LexHost>,
    msg: &A2aMessage,
) -> Result<Vec<Action>, Error> {
    let handler = handlers
        .get_mut(&msg.topic)
        .ok_or_else(|| Error::HandlerNotRegistered(msg.topic.clone()))?;
    match handler {
        Handler::Rust(f) => Ok(f(state, msg)),
        Handler::Lex(fn_name) => {
            let host = lex_host.ok_or_else(|| {
                Error::InvalidConfig(format!(
                    "Lex handler `{fn_name}` registered for `{}` but no lex_host on runner",
                    msg.topic
                ))
            })?;
            let state_v = LexValue::from_json(state);
            // Always pass a fixed-shape message record so Lex handlers
            // can declare a stable signature. The original payload is
            // serialised to a JSON string so handlers can choose whether
            // to parse it.
            let msg_v = LexValue::from_json(&json!({
                "from": msg.from,
                "topic": msg.topic,
                "payload_json": serde_json::to_string(&msg.payload)
                    .unwrap_or_else(|_| "null".into()),
            }));
            let result = host.call(fn_name, vec![state_v, msg_v])?;
            actions_from_lex_value(&result.value)
        }
    }
}

fn actions_from_lex_value(v: &LexValue) -> Result<Vec<Action>, Error> {
    let LexValue::List(items) = v else {
        return Err(Error::Spec(format!(
            "Lex handler must return a List of action records, got {v:?}"
        )));
    };
    items.iter().map(Action::from_lex_value).collect()
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
    lex_host: Option<LexHost>,
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

    /// Register a Rust closure as the handler for `topic`.
    pub fn handle<F>(mut self, topic: impl Into<String>, handler: F) -> Self
    where
        F: FnMut(&mut Value, &A2aMessage) -> Vec<Action> + Send + 'static,
    {
        self.handlers
            .insert(topic.into(), Handler::Rust(Box::new(handler)));
        self
    }

    /// Register a Lex function as the handler for `topic`. The function
    /// must exist in the runner's [`LexHost`] (configured via
    /// [`Self::lex_host`]) and have the signature
    /// `fn(state, msg) -> List[ActionRecord]`.
    pub fn handle_lex(
        mut self,
        topic: impl Into<String>,
        fn_name: impl Into<String>,
    ) -> Self {
        self.handlers
            .insert(topic.into(), Handler::Lex(fn_name.into()));
        self
    }

    /// Provide the [`LexHost`] used to dispatch Lex handlers.
    pub fn lex_host(mut self, host: LexHost) -> Self {
        self.lex_host = Some(host);
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

        // Validate: every Lex handler needs a lex_host.
        let has_lex = self.handlers.values().any(|h| matches!(h, Handler::Lex(_)));
        if has_lex && self.lex_host.is_none() {
            return Err(Error::InvalidConfig(
                "Lex handler registered but no lex_host provided".into(),
            ));
        }

        let executor = self.executor.unwrap_or_else(|| Box::new(MockExecutor::new()));
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
            lex_host: self.lex_host,
            gate: self.gate,
            bindings_fn,
            executor,
            trace,
        })
    }
}
