//! Build pipeline dependency resolver.
//!
//! Models a software build system as a DAG where nodes are build steps and
//! edge weights represent the estimated execution time in seconds. The
//! shortest path gives the critical path with the minimum total build time
//! between two steps.
//!
//! Run with: `cargo run --example build_pipeline`

use grafo::Graph;

fn main() {
    // Build steps and their dependencies (step → next_step, seconds).
    //
    //   fetch_deps ──5──► compile ──20──► link ──3──► package ──2──► deploy
    //                        │                            ▲
    //                        └──────── 8 ─── test ────────┘
    //
    let steps = &["fetch_deps", "compile", "test", "link", "package", "deploy"];

    let dependencies: &[(&str, &str, f64)] = &[
        ("fetch_deps", "compile", 5.0),
        ("compile", "link", 20.0),
        ("compile", "test", 8.0),
        ("test", "package", 3.0),
        ("link", "package", 3.0),
        ("package", "deploy", 2.0),
    ];

    let graph = Graph::new(steps, dependencies).expect("graph construction failed");

    println!("=== Build Pipeline Critical Path ===\n");

    let queries = [
        ("fetch_deps", "deploy"),
        ("fetch_deps", "package"),
        ("compile", "deploy"),
    ];

    for (from, to) in queries {
        print!("  {from} → {to}: ");
        match graph.shortest_path(from, to).expect("search failed") {
            Some(r) => println!("{} ({}s)", r.resolve_labels(&graph).join(" → "), r.cost),
            None => println!("no path"),
        }
    }

    // Demonstrate graceful handling of an unknown step.
    println!("\n=== Error Handling ===\n");
    match graph.shortest_path("fetch_deps", "unknown_step") {
        Err(e) => println!("  Expected error: {e}"),
        Ok(_) => unreachable!(),
    }

    // Demonstrate unreachable step (no edges lead back in a DAG).
    match graph.shortest_path("deploy", "compile") {
        Ok(None) => println!("  No path from 'deploy' back to 'compile' (expected in a DAG)"),
        _ => unreachable!(),
    }
}
