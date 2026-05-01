//! Multi-tool release orchestration.
//!
//! Action names are namespaced by the tool that executes them
//! (`git_*`, `docker_*`, `helm_*`, `k8s_*`, `slack_*`) so an agent
//! consuming the plan can dispatch each step to the right MCP server
//! or CLI without re-classifying.
//!
//! Pipeline:
//!   git_tag → git_push_tag
//!     → docker_build → docker_push
//!     → helm_package → helm_publish
//!     → k8s_deploy_staging → smoke_test_staging
//!     → k8s_deploy_prod → smoke_test_prod
//!     → slack_announce
//!
//! `update_changelog` is independent of the build/deploy chain but is a
//! precondition of `slack_announce`, so it can interleave at any point —
//! the planner places it where total cost is minimised.
//!
//! ```text
//! # Full release from a ready branch
//! cargo run --example release -- --have release_branch_ready --goal announced --pretty
//!
//! # Resume mid-flight: image already pushed, helm work and beyond remain
//! cargo run --example release -- \
//!     --have release_branch_ready --have tagged --have tag_published \
//!     --have image_built --have image_published \
//!     --goal announced --pretty
//!
//! # JSON output an agent can dispatch directly
//! cargo run --example release -- --have release_branch_ready --goal announced
//! ```

#[path = "common/mod.rs"]
mod common;

use std::process::ExitCode;

use goap_planner::Action;

fn actions() -> Vec<Action> {
    vec![
        Action::new("git_tag", 1.0)
            .requires("release_branch_ready")
            .adds("tagged"),
        Action::new("git_push_tag", 1.0)
            .requires("tagged")
            .adds("tag_published"),
        Action::new("docker_build", 8.0)
            .requires("tag_published")
            .adds("image_built"),
        Action::new("docker_push", 3.0)
            .requires("image_built")
            .adds("image_published"),
        Action::new("helm_package", 1.0)
            .requires("image_published")
            .adds("chart_packaged"),
        Action::new("helm_publish", 2.0)
            .requires("chart_packaged")
            .adds("chart_published"),
        Action::new("k8s_deploy_staging", 4.0)
            .requires("chart_published")
            .adds("staging_deployed"),
        Action::new("smoke_test_staging", 5.0)
            .requires("staging_deployed")
            .adds("staging_verified"),
        Action::new("k8s_deploy_prod", 4.0)
            .requires("staging_verified")
            .adds("prod_deployed"),
        Action::new("smoke_test_prod", 5.0)
            .requires("prod_deployed")
            .adds("prod_verified"),
        Action::new("update_changelog", 2.0).adds("changelog_updated"),
        Action::new("slack_announce", 1.0)
            .requires("prod_verified")
            .requires("changelog_updated")
            .adds("announced"),
    ]
}

fn main() -> ExitCode {
    common::run("release", actions())
}
