use goap_planner::{Action, Goal, Planner, State};

#[test]
fn classic_woodcutter_plan() {
    let actions = vec![
        Action::new("chop_tree", 5.0)
            .requires("has_axe")
            .adds("has_log"),
        Action::new("split_log", 2.0)
            .requires("has_log")
            .adds("has_firewood")
            .removes("has_log"),
    ];

    let initial = State::from_facts(["has_axe"]);
    let goal = Goal::new().requires("has_firewood");

    let plan = Planner::new(actions)
        .plan(&initial, &goal)
        .unwrap()
        .unwrap();

    assert_eq!(plan.steps, vec!["chop_tree", "split_log"]);
    assert_eq!(plan.cost, 7.0);
}

#[test]
fn planner_picks_cheapest_branch() {
    let actions = vec![
        Action::new("buy_firewood_expensive", 100.0).adds("has_firewood"),
        Action::new("chop_tree", 5.0)
            .requires("has_axe")
            .adds("has_log"),
        Action::new("split_log", 2.0)
            .requires("has_log")
            .adds("has_firewood")
            .removes("has_log"),
    ];

    let initial = State::from_facts(["has_axe"]);
    let goal = Goal::new().requires("has_firewood");

    let plan = Planner::new(actions)
        .plan(&initial, &goal)
        .unwrap()
        .unwrap();

    assert_eq!(plan.cost, 7.0);
    assert_eq!(plan.steps, vec!["chop_tree", "split_log"]);
}

#[test]
fn no_plan_when_unreachable() {
    let actions = vec![
        Action::new("split_log", 2.0)
            .requires("has_log")
            .adds("has_firewood"),
    ];

    let initial = State::new();
    let goal = Goal::new().requires("has_firewood");

    assert!(
        Planner::new(actions)
            .plan(&initial, &goal)
            .unwrap()
            .is_none()
    );
}

#[test]
fn empty_plan_when_goal_already_satisfied() {
    let initial = State::from_facts(["has_firewood"]);
    let goal = Goal::new().requires("has_firewood");

    let plan = Planner::new(Vec::new())
        .plan(&initial, &goal)
        .unwrap()
        .unwrap();

    assert!(plan.is_empty());
    assert_eq!(plan.cost, 0.0);
}

#[test]
fn forbidden_facts_block_goal() {
    let actions = vec![
        Action::new("light_fire", 1.0)
            .requires("has_firewood")
            .adds("fire_lit"),
    ];

    let initial = State::from_facts(["has_firewood", "raining"]);
    let goal = Goal::new().requires("fire_lit").forbids("raining");

    assert!(
        Planner::new(actions)
            .plan(&initial, &goal)
            .unwrap()
            .is_none()
    );
}
