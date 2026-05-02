//! State-space enumeration for [`crate::Planner`].
//!
//! [`Planner::plan`] runs forward BFS, builds a state-action graph, and then
//! discards the graph after Dijkstra has selected the cheapest path. That's
//! the right shape for production planning, but inspection tools (config
//! debuggers, visualisers, regression tests) need the structure itself.
//! [`Planner::explore`] returns it as a [`StateGraph`].
//!
//! [`Planner::plan`]: crate::Planner::plan
//! [`Planner::explore`]: crate::Planner::explore

use std::collections::BTreeSet;

/// The bounded state-action graph reachable from a given initial state.
///
/// Returned by [`crate::Planner::explore`]. Use this for inspection,
/// visualisation, and static analysis. For finding the cheapest plan,
/// call [`crate::Planner::plan`] instead — it runs Dijkstra over this same
/// graph internally.
///
/// Iteration order is stable: `states` is sorted by signature, `edges` is
/// sorted by `(from, action, to)`, and `goal_satisfying` is sorted.
///
/// # Examples
///
/// ```
/// use goap_planner::{Action, Planner, State};
///
/// let actions = vec![
///     Action::new("chop_tree", 5.0).requires("has_axe").adds("has_log").removes("has_axe"),
///     Action::new("split_log", 2.0).requires("has_log").adds("has_firewood").removes("has_log"),
/// ];
/// let initial = State::from_facts(["has_axe"]);
/// let graph = Planner::new(actions).explore(&initial);
///
/// assert_eq!(graph.states.len(), 3);              // initial + after-chop + after-split
/// assert_eq!(graph.edges.len(), 2);
/// assert!(!graph.truncated);
/// // The goal `has_firewood` isn't asked here — explore is goal-agnostic.
/// assert!(graph.goal_satisfying.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct StateGraph {
    /// All discovered states, sorted by [`StateNode::signature`].
    pub states: Vec<StateNode>,
    /// All transitions between discovered states, sorted by
    /// `(from, action, to)`. When several actions connect the same pair
    /// of states, the cheapest is kept (consistent with [`crate::Planner::plan`]'s
    /// shortest-path selection).
    pub edges: Vec<StateEdge>,
    /// Index of the initial state in `states`.
    pub initial: usize,
    /// Indices of the states that satisfy the goal passed to
    /// [`crate::Planner::explore_for_goal`]. Empty for goal-agnostic exploration.
    pub goal_satisfying: Vec<usize>,
    /// `true` if BFS hit the planner's `max_states` cap and stopped before
    /// exhausting the reachable state space. The returned graph is still
    /// usable, but it is a partial view: there may be states or edges that
    /// would have been discovered with a larger cap.
    pub truncated: bool,
}

/// A single discovered state in a [`StateGraph`].
///
/// `signature` is the canonical string returned by
/// [`crate::State::signature`]; two `StateNode`s with the same signature
/// represent the same world state. `facts` is the same set of facts in
/// sorted form, ready for stable display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateNode {
    /// Canonical signature of the state.
    pub signature: String,
    /// Sorted facts that hold in this state.
    pub facts: BTreeSet<String>,
}

/// A directed transition between two states in a [`StateGraph`], produced
/// by an action firing.
#[derive(Debug, Clone, PartialEq)]
pub struct StateEdge {
    /// Index of the source state in [`StateGraph::states`].
    pub from: usize,
    /// Index of the destination state.
    pub to: usize,
    /// Name of the action that produced this transition (the cheapest
    /// action when multiple actions connect the same pair of states).
    pub action: String,
    /// Cost of the action.
    pub cost: f64,
}

impl StateGraph {
    /// Iterates over the outgoing edges of a given state index.
    ///
    /// Convenience wrapper over the `edges` slice. Edges are pre-sorted by
    /// `(from, action, to)`, so this returns them in stable order.
    pub fn outgoing(&self, state_idx: usize) -> impl Iterator<Item = &StateEdge> {
        self.edges.iter().filter(move |e| e.from == state_idx)
    }

    /// Returns `true` if the given state index has no outgoing edges.
    ///
    /// Combined with `goal_satisfying`, this identifies dead-end states:
    /// `is_dead_end(i) && !goal_satisfying.contains(&i)`.
    pub fn is_dead_end(&self, state_idx: usize) -> bool {
        self.outgoing(state_idx).next().is_none()
    }
}
