//! Lightweight Prometheus-format counters and histograms for the
//! runner.
//!
//! All metrics are atomic, `Arc`-shared, and safe to read from a
//! different thread than the one running the runner — soft-a2a's HTTP
//! server reads them to render `/metrics` while the runner increments
//! them from its step loop.
//!
//! Two metric kinds: counters (`monotonic_total{label="..."}`) and
//! histograms (cumulative bucket counts + sum + count, in
//! `*_seconds`-suffixed Prometheus convention). Render via
//! [`Metrics::render_prometheus`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

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

/// Standard Prometheus latency buckets (seconds). Covers 1 ms through
/// 10 s exponentially — appropriate for agent step latencies (mostly
/// sub-100 ms) and outliers (LLM/MCP calls in the hundreds-of-ms to
/// multi-second range).
const DEFAULT_LATENCY_BUCKETS_SECONDS: &[f64] = &[
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Per-label histogram. Each labeled series carries: one `AtomicU64`
/// per bucket (cumulative count of observations ≤ that boundary), an
/// `AtomicU64` running sum (in microseconds — fits 5800 years), and
/// an `AtomicU64` total observation count.
struct HistogramSeries {
    bucket_counts: Vec<AtomicU64>,
    sum_micros: AtomicU64,
    count: AtomicU64,
}

impl HistogramSeries {
    fn new(num_buckets: usize) -> Self {
        let mut bucket_counts = Vec::with_capacity(num_buckets + 1);
        for _ in 0..=num_buckets {
            bucket_counts.push(AtomicU64::new(0));
        }
        Self {
            bucket_counts,
            sum_micros: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn observe(&self, seconds: f64, buckets: &[f64]) {
        // Increment every bucket whose upper bound is ≥ the observed
        // value. Prometheus histograms are cumulative — a single
        // observation is counted in its own bucket *and* every wider
        // bucket above it.
        for (i, &boundary) in buckets.iter().enumerate() {
            if seconds <= boundary {
                self.bucket_counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        // +Inf bucket (last entry) always increments.
        self.bucket_counts[buckets.len()].fetch_add(1, Ordering::Relaxed);

        let micros = (seconds * 1_000_000.0).round() as u64;
        self.sum_micros.fetch_add(micros, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

struct LabeledHistogram {
    name: &'static str,
    label_key: &'static str,
    buckets: &'static [f64],
    inner: Mutex<HashMap<String, HistogramSeries>>,
}

impl LabeledHistogram {
    fn new(name: &'static str, label_key: &'static str, buckets: &'static [f64]) -> Self {
        Self {
            name,
            label_key,
            buckets,
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn observe(&self, label: &str, duration: Duration) {
        let seconds = duration.as_secs_f64();
        let mut map = self.inner.lock().expect("metrics map poisoned");
        let series = map
            .entry(label.to_string())
            .or_insert_with(|| HistogramSeries::new(self.buckets.len()));
        series.observe(seconds, self.buckets);
    }

    fn render(&self, out: &mut String) {
        let map = self.inner.lock().expect("metrics map poisoned");
        out.push_str(&format!("# TYPE {} histogram\n", self.name));
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for label in keys {
            let series = &map[label];
            let escaped = escape_label(label);
            for (i, &boundary) in self.buckets.iter().enumerate() {
                let count = series.bucket_counts[i].load(Ordering::Relaxed);
                out.push_str(&format!(
                    "{}_bucket{{{}=\"{}\",le=\"{}\"}} {}\n",
                    self.name, self.label_key, escaped, boundary, count,
                ));
            }
            let inf_count = series.bucket_counts[self.buckets.len()].load(Ordering::Relaxed);
            out.push_str(&format!(
                "{}_bucket{{{}=\"{}\",le=\"+Inf\"}} {}\n",
                self.name, self.label_key, escaped, inf_count,
            ));
            let sum_seconds = series.sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            out.push_str(&format!(
                "{}_sum{{{}=\"{}\"}} {}\n",
                self.name, self.label_key, escaped, sum_seconds,
            ));
            let total = series.count.load(Ordering::Relaxed);
            out.push_str(&format!(
                "{}_count{{{}=\"{}\"}} {}\n",
                self.name, self.label_key, escaped, total,
            ));
        }
    }
}

/// Set of counters and histograms the runner records. Construct one
/// per agent and share via `Arc::clone` between the runner and the
/// HTTP server.
pub struct Metrics {
    agent: String,
    messages_received: LabeledCounter,
    actions_proposed: LabeledCounter,
    actions_allowed: LabeledCounter,
    actions_denied: LabeledCounter2,
    gate_verdict: LabeledCounter,
    tick_fires: LabeledCounter,
    steps: LabeledCounter,
    step_duration: LabeledHistogram,
    action_execute_duration: LabeledHistogram,
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
            step_duration: LabeledHistogram::new(
                "soft_step_duration_seconds",
                "outcome",
                DEFAULT_LATENCY_BUCKETS_SECONDS,
            ),
            action_execute_duration: LabeledHistogram::new(
                "soft_action_execute_duration_seconds",
                "kind",
                DEFAULT_LATENCY_BUCKETS_SECONDS,
            ),
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

    /// Record one full step's wall-clock duration, labeled by
    /// outcome (`Idle`, `Processed`, `Error`).
    pub fn observe_step_duration(&self, outcome: &str, duration: Duration) {
        self.step_duration.observe(outcome, duration);
    }

    /// Record one action-execution wall-clock duration, labeled by
    /// action kind. Captures only execution itself — not gate
    /// evaluation or trace bookkeeping.
    pub fn observe_action_execute_duration(&self, kind: &str, duration: Duration) {
        self.action_execute_duration.observe(kind, duration);
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
        self.step_duration.render(&mut out);
        self.action_execute_duration.render(&mut out);
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

    #[test]
    fn histogram_observation_lands_in_correct_buckets() {
        let m = Metrics::new("hist-test");
        // 7 ms — should land in 0.01, 0.025, 0.05, ... +Inf buckets.
        m.observe_step_duration("Processed", Duration::from_millis(7));
        let txt = m.render_prometheus();

        // 0.005 bucket excludes 7 ms; 0.01 and above include it.
        assert!(
            txt.contains("soft_step_duration_seconds_bucket{outcome=\"Processed\",le=\"0.005\"} 0")
        );
        assert!(
            txt.contains("soft_step_duration_seconds_bucket{outcome=\"Processed\",le=\"0.01\"} 1")
        );
        assert!(
            txt.contains("soft_step_duration_seconds_bucket{outcome=\"Processed\",le=\"+Inf\"} 1")
        );
        assert!(txt.contains("soft_step_duration_seconds_count{outcome=\"Processed\"} 1"));
        // Sum lands at 0.007 seconds.
        assert!(
            txt.contains("soft_step_duration_seconds_sum{outcome=\"Processed\"} 0.007"),
            "missing sum line in:\n{txt}"
        );
    }

    #[test]
    fn histogram_handles_multiple_observations_under_same_label() {
        let m = Metrics::new("hist-multi");
        m.observe_action_execute_duration("send_a2a", Duration::from_millis(2));
        m.observe_action_execute_duration("send_a2a", Duration::from_millis(20));
        m.observe_action_execute_duration("send_a2a", Duration::from_millis(2000));

        let txt = m.render_prometheus();
        // 2 ms ≤ 0.005; 20 ms exceeds 0.005 but ≤ 0.025; 2 s exceeds
        // every bucket up to 2.5 s.
        assert!(txt.contains(
            "soft_action_execute_duration_seconds_bucket{kind=\"send_a2a\",le=\"0.005\"} 1"
        ));
        assert!(txt.contains(
            "soft_action_execute_duration_seconds_bucket{kind=\"send_a2a\",le=\"0.025\"} 2"
        ));
        assert!(txt.contains(
            "soft_action_execute_duration_seconds_bucket{kind=\"send_a2a\",le=\"2.5\"} 3"
        ));
        assert!(txt.contains("soft_action_execute_duration_seconds_count{kind=\"send_a2a\"} 3"));
    }

    #[test]
    fn histogram_renders_type_line_when_empty() {
        let m = Metrics::new("hist-empty");
        let txt = m.render_prometheus();
        assert!(txt.contains("# TYPE soft_step_duration_seconds histogram"));
        assert!(txt.contains("# TYPE soft_action_execute_duration_seconds histogram"));
        // No bucket lines yet.
        assert!(!txt.contains("soft_step_duration_seconds_bucket"));
    }

    #[test]
    fn histogram_observations_are_independent_per_label() {
        let m = Metrics::new("hist-labels");
        m.observe_step_duration("Idle", Duration::from_millis(1));
        m.observe_step_duration("Processed", Duration::from_millis(50));
        let txt = m.render_prometheus();
        assert!(txt.contains("soft_step_duration_seconds_count{outcome=\"Idle\"} 1"));
        assert!(txt.contains("soft_step_duration_seconds_count{outcome=\"Processed\"} 1"));
    }
}
