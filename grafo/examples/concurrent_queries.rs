//! Concurrent shortest-path queries with `Arc<Graph>`.
//!
//! `Graph` is `Send + Sync` — once built it is fully immutable and can be
//! shared across threads at zero cost. This example shows two patterns:
//!
//! 1. **`std::thread`** — spawn one thread per query (useful when each query
//!    is long-running and you want OS-level parallelism).
//! 2. **Rayon** — run a batch of queries as a parallel iterator (best when
//!    queries are many and short, e.g. bulk distance lookups).
//!
//! Run with: `cargo run --example concurrent_queries`

use grafo::Graph;
use rayon::prelude::*;
use std::sync::Arc;

fn main() {
    // Build a moderately large graph once.
    //
    //   City road network: 8 cities, one-way connections, travel time in minutes.
    //
    //   London ──60──► Oxford ──45──► Birmingham ──90──► Manchester
    //     │                │                                 ▲
    //     │                └──────── 120 ────────────────────┘
    //     └──────────── 150 ──────► Birmingham
    //
    //   Leeds ──30──► Sheffield ──40──► Nottingham ──50──► Leicester
    //     └─────────────────────── 90 ──────────────────────────────┘
    //
    let graph = Arc::new(
        Graph::new(
            &[
                "London",
                "Oxford",
                "Birmingham",
                "Manchester",
                "Leeds",
                "Sheffield",
                "Nottingham",
                "Leicester",
            ],
            &[
                ("London", "Oxford", 60.0),
                ("London", "Birmingham", 150.0),
                ("Oxford", "Birmingham", 45.0),
                ("Oxford", "Manchester", 120.0),
                ("Birmingham", "Manchester", 90.0),
                ("Leeds", "Sheffield", 30.0),
                ("Leeds", "Leicester", 90.0),
                ("Sheffield", "Nottingham", 40.0),
                ("Nottingham", "Leicester", 50.0),
            ],
        )
        .expect("graph construction failed"),
    );

    // --- Pattern 1: std::thread -----------------------------------------------
    println!("=== Pattern 1: std::thread (one thread per query) ===\n");

    let queries = vec![
        ("London", "Manchester"),
        ("London", "Birmingham"),
        ("Oxford", "Manchester"),
        ("Leeds", "Leicester"),
        ("Sheffield", "Leicester"),
    ];

    let handles: Vec<_> = queries
        .into_iter()
        .map(|(from, to)| {
            let g = Arc::clone(&graph);
            std::thread::spawn(move || {
                let result = g.shortest_path(from, to).expect("search failed");
                (from, to, result)
            })
        })
        .collect();

    for handle in handles {
        let (from, to, result) = handle.join().expect("thread panicked");
        match result {
            Some(r) => println!(
                "  {from} → {to}: {} ({} min)",
                r.resolve_labels(&graph).join(" → "),
                r.cost
            ),
            None => println!("  {from} → {to}: no route"),
        }
    }

    // --- Pattern 2: Rayon parallel iterator ------------------------------------
    println!("\n=== Pattern 2: Rayon par_iter (batch cost-only lookups) ===\n");

    // All pairs in the graph — check reachability in parallel.
    let cities = [
        "London",
        "Oxford",
        "Birmingham",
        "Manchester",
        "Leeds",
        "Sheffield",
        "Nottingham",
        "Leicester",
    ];

    let pairs: Vec<(&str, &str)> = cities
        .iter()
        .flat_map(|&from| cities.iter().map(move |&to| (from, to)))
        .filter(|(from, to)| from != to)
        .collect();

    // Run all queries in parallel; collect results in input order.
    let results: Vec<_> = pairs
        .par_iter()
        .map(|&(from, to)| {
            let cost = graph.shortest_path_cost(from, to).expect("search failed");
            (from, to, cost)
        })
        .collect();

    let reachable = results.iter().filter(|(_, _, c)| c.is_some()).count();
    let total = results.len();
    println!("  Queried {total} pairs in parallel.");
    println!("  Reachable: {reachable} / {total}\n");

    // Print a sample of results.
    for (from, to, cost) in results.iter().take(6) {
        match cost {
            Some(c) => println!("  {from} → {to}: {c} min"),
            None => println!("  {from} → {to}: unreachable"),
        }
    }
}
