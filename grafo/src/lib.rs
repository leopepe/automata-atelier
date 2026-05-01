//! # Grafo
//!
//! A fast directed acyclic graph (DAG) library with shortest-path search.
//!
//! Graphs are stored in **Compressed Sparse Row (CSR)** format for minimal
//! memory usage and cache-friendly edge traversal. Shortest-path queries use
//! **Dijkstra's algorithm** with a binary min-heap.
//!
//! ## Quick start
//!
//! ```
//! use grafo::Graph;
//!
//! let graph = Graph::new(
//!     &["a", "b", "c", "d"],
//!     &[
//!         ("a", "b", 1.0),
//!         ("a", "c", 4.0),
//!         ("b", "c", 2.0),
//!         ("b", "d", 5.0),
//!         ("c", "d", 1.0),
//!     ],
//! )
//! .unwrap();
//!
//! let result = graph.shortest_path("a", "d").unwrap().unwrap();
//! assert_eq!(result.cost, 4.0);
//! assert_eq!(result.resolve_labels(&graph), vec!["a", "b", "c", "d"]);
//! ```

mod graph;

pub use graph::{Graph, GraphError, NodeAttrs, PathResult};

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    /// Diamond-shaped DAG used across several tests.
    ///
    /// ```text
    ///        1       2       1
    ///   a ──────► b ──────► c ──────► d
    ///   │                             ▲
    ///   └──────────────── 4 ──────────┘  (a → c shortcut)
    ///              b ──────────── 5 ─────► d
    /// ```
    fn diamond() -> Graph {
        Graph::new(
            &["a", "b", "c", "d"],
            &[
                ("a", "b", 1.0),
                ("a", "c", 4.0),
                ("b", "c", 2.0),
                ("b", "d", 5.0),
                ("c", "d", 1.0),
            ],
        )
        .unwrap()
    }

    // ---------------------------------------------------------------------------
    // Construction
    // ---------------------------------------------------------------------------

    #[test]
    fn duplicate_node_is_rejected() {
        let err = Graph::new(&["x", "x"], &[]).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateNode(n) if n == "x"));
    }

    #[test]
    fn edge_with_unknown_source_is_rejected() {
        let err = Graph::new(&["a", "b"], &[("ghost", "b", 1.0)]).unwrap_err();
        assert!(matches!(err, GraphError::UnknownNode(n) if n == "ghost"));
    }

    #[test]
    fn edge_with_unknown_destination_is_rejected() {
        let err = Graph::new(&["a", "b"], &[("a", "ghost", 1.0)]).unwrap_err();
        assert!(matches!(err, GraphError::UnknownNode(n) if n == "ghost"));
    }

    #[test]
    fn empty_graph_builds_successfully() {
        Graph::new(&[], &[]).unwrap();
    }

    #[test]
    fn single_node_no_edges_builds_successfully() {
        Graph::new(&["solo"], &[]).unwrap();
    }

    // ---------------------------------------------------------------------------
    // Shortest path — happy paths
    // ---------------------------------------------------------------------------

    #[test]
    fn shortest_path_picks_cheapest_route() {
        // a→b→c→d costs 4; a→b→d costs 6; a→c→d costs 5.
        let g = diamond();
        let r = g.shortest_path("a", "d").unwrap().unwrap();
        assert_eq!(r.cost, 4.0);
        assert_eq!(r.resolve_labels(&g), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn shortest_path_direct_neighbour() {
        let g = diamond();
        let r = g.shortest_path("a", "b").unwrap().unwrap();
        assert_eq!(r.cost, 1.0);
        assert_eq!(r.resolve_labels(&g), vec!["a", "b"]);
    }

    #[test]
    fn shortest_path_same_node_returns_zero_cost() {
        let g = diamond();
        let r = g.shortest_path("b", "b").unwrap().unwrap();
        assert_eq!(r.cost, 0.0);
        assert_eq!(r.resolve_labels(&g), vec!["b"]);
    }

    #[test]
    fn shortest_path_two_node_graph() {
        let g = Graph::new(&["x", "y"], &[("x", "y", 7.0)]).unwrap();
        let r = g.shortest_path("x", "y").unwrap().unwrap();
        assert_eq!(r.cost, 7.0);
        assert_eq!(r.resolve_labels(&g), vec!["x", "y"]);
    }

    #[test]
    fn shortest_path_prefers_indirect_cheaper_route() {
        // Direct edge a→c costs 10; going a→b→c costs 3.
        let g = Graph::new(
            &["a", "b", "c"],
            &[("a", "b", 1.0), ("b", "c", 2.0), ("a", "c", 10.0)],
        )
        .unwrap();
        let r = g.shortest_path("a", "c").unwrap().unwrap();
        assert_eq!(r.cost, 3.0);
        assert_eq!(r.resolve_labels(&g), vec!["a", "b", "c"]);
    }

    #[test]
    fn shortest_path_long_chain() {
        // a → b → c → d → e, weight 1 each.
        let g = Graph::new(
            &["a", "b", "c", "d", "e"],
            &[
                ("a", "b", 1.0),
                ("b", "c", 1.0),
                ("c", "d", 1.0),
                ("d", "e", 1.0),
            ],
        )
        .unwrap();
        let r = g.shortest_path("a", "e").unwrap().unwrap();
        assert_eq!(r.cost, 4.0);
        assert_eq!(r.resolve_labels(&g), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn shortest_path_with_zero_weight_edges() {
        let g = Graph::new(&["a", "b", "c"], &[("a", "b", 0.0), ("b", "c", 0.0)]).unwrap();
        let r = g.shortest_path("a", "c").unwrap().unwrap();
        assert_eq!(r.cost, 0.0);
        assert_eq!(r.resolve_labels(&g), vec!["a", "b", "c"]);
    }

    // ---------------------------------------------------------------------------
    // Shortest path — no path / errors
    // ---------------------------------------------------------------------------

    #[test]
    fn no_path_in_dag_returns_none() {
        // DAG: no edge leads back to "a".
        assert!(diamond().shortest_path("d", "a").unwrap().is_none());
    }

    #[test]
    fn no_path_between_disconnected_nodes_returns_none() {
        let g = Graph::new(
            &["a", "b", "c"],
            &[("a", "b", 1.0)], // c is isolated
        )
        .unwrap();
        assert!(g.shortest_path("a", "c").unwrap().is_none());
    }

    #[test]
    fn unknown_source_node_returns_error() {
        let err = diamond().shortest_path("z", "d").unwrap_err();
        assert!(matches!(err, GraphError::UnknownNode(n) if n == "z"));
    }

    #[test]
    fn unknown_destination_node_returns_error() {
        let err = diamond().shortest_path("a", "z").unwrap_err();
        assert!(matches!(err, GraphError::UnknownNode(n) if n == "z"));
    }

    // ---------------------------------------------------------------------------
    // new_with_attrs — construction
    // ---------------------------------------------------------------------------

    /// Shared fixture: city graph where nodes carry transport-mode attributes.
    ///
    /// ```text
    ///   A ──1──► B ──1──► C ──1──► D
    ///   │                          ▲
    ///   └──────────── 5 ───────────┘
    ///
    ///   attrs:  A=[taxi,bus]  B=[bus]  C=[taxi,bus]  D=[taxi,bus]
    /// ```
    fn city() -> Graph {
        Graph::new_with_attrs(
            &[
                ("A", &["taxi", "bus"][..]),
                ("B", &["bus"][..]),
                ("C", &["taxi", "bus"][..]),
                ("D", &["taxi", "bus"][..]),
            ],
            &[
                ("A", "B", 1.0),
                ("B", "C", 1.0),
                ("C", "D", 1.0),
                ("A", "D", 5.0),
            ],
        )
        .unwrap()
    }

    #[test]
    fn new_with_attrs_duplicate_node_is_rejected() {
        let err = Graph::new_with_attrs(&[("x", &[][..]), ("x", &[][..])], &[]).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateNode(n) if n == "x"));
    }

    #[test]
    fn new_with_attrs_unknown_edge_node_is_rejected() {
        let err = Graph::new_with_attrs(&[("a", &[][..])], &[("a", "ghost", 1.0)]).unwrap_err();
        assert!(matches!(err, GraphError::UnknownNode(n) if n == "ghost"));
    }

    // ---------------------------------------------------------------------------
    // shortest_path_filtered — filter allows all nodes (same as shortest_path)
    // ---------------------------------------------------------------------------

    #[test]
    fn filtered_with_pass_all_matches_unfiltered() {
        let g = city();
        let unfiltered = g.shortest_path("A", "D").unwrap();
        let filtered = g.shortest_path_filtered("A", "D", |_| true).unwrap();
        assert_eq!(unfiltered, filtered);
    }

    // ---------------------------------------------------------------------------
    // shortest_path_filtered — filter blocks intermediate nodes
    // ---------------------------------------------------------------------------

    #[test]
    fn filtered_by_taxi_skips_bus_only_node() {
        // B has no taxi stop, so A→B→C→D (cost 3) is blocked.
        // The only viable taxi path is A→D (cost 5).
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        let r = g.shortest_path_filtered("A", "D", taxi).unwrap().unwrap();
        assert_eq!(r.cost, 5.0);
        assert_eq!(r.resolve_labels(&g), vec!["A", "D"]);
    }

    #[test]
    fn filtered_by_bus_uses_cheapest_allowed_path() {
        // All nodes have a bus stop, so the cheapest path A→B→C→D (cost 3) is used.
        let g = city();
        let bus = |attrs: &NodeAttrs| attrs.contains("bus");
        let r = g.shortest_path_filtered("A", "D", bus).unwrap().unwrap();
        assert_eq!(r.cost, 3.0);
        assert_eq!(r.resolve_labels(&g), vec!["A", "B", "C", "D"]);
    }

    // ---------------------------------------------------------------------------
    // shortest_path_filtered — filter blocks source or destination
    // ---------------------------------------------------------------------------

    #[test]
    fn filtered_source_blocked_returns_none() {
        // A has no "ferry" stop, so no path can start there.
        let g = city();
        let ferry = |attrs: &NodeAttrs| attrs.contains("ferry");
        assert!(g.shortest_path_filtered("A", "D", ferry).unwrap().is_none());
    }

    #[test]
    fn filtered_destination_blocked_returns_none() {
        // B has no taxi stop, so we can never arrive there by taxi.
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        assert!(g.shortest_path_filtered("A", "B", taxi).unwrap().is_none());
    }

    // ---------------------------------------------------------------------------
    // shortest_path_filtered — no viable path after filtering
    // ---------------------------------------------------------------------------

    #[test]
    fn filtered_all_paths_blocked_returns_none() {
        // Every path from A to D goes through B or C; if we require "ferry"
        // (which no node has), there is no viable path.
        let g = city();
        let ferry = |attrs: &NodeAttrs| attrs.contains("ferry");
        assert!(g.shortest_path_filtered("A", "D", ferry).unwrap().is_none());
    }

    #[test]
    fn filtered_same_node_passes_filter_returns_zero_cost() {
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        let r = g.shortest_path_filtered("A", "A", taxi).unwrap().unwrap();
        assert_eq!(r.cost, 0.0);
        assert_eq!(r.resolve_labels(&g), vec!["A"]);
    }

    // ---------------------------------------------------------------------------
    // shortest_path_cost — cost agrees with full path variant
    // ---------------------------------------------------------------------------

    #[test]
    fn cost_only_agrees_with_full_path() {
        let g = diamond();
        let full = g.shortest_path("a", "d").unwrap().unwrap();
        let cost = g.shortest_path_cost("a", "d").unwrap().unwrap();
        assert_eq!(cost, full.cost);
    }

    #[test]
    fn cost_only_no_path_returns_none() {
        assert!(diamond().shortest_path_cost("d", "a").unwrap().is_none());
    }

    #[test]
    fn cost_only_same_node_returns_zero() {
        assert_eq!(diamond().shortest_path_cost("b", "b").unwrap(), Some(0.0));
    }

    #[test]
    fn cost_only_unknown_node_returns_error() {
        assert!(matches!(
            diamond().shortest_path_cost("a", "z"),
            Err(GraphError::UnknownNode(_))
        ));
    }

    // ---------------------------------------------------------------------------
    // shortest_path_filtered_cost — cost agrees with filtered full path variant
    // ---------------------------------------------------------------------------

    #[test]
    fn filtered_cost_only_agrees_with_filtered_full_path() {
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        let full = g.shortest_path_filtered("A", "D", taxi).unwrap().unwrap();
        let cost = g
            .shortest_path_filtered_cost("A", "D", taxi)
            .unwrap()
            .unwrap();
        assert_eq!(cost, full.cost);
    }

    #[test]
    fn filtered_cost_only_source_blocked_returns_none() {
        let g = city();
        let ferry = |attrs: &NodeAttrs| attrs.contains("ferry");
        assert!(
            g.shortest_path_filtered_cost("A", "D", ferry)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn filtered_cost_only_destination_blocked_returns_none() {
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        assert!(
            g.shortest_path_filtered_cost("A", "B", taxi)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn filtered_cost_only_same_node_returns_zero() {
        let g = city();
        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        assert_eq!(
            g.shortest_path_filtered_cost("A", "A", taxi).unwrap(),
            Some(0.0)
        );
    }

    // ---------------------------------------------------------------------------
    // Thread safety — Arc<Graph> concurrent queries
    // ---------------------------------------------------------------------------

    #[test]
    fn concurrent_queries_produce_correct_results() {
        use std::sync::Arc;

        let graph = Arc::new(diamond());

        // Spawn one thread per query; all share the same Arc<Graph>.
        let queries: &[(&str, &str, Option<f64>)] = &[
            ("a", "d", Some(4.0)),
            ("a", "b", Some(1.0)),
            ("a", "c", Some(3.0)),
            ("b", "d", Some(3.0)),
            ("d", "a", None), // unreachable in a DAG
        ];

        let handles: Vec<_> = queries
            .iter()
            .map(|&(from, to, expected_cost)| {
                let g = Arc::clone(&graph);
                std::thread::spawn(move || {
                    let cost = g.shortest_path_cost(from, to).unwrap();
                    assert_eq!(cost, expected_cost, "{from} → {to}");
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread panicked");
        }
    }

    #[test]
    fn concurrent_filtered_queries_produce_correct_results() {
        use std::sync::Arc;

        let graph = Arc::new(city());

        let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
        let bus = |attrs: &NodeAttrs| attrs.contains("bus");

        // Taxi: A→D costs 5 (B has no taxi stop, so A→B→C→D is blocked).
        // Bus:  A→D costs 3 (all nodes have bus stops).
        let (g1, g2) = (Arc::clone(&graph), Arc::clone(&graph));
        let t1 =
            std::thread::spawn(move || g1.shortest_path_filtered_cost("A", "D", taxi).unwrap());
        let t2 = std::thread::spawn(move || g2.shortest_path_filtered_cost("A", "D", bus).unwrap());

        assert_eq!(t1.join().unwrap(), Some(5.0));
        assert_eq!(t2.join().unwrap(), Some(3.0));
    }
}
