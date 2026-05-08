//! Tiny HTTP viewer for lex-store traces.
//!
//! Serves four things:
//!   - `GET /`                  → embedded `index.html` (vanilla JS, no deps)
//!   - `GET /api/traces`        → JSON array of run IDs in the store
//!   - `GET /api/trace/<id>`    → the full `TraceTree` as JSON
//!   - `GET /api/search?q=...`  → list of trace ids whose contents
//!     contain the (case-insensitive) substring `q`, plus a one-line
//!     snippet from the first matching node.
//!
//! Designed to be a one-process companion to `soft-run` containers
//! sharing a `/traces` volume. No auth — assume the bind address is
//! either localhost or already inside an internal network.

use std::path::PathBuf;
use std::process::ExitCode;

use lex_store::Store;
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("index.html");

struct Args {
    store: PathBuf,
    bind: String,
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("error: {msg}\n");
    eprintln!("usage: soft-trace-viewer --store <path> [--bind <host:port>]");
    eprintln!("  --bind defaults to 127.0.0.1:8080");
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut iter = std::env::args().skip(1);
    let mut store: Option<PathBuf> = None;
    let mut bind = "127.0.0.1:8080".to_string();

    while let Some(arg) = iter.next() {
        // Accept both `--flag value` and `--flag=value`.
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (arg, None),
        };
        match flag.as_str() {
            "--store" => {
                let v = inline.unwrap_or_else(|| {
                    iter.next()
                        .unwrap_or_else(|| usage_exit("--store needs a path"))
                });
                store = Some(PathBuf::from(v));
            }
            "--bind" => {
                bind = inline.unwrap_or_else(|| {
                    iter.next()
                        .unwrap_or_else(|| usage_exit("--bind needs host:port"))
                });
            }
            "-h" | "--help" => usage_exit("help"),
            other => usage_exit(&format!("unknown flag: `{other}`")),
        }
    }

    Args {
        store: store.unwrap_or_else(|| usage_exit("--store is required")),
        bind,
    }
}

fn main() -> ExitCode {
    let args = parse_args();

    let store = match Store::open(&args.store) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open store {}: {e}", args.store.display());
            return ExitCode::from(2);
        }
    };

    let server = match Server::http(&args.bind) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bind {}: {e}", args.bind);
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "soft-trace-viewer: serving {} on http://{}",
        args.store.display(),
        args.bind
    );

    for req in server.incoming_requests() {
        handle(req, &store);
    }
    ExitCode::SUCCESS
}

fn handle(req: Request, store: &Store) {
    let method = req.method().clone();
    let url = req.url().to_string();
    match (method, url.as_str()) {
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            respond_html(req, INDEX_HTML, 200);
        }
        (Method::Get, "/api/traces") => match store.list_traces() {
            Ok(ids) => respond_json(req, &json!({"traces": ids}), 200),
            Err(e) => respond_text(req, &format!("list_traces: {e}"), 500),
        },
        (Method::Get, path) if path.starts_with("/api/trace/") => {
            let id = path.trim_start_matches("/api/trace/").to_string();
            if id.is_empty() || id.contains('/') {
                respond_text(req, "bad run_id", 400);
                return;
            }
            match store.load_trace(&id) {
                Ok(tree) => respond_json(req, &tree, 200),
                Err(e) => respond_text(req, &format!("load_trace {id}: {e}"), 404),
            }
        }
        (Method::Get, path) if path.starts_with("/api/search") => {
            handle_search(req, store, path);
        }
        _ => respond_text(req, "not found", 404),
    }
}

/// Naive case-insensitive substring search over each trace's
/// serialized JSON. For Phase 1 trace volumes (~1 KB per trace,
/// ~tens of traces) this is fast enough; if the store grows past a
/// few thousand traces this would benefit from `lex-search` (lex 0.3
/// #224) or a precomputed inverted index.
fn handle_search(req: Request, store: &Store, raw_path: &str) {
    let q = match raw_path
        .split_once('?')
        .and_then(|(_, qs)| query_param(qs, "q"))
    {
        Some(q) if !q.trim().is_empty() => q.to_lowercase(),
        _ => {
            respond_json(req, &json!({"results": [], "query": ""}), 200);
            return;
        }
    };

    let ids = match store.list_traces() {
        Ok(ids) => ids,
        Err(e) => {
            respond_text(req, &format!("list_traces: {e}"), 500);
            return;
        }
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    for id in &ids {
        let tree = match store.load_trace(id) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let body = match serde_json::to_string(&tree) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let lower = body.to_lowercase();
        if let Some(pos) = lower.find(&q) {
            // Build a ~80-char snippet centered on the match.
            let snippet_start = pos.saturating_sub(30);
            let snippet_end = (pos + q.len() + 50).min(body.len());
            // Snap to UTF-8 boundaries.
            let snippet_start = body
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= snippet_start)
                .last()
                .unwrap_or(0);
            let snippet_end = body
                .char_indices()
                .map(|(i, _)| i)
                .find(|&i| i >= snippet_end)
                .unwrap_or(body.len());
            let snippet = &body[snippet_start..snippet_end];
            results.push(json!({
                "run_id": id,
                "snippet": snippet,
                "match_offset": pos,
            }));
        }
    }

    respond_json(
        req,
        &json!({
            "query": q,
            "results": results,
            "scanned": ids.len(),
        }),
        200,
    );
}

/// Minimal `application/x-www-form-urlencoded` query-string parser.
/// Returns the first value for `key`, decoded for the simple
/// `+` → space and `%XX` → byte cases. Sufficient for our search
/// box; not a full URL parser.
fn query_param(qs: &str, key: &str) -> Option<String> {
    for pair in qs.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            other => {
                out.push(other as char);
                i += 1;
            }
        }
    }
    out
}

fn respond_html(req: Request, body: &str, status: u16) {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
        .expect("static content-type header is well-formed");
    let resp = Response::from_string(body)
        .with_header(header)
        .with_status_code(status);
    let _ = req.respond(resp);
}

fn respond_json<T: serde::Serialize>(req: Request, body: &T, status: u16) {
    let json_str = match serde_json::to_string(body) {
        Ok(s) => s,
        Err(e) => return respond_text(req, &format!("encode: {e}"), 500),
    };
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static content-type header is well-formed");
    let resp = Response::from_string(json_str)
        .with_header(header)
        .with_status_code(status);
    let _ = req.respond(resp);
}

fn respond_text(req: Request, body: &str, status: u16) {
    let resp = Response::from_string(body).with_status_code(status);
    let _ = req.respond(resp);
}
