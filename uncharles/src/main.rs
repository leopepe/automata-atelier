//! `uncharles` — a sense → plan → act loop driving `goap-planner` from a
//! YAML config. Named after the obsessive task-list-driven robot from
//! Adrian Tchaikovsky's *Service Model*.
//!
//! Subcommands:
//!
//! - `run` — load the config, run sensors, plan, and (with `--execute`)
//!   drive the actor-based reactive runtime (ADR 0005): sensors poll
//!   continuously and in parallel, world-state changes trigger replanning, and
//!   the executor runs the freshest plan. Without `--execute` it does a single
//!   sense → plan and prints the result. The default subcommand for backward
//!   compatibility — `uncharles --config X` is parsed as `uncharles run
//!   --config X`.
//! - `inspect` — load the config and print the static structure plus the
//!   bounded reachable state-action graph, *without* running sensors or
//!   actions. For visual debugging.

mod actors;
mod config;
mod inspect;
mod run;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use actors::{AgentRuntime, RuntimeEvent, RuntimeOutcome, RuntimeParams};
use config::Config;
use run::{Outcome, RunError, sense_and_plan};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Sense → plan → act loop driving goap-planner from a YAML config"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the sense → plan → act loop. Optionally executes actions
    /// (`--execute`) and replans on divergence.
    Run(RunArgs),
    /// Inspect a config: print the sensors, actions, goal, simulated initial
    /// state, the bounded reachable state-action graph, and a static-analysis
    /// section flagging orphan actions, unreachable goal facts, and dead-end
    /// states. Does not run any sensor or action commands.
    Inspect(InspectArgs),
}

#[derive(Args, Debug, Clone)]
struct RunArgs {
    /// Path to the YAML config describing sensors, actions, and the goal.
    #[arg(short, long)]
    config: PathBuf,

    /// Extra facts to seed into the initial state, before sensors run.
    #[arg(long, value_name = "FACT")]
    have: Vec<String>,

    /// Run the automaton (ADR 0005). This is a **perpetual** sense → plan →
    /// act → sense loop: sensors poll continuously and in parallel, a change to
    /// the world state triggers a replan, the executor drives the freshest plan
    /// to the goal, and once the goal is reached the automaton returns to
    /// sensing — waiting for the world to diverge again. It runs until SIGINT
    /// (or an unrecoverable action failure). Without this flag, uncharles does a
    /// single sense → plan and exits. See `--once` to drive to the goal and exit.
    #[arg(long)]
    execute: bool,

    /// Safety cap on the number of actions executed when `--execute` is set.
    #[arg(long, default_value_t = 100)]
    max_iterations: usize,

    /// Per-sensor poll cadence in milliseconds when `--execute` is set.
    /// `0` (the default) polls as fast as each shell-out allows. Set a non-zero
    /// value to pace sensors that hit an external API every cycle.
    #[arg(long, default_value_t = 0, value_name = "MS")]
    interval_ms: u64,

    /// One-shot mode: drive to the goal once and exit instead of the default
    /// perpetual loop. With `--once`, a satisfied goal exits 0 and an
    /// unreachable goal exits 1 (no-plan) — useful for CI and scripting where
    /// you want a definitive outcome rather than a long-lived automaton. Only
    /// meaningful with `--execute`.
    #[arg(long)]
    once: bool,

    /// Human-readable output. Defaults to JSON (or NDJSON in execute mode).
    #[arg(long)]
    pretty: bool,

    /// Print the parsed config and exit without running sensors or planning.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct InspectArgs {
    /// Path to the YAML config to inspect.
    #[arg(short, long)]
    config: PathBuf,

    /// Extra facts to layer on top of the simulated initial state. Useful
    /// for "what does the planner see if I assume fact X is also true?".
    #[arg(long, value_name = "FACT")]
    have: Vec<String>,

    /// Override the planner's `max_states` cap on state-space exploration.
    /// When the cap is hit, the printed graph is marked TRUNCATED.
    #[arg(long, value_name = "N")]
    max_states: Option<usize>,

    /// Output format. Default is human-readable text. `dot` and `mermaid`
    /// are graph-renderer-friendly; `json` is pipeable / structured.
    #[arg(long, value_enum, default_value_t = InspectFormat::Text)]
    format: InspectFormat,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum InspectFormat {
    /// Human-readable text report (default). Six labelled sections.
    Text,
    /// Graphviz DOT. Pipe to `dot -Tsvg`, `graph-easy --as=boxart`, or
    /// `dot -Tpng | chafa -` for terminal-friendly rendering.
    Dot,
    /// Mermaid `flowchart LR`. Paste into a Markdown viewer with Mermaid
    /// support, or <https://mermaid.live>.
    Mermaid,
    /// Pretty-printed JSON. Pipeable to `jq` and friends.
    Json,
}

fn main() -> ExitCode {
    // Backward compatibility: if the first arg is not a known subcommand or
    // a help/version flag, insert "run" implicitly so `uncharles --config X`
    // continues to work.
    let argv = bridge_legacy_argv(std::env::args().collect());
    let cli = Cli::parse_from(argv);

    match cli.command {
        Command::Run(args) => run_command(args),
        Command::Inspect(args) => inspect_command(&args),
    }
}

/// If the first argument looks like a flag (starts with `-`) rather than a
/// known subcommand, splice in `run` so the historical flat CLI keeps
/// working: `uncharles --config X` → `uncharles run --config X`.
fn bridge_legacy_argv(argv: Vec<String>) -> Vec<String> {
    if argv.len() < 2 {
        return argv;
    }
    let first = argv[1].as_str();
    let known = ["run", "inspect", "help", "-h", "--help", "-V", "--version"];
    if known.contains(&first) {
        return argv;
    }
    // Anything else (most importantly, anything starting with `-`) is treated
    // as legacy run-mode argv. Insert `run` in front of the first user arg.
    let mut bridged = Vec::with_capacity(argv.len() + 1);
    bridged.push(argv[0].clone());
    bridged.push("run".to_string());
    bridged.extend(argv.into_iter().skip(1));
    bridged
}

// ---------------------------------------------------------------------------
// `run` subcommand — the existing sense → plan → act behaviour
// ---------------------------------------------------------------------------

fn run_command(args: RunArgs) -> ExitCode {
    let raw = match fs::read_to_string(&args.config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", args.config.display());
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

    if let Err(e) = config.validate() {
        eprintln!("error: invalid config: {e}");
        return ExitCode::from(2);
    }

    if args.dry_run {
        emit_dry_run(&config, args.pretty);
        return ExitCode::SUCCESS;
    }

    if args.execute {
        return run_execute_mode(&args, &config);
    }

    match sense_and_plan(&config, args.have) {
        Ok(outcome) => {
            emit_outcome(&outcome, args.pretty);
            match outcome.plan {
                Some(_) => ExitCode::SUCCESS,
                None => ExitCode::from(1),
            }
        }
        Err(e) => {
            emit_error(&e, args.pretty);
            ExitCode::from(2)
        }
    }
}

fn run_execute_mode(args: &RunArgs, config: &Config) -> ExitCode {
    // The reactive runtime needs a multi-threaded runtime so sensors get real
    // OS-thread parallelism (ADR 0005). `enable_all` turns on the timer and
    // signal drivers the poll loop and graceful ctrl-c drain depend on.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to start async runtime: {e}");
            return ExitCode::from(2);
        }
    };

    let pretty = args.pretty;
    let params = RuntimeParams {
        seed: args.have.clone(),
        interval_ms: args.interval_ms,
        max_actions: args.max_iterations,
        // Perpetual by default; `--once` opts into drive-to-goal-and-exit.
        watch: !args.once,
    };

    let outcome = runtime.block_on(AgentRuntime::run(config, params, |event| {
        emit_runtime_event(event, pretty);
    }));

    match outcome {
        // A signal-driven drain is a clean service stop (systemd/Docker send
        // SIGTERM to stop a daemon), as is reaching the goal in `--once` mode.
        RuntimeOutcome::GoalSatisfied | RuntimeOutcome::Interrupted => ExitCode::SUCCESS,
        // `--once`-only terminals: the goal is unreachable or the cap was hit.
        RuntimeOutcome::NoPlan
        | RuntimeOutcome::ActionFailed { .. }
        | RuntimeOutcome::MaxActionsReached { .. } => ExitCode::from(1),
        RuntimeOutcome::Error { .. } => ExitCode::from(2),
    }
}

// ---------------------------------------------------------------------------
// `inspect` subcommand — issue #22
// ---------------------------------------------------------------------------

fn inspect_command(args: &InspectArgs) -> ExitCode {
    let raw = match fs::read_to_string(&args.config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", args.config.display());
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

    if let Err(e) = config.validate() {
        eprintln!("error: invalid config: {e}");
        return ExitCode::from(2);
    }

    let (initial, graph, analysis) = inspect::inspect(&config, &args.have, args.max_states);
    let report = match args.format {
        InspectFormat::Text => inspect::render_text(&config, &initial, &graph, &analysis),
        InspectFormat::Dot => inspect::render_dot(&config, &initial, &graph, &analysis),
        InspectFormat::Mermaid => inspect::render_mermaid(&config, &initial, &graph, &analysis),
        InspectFormat::Json => inspect::render_json(&config, &initial, &graph, &analysis),
    };
    print!("{report}");
    // Non-text formats often don't end with their own trailing newline;
    // make sure stdout looks tidy when piped.
    if !report.ends_with('\n') {
        println!();
    }

    if analysis.is_clean() {
        ExitCode::SUCCESS
    } else {
        // Static-analysis findings still produce a useful report; surface
        // them via a non-zero exit so `uncharles inspect` is usable as a
        // lint pass in scripts. Exit code 1 means "issues found", distinct
        // from 2 ("could not run").
        ExitCode::from(1)
    }
}

// ---------------------------------------------------------------------------
// Existing emitters (unchanged)
// ---------------------------------------------------------------------------

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
        if !outcome.values.is_empty() {
            println!("\n--- values ---");
            for (k, v) in &outcome.values {
                println!("  {k}={v}");
            }
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
            "captured_value": r.captured_value,
        })).collect::<Vec<_>>(),
        "state": outcome.state_facts,
        "values": outcome.values,
        "plan": outcome.plan.as_ref().map(|p| serde_json::json!({
            "steps": p.steps,
            "cost": p.cost,
        })),
    });
    println!("{v}");
}

/// Render one reactive-runtime event (ADR 0005) as pretty text or NDJSON.
///
/// `sensed` events are now per-sensor (sensors no longer run as a batched
/// sweep) and carry a `changed` flag — whether the reading moved the world and
/// triggered a replan. `planned`/`executed`/`complete` keep their previous JSON
/// tags; the terminal `complete` event is emitted once, last.
fn emit_runtime_event(event: &RuntimeEvent, pretty: bool) {
    if pretty {
        emit_runtime_event_pretty(event);
    } else {
        println!("{}", runtime_event_json(event));
    }
}

fn emit_runtime_event_pretty(event: &RuntimeEvent) {
    match event {
        RuntimeEvent::Sensed {
            sensor,
            success,
            changed,
            state,
            values,
            ..
        } => {
            let status = if *success { "ok" } else { "fail" };
            let mark = if *changed { "Δ" } else { " " };
            println!(
                "{mark} sense    {sensor} [{status}] → state({}): {}",
                state.len(),
                state.join(", "),
            );
            if *changed && !values.is_empty() {
                let pairs: Vec<String> = values.iter().map(|(k, v)| format!("{k}={v}")).collect();
                println!("      values   {}", pairs.join(", "));
            }
        }
        RuntimeEvent::Planned { plan } => match plan {
            None => println!("  plan     no plan exists"),
            Some(p) if p.steps.is_empty() => println!("  plan     goal satisfied"),
            Some(p) => println!(
                "  plan     {} steps, cost {:.1}: {}",
                p.steps.len(),
                p.cost,
                p.steps.join(" → "),
            ),
        },
        RuntimeEvent::Executed { result } => {
            let status = if result.success { "ok" } else { "FAIL" };
            println!("  exec     {} ... {status}", result.name);
            if !result.success && !result.stderr.is_empty() {
                for line in result.stderr.lines().take(5) {
                    println!("            stderr: {line}");
                }
            }
        }
        RuntimeEvent::Complete { outcome } => match outcome {
            RuntimeOutcome::GoalSatisfied => println!("[done]   goal satisfied"),
            RuntimeOutcome::NoPlan => println!("[stop]   no plan exists"),
            RuntimeOutcome::ActionFailed {
                name,
                exit_code,
                stderr,
            } => {
                println!(
                    "[stop]   action `{name}` failed (exit {})",
                    exit_code.map_or_else(|| "?".into(), |c| c.to_string()),
                );
                if !stderr.is_empty() {
                    for line in stderr.lines().take(10) {
                        println!("         stderr: {line}");
                    }
                }
            }
            RuntimeOutcome::MaxActionsReached { max } => {
                println!("[stop]   max actions reached ({max})");
            }
            RuntimeOutcome::Interrupted => println!("[stop]   interrupted"),
            RuntimeOutcome::Error { message } => println!("[error]  {message}"),
        },
    }
}

fn runtime_event_json(event: &RuntimeEvent) -> serde_json::Value {
    match event {
        RuntimeEvent::Sensed {
            sensor,
            success,
            added,
            removed,
            captured,
            changed,
            state,
            values,
        } => serde_json::json!({
            "event": "sensed",
            "sensor": sensor,
            "success": success,
            "added": added,
            "removed": removed,
            "captured_value": captured,
            "changed": changed,
            "state": state,
            "values": values,
        }),
        RuntimeEvent::Planned { plan } => serde_json::json!({
            "event": "planned",
            "plan": plan.as_ref().map(|p| serde_json::json!({
                "steps": p.steps,
                "cost": p.cost,
            })),
        }),
        RuntimeEvent::Executed { result } => serde_json::json!({
            "event": "executed",
            "result": {
                "name": result.name,
                "success": result.success,
                "exit_code": result.exit_code,
                "stderr": result.stderr,
            },
        }),
        RuntimeEvent::Complete { outcome } => match outcome {
            RuntimeOutcome::GoalSatisfied => serde_json::json!({
                "event": "complete",
                "outcome": "goal_satisfied",
            }),
            RuntimeOutcome::NoPlan => serde_json::json!({
                "event": "complete",
                "outcome": "no_plan",
            }),
            RuntimeOutcome::ActionFailed {
                name,
                exit_code,
                stderr,
            } => serde_json::json!({
                "event": "complete",
                "outcome": "action_failed",
                "action": name,
                "exit_code": exit_code,
                "stderr": stderr,
            }),
            RuntimeOutcome::MaxActionsReached { max } => serde_json::json!({
                "event": "complete",
                "outcome": "max_actions_reached",
                "max": max,
            }),
            RuntimeOutcome::Interrupted => serde_json::json!({
                "event": "complete",
                "outcome": "interrupted",
            }),
            RuntimeOutcome::Error { message } => serde_json::json!({
                "event": "complete",
                "outcome": "error",
                "error": message,
            }),
        },
    }
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
