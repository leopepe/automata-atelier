mod common;
use grafo::{Graph, NodeAttrs, SearchWorkspace};
use std::cell::RefCell;

#[test]
fn overflow_filters_follow_one_traversal_without_predicate_replay() {
    // Given — cycle ensures equal infinite distances are not repeatedly queued.
    let graph = Graph::new_with_attrs(
        &[
            ("s", &["s"][..]),
            ("b", &["b"][..]),
            ("c", &["c"][..]),
            ("g", &["g"][..]),
        ],
        &[
            ("s", "b", f64::MAX),
            ("b", "c", f64::MAX),
            ("c", "g", 0.0),
            ("c", "b", 0.0),
        ],
    )
    .unwrap();
    for variant in 0..3 {
        let calls = RefCell::new(Vec::new());
        let filter = |attrs: &NodeAttrs| {
            calls
                .borrow_mut()
                .push(attrs.iter().next().unwrap().clone());
            true
        };
        let mut workspace = SearchWorkspace::new();
        // When
        let result = match variant {
            0 => graph
                .shortest_path_filtered("s", "g", filter)
                .map(|p| p.map(|p| p.cost)),
            1 => graph.shortest_path_filtered_cost("s", "g", filter),
            _ => graph.shortest_path_filtered_cost_with_workspace("s", "g", filter, &mut workspace),
        };
        // Then
        assert_eq!(result.unwrap_err().to_string(), "path cost overflow");
        assert_eq!(*calls.borrow(), ["s", "g", "b", "c", "g", "b"]);
    }
}

#[test]
fn filtered_path_prunes_waypoints_and_returns_exact_route() {
    // Given
    let graph = common::competing_routes();
    // When
    let restricted = graph
        .shortest_path_filtered("a", "d", |attrs| attrs.contains("ok"))
        .unwrap()
        .unwrap();
    let pass_all = graph
        .shortest_path_filtered("a", "d", |_| true)
        .unwrap()
        .unwrap();
    let absent = graph.shortest_path_filtered("z", "d", |_| true).unwrap();
    // Then
    assert_eq!(restricted.cost, 10.0);
    assert_eq!(restricted.indices, [0, 2, 3]);
    assert_eq!(pass_all.cost, 3.0);
    assert_eq!(pass_all.indices, [0, 2, 1, 3]);
    assert_eq!(absent, None);
}

#[test]
fn filtered_endpoint_checks_short_circuit_before_self_success() {
    // Given
    let graph = common::competing_routes();
    for variant in 0..2 {
        for (reject_at, expected_calls, expected) in
            [(Some(1), 1, None), (Some(2), 2, None), (None, 2, Some(0.0))]
        {
            let calls = std::cell::Cell::new(0);
            let filter = |_: &NodeAttrs| {
                calls.set(calls.get() + 1);
                reject_at != Some(calls.get())
            };
            // When
            let result = if variant == 0 {
                graph
                    .shortest_path_filtered("a", "a", filter)
                    .map(|p| p.map(|p| p.cost))
            } else {
                graph.shortest_path_filtered_cost("a", "a", filter)
            }
            .unwrap();
            // Then
            assert_eq!(result, expected);
            assert_eq!(calls.get(), expected_calls);
        }
    }
}
