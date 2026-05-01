//! Service deployment orchestration.
//!
//! Pipeline: tests → build → push → migrate → deploy → smoke.
//! The agent reports which facts already hold; the planner returns the
//! cheapest remaining sequence to reach `smoke_tests_pass`.
//!
//! ```text
//! cargo run --example deploy -- --have code_committed --goal smoke_tests_pass --pretty
//! cargo run --example deploy -- --have image_pushed --goal smoke_tests_pass
//! cargo run --example deploy -- --list-actions --pretty
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("run_tests", 2.0)
            .requires("code_committed")
            .adds("tests_pass"),
        Action::new("build_image", 5.0)
            .requires("tests_pass")
            .adds("image_built"),
        Action::new("push_image", 3.0)
            .requires("image_built")
            .adds("image_pushed"),
        Action::new("apply_migrations", 4.0)
            .requires("image_pushed")
            .adds("migrations_applied"),
        Action::new("deploy_service", 6.0)
            .requires("migrations_applied")
            .adds("service_deployed"),
        Action::new("run_smoke_tests", 2.0)
            .requires("service_deployed")
            .adds("smoke_tests_pass"),
    ]
}

fn main() -> ExitCode {
    common::run("deploy", actions())
}
