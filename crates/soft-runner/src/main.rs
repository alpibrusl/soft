//! `soft-run` — single-process CLI for one soft-agent.
//!
//! Loads an agent definition from a `.lex` file, binds an A2A HTTP
//! listener on a port, configures an `A2aRoutedExecutor` over the
//! supplied peer URL map, optionally registers ticks, and runs the
//! agent's mailbox loop forever.
//!
//! Usage:
//!
//! ```text
//! soft-run <agent.lex> --port <N>
//!     [--peer <name>=<url>]...      e.g. --peer depot=http://localhost:8002
//!     [--tick <topic>=<duration>]...   e.g. --tick Tick=2s
//!     [--state-json <json>]            e.g. --state-json '{"soc":0.85}'
//!     [--bind <addr>]                  default 127.0.0.1
//! ```

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use soft_a2a::{A2aRoutedExecutor, A2aServer, AgentCard};
use soft_agent::{Mailbox, Runner, StepReport};

struct Args {
    agent_file: String,
    port: u16,
    bind: String,
    peers: HashMap<String, String>,
    initial_state: Value,
    ticks: Vec<(String, Duration)>,
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("error: {msg}\n");
    eprintln!(
        "usage: soft-run <agent.lex> --port <N> \\\n  \
            [--peer <name>=<url>]... \\\n  \
            [--tick <topic>=<duration>]...   (e.g. --tick Tick=2s) \\\n  \
            [--state-json <json>] \\\n  \
            [--bind <addr>]                  (default 127.0.0.1)"
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
    }
}

fn main() -> ExitCode {
    let args = parse_args();

    let source = match std::fs::read_to_string(&args.agent_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {e}", args.agent_file);
            return ExitCode::from(2);
        }
    };

    let (mailbox, sender) = Mailbox::new();

    // Build a partial RunnerBuilder so we can read the agent's name out
    // of it, then plug in the executor + ticks.
    let mut builder = match Runner::from_lex_source(&source) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("compile {}: {e}", args.agent_file);
            return ExitCode::from(1);
        }
    };
    let agent_name = builder
        .agent_name()
        .expect("from_lex_source sets agent")
        .to_string();
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
    let server = match A2aServer::bind(&listen, card, sender) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bind {listen}: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "soft-run: agent `{agent_name}` on http://{listen}  peers={}{}",
        format_peers(&args.peers),
        format_ticks(&args.ticks),
    );
    let _server_handle = server.spawn();

    // Run the mailbox loop forever. Sleep briefly when idle to avoid a
    // busy-wait; the tick threads will wake the mailbox themselves.
    loop {
        match runner.step() {
            Ok(StepReport::Idle) => std::thread::sleep(Duration::from_millis(20)),
            Ok(StepReport::Processed { allowed, denied }) => {
                eprintln!("[{agent_name}] step: allowed={allowed} denied={denied}");
            }
            Err(e) => {
                eprintln!("[{agent_name}] step error: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
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
