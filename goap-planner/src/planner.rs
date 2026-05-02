use std::collections::{BTreeSet, VecDeque};
use std::fmt;

use grafo::Graph;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::action::Action;
use crate::explore::{StateEdge, StateGraph, StateNode};
use crate::goal::Goal;
use crate::plan::Plan;
use crate::state::State;

const GOAL_SINK: &str = "__GOAL_SINK__";

#[derive(Debug)]
pub enum PlannerError {
    StateSpaceLimitExceeded { max_states: usize },
    Graph(grafo::GraphError),
}

impl fmt::Display for PlannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateSpaceLimitExceeded { max_states } => {
                write!(f, "state-space limit exceeded ({max_states} states)")
            }
            Self::Graph(e) => write!(f, "graph error: {e}"),
        }
    }
}

impl std::error::Error for PlannerError {}

impl From<grafo::GraphError> for PlannerError {
    fn from(e: grafo::GraphError) -> Self {
        Self::Graph(e)
    }
}

/// A planner over a fixed library of [`Action`]s.
pub struct Planner {
    actions: Vec<Action>,
    max_states: usize,
}

impl Planner {
    pub fn new(actions: Vec<Action>) -> Self {
        Self {
            actions,
            max_states: 10_000,
        }
    }

    /// Cap on reachable states explored before [`PlannerError::StateSpaceLimitExceeded`]
    /// is returned by [`Planner::plan`], or before [`Planner::explore`] sets
    /// [`StateGraph::truncated`]. Defaults to 10_000.
    pub fn with_max_states(mut self, max_states: usize) -> Self {
        self.max_states = max_states;
        self
    }

    /// Enumerate the bounded state-action graph reachable from `initial`.
    ///
    /// Goal-agnostic: the returned [`StateGraph::goal_satisfying`] is empty.
    /// Use [`Planner::explore_for_goal`] when you also want to know which
    /// of the discovered states satisfy a goal.
    ///
    /// Never returns an error for hitting `max_states`; instead, the BFS
    /// stops cleanly and [`StateGraph::truncated`] is set to `true`. The
    /// returned graph is a partial but consistent view of what's been
    /// explored so far.
    ///
    /// # Examples
    ///
    /// ```
    /// use goap_planner::{Action, Planner, State};
    ///
    /// let actions = vec![
    ///     Action::new("step", 1.0).requires("a").adds("b").removes("a"),
    /// ];
    /// let graph = Planner::new(actions).explore(&State::from_facts(["a"]));
    /// assert_eq!(graph.states.len(), 2);
    /// assert_eq!(graph.edges.len(), 1);
    /// assert_eq!(graph.edges[0].action, "step");
    /// ```
    pub fn explore(&self, initial: &State) -> StateGraph {
        self.bfs(initial, None)
    }

    /// Like [`Planner::explore`], but additionally records which discovered
    /// states satisfy `goal` in [`StateGraph::goal_satisfying`].
    ///
    /// # Examples
    ///
    /// ```
    /// use goap_planner::{Action, Goal, Planner, State};
    ///
    /// let actions = vec![
    ///     Action::new("chop", 5.0).requires("axe").adds("log").removes("axe"),
    ///     Action::new("split", 2.0).requires("log").adds("firewood").removes("log"),
    /// ];
    /// let initial = State::from_facts(["axe"]);
    /// let goal = Goal::new().requires("firewood");
    /// let graph = Planner::new(actions).explore_for_goal(&initial, &goal);
    ///
    /// assert_eq!(graph.goal_satisfying.len(), 1);
    /// let goal_state = &graph.states[graph.goal_satisfying[0]];
    /// assert!(goal_state.facts.contains("firewood"));
    /// ```
    pub fn explore_for_goal(&self, initial: &State, goal: &Goal) -> StateGraph {
        self.bfs(initial, Some(goal))
    }

    /// Plan from `initial` to any state satisfying `goal`.
    ///
    /// Returns `Ok(Some(plan))` on success, `Ok(None)` when no plan exists in
    /// the discovered state space, or [`PlannerError::StateSpaceLimitExceeded`]
    /// when expansion exceeded `max_states`.
    pub fn plan(&self, initial: &State, goal: &Goal) -> Result<Option<Plan>, PlannerError> {
        if goal.satisfied_by(initial) {
            return Ok(Some(Plan {
                steps: Vec::new(),
                cost: 0.0,
            }));
        }

        let graph = self.explore_for_goal(initial, goal);
        if graph.truncated {
            return Err(PlannerError::StateSpaceLimitExceeded {
                max_states: self.max_states,
            });
        }
        if graph.goal_satisfying.is_empty() {
            return Ok(None);
        }

        // Build a grafo::Graph over the discovered states and run Dijkstra
        // from initial to the synthetic GOAL_SINK linked from every
        // goal-satisfying state.
        let mut nodes: Vec<&str> = graph.states.iter().map(|s| s.signature.as_str()).collect();
        nodes.push(GOAL_SINK);

        let mut edges: Vec<(&str, &str, f64)> = graph
            .edges
            .iter()
            .map(|e| {
                (
                    graph.states[e.from].signature.as_str(),
                    graph.states[e.to].signature.as_str(),
                    e.cost,
                )
            })
            .collect();
        for &gs in &graph.goal_satisfying {
            edges.push((graph.states[gs].signature.as_str(), GOAL_SINK, 0.0));
        }

        let g = Graph::new(&nodes, &edges)?;
        let initial_sig = graph.states[graph.initial].signature.as_str();

        let path = match g.shortest_path(initial_sig, GOAL_SINK)? {
            Some(p) => p,
            None => return Ok(None),
        };

        // Reconstruct action names from the path. For each consecutive pair
        // of state signatures, look up the cheapest edge (which `explore`
        // already kept) and emit its action name.
        let labels = path.resolve_labels(&g);
        let edge_lookup: FxHashMap<(String, String), &StateEdge> = graph
            .edges
            .iter()
            .map(|e| {
                (
                    (
                        graph.states[e.from].signature.clone(),
                        graph.states[e.to].signature.clone(),
                    ),
                    e,
                )
            })
            .collect();

        let steps: Vec<String> = labels
            .windows(2)
            .filter_map(|pair| {
                if pair[1] == GOAL_SINK {
                    None
                } else {
                    edge_lookup
                        .get(&(pair[0].clone(), pair[1].clone()))
                        .map(|e| e.action.clone())
                }
            })
            .collect();

        Ok(Some(Plan {
            steps,
            cost: path.cost,
        }))
    }

    /// Internal BFS over the state space, shared by [`Planner::explore`]
    /// and [`Planner::explore_for_goal`].
    ///
    /// When `goal` is `Some`, every discovered state is checked against it
    /// and recorded in `goal_satisfying`. When `None`, that work is skipped.
    fn bfs(&self, initial: &State, goal: Option<&Goal>) -> StateGraph {
        let initial_sig = initial.signature();

        // Internal BFS uses fast Fx hashing; we sort once at the end for
        // a stable public iteration order.
        let mut signatures: FxHashMap<String, State> = FxHashMap::default();
        signatures.insert(initial_sig.clone(), initial.clone());

        let mut edge_map: FxHashMap<(String, String), (f64, String)> = FxHashMap::default();
        let mut goal_state_sigs: FxHashSet<String> = FxHashSet::default();

        // Check the initial state itself for goal satisfaction.
        if let Some(g) = goal
            && g.satisfied_by(initial)
        {
            goal_state_sigs.insert(initial_sig.clone());
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(initial_sig.clone());

        let mut truncated = false;

        while let Some(sig) = queue.pop_front() {
            if signatures.len() > self.max_states {
                truncated = true;
                break;
            }

            let state = signatures[&sig].clone();

            for action in &self.actions {
                if !action.applicable(&state) {
                    continue;
                }
                let next = action.apply(&state);
                let next_sig = next.signature();

                if !signatures.contains_key(&next_sig) {
                    signatures.insert(next_sig.clone(), next.clone());
                    queue.push_back(next_sig.clone());
                }

                edge_map
                    .entry((sig.clone(), next_sig.clone()))
                    .and_modify(|(c, name)| {
                        if action.cost < *c {
                            *c = action.cost;
                            *name = action.name.clone();
                        }
                    })
                    .or_insert((action.cost, action.name.clone()));

                if let Some(g) = goal
                    && g.satisfied_by(&next)
                {
                    goal_state_sigs.insert(next_sig.clone());
                }
            }
        }

        // Convert to the public, stable-ordered structure.
        let mut states: Vec<StateNode> = signatures
            .iter()
            .map(|(sig, state)| StateNode {
                signature: sig.clone(),
                facts: state.facts().map(String::from).collect::<BTreeSet<_>>(),
            })
            .collect();
        states.sort_by(|a, b| a.signature.cmp(&b.signature));

        let sig_to_idx: FxHashMap<String, usize> = states
            .iter()
            .enumerate()
            .map(|(i, n)| (n.signature.clone(), i))
            .collect();

        let mut edges: Vec<StateEdge> = edge_map
            .into_iter()
            .map(|((from, to), (cost, action))| StateEdge {
                from: sig_to_idx[&from],
                to: sig_to_idx[&to],
                action,
                cost,
            })
            .collect();
        edges.sort_by(|a, b| {
            a.from
                .cmp(&b.from)
                .then(a.action.cmp(&b.action))
                .then(a.to.cmp(&b.to))
        });

        let initial_idx = sig_to_idx[&initial_sig];

        let mut goal_satisfying: Vec<usize> =
            goal_state_sigs.iter().map(|sig| sig_to_idx[sig]).collect();
        goal_satisfying.sort();

        StateGraph {
            states,
            edges,
            initial: initial_idx,
            goal_satisfying,
            truncated,
        }
    }
}
