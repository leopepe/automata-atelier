mod common;
use grafo::{Graph, GraphError};

fn construct(with_attrs: bool, edges: &[(&str, &str, f64)]) -> Result<Graph, GraphError> {
    if with_attrs {
        Graph::new_with_attrs(&[("a", &["taxi"][..]), ("b", &[][..])], edges)
    } else {
        Graph::new(&["a", "b"], edges)
    }
}

#[test]
fn invalid_weights_are_recoverable_in_both_constructor_branches() {
    // Given — one bad edge in either the sequential or Rayon branch.
    for with_attrs in [false, true] {
        for edge_count in [1, 9_999, 10_000] {
            for weight in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut edges = vec![("a", "b", 1.0); edge_count];
                edges[edge_count - 1].2 = weight;
                // When
                let error = construct(with_attrs, &edges).unwrap_err();
                // Then
                assert_eq!(
                    error.to_string(),
                    format!(
                        "invalid weight on edge 'a' -> 'b': {weight}; expected finite non-negative weight"
                    )
                );
                assert!(
                    matches!(error, GraphError::InvalidWeight { from, to, weight: actual }
                    if from == "a" && to == "b" && (actual == weight || (actual.is_nan() && weight.is_nan())))
                );
            }
        }
    }
}

#[test]
fn valid_weight_boundaries_build_in_both_constructor_branches() {
    // Given
    for with_attrs in [false, true] {
        for edge_count in [1, 10_000] {
            for weight in [-0.0, 0.0, f64::MAX] {
                let edges = vec![("a", "b", weight); edge_count];
                // When
                let graph = construct(with_attrs, &edges).unwrap();
                let cost = graph.shortest_path_cost("a", "b").unwrap();
                // Then
                assert_eq!(cost, Some(weight));
            }
        }
    }
}

#[test]
fn unknown_endpoints_precede_invalid_weights_in_both_constructor_branches() {
    // Given
    for with_attrs in [false, true] {
        for edge_count in [1, 10_000] {
            for (edge, expected) in [
                (
                    ("missing-source", "missing-goal", f64::NAN),
                    "missing-source",
                ),
                (("a", "missing-goal", -1.0), "missing-goal"),
            ] {
                let mut edges = vec![("a", "b", 1.0); edge_count];
                edges[edge_count - 1] = edge;
                // When
                let error = construct(with_attrs, &edges).unwrap_err();
                // Then
                assert!(matches!(error, GraphError::UnknownNode(label) if label == expected));
            }
        }
    }
}

#[test]
fn duplicate_nodes_precede_edge_errors_for_both_constructors() {
    // Given
    let edges = [("missing", "a", f64::NAN)];
    // When
    let errors = [
        Graph::new(&["a", "a"], &edges).unwrap_err(),
        Graph::new_with_attrs(&[("a", &[][..]), ("a", &[][..])], &edges).unwrap_err(),
    ];
    // Then
    for error in errors {
        assert!(matches!(error, GraphError::DuplicateNode(label) if label == "a"));
    }
}

#[test]
fn node_attrs_consumer_observes_distinct_set_membership_and_empty_boundaries() {
    // Given
    let graph =
        Graph::new_with_attrs(&[("tagged", &["taxi", "taxi", "", "日本語"][..])], &[]).unwrap();
    let observed = std::cell::RefCell::new(Vec::new());
    // When
    let result = graph
        .shortest_path_filtered("tagged", "tagged", |attrs| {
            observed.borrow_mut().push((
                attrs.len(),
                attrs.is_empty(),
                attrs
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>(),
                attrs.contains("taxi"),
                attrs.contains("Taxi"),
                attrs.contains(""),
                attrs.contains("日本語"),
                attrs.contains("missing"),
            ));
            true
        })
        .unwrap();
    let empty = grafo::NodeAttrs::default();
    // Then
    assert_eq!(result.unwrap().cost, 0.0);
    let expected = (
        3,
        false,
        ["taxi", "", "日本語"]
            .map(String::from)
            .into_iter()
            .collect(),
        true,
        false,
        true,
        true,
        false,
    );
    assert_eq!(*observed.borrow(), vec![expected; 2]);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
    assert_eq!(empty.iter().count(), 0);
    assert!(!empty.contains(""));
}

#[test]
fn graph_error_diagnostics_preserve_variants_payloads_and_display() {
    // Given
    let duplicate = Graph::new(&["x", "x"], &[]).unwrap_err();
    let unknown = common::competing_routes()
        .shortest_path_cost("ghost", "d")
        .unwrap_err();
    let too_large = GraphError::GraphTooLarge {
        nodes: usize::MAX,
        edges: 4,
    };
    // When
    let messages = [
        duplicate.to_string(),
        unknown.to_string(),
        too_large.to_string(),
    ];
    // Then
    assert_eq!(
        messages,
        [
            "duplicate node: 'x'".to_string(),
            "unknown node: 'ghost'".to_string(),
            format!("graph too large: {} nodes, 4 edges", usize::MAX)
        ]
    );
    assert!(matches!(duplicate, GraphError::DuplicateNode(label) if label == "x"));
    assert!(matches!(unknown, GraphError::UnknownNode(label) if label == "ghost"));
    assert!(matches!(
        too_large,
        GraphError::GraphTooLarge {
            nodes: usize::MAX,
            edges: 4
        }
    ));
    let error: &dyn std::error::Error = &too_large;
    assert!(error.source().is_none());
}

#[test]
fn parallel_constructor_preserves_duplicate_priority() {
    // Given
    let edges = vec![("missing", "a", f64::NAN); 10_000];
    // When
    let errors = [
        Graph::new(&["a", "a"], &edges).unwrap_err(),
        Graph::new_with_attrs(&[("a", &[][..]), ("a", &[][..])], &edges).unwrap_err(),
    ];
    // Then
    for error in errors {
        assert!(matches!(error, GraphError::DuplicateNode(label) if label == "a"));
    }
}
