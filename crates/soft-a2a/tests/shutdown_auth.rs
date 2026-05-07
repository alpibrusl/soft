//! Auth coverage for `POST /shutdown`. Default (no token) accepts any
//! request — the runner pairs this with a bind-address check to prevent
//! exposure on non-loopback interfaces. With a token, only requests
//! carrying a matching `X-Shutdown-Token` header may flip the flag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use soft_a2a::{A2aServer, AgentCard};
use soft_agent::Mailbox;

fn boot(token: Option<&str>) -> (String, Arc<AtomicBool>) {
    let card = AgentCard::new("test", "shutdown-auth", "http://127.0.0.1/");
    let (mailbox, sender) = Mailbox::new();
    let _ = mailbox;
    let flag = Arc::new(AtomicBool::new(false));
    let mut server = A2aServer::bind("127.0.0.1:0", card, sender)
        .expect("bind ephemeral")
        .with_shutdown_flag(Arc::clone(&flag));
    if let Some(t) = token {
        server = server.with_shutdown_token(t.into());
    }
    let addr = server.local_addr().expect("local_addr");
    let _ = server.spawn();
    (addr, flag)
}

fn post_shutdown(addr: &str, token: Option<&str>) -> u16 {
    let url = format!("http://{addr}/shutdown");
    let mut req = ureq::post(&url).timeout(Duration::from_secs(2));
    if let Some(t) = token {
        req = req.set("X-Shutdown-Token", t);
    }
    match req.send_string("") {
        Ok(r) => r.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(e) => panic!("transport: {e}"),
    }
}

#[test]
fn no_token_configured_accepts_any_request() {
    let (addr, flag) = boot(None);
    let status = post_shutdown(&addr, None);
    assert_eq!(status, 202);
    std::thread::sleep(Duration::from_millis(20));
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn token_configured_rejects_missing_header() {
    let (addr, flag) = boot(Some("secret"));
    let status = post_shutdown(&addr, None);
    assert_eq!(status, 401);
    assert!(!flag.load(Ordering::SeqCst));
}

#[test]
fn token_configured_rejects_wrong_header() {
    let (addr, flag) = boot(Some("secret"));
    let status = post_shutdown(&addr, Some("nope"));
    assert_eq!(status, 401);
    assert!(!flag.load(Ordering::SeqCst));
}

#[test]
fn token_configured_accepts_matching_header() {
    let (addr, flag) = boot(Some("secret"));
    let status = post_shutdown(&addr, Some("secret"));
    assert_eq!(status, 202);
    std::thread::sleep(Duration::from_millis(20));
    assert!(flag.load(Ordering::SeqCst));
}
