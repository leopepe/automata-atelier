//! Basic usage of Grafo.
//!
//! Demonstrates how to build a graph and run a shortest-path query.
//! Run with: `cargo run --example basic`

use grafo::Graph;

fn main() {
    // Define the nodes and directed weighted edges.
    //
    //        1       2       1
    //   a ──────► b ──────► c ──────► d
    //   │                             ▲
    //   └──────────── 4 ──────────────┘
    //
    let graph = Graph::new(
        &["a", "b", "c", "d"],
        &[
            ("a", "b", 1.0),
            ("a", "c", 4.0),
            ("b", "c", 2.0),
            ("b", "d", 5.0),
            ("c", "d", 1.0),
        ],
    )
    .expect("graph construction failed");

    // Query the shortest path from "a" to "d".
    match graph.shortest_path("a", "d").expect("search failed") {
        Some(result) => {
            println!(
                "Shortest path: {}",
                result.resolve_labels(&graph).join(" → ")
            );
            println!("Total cost:    {}", result.cost);
        }
        None => println!("No path found."),
    }
}
