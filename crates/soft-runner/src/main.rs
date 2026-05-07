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
use soft_agent::{LexHost, Mailbox, Runner, StepReport, DSL_PREAMBLE};

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
            [--llm-cloud-provider <name>]     (default | anthropic)"
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
        u => Err(format!("duration `{s}`: unknown unit `{u}` (expected ms|s|m)")),
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

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--port" => {
                let v = iter.next().unwrap_or_else(|| usage_exit("--port needs a value"));
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
            other => usage_exit(&format!("unknown flag: `{other}`")),
        }
    }

    let port = port.unwrap_or_else(|| usage_exit("--port is required"));
    Args {
        agent_file,
        port,
        bind,
        peers,
        initial_state,
        ticks,
        store,
        llm_cloud_provider,
    }
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

    let host = match args.llm_cloud_provider {
        LlmCloudProvider::Default => host,
        LlmCloudProvider::Anthropic => match install_anthropic_handler(host) {
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

    let (mailbox, sender) = Mailbox::new();
    builder = builder
        .mailbox(mailbox)
        .state(args.initial_state)
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
    let card = AgentCard::new(
        &agent_name,
        "soft-run agent",
        format!("http://{listen}/"),
    );
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
        Ok(s) => s.with_shutdown_flag(Arc::clone(&shutdown)),
        Err(e) => {
            eprintln!("bind {listen}: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "soft-run: agent `{agent_name}` on http://{listen}  peers={}{}{}{}",
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

fn install_anthropic_handler(host: LexHost) -> Result<LexHost, String> {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return Err("ANTHROPIC_API_KEY env var not set".into());
    }
    Ok(host.with_handler_factory(|| {
        // `from_env` re-reads env per call; cheap, and gives users
        // hot-reload of model / max-tokens between calls.
        Box::new(
            AnthropicCloudHandler::from_env(Policy::permissive())
                .expect("ANTHROPIC_API_KEY checked at startup"),
        )
    }))
}

fn format_peers(peers: &HashMap<String, String>) -> String {
    if peers.is_empty() {
        return "{}".into();
    }
    let mut parts: Vec<String> = peers
        .iter()
        .map(|(k, v)| format!("{k}=>{v}"))
        .collect();
    parts.sort();
    format!("{{{}}}", parts.join(", "))
}

fn format_ticks(ticks: &[(String, Duration)]) -> String {
    if ticks.is_empty() {
        return "".into();
    }
    let parts: Vec<String> = ticks
        .iter()
        .map(|(t, d)| format!("{t}@{:?}", d))
        .collect();
    format!("  ticks=[{}]", parts.join(", "))
}
