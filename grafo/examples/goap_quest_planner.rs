//! GOAP quest planner — character-class-aware dungeon routing.
//!
//! Demonstrates Goal-Oriented Action Planning with Grafo:
//!
//! | GOAP concept  | Grafo equivalent                              |
//! |---------------|-----------------------------------------------|
//! | World state   | Node + its `NodeAttrs` tags                   |
//! | Action        | Directed weighted edge (cost = difficulty)    |
//! | Precondition  | `filter` closure passed to `shortest_path_filtered` |
//! | Agent         | A filter predicate representing class tags    |
//!
//! The dungeon graph is built once. Each character class gets its own query
//! with a different filter. Nodes that don't carry the required class tag are
//! invisible to that agent — Dijkstra never enters them.
//!
//! ## Dungeon layout
//!
//! ```text
//!                        [combat_ready only]
//!              ┌──3──► monster_den ──4──► barracks ──3──┐
//!              │                                         ▼
//!  start ──2──► guard_post                         vault_door ──1──► treasury
//!              │                                         ▲
//!              └──2──► trap_corridor ──────────────2─────┘
//!                        [sneaky / magical only]
//! ```
//!
//! Run with: `cargo run --example goap_quest_planner`

use grafo::{Graph, NodeAttrs};

fn main() {
    // -------------------------------------------------------------------------
    // Build the dungeon state graph.
    //
    // Every node that could be a starting point or goal must carry ALL class
    // tags — otherwise a class whose tag is absent would have its source or
    // destination rejected before the search even begins.
    // -------------------------------------------------------------------------
    let graph = Graph::new_with_attrs(
        &[
            // Shared entry / exit nodes — every class can be here.
            ("start", &["combat_ready", "sneaky", "magical"][..]),
            ("guard_post", &["combat_ready", "sneaky", "magical"][..]),
            ("vault_door", &["combat_ready", "sneaky", "magical"][..]),
            ("treasury", &["combat_ready", "sneaky", "magical"][..]),
            // Warrior-only combat rooms — no sneaky/magic tag, so rogues and
            // mages cannot enter these states.
            ("monster_den", &["combat_ready"][..]),
            ("barracks", &["combat_ready"][..]),
            // Shortcut corridor — traps block warriors (no combat_ready tag).
            ("trap_corridor", &["sneaky", "magical"][..]),
        ],
        &[
            // Shared entrance.
            ("start", "guard_post", 2.0),
            // Warrior branch: fight through two rooms.
            ("guard_post", "monster_den", 3.0),
            ("monster_den", "barracks", 4.0),
            ("barracks", "vault_door", 3.0),
            // Rogue / mage shortcut through traps.
            ("guard_post", "trap_corridor", 2.0),
            ("trap_corridor", "vault_door", 2.0),
            // Shared final door.
            ("vault_door", "treasury", 1.0),
        ],
    )
    .expect("graph construction failed");

    let warrior = |attrs: &NodeAttrs| attrs.contains("combat_ready");
    let rogue = |attrs: &NodeAttrs| attrs.contains("sneaky");
    let mage = |attrs: &NodeAttrs| attrs.contains("magical");
    let cursed = |attrs: &NodeAttrs| attrs.contains("cursed");

    println!("=== GOAP Quest Planner ===");
    println!("Goal: start → treasury\n");

    // Warrior cannot enter trap_corridor (no "combat_ready" tag there).
    // Forced through monster_den → barracks at higher cost.
    println!("Warrior (combat_ready):");
    plan(&graph, "start", "treasury", warrior);
    // Expected: start → guard_post → monster_den → barracks → vault_door → treasury (13.0)

    println!("\nRogue (sneaky):");
    plan(&graph, "start", "treasury", rogue);
    // Expected: start → guard_post → trap_corridor → vault_door → treasury (7.0)

    println!("\nMage (magical):");
    plan(&graph, "start", "treasury", mage);
    // Expected: start → guard_post → trap_corridor → vault_door → treasury (7.0)

    // No node carries "cursed" — source fails the precondition check immediately.
    // Dijkstra is never invoked.
    println!("\nCursed (no valid class tag):");
    plan(&graph, "start", "treasury", cursed);
    // Expected: no path
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
        None => println!("  no valid path for this class"),
    }
}
