//! City route planner with transport-mode filtering.
//!
//! Models a one-way road network as a DAG where each city (node) advertises
//! which transport modes stop there. The shortest-path query accepts a
//! predicate so the caller can restrict the route to cities that support a
//! specific mode — inspired by Goal-Oriented Action Planning (GOAP), where
//! nodes are world states and the predicate acts as a precondition.
//!
//! Run with: `cargo run --example city_routes`

use grafo::{Graph, NodeAttrs};

fn main() {
    // Network layout (one-way roads, travel time in minutes):
    //
    //   London ──60──► Oxford ──45──► Birmingham ──90──► Manchester
    //     │                               ▲                   ▲
    //     └──────────────── 150 ──────────┘                   │
    //                    Oxford ──────────── 120 ──────────────┘
    //
    // Transport stops per city:
    //   London     — taxi, bus, train
    //   Oxford     — bus                  ← no taxi or train
    //   Birmingham — taxi, bus, train
    //   Manchester — taxi, bus, train
    //
    let graph = Graph::new_with_attrs(
        &[
            ("London", &["taxi", "bus", "train"][..]),
            ("Oxford", &["bus"][..]),
            ("Birmingham", &["taxi", "bus", "train"][..]),
            ("Manchester", &["taxi", "bus", "train"][..]),
        ],
        &[
            ("London", "Oxford", 60.0),
            ("London", "Birmingham", 150.0),
            ("Oxford", "Birmingham", 45.0),
            ("Oxford", "Manchester", 120.0),
            ("Birmingham", "Manchester", 90.0),
        ],
    )
    .expect("graph construction failed");

    let has = |mode: &'static str| move |attrs: &NodeAttrs| attrs.contains(mode);

    // --- Unrestricted (any transport mode) -----------------------------------
    println!("=== No restriction ===");
    print_route(&graph, "London", "Manchester", |_| true);
    // Expects: London → Oxford → Birmingham → Manchester (195 min)

    // --- Bus only ------------------------------------------------------------
    // Oxford has a bus stop, so the cheap Oxford route is available.
    println!("\n=== Bus only ===");
    print_route(&graph, "London", "Manchester", has("bus"));
    // Expects: London → Oxford → Manchester (180 min)

    // --- Taxi only -----------------------------------------------------------
    // Oxford has NO taxi stop, so it is skipped entirely.
    // The only viable taxi path goes London → Birmingham → Manchester.
    println!("\n=== Taxi only ===");
    print_route(&graph, "London", "Manchester", has("taxi"));
    // Expects: London → Birmingham → Manchester (240 min)

    // --- Train only ----------------------------------------------------------
    // Oxford has no train stop either, same result as taxi-only.
    println!("\n=== Train only ===");
    print_route(&graph, "London", "Manchester", has("train"));
    // Expects: London → Birmingham → Manchester (240 min)

    // --- Mode with no valid route --------------------------------------------
    // Suppose we invent a "ferry" mode; no city has it.
    println!("\n=== Ferry only (no stops exist) ===");
    print_route(&graph, "London", "Manchester", has("ferry"));
    // Expects: no route available
}

fn print_route<F>(graph: &Graph, from: &str, to: &str, filter: F)
where
    F: Fn(&NodeAttrs) -> bool,
{
    print!("  {from} → {to}: ");
    match graph
        .shortest_path_filtered(from, to, filter)
        .expect("search failed")
    {
        Some(r) => println!("{} ({} min)", r.resolve_labels(graph).join(" → "), r.cost),
        None => println!("no route available"),
    }
}
