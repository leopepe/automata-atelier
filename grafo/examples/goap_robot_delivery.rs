//! GOAP robot delivery planner — capability-constrained warehouse routing.
//!
//! Three robot types must deliver cargo from a depot to a drop-off point.
//! Each robot has a different set of capabilities encoded as node attribute
//! tags. Road segments and facilities are only accessible to robots whose
//! capability set matches the node's tags.
//!
//! | GOAP concept  | Grafo equivalent                              |
//! |---------------|-----------------------------------------------|
//! | World state   | Node + its `NodeAttrs` capability tags        |
//! | Action        | Directed weighted edge (cost = travel time)   |
//! | Precondition  | Filter: robot must carry the node's required tag |
//! | Agent         | Robot type — a filter predicate               |
//!
//! ## Warehouse layout
//!
//! ```text
//!                  [wheeled, tracked]
//!           ┌──2──► loading_bay ──3──┐
//!           │                        ▼
//!  depot ───┤                  cargo_loaded ──2──► highway  [wheeled, flying]
//!           │                         │                └──3──► staging_area ──1──► delivered
//!           └──8──────────────────────┘            off_road [tracked, flying]
//!           (drone aerial pickup)                     └──4──► staging_area
//!
//! ```
//!
//! - **Wheeled truck** uses the loading bay then the highway.
//! - **Tracked crawler** uses the loading bay then the off-road segment.
//! - **Flying drone** cannot enter the loading bay (low ceiling) and uses an
//!   expensive aerial pickup to obtain cargo directly at the depot.
//!
//! Run with: `cargo run --example goap_robot_delivery`

use grafo::{Graph, NodeAttrs};

fn main() {
    // -------------------------------------------------------------------------
    // Build the warehouse state graph.
    // -------------------------------------------------------------------------
    let graph = Graph::new_with_attrs(
        &[
            // Shared endpoints — every robot type can start and finish here.
            ("depot", &["wheeled", "tracked", "flying"][..]),
            ("cargo_loaded", &["wheeled", "tracked", "flying"][..]),
            ("staging_area", &["wheeled", "tracked", "flying"][..]),
            ("delivered", &["wheeled", "tracked", "flying"][..]),
            // Ground loading bay — low ceiling blocks flying drones.
            ("loading_bay", &["wheeled", "tracked"][..]),
            // Highway — too fast for tracked vehicles (speed limit compliance).
            ("highway", &["wheeled", "flying"][..]),
            // Off-road segment — wheeled vehicles get stuck on rough terrain.
            ("off_road", &["tracked", "flying"][..]),
        ],
        &[
            // Ground approach: loading bay available to wheeled and tracked.
            ("depot", "loading_bay", 2.0),
            ("loading_bay", "cargo_loaded", 3.0),
            // Aerial pickup: drone bypasses loading bay at higher cost.
            ("depot", "cargo_loaded", 8.0),
            // Route split after pickup.
            ("cargo_loaded", "highway", 2.0),
            ("cargo_loaded", "off_road", 2.0),
            // Highway is faster; off-road terrain is slower.
            ("highway", "staging_area", 3.0),
            ("off_road", "staging_area", 4.0),
            // Final drop-off.
            ("staging_area", "delivered", 1.0),
        ],
    )
    .expect("graph construction failed");

    let wheeled = |attrs: &NodeAttrs| attrs.contains("wheeled");
    let tracked = |attrs: &NodeAttrs| attrs.contains("tracked");
    let flying = |attrs: &NodeAttrs| attrs.contains("flying");

    println!("=== GOAP Robot Delivery Planner ===");
    println!("Goal: depot → delivered\n");

    // off_road is pruned (no "wheeled" tag); takes loading bay + highway.
    println!("Wheeled truck:");
    plan(&graph, "depot", "delivered", wheeled);
    // Expected: depot → loading_bay → cargo_loaded → highway → staging_area → delivered (11.0)

    // highway is pruned (no "tracked" tag); takes loading bay + off_road.
    println!("\nTracked crawler:");
    plan(&graph, "depot", "delivered", tracked);
    // Expected: depot → loading_bay → cargo_loaded → off_road → staging_area → delivered (12.0)

    // loading_bay is pruned (no "flying" tag); forced onto expensive aerial pickup.
    // After pickup, prefers highway (3.0) over off_road (4.0).
    println!("\nFlying drone:");
    plan(&graph, "depot", "delivered", flying);
    // Expected: depot → cargo_loaded → highway → staging_area → delivered (14.0)
}

fn plan<F>(graph: &Graph, from: &str, to: &str, filter: F)
where
    F: Fn(&NodeAttrs) -> bool,
{
    match graph
        .shortest_path_filtered(from, to, filter)
        .expect("search failed")
    {
        Some(r) => println!(
            "  {} (cost {})",
            r.resolve_labels(graph).join(" → "),
            r.cost,
        ),
        None => println!("  no valid path for this robot type"),
    }
}
