//! `soft-replay` binary — load every trace in a `lex_store::Store` and
//! report the soft-agent invariant set.
//!
//! Exit code 0 on clean replay, 1 on any invariant violation.
//!
//! Usage: `soft-replay <store_root> [run_id ...]`

use std::env;
use std::process::ExitCode;

use lex_store::Store;
use soft_agent::replay::{scan_trace, Counts};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: soft-replay <store_root> [run_id ...]");
        return ExitCode::from(2);
    }
    let root = &args[1];

    let store = match Store::open(root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open store {root}: {e}");
            return ExitCode::from(2);
        }
    };

    let run_ids: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        match store.list_traces() {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("list traces: {e}");
                return ExitCode::from(2);
            }
        }
    };

    if run_ids.is_empty() {
        println!("no traces under {root}");
        return ExitCode::SUCCESS;
    }

    let mut total = Counts::default();
    let mut violations = 0usize;

    println!(
        "{:<16} {:>9} {:>8} {:>7} {:>13} {:>9} {:>8}",
        "run_id...", "proposed", "allowed", "denied", "inconclusive", "executed", "skipped",
    );

    for run_id in &run_ids {
        match store.load_trace(run_id) {
            Ok(tree) => {
                let counts = scan_trace(&tree);
                let row = format!(
                    "{:<16} {:>9} {:>8} {:>7} {:>13} {:>9} {:>8}",
                    short(run_id),
                    counts.proposed,
                    counts.allowed,
                    counts.denied,
                    counts.inconclusive,
                    counts.executed,
                    counts.skipped,
                );
                if counts.has_violation() {
                    violations += 1;
                    println!("{row}  ← VIOLATION: executed > allowed");
                } else {
                    println!("{row}");
                }
                total.add(&counts);
            }
            Err(e) => {
                eprintln!("load_trace {run_id}: {e}");
                violations += 1;
            }
        }
    }

    println!(
        "{:<16} {:>9} {:>8} {:>7} {:>13} {:>9} {:>8}",
        "(total)",
        total.proposed,
        total.allowed,
        total.denied,
        total.inconclusive,
        total.executed,
        total.skipped,
    );

    if violations == 0 {
        println!("\n{} trace(s) replayed; no violations.", run_ids.len());
        ExitCode::SUCCESS
    } else {
        println!("\n{violations} violation(s) found.");
        ExitCode::from(1)
    }
}

fn short(s: &str) -> &str {
    let n = s.len().min(16);
    &s[..n]
}
