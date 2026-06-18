use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use goap_planner::{Action, Goal, Planner, State};

use crate::config::{ActionSpec, Capture, Config, SensorSpec};

/// Per-cycle map of fact name → captured string value.
///
/// Populated by sensors that opt into [`Capture::Stdout`] and consumed by
/// [`execute_action`] as `UNCHARLES_FACT_<NAME>` env vars on the child
/// process. Lives alongside [`State`] but is invisible to `goap-planner`'s
/// BFS — see ADR 0003 for the layering rationale.
pub type Values = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct SensorReading {
    pub name: String,
    pub success: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Trimmed stdout when the sensor declared `capture: stdout` and exited
    /// successfully; `None` otherwise.
    pub captured_value: Option<String>,
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

/// Execute a sensor command and compute its [`SensorReading`] **without
/// mutating any state**.
///
/// This is the pure-I/O half of sensing: it runs the command, captures
/// stdout when the sensor opts into [`Capture::Stdout`], and resolves the
/// effect lists (`added`/`removed`) via [`SensorSpec::effects_for`]. Applying
/// those effects to a [`State`]/[`Values`] is the caller's job — see
/// [`apply_reading`]. Splitting read from apply lets the actor runtime (ADR
/// 0005) run the blocking command off-thread in one actor and apply the
/// resulting reading in the world-state-owning actor.
pub fn read_sensor(spec: &SensorSpec) -> Result<SensorReading, RunError> {
    let cmd_name = spec.cmd.first().cloned().unwrap_or_default();
    let mut command = Command::new(&cmd_name);
    command.args(spec.cmd.iter().skip(1)).stderr(Stdio::null());

    let (success, captured_value) = match spec.capture {
        Some(Capture::Stdout) => {
            let output =
                command
                    .stdout(Stdio::piped())
                    .output()
                    .map_err(|e| RunError::SensorExec {
                        name: spec.name.clone(),
                        error: e.to_string(),
                    })?;
            let success = output.status.success();
            let captured = if success {
                Some(
                    String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string(),
                )
            } else {
                None
            };
            (success, captured)
        }
        None => {
            let status =
                command
                    .stdout(Stdio::null())
                    .status()
                    .map_err(|e| RunError::SensorExec {
                        name: spec.name.clone(),
                        error: e.to_string(),
                    })?;
            (status.success(), None)
        }
    };

    let effects = spec.effects_for(success);
    Ok(SensorReading {
        name: spec.name.clone(),
        success,
        added: effects.add,
        removed: effects.remove,
        captured_value,
    })
}

/// Apply a [`SensorReading`]'s effects to `state` and `values`.
///
/// The pure-mutation half of sensing (see [`read_sensor`]). Adds every fact
/// in `reading.added`, stores the captured value (if any) under the sensor's
/// name, and removes every fact in `reading.removed` — dropping its value too,
/// per ADR 0003's atomic-remove rule.
pub fn apply_reading(reading: &SensorReading, state: &mut State, values: &mut Values) {
    for fact in &reading.added {
        state.insert(fact.clone());
    }
    if let Some(ref v) = reading.captured_value {
        values.insert(reading.name.clone(), v.clone());
    }
    for fact in &reading.removed {
        state.remove(fact);
        values.remove(fact);
    }
}

/// Run a sensor and apply its effects in one step.
///
/// Thin composition of [`read_sensor`] + [`apply_reading`], retained for the
/// one-shot [`sense_and_plan`] path and as the unit under test for the sensor
/// effect contract. The actor runtime calls the two halves separately.
pub fn run_sensor(
    spec: &SensorSpec,
    state: &mut State,
    values: &mut Values,
) -> Result<SensorReading, RunError> {
    let reading = read_sensor(spec)?;
    apply_reading(&reading, state, values);
    Ok(reading)
}

pub fn execute_action(spec: &ActionSpec, values: &Values) -> Result<ActionResult, RunError> {
    let cmd = spec
        .cmd
        .as_ref()
        .ok_or_else(|| RunError::ActionMissingCmd {
            name: spec.name.clone(),
        })?;
    let cmd_name = cmd.first().cloned().unwrap_or_default();
    let mut command = Command::new(&cmd_name);
    command
        .args(cmd.iter().skip(1))
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    // Inject UNCHARLES_FACT_<NAME>=<value> for each `requires` fact that has
    // a captured value. Facts without values produce no env var (not an
    // empty one). See ADR 0003 for the contract.
    for fact in &spec.requires {
        if let Some(value) = values.get(fact) {
            command.env(fact_env_var(fact), value);
        }
    }

    let output = command.output().map_err(|e| RunError::ActionExec {
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

/// Map a fact name to the env-var name used for value injection.
///
/// `target_sha` → `UNCHARLES_FACT_TARGET_SHA`. Non-ASCII-alphanumeric
/// characters collapse to `_`. The mapping is lossy on purpose: two distinct
/// fact names that collide here would clash in the child environment, but
/// such names are exotic enough that the collision is acceptable
/// (documented in ADR 0003).
pub fn fact_env_var(fact: &str) -> String {
    let mut s = String::with_capacity("UNCHARLES_FACT_".len() + fact.len());
    s.push_str("UNCHARLES_FACT_");
    for c in fact.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_uppercase());
        } else {
            s.push('_');
        }
    }
    s
}

/// Find an [`ActionSpec`] by name, or return [`RunError::PlannerActionMismatch`].
///
/// Used by the executor actor (ADR 0005) to resolve a planner-named step back
/// to its config spec before running its `cmd`.
pub fn find_action_spec<'a>(
    specs: &'a [ActionSpec],
    name: &str,
) -> Result<&'a ActionSpec, RunError> {
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
    pub values: Values,
    pub plan: Option<goap_planner::Plan>,
}

pub fn sense_and_plan(config: &Config, seed: Vec<String>) -> Result<Outcome, RunError> {
    let mut state = State::from_facts(seed);
    let mut values = Values::new();

    let mut readings = Vec::with_capacity(config.sensors.len());
    for sensor in &config.sensors {
        readings.push(run_sensor(sensor, &mut state, &mut values)?);
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
        values,
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Effects, GoalSpec};

    fn sensor(name: &str, cmd: &[&str]) -> SensorSpec {
        SensorSpec {
            name: name.into(),
            cmd: cmd.iter().map(std::string::ToString::to_string).collect(),
            on_success: None,
            on_failure: None,
            capture: None,
        }
    }

    fn action(name: &str, cmd: &[&str], requires: &[&str], adds: &[&str]) -> ActionSpec {
        ActionSpec {
            name: name.into(),
            cost: 1.0,
            requires: requires
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            forbids: Vec::new(),
            adds: adds.iter().map(std::string::ToString::to_string).collect(),
            removes: Vec::new(),
            cmd: Some(cmd.iter().map(std::string::ToString::to_string).collect()),
            on_failure: None,
        }
    }

    // ---------------------------------------------------------------------
    // run_sensor — default effect mapping (success → add, failure → remove)
    // ---------------------------------------------------------------------

    #[test]
    fn sensor_success_adds_fact_named_after_sensor() {
        let mut state = State::new();
        let mut values = Values::new();
        let reading = run_sensor(&sensor("ready", &["true"]), &mut state, &mut values).unwrap();
        assert!(reading.success);
        assert_eq!(reading.added, vec!["ready"]);
        assert!(reading.removed.is_empty());
        assert!(reading.captured_value.is_none());
        assert!(state.contains("ready"));
        assert!(values.is_empty());
    }

    #[test]
    fn sensor_failure_removes_fact_named_after_sensor() {
        let mut state = State::from_facts(["ready"]);
        let mut values = Values::new();
        let reading = run_sensor(&sensor("ready", &["false"]), &mut state, &mut values).unwrap();
        assert!(!reading.success);
        assert_eq!(reading.removed, vec!["ready"]);
        assert!(reading.added.is_empty());
        assert!(!state.contains("ready"));
    }

    #[test]
    fn sensor_custom_on_success_overrides_default() {
        let mut state = State::new();
        let mut values = Values::new();
        let spec = SensorSpec {
            name: "build".into(),
            cmd: vec!["true".into()],
            on_success: Some(Effects {
                add: vec!["build_ok".into(), "tests_pass".into()],
                remove: vec!["build_failing".into()],
            }),
            on_failure: None,
            capture: None,
        };
        let mut state_with_failing = State::from_facts(["build_failing"]);
        let reading = run_sensor(&spec, &mut state_with_failing, &mut values).unwrap();
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
            capture: None,
        };
        run_sensor(&suppress, &mut state, &mut values).unwrap();
        assert!(!state.contains("noisy"));
    }

    #[test]
    fn sensor_missing_command_returns_sensor_exec_error() {
        let mut state = State::new();
        let mut values = Values::new();
        let err = run_sensor(
            &sensor("ghost", &["definitely-not-a-real-binary-xyz123"]),
            &mut state,
            &mut values,
        )
        .unwrap_err();
        assert!(matches!(err, RunError::SensorExec { ref name, .. } if name == "ghost"));
    }

    // ---------------------------------------------------------------------
    // run_sensor — capture: stdout (ADR 0003)
    // ---------------------------------------------------------------------

    #[test]
    fn sensor_with_capture_stdout_stores_trimmed_value_on_success() {
        let mut state = State::new();
        let mut values = Values::new();
        let spec = SensorSpec {
            name: "target_sha".into(),
            cmd: vec!["sh".into(), "-c".into(), "printf 'abc123\\n'".into()],
            on_success: None,
            on_failure: None,
            capture: Some(Capture::Stdout),
        };
        let reading = run_sensor(&spec, &mut state, &mut values).unwrap();
        assert!(reading.success);
        assert_eq!(reading.captured_value.as_deref(), Some("abc123"));
        assert_eq!(values.get("target_sha").map(String::as_str), Some("abc123"));
        assert!(state.contains("target_sha"));
    }

    #[test]
    fn sensor_with_capture_stdout_does_not_store_value_on_failure() {
        let mut state = State::from_facts(["target_sha"]);
        let mut values = Values::new();
        values.insert("target_sha".into(), "stale".into());
        let spec = SensorSpec {
            name: "target_sha".into(),
            cmd: vec!["false".into()],
            on_success: None,
            on_failure: None,
            capture: Some(Capture::Stdout),
        };
        let reading = run_sensor(&spec, &mut state, &mut values).unwrap();
        assert!(!reading.success);
        assert!(reading.captured_value.is_none());
        // Default on_failure removes the named fact, which atomically drops
        // its value too — ADR 0003's atomic-remove rule.
        assert!(!state.contains("target_sha"));
        assert!(!values.contains_key("target_sha"));
    }

    #[test]
    fn sensor_without_capture_does_not_populate_values() {
        let mut state = State::new();
        let mut values = Values::new();
        let spec = sensor("ready", &["sh", "-c", "echo would-have-been-captured"]);
        let reading = run_sensor(&spec, &mut state, &mut values).unwrap();
        assert!(reading.success);
        assert!(reading.captured_value.is_none());
        assert!(values.is_empty());
    }

    #[test]
    fn sensor_capture_overwrites_prior_value() {
        let mut state = State::new();
        let mut values = Values::new();
        values.insert("target_sha".into(), "old".into());
        let spec = SensorSpec {
            name: "target_sha".into(),
            cmd: vec!["sh".into(), "-c".into(), "echo new".into()],
            on_success: None,
            on_failure: None,
            capture: Some(Capture::Stdout),
        };
        run_sensor(&spec, &mut state, &mut values).unwrap();
        assert_eq!(values.get("target_sha").map(String::as_str), Some("new"));
    }

    // ---------------------------------------------------------------------
    // execute_action — exit code, stderr capture, missing-cmd error
    // ---------------------------------------------------------------------

    #[test]
    fn execute_action_success_returns_zero_exit() {
        let values = Values::new();
        let result = execute_action(&action("noop", &["true"], &[], &[]), &values).unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn execute_action_failure_captures_exit_and_stderr() {
        let spec = action("failing", &["sh", "-c", "echo boom >&2; exit 7"], &[], &[]);
        let values = Values::new();
        let result = execute_action(&spec, &values).unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.stderr, "boom");
    }

    #[test]
    fn execute_action_injects_uncharles_fact_env_for_required_facts() {
        // Action with `requires: [target_sha]`. The cmd reads the env var
        // and prints it to stderr (which `execute_action` captures), so
        // the round-trip is observable via ActionResult.stderr.
        let spec = action(
            "deploy",
            &["sh", "-c", "echo $UNCHARLES_FACT_TARGET_SHA >&2"],
            &["target_sha"],
            &[],
        );
        let mut values = Values::new();
        values.insert("target_sha".into(), "abc123".into());
        let result = execute_action(&spec, &values).unwrap();
        assert!(
            result.success,
            "exit={:?} stderr={:?}",
            result.exit_code, result.stderr
        );
        assert_eq!(result.stderr, "abc123");
    }

    #[test]
    fn execute_action_does_not_inject_env_for_facts_without_values() {
        // `requires` mentions `available`, but no value exists for it.
        // The env var must be unset (printf prints empty).
        let spec = action(
            "noop",
            &[
                "sh",
                "-c",
                "printf '<%s>' \"${UNCHARLES_FACT_AVAILABLE-unset}\" >&2",
            ],
            &["available"],
            &[],
        );
        let values = Values::new();
        let result = execute_action(&spec, &values).unwrap();
        assert!(result.success);
        assert_eq!(result.stderr, "<unset>");
    }

    #[test]
    fn execute_action_does_not_inject_env_for_non_required_facts() {
        // Value exists for `target_sha` but the action does not require
        // it. Env var must NOT be set (privacy: an action cannot peek at
        // values it didn't declare a dependency on).
        let spec = action(
            "noop",
            &[
                "sh",
                "-c",
                "printf '<%s>' \"${UNCHARLES_FACT_TARGET_SHA-unset}\" >&2",
            ],
            &[],
            &[],
        );
        let mut values = Values::new();
        values.insert("target_sha".into(), "abc123".into());
        let result = execute_action(&spec, &values).unwrap();
        assert!(result.success);
        assert_eq!(result.stderr, "<unset>");
    }

    #[test]
    fn fact_env_var_uppercases_and_replaces_non_alphanumeric() {
        assert_eq!(fact_env_var("target_sha"), "UNCHARLES_FACT_TARGET_SHA");
        assert_eq!(fact_env_var("plan_clean"), "UNCHARLES_FACT_PLAN_CLEAN");
        assert_eq!(fact_env_var("a-b.c"), "UNCHARLES_FACT_A_B_C");
        assert_eq!(fact_env_var("simple"), "UNCHARLES_FACT_SIMPLE");
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
        let values = Values::new();
        let err = execute_action(&spec, &values).unwrap_err();
        assert!(matches!(err, RunError::ActionMissingCmd { ref name } if name == "no_cmd"));
    }

    #[test]
    fn execute_action_unknown_binary_returns_action_exec_error() {
        let spec = action("ghost", &["definitely-not-a-real-binary-xyz123"], &[], &[]);
        let values = Values::new();
        let err = execute_action(&spec, &values).unwrap_err();
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
