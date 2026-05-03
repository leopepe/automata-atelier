//! Tests for [`Action::forbids`] — negative preconditions on actions.
//!
//! Mirrors [`Goal::forbids`] semantics: a fact in `forbidden` disables the
//! action whenever it is present in the source state. These tests pin the
//! contract end-to-end — `applicable()` direct, planner picks correctly,
//! `explore` reflects the gating.

use goap_planner::{Action, Goal, Planner, State};

// ---------------------------------------------------------------------------
// Action::applicable — direct
// ---------------------------------------------------------------------------

#[test]
fn applicable_blocks_when_forbidden_fact_is_present() {
    let act = Action::new("eject", 1.0)
        .requires("ready")
        .forbids("locked")
        .adds("done");

    assert!(act.applicable(&State::from_facts(["ready"])));
    assert!(!act.applicable(&State::from_facts(["ready", "locked"])));
}

#[test]
fn applicable_passes_when_forbidden_fact_is_absent() {
    let act = Action::new("step", 1.0).forbids("blocker").adds("done");
    assert!(act.applicable(&State::new()));
    assert!(act.applicable(&State::from_facts(["unrelated"])));
}

#[test]
fn multiple_forbids_must_all_be_absent() {
    let act = Action::new("step", 1.0)
        .forbids("a")
        .forbids("b")
        .forbids("c")
        .adds("done");

    assert!(act.applicable(&State::new()));
    assert!(!act.applicable(&State::from_facts(["a"])));
    assert!(!act.applicable(&State::from_facts(["b"])));
    assert!(!act.applicable(&State::from_facts(["c"])));
    assert!(!act.applicable(&State::from_facts(["a", "b", "c"])));
}

#[test]
fn empty_forbids_does_not_block_anything() {
    // Backward-compat sanity: actions without forbids behave exactly as
    // before — only preconditions gate them.
    let act = Action::new("step", 1.0).requires("a").adds("b");
    assert!(act.applicable(&State::from_facts(["a"])));
    assert!(act.applicable(&State::from_facts(["a", "anything", "else"])));
}

#[test]
fn requires_and_forbids_overlap_makes_action_unsatisfiable() {
    // The library accepts the contradiction (loose at this layer); higher
    // layers (e.g. uncharles config validation) reject it. Here we just
    // pin the planner's behaviour: such an action is never applicable.
    let act = Action::new("step", 1.0)
        .requires("foo")
        .forbids("foo")
        .adds("bar");

    assert!(!act.applicable(&State::new()));
    assert!(!act.applicable(&State::from_facts(["foo"])));
    assert!(!act.applicable(&State::from_facts(["foo", "extra"])));
}

// ---------------------------------------------------------------------------
// Planner integration — forbids gates which path is chosen
// ---------------------------------------------------------------------------

#[test]
fn planner_routes_around_forbidden_actions() {
    // Two paths from {start} to {done}:
    //   - via "fast" (cheap) but forbids "danger"
    //   - via "slow" (expensive) with no forbid
    // With danger present, planner must take the slow path.
    let actions = vec![
        Action::new("fast", 1.0)
            .requires("start")
            .forbids("danger")
            .adds("done")
            .removes("start"),
        Action::new("slow", 5.0)
            .requires("start")
            .adds("done")
            .removes("start"),
    ];
    let goal = Goal::new().requires("done");

    // Without danger, planner picks the cheap path.
    let plan_clean = Planner::new(actions.clone())
        .plan(&State::from_facts(["start"]), &goal)
        .unwrap()
        .unwrap();
    assert_eq!(plan_clean.steps, vec!["fast"]);
    assert_eq!(plan_clean.cost, 1.0);

    // With danger present, the cheap path is gated; planner falls back.
    let plan_blocked = Planner::new(actions)
        .plan(&State::from_facts(["start", "danger"]), &goal)
        .unwrap()
        .unwrap();
    assert_eq!(plan_blocked.steps, vec!["slow"]);
    assert_eq!(plan_blocked.cost, 5.0);
}

#[test]
fn planner_returns_none_when_only_path_is_blocked_by_forbids() {
    let actions = vec![
        Action::new("only_path", 1.0)
            .requires("start")
            .forbids("blocker")
            .adds("done")
            .removes("start"),
    ];
    let goal = Goal::new().requires("done");

    let plan = Planner::new(actions)
        .plan(&State::from_facts(["start", "blocker"]), &goal)
        .unwrap();
    assert!(plan.is_none());
}

#[test]
fn explore_excludes_transitions_blocked_by_forbids() {
    // `step` consumes `start` so it can only fire once — keeps the test
    // free of self-loops that would otherwise pad the edge count.
    let actions = vec![
        Action::new("step", 1.0)
            .requires("start")
            .forbids("locked")
            .adds("done")
            .removes("start"),
    ];

    // Initial state has the forbidden fact — no transitions discovered.
    let blocked = Planner::new(actions.clone()).explore(&State::from_facts(["start", "locked"]));
    assert_eq!(blocked.states.len(), 1); // just the initial state
    assert!(blocked.edges.is_empty());

    // Initial state without forbidden fact — exactly one transition.
    let clean = Planner::new(actions).explore(&State::from_facts(["start"]));
    assert_eq!(clean.states.len(), 2);
    assert_eq!(clean.edges.len(), 1);
}

// ---------------------------------------------------------------------------
// Forbids interacting with effects across iterations
// ---------------------------------------------------------------------------

#[test]
fn forbids_can_become_satisfied_after_a_remove_effect() {
    // Action `unlock` removes the fact `locked`; once that's done, the
    // forbidding action `proceed` becomes applicable.
    let actions = vec![
        Action::new("unlock", 1.0)
            .requires("locked")
            .removes("locked"),
        Action::new("proceed", 1.0)
            .requires("ready")
            .forbids("locked")
            .adds("done"),
    ];
    let goal = Goal::new().requires("done");
    let plan = Planner::new(actions)
        .plan(&State::from_facts(["ready", "locked"]), &goal)
        .unwrap()
        .unwrap();

    assert_eq!(plan.steps, vec!["unlock", "proceed"]);
    assert_eq!(plan.cost, 2.0);
}
