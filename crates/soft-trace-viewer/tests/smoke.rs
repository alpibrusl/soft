//! Spawn the binary against a temp store and verify each route
//! returns a sensible response. Uses `CARGO_BIN_EXE_soft-trace-viewer`
//! which cargo populates for integration tests.

use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use tempfile::tempdir;

const BIN: &str = env!("CARGO_BIN_EXE_soft-trace-viewer");

fn pick_ephemeral_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct ViewerProc {
    child: Child,
}

impl Drop for ViewerProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(store_path: &str, port: u16) -> ViewerProc {
    let child = Command::new(BIN)
        .args([
            "--store",
            store_path,
            "--bind",
            &format!("127.0.0.1:{port}"),
        ])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn soft-trace-viewer");
    let proc = ViewerProc { child };
    // Poll until the server accepts a connection.
    let deadline = Instant::now() + Duration::from_secs(5);
    let url = format!("http://127.0.0.1:{port}/api/traces");
    while Instant::now() < deadline {
        if ureq::get(&url)
            .timeout(Duration::from_millis(200))
            .call()
            .is_ok()
        {
            return proc;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("viewer did not start within 5s");
}

#[test]
fn index_html_is_served() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let resp = ureq::get(&format!("http://127.0.0.1:{port}/"))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET /");
    assert_eq!(resp.status(), 200);
    let body = resp.into_string().unwrap();
    assert!(body.contains("soft trace viewer"));
    assert!(body.contains("/api/traces"));
}

#[test]
fn empty_store_returns_empty_traces_array() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let resp = ureq::get(&format!("http://127.0.0.1:{port}/api/traces"))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET /api/traces");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(body["traces"], serde_json::json!([]));
}

#[test]
fn missing_trace_id_returns_404() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let url = format!("http://127.0.0.1:{port}/api/trace/no-such-id");
    let status = match ureq::get(&url).timeout(Duration::from_secs(2)).call() {
        Ok(r) => r.status(),
        Err(ureq::Error::Status(c, _)) => c,
        Err(e) => panic!("transport: {e}"),
    };
    assert_eq!(status, 404);
}

#[test]
fn unknown_path_returns_404() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let url = format!("http://127.0.0.1:{port}/totally-unknown");
    let status = match ureq::get(&url).timeout(Duration::from_secs(2)).call() {
        Ok(r) => r.status(),
        Err(ureq::Error::Status(c, _)) => c,
        Err(e) => panic!("transport: {e}"),
    };
    assert_eq!(status, 404);
}

#[test]
fn search_with_no_q_returns_empty() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let url = format!("http://127.0.0.1:{port}/api/search");
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET /api/search");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(body["results"], serde_json::json!([]));
    assert_eq!(body["query"], "");
}

#[test]
fn search_empty_store_returns_zero_results() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    let url = format!("http://127.0.0.1:{port}/api/search?q=anything");
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET /api/search?q=…");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(body["results"], serde_json::json!([]));
    assert_eq!(body["scanned"], 0);
    assert_eq!(body["query"], "anything");
}

#[test]
fn search_url_decodes_query() {
    let dir = tempdir().unwrap();
    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    // %20 → space, + → space.
    let url = format!("http://127.0.0.1:{port}/api/search?q=hello%20world");
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(body["query"], "hello world");

    let url = format!("http://127.0.0.1:{port}/api/search?q=hello+world");
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET");
    let body: serde_json::Value = resp.into_json().unwrap();
    assert_eq!(body["query"], "hello world");
}

#[test]
fn search_finds_matching_trace_via_real_runner() {
    // End-to-end: drive a real soft-agent runner, persist a trace,
    // then start the viewer against the same store and verify search
    // finds a known token from the agent's outbound message.
    use serde_json::json;
    use soft_agent::{A2aMessage, Action, AgentConfig, Effect, Mailbox, Runner};

    let dir = tempdir().unwrap();
    let store = lex_store::Store::open(dir.path()).unwrap();

    let agent = AgentConfig::new("vehicle")
        .peers(["depot"])
        .effects([Effect::A2a])
        .build()
        .unwrap();
    let (mailbox, sender) = Mailbox::new();
    let mut runner = Runner::builder()
        .agent(agent)
        .state(json!({}))
        .mailbox(mailbox)
        .handle("Dispatch", |_state, msg| {
            vec![Action::SendA2a {
                peer: msg.from.clone(),
                topic: "Acknowledge".into(),
                payload: json!({"unique_token_searchable": true}),
            }]
        })
        .build()
        .unwrap();

    sender
        .send(A2aMessage {
            from: "tester".into(),
            topic: "Dispatch".into(),
            payload: json!({}),
        })
        .unwrap();
    runner.drain().unwrap();
    runner.finalize(&store).unwrap();

    let port = pick_ephemeral_port();
    let _proc = spawn(dir.path().to_str().unwrap(), port);

    // Search for the token we embedded in the SendA2a payload.
    let url = format!("http://127.0.0.1:{port}/api/search?q=unique_token_searchable");
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("GET search");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.into_json().unwrap();
    let results = body["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        1,
        "expected exactly one trace to match, got: {body}"
    );
    let snippet = results[0]["snippet"].as_str().unwrap();
    assert!(
        snippet.contains("unique_token_searchable"),
        "snippet should include the matched substring; got: {snippet}"
    );

    // Negative case: an unrelated query returns no results.
    let url = format!("http://127.0.0.1:{port}/api/search?q=definitely_not_in_any_trace_zzz");
    let body: serde_json::Value = ureq::get(&url)
        .timeout(Duration::from_secs(2))
        .call()
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(body["results"], serde_json::json!([]));
    assert_eq!(body["scanned"], 1);
}
