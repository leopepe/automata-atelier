use grafo::Graph;

/// Competing routes, stale heap entries, and an isolated tagged node.
pub fn competing_routes() -> Graph {
    Graph::new_with_attrs(
        &[
            ("a", &["ok"][..]),
            ("b", &["blocked"][..]),
            ("c", &["ok"][..]),
            ("d", &["ok"][..]),
            ("z", &["ok"][..]),
        ],
        &[
            ("a", "b", 10.0),
            ("a", "c", 1.0),
            ("c", "b", 1.0),
            ("b", "d", 1.0),
            ("c", "d", 9.0),
            ("a", "d", 20.0),
        ],
    )
    .unwrap()
}
