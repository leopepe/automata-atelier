//! End-to-end tests against the compiled `uncharles` binary.
//!
//! Validates the seam unit tests cannot reach: argv parsing, file loading,
//! exit codes, and stdout/stderr separation. Drives the binary against the
//! checked-in smoke configs (which use only `true` / `sh -c` so they have
//! no side effects on the host).

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_uncharles")
}

fn config_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("configs")
        .join(name)
}

#[test]
fn one_shot_emits_json_plan_and_exits_zero() {
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_loop.yaml"))
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");
    assert_eq!(parsed["ok"], true);
    let steps = parsed["plan"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0], "do_a");
    assert_eq!(steps[2], "do_c");
}

#[test]
fn execute_loop_drives_smoke_config_to_goal() {
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_loop.yaml"))
        .arg("--execute")
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "expected exit 0 (goal satisfied), got {:?}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let last_line = stdout
        .lines()
        .last()
        .expect("expected at least one NDJSON line");
    let final_event: serde_json::Value =
        serde_json::from_str(last_line).expect("final line must be JSON");
    assert_eq!(final_event["event"], "complete");
    assert_eq!(final_event["outcome"], "goal_satisfied");
    assert_eq!(final_event["iterations"], 4);
}

#[test]
fn execute_loop_stops_on_action_failure_and_exits_nonzero() {
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_failure.yaml"))
        .arg("--execute")
        .output()
        .expect("failed to spawn uncharles");
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 on action failure"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let last_line = stdout.lines().last().unwrap();
    let final_event: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert_eq!(final_event["outcome"], "action_failed");
    assert_eq!(final_event["action"], "do_a");
    assert_eq!(final_event["exit_code"], 7);
    assert!(
        final_event["stderr"]
            .as_str()
            .unwrap()
            .contains("simulated failure"),
        "expected captured stderr to contain marker: {final_event}"
    );
}

#[test]
fn dry_run_emits_schema_without_running_sensors() {
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_loop.yaml"))
        .arg("--dry-run")
        .output()
        .expect("failed to spawn uncharles");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let sensors = parsed["sensors"].as_array().unwrap();
    assert_eq!(sensors.len(), 1);
    assert_eq!(sensors[0], "heartbeat");
    let actions = parsed["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0]["name"], "do_a");
    assert_eq!(actions[0]["cost"], 1.0);
}

#[test]
fn missing_config_file_exits_two() {
    let output = Command::new(binary())
        .arg("--config")
        .arg("/tmp/uncharles_definitely_does_not_exist.yaml")
        .output()
        .expect("failed to spawn uncharles");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot read"));
}

#[test]
fn execute_loop_with_interval_ms_paces_iterations() {
    // smoke_loop runs 3 actions; with --interval-ms=120 the inter-iteration
    // sleep fires after each successful exec, so wall clock should be at
    // least 360 ms (3 × 120 ms). Loose upper bound to keep CI noise out.
    let start = Instant::now();
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_loop.yaml"))
        .arg("--execute")
        .arg("--interval-ms")
        .arg("120")
        .output()
        .expect("failed to spawn uncharles");
    let elapsed = start.elapsed();
    assert!(output.status.success(), "expected exit 0 (goal satisfied)");
    assert!(
        elapsed >= Duration::from_millis(360),
        "expected ≥360 ms wall clock with --interval-ms=120, got {elapsed:?}"
    );
}

#[test]
fn release_watch_dry_run_emits_expected_schema() {
    // Validates the release-watch config parses, has the expected sensor
    // and action surface, and the `idle` goal is wired up. `--dry-run`
    // skips sensor execution, so this test does not depend on `git`/`gh`
    // being available or the cwd being a configured repo.
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("release_watch.yaml"))
        .arg("--dry-run")
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    let sensor_names: Vec<&str> = parsed["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        sensor_names,
        vec![
            "idle",
            "new_commit_available",
            "deploy_in_flight",
            "deploy_succeeded",
        ],
    );

    let action_names: Vec<&str> = parsed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        action_names,
        vec![
            "trigger_deploy",
            "await_deploy",
            "run_integration_tests",
            "collect_evidence",
        ],
    );

    let goal_requires: Vec<&str> = parsed["goal"]["requires"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(goal_requires, vec!["idle"]);

    // `collect_evidence` must remove `tests_pass`; without it the next
    // cycle's planner would see tests_pass leaked from the previous run
    // and skip integration tests for a fresh commit. Guard against a
    // future config edit silently dropping that effect.
    let collect_evidence = parsed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "collect_evidence")
        .unwrap();
    let removes: Vec<&str> = collect_evidence["removes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        removes.contains(&"tests_pass"),
        "collect_evidence must remove tests_pass; got removes={removes:?}"
    );
}

#[test]
fn execute_loop_drives_podcast_pipeline_end_to_end() {
    // Hermetic e2e for podcast.yaml. Pre-stages a fixture-guids.txt with
    // two GUIDs in a fresh temp dir, runs uncharles, and asserts the full
    // happy path: discover → download → verify → notify → commit. Each
    // step has a distinct artifact (file in library/, line in
    // downloaded.txt, content in last-cycle.txt) so a regression in any
    // stage trips a specific assertion rather than a vague "failed".
    use std::fs;

    let work_dir = std::env::temp_dir().join(format!(
        "uncharles-podcast-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).unwrap();

    let state = work_dir.join(".uncharles/state/podcast");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join("fixture-guids.txt"), "ep-001\nep-002\n").unwrap();
    fs::write(state.join("downloaded.txt"), "").unwrap();

    let output = Command::new(binary())
        .current_dir(&work_dir)
        .arg("--config")
        .arg(config_path("podcast.yaml"))
        .arg("--execute")
        .output()
        .expect("failed to spawn uncharles");

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let last_line = stdout.lines().last().expect("expected NDJSON output");
    let final_event: serde_json::Value =
        serde_json::from_str(last_line).expect("final line must be JSON");
    assert_eq!(final_event["outcome"], "goal_satisfied");

    // Pipeline ran the four happy-path actions in order. The recovery
    // alternative (await_download_retry) must NOT have fired on a clean
    // run — its presence here would mean the cheap path failed
    // unexpectedly.
    let executed: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v["event"] == "executed" && v["result"]["success"] == true {
                Some(v["result"]["name"].as_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        executed,
        vec![
            "download_episodes".to_string(),
            "verify_episodes".to_string(),
            "notify_user".to_string(),
            "commit_cycle".to_string(),
        ],
        "expected the four-action happy path; got {executed:?}",
    );

    let lib = work_dir.join(".uncharles/library/podcast");
    assert!(
        lib.join("ep-001.mp3").exists(),
        "ep-001.mp3 missing from library — download_episodes regression",
    );
    assert!(
        lib.join("ep-002.mp3").exists(),
        "ep-002.mp3 missing from library — download_episodes regression",
    );

    let downloaded = fs::read_to_string(state.join("downloaded.txt")).unwrap();
    assert!(
        downloaded.contains("ep-001") && downloaded.contains("ep-002"),
        "downloaded.txt missing GUIDs — commit_cycle regression: {downloaded:?}",
    );

    let pending = state.join("pending");
    let pending_count = fs::read_dir(&pending).map(|d| d.count()).unwrap_or(0);
    assert_eq!(
        pending_count, 0,
        "pending/ should be empty after commit — commit_cycle regression",
    );

    let last_cycle = fs::read_to_string(state.join("last-cycle.txt"))
        .expect("last-cycle.txt missing — notify_user regression");
    assert!(
        last_cycle.contains("new episodes available"),
        "notification log missing banner: {last_cycle:?}",
    );
    assert!(
        last_cycle.contains("ep-001") && last_cycle.contains("ep-002"),
        "notification log missing episode entries: {last_cycle:?}",
    );

    let _ = fs::remove_dir_all(&work_dir);
}

#[test]
fn execute_loop_recovers_via_action_on_failure() {
    // smoke_recover.yaml exercises the `on_failure` path: try_fast's cmd
    // exits 1, the on_failure clause flips state, and try_slow takes over.
    // The loop must reach `goal_satisfied`, not `action_failed`.
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_recover.yaml"))
        .arg("--execute")
        .arg("--have")
        .arg("fast_path_available")
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "expected exit 0 (goal satisfied via fallback path), got {:?}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let last_line = stdout.lines().last().unwrap();
    let final_event: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert_eq!(final_event["outcome"], "goal_satisfied");

    let executed: Vec<(String, bool)> = stdout
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v["event"] == "executed" {
                Some((
                    v["result"]["name"].as_str()?.to_string(),
                    v["result"]["success"].as_bool()?,
                ))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        executed,
        vec![
            ("try_fast".to_string(), false),
            ("try_slow".to_string(), true),
        ],
        "expected try_fast to fail then try_slow to succeed; got {executed:?}",
    );
}

#[test]
fn merge_gate_dry_run_pins_multi_requirement_design() {
    // Pins the design choices for `merge_gate.yaml`: a five-precondition
    // join (`merge_pr`), a single structural ordering rule
    // (`request_review` requires `ci_green`), and a cost scale that
    // reflects real-world expense (10 for CI, 3 for rebase, 1 for the
    // others). A future edit that drops a precondition or flattens the
    // costs will fail this test loudly.
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("merge_gate.yaml"))
        .arg("--dry-run")
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "expected exit 0, got {:?}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    let action_names: Vec<&str> = parsed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        action_names,
        vec![
            "resolve_threads",
            "request_review",
            "rebase_branch",
            "await_manual_rebase",
            "rerun_ci",
            "merge_pr",
        ],
    );

    let action = |name: &str| {
        parsed["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap_or_else(|| panic!("action `{name}` missing from merge_gate.yaml"))
    };
    let requires_of = |name: &str| -> Vec<&str> {
        action(name)["requires"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    };
    let cost_of = |name: &str| action(name)["cost"].as_f64().unwrap();

    assert_eq!(
        requires_of("merge_pr"),
        vec![
            "pr_open",
            "ci_green",
            "reviewer_approved",
            "branch_up_to_date",
            "no_unresolved_threads",
        ],
        "merge_pr must remain a five-precondition join",
    );

    assert_eq!(
        requires_of("request_review"),
        vec!["pr_open", "ci_green"],
        "request_review must stay gated on ci_green — the one structural ordering rule",
    );
    for parallel in ["resolve_threads", "rebase_branch", "rerun_ci"] {
        assert_eq!(
            requires_of(parallel),
            vec!["pr_open"],
            "`{parallel}` must remain a parallel prep with no ordering constraint",
        );
    }

    assert_eq!(cost_of("rerun_ci"), 10.0);
    assert_eq!(cost_of("rebase_branch"), 3.0);
    assert_eq!(cost_of("resolve_threads"), 1.0);
    assert_eq!(cost_of("request_review"), 1.0);
    assert_eq!(cost_of("merge_pr"), 1.0);

    // Failure-routing pattern: `rebase_branch` flags `auto_rebase_failed`
    // on failure; `await_manual_rebase` is the cheaper alternative that
    // becomes eligible only once the failure marker is set, so the
    // planner reroutes through it. Pin both halves of the pattern.
    let on_failure_adds: Vec<&str> = action("rebase_branch")["on_failure"]["add"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        on_failure_adds,
        vec!["auto_rebase_failed"],
        "rebase_branch.on_failure must set the marker that gates the recovery path",
    );
    assert_eq!(
        requires_of("await_manual_rebase"),
        vec!["pr_open", "auto_rebase_failed"],
        "await_manual_rebase must be gated on auto_rebase_failed so it's only eligible post-failure",
    );
    assert!(
        cost_of("await_manual_rebase") < cost_of("rebase_branch"),
        "await_manual_rebase must be cheaper than rebase_branch so the planner reroutes after failure (got {} vs {})",
        cost_of("await_manual_rebase"),
        cost_of("rebase_branch"),
    );
    let manual_adds: Vec<&str> = action("await_manual_rebase")["adds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        manual_adds,
        vec!["branch_up_to_date"],
        "await_manual_rebase must produce the same fact rebase_branch would have, so the join is reachable through either",
    );

    let goal_requires: Vec<&str> = parsed["goal"]["requires"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(goal_requires, vec!["pr_merged"]);
}

#[test]
fn one_shot_does_not_invoke_action_cmds() {
    // smoke_failure.yaml has an action whose `cmd` exits 7. In execute
    // mode that action would fail and the loop would stop. In one-shot
    // mode the planner only computes the plan — action cmds are never
    // run — so the binary exits 0 even though the action would fail.
    let output = Command::new(binary())
        .arg("--config")
        .arg(config_path("smoke_failure.yaml"))
        .output()
        .expect("failed to spawn uncharles");
    assert!(
        output.status.success(),
        "one-shot mode should not invoke action cmds; expected exit 0"
    );
}
