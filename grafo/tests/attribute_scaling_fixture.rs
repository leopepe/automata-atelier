#[path = "../benches/support/attribute_scaling.rs"]
mod attribute_scaling;

#[test]
fn attribute_scaling_fixture_has_exact_distinct_cardinality_on_every_node() {
    // Given — this is the generator imported by the actual Criterion harness.
    for count in [1, 5, 10, 20, 50] {
        let graph = attribute_scaling::attribute_scaling_dag(24, 4, count).graph;
        for node in 0..24 {
            let cardinalities = std::cell::RefCell::new(Vec::new());
            // When
            let path = graph
                .shortest_path_filtered(&node.to_string(), &node.to_string(), |attrs| {
                    cardinalities.borrow_mut().push((
                        attrs.len(),
                        attrs.iter().collect::<std::collections::HashSet<_>>().len(),
                    ));
                    true
                })
                .unwrap()
                .unwrap();
            // Then
            assert_eq!(path.cost, 0.0);
            assert_eq!(*cardinalities.borrow(), [(count, count); 2]);
        }
    }
}

#[test]
fn attribute_scaling_fixture_has_controlled_taxi_route_independent_of_count() {
    // Given
    for count in [1, 5, 10, 20, 50] {
        let graph = attribute_scaling::attribute_scaling_dag(24, 4, count).graph;
        // When
        let path = graph
            .shortest_path_filtered("0", "23", |attrs| attrs.contains("taxi"))
            .unwrap();
        // Then
        let path = path.expect("both endpoints and nontrivial taxi route must be accepted");
        assert_eq!(path.cost, 23.0);
        assert_eq!(path.indices, (0..24).collect::<Vec<_>>());
    }
}

#[test]
fn legacy_filter_cost_control_still_exercises_the_same_graph() {
    // Given — keep non-scaling attributed workloads on their original generator.
    let graph = attribute_scaling::attributed_dag(24, 4, 5);
    // When
    let plain = graph.shortest_path("0", "23").unwrap().unwrap();
    let pass_all = graph
        .shortest_path_filtered("0", "23", |_| true)
        .unwrap()
        .unwrap();
    // Then
    assert_eq!(plain, pass_all);
    assert!(plain.indices.len() > 1);
}

#[test]
fn attribute_scaling_fixture_advertised_fifty_tags_are_distinct() {
    let graph = attribute_scaling::attribute_scaling_dag(24, 4, 50).graph;
    let observed = std::cell::Cell::new(0);
    graph
        .shortest_path_filtered("0", "0", |attrs| {
            observed.set(attrs.len());
            true
        })
        .unwrap();
    assert_eq!(observed.get(), 50);
}

#[test]
fn attribute_scaling_fixture_keeps_actual_topology_and_weights_fixed() {
    let fixtures =
        [1, 5, 10, 20, 50].map(|count| attribute_scaling::attribute_scaling_dag(24, 4, count));
    for (fixture, count) in fixtures.iter().zip([1, 5, 10, 20, 50]) {
        attribute_scaling::validate_attribute_scaling(fixture, 24, count);
        assert_eq!(fixture.edges, fixtures[0].edges);
        for &(from, to, weight) in &fixture.edges {
            assert!(from < to && to < 24);
            assert_eq!(
                weight,
                if to == from + 1 {
                    1.0
                } else {
                    (to - from + 1) as f64
                }
            );
        }
        for from in 0..24 {
            for to in 0..24 {
                let cost = fixture
                    .graph
                    .shortest_path_cost(&from.to_string(), &to.to_string())
                    .unwrap();
                assert_eq!(cost, (to >= from).then_some((to as f64) - (from as f64)));
            }
        }
    }
}
