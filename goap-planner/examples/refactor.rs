//! Multi-file refactor: rename `old_api` to `new_api` across three modules.
//!
//! Safe-refactor invariant: callers must keep working at every step. The
//! library encodes this by structural ordering rather than negative
//! preconditions:
//!
//! 1. `add_alias`            — introduce `new_api` as a forwarding alias
//! 2. `migrate_*` (per file) — switch each call site to `new_api` (any order)
//! 3. `remove_old_api`       — only applicable once every caller migrated
//! 4. `update_docs`          — only meaningful after the rename is final
//!
//! Modules differ in size, so migration costs differ. The planner picks an
//! order that respects the barrier in step 3 and minimises total cost.
//!
//! ```text
//! # Start from an unrefactored repo
//! cargo run --example refactor -- --goal old_api_removed --goal docs_updated --pretty
//!
//! # Two modules already migrated; only gamma + remove + docs remain
//! cargo run --example refactor -- \
//!     --have alias_added --have alpha_migrated --have beta_migrated \
//!     --goal old_api_removed --goal docs_updated --pretty
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("add_alias", 1.0).adds("alias_added"),
        Action::new("migrate_alpha", 2.0)
            .requires("alias_added")
            .adds("alpha_migrated"),
        Action::new("migrate_beta", 4.0)
            .requires("alias_added")
            .adds("beta_migrated"),
        Action::new("migrate_gamma", 3.0)
            .requires("alias_added")
            .adds("gamma_migrated"),
        Action::new("remove_old_api", 1.0)
            .requires("alpha_migrated")
            .requires("beta_migrated")
            .requires("gamma_migrated")
            .removes("alias_added")
            .adds("old_api_removed"),
        Action::new("update_docs", 1.0)
            .requires("old_api_removed")
            .adds("docs_updated"),
    ]
}

fn main() -> ExitCode {
    common::run("refactor", actions())
}
