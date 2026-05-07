//! Compile and call a Lex program with the trace recorder attached.
//!
//! Wraps `lex_runtime::Vm` so soft-agent can invoke Lex helper functions
//! under a `lex_trace::Recorder` that captures the full call/effect tree.
//!
//! v1 scope: standalone — each [`LexHost::call`] creates its own VM +
//! Recorder and returns a per-call [`TraceTree`]. Folding these per-call
//! trees into the runner's [`crate::TraceWriter`] (so they appear as
//! nested events in the agent's overall trace) is a follow-up slice.
//!
//! ## Custom effect handlers
//!
//! By default each `call` constructs a fresh [`DefaultHandler`] with a
//! permissive policy — that gives Lex code access to lex-runtime's
//! shipped `[a2a]`, `[mcp]`, `[llm_local]`, `[llm_cloud]` builtins via
//! their default backends (Ollama, OpenAI-shape HTTP, etc.).
//!
//! Use [`LexHost::with_handler_factory`] to plug in a custom
//! `EffectHandler` instead. Typical wrapper pattern: hold a
//! `DefaultHandler` inside, intercept specific `(kind, op)` pairs, and
//! delegate everything else to the inner handler. soft-runner uses this
//! to swap the OpenAI-shape `agent.cloud_complete` for an Anthropic
//! Messages API call when the user wants real Claude in the loop.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lex_bytecode::{
    vm::{EffectHandler, Vm},
    Program, Value as LexValue,
};
use lex_runtime::{DefaultHandler, Policy};
use lex_trace::{Recorder, TraceTree};
use serde_json::Value;

use crate::Error;

/// Result of one Lex function invocation.
#[derive(Debug)]
pub struct LexCall {
    pub value: LexValue,
    pub tree: TraceTree,
}

/// Factory closure that produces a fresh `Box<dyn EffectHandler>` per
/// VM invocation. Each call produces its own VM and tracer; the factory
/// also produces its own handler so handlers don't have to be
/// `Sync + Send` themselves — only the factory is.
pub type HandlerFactory = Arc<dyn Fn() -> Box<dyn EffectHandler> + Send + Sync>;

fn default_handler_factory() -> HandlerFactory {
    Arc::new(|| Box::new(DefaultHandler::new(Policy::permissive())))
}

/// A compiled Lex program that can be invoked with a trace recorder
/// attached. Build once, call many times — each call creates its own
/// fresh VM and Recorder, and a fresh handler from the configured
/// factory.
pub struct LexHost {
    program: Program,
    handler_factory: HandlerFactory,
}

impl LexHost {
    /// Parse, type-check, and compile a Lex source string. Uses the
    /// default `EffectHandler` (lex-runtime's `DefaultHandler` with
    /// `Policy::permissive`).
    pub fn from_source(src: &str) -> Result<Self, Error> {
        let prog = lex_syntax::parse_source(src)
            .map_err(|e| Error::Spec(format!("parse: {e:?}")))?;
        let stages = lex_ast::canonicalize_program(&prog);
        if let Err(errs) = lex_types::check_program(&stages) {
            return Err(Error::Spec(format!("typecheck: {errs:?}")));
        }
        let program = lex_bytecode::compile_program(&stages);
        Ok(LexHost {
            program,
            handler_factory: default_handler_factory(),
        })
    }

    /// Replace the `EffectHandler` factory. The closure is invoked once
    /// per [`Self::call`] to produce a fresh handler. Use to install a
    /// wrapper around `DefaultHandler` that intercepts specific
    /// `(kind, op)` pairs (e.g. routing `agent.cloud_complete` to a
    /// non-OpenAI backend).
    pub fn with_handler_factory<F>(mut self, factory: F) -> Self
    where
        F: Fn() -> Box<dyn EffectHandler> + Send + Sync + 'static,
    {
        self.handler_factory = Arc::new(factory);
        self
    }

    /// Call a Lex function by name with the given arguments.
    ///
    /// Returns the function's value plus a [`TraceTree`] capturing call
    /// and effect events recorded during this invocation.
    pub fn call(&self, fn_name: &str, args: Vec<LexValue>) -> Result<LexCall, Error> {
        let recorder = Recorder::new();
        let handle = recorder.handle();

        let handler = (self.handler_factory)();
        let mut vm = Vm::with_handler(&self.program, handler);
        vm.set_tracer(Box::new(recorder));

        let args_summary = Value::Array(args.iter().map(LexValue::to_json).collect());
        let started = now_ms();
        let result = vm
            .call(fn_name, args)
            .map_err(|e| Error::Spec(format!("vm call `{fn_name}`: {e}")))?;
        let ended = now_ms();
        let result_json = result.to_json();

        let tree = handle.finalize(
            fn_name.to_string(),
            args_summary,
            Some(result_json),
            None,
            started,
            ended,
        );

        Ok(LexCall { value: result, tree })
    }

    pub fn program(&self) -> &Program {
        &self.program
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
