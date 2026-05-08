//! Replay tooling: scan a [`lex_trace::TraceTree`] for the soft-agent
//! invariant set.
//!
//! Counts events by target name and verifies that
//! `executed ≤ allowed` for *gated* runs (no `Deny` or `Inconclusive`
//! was bypassed). Ungated runs — those with no `gate.verdict` events
//! at all — are not flagged: there's no Deny to bypass, so
//! `executed > allowed` is structural, not a real violation. Use
//! [`Counts::is_gated`] to distinguish the two.

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

    /// True iff the run produced any `gate.verdict` events. An
    /// ungated run (no spec gate configured, or no actions reached
    /// the gate) doesn't have an Allow/Deny to compare `executed`
    /// against, so the `executed > allowed` violation rule doesn't
    /// apply.
    pub fn is_gated(&self) -> bool {
        self.allowed + self.denied + self.inconclusive > 0
    }

    /// True if the trace bypassed a `Deny`/`Inconclusive` verdict
    /// — i.e. the run was gated AND `executed > allowed`. Ungated
    /// runs always return `false` (use [`Self::is_gated`] to detect
    /// them separately).
    pub fn has_violation(&self) -> bool {
        self.is_gated() && self.executed > self.allowed
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_counts_are_ungated_and_clean() {
        let c = Counts::default();
        assert!(!c.is_gated());
        assert!(!c.has_violation());
    }

    #[test]
    fn ungated_run_with_executions_is_not_a_violation() {
        // Classic pv-agent shape: tick fires, action proposed +
        // executed, no gate ever evaluated.
        let c = Counts {
            proposed: 1,
            executed: 1,
            ..Counts::default()
        };
        assert!(!c.is_gated(), "no Allow/Deny/Inconclusive ⇒ ungated");
        assert!(
            !c.has_violation(),
            "ungated runs must not be flagged just because executed > allowed"
        );
    }

    #[test]
    fn gated_clean_allow_run_is_not_a_violation() {
        let c = Counts {
            proposed: 1,
            allowed: 1,
            executed: 1,
            ..Counts::default()
        };
        assert!(c.is_gated());
        assert!(!c.has_violation());
    }

    #[test]
    fn gated_run_that_bypasses_deny_is_a_violation() {
        let c = Counts {
            proposed: 2,
            allowed: 1,
            denied: 1,
            executed: 2,
            ..Counts::default()
        };
        assert!(c.is_gated());
        assert!(
            c.has_violation(),
            "gate denied 1 but 2 actions executed — real bypass"
        );
    }

    #[test]
    fn gated_clean_deny_run_is_not_a_violation() {
        let c = Counts {
            proposed: 1,
            denied: 1,
            executed: 0,
            ..Counts::default()
        };
        assert!(c.is_gated());
        assert!(
            !c.has_violation(),
            "denied actions correctly didn't execute"
        );
    }

    #[test]
    fn inconclusive_alone_marks_run_as_gated() {
        // Default policy treats Inconclusive as Deny — but the trace
        // still records the gate.verdict event. A run that produced
        // only Inconclusive verdicts is gated; if anything executed
        // anyway, that's a violation.
        let c = Counts {
            proposed: 1,
            inconclusive: 1,
            executed: 1,
            ..Counts::default()
        };
        assert!(c.is_gated());
        assert!(c.has_violation());
    }
}
