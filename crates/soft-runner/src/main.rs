//! `soft-run` — single-process CLI for one soft-agent.
//!
//! Loads an agent definition from a `.lex` file, binds an A2A HTTP
//! listener on a port, configures an `A2aRoutedExecutor` over the
//! supplied peer URL map, optionally registers ticks, and runs the
//! agent's mailbox loop until SIGTERM or SIGINT.
//!
//! Usage:
//!
//! ```text
//! soft-run <agent.lex> --port <N>
//!     [--peer <name>=<url>]...           e.g. --peer depot=http://localhost:8002
//!     [--tick <topic>=<duration>]...     e.g. --tick Tick=2s    (units: ms|s|m)
//!     [--state-json <json>]              initial state for the agent
//!     [--bind <addr>]                    default 127.0.0.1
//!     [--store <path>]                   if set, finalize the trace here on shutdown
//!     [--llm-cloud-provider <name>]      `default` (OpenAI-shape, the lex-runtime
//!                                        builtin) or `anthropic` (custom EffectHandler)
//! ```
//!
//! Anthropic provider env vars: `ANTHROPIC_API_KEY` (required), plus
//! optional `ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL`,
//! `ANTHROPIC_MAX_TOKENS`, `ANTHROPIC_VERSION`. See
//! [`crate::anthropic`] for the full list.

mod anthropic;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use lex_runtime::Policy;
use serde_json::Value;
use soft_a2a::{A2aRoutedExecutor, A2aServer, AgentCard};
use soft_agent::{
    record_bindings, Gate, LexHost, Mailbox, Metrics, Runner, StepReport, DSL_PREAMBLE,
};

use crate::anthropic::AnthropicCloudHandler;

#[derive(Clone, Copy, Debug)]
enum LlmCloudProvider {
    Default,
    Anthropic,
}

struct Args {
    agent_file: String,
    port: u16,
    bind: String,
    peers: HashMap<String, String>,
    initial_state: Value,
    ticks: Vec<(String, Duration)>,
    store: Option<PathBuf>,
    llm_cloud_provider: LlmCloudProvider,
    shutdown_token: Option<String>,
    budget: Option<u64>,
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("error: {msg}\n");
    eprintln!(
        "usage: soft-run <agent.lex> --port <N> \\\n  \
            [--peer <name>=<url>]... \\\n  \
            [--tick <topic>=<duration>]...    (e.g. --tick Tick=2s) \\\n  \
            [--state-json <json>] \\\n  \
            [--bind <addr>]                   (default 127.0.0.1) \\\n  \
            [--store <path>]                  (persist trace on shutdown) \\\n  \
            [--llm-cloud-provider <name>]     (default | anthropic) \\\n  \
            [--budget <N>]                    (lex 0.3 [budget(N)] ceiling; \
no flag = unbounded) \\\n  \
            [--shutdown-token <T>]            (or SOFT_SHUTDOWN_TOKEN env; \
required when --bind is non-loopback)"
    );
    std::process::exit(2);
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let split_at = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("duration `{s}` must end with unit (ms|s|m)"))?;
    let (num_str, unit) = s.split_at(split_at);
    let n: u64 = num_str
        .parse()
        .map_err(|e| format!("duration `{s}`: bad number: {e}"))?;
    match unit {
        "ms" => Ok(Duration::from_millis(n)),
        "s" => Ok(Duration::from_secs(n)),
        "m" => Ok(Duration::from_secs(n * 60)),
        u => Err(format!(
            "duration `{s}`: unknown unit `{u}` (expected ms|s|m)"
        )),
    }
}

fn parse_args() -> Args {
    let mut iter = std::env::args().skip(1);
    let agent_file = iter
        .next()
        .unwrap_or_else(|| usage_exit("missing <agent.lex>"));
    let mut port: Option<u16> = None;
    let mut bind = "127.0.0.1".to_string();
    let mut peers = HashMap::new();
    let mut initial_state = serde_json::json!({});
    let mut ticks = Vec::new();
    let mut store = None;
    let mut llm_cloud_provider = LlmCloudProvider::Default;
    let mut shutdown_token: Option<String> = std::env::var("SOFT_SHUTDOWN_TOKEN").ok();
    let mut budget: Option<u64> = std::env::var("SOFT_BUDGET")
        .ok()
        .and_then(|s| s.parse().ok());

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => {
                let v = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--port needs a value"));
                port = Some(
                    v.parse()
                        .unwrap_or_else(|e| usage_exit(&format!("--port: {e}"))),
                );
            }
            "--bind" => {
                bind = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--bind needs an address"));
            }
            "--peer" => {
                let kv = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--peer needs name=url"));
                let (name, url) = kv
                    .split_once('=')
                    .unwrap_or_else(|| usage_exit("--peer must be name=url"));
                peers.insert(name.to_string(), url.to_string());
            }
            "--state-json" => {
                let s = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--state-json needs a value"));
                initial_state = serde_json::from_str(&s)
                    .unwrap_or_else(|e| usage_exit(&format!("--state-json: {e}")));
            }
            "--tick" => {
                let kv = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--tick needs topic=duration"));
                let (topic, dur_str) = kv
                    .split_once('=')
                    .unwrap_or_else(|| usage_exit("--tick must be topic=duration"));
                let duration = parse_duration(dur_str).unwrap_or_else(|e| usage_exit(&e));
                ticks.push((topic.to_string(), duration));
            }
            "--store" => {
                let p = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--store needs a path"));
                store = Some(PathBuf::from(p));
            }
            "--shutdown-token" => {
                let v = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--shutdown-token needs a value"));
                shutdown_token = Some(v);
            }
            "--llm-cloud-provider" => {
                let v = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--llm-cloud-provider needs a value"));
                llm_cloud_provider = match v.as_str() {
                    "default" => LlmCloudProvider::Default,
                    "anthropic" => LlmCloudProvider::Anthropic,
                    other => usage_exit(&format!(
                        "--llm-cloud-provider: unknown `{other}` (expected default | anthropic)"
                    )),
                };
            }
            "--budget" => {
                let v = iter
                    .next()
                    .unwrap_or_else(|| usage_exit("--budget needs a u64 ceiling"));
                budget = Some(
                    v.parse()
                        .unwrap_or_else(|e| usage_exit(&format!("--budget: {e}"))),
                );
            }
            other => usage_exit(&format!("unknown flag: `{other}`")),
        }
    }

    let port = port.unwrap_or_else(|| usage_exit("--port is required"));
    if !is_loopback_bind(&bind) && shutdown_token.is_none() {
        usage_exit(
            "--bind is non-loopback; --shutdown-token (or SOFT_SHUTDOWN_TOKEN env) is required \
to protect POST /shutdown. Use --bind 127.0.0.1 for local-only, or supply a token.",
        );
    }
    Args {
        agent_file,
        port,
        bind,
        peers,
        initial_state,
        ticks,
        store,
        llm_cloud_provider,
        budget,
        shutdown_token,
    }
}

fn is_loopback_bind(addr: &str) -> bool {
    use std::net::IpAddr;
    addr.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let args = parse_args();

    let user_src = match std::fs::read_to_string(&args.agent_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {e}", args.agent_file);
            return ExitCode::from(2);
        }
    };

    // Build the LexHost manually so we can install a custom handler
    // factory before Runner::from_lex_host consumes it. The DSL preamble
    // is prepended exactly as Runner::from_lex_source would do.
    let combined = format!("{}\n{}", DSL_PREAMBLE, user_src);
    let host = match LexHost::from_source(&combined) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("compile {}: {e}", args.agent_file);
            return ExitCode::from(1);
        }
    };

    let policy = make_policy(args.budget);
    let host = match args.llm_cloud_provider {
        LlmCloudProvider::Default => host.with_policy(policy.clone()),
        LlmCloudProvider::Anthropic => match install_anthropic_handler(host, policy.clone()) {
            Ok(h) => h,
            Err(msg) => {
                eprintln!("--llm-cloud-provider anthropic: {msg}");
                return ExitCode::from(1);
            }
        },
    };

    let mut builder = match Runner::from_lex_host(host) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("build runner: {e}");
            return ExitCode::from(1);
        }
    };
    let agent_name = builder
        .agent_name()
        .expect("from_lex_host sets agent")
        .to_string();

    // Optional: build a spec gate from `agent_specs([...])` paths in
    // the lex source. Paths are resolved relative to the agent.lex
    // file's directory so users write `agent_specs(["depot.spec"])`
    // rather than absolute paths.
    let spec_paths: Vec<String> = builder.spec_paths().to_vec();
    let agent_dir: PathBuf = match PathBuf::from(&args.agent_file).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if !spec_paths.is_empty() {
        let mut sources: Vec<String> = Vec::with_capacity(spec_paths.len());
        for rel in &spec_paths {
            let full = agent_dir.join(rel);
            match std::fs::read_to_string(&full) {
                Ok(s) => sources.push(s),
                Err(e) => {
                    eprintln!("read spec {}: {e}", full.display());
                    return ExitCode::from(2);
                }
            }
        }
        let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
        let gate = match Gate::from_sources(&src_refs, "fn _host() -> Int { 0 }") {
            Ok(g) => g,
            Err(e) => {
                eprintln!("compile gate for `{agent_name}`: {e}");
                return ExitCode::from(1);
            }
        };
        eprintln!(
            "[{agent_name}] gate active: {} spec(s) from {}",
            gate.spec_count(),
            spec_paths.join(", ")
        );
        builder = builder.gate(gate).bindings_fn(Box::new(record_bindings));
    }

    let metrics = Arc::new(Metrics::new(&agent_name));
    let (mailbox, sender) = Mailbox::new();
    builder = builder
        .mailbox(mailbox)
        .state(args.initial_state)
        .metrics(Arc::clone(&metrics))
        .executor(Box::new(A2aRoutedExecutor::new(
            &agent_name,
            args.peers.clone(),
        )));
    for (topic, duration) in &args.ticks {
        builder = builder.tick(*duration, topic.clone(), sender.clone());
    }
    let mut runner = match builder.build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("build runner for `{agent_name}`: {e}");
            return ExitCode::from(1);
        }
    };

    let listen = format!("{}:{}", args.bind, args.port);
    let card = AgentCard::new(&agent_name, "soft-run agent", format!("http://{listen}/"));
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        // Best-effort SIGINT/SIGTERM handler. In some sandboxed environments
        // signals are intercepted by a parent process before our handler can
        // run; in that case the `POST /shutdown` endpoint below is the
        // reliable fallback (Docker preStop hooks, Procfile teardown, etc.).
        let s = Arc::clone(&shutdown);
        let result = ctrlc::set_handler(move || s.store(true, Ordering::SeqCst));
        match result {
            Ok(()) => eprintln!("[{agent_name}] signal handler installed"),
            Err(e) => eprintln!("[{agent_name}] WARN: ctrlc handler not installed: {e}"),
        }
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }

    let server = match A2aServer::bind(&listen, card, sender) {
        Ok(s) => {
            let mut s = s
                .with_shutdown_flag(Arc::clone(&shutdown))
                .with_metrics(Arc::clone(&metrics));
            if let Some(tok) = args.shutdown_token.clone() {
                s = s.with_shutdown_token(tok);
            }
            s
        }
        Err(e) => {
            eprintln!("bind {listen}: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "soft-run: agent `{agent_name}` on http://{listen}  peers={}{}{}{}{}",
        format_peers(&args.peers),
        format_ticks(&args.ticks),
        match args.store {
            Some(ref p) => format!("  store={}", p.display()),
            None => String::new(),
        },
        match args.llm_cloud_provider {
            LlmCloudProvider::Default => "",
            LlmCloudProvider::Anthropic => "  llm_cloud=anthropic",
        },
        match args.budget {
            Some(n) => format!("  budget={n}"),
            None => String::new(),
        },
    );
    let _server_handle = server.spawn();

    use std::io::Write;
    let _ = std::io::stderr().flush();

    // Run until the shutdown flag flips. The flag is wired to two
    // sources: (a) the ctrlc handler installed earlier (catches SIGINT
    // in real terminals; SIGTERM where the process inherits a default
    // disposition), and (b) the `POST /shutdown` route on the A2A
    // server (the reliable orchestrator-driven path — works in
    // sandboxes and Docker preStop hooks).
    while !shutdown.load(Ordering::SeqCst) {
        match runner.step() {
            Ok(StepReport::Idle) => std::thread::sleep(Duration::from_millis(20)),
            Ok(StepReport::Processed { allowed, denied }) => {
                eprintln!("[{agent_name}] step: allowed={allowed} denied={denied}");
                let _ = std::io::stderr().flush();
            }
            Err(e) => {
                eprintln!("[{agent_name}] step error: {e}");
                let _ = std::io::stderr().flush();
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    // Graceful shutdown: persist trace if --store was given.
    eprintln!("[{agent_name}] shutdown signal received; finalizing trace");
    let _ = std::io::stderr().flush();
    if let Some(store_path) = args.store {
        match lex_store::Store::open(&store_path) {
            Ok(store) => match runner.finalize(&store) {
                Ok(run_id) => eprintln!(
                    "[{agent_name}] trace saved: {}/traces/{}/trace.json",
                    store_path.display(),
                    run_id.0
                ),
                Err(e) => eprintln!("[{agent_name}] finalize: {e}"),
            },
            Err(e) => eprintln!("[{agent_name}] open store {}: {e}", store_path.display()),
        }
        let _ = std::io::stderr().flush();
    }

    ExitCode::SUCCESS
}

fn install_anthropic_handler(host: LexHost, policy: Policy) -> Result<LexHost, String> {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return Err("ANTHROPIC_API_KEY env var not set".into());
    }
    Ok(host.with_handler_factory(move || {
        // `from_env` re-reads env per call; cheap, and gives users
        // hot-reload of model / max-tokens between calls.
        Box::new(
            AnthropicCloudHandler::from_env(policy.clone())
                .expect("ANTHROPIC_API_KEY checked at startup"),
        )
    }))
}

/// Build the lex Policy applied to every handler. Today only the
/// `[budget(N)]` ceiling is configurable here; future per-call
/// policy bits (effect allowlist, time limits) will land alongside.
fn make_policy(budget: Option<u64>) -> Policy {
    let mut p = Policy::permissive();
    p.budget = budget;
    p
}

fn format_peers(peers: &HashMap<String, String>) -> String {
    if peers.is_empty() {
        return "{}".into();
    }
    let mut parts: Vec<String> = peers.iter().map(|(k, v)| format!("{k}=>{v}")).collect();
    parts.sort();
    format!("{{{}}}", parts.join(", "))
}

fn format_ticks(ticks: &[(String, Duration)]) -> String {
    if ticks.is_empty() {
        return "".into();
    }
    let parts: Vec<String> = ticks.iter().map(|(t, d)| format!("{t}@{:?}", d)).collect();
    format!("  ticks=[{}]", parts.join(", "))
}
