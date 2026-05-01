//! # GOAP Planner
//!
//! Goal-Oriented Action Planning over a state-space graph built with [`grafo`].
//!
//! Given an initial [`State`], a [`Goal`], and a library of [`Action`]s with
//! preconditions and effects, the [`Planner`] expands reachable states by
//! forward search, builds a directed graph of state transitions, and runs
//! Dijkstra to return the cheapest action sequence that reaches the goal.
//!
//! This crate has no opinion on how [`State`] is gathered — observation,
//! shell-outs, and cloud adapters are the runtime's concern. Callers
//! construct [`State`] however they like (programmatically, from CLI flags,
//! deserialised from JSON, etc.) and pass it to [`Planner::plan`].
//!
//! ## Quick start
//!
//! ```
//! use goap_planner::{Action, Goal, Planner, State};
//!
//! let actions = vec![
//!     Action::new("chop_tree", 5.0).requires("has_axe").adds("has_log"),
//!     Action::new("split_log", 2.0)
//!         .requires("has_log")
//!         .adds("has_firewood")
//!         .removes("has_log"),
//! ];
//!
//! let initial = State::from_facts(["has_axe"]);
//! let goal = Goal::new().requires("has_firewood");
//!
//! let plan = Planner::new(actions).plan(&initial, &goal).unwrap().unwrap();
//! assert_eq!(plan.steps, vec!["chop_tree", "split_log"]);
//! assert_eq!(plan.cost, 7.0);
//! ```

mod action;
mod goal;
mod plan;
mod planner;
mod state;

pub use action::Action;
pub use goal::Goal;
pub use plan::Plan;
pub use planner::{Planner, PlannerError};
pub use state::State;
