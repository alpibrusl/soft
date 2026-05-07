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
