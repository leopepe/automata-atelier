//! Dependency-graph build planning.
//!
//! Project topology:
//! ```text
//!   target ─► lib_a ─► core
//!         └─► lib_b ─► core
//!         └─► lib_a ─► utils
//! ```
//! Each leaf can be built from source OR fetched from a cache (cheaper).
//! The planner selects the lowest-cost order that respects every
//! `requires` precondition. Pass `--have built_X` to skip artefacts that
//! the agent has already produced — the plan reflects only the work left.
//!
//! ```text
//! # Cold cache, full build
//! cargo run --example dependency -- --goal built_target --pretty
//!
//! # Cache contains core; planner fetches utils from cache, builds the rest
//! cargo run --example dependency -- --have built_core --goal built_target --pretty
//!
//! # Inspect the action library — agent uses this to discover legal facts
//! cargo run --example dependency -- --list-actions --pretty
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("build_core", 3.0).adds("built_core"),
        Action::new("fetch_cached_core", 1.0).adds("built_core"),
        Action::new("build_utils", 2.0).adds("built_utils"),
        Action::new("fetch_cached_utils", 1.0).adds("built_utils"),
        Action::new("build_lib_a", 4.0)
            .requires("built_core")
            .requires("built_utils")
            .adds("built_lib_a"),
        Action::new("fetch_cached_lib_a", 2.0)
            .requires("built_core")
            .requires("built_utils")
            .adds("built_lib_a"),
        Action::new("build_lib_b", 3.0)
            .requires("built_core")
            .adds("built_lib_b"),
        Action::new("build_target", 5.0)
            .requires("built_lib_a")
            .requires("built_lib_b")
            .adds("built_target"),
    ]
}

fn main() -> ExitCode {
    common::run("dependency", actions())
}
