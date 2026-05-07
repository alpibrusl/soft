//! Lightweight Prometheus-format counters for the runner.
//!
//! All counters are atomic, `Arc`-shared, and safe to read from a
//! different thread than the one running the runner — soft-a2a's HTTP
//! server reads them to render `/metrics` while the runner increments
//! them from its step loop.
//!
//! Only counters here, no histograms or gauges. Keeping the surface
//! minimal: every metric is `monotonic_total{label="..."}`. Render via
//! [`Metrics::render_prometheus`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Per-label counter map. Lock contention is negligible: the runner
/// holds the lock for at most one `entry().or_insert(0).fetch_add(1)`
/// per action, and the HTTP renderer holds it for one snapshot read.
#[derive(Default)]
struct LabeledCounter {
    name: &'static str,
    label_key: &'static str,
    inner: Mutex<HashMap<String, AtomicU64>>,
}

impl LabeledCounter {
    fn new(name: &'static str, label_key: &'static str) -> Self {
        Self {
            name,
            label_key,
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn inc(&self, label: &str) {
        let mut map = self.inner.lock().expect("metrics map poisoned");
        map.entry(label.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    fn render(&self, out: &mut String) {
        let map = self.inner.lock().expect("metrics map poisoned");
        if map.is_empty() {
            // Always emit a HELP/TYPE preamble even with zero series so
            // scrapers see the metric exists.
            out.push_str(&format!("# TYPE {} counter\n", self.name));
            return;
        }
        out.push_str(&format!("# TYPE {} counter\n", self.name));
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for k in keys {
            let v = map[k].load(Ordering::Relaxed);
            out.push_str(&format!(
                "{}{{{}=\"{}\"}} {}\n",
                self.name,
                self.label_key,
                escape_label(k),
                v
            ));
        }
    }
}

/// Two-label counter (e.g. denied{kind, reason}).
#[derive(Default)]
struct LabeledCounter2 {
    name: &'static str,
    keys: (&'static str, &'static str),
    inner: Mutex<HashMap<(String, String), AtomicU64>>,
}

impl LabeledCounter2 {
    fn new(name: &'static str, keys: (&'static str, &'static str)) -> Self {
        Self {
            name,
            keys,
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn inc(&self, a: &str, b: &str) {
        let mut map = self.inner.lock().expect("metrics map poisoned");
        map.entry((a.to_string(), b.to_string()))
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    fn render(&self, out: &mut String) {
        let map = self.inner.lock().expect("metrics map poisoned");
        out.push_str(&format!("# TYPE {} counter\n", self.name));
        let mut keys: Vec<&(String, String)> = map.keys().collect();
        keys.sort();
        for k in keys {
            let v = map[k].load(Ordering::Relaxed);
            out.push_str(&format!(
                "{}{{{}=\"{}\",{}=\"{}\"}} {}\n",
                self.name,
                self.keys.0,
                escape_label(&k.0),
                self.keys.1,
                escape_label(&k.1),
                v
            ));
        }
    }
}

/// Set of counters the runner increments. Construct one per agent and
/// share via `Arc::clone` between the runner and the HTTP server.
pub struct Metrics {
    agent: String,
    messages_received: LabeledCounter,
    actions_proposed: LabeledCounter,
    actions_allowed: LabeledCounter,
    actions_denied: LabeledCounter2,
    gate_verdict: LabeledCounter,
    tick_fires: LabeledCounter,
    steps: LabeledCounter,
}

impl Metrics {
    pub fn new(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            messages_received: LabeledCounter::new("soft_messages_received_total", "topic"),
            actions_proposed: LabeledCounter::new("soft_actions_proposed_total", "kind"),
            actions_allowed: LabeledCounter::new("soft_actions_allowed_total", "kind"),
            actions_denied: LabeledCounter2::new("soft_actions_denied_total", ("kind", "reason")),
            gate_verdict: LabeledCounter::new("soft_gate_verdict_total", "verdict"),
            tick_fires: LabeledCounter::new("soft_tick_fires_total", "topic"),
            steps: LabeledCounter::new("soft_steps_total", "outcome"),
        }
    }

    pub fn inc_message(&self, topic: &str) {
        self.messages_received.inc(topic);
    }

    pub fn inc_action_proposed(&self, kind: &str) {
        self.actions_proposed.inc(kind);
    }

    pub fn inc_action_allowed(&self, kind: &str) {
        self.actions_allowed.inc(kind);
    }

    pub fn inc_action_denied(&self, kind: &str, reason: &str) {
        self.actions_denied.inc(kind, reason);
    }

    pub fn inc_gate_verdict(&self, verdict: &str) {
        self.gate_verdict.inc(verdict);
    }

    pub fn inc_tick(&self, topic: &str) {
        self.tick_fires.inc(topic);
    }

    pub fn inc_step(&self, outcome: &str) {
        self.steps.inc(outcome);
    }

    /// Render all metrics as Prometheus text exposition format.
    /// The agent name is emitted as a constant `# HELP` comment so
    /// scrapers can identify the source even without re-labeling.
    pub fn render_prometheus(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# HELP soft_agent {}\n",
            escape_label(&self.agent)
        ));
        self.messages_received.render(&mut out);
        self.actions_proposed.render(&mut out);
        self.actions_allowed.render(&mut out);
        self.actions_denied.render(&mut out);
        self.gate_verdict.render(&mut out);
        self.tick_fires.render(&mut out);
        self.steps.render(&mut out);
        out
    }

    pub fn agent_name(&self) -> &str {
        &self.agent
    }
}

fn escape_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_all_metric_types() {
        let m = Metrics::new("test-agent");
        m.inc_message("Tick");
        m.inc_action_proposed("send_a2a");
        m.inc_action_allowed("send_a2a");
        m.inc_action_denied("call_mcp", "gate");
        m.inc_gate_verdict("Allow");
        m.inc_tick("Tick");
        m.inc_step("Processed");

        let txt = m.render_prometheus();
        assert!(txt.contains("soft_messages_received_total{topic=\"Tick\"} 1"));
        assert!(txt.contains("soft_actions_proposed_total{kind=\"send_a2a\"} 1"));
        assert!(txt.contains("soft_actions_allowed_total{kind=\"send_a2a\"} 1"));
        assert!(txt.contains("soft_actions_denied_total{kind=\"call_mcp\",reason=\"gate\"} 1"));
        assert!(txt.contains("soft_gate_verdict_total{verdict=\"Allow\"} 1"));
        assert!(txt.contains("soft_tick_fires_total{topic=\"Tick\"} 1"));
        assert!(txt.contains("soft_steps_total{outcome=\"Processed\"} 1"));
        assert!(txt.contains("# HELP soft_agent test-agent"));
    }

    #[test]
    fn empty_metrics_still_emit_type_lines() {
        let m = Metrics::new("idle");
        let txt = m.render_prometheus();
        assert!(txt.contains("# TYPE soft_messages_received_total counter"));
        assert!(txt.contains("# TYPE soft_steps_total counter"));
    }

    #[test]
    fn labels_with_quotes_are_escaped() {
        let m = Metrics::new("a");
        m.inc_message("weird\"topic");
        let txt = m.render_prometheus();
        assert!(txt.contains("topic=\"weird\\\"topic\""));
    }
}
