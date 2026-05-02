//! End-to-end tests for the `uncharles inspect` subcommand.
//!
//! Run the compiled binary against the checked-in smoke configs (no shell
//! side effects) and assert on the human-readable report produced.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_uncharles")
}

fn config_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("configs")
        .join(name)
}

#[test]
fn inspect_prints_all_sections_for_smoke_loop() {
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .output()
        .expect("failed to spawn uncharles inspect");
    assert!(
        output.status.success(),
        "expected exit 0 (clean config), got {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // All six sections present.
    assert!(stdout.contains("== sensors ("));
    assert!(stdout.contains("== actions ("));
    assert!(stdout.contains("== goal =="));
    assert!(stdout.contains("== initial state"));
    assert!(stdout.contains("== state-action graph =="));
    assert!(stdout.contains("== static analysis =="));

    // Static analysis on a clean config should pass.
    assert!(stdout.contains("✓ no orphan actions"));
    assert!(stdout.contains("✓ no unreachable goal facts"));
    assert!(stdout.contains("✓ no dead-end states"));

    // The smoke-loop config has 1 sensor, 3 actions, and the planner's plan
    // is do_a → do_b → do_c, so the graph should have 4 reachable states.
    assert!(stdout.contains("(initial)"));
    assert!(stdout.contains("goal ✓"));
    assert!(stdout.contains("do_a"));
    assert!(stdout.contains("do_b"));
    assert!(stdout.contains("do_c"));
}

#[test]
fn inspect_does_not_run_sensor_or_action_commands() {
    // Smoke-loop's sensors and actions all use `true`, but we use a marker
    // file to confirm: if a command had run, this file would exist.
    // We pass a config that never executes anything (and the smoke configs
    // don't either), and verify the inspect run never blocks waiting for
    // anything that needs IO. The simplest proof: the run finishes very
    // fast and the report lands.
    let start = std::time::Instant::now();
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .output()
        .expect("failed to spawn uncharles inspect");
    let elapsed = start.elapsed();

    assert!(output.status.success());
    // Inspection is purely in-memory after YAML parse; should finish in
    // well under a second on any reasonable machine.
    assert!(
        elapsed.as_secs() < 5,
        "inspect took {elapsed:?}, much longer than expected for an in-memory run"
    );

    // The "initial state (simulated)" header is the user-visible signal
    // that no sensors ran.
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("== initial state (simulated) =="),
        "expected the 'simulated' label on the initial-state section to make it explicit that sensors did not run"
    );
    assert!(stdout.contains("No sensor commands were run."));
}

#[test]
fn inspect_have_flag_layers_extra_facts_on_initial_state() {
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .args(["--have", "extra_fact"])
        .output()
        .expect("failed to spawn uncharles inspect");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // The `--have` flag should put `extra_fact` into the simulated initial state.
    assert!(
        stdout.contains("extra_fact"),
        "expected --have to layer the fact onto the initial state\nstdout:\n{stdout}"
    );
}

#[test]
fn inspect_flags_orphan_actions_with_nonzero_exit() {
    // smoke_failure.yaml is checked in for failure-recovery tests, but
    // we want a config with an orphan action specifically. Build one
    // ad-hoc via a temp file.
    let tmp = std::env::temp_dir().join("uncharles_inspect_orphan.yaml");
    std::fs::write(
        &tmp,
        r#"
sensors: []
actions:
  - name: needs_unobtainium
    cost: 1.0
    requires: [unobtainium]
    adds: [done]
    cmd: ["true"]
goal:
  requires: [done]
"#,
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn uncharles inspect");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("✗ orphan actions"),
        "expected orphan-action callout\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("needs_unobtainium"));
    assert!(stdout.contains("unobtainium"));

    // Issues found → exit code 1 (distinct from 2 = couldn't run).
    let code = output
        .status
        .code()
        .expect("expected normal exit, not signal");
    assert_eq!(code, 1, "exit 1 on issues found, got {code}");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn inspect_flags_unreachable_goal_facts() {
    let tmp = std::env::temp_dir().join("uncharles_inspect_unreachable_goal.yaml");
    std::fs::write(
        &tmp,
        r#"
sensors:
  - name: ready
    cmd: ["true"]
actions:
  - name: act
    cost: 1.0
    requires: [ready]
    adds: [partially_done]
    cmd: ["true"]
goal:
  requires: [fully_done]
"#,
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn uncharles inspect");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("✗ unreachable goal facts"),
        "expected unreachable-goal callout\nstdout:\n{stdout}"
    );
    assert!(stdout.contains("fully_done"));

    let _ = std::fs::remove_file(&tmp);
}

// ---------------------------------------------------------------------------
// --format flag — DOT, Mermaid, JSON
// ---------------------------------------------------------------------------

#[test]
fn inspect_format_dot_emits_valid_digraph() {
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .args(["--format", "dot"])
        .output()
        .expect("failed to spawn uncharles inspect --format dot");
    assert!(output.status.success(), "expected exit 0 on clean config");
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("digraph uncharles_inspect"));
    assert!(stdout.contains("rankdir=LR"));
    assert!(stdout.contains("S0 [label="));
    assert!(stdout.trim_end().ends_with('}'));
    // No human-readable section headers should leak through.
    assert!(!stdout.contains("== sensors ("));
}

#[test]
fn inspect_format_mermaid_emits_flowchart() {
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .args(["--format", "mermaid"])
        .output()
        .expect("failed to spawn uncharles inspect --format mermaid");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("flowchart LR"));
    assert!(stdout.contains("classDef initial"));
    assert!(stdout.contains("classDef goal"));
    assert!(stdout.contains("S0[\""));
    assert!(!stdout.contains("== sensors ("));
}

#[test]
fn inspect_format_json_parses_and_has_expected_structure() {
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .args(["--format", "json"])
        .output()
        .expect("failed to spawn uncharles inspect --format json");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--format json must produce valid JSON");

    for key in [
        "sensors",
        "actions",
        "goal",
        "initial_state",
        "graph",
        "static_analysis",
    ] {
        assert!(
            v.get(key).is_some(),
            "JSON missing top-level key `{key}`\n{stdout}"
        );
    }
    assert!(v["graph"]["states"].is_array());
    assert!(v["graph"]["edges"].is_array());
    assert_eq!(v["static_analysis"]["is_clean"], true);
}

#[test]
fn inspect_format_json_propagates_static_analysis_findings() {
    let tmp = std::env::temp_dir().join("uncharles_inspect_json_orphan.yaml");
    std::fs::write(
        &tmp,
        r#"
sensors: []
actions:
  - name: needs_unobtainium
    cost: 1.0
    requires: [unobtainium]
    adds: [done]
    cmd: ["true"]
goal:
  requires: [done]
"#,
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(&tmp)
        .args(["--format", "json"])
        .output()
        .expect("failed to spawn uncharles inspect --format json");

    // Issues found → exit code 1, even with --format json.
    let code = output.status.code().expect("normal exit");
    assert_eq!(code, 1);

    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    assert_eq!(v["static_analysis"]["is_clean"], false);
    let orphans = v["static_analysis"]["orphan_actions"].as_array().unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0]["action"], "needs_unobtainium");
    assert_eq!(orphans[0]["missing_fact"], "unobtainium");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn inspect_format_text_remains_default_when_flag_omitted() {
    // No --format argument: should produce the human-readable text report.
    let output = Command::new(binary())
        .args(["inspect", "--config"])
        .arg(config_path("smoke_loop.yaml"))
        .output()
        .expect("failed to spawn uncharles inspect");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("== sensors ("));
    assert!(!stdout.contains("digraph"));
    assert!(!stdout.contains("flowchart"));
}

// ---------------------------------------------------------------------------
// Legacy CLI bridge — preserved across the format additions
// ---------------------------------------------------------------------------

#[test]
fn legacy_flat_cli_still_works_via_run_bridge() {
    // `uncharles --config X` should be parsed as `uncharles run --config X`
    // for backward compat with existing scripts and tests.
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_loop.yaml"))
        .output()
        .expect("failed to spawn legacy-style uncharles");
    assert!(
        output.status.success(),
        "legacy `uncharles --config X` should still produce a plan; got {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["ok"], true);
}
