//! `uncharles inspect` — load a YAML config and print the static structure
//! plus the bounded reachable state-action graph, without running any
//! sensors or actions.
//!
//! See `docs/issues.md` for the design rationale; this is the implementation
//! of issue #22 (subcommand for visual debugging of configs).
//!
//! No commands run. The "initial state" used to seed exploration is
//! *simulated* by applying each sensor's `on_success` effects in declaration
//! order, plus any extra facts from `--have <fact>`. This produces a
//! deterministic starting state derived from the config alone — useful for
//! "what would the planner see if every sensor reported success?" debugging
//! without the side effects of actually running the sensors.

use std::collections::BTreeSet;
use std::fmt::Write;

use goap_planner::{Goal, Planner, State, StateGraph};

use crate::config::{ActionSpec, Config, GoalSpec, SensorSpec};
use crate::run::{build_actions, build_goal};

/// Construct a planner-ready initial state from a config without running
/// anything: apply each sensor's `on_success` effects in YAML declaration
/// order, then layer the extra `have` facts on top.
///
/// This is the "best-case" simulation: every sensor is treated as if it
/// returned success. It is *not* what the live runtime would see — for
/// that, run `uncharles run` (which actually executes sensor commands).
pub fn simulate_initial_state(config: &Config, extra_have: &[String]) -> State {
    let mut state = State::new();
    for sensor in &config.sensors {
        let effects = sensor.effects_for(true);
        for fact in &effects.add {
            state.insert(fact);
        }
        for fact in &effects.remove {
            state.remove(fact);
        }
    }
    for fact in extra_have {
        state.insert(fact);
    }
    state
}

/// Static-analysis findings for a config — issues that don't depend on the
/// state-space graph and can be derived from the spec lists alone, plus
/// dead-end states that fall out of the explored graph.
pub struct StaticAnalysis {
    /// Action names whose preconditions reference a fact no producer
    /// (sensor's `on_success.add` or another action's `adds`) can produce.
    /// The fact is the offending one.
    pub orphan_actions: Vec<(String, String)>,
    /// Goal-required facts that no producer can produce. Same shape as
    /// orphan actions, but for the goal.
    pub unreachable_goal_facts: Vec<String>,
    /// Indices into `StateGraph::states` for states with no outgoing edges
    /// that don't satisfy the goal — places exploration ran out of moves
    /// without succeeding.
    pub dead_end_states: Vec<usize>,
}

impl StaticAnalysis {
    pub fn is_clean(&self) -> bool {
        self.orphan_actions.is_empty()
            && self.unreachable_goal_facts.is_empty()
            && self.dead_end_states.is_empty()
    }
}

/// Run all three static analyses against a config and an explored graph.
pub fn analyse(config: &Config, graph: &StateGraph) -> StaticAnalysis {
    let producers = collect_producers(&config.sensors, &config.actions);

    let orphan_actions: Vec<(String, String)> = config
        .actions
        .iter()
        .flat_map(|a| {
            a.requires
                .iter()
                .filter(|fact| !producers.contains(*fact))
                .map(|fact| (a.name.clone(), fact.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let unreachable_goal_facts: Vec<String> = config
        .goal
        .requires
        .iter()
        .filter(|fact| !producers.contains(*fact))
        .cloned()
        .collect();

    let goal_set: BTreeSet<usize> = graph.goal_satisfying.iter().copied().collect();
    let mut dead_end_states: Vec<usize> = (0..graph.states.len())
        .filter(|i| graph.is_dead_end(*i) && !goal_set.contains(i))
        .collect();
    dead_end_states.sort();

    StaticAnalysis {
        orphan_actions,
        unreachable_goal_facts,
        dead_end_states,
    }
}

/// The set of facts any producer (a sensor's `on_success` add list or an
/// action's `adds` list) can introduce. Used by `analyse` to detect
/// preconditions and goal-required facts with no source.
fn collect_producers(sensors: &[SensorSpec], actions: &[ActionSpec]) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for s in sensors {
        for fact in &s.effects_for(true).add {
            set.insert(fact.clone());
        }
    }
    for a in actions {
        for fact in &a.adds {
            set.insert(fact.clone());
        }
    }
    set
}

/// Render a complete inspection report as plain text. Composed of:
///
/// 1. Sensors — name, effects on success/failure.
/// 2. Actions — preconditions, effects, costs.
/// 3. Goal — required and forbidden facts.
/// 4. Initial state — simulated, with note about how it was derived.
/// 5. State-action graph — every reachable state with outgoing edges.
/// 6. Static analysis — orphans, unreachable goal facts, dead-ends.
pub fn render_text(
    config: &Config,
    initial: &State,
    graph: &StateGraph,
    analysis: &StaticAnalysis,
) -> String {
    let mut out = String::new();
    render_sensors(&mut out, &config.sensors);
    render_actions(&mut out, &config.actions);
    render_goal(&mut out, &config.goal);
    render_initial_state(&mut out, initial);
    render_state_graph(&mut out, graph);
    render_analysis(&mut out, analysis, graph);
    out
}

// ---------------------------------------------------------------------------
// Section renderers
// ---------------------------------------------------------------------------

fn render_sensors(out: &mut String, sensors: &[SensorSpec]) {
    let _ = writeln!(out, "== sensors ({}) ==", sensors.len());
    if sensors.is_empty() {
        let _ = writeln!(out, "  (none)");
    }
    for s in sensors {
        let success = s.effects_for(true);
        let failure = s.effects_for(false);
        let _ = writeln!(out, "  {}", s.name);
        let _ = writeln!(out, "    cmd:        {}", s.cmd.join(" "));
        let _ = writeln!(
            out,
            "    on success: {}",
            format_effects(&success.add, &success.remove)
        );
        let _ = writeln!(
            out,
            "    on failure: {}",
            format_effects(&failure.add, &failure.remove)
        );
    }
    let _ = writeln!(out);
}

fn render_actions(out: &mut String, actions: &[ActionSpec]) {
    let _ = writeln!(out, "== actions ({}) ==", actions.len());
    if actions.is_empty() {
        let _ = writeln!(out, "  (none)");
    }
    for a in actions {
        let _ = writeln!(out, "  {} [cost {}]", a.name, a.cost);
        let _ = writeln!(out, "    requires: {}", join_or_none(&a.requires));
        let _ = writeln!(out, "    adds:     {}", join_or_none(&a.adds));
        let _ = writeln!(out, "    removes:  {}", join_or_none(&a.removes));
    }
    let _ = writeln!(out);
}

fn render_goal(out: &mut String, goal: &GoalSpec) {
    let _ = writeln!(out, "== goal ==");
    let _ = writeln!(out, "  requires: {}", join_or_none(&goal.requires));
    let _ = writeln!(out, "  forbids:  {}", join_or_none(&goal.forbids));
    let _ = writeln!(out);
}

fn render_initial_state(out: &mut String, initial: &State) {
    let _ = writeln!(out, "== initial state (simulated) ==");
    let _ = writeln!(
        out,
        "  derived by applying each sensor's on_success effects in declaration"
    );
    let _ = writeln!(
        out,
        "  order, then any --have facts. No sensor commands were run."
    );
    let mut facts: Vec<String> = initial.facts().map(String::from).collect();
    facts.sort();
    if facts.is_empty() {
        let _ = writeln!(out, "  facts: (empty)");
    } else {
        let _ = writeln!(out, "  facts: {}", facts.join(", "));
    }
    let _ = writeln!(out);
}

fn render_state_graph(out: &mut String, graph: &StateGraph) {
    let goal_set: BTreeSet<usize> = graph.goal_satisfying.iter().copied().collect();
    let _ = writeln!(out, "== state-action graph ==");
    let _ = writeln!(
        out,
        "  {} states, {} transitions, {} goal-satisfying{}",
        graph.states.len(),
        graph.edges.len(),
        graph.goal_satisfying.len(),
        if graph.truncated {
            " (TRUNCATED — max_states reached)"
        } else {
            ""
        }
    );
    let _ = writeln!(out);

    for (i, node) in graph.states.iter().enumerate() {
        let mut tags: Vec<&str> = Vec::new();
        if i == graph.initial {
            tags.push("initial");
        }
        if goal_set.contains(&i) {
            tags.push("goal ✓");
        }
        let suffix = if tags.is_empty() {
            String::new()
        } else {
            format!("  ({})", tags.join(", "))
        };
        let _ = writeln!(out, "  S{i} = {}{}", format_facts(&node.facts), suffix);

        let mut out_edges: Vec<_> = graph.outgoing(i).collect();
        out_edges.sort_by(|a, b| a.action.cmp(&b.action));
        for e in out_edges {
            let _ = writeln!(out, "    → {} [{}] → S{}", e.action, e.cost, e.to);
        }
    }
    let _ = writeln!(out);
}

fn render_analysis(out: &mut String, analysis: &StaticAnalysis, graph: &StateGraph) {
    let _ = writeln!(out, "== static analysis ==");
    if analysis.is_clean() {
        let _ = writeln!(out, "  ✓ no orphan actions");
        let _ = writeln!(out, "  ✓ no unreachable goal facts");
        let _ = writeln!(out, "  ✓ no dead-end states");
        return;
    }

    if analysis.orphan_actions.is_empty() {
        let _ = writeln!(out, "  ✓ no orphan actions");
    } else {
        let _ = writeln!(
            out,
            "  ✗ orphan actions ({}):",
            analysis.orphan_actions.len()
        );
        for (action, fact) in &analysis.orphan_actions {
            let _ = writeln!(
                out,
                "    - `{action}` requires `{fact}` but no sensor or action produces it"
            );
        }
    }

    if analysis.unreachable_goal_facts.is_empty() {
        let _ = writeln!(out, "  ✓ no unreachable goal facts");
    } else {
        let _ = writeln!(
            out,
            "  ✗ unreachable goal facts ({}):",
            analysis.unreachable_goal_facts.len()
        );
        for fact in &analysis.unreachable_goal_facts {
            let _ = writeln!(out, "    - `{fact}` has no producer");
        }
    }

    if analysis.dead_end_states.is_empty() {
        let _ = writeln!(out, "  ✓ no dead-end states");
    } else {
        let _ = writeln!(
            out,
            "  ✗ dead-end states ({}):",
            analysis.dead_end_states.len()
        );
        for &i in &analysis.dead_end_states {
            let _ = writeln!(
                out,
                "    - S{i} = {} (no applicable actions, not goal-satisfying)",
                format_facts(&graph.states[i].facts)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_string()
    } else {
        items.join(", ")
    }
}

fn format_effects(add: &[String], remove: &[String]) -> String {
    let mut parts = Vec::new();
    for f in add {
        parts.push(format!("+{f}"));
    }
    for f in remove {
        parts.push(format!("-{f}"));
    }
    if parts.is_empty() {
        "(no effect)".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_facts(facts: &BTreeSet<String>) -> String {
    if facts.is_empty() {
        "{}".to_string()
    } else {
        let joined: Vec<&str> = facts.iter().map(String::as_str).collect();
        format!("{{{}}}", joined.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Top-level driver
// ---------------------------------------------------------------------------

/// Build the full inspection report for a config: simulate the initial
/// state, run [`Planner::explore_for_goal`], compute static analysis, and
/// return everything ready for rendering.
///
/// `max_states` overrides the planner's default cap; `None` uses the
/// default (10 000).
pub fn inspect(
    config: &Config,
    extra_have: &[String],
    max_states: Option<usize>,
) -> (State, StateGraph, StaticAnalysis) {
    let initial = simulate_initial_state(config, extra_have);
    let actions = build_actions(&config.actions);
    let goal: Goal = build_goal(&config.goal);

    let mut planner = Planner::new(actions);
    if let Some(cap) = max_states {
        planner = planner.with_max_states(cap);
    }
    let graph = planner.explore_for_goal(&initial, &goal);
    let analysis = analyse(config, &graph);
    (initial, graph, analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Effects;

    fn config(yaml: &str) -> Config {
        serde_yaml::from_str(yaml).unwrap()
    }

    // -----------------------------------------------------------------------
    // simulate_initial_state
    // -----------------------------------------------------------------------

    #[test]
    fn simulate_applies_each_sensor_default_on_success_add() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
              - name: b
                cmd: ["true"]
            actions: []
            goal:
              requires: [done]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let mut facts: Vec<String> = initial.facts().map(String::from).collect();
        facts.sort();
        assert_eq!(facts, vec!["a", "b"]);
    }

    #[test]
    fn simulate_respects_explicit_on_success_effects() {
        let c = config(
            r#"
            sensors:
              - name: build
                cmd: ["cargo", "build"]
                on_success:
                  add: [built, fresh]
                  remove: [stale]
            actions: []
            goal:
              requires: [done]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let mut facts: Vec<String> = initial.facts().map(String::from).collect();
        facts.sort();
        // Only `built` and `fresh` were added; `stale` was never present so
        // the remove is a no-op.
        assert_eq!(facts, vec!["built", "fresh"]);
    }

    #[test]
    fn simulate_layers_extra_have_facts_on_top() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions: []
            goal:
              requires: [done]
            "#,
        );
        let extra = vec!["foo".to_string(), "bar".to_string()];
        let initial = simulate_initial_state(&c, &extra);
        let mut facts: Vec<String> = initial.facts().map(String::from).collect();
        facts.sort();
        assert_eq!(facts, vec!["a", "bar", "foo"]);
    }

    #[test]
    fn simulate_with_no_sensors_and_no_have_yields_empty_state() {
        let c = config(
            r#"
            sensors: []
            actions:
              - name: noop
                cost: 1.0
                cmd: ["true"]
            goal:
              requires: [done]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        assert!(initial.is_empty());
    }

    // -----------------------------------------------------------------------
    // analyse — orphan actions
    // -----------------------------------------------------------------------

    #[test]
    fn analyse_flags_orphan_actions() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: needs_b
                cost: 1.0
                requires: [b]
                adds: [c]
                cmd: ["true"]
            goal:
              requires: [c]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let actions = build_actions(&c.actions);
        let goal = build_goal(&c.goal);
        let graph = Planner::new(actions).explore_for_goal(&initial, &goal);
        let analysis = analyse(&c, &graph);

        assert_eq!(analysis.orphan_actions.len(), 1);
        assert_eq!(analysis.orphan_actions[0].0, "needs_b");
        assert_eq!(analysis.orphan_actions[0].1, "b");
    }

    #[test]
    fn analyse_recognises_action_chain_as_producer() {
        // a-action produces b; b-action requires b. Not orphan.
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: a_action
                cost: 1.0
                requires: [a]
                adds: [b]
                cmd: ["true"]
              - name: b_action
                cost: 1.0
                requires: [b]
                adds: [c]
                cmd: ["true"]
            goal:
              requires: [c]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let actions = build_actions(&c.actions);
        let goal = build_goal(&c.goal);
        let graph = Planner::new(actions).explore_for_goal(&initial, &goal);
        let analysis = analyse(&c, &graph);

        assert!(analysis.orphan_actions.is_empty());
    }

    // -----------------------------------------------------------------------
    // analyse — unreachable goal facts
    // -----------------------------------------------------------------------

    #[test]
    fn analyse_flags_unreachable_goal_facts() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: act
                cost: 1.0
                requires: [a]
                adds: [b]
                cmd: ["true"]
            goal:
              requires: [unreachable]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let actions = build_actions(&c.actions);
        let goal = build_goal(&c.goal);
        let graph = Planner::new(actions).explore_for_goal(&initial, &goal);
        let analysis = analyse(&c, &graph);

        assert_eq!(analysis.unreachable_goal_facts, vec!["unreachable"]);
    }

    // -----------------------------------------------------------------------
    // analyse — clean config
    // -----------------------------------------------------------------------

    #[test]
    fn analyse_clean_config_is_clean() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: act
                cost: 1.0
                requires: [a]
                adds: [done]
                removes: [a]
                cmd: ["true"]
            goal:
              requires: [done]
            "#,
        );
        let initial = simulate_initial_state(&c, &[]);
        let actions = build_actions(&c.actions);
        let goal = build_goal(&c.goal);
        let graph = Planner::new(actions).explore_for_goal(&initial, &goal);
        let analysis = analyse(&c, &graph);

        assert!(analysis.is_clean());
    }

    // -----------------------------------------------------------------------
    // render_text — smoke check on shape
    // -----------------------------------------------------------------------

    #[test]
    fn render_text_includes_all_sections() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: act
                cost: 1.0
                requires: [a]
                adds: [done]
                removes: [a]
                cmd: ["true"]
            goal:
              requires: [done]
            "#,
        );
        let (initial, graph, analysis) = inspect(&c, &[], None);
        let text = render_text(&c, &initial, &graph, &analysis);

        assert!(text.contains("== sensors ("));
        assert!(text.contains("== actions ("));
        assert!(text.contains("== goal =="));
        assert!(text.contains("== initial state"));
        assert!(text.contains("== state-action graph =="));
        assert!(text.contains("== static analysis =="));
    }

    #[test]
    fn render_text_marks_initial_and_goal_states() {
        let c = config(
            r#"
            sensors:
              - name: a
                cmd: ["true"]
            actions:
              - name: act
                cost: 1.0
                requires: [a]
                adds: [done]
                removes: [a]
                cmd: ["true"]
            goal:
              requires: [done]
            "#,
        );
        let (initial, graph, analysis) = inspect(&c, &[], None);
        let text = render_text(&c, &initial, &graph, &analysis);

        assert!(text.contains("(initial)"));
        assert!(text.contains("goal ✓"));
    }

    // -----------------------------------------------------------------------
    // helpers (small)
    // -----------------------------------------------------------------------

    #[test]
    fn format_effects_lists_adds_and_removes() {
        let s = format_effects(&["x".into(), "y".into()], &["z".into()]);
        assert_eq!(s, "+x, +y, -z");
    }

    #[test]
    fn format_effects_no_effect_when_empty() {
        let s = format_effects(&[], &[]);
        assert_eq!(s, "(no effect)");
    }

    #[test]
    fn collect_producers_unions_sensor_adds_and_action_adds() {
        let sensors = vec![SensorSpec {
            name: "ready".into(),
            cmd: vec!["true".into()],
            on_success: Some(Effects {
                add: vec!["a".into(), "b".into()],
                remove: vec![],
            }),
            on_failure: None,
        }];
        let actions = vec![ActionSpec {
            name: "act".into(),
            cost: 1.0,
            requires: vec![],
            adds: vec!["c".into()],
            removes: vec![],
            cmd: None,
            on_failure: None,
        }];
        let producers = collect_producers(&sensors, &actions);
        let v: Vec<_> = producers.into_iter().collect();
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
