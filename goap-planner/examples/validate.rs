//! Pre-merge validation gate. Given which checks have already passed,
//! return the remaining sequence needed to reach `shippable`.
//!
//! Useful for an agent answering "what's left before this PR can ship?"
//! without re-running checks that already succeeded.
//!
//! ```text
//! # nothing done yet
//! cargo run --example validate -- --have code_present --goal shippable --pretty
//!
//! # type-check already passed; only test + build + ship remain
//! cargo run --example validate -- \
//!     --have code_present --have formatted --have linted --have type_checked \
//!     --goal shippable --pretty
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("format", 1.0)
            .requires("code_present")
            .adds("formatted"),
        Action::new("lint", 2.0)
            .requires("formatted")
            .adds("linted"),
        Action::new("type_check", 3.0)
            .requires("linted")
            .adds("type_checked"),
        Action::new("unit_tests", 5.0)
            .requires("type_checked")
            .adds("tested"),
        Action::new("integration_tests", 12.0)
            .requires("type_checked")
            .adds("tested")
            .adds("integration_verified"),
        Action::new("build", 4.0).requires("tested").adds("built"),
        Action::new("ship", 1.0).requires("built").adds("shippable"),
    ]
}

fn main() -> ExitCode {
    common::run("validate", actions())
}
