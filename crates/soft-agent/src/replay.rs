//! Replay tooling: scan a [`lex_trace::TraceTree`] for the soft-agent
//! invariant set.
//!
//! Counts events by target name and verifies that
//! `executed ≤ allowed` (no `Deny` or `Inconclusive` was bypassed).

use lex_trace::{TraceNode, TraceTree};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub proposed: usize,
    pub allowed: usize,
    pub denied: usize,
    pub inconclusive: usize,
    pub executed: usize,
    pub skipped: usize,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.proposed += other.proposed;
        self.allowed += other.allowed;
        self.denied += other.denied;
        self.inconclusive += other.inconclusive;
        self.executed += other.executed;
        self.skipped += other.skipped;
    }

    /// True if the trace bypassed a `Deny`/`Inconclusive` verdict — i.e.
    /// `action.executed` count exceeds `allowed` count.
    pub fn has_violation(&self) -> bool {
        self.executed > self.allowed
    }
}

/// Walk every node in `tree`, counting events by target.
pub fn scan_trace(tree: &TraceTree) -> Counts {
    let mut c = Counts::default();
    for n in &tree.nodes {
        scan_node(n, &mut c);
    }
    c
}

fn scan_node(n: &TraceNode, c: &mut Counts) {
    match n.target.as_str() {
        "action.proposed" => c.proposed += 1,
        "gate.verdict" => match verdict_kind(n) {
            Some("allow") => c.allowed += 1,
            Some("deny") => c.denied += 1,
            Some("inconclusive") => c.inconclusive += 1,
            _ => {}
        },
        "action.executed" => c.executed += 1,
        "action.skipped" => c.skipped += 1,
        _ => {}
    }
    for child in &n.children {
        scan_node(child, c);
    }
}

fn verdict_kind(n: &TraceNode) -> Option<&str> {
    let out = n.output.as_ref()?;
    let verdict = out.get("verdict")?;
    match verdict {
        Value::Object(map) => map.get("verdict").and_then(|v| v.as_str()),
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}
