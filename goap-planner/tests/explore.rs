//! Tests for [`Planner::explore`] and [`Planner::explore_for_goal`].
//!
//! `explore` is the inspection-time API. It returns the bounded state-action
//! graph reachable from a given initial state, with stable iteration order
//! and a `truncated` flag instead of an error when `max_states` is hit.
//! These tests pin the contract: shape of the returned graph, goal-tracking
//! semantics, dead-end detection, and truncation behaviour.

use std::collections::BTreeSet;

use goap_planner::{Action, Goal, Planner, State};

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

#[test]
fn empty_action_library_yields_singleton_graph() {
    let graph = Planner::new(Vec::new()).explore(&State::from_facts(["x"]));

    assert_eq!(graph.states.len(), 1);
    assert!(graph.edges.is_empty());
    assert_eq!(graph.initial, 0);
    assert!(graph.goal_satisfying.is_empty());
    assert!(!graph.truncated);

    let only = &graph.states[0];
    let expected: BTreeSet<String> = ["x".to_string()].into_iter().collect();
    assert_eq!(only.facts, expected);
}

#[test]
fn linear_chain_produces_one_edge_per_step() {
    // a → b → c → d via three actions.
    let actions = vec![
        Action::new("ab", 1.0).requires("a").adds("b").removes("a"),
        Action::new("bc", 1.0).requires("b").adds("c").removes("b"),
        Action::new("cd", 1.0).requires("c").adds("d").removes("c"),
    ];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    assert_eq!(graph.states.len(), 4);
    assert_eq!(graph.edges.len(), 3);
    assert!(!graph.truncated);
    assert!(graph.goal_satisfying.is_empty()); // goal-agnostic explore

    // Each state is the initial of exactly one outgoing edge except the last.
    let mut states_with_outgoing = std::collections::HashSet::new();
    for e in &graph.edges {
        states_with_outgoing.insert(e.from);
    }
    assert_eq!(states_with_outgoing.len(), 3);
}

#[test]
fn redundant_paths_keep_cheapest_action_per_edge() {
    // Two ways from {a} to {b}: via cheap_path (cost 1) and via expensive_path (cost 5).
    // explore should keep the cheaper one — that's the contract plan() depends on.
    let actions = vec![
        Action::new("cheap_path", 1.0)
            .requires("a")
            .adds("b")
            .removes("a"),
        Action::new("expensive_path", 5.0)
            .requires("a")
            .adds("b")
            .removes("a"),
    ];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    assert_eq!(graph.states.len(), 2);
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0].action, "cheap_path");
    assert_eq!(graph.edges[0].cost, 1.0);
}

#[test]
fn explore_visits_all_branches_in_wide_graph() {
    // From {start}, four actions each produce a distinct fact and consume start.
    // Each branch is a one-step dead-end.
    let actions = (0..4)
        .map(|i| {
            Action::new(format!("branch_{i}"), 1.0)
                .requires("start")
                .removes("start")
                .adds(format!("done_{i}"))
        })
        .collect::<Vec<_>>();
    let graph = Planner::new(actions).explore(&State::from_facts(["start"]));

    // 1 initial state + 4 leaf states.
    assert_eq!(graph.states.len(), 5);
    assert_eq!(graph.edges.len(), 4);

    // Initial has 4 outgoing edges; every leaf has 0.
    let outgoing_counts: Vec<usize> = (0..graph.states.len())
        .map(|i| graph.outgoing(i).count())
        .collect();
    let mut sorted = outgoing_counts.clone();
    sorted.sort();
    assert_eq!(sorted, vec![0, 0, 0, 0, 4]);
}

// ---------------------------------------------------------------------------
// Stable iteration order — explicit checks
// ---------------------------------------------------------------------------

#[test]
fn states_are_sorted_by_signature() {
    let actions = vec![
        Action::new("ab", 1.0).requires("a").adds("b"),
        Action::new("ac", 1.0).requires("a").adds("c"),
        Action::new("ad", 1.0).requires("a").adds("d"),
    ];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    let signatures: Vec<&str> = graph.states.iter().map(|s| s.signature.as_str()).collect();
    let mut sorted = signatures.clone();
    sorted.sort();
    assert_eq!(signatures, sorted);
}

#[test]
fn edges_are_sorted_by_from_action_to() {
    // Each action consumes "a" so they're one-shot from the initial state.
    let actions = vec![
        Action::new("z_action", 1.0)
            .requires("a")
            .adds("b")
            .removes("a"),
        Action::new("a_action", 1.0)
            .requires("a")
            .adds("c")
            .removes("a"),
    ];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    // Both edges originate from the {a}-state; "a_action" should come first.
    let action_names: Vec<&str> = graph.edges.iter().map(|e| e.action.as_str()).collect();
    assert_eq!(action_names, vec!["a_action", "z_action"]);
}

// ---------------------------------------------------------------------------
// Goal tracking via explore_for_goal
// ---------------------------------------------------------------------------

#[test]
fn explore_for_goal_records_goal_satisfying_states() {
    // Linear, fully-consuming chain: {axe} → {log} → {firewood}.
    let actions = vec![
        Action::new("chop", 5.0)
            .requires("axe")
            .adds("log")
            .removes("axe"),
        Action::new("split", 2.0)
            .requires("log")
            .adds("firewood")
            .removes("log"),
    ];
    let initial = State::from_facts(["axe"]);
    let goal = Goal::new().requires("firewood");
    let graph = Planner::new(actions).explore_for_goal(&initial, &goal);

    assert_eq!(graph.goal_satisfying.len(), 1);
    let goal_state = &graph.states[graph.goal_satisfying[0]];
    assert!(goal_state.facts.contains("firewood"));
    assert!(!goal_state.facts.contains("log"));
    assert!(!goal_state.facts.contains("axe"));
}

#[test]
fn explore_for_goal_initial_already_satisfies_records_initial() {
    let initial = State::from_facts(["done"]);
    let goal = Goal::new().requires("done");
    let graph = Planner::new(Vec::new()).explore_for_goal(&initial, &goal);

    assert_eq!(graph.states.len(), 1);
    assert_eq!(graph.goal_satisfying, vec![0]);
    assert_eq!(graph.initial, 0);
}

#[test]
fn explore_for_goal_unreachable_returns_empty_goal_satisfying() {
    // Goal requires a fact no action produces.
    let actions = vec![Action::new("step", 1.0).requires("a").adds("b")];
    let initial = State::from_facts(["a"]);
    let goal = Goal::new().requires("c"); // no producer
    let graph = Planner::new(actions).explore_for_goal(&initial, &goal);

    assert_eq!(graph.states.len(), 2); // initial + after-step
    assert!(graph.goal_satisfying.is_empty());
    assert!(!graph.truncated);
}

#[test]
fn explore_skips_goal_check_when_called_goal_agnostic() {
    let actions = vec![Action::new("step", 1.0).requires("a").adds("b")];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    assert!(graph.goal_satisfying.is_empty());
    // Even when the structurally-corresponding "goal {b}" would match a state,
    // the goal-agnostic API doesn't track it.
}

// ---------------------------------------------------------------------------
// Dead-end detection helper
// ---------------------------------------------------------------------------

#[test]
fn is_dead_end_identifies_states_with_no_outgoing() {
    // a → b is the only transition; b has no outgoing.
    let actions = vec![
        Action::new("step", 1.0)
            .requires("a")
            .adds("b")
            .removes("a"),
    ];
    let graph = Planner::new(actions).explore(&State::from_facts(["a"]));

    let initial_dead = graph.is_dead_end(graph.initial);
    let other_idx = (0..graph.states.len())
        .find(|&i| i != graph.initial)
        .unwrap();
    let other_dead = graph.is_dead_end(other_idx);

    assert!(!initial_dead);
    assert!(other_dead);
}

// ---------------------------------------------------------------------------
// Truncation behaviour
// ---------------------------------------------------------------------------

#[test]
fn truncation_sets_flag_instead_of_returning_error() {
    // Build a chain longer than max_states so BFS cannot complete.
    let actions: Vec<Action> = (0..50)
        .map(|i| {
            Action::new(format!("step_{i}"), 1.0)
                .requires(if i == 0 {
                    "start".to_string()
                } else {
                    format!("s_{}", i - 1)
                })
                .removes(if i == 0 {
                    "start".to_string()
                } else {
                    format!("s_{}", i - 1)
                })
                .adds(format!("s_{i}"))
        })
        .collect();
    let initial = State::from_facts(["start"]);
    let graph = Planner::new(actions).with_max_states(5).explore(&initial);

    assert!(graph.truncated);
    // BFS stopped early; we should have at most max_states + a small buffer
    // states discovered (the cap is checked at top of each iteration).
    assert!(
        graph.states.len() <= 10,
        "expected ≤ 10 states under max_states=5, got {}",
        graph.states.len()
    );
}

#[test]
fn no_truncation_when_state_space_fits() {
    let actions = vec![
        Action::new("ab", 1.0).requires("a").adds("b").removes("a"),
        Action::new("bc", 1.0).requires("b").adds("c").removes("b"),
    ];
    let graph = Planner::new(actions)
        .with_max_states(100)
        .explore(&State::from_facts(["a"]));

    assert!(!graph.truncated);
    assert_eq!(graph.states.len(), 3);
}

// ---------------------------------------------------------------------------
// Behavioural parity with plan()
// ---------------------------------------------------------------------------

#[test]
fn plan_uses_same_graph_explore_returns() {
    // explore_for_goal and plan should agree on whether the goal is reachable
    // and on the cheapest cost via Dijkstra.
    let actions = vec![
        Action::new("cheap", 1.0).requires("a").adds("b"),
        Action::new("expensive", 10.0).requires("a").adds("b"),
        Action::new("finish", 1.0).requires("b").adds("done"),
    ];
    let initial = State::from_facts(["a"]);
    let goal = Goal::new().requires("done");

    let planner = Planner::new(actions);
    let graph = planner.explore_for_goal(&initial, &goal);
    let plan = planner.plan(&initial, &goal).unwrap().unwrap();

    assert!(!graph.goal_satisfying.is_empty());
    assert_eq!(plan.steps, vec!["cheap", "finish"]);
    assert_eq!(plan.cost, 2.0);
}
