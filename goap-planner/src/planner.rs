use std::collections::VecDeque;
use std::fmt;

use grafo::Graph;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::action::Action;
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
    /// is returned. Defaults to 10_000.
    pub fn with_max_states(mut self, max_states: usize) -> Self {
        self.max_states = max_states;
        self
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

        let initial_sig = initial.signature();
        let mut signatures: FxHashMap<String, State> = FxHashMap::default();
        signatures.insert(initial_sig.clone(), initial.clone());

        let mut edge_map: FxHashMap<(String, String), (f64, String)> = FxHashMap::default();
        let mut goal_states: FxHashSet<String> = FxHashSet::default();

        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(initial_sig.clone());

        while let Some(sig) = queue.pop_front() {
            if signatures.len() > self.max_states {
                return Err(PlannerError::StateSpaceLimitExceeded {
                    max_states: self.max_states,
                });
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

                if goal.satisfied_by(&next) {
                    goal_states.insert(next_sig.clone());
                }
            }
        }

        if goal_states.is_empty() {
            return Ok(None);
        }

        let mut nodes: Vec<String> = signatures.keys().cloned().collect();
        nodes.push(GOAL_SINK.to_string());

        let mut edges: Vec<(String, String, f64)> = edge_map
            .iter()
            .map(|((from, to), (cost, _))| (from.clone(), to.clone(), *cost))
            .collect();
        for gs in &goal_states {
            edges.push((gs.clone(), GOAL_SINK.to_string(), 0.0));
        }

        let node_refs: Vec<&str> = nodes.iter().map(String::as_str).collect();
        let edge_refs: Vec<(&str, &str, f64)> = edges
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), *c))
            .collect();
        let graph = Graph::new(&node_refs, &edge_refs)?;

        let path = match graph.shortest_path(&initial_sig, GOAL_SINK)? {
            Some(p) => p,
            None => return Ok(None),
        };

        let labels = path.resolve_labels(&graph);
        let steps: Vec<String> = labels
            .windows(2)
            .filter_map(|pair| {
                if pair[1] == GOAL_SINK {
                    None
                } else {
                    edge_map
                        .get(&(pair[0].clone(), pair[1].clone()))
                        .map(|(_, name)| name.clone())
                }
            })
            .collect();

        Ok(Some(Plan {
            steps,
            cost: path.cost,
        }))
    }
}
