mod common;
use grafo::{Graph, GraphError, PathResult};

#[test]
fn resolve_labels_modified_indices_return_first_invalid_index() {
    // Given
    let graph = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
    let mut path = graph.shortest_path("a", "b").unwrap().unwrap();
    path.indices.extend([7, u32::MAX]);
    // When
    let error = path.resolve_labels(&graph).unwrap_err();
    // Then
    assert_eq!(error.to_string(), "invalid node index: 7");
    assert!(matches!(error, GraphError::InvalidNodeIndex(7)));
}

#[test]
fn resolve_labels_empty_or_shorter_foreign_graph_returns_checked_error() {
    // Given — bounds checks cannot identify a same-sized foreign graph.
    let graph = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
    let path = graph.shortest_path("a", "b").unwrap().unwrap();
    let empty = Graph::new(&[], &[]).unwrap();
    let shorter = Graph::new(&["x"], &[]).unwrap();
    // When
    let errors = [
        path.resolve_labels(&empty).unwrap_err(),
        path.resolve_labels(&shorter).unwrap_err(),
    ];
    // Then
    for (error, index) in errors.into_iter().zip([0, 1]) {
        assert!(matches!(error, GraphError::InvalidNodeIndex(actual) if actual == index));
    }
}

#[test]
fn resolve_labels_preserves_originating_graph_order_and_empty_results() {
    // Given
    let graph = Graph::new(&["b", "a"], &[("a", "b", 1.0)]).unwrap();
    let path = graph.shortest_path("a", "b").unwrap().unwrap();
    let empty = PathResult {
        cost: 0.0,
        indices: vec![],
    };
    // When
    let labels = path.resolve_labels(&graph).unwrap();
    let no_labels = empty.resolve_labels(&graph).unwrap();
    // Then
    assert_eq!(labels, ["a", "b"]);
    assert!(no_labels.is_empty());
}

#[test]
fn shortest_path_consumer_returns_ordered_indices_and_comparable_results() {
    // Given
    let graph = common::competing_routes();
    // When
    let path = graph.shortest_path("a", "d").unwrap().unwrap();
    let same = graph.shortest_path("a", "d").unwrap().unwrap();
    let self_path = graph.shortest_path("a", "a").unwrap().unwrap();
    let unreachable = graph.shortest_path("d", "a").unwrap();
    // Then
    assert_eq!(path.cost, 3.0);
    assert_eq!(path.indices, [0, 2, 1, 3]);
    assert_eq!(path.resolve_labels(&graph).unwrap(), ["a", "c", "b", "d"]);
    assert_eq!(path, same);
    assert_ne!(path, self_path);
    assert_eq!(
        self_path,
        PathResult {
            cost: 0.0,
            indices: vec![0]
        }
    );
    assert_eq!(unreachable, None);
}
