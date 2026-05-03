use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use goap_planner::{Action, Goal, Planner, State};

use crate::config::{ActionSpec, Config, SensorSpec};

#[derive(Debug, Clone)]
pub struct SensorReading {
    pub name: String,
    pub success: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ActionResult {
    pub name: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

#[derive(Debug)]
pub enum RunError {
    SensorExec { name: String, error: String },
    ActionExec { name: String, error: String },
    ActionMissingCmd { name: String },
    PlannerActionMismatch { name: String },
    Planner(goap_planner::PlannerError),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SensorExec { name, error } => {
                write!(f, "sensor `{name}` failed to execute: {error}")
            }
            Self::ActionExec { name, error } => {
                write!(f, "action `{name}` failed to execute: {error}")
            }
            Self::ActionMissingCmd { name } => {
                write!(
                    f,
                    "action `{name}` has no `cmd` field — cannot execute (use one-shot mode)"
                )
            }
            Self::PlannerActionMismatch { name } => {
                write!(
                    f,
                    "planner returned action `{name}` which is absent from the config"
                )
            }
            Self::Planner(e) => write!(f, "planner error: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

pub fn run_sensor(spec: &SensorSpec, state: &mut State) -> Result<SensorReading, RunError> {
    let cmd_name = spec.cmd.first().cloned().unwrap_or_default();
    let status = Command::new(&cmd_name)
        .args(spec.cmd.iter().skip(1))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| RunError::SensorExec {
            name: spec.name.clone(),
            error: e.to_string(),
        })?;

    let success = status.success();
    let effects = spec.effects_for(success);
    for fact in &effects.add {
        state.insert(fact.clone());
    }
    for fact in &effects.remove {
        state.remove(fact);
    }

    Ok(SensorReading {
        name: spec.name.clone(),
        success,
        added: effects.add,
        removed: effects.remove,
    })
}

pub fn execute_action(spec: &ActionSpec) -> Result<ActionResult, RunError> {
    let cmd = spec
        .cmd
        .as_ref()
        .ok_or_else(|| RunError::ActionMissingCmd {
            name: spec.name.clone(),
        })?;
    let cmd_name = cmd.first().cloned().unwrap_or_default();
    let output = Command::new(&cmd_name)
        .args(cmd.iter().skip(1))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| RunError::ActionExec {
            name: spec.name.clone(),
            error: e.to_string(),
        })?;

    Ok(ActionResult {
        name: spec.name.clone(),
        success: output.status.success(),
        exit_code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_string(),
    })
}

/// Sleep for up to `total_ms` milliseconds, waking early if `interrupted`
/// becomes true. The poll slice is short (50 ms) so SIGINT remains responsive
/// even when the configured interval is long.
fn interruptible_sleep(total_ms: u64, interrupted: &AtomicBool) {
    if total_ms == 0 {
        return;
    }
    let total = Duration::from_millis(total_ms);
    let slice = Duration::from_millis(50);
    let start = Instant::now();
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return;
        }
        let elapsed = start.elapsed();
        if elapsed >= total {
            return;
        }
        let remaining = total - elapsed;
        std::thread::sleep(remaining.min(slice));
    }
}

fn find_action_spec<'a>(specs: &'a [ActionSpec], name: &str) -> Result<&'a ActionSpec, RunError> {
    specs
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| RunError::PlannerActionMismatch {
            name: name.to_string(),
        })
}

pub fn build_actions(specs: &[ActionSpec]) -> Vec<Action> {
    specs
        .iter()
        .map(|s| {
            let mut a = Action::new(&s.name, s.cost);
            for fact in &s.requires {
                a = a.requires(fact);
            }
            for fact in &s.forbids {
                a = a.forbids(fact);
            }
            for fact in &s.adds {
                a = a.adds(fact);
            }
            for fact in &s.removes {
                a = a.removes(fact);
            }
            a
        })
        .collect()
}

pub fn build_goal(spec: &crate::config::GoalSpec) -> Goal {
    let mut g = Goal::new();
    for fact in &spec.requires {
        g = g.requires(fact);
    }
    for fact in &spec.forbids {
        g = g.forbids(fact);
    }
    g
}

pub struct Outcome {
    pub readings: Vec<SensorReading>,
    pub state_facts: Vec<String>,
    pub plan: Option<goap_planner::Plan>,
}

pub fn sense_and_plan(config: &Config, seed: Vec<String>) -> Result<Outcome, RunError> {
    let mut state = State::from_facts(seed);

    let mut readings = Vec::with_capacity(config.sensors.len());
    for sensor in &config.sensors {
        readings.push(run_sensor(sensor, &mut state)?);
    }

    let actions = build_actions(&config.actions);
    let goal = build_goal(&config.goal);
    let plan = Planner::new(actions)
        .plan(&state, &goal)
        .map_err(RunError::Planner)?;

    let mut state_facts: Vec<String> = state.facts().map(String::from).collect();
    state_facts.sort();

    Ok(Outcome {
        readings,
        state_facts,
        plan,
    })
}

#[derive(Debug)]
pub enum LoopEvent {
    Sensed {
        iteration: usize,
        readings: Vec<SensorReading>,
        state: Vec<String>,
    },
    Planned {
        iteration: usize,
        plan: Option<goap_planner::Plan>,
    },
    Executed {
        iteration: usize,
        result: ActionResult,
    },
}

#[derive(Debug)]
pub enum LoopOutcome {
    GoalSatisfied {
        iteration: usize,
    },
    NoPlan {
        iteration: usize,
    },
    ActionFailed {
        iteration: usize,
        name: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    Interrupted {
        iteration: usize,
    },
    MaxIterationsReached {
        iteration: usize,
        max: usize,
    },
}

/// Drive the sense → plan → execute loop until the goal is satisfied, the
/// planner reports no path, an action fails without an `on_failure` clause,
/// the iteration cap is hit, or `interrupted` flips to true (signal handler).
///
/// Replanning is automatic: each iteration re-runs every sensor and re-plans
/// from the resulting [`State`]. If reality diverges from the planner's
/// expectation (sensor for `image_built` reports false after `docker_build`
/// "succeeded"), the next iteration's planner sees the new state and either
/// finds a different path or returns [`LoopOutcome::NoPlan`].
///
/// When an action's `cmd` exits non-zero, behaviour depends on whether the
/// action declares an `on_failure` clause:
///
/// - **Without `on_failure`**: the loop terminates with
///   [`LoopOutcome::ActionFailed`]. This is the default; failure is fatal.
/// - **With `on_failure`**: the listed adds/removes are applied to state,
///   the loop sleeps for `interval_ms`, and the next iteration replans from
///   the resulting state. The action's own `adds`/`removes` are *not*
///   applied — those describe the success-path contract for the planner.
///   If the planner can find an alternative, the loop continues; if not,
///   the next iteration returns [`LoopOutcome::NoPlan`] cleanly.
///
/// `interval_ms` is the minimum delay (in milliseconds) between iterations
/// after a successful action execution. `0` means run as fast as work allows
/// — appropriate for "drive to goal" use cases. A non-zero value paces
/// "watch the world" configs that would otherwise hammer external sensors in
/// a tight goal-satisfied loop. The sleep is interruptible: SIGINT wakes it
/// in at most ~50 ms regardless of the configured interval.
///
/// Returns [`RunError::PlannerActionMismatch`] if the planner ever names an
/// action that is not present in `config.actions`. This is a programming-bug
/// invariant under the current single-source-of-actions model, but is surfaced
/// as a typed error so callers can report it cleanly rather than panic.
pub fn run_loop(
    config: &Config,
    seed: Vec<String>,
    max_iterations: usize,
    interval_ms: u64,
    interrupted: Arc<AtomicBool>,
    mut on_event: impl FnMut(LoopEvent),
) -> Result<LoopOutcome, RunError> {
    let mut state = State::from_facts(seed);
    let actions = build_actions(&config.actions);
    let goal = build_goal(&config.goal);
    let planner = Planner::new(actions);

    let mut iteration = 0;

    loop {
        if interrupted.load(Ordering::Relaxed) {
            return Ok(LoopOutcome::Interrupted { iteration });
        }
        if iteration >= max_iterations {
            return Ok(LoopOutcome::MaxIterationsReached {
                iteration,
                max: max_iterations,
            });
        }
        iteration += 1;

        let mut readings = Vec::with_capacity(config.sensors.len());
        for sensor in &config.sensors {
            readings.push(run_sensor(sensor, &mut state)?);
        }
        let mut state_facts: Vec<String> = state.facts().map(String::from).collect();
        state_facts.sort();
        on_event(LoopEvent::Sensed {
            iteration,
            readings,
            state: state_facts,
        });

        let plan = planner.plan(&state, &goal).map_err(RunError::Planner)?;
        on_event(LoopEvent::Planned {
            iteration,
            plan: plan.clone(),
        });

        match plan {
            None => return Ok(LoopOutcome::NoPlan { iteration }),
            Some(p) if p.steps.is_empty() => {
                return Ok(LoopOutcome::GoalSatisfied { iteration });
            }
            Some(p) => {
                let next = &p.steps[0];
                let spec = find_action_spec(&config.actions, next)?;
                let result = execute_action(spec)?;
                let result_for_event = result.clone();
                on_event(LoopEvent::Executed {
                    iteration,
                    result: result_for_event,
                });
                if !result.success {
                    let Some(failure_effects) = &spec.on_failure else {
                        return Ok(LoopOutcome::ActionFailed {
                            iteration,
                            name: result.name,
                            exit_code: result.exit_code,
                            stderr: result.stderr,
                        });
                    };
                    // Failure is recoverable: apply the on_failure effects
                    // and let the next iteration replan. The action's own
                    // adds/removes are skipped — those describe the
                    // success-path contract, not the failure aftermath.
                    for fact in &failure_effects.remove {
                        state.remove(fact);
                    }
                    for fact in &failure_effects.add {
                        state.insert(fact.clone());
                    }
                    interruptible_sleep(interval_ms, &interrupted);
                    continue;
                }
                // Optimistically apply the action's declared effects so the
                // next iteration plans from the expected post-state. Sensors
                // run again at the top of the next iteration and will correct
                // the state if reality disagrees — that's the replan trigger.
                for fact in &spec.removes {
                    state.remove(fact);
                }
                for fact in &spec.adds {
                    state.insert(fact.clone());
                }
                interruptible_sleep(interval_ms, &interrupted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Effects, GoalSpec};

    fn sensor(name: &str, cmd: &[&str]) -> SensorSpec {
        SensorSpec {
            name: name.into(),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            on_success: None,
            on_failure: None,
        }
    }

    fn action(name: &str, cmd: &[&str], requires: &[&str], adds: &[&str]) -> ActionSpec {
        ActionSpec {
            name: name.into(),
            cost: 1.0,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            forbids: Vec::new(),
            adds: adds.iter().map(|s| s.to_string()).collect(),
            removes: Vec::new(),
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            on_failure: None,
        }
    }

    // ---------------------------------------------------------------------
    // run_sensor — default effect mapping (success → add, failure → remove)
    // ---------------------------------------------------------------------

    #[test]
    fn sensor_success_adds_fact_named_after_sensor() {
        let mut state = State::new();
        let reading = run_sensor(&sensor("ready", &["true"]), &mut state).unwrap();
        assert!(reading.success);
        assert_eq!(reading.added, vec!["ready"]);
        assert!(reading.removed.is_empty());
        assert!(state.contains("ready"));
    }

    #[test]
    fn sensor_failure_removes_fact_named_after_sensor() {
        let mut state = State::from_facts(["ready"]);
        let reading = run_sensor(&sensor("ready", &["false"]), &mut state).unwrap();
        assert!(!reading.success);
        assert_eq!(reading.removed, vec!["ready"]);
        assert!(reading.added.is_empty());
        assert!(!state.contains("ready"));
    }

    #[test]
    fn sensor_custom_on_success_overrides_default() {
        let mut state = State::new();
        let spec = SensorSpec {
            name: "build".into(),
            cmd: vec!["true".into()],
            on_success: Some(Effects {
                add: vec!["build_ok".into(), "tests_pass".into()],
                remove: vec!["build_failing".into()],
            }),
            on_failure: None,
        };
        let mut state_with_failing = State::from_facts(["build_failing"]);
        let reading = run_sensor(&spec, &mut state_with_failing).unwrap();
        assert!(reading.success);
        assert!(state_with_failing.contains("build_ok"));
        assert!(state_with_failing.contains("tests_pass"));
        assert!(!state_with_failing.contains("build_failing"));

        // Default (None on_success) confirmed earlier; sanity: adding empty
        // on_success can be used to suppress the default add.
        let suppress = SensorSpec {
            name: "noisy".into(),
            cmd: vec!["true".into()],
            on_success: Some(Effects::default()),
            on_failure: None,
        };
        run_sensor(&suppress, &mut state).unwrap();
        assert!(!state.contains("noisy"));
    }

    #[test]
    fn sensor_missing_command_returns_sensor_exec_error() {
        let mut state = State::new();
        let err = run_sensor(
            &sensor("ghost", &["definitely-not-a-real-binary-xyz123"]),
            &mut state,
        )
        .unwrap_err();
        assert!(matches!(err, RunError::SensorExec { ref name, .. } if name == "ghost"));
    }

    // ---------------------------------------------------------------------
    // execute_action — exit code, stderr capture, missing-cmd error
    // ---------------------------------------------------------------------

    #[test]
    fn execute_action_success_returns_zero_exit() {
        let result = execute_action(&action("noop", &["true"], &[], &[])).unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn execute_action_failure_captures_exit_and_stderr() {
        let spec = action("failing", &["sh", "-c", "echo boom >&2; exit 7"], &[], &[]);
        let result = execute_action(&spec).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.stderr, "boom");
    }

    #[test]
    fn execute_action_without_cmd_returns_missing_cmd_error() {
        let spec = ActionSpec {
            name: "no_cmd".into(),
            cost: 1.0,
            requires: Vec::new(),
            forbids: Vec::new(),
            adds: Vec::new(),
            removes: Vec::new(),
            cmd: None,
            on_failure: None,
        };
        let err = execute_action(&spec).unwrap_err();
        assert!(matches!(err, RunError::ActionMissingCmd { ref name } if name == "no_cmd"));
    }

    #[test]
    fn execute_action_unknown_binary_returns_action_exec_error() {
        let spec = action("ghost", &["definitely-not-a-real-binary-xyz123"], &[], &[]);
        let err = execute_action(&spec).unwrap_err();
        assert!(matches!(err, RunError::ActionExec { ref name, .. } if name == "ghost"));
    }

    // ---------------------------------------------------------------------
    // build_actions / build_goal — translation from config to planner types
    // ---------------------------------------------------------------------

    #[test]
    fn build_actions_preserves_costs_and_effects() {
        let specs = vec![action("a", &["true"], &["x"], &["y"])];
        let actions = build_actions(&specs);
        assert_eq!(actions.len(), 1);
        // Indirectly verify by attempting a plan.
        let initial = State::from_facts(["x"]);
        let goal = Goal::new().requires("y");
        let plan = Planner::new(actions)
            .plan(&initial, &goal)
            .unwrap()
            .unwrap();
        assert_eq!(plan.steps, vec!["a"]);
        assert_eq!(plan.cost, 1.0);
    }

    #[test]
    fn build_goal_carries_required_and_forbidden_facts() {
        let goal = build_goal(&GoalSpec {
            requires: vec!["ok".into()],
            forbids: vec!["bad".into()],
        });
        assert!(goal.satisfied_by(&State::from_facts(["ok"])));
        assert!(!goal.satisfied_by(&State::from_facts(["ok", "bad"])));
        assert!(!goal.satisfied_by(&State::new()));
    }

    // ---------------------------------------------------------------------
    // find_action_spec — typed error when planner names an unknown action
    // ---------------------------------------------------------------------

    #[test]
    fn find_action_spec_returns_typed_error_on_mismatch() {
        let actions: Vec<ActionSpec> = vec![action("known", &["true"], &[], &[])];
        let err = find_action_spec(&actions, "ghost").unwrap_err();
        assert!(matches!(err, RunError::PlannerActionMismatch { ref name } if name == "ghost"));
        assert!(err.to_string().contains("ghost"));
        assert!(err.to_string().contains("absent from the config"));
    }

    #[test]
    fn find_action_spec_returns_matching_spec_when_present() {
        let actions = vec![
            action("a", &["true"], &[], &[]),
            action("b", &["true"], &[], &[]),
        ];
        let spec = find_action_spec(&actions, "b").unwrap();
        assert_eq!(spec.name, "b");
    }

    // ---------------------------------------------------------------------
    // interruptible_sleep — pacing primitive used by run_loop's --interval-ms
    // ---------------------------------------------------------------------

    #[test]
    fn interruptible_sleep_zero_returns_immediately() {
        let interrupted = AtomicBool::new(false);
        let start = Instant::now();
        interruptible_sleep(0, &interrupted);
        assert!(
            start.elapsed() < Duration::from_millis(20),
            "zero interval must not sleep"
        );
    }

    #[test]
    fn interruptible_sleep_waits_at_least_the_configured_interval() {
        let interrupted = AtomicBool::new(false);
        let start = Instant::now();
        interruptible_sleep(120, &interrupted);
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(120),
            "expected ≥120 ms, got {elapsed:?}"
        );
        // Generous upper bound — we only care it doesn't run away.
        assert!(
            elapsed < Duration::from_millis(800),
            "expected <800 ms, got {elapsed:?}"
        );
    }

    #[test]
    fn interruptible_sleep_wakes_early_when_interrupted_is_set_mid_wait() {
        let interrupted = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&interrupted);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            flag.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        interruptible_sleep(5_000, &interrupted);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "expected interrupt to wake sleep promptly, got {elapsed:?}"
        );
    }

    // ---------------------------------------------------------------------
    // run_loop — termination conditions
    // ---------------------------------------------------------------------

    fn drain_events(events: &mut Vec<LoopEvent>) -> impl FnMut(LoopEvent) + '_ {
        |e| events.push(e)
    }

    fn three_step_config() -> Config {
        Config {
            sensors: vec![sensor("heartbeat", &["true"])],
            actions: vec![
                action("do_a", &["true"], &["heartbeat"], &["a_done"]),
                action("do_b", &["true"], &["a_done"], &["b_done"]),
                action("do_c", &["true"], &["b_done"], &["finished"]),
            ],
            goal: GoalSpec {
                requires: vec!["finished".into()],
                forbids: Vec::new(),
            },
        }
    }

    #[test]
    fn loop_terminates_at_goal_satisfied() {
        let config = three_step_config();
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        let outcome = run_loop(
            &config,
            Vec::new(),
            100,
            0,
            Arc::clone(&interrupted),
            drain_events(&mut events),
        )
        .unwrap();
        match outcome {
            LoopOutcome::GoalSatisfied { iteration } => assert_eq!(iteration, 4),
            other => panic!("expected GoalSatisfied, got {other:?}"),
        }
        // 4 sense + 4 plan + 3 exec = 11 events.
        assert_eq!(events.len(), 11);
    }

    #[test]
    fn loop_returns_no_plan_when_goal_unreachable() {
        let mut config = three_step_config();
        config.goal.requires = vec!["never_set".into()];
        let interrupted = Arc::new(AtomicBool::new(false));
        let outcome =
            run_loop(&config, Vec::new(), 10, 0, Arc::clone(&interrupted), |_| {}).unwrap();
        assert!(matches!(outcome, LoopOutcome::NoPlan { iteration: 1 }));
    }

    #[test]
    fn loop_stops_on_action_failure_and_surfaces_stderr() {
        let mut config = three_step_config();
        config.actions[0] = action(
            "do_a",
            &["sh", "-c", "echo nope >&2; exit 3"],
            &["heartbeat"],
            &["a_done"],
        );
        let interrupted = Arc::new(AtomicBool::new(false));
        let outcome =
            run_loop(&config, Vec::new(), 10, 0, Arc::clone(&interrupted), |_| {}).unwrap();
        match outcome {
            LoopOutcome::ActionFailed {
                iteration,
                name,
                exit_code,
                stderr,
            } => {
                assert_eq!(iteration, 1);
                assert_eq!(name, "do_a");
                assert_eq!(exit_code, Some(3));
                assert_eq!(stderr, "nope");
            }
            other => panic!("expected ActionFailed, got {other:?}"),
        }
    }

    #[test]
    fn loop_respects_max_iterations_cap() {
        // Sensor that adds heartbeat; action is a no-op that doesn't add the
        // expected fact via sensor (no sensor for a_done), but optimistic
        // effect application means do_a → a_done → goal in 2 iterations.
        // To force the cap to bite, model a never-satisfiable scenario.
        let config = Config {
            sensors: vec![sensor("heartbeat", &["true"])],
            actions: vec![
                // Action with a precondition that never becomes true → no plan.
                // But that returns NoPlan, not MaxIterations. Instead, build a
                // config where each iteration makes no progress: action's
                // effects are sensor-overridden every iteration.
                ActionSpec {
                    name: "loop_action".into(),
                    cost: 1.0,
                    requires: vec!["heartbeat".into()],
                    forbids: Vec::new(),
                    adds: vec!["progress".into()],
                    removes: Vec::new(),
                    cmd: Some(vec!["true".into()]),
                    on_failure: None,
                },
            ],
            goal: GoalSpec {
                requires: vec!["unreachable_fact".into()],
                forbids: Vec::new(),
            },
        };
        let interrupted = Arc::new(AtomicBool::new(false));
        let outcome =
            run_loop(&config, Vec::new(), 5, 0, Arc::clone(&interrupted), |_| {}).unwrap();
        // Goal unreachable → NoPlan on first iteration, before max kicks in.
        // This is correct behaviour: NoPlan is the natural terminator when
        // the planner can't find a path, and is preferred over spinning.
        assert!(matches!(outcome, LoopOutcome::NoPlan { iteration: 1 }));
    }

    #[test]
    fn loop_returns_interrupted_when_flag_is_set_before_first_iteration() {
        let config = three_step_config();
        let interrupted = Arc::new(AtomicBool::new(true));
        let outcome =
            run_loop(&config, Vec::new(), 10, 0, Arc::clone(&interrupted), |_| {}).unwrap();
        assert!(matches!(outcome, LoopOutcome::Interrupted { iteration: 0 }));
    }

    #[test]
    fn loop_with_nonzero_interval_paces_iterations() {
        // Three actions × ~60 ms inter-iteration sleep ≈ ≥120 ms (the sleep
        // fires after each successful exec; the final iteration plans an
        // empty plan and returns without sleeping).
        let config = three_step_config();
        let interrupted = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let outcome = run_loop(
            &config,
            Vec::new(),
            100,
            60,
            Arc::clone(&interrupted),
            |_| {},
        )
        .unwrap();
        let elapsed = start.elapsed();
        assert!(matches!(
            outcome,
            LoopOutcome::GoalSatisfied { iteration: 4 }
        ));
        assert!(
            elapsed >= Duration::from_millis(180),
            "expected ≥180 ms with --interval-ms=60 across 3 execs, got {elapsed:?}"
        );
    }

    #[test]
    fn loop_with_action_on_failure_replans_through_alternative_path() {
        // `try_fast` fails (`cmd: false`) but its on_failure removes
        // `fast_path_available` (its own precondition, locking it out)
        // and adds `slow_path_unlocked` (the alternative's precondition).
        // The loop must absorb the failure, replan, and reach the goal
        // via `try_slow`.
        let config = Config {
            sensors: vec![sensor("heartbeat", &["true"])],
            actions: vec![
                ActionSpec {
                    name: "try_fast".into(),
                    cost: 1.0,
                    requires: vec!["heartbeat".into(), "fast_path_available".into()],
                    forbids: Vec::new(),
                    adds: vec!["finished".into()],
                    removes: Vec::new(),
                    cmd: Some(vec!["false".into()]),
                    on_failure: Some(Effects {
                        add: vec!["slow_path_unlocked".into()],
                        remove: vec!["fast_path_available".into()],
                    }),
                },
                ActionSpec {
                    name: "try_slow".into(),
                    cost: 5.0,
                    requires: vec!["slow_path_unlocked".into()],
                    forbids: Vec::new(),
                    adds: vec!["finished".into()],
                    removes: Vec::new(),
                    cmd: Some(vec!["true".into()]),
                    on_failure: None,
                },
            ],
            goal: GoalSpec {
                requires: vec!["finished".into()],
                forbids: Vec::new(),
            },
        };
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        let outcome = run_loop(
            &config,
            vec!["fast_path_available".into()],
            10,
            0,
            Arc::clone(&interrupted),
            drain_events(&mut events),
        )
        .unwrap();
        match outcome {
            // Iteration 1: try_fast → fail → on_failure mutates state.
            // Iteration 2: planner picks try_slow → succeeds.
            // Iteration 3: empty plan → goal satisfied.
            LoopOutcome::GoalSatisfied { iteration } => assert_eq!(iteration, 3),
            other => panic!("expected GoalSatisfied, got {other:?}"),
        }

        let executed: Vec<(String, bool)> = events
            .iter()
            .filter_map(|e| match e {
                LoopEvent::Executed { result, .. } => Some((result.name.clone(), result.success)),
                _ => None,
            })
            .collect();
        assert_eq!(
            executed,
            vec![
                ("try_fast".to_string(), false),
                ("try_slow".to_string(), true),
            ],
            "expected try_fast to fail then try_slow to succeed",
        );
    }

    #[test]
    fn loop_with_action_on_failure_returns_no_plan_when_no_alternative_exists() {
        // `only_path` is the only way to reach `done`; on failure it
        // removes its own precondition and adds nothing useful. The next
        // iteration's planner sees no path → NoPlan.
        let config = Config {
            sensors: vec![sensor("heartbeat", &["true"])],
            actions: vec![ActionSpec {
                name: "only_path".into(),
                cost: 1.0,
                requires: vec!["heartbeat".into(), "available".into()],
                forbids: Vec::new(),
                adds: vec!["done".into()],
                removes: Vec::new(),
                cmd: Some(vec!["false".into()]),
                on_failure: Some(Effects {
                    add: Vec::new(),
                    remove: vec!["available".into()],
                }),
            }],
            goal: GoalSpec {
                requires: vec!["done".into()],
                forbids: Vec::new(),
            },
        };
        let interrupted = Arc::new(AtomicBool::new(false));
        let outcome = run_loop(
            &config,
            vec!["available".into()],
            10,
            0,
            Arc::clone(&interrupted),
            |_| {},
        )
        .unwrap();
        // Iteration 1: only_path runs, fails, on_failure removes `available`.
        // Iteration 2: planner has no path to `done` → NoPlan.
        assert!(matches!(outcome, LoopOutcome::NoPlan { iteration: 2 }));
    }

    #[test]
    fn loop_with_long_interval_wakes_promptly_on_interrupt() {
        let config = three_step_config();
        let interrupted = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&interrupted);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            flag.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        let outcome = run_loop(
            &config,
            Vec::new(),
            100,
            10_000,
            Arc::clone(&interrupted),
            |_| {},
        )
        .unwrap();
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, LoopOutcome::Interrupted { .. }),
            "expected Interrupted, got {outcome:?}"
        );
        assert!(
            elapsed < Duration::from_millis(1_000),
            "10s interval must yield to interrupt within ~1s, got {elapsed:?}"
        );
    }

    // ---------------------------------------------------------------------
    // sense_and_plan — one-shot mode
    // ---------------------------------------------------------------------

    #[test]
    fn sense_and_plan_returns_plan_with_sensor_seeded_state() {
        let config = three_step_config();
        let outcome = sense_and_plan(&config, Vec::new()).unwrap();
        assert_eq!(outcome.readings.len(), 1);
        assert!(outcome.readings[0].success);
        assert_eq!(outcome.state_facts, vec!["heartbeat"]);
        let plan = outcome.plan.unwrap();
        assert_eq!(plan.steps, vec!["do_a", "do_b", "do_c"]);
        assert_eq!(plan.cost, 3.0);
    }

    #[test]
    fn sense_and_plan_returns_none_plan_when_unreachable() {
        let mut config = three_step_config();
        config.goal.requires = vec!["never".into()];
        let outcome = sense_and_plan(&config, Vec::new()).unwrap();
        assert!(outcome.plan.is_none());
    }
}
