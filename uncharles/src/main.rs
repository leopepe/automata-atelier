//! `uncharles` — a sense → plan → act loop driving `goap-planner` from a
//! YAML config. Named after the obsessive task-list-driven robot from
//! Adrian Tchaikovsky's *Service Model*.
//!
//! Default mode is one-shot: load the config, run sensors, plan, print the
//! plan, exit. Pass `--execute` to switch into the persistent loop that
//! actually runs each action's `cmd`, re-senses, and replans until the
//! goal is satisfied (or something goes wrong).

mod config;
mod run;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;

use config::Config;
use run::{LoopEvent, LoopOutcome, Outcome, RunError, run_loop, sense_and_plan};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Sense → plan → act loop driving goap-planner from a YAML config"
)]
struct Cli {
    /// Path to the YAML config describing sensors, actions, and the goal.
    #[arg(short, long)]
    config: PathBuf,

    /// Extra facts to seed into the initial state, before sensors run.
    #[arg(long, value_name = "FACT")]
    have: Vec<String>,

    /// Execute the plan: actually run each action's `cmd`, re-sense, replan,
    /// and continue until the goal is satisfied. Without this flag,
    /// uncharles only prints the plan and exits.
    #[arg(long)]
    execute: bool,

    /// Safety cap on loop iterations when `--execute` is set. Each iteration
    /// runs all sensors, plans, and executes one action.
    #[arg(long, default_value_t = 100)]
    max_iterations: usize,

    /// Minimum delay (milliseconds) between iterations of the execute loop.
    /// `0` (the default) runs as fast as work allows, which suits "drive to
    /// goal" configs. Set a non-zero value to pace "watch the world" configs
    /// where sensors poll an external API. The sleep is interruptible —
    /// SIGINT wakes it within ~50 ms regardless of the configured value.
    #[arg(long, default_value_t = 0, value_name = "MS")]
    interval_ms: u64,

    /// Human-readable output. Defaults to JSON (or NDJSON in execute mode).
    #[arg(long)]
    pretty: bool,

    /// Print the parsed config and exit without running sensors or planning.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let raw = match fs::read_to_string(&cli.config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", cli.config.display());
            return ExitCode::from(2);
        }
    };

    let config: Config = match serde_yaml::from_str(&raw) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: invalid config: {e}");
            return ExitCode::from(2);
        }
    };

    if cli.dry_run {
        emit_dry_run(&config, cli.pretty);
        return ExitCode::SUCCESS;
    }

    if cli.execute {
        return run_execute_mode(&cli, &config);
    }

    match sense_and_plan(&config, cli.have) {
        Ok(outcome) => {
            emit_outcome(&outcome, cli.pretty);
            match outcome.plan {
                Some(_) => ExitCode::SUCCESS,
                None => ExitCode::from(1),
            }
        }
        Err(e) => {
            emit_error(&e, cli.pretty);
            ExitCode::from(2)
        }
    }
}

fn run_execute_mode(cli: &Cli, config: &Config) -> ExitCode {
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&interrupted);
    if let Err(e) = ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::Relaxed);
    }) {
        eprintln!("error: failed to install signal handler: {e}");
        return ExitCode::from(2);
    }

    let pretty = cli.pretty;
    let result = run_loop(
        config,
        cli.have.clone(),
        cli.max_iterations,
        cli.interval_ms,
        Arc::clone(&interrupted),
        |event| emit_event(&event, pretty),
    );

    match result {
        Ok(outcome) => {
            emit_loop_outcome(&outcome, pretty);
            match outcome {
                LoopOutcome::GoalSatisfied { .. } => ExitCode::SUCCESS,
                LoopOutcome::NoPlan { .. }
                | LoopOutcome::ActionFailed { .. }
                | LoopOutcome::MaxIterationsReached { .. }
                | LoopOutcome::Interrupted { .. } => ExitCode::from(1),
            }
        }
        Err(e) => {
            emit_error(&e, pretty);
            ExitCode::from(2)
        }
    }
}

fn emit_dry_run(config: &Config, pretty: bool) {
    let v = serde_json::json!({
        "sensors": config.sensors.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "actions": config.actions.iter().map(|a| serde_json::json!({
            "name": a.name,
            "cost": a.cost,
            "requires": a.requires,
            "adds": a.adds,
            "removes": a.removes,
            "on_failure": a.on_failure.as_ref().map(|e| serde_json::json!({
                "add": e.add,
                "remove": e.remove,
            })),
        })).collect::<Vec<_>>(),
        "goal": {
            "requires": config.goal.requires,
            "forbids": config.goal.forbids,
        },
    });
    let s = if pretty {
        serde_json::to_string_pretty(&v).unwrap()
    } else {
        v.to_string()
    };
    println!("{s}");
}

fn emit_outcome(outcome: &Outcome, pretty: bool) {
    if pretty {
        println!("--- sensors ---");
        if outcome.readings.is_empty() {
            println!("(none)");
        }
        for r in &outcome.readings {
            let status = if r.success { "ok" } else { "fail" };
            println!(
                "{:>6}  {}  +{:?}  -{:?}",
                status, r.name, r.added, r.removed
            );
        }
        println!("\n--- state ---");
        for fact in &outcome.state_facts {
            println!("  {fact}");
        }
        println!("\n--- plan ---");
        match &outcome.plan {
            Some(plan) if plan.steps.is_empty() => {
                println!("(goal already satisfied)");
                println!("total cost: 0.0");
            }
            Some(plan) => {
                for (i, step) in plan.steps.iter().enumerate() {
                    println!("{:>2}. {step}", i + 1);
                }
                println!("total cost: {:.1}", plan.cost);
            }
            None => println!("no plan exists"),
        }
        return;
    }

    let v = serde_json::json!({
        "ok": true,
        "sensors": outcome.readings.iter().map(|r| serde_json::json!({
            "name": r.name,
            "success": r.success,
            "added": r.added,
            "removed": r.removed,
        })).collect::<Vec<_>>(),
        "state": outcome.state_facts,
        "plan": outcome.plan.as_ref().map(|p| serde_json::json!({
            "steps": p.steps,
            "cost": p.cost,
        })),
    });
    println!("{v}");
}

fn emit_event(event: &LoopEvent, pretty: bool) {
    if pretty {
        match event {
            LoopEvent::Sensed {
                iteration,
                readings,
                state,
            } => {
                let ok_count = readings.iter().filter(|r| r.success).count();
                println!(
                    "[{iteration:>3}] sense    {}/{} ok → state({}): {}",
                    ok_count,
                    readings.len(),
                    state.len(),
                    state.join(", "),
                );
            }
            LoopEvent::Planned { iteration, plan } => match plan {
                None => println!("[{iteration:>3}] plan     no plan exists"),
                Some(p) if p.steps.is_empty() => {
                    println!("[{iteration:>3}] plan     goal satisfied");
                }
                Some(p) => println!(
                    "[{iteration:>3}] plan     {} steps, cost {:.1}: {}",
                    p.steps.len(),
                    p.cost,
                    p.steps.join(" → "),
                ),
            },
            LoopEvent::Executed { iteration, result } => {
                let status = if result.success { "ok" } else { "FAIL" };
                println!("[{iteration:>3}] exec     {} ... {status}", result.name);
                if !result.success && !result.stderr.is_empty() {
                    for line in result.stderr.lines().take(5) {
                        println!("            stderr: {line}");
                    }
                }
            }
        }
        return;
    }

    let v = match event {
        LoopEvent::Sensed {
            iteration,
            readings,
            state,
        } => serde_json::json!({
            "event": "sensed",
            "iteration": iteration,
            "readings": readings.iter().map(|r| serde_json::json!({
                "name": r.name,
                "success": r.success,
                "added": r.added,
                "removed": r.removed,
            })).collect::<Vec<_>>(),
            "state": state,
        }),
        LoopEvent::Planned { iteration, plan } => serde_json::json!({
            "event": "planned",
            "iteration": iteration,
            "plan": plan.as_ref().map(|p| serde_json::json!({
                "steps": p.steps,
                "cost": p.cost,
            })),
        }),
        LoopEvent::Executed { iteration, result } => serde_json::json!({
            "event": "executed",
            "iteration": iteration,
            "result": {
                "name": result.name,
                "success": result.success,
                "exit_code": result.exit_code,
                "stderr": result.stderr,
            },
        }),
    };
    println!("{v}");
}

fn emit_loop_outcome(outcome: &LoopOutcome, pretty: bool) {
    if pretty {
        match outcome {
            LoopOutcome::GoalSatisfied { iteration } => {
                println!("[done]   goal satisfied after {iteration} iteration(s)");
            }
            LoopOutcome::NoPlan { iteration } => {
                println!("[stop]   no plan exists at iteration {iteration}");
            }
            LoopOutcome::ActionFailed {
                iteration,
                name,
                exit_code,
                stderr,
            } => {
                println!(
                    "[stop]   action `{name}` failed at iteration {iteration} (exit {})",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "?".into()),
                );
                if !stderr.is_empty() {
                    for line in stderr.lines().take(10) {
                        println!("         stderr: {line}");
                    }
                }
            }
            LoopOutcome::Interrupted { iteration } => {
                println!("[stop]   interrupted after {iteration} iteration(s)");
            }
            LoopOutcome::MaxIterationsReached { iteration, max } => {
                println!("[stop]   max iterations reached ({iteration}/{max})");
            }
        }
        return;
    }

    let v = match outcome {
        LoopOutcome::GoalSatisfied { iteration } => serde_json::json!({
            "event": "complete",
            "outcome": "goal_satisfied",
            "iterations": iteration,
        }),
        LoopOutcome::NoPlan { iteration } => serde_json::json!({
            "event": "complete",
            "outcome": "no_plan",
            "iterations": iteration,
        }),
        LoopOutcome::ActionFailed {
            iteration,
            name,
            exit_code,
            stderr,
        } => serde_json::json!({
            "event": "complete",
            "outcome": "action_failed",
            "iterations": iteration,
            "action": name,
            "exit_code": exit_code,
            "stderr": stderr,
        }),
        LoopOutcome::Interrupted { iteration } => serde_json::json!({
            "event": "complete",
            "outcome": "interrupted",
            "iterations": iteration,
        }),
        LoopOutcome::MaxIterationsReached { iteration, max } => serde_json::json!({
            "event": "complete",
            "outcome": "max_iterations_reached",
            "iterations": iteration,
            "max": max,
        }),
    };
    println!("{v}");
}

fn emit_error(err: &RunError, pretty: bool) {
    let msg = err.to_string();
    if pretty {
        eprintln!("error: {msg}");
    } else {
        let v = serde_json::json!({ "ok": false, "error": msg });
        println!("{v}");
    }
}
