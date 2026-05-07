//! Runner — drives an agent through its mailbox, applying the gate and
//! recording each step to the trace.
//!
//! Pipeline per inbound message:
//!
//! 1. Trace `a2a.received`.
//! 2. Dispatch to the topic handler (Rust closure or Lex function); collect
//!    proposed actions.
//! 3. For each action: trace `action.proposed`; if a gate is configured,
//!    evaluate it and trace `gate.verdict` (Inconclusive is treated as Deny
//!    by default); if the action is `CallMcp`, check
//!    `Agent::allows_mcp_server` (per-server runtime allowlist; lex 0.2 ships
//!    flat `[mcp]`); execute via the configured [`ActionExecutor`]; trace
//!    `action.executed`.
//!
//! Trace records are flushed at [`Runner::finalize`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use indexmap::IndexMap;
use lex_bytecode::Value as LexValue;
use serde_json::{json, Value};

use crate::{
    executor::{ActionExecutor, MockExecutor},
    gate::{action_to_json, Gate, Verdict},
    lex_host::LexHost,
    metrics::Metrics,
    trace::TraceWriter,
    A2aMessage, Action, Agent, Error, Mailbox, MailboxSender,
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::CallMcp { .. } => "call_mcp",
        Action::SendA2a { .. } => "send_a2a",
        Action::LocalLlm { .. } => "local_llm",
        Action::CloudLlm { .. } => "cloud_llm",
    }
}

fn verdict_label(v: &Verdict) -> &'static str {
    match v {
        Verdict::Allow => "Allow",
        Verdict::Deny { .. } => "Deny",
        Verdict::Inconclusive { .. } => "Inconclusive",
    }
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

/// Native Rust topic handler. See [`Handler::Rust`].
pub type RustHandlerFn = Box<dyn FnMut(&mut Value, &A2aMessage) -> Vec<Action> + Send>;

/// One topic handler. Either a Rust closure or the name of a Lex function
/// that the runner will invoke via its [`LexHost`].
pub enum Handler {
    /// Native Rust closure. Mutates state directly; returns proposed actions.
    Rust(RustHandlerFn),
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
    metrics: Arc<Metrics>,
}

impl Runner {
    pub fn builder() -> RunnerBuilder {
        RunnerBuilder::default()
    }

    /// Construct a [`RunnerBuilder`] from a Lex source file declaring an
    /// agent via the soft-agent DSL (see [`crate::lex_dsl`]). The user's
    /// source is prepended with [`crate::DSL_PREAMBLE`] and compiled into
    /// a [`LexHost`]; its `config()` function is invoked to produce the
    /// agent's [`AgentConfig`]; and each handler entry is registered as
    /// `handle_lex(topic, fn_name)`.
    ///
    /// The caller still supplies `mailbox`, `state`, `gate`, `bindings_fn`,
    /// `executor`, etc. via the returned builder.
    pub fn from_lex_source(user_src: &str) -> Result<RunnerBuilder, Error> {
        let combined = format!("{}\n{}", crate::DSL_PREAMBLE, user_src);
        let host = LexHost::from_source(&combined)?;
        Self::from_lex_host(host)
    }

    /// Like [`Self::from_lex_source`] but takes a pre-built [`LexHost`]
    /// directly. Use this when you need to install a custom
    /// `EffectHandler` factory on the host before the runner consumes it
    /// (see [`LexHost::with_handler_factory`]).
    ///
    /// The caller is responsible for prepending [`crate::DSL_PREAMBLE`]
    /// to the user source before compiling, if they want the DSL.
    pub fn from_lex_host(host: LexHost) -> Result<RunnerBuilder, Error> {
        let result = host.call("config", Vec::new())?;
        let setup = crate::lex_dsl::parse_lex_config(&result.value)?;
        let mut builder = RunnerBuilder::default()
            .agent(setup.config.build()?)
            .lex_host(host);
        for (topic, fn_name) in setup.handlers {
            builder = builder.handle_lex(topic, fn_name);
        }
        Ok(builder)
    }

    /// Process at most one inbound message. Returns [`StepReport::Idle`]
    /// if the mailbox is empty.
    pub fn step(&mut self) -> Result<StepReport, Error> {
        let msg = match self.mailbox.try_recv() {
            Some(m) => m,
            None => {
                self.metrics.inc_step("Idle");
                return Ok(StepReport::Idle);
            }
        };
        self.metrics.inc_message(&msg.topic);
        if msg.from == "self" {
            self.metrics.inc_tick(&msg.topic);
        }
        self.trace.record_effect(
            "a2a.received",
            json!({"from": msg.from, "topic": msg.topic, "payload": msg.payload}),
            Ok(Value::Null),
        );

        let proposed = match dispatch(
            &mut self.state,
            &mut self.handlers,
            self.lex_host.as_ref(),
            &msg,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.metrics.inc_step("Error");
                return Err(e);
            }
        };

        let mut allowed = 0usize;
        let mut denied = 0usize;

        for action in &proposed {
            let summary = action_to_json(action);
            let kind = action_kind(action);
            self.metrics.inc_action_proposed(kind);
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
                        Box::new(h_for_closure.clone()) as Box<dyn lex_bytecode::vm::Tracer>
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
                    self.metrics.inc_gate_verdict(verdict_label(&verdict));
                    matches!(verdict, Verdict::Allow)
                }
                None => true,
            };
            if !gate_ok {
                self.metrics.inc_action_denied(kind, "gate");
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
                    self.metrics.inc_action_denied(kind, "mcp_allowlist");
                    denied += 1;
                    continue;
                }
            }

            // Execute. Executor errors don't count as denials — the
            // action passed all gates; the failure is downstream and is
            // recorded in the trace's `action.executed` outcome.
            let outcome = self.executor.execute(action).map_err(|e| e.to_string());
            self.trace
                .record_effect("action.executed", summary, outcome);
            self.metrics.inc_action_allowed(kind);
            allowed += 1;
        }

        self.metrics.inc_step("Processed");
        Ok(StepReport::Processed { allowed, denied })
    }

    /// Read-only access to this runner's metrics handle. Cloneable
    /// `Arc` — share with the soft-a2a server's `/metrics` route.
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
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
        Ok(DrainReport {
            messages,
            total_allowed,
            total_denied,
        })
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
            let msg_v = LexValue::from_json(&json!({
                "from": msg.from,
                "topic": msg.topic,
                "payload_json": serde_json::to_string(&msg.payload)
                    .unwrap_or_else(|_| "null".into()),
            }));
            let result = host.call(fn_name, vec![state_v, msg_v])?;
            let (actions, new_state) = interpret_lex_result(&result.value)?;
            if let Some(ns) = new_state {
                *state = ns;
            }
            Ok(actions)
        }
    }
}

/// Interpret a Lex handler's return value.
///
/// Two shapes are accepted:
/// - **Stateless**: `List[ActionRecord]`. The handler proposed actions
///   and didn't change state.
/// - **Stateful**: `{ state: S, actions: List[ActionRecord] }`. The
///   handler proposed actions *and* returned a replacement state record.
///
/// Stateful is detected by the result being a `Record` with both
/// `state` and `actions` fields. Otherwise we expect a `List`.
fn interpret_lex_result(v: &LexValue) -> Result<(Vec<Action>, Option<Value>), Error> {
    let json = v.to_json();
    if let Value::Object(map) = &json {
        if let (Some(actions_v), Some(state_v)) = (map.get("actions"), map.get("state")) {
            let actions = actions_from_json_array(actions_v)?;
            return Ok((actions, Some(state_v.clone())));
        }
    }
    let actions = actions_from_json_array(&json)?;
    Ok((actions, None))
}

fn actions_from_json_array(v: &Value) -> Result<Vec<Action>, Error> {
    let arr = v.as_array().ok_or_else(|| {
        Error::Spec(format!(
            "Lex handler return must be a list of action records (or {{state, actions}}), got: {v}"
        ))
    })?;
    arr.iter().map(Action::from_json).collect()
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
    ticks: Vec<TickSpec>,
    metrics: Option<Arc<Metrics>>,
}

/// One periodic tick configured on the runner. The tick thread sends a
/// synthetic [`A2aMessage`] (`from = "self"`, payload = `null`) on the
/// configured `topic` every `duration`. The thread exits when the
/// mailbox is closed (i.e. the [`Runner`] is dropped).
struct TickSpec {
    duration: Duration,
    topic: String,
    sender: MailboxSender,
}

impl RunnerBuilder {
    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Returns the agent's declared name once an agent has been set —
    /// useful for callers that need to construct an executor or trace
    /// writer keyed by the agent's identity *before* `.build()` consumes
    /// the builder (e.g. `A2aRoutedExecutor::new(name, peers)` in
    /// [`soft-runner`]).
    pub fn agent_name(&self) -> Option<&str> {
        self.agent.as_ref().map(|a| a.id().as_str())
    }

    /// The agent's declared `spec_paths` from `agent_specs([...])` in
    /// the lex DSL. Empty until [`Self::agent`] has been set (or until
    /// the builder has been seeded by [`Runner::from_lex_host`]).
    /// soft-runner reads this after compile to decide whether to load
    /// `.spec` files and install a [`Gate`].
    pub fn spec_paths(&self) -> &[String] {
        match &self.agent {
            Some(a) => a.spec_paths(),
            None => &[],
        }
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
    pub fn handle_lex(mut self, topic: impl Into<String>, fn_name: impl Into<String>) -> Self {
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

    /// Provide a shared [`Metrics`] handle. If not set, the runner
    /// constructs its own private one (still readable via
    /// [`Runner::metrics`]). Pass an explicit `Arc<Metrics>` when you
    /// want the soft-a2a `/metrics` route to read the same counters
    /// the runner increments.
    pub fn metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }

    /// Register a periodic self-tick. Every `duration`, the runner's
    /// mailbox receives an [`A2aMessage`] with `from = "self"`, the given
    /// `topic`, and a `null` payload. Useful for self-initiated agents
    /// (heartbeats, periodic broadcasts, scheduled dispatch).
    ///
    /// The `sender` must be a clone of the same `MailboxSender` returned
    /// by [`Mailbox::new`] for this runner. The tick thread exits when
    /// the mailbox is closed (i.e. the [`Runner`] is dropped).
    pub fn tick(
        mut self,
        duration: Duration,
        topic: impl Into<String>,
        sender: MailboxSender,
    ) -> Self {
        self.ticks.push(TickSpec {
            duration,
            topic: topic.into(),
            sender,
        });
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

        // Spawn one detached thread per tick. They exit when the mailbox
        // is closed (mpsc::Sender::send returns Err once the receiver is
        // dropped).
        for spec in self.ticks {
            std::thread::spawn(move || loop {
                std::thread::sleep(spec.duration);
                let msg = A2aMessage {
                    from: "self".to_string(),
                    topic: spec.topic.clone(),
                    payload: Value::Null,
                };
                if spec.sender.send(msg).is_err() {
                    break;
                }
            });
        }

        let metrics = self
            .metrics
            .unwrap_or_else(|| Arc::new(Metrics::new(agent.id().as_str())));

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
            metrics,
        })
    }
}
