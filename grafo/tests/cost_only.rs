mod common;
use grafo::{Graph, GraphError, NodeAttrs, SearchWorkspace};

fn six_queries(graph: &Graph, from: &str, to: &str) -> [Result<Option<f64>, GraphError>; 6] {
    let mut workspace = SearchWorkspace::new();
    [
        graph
            .shortest_path(from, to)
            .map(|path| path.map(|p| p.cost)),
        graph.shortest_path_cost(from, to),
        graph
            .shortest_path_filtered(from, to, |_| true)
            .map(|path| path.map(|p| p.cost)),
        graph.shortest_path_filtered_cost(from, to, |_| true),
        graph.shortest_path_cost_with_workspace(from, to, &mut workspace),
        graph.shortest_path_filtered_cost_with_workspace(from, to, |_| true, &mut workspace),
    ]
}

#[test]
fn overflow_only_route_returns_numerical_error_across_six_queries() {
    // Given — infinite distances must propagate over more than the first overflow edge.
    let graph = Graph::new(
        &["s", "b", "c", "g"],
        &[
            ("s", "b", f64::MAX),
            ("b", "c", f64::MAX),
            ("c", "g", 0.0),
            ("c", "b", 0.0),
        ],
    )
    .unwrap();
    // When
    let results = six_queries(&graph, "s", "g");
    // Then
    for result in results {
        let error = result.unwrap_err();
        assert_eq!(error.to_string(), "path cost overflow");
        assert!(matches!(error, GraphError::CostOverflow));
    }
}

#[test]
fn finite_alternative_discovered_after_overflow_wins_across_six_queries() {
    // Given — equal MAX heap keys pop the higher-index overflow node first.
    let graph = Graph::new(
        &["s", "finite", "overflow", "g"],
        &[
            ("s", "overflow", f64::MAX),
            ("s", "finite", f64::MAX),
            ("overflow", "g", f64::MAX),
            ("finite", "g", 0.0),
        ],
    )
    .unwrap();
    // When
    let results = six_queries(&graph, "s", "g");
    let path = graph.shortest_path("s", "g").unwrap().unwrap();
    // Then
    for result in results {
        assert_eq!(result.unwrap(), Some(f64::MAX));
    }
    assert_eq!(path.indices, [0, 1, 3]);
}

#[test]
fn unrelated_overflow_cycle_does_not_make_disconnected_goal_an_error() {
    // Given
    let graph = Graph::new(
        &["s", "b", "c", "g"],
        &[("s", "b", f64::MAX), ("b", "c", f64::MAX), ("c", "b", 0.0)],
    )
    .unwrap();
    // When
    let results = six_queries(&graph, "s", "g");
    // Then
    for result in results {
        assert_eq!(result.unwrap(), None);
    }
}

#[test]
fn zero_max_and_self_queries_remain_finite_across_six_queries() {
    // Given
    let graph = Graph::new(
        &["s", "zero", "g"],
        &[("s", "g", 1.0), ("s", "zero", -0.0), ("zero", "g", -0.0)],
    )
    .unwrap();
    let max = Graph::new(&["s", "g"], &[("s", "g", f64::MAX)]).unwrap();
    // When
    let results = [
        (six_queries(&graph, "s", "g"), 0.0),
        (six_queries(&max, "s", "g"), f64::MAX),
        (six_queries(&max, "s", "s"), 0.0),
    ];
    // Then
    for (queries, expected) in results {
        for result in queries {
            let cost = result.unwrap().unwrap();
            assert_eq!(cost, expected);
            assert_eq!(cost.to_bits(), expected.to_bits());
        }
    }
}

#[test]
fn filtered_rejected_waypoint_or_endpoint_is_absence_not_overflow() {
    // Given
    let graph = Graph::new_with_attrs(
        &[
            ("s", &["ok"][..]),
            ("b", &["blocked"][..]),
            ("g", &["ok"][..]),
        ],
        &[("s", "b", f64::MAX), ("b", "g", f64::MAX)],
    )
    .unwrap();
    let mut workspace = SearchWorkspace::new();
    let filter = |attrs: &NodeAttrs| attrs.contains("ok");
    // When
    let results = [("s", "g"), ("s", "b"), ("b", "g"), ("b", "b")].map(|(from, to)| {
        [
            graph
                .shortest_path_filtered(from, to, filter)
                .map(|p| p.map(|p| p.cost)),
            graph.shortest_path_filtered_cost(from, to, filter),
            graph.shortest_path_filtered_cost_with_workspace(from, to, filter, &mut workspace),
        ]
    });
    // Then
    for queries in results {
        for result in queries {
            assert_eq!(result.unwrap(), None);
        }
    }
}

#[test]
fn one_shot_unknown_labels_preserve_source_first_errors() {
    // Given
    let graph = common::competing_routes();
    // When
    let results = [("absent", "missing", "absent"), ("a", "missing", "missing")].map(
        |(from, to, expected)| {
            (
                expected,
                [
                    graph.shortest_path(from, to).map(|p| p.map(|p| p.cost)),
                    graph.shortest_path_cost(from, to),
                    graph
                        .shortest_path_filtered(from, to, |_| {
                            panic!("unknown endpoints must precede filter")
                        })
                        .map(|p| p.map(|p| p.cost)),
                    graph.shortest_path_filtered_cost(from, to, |_| {
                        panic!("unknown endpoints must precede filter")
                    }),
                ],
            )
        },
    );
    // Then
    for (expected, queries) in results {
        for result in queries {
            assert!(matches!(result, Err(GraphError::UnknownNode(label)) if label == expected));
        }
    }
}

#[test]
fn one_shot_cost_variants_return_hand_computed_finite_or_absent_results() {
    // Given
    let graph = common::competing_routes();
    // When
    let results = [
        graph.shortest_path_cost("a", "d").unwrap(),
        graph.shortest_path_cost("d", "a").unwrap(),
        graph
            .shortest_path_filtered_cost("a", "d", |a| a.contains("ok"))
            .unwrap(),
        graph
            .shortest_path_filtered_cost("z", "d", |_| true)
            .unwrap(),
        graph
            .shortest_path_filtered_cost("a", "a", |_| true)
            .unwrap(),
    ];
    // Then
    assert_eq!(results, [Some(3.0), None, Some(10.0), None, Some(0.0)]);
}
