mod common;
use grafo::{Graph, GraphError, NodeAttrs, SearchWorkspace};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn workspace_repeated_queries_do_not_leak_distances_or_pending_heap_entries() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::new();
    let queries = [
        ("a", "d"),
        ("d", "a"),
        ("d", "z"),
        ("z", "z"),
        ("c", "d"),
        ("a", "d"),
        ("z", "a"),
    ];
    // When
    let results: Vec<_> = queries
        .into_iter()
        .map(|(from, to)| {
            graph
                .shortest_path_cost_with_workspace(from, to, &mut workspace)
                .unwrap()
        })
        .collect();
    // Then
    assert_eq!(
        results,
        [Some(3.0), None, None, Some(0.0), Some(2.0), Some(3.0), None]
    );
}

#[test]
fn workspace_unknown_endpoints_preserve_error_priority_and_allow_reuse() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::default();
    // When
    let errors = [("missing", "d"), ("a", "missing"), ("first", "second")].map(|(from, to)| {
        graph
            .shortest_path_cost_with_workspace(from, to, &mut workspace)
            .unwrap_err()
    });
    let recovered = graph
        .shortest_path_cost_with_workspace("a", "d", &mut workspace)
        .unwrap();
    // Then
    for (error, expected) in errors.into_iter().zip(["missing", "missing", "first"]) {
        assert!(matches!(error, GraphError::UnknownNode(label) if label == expected));
    }
    assert_eq!(recovered, Some(3.0));
}

#[test]
fn workspace_filtered_queries_prune_waypoints_and_change_predicates() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::new();
    // When
    let results = [true, false, true].map(|restrict| {
        graph
            .shortest_path_filtered_cost_with_workspace(
                "a",
                "d",
                |attrs| !restrict || attrs.contains("ok"),
                &mut workspace,
            )
            .unwrap()
    });
    // Then
    assert_eq!(results, [Some(10.0), Some(3.0), Some(10.0)]);
}

#[test]
fn workspace_filtered_endpoints_and_self_queries_obey_predicate() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::default();
    let filter = |attrs: &NodeAttrs| attrs.contains("ok");
    // When
    let results = [("b", "d"), ("a", "b"), ("b", "b"), ("a", "a"), ("z", "d")].map(|(from, to)| {
        graph
            .shortest_path_filtered_cost_with_workspace(from, to, filter, &mut workspace)
            .unwrap()
    });
    // Then
    assert_eq!(results, [None, None, None, Some(0.0), None]);
}

#[test]
fn workspace_filtered_unknown_endpoint_does_not_call_predicate() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::new();
    let calls = Cell::new(0);
    // When
    let error = graph
        .shortest_path_filtered_cost_with_workspace(
            "a",
            "missing",
            |_| {
                calls.set(calls.get() + 1);
                true
            },
            &mut workspace,
        )
        .unwrap_err();
    // Then
    assert!(matches!(error, GraphError::UnknownNode(label) if label == "missing"));
    assert_eq!(calls.get(), 0);
}

#[test]
fn workspace_self_query_preserves_two_endpoint_filter_evaluations() {
    // Given
    let graph = common::competing_routes();
    let mut workspace = SearchWorkspace::new();
    let calls = Cell::new(0);
    // When
    let result = graph
        .shortest_path_filtered_cost_with_workspace(
            "a",
            "a",
            |_| {
                calls.set(calls.get() + 1);
                true
            },
            &mut workspace,
        )
        .unwrap();
    // Then
    assert_eq!(result, Some(0.0));
    assert_eq!(calls.get(), 2);
}

#[test]
fn workspace_different_graphs_and_sizes_do_not_share_query_state() {
    // Given
    let first = common::competing_routes();
    let reordered = Graph::new(&["z", "d", "c", "b", "a"], &[("a", "d", 7.0)]).unwrap();
    let small = Graph::new(&["a", "d"], &[("a", "d", 4.0)]).unwrap();
    let labels: Vec<String> = (0..100).map(|i| format!("n{i}")).collect();
    let nodes: Vec<&str> = labels.iter().map(String::as_str).collect();
    let large = Graph::new(&nodes, &[("n0", "n99", 9.0)]).unwrap();
    let empty = Graph::new(&[], &[]).unwrap();
    let mut workspace = SearchWorkspace::new();
    // When
    let results = [
        first
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap(),
        reordered
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap(),
        small
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap(),
        large
            .shortest_path_cost_with_workspace("n0", "n99", &mut workspace)
            .unwrap(),
        first
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap(),
    ];
    let empty_error = empty
        .shortest_path_cost_with_workspace("a", "d", &mut workspace)
        .unwrap_err();
    let recovered = small
        .shortest_path_cost_with_workspace("a", "d", &mut workspace)
        .unwrap();
    // Then
    assert_eq!(
        results,
        [Some(3.0), Some(7.0), Some(4.0), Some(9.0), Some(3.0)]
    );
    assert!(matches!(empty_error, GraphError::UnknownNode(label) if label == "a"));
    assert_eq!(recovered, Some(4.0));
}

#[test]
fn workspace_filter_panic_during_search_allows_subsequent_reuse() {
    // Given — b is queued before evaluating the panicking c predicate.
    let graph = Graph::new_with_attrs(
        &[
            ("a", &["ok"][..]),
            ("b", &["ok"][..]),
            ("c", &["panic"][..]),
            ("d", &["ok"][..]),
        ],
        &[("a", "b", 10.0), ("a", "c", 20.0), ("b", "d", 1.0)],
    )
    .unwrap();
    let mut workspace = SearchWorkspace::new();
    // When
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        graph.shortest_path_filtered_cost_with_workspace(
            "a",
            "d",
            |attrs| {
                assert!(!attrs.contains("panic"), "interrupt waypoint filter");
                true
            },
            &mut workspace,
        )
    }));
    let unreachable = graph
        .shortest_path_cost_with_workspace("d", "b", &mut workspace)
        .unwrap();
    let result = graph
        .shortest_path_cost_with_workspace("a", "d", &mut workspace)
        .unwrap();
    // Then
    assert!(interrupted.is_err());
    assert_eq!(result, Some(11.0));
    assert_eq!(unreachable, None);
}

#[test]
fn workspace_numeric_boundaries_match_hand_computed_costs() {
    // Given
    let graph = Graph::new(
        &["a", "b", "c", "d"],
        &[("a", "b", -0.0), ("b", "c", 0.25), ("c", "d", 0.5)],
    )
    .unwrap();
    let overflow = Graph::new(
        &["a", "b", "c"],
        &[("a", "b", f64::MAX), ("b", "c", f64::MAX)],
    )
    .unwrap();
    let mut workspace = SearchWorkspace::default();
    // When
    let result = graph
        .shortest_path_cost_with_workspace("a", "d", &mut workspace)
        .unwrap();
    let finite = overflow
        .shortest_path_cost_with_workspace("a", "b", &mut workspace)
        .unwrap();
    let infinite = overflow
        .shortest_path_cost_with_workspace("a", "c", &mut workspace)
        .unwrap();
    // Then
    assert_eq!(result, Some(0.75));
    assert_eq!(finite, Some(f64::MAX));
    assert_eq!(infinite, None);
}

#[test]
fn workspace_all_pairs_on_reordered_dag_match_independent_reference() {
    // Given — labels are deliberately not in topological order.
    let nodes = ["c", "f", "a", "e", "b", "d"];
    let raw = [
        (2, 4, 0.0),
        (2, 0, 3.0),
        (4, 0, 0.25),
        (0, 5, 0.5),
        (4, 3, 4.0),
        (5, 3, 1.0),
        (3, 1, 2.0),
        (2, 1, 10.0),
    ];
    let edges: Vec<_> = raw
        .iter()
        .map(|&(a, b, cost)| (nodes[a], nodes[b], cost))
        .collect();
    let graph = Graph::new(&nodes, &edges).unwrap();
    let mut reference = [[f64::INFINITY; 6]; 6];
    for (i, row) in reference.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    for (a, b, cost) in raw {
        reference[a][b] = cost;
    }
    for k in 0..6 {
        for i in 0..6 {
            for j in 0..6 {
                reference[i][j] = reference[i][j].min(reference[i][k] + reference[k][j]);
            }
        }
    }
    let mut workspace = SearchWorkspace::new();
    // When
    let results: Vec<_> = nodes
        .iter()
        .flat_map(|from| nodes.iter().map(move |to| (*from, *to)))
        .map(|(from, to)| {
            graph
                .shortest_path_cost_with_workspace(from, to, &mut workspace)
                .unwrap()
        })
        .collect();
    // Then
    for (actual, expected) in results.into_iter().zip(reference.into_iter().flatten()) {
        assert_eq!(actual, expected.is_finite().then_some(expected));
    }
}

#[test]
fn workspace_independent_workers_share_immutable_graph_safely() {
    // Given
    let graph = common::competing_routes();
    // When
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let graph = &graph;
                scope.spawn(move || {
                    let mut workspace = SearchWorkspace::new();
                    (0..100)
                        .map(|_| {
                            graph
                                .shortest_path_cost_with_workspace("a", "d", &mut workspace)
                                .unwrap()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    // Then
    assert_eq!(results, vec![vec![Some(3.0); 100]; 4]);
}
