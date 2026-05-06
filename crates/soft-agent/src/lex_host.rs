//! Compile and call a Lex program with the trace recorder attached.
//!
//! Wraps `lex_runtime::Vm` so soft-agent can invoke Lex helper functions
//! under a `lex_trace::Recorder` that captures the full call/effect tree.
//!
//! v1 scope: standalone — each [`LexHost::call`] creates its own VM +
//! Recorder and returns a per-call [`TraceTree`]. Folding these per-call
//! trees into the runner's [`crate::TraceWriter`] (so they appear as
//! nested events in the agent's overall trace) is a follow-up slice.

use std::time::{SystemTime, UNIX_EPOCH};

use lex_bytecode::{vm::Vm, Program, Value as LexValue};
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

/// A compiled Lex program that can be invoked with a trace recorder
/// attached. Build once, call many times — each call creates its own
/// fresh VM and Recorder.
pub struct LexHost {
    program: Program,
}

impl LexHost {
    /// Parse, type-check, and compile a Lex source string.
    pub fn from_source(src: &str) -> Result<Self, Error> {
        let prog = lex_syntax::parse_source(src)
            .map_err(|e| Error::Spec(format!("parse: {e:?}")))?;
        let stages = lex_ast::canonicalize_program(&prog);
        if let Err(errs) = lex_types::check_program(&stages) {
            return Err(Error::Spec(format!("typecheck: {errs:?}")));
        }
        let program = lex_bytecode::compile_program(&stages);
        Ok(LexHost { program })
    }

    /// Call a Lex function by name with the given arguments.
    ///
    /// Returns the function's value plus a [`TraceTree`] capturing call
    /// and effect events recorded during this invocation.
    pub fn call(&self, fn_name: &str, args: Vec<LexValue>) -> Result<LexCall, Error> {
        let recorder = Recorder::new();
        let handle = recorder.handle();

        let policy = Policy::permissive();
        let handler = DefaultHandler::new(policy);
        let mut vm = Vm::with_handler(&self.program, Box::new(handler));
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
