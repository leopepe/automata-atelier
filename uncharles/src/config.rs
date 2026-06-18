use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub sensors: Vec<SensorSpec>,
    pub actions: Vec<ActionSpec>,
    pub goal: GoalSpec,
}

/// Declarative sensor: a shell command whose exit status drives state mutation.
///
/// If `on_success`/`on_failure` are omitted, the default is:
/// - exit 0 → add the fact named after the sensor
/// - non-zero → remove the fact named after the sensor
///
/// Override either side to model richer outcomes (e.g. a build sensor that
/// adds `tests_failing` on failure rather than just removing `tests_passing`).
///
/// Set `capture: stdout` to opt the sensor into value capture: when the
/// command exits successfully, its trimmed stdout is stored in the runtime
/// `Values` map under this sensor's `name`. Action commands then receive the
/// value as `UNCHARLES_FACT_<NAME>=<value>` whenever the action's `requires`
/// list mentions a fact with a captured value. See ADR 0003.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorSpec {
    pub name: String,
    pub cmd: Vec<String>,
    #[serde(default)]
    pub on_success: Option<Effects>,
    #[serde(default)]
    pub on_failure: Option<Effects>,
    #[serde(default)]
    pub capture: Option<Capture>,
}

/// Source of a sensor's captured value.
///
/// `stdout` is the only variant in v1: when the sensor command exits
/// successfully, its stdout (trimmed of trailing whitespace) becomes the
/// captured value. Future variants (`stderr`, structured shapes) would extend
/// this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Capture {
    /// Capture the trimmed stdout of the sensor command.
    Stdout,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effects {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSpec {
    pub name: String,
    pub cost: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Negative preconditions: facts that must be **absent** for the
    /// action to fire. Mirrors `GoalSpec.forbids`. Lets configs express
    /// "do this only when X is not present" without inventing a
    /// synthetic sensor that observes the negative shape — see
    /// `pendrive_audit.yaml`'s migration of the old `eject_pending` /
    /// `tools_not_installed` proxies.
    ///
    /// Validated for non-overlap with `requires` at config load time
    /// (see [`Config::validate`]); a fact in both fields would make
    /// the action structurally unsatisfiable.
    #[serde(default)]
    pub forbids: Vec<String>,
    #[serde(default)]
    pub adds: Vec<String>,
    #[serde(default)]
    pub removes: Vec<String>,
    /// Command the runtime executes for this action in `--execute` mode.
    /// Optional: actions without `cmd` are valid in one-shot planning mode
    /// (where the planner only computes a sequence of names) but cause
    /// [`crate::run::RunError::ActionMissingCmd`] if the loop tries to run
    /// them.
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    /// State mutation applied when the action's `cmd` exits non-zero.
    ///
    /// Default (omitted): non-zero exit terminates the loop with
    /// [`crate::run::LoopOutcome::ActionFailed`]. With `on_failure` set:
    /// the listed adds/removes are applied to state, the loop sleeps for
    /// `--interval-ms` and replans from the new state on the next
    /// iteration. The action's own `adds`/`removes` are *not* applied —
    /// those describe the success-path contract the planner assumes.
    ///
    /// Use this to express recoverable failure: the action that tried
    /// the cheap path declares what state to leave behind for the
    /// planner to route around (`add: [needs_human]`, `remove: [path_a]`),
    /// and an alternative action picks up from there.
    #[serde(default)]
    pub on_failure: Option<Effects>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalSpec {
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub forbids: Vec<String>,
}

/// Errors returned by [`Config::validate`].
#[derive(Debug)]
pub enum ConfigError {
    /// An action declares the same fact in both `requires` and `forbids`,
    /// which would make it structurally unsatisfiable.
    ActionContradiction { action: String, facts: Vec<String> },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActionContradiction { action, facts } => {
                let list = facts.join(", ");
                write!(
                    f,
                    "action `{action}` lists `{list}` in both `requires` and `forbids` — \
                     the action could never fire. Drop the fact from one of the lists."
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Run static validation on a parsed config. Currently checks:
    ///
    /// - No action declares the same fact in both `requires` and
    ///   `forbids` (would be structurally unsatisfiable).
    ///
    /// Called by `main.rs` after YAML parse, before either `run` or
    /// `inspect` consumes the config. Future config-level invariants
    /// belong here so they're enforced uniformly across subcommands.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for action in &self.actions {
            let requires: std::collections::BTreeSet<&str> =
                action.requires.iter().map(String::as_str).collect();
            let overlap: Vec<String> = action
                .forbids
                .iter()
                .filter(|f| requires.contains(f.as_str()))
                .cloned()
                .collect();
            if !overlap.is_empty() {
                let mut overlap = overlap;
                overlap.sort();
                return Err(ConfigError::ActionContradiction {
                    action: action.name.clone(),
                    facts: overlap,
                });
            }
        }
        Ok(())
    }
}

impl SensorSpec {
    pub fn effects_for(&self, success: bool) -> Effects {
        if success {
            self.on_success.clone().unwrap_or_else(|| Effects {
                add: vec![self.name.clone()],
                remove: Vec::new(),
            })
        } else {
            self.on_failure.clone().unwrap_or_else(|| Effects {
                add: Vec::new(),
                remove: vec![self.name.clone()],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // ActionSpec.forbids — YAML parsing + validation
    // ---------------------------------------------------------------------

    #[test]
    fn parses_action_with_forbids_field() {
        let yaml = r#"
            sensors: []
            actions:
              - name: eject_now
                cost: 1.0
                requires: [audit_sealed]
                forbids: [pendrive_mounted]
                adds: [eject_done]
                cmd: ["true"]
            goal:
              requires: [eject_done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.actions[0].requires, vec!["audit_sealed"]);
        assert_eq!(config.actions[0].forbids, vec!["pendrive_mounted"]);
    }

    #[test]
    fn forbids_defaults_to_empty_when_omitted() {
        let yaml = r#"
            sensors: []
            actions:
              - name: noop
                cost: 1.0
                cmd: ["true"]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.actions[0].forbids.is_empty());
    }

    #[test]
    fn validate_accepts_action_with_disjoint_requires_and_forbids() {
        let yaml = r#"
            sensors: []
            actions:
              - name: act
                cost: 1.0
                requires: [a]
                forbids: [b]
                adds: [c]
                cmd: ["true"]
            goal:
              requires: [c]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_action_with_overlapping_requires_and_forbids() {
        let yaml = r#"
            sensors: []
            actions:
              - name: contradiction
                cost: 1.0
                requires: [foo, bar]
                forbids: [foo]
                adds: [done]
                cmd: ["true"]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let err = config.validate().expect_err("expected ActionContradiction");
        match err {
            ConfigError::ActionContradiction { action, facts } => {
                assert_eq!(action, "contradiction");
                assert_eq!(facts, vec!["foo"]);
            }
        }
    }

    #[test]
    fn validate_reports_all_overlapping_facts_in_a_single_action() {
        let yaml = r#"
            sensors: []
            actions:
              - name: tangled
                cost: 1.0
                requires: [a, b, c]
                forbids: [b, a]
                adds: [done]
                cmd: ["true"]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let err = config.validate().expect_err("expected ActionContradiction");
        match err {
            ConfigError::ActionContradiction { action, facts } => {
                assert_eq!(action, "tangled");
                // sorted for stable error output
                assert_eq!(facts, vec!["a", "b"]);
            }
        }
    }

    #[test]
    fn validate_error_message_names_action_and_facts() {
        let err = ConfigError::ActionContradiction {
            action: "act".into(),
            facts: vec!["x".into(), "y".into()],
        };
        let msg = err.to_string();
        assert!(msg.contains("`act`"));
        assert!(msg.contains("`x, y`"));
        assert!(msg.contains("requires"));
        assert!(msg.contains("forbids"));
    }

    // ---------------------------------------------------------------------
    // SensorSpec::effects_for — default and custom mappings
    // ---------------------------------------------------------------------

    #[test]
    fn default_effects_add_named_fact_on_success() {
        let spec = SensorSpec {
            name: "ready".into(),
            cmd: vec!["true".into()],
            on_success: None,
            on_failure: None,
            capture: None,
        };
        let effects = spec.effects_for(true);
        assert_eq!(effects.add, vec!["ready"]);
        assert!(effects.remove.is_empty());
    }

    #[test]
    fn default_effects_remove_named_fact_on_failure() {
        let spec = SensorSpec {
            name: "ready".into(),
            cmd: vec!["false".into()],
            on_success: None,
            on_failure: None,
            capture: None,
        };
        let effects = spec.effects_for(false);
        assert!(effects.add.is_empty());
        assert_eq!(effects.remove, vec!["ready"]);
    }

    #[test]
    fn custom_on_success_overrides_default() {
        let spec = SensorSpec {
            name: "ready".into(),
            cmd: vec!["true".into()],
            on_success: Some(Effects {
                add: vec!["ok".into(), "warm".into()],
                remove: vec!["cold".into()],
            }),
            on_failure: None,
            capture: None,
        };
        let effects = spec.effects_for(true);
        assert_eq!(effects.add, vec!["ok", "warm"]);
        assert_eq!(effects.remove, vec!["cold"]);
    }

    // ---------------------------------------------------------------------
    // YAML deserialisation — round-trips, defaults, and rejection
    // ---------------------------------------------------------------------

    #[test]
    fn parses_minimal_config() {
        let yaml = r#"
            sensors: []
            actions:
              - name: noop
                cost: 1.0
                cmd: ["true"]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.sensors.len(), 0);
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].name, "noop");
        assert_eq!(config.actions[0].cost, 1.0);
        assert_eq!(config.goal.requires, vec!["done"]);
        assert!(config.goal.forbids.is_empty());
    }

    #[test]
    fn parses_sensor_with_explicit_on_success_and_on_failure() {
        let yaml = r#"
            sensors:
              - name: build
                cmd: ["cargo", "build"]
                on_success:
                  add: [built]
                  remove: [build_failing]
                on_failure:
                  add: [build_failing]
                  remove: [built]
            actions: []
            goal:
              requires: [built]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let s = &config.sensors[0];
        assert_eq!(s.name, "build");
        assert_eq!(s.cmd, vec!["cargo", "build"]);
        let success = s.on_success.as_ref().unwrap();
        assert_eq!(success.add, vec!["built"]);
        assert_eq!(success.remove, vec!["build_failing"]);
        let failure = s.on_failure.as_ref().unwrap();
        assert_eq!(failure.add, vec!["build_failing"]);
        assert_eq!(failure.remove, vec!["built"]);
    }

    #[test]
    fn parses_action_with_on_failure_clause() {
        let yaml = r#"
            sensors: []
            actions:
              - name: try_fast
                cost: 1.0
                requires: [available]
                adds: [done]
                cmd: ["false"]
                on_failure:
                  add: [needs_human]
                  remove: [available]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let on_failure = config.actions[0].on_failure.as_ref().unwrap();
        assert_eq!(on_failure.add, vec!["needs_human"]);
        assert_eq!(on_failure.remove, vec!["available"]);
    }

    #[test]
    fn action_on_failure_defaults_to_none() {
        let yaml = r#"
            sensors: []
            actions:
              - name: noop
                cost: 1.0
                cmd: ["true"]
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.actions[0].on_failure.is_none());
    }

    #[test]
    fn parses_sensor_with_capture_stdout() {
        let yaml = r#"
            sensors:
              - name: target_sha
                cmd: ["git", "rev-parse", "origin/main"]
                capture: stdout
            actions: []
            goal:
              requires: [target_sha]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.sensors[0].capture, Some(Capture::Stdout));
    }

    #[test]
    fn sensor_capture_defaults_to_none() {
        let yaml = r#"
            sensors:
              - name: ready
                cmd: ["true"]
            actions: []
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.sensors[0].capture.is_none());
    }

    #[test]
    fn rejects_unknown_capture_variant() {
        // ADR 0003 commits to `stdout` as the only v1 variant. Future
        // variants (`stderr`, structured) are reserved; mistypes must fail
        // loudly at config-load time rather than silently disabling capture.
        let yaml = r#"
            sensors:
              - name: target_sha
                cmd: ["true"]
                capture: bogus
            actions: []
            goal:
              requires: [target_sha]
        "#;
        let err = serde_yaml::from_str::<Config>(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bogus") || msg.contains("variant"),
            "expected unknown-variant error, got: {msg}",
        );
    }

    #[test]
    fn parses_action_without_cmd_for_pure_planning() {
        let yaml = r#"
            sensors: []
            actions:
              - name: imaginary
                cost: 2.5
                requires: [a]
                adds: [b]
            goal:
              requires: [b]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let a = &config.actions[0];
        assert_eq!(a.name, "imaginary");
        assert_eq!(a.cost, 2.5);
        assert_eq!(a.requires, vec!["a"]);
        assert_eq!(a.adds, vec!["b"]);
        assert!(a.removes.is_empty());
        assert!(a.cmd.is_none());
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        // deny_unknown_fields catches typos in the schema.
        let yaml = r#"
            sensors: []
            actions: []
            goal:
              requires: [done]
            unrelated_field: 42
        "#;
        let err = serde_yaml::from_str::<Config>(yaml).unwrap_err();
        assert!(
            err.to_string().contains("unrelated_field"),
            "expected error about unknown field, got: {err}"
        );
    }

    #[test]
    fn rejects_unknown_field_in_sensor() {
        let yaml = r#"
            sensors:
              - name: s
                cmd: ["true"]
                bogus: 1
            actions: []
            goal:
              requires: [done]
        "#;
        let err = serde_yaml::from_str::<Config>(yaml).unwrap_err();
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn empty_effects_struct_is_valid_and_suppresses_default() {
        // {} is a useful pattern: "this sensor observed something but
        // produces no state effect".
        let yaml = r#"
            sensors:
              - name: noisy
                cmd: ["true"]
                on_success: {}
            actions: []
            goal:
              requires: [done]
        "#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let success = config.sensors[0].on_success.as_ref().unwrap();
        assert!(success.add.is_empty());
        assert!(success.remove.is_empty());
    }

    #[test]
    fn requires_goal_section() {
        let yaml = r#"
            sensors: []
            actions: []
        "#;
        let err = serde_yaml::from_str::<Config>(yaml).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("goal"));
    }
}
