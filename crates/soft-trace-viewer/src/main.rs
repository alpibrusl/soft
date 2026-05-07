//! Tiny HTTP viewer for lex-store traces.
//!
//! Serves three things:
//!   - `GET /`              → embedded `index.html` (vanilla JS, no deps)
//!   - `GET /api/traces`    → JSON array of run IDs in the store
//!   - `GET /api/trace/<id>`→ the full `TraceTree` as JSON
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
        _ => respond_text(req, "not found", 404),
    }
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
