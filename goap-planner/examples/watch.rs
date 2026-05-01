//! Service health remediation: given the observed state of a service, plan
//! the cheapest sequence of repair actions that returns it to a healthy
//! steady state.
//!
//! Unlike `deploy`, this example exercises branching: different observed
//! conditions (stopped, unhealthy, overloaded) lead to different plans.
//!
//! Goal predicates need care: `--goal service_healthy` alone is satisfiable by
//! a `health_check` that *adds* `service_healthy` while leaving
//! `service_unhealthy` set. Use `--forbid service_unhealthy` to force the
//! planner to choose an action that actually clears the bad state.
//!
//! ```text
//! # service is stopped and CPU is high → start, scale, then health-check
//! cargo run --example watch -- \
//!     --have service_stopped --have cpu_high \
//!     --goal service_healthy --forbid cpu_high --pretty
//!
//! # service is up but unhealthy → restart_service is the only action that
//! # both adds service_healthy AND removes service_unhealthy
//! cargo run --example watch -- \
//!     --have service_running --have service_unhealthy \
//!     --goal service_healthy --forbid service_unhealthy --pretty
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("start_service", 3.0)
            .requires("service_stopped")
            .removes("service_stopped")
            .adds("service_running"),
        Action::new("restart_service", 5.0)
            .requires("service_unhealthy")
            .removes("service_unhealthy")
            .adds("service_running")
            .adds("service_healthy"),
        Action::new("health_check", 1.0)
            .requires("service_running")
            .adds("service_healthy"),
        Action::new("scale_up", 3.0)
            .requires("cpu_high")
            .removes("cpu_high"),
        Action::new("page_oncall", 50.0)
            .requires("service_unhealthy")
            .adds("service_healthy")
            .removes("service_unhealthy"),
    ]
}

fn main() -> ExitCode {
    common::run("watch", actions())
}
