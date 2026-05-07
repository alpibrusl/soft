//! `/metrics` and `/healthz` route coverage. The server reads the same
//! `Arc<Metrics>` the runner increments, so a cross-thread render is
//! safe — this test exercises that path.

use std::sync::Arc;
use std::time::Duration;

use soft_a2a::{A2aServer, AgentCard};
use soft_agent::{Mailbox, Metrics};

fn boot(metrics: Option<Arc<Metrics>>) -> String {
    let card = AgentCard::new("test", "metrics-route", "http://127.0.0.1/");
    let (mailbox, sender) = Mailbox::new();
    let _ = mailbox;
    let mut server = A2aServer::bind("127.0.0.1:0", card, sender).expect("bind ephemeral");
    if let Some(m) = metrics {
        server = server.with_metrics(m);
    }
    let addr = server.local_addr().expect("local_addr");
    let _ = server.spawn();
    addr
}

fn get(addr: &str, path: &str) -> (u16, String, Option<String>) {
    let url = format!("http://{addr}{path}");
    match ureq::get(&url).timeout(Duration::from_secs(2)).call() {
        Ok(r) => {
            let ct = r.header("Content-Type").map(|s| s.to_string());
            let status = r.status();
            let body = r.into_string().unwrap_or_default();
            (status, body, ct)
        }
        Err(ureq::Error::Status(code, r)) => {
            let ct = r.header("Content-Type").map(|s| s.to_string());
            let body = r.into_string().unwrap_or_default();
            (code, body, ct)
        }
        Err(e) => panic!("transport: {e}"),
    }
}

#[test]
fn healthz_returns_ok_json() {
    let addr = boot(None);
    let (status, body, _) = get(&addr, "/healthz");
    assert_eq!(status, 200);
    assert!(body.contains("\"ok\":true"));
}

#[test]
fn metrics_returns_404_when_not_configured() {
    let addr = boot(None);
    let (status, _, _) = get(&addr, "/metrics");
    assert_eq!(status, 404);
}

#[test]
fn metrics_returns_prometheus_text_when_wired() {
    let m = Arc::new(Metrics::new("test-agent"));
    m.inc_message("Tick");
    m.inc_action_proposed("send_a2a");
    m.inc_action_allowed("send_a2a");

    let addr = boot(Some(Arc::clone(&m)));
    let (status, body, ct) = get(&addr, "/metrics");
    assert_eq!(status, 200);
    assert!(
        ct.as_deref()
            .map(|c| c.starts_with("text/plain"))
            .unwrap_or(false),
        "expected prometheus content-type, got {ct:?}"
    );
    assert!(body.contains("soft_messages_received_total{topic=\"Tick\"} 1"));
    assert!(body.contains("soft_actions_proposed_total{kind=\"send_a2a\"} 1"));
    assert!(body.contains("soft_actions_allowed_total{kind=\"send_a2a\"} 1"));
}

#[test]
fn metrics_reflects_concurrent_increments() {
    let m = Arc::new(Metrics::new("concur"));
    let addr = boot(Some(Arc::clone(&m)));

    // Increment from another thread while server is up; render must
    // see it.
    let m2 = Arc::clone(&m);
    let h = std::thread::spawn(move || {
        for _ in 0..50 {
            m2.inc_message("X");
        }
    });
    h.join().unwrap();

    let (_, body, _) = get(&addr, "/metrics");
    assert!(body.contains("soft_messages_received_total{topic=\"X\"} 50"));
}
