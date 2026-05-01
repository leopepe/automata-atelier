//! Shared CLI runner for the example binaries.
//!
//! Each example declares its action library and delegates to [`run`]. The
//! resulting binary accepts the same flag surface — agents only have to learn
//! it once.

use std::process::ExitCode;

use goap_planner::{Action, Goal, Planner, State};

#[derive(Default)]
pub struct Args {
    pub have: Vec<String>,
    pub forbid: Vec<String>,
    pub goal: Vec<String>,
    pub pretty: bool,
    pub list_actions: bool,
    pub help: bool,
}

pub fn parse_args() -> Result<Args, String> {
    let mut out = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--have" => out.have.push(iter.next().ok_or("--have needs a value")?),
            "--forbid" => out
                .forbid
                .push(iter.next().ok_or("--forbid needs a value")?),
            "--goal" => out.goal.push(iter.next().ok_or("--goal needs a value")?),
            "--pretty" => out.pretty = true,
            "--list-actions" => out.list_actions = true,
            "--help" | "-h" => out.help = true,
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(out)
}

pub fn print_help(name: &str) {
    eprintln!(
        "Usage: {name} [--have FACT]... [--forbid FACT]... [--goal FACT]... [--pretty] [--list-actions]"
    );
}

fn actions_to_json(actions: &[Action]) -> serde_json::Value {
    let entries: Vec<_> = actions
        .iter()
        .map(|a| {
            let mut requires: Vec<&str> = a.preconditions.iter().map(String::as_str).collect();
            let mut adds: Vec<&str> = a.add_effects.iter().map(String::as_str).collect();
            let mut removes: Vec<&str> = a.remove_effects.iter().map(String::as_str).collect();
            requires.sort_unstable();
            adds.sort_unstable();
            removes.sort_unstable();
            serde_json::json!({
                "name": a.name,
                "cost": a.cost,
                "requires": requires,
                "adds": adds,
                "removes": removes,
            })
        })
        .collect();
    serde_json::json!({ "actions": entries })
}

pub fn run(name: &str, actions: Vec<Action>) -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_help(name);
            return ExitCode::from(2);
        }
    };

    if args.help {
        print_help(name);
        return ExitCode::SUCCESS;
    }

    if args.list_actions {
        let v = actions_to_json(&actions);
        let s = if args.pretty {
            serde_json::to_string_pretty(&v).unwrap()
        } else {
            v.to_string()
        };
        println!("{s}");
        return ExitCode::SUCCESS;
    }

    if args.goal.is_empty() {
        eprintln!("error: at least one --goal is required");
        print_help(name);
        return ExitCode::from(2);
    }

    let initial = State::from_facts(args.have.iter().cloned());
    let mut goal = Goal::new();
    for f in &args.goal {
        goal = goal.requires(f);
    }
    for f in &args.forbid {
        goal = goal.forbids(f);
    }

    match Planner::new(actions).plan(&initial, &goal) {
        Ok(Some(plan)) => {
            if args.pretty {
                if plan.steps.is_empty() {
                    println!("(goal already satisfied)");
                } else {
                    for (i, step) in plan.steps.iter().enumerate() {
                        println!("{:>2}. {step}", i + 1);
                    }
                }
                println!("total cost: {:.1}", plan.cost);
            } else {
                let v = serde_json::json!({
                    "ok": true,
                    "plan": { "steps": plan.steps, "cost": plan.cost },
                });
                println!("{v}");
            }
            ExitCode::SUCCESS
        }
        Ok(None) => {
            if args.pretty {
                println!("no plan exists");
            } else {
                let v = serde_json::json!({
                    "ok": true,
                    "plan": null,
                    "reason": "no plan exists",
                });
                println!("{v}");
            }
            ExitCode::from(1)
        }
        Err(e) => {
            let msg = e.to_string();
            if args.pretty {
                eprintln!("error: {msg}");
            } else {
                let v = serde_json::json!({ "ok": false, "error": msg });
                println!("{v}");
            }
            ExitCode::from(2)
        }
    }
}
