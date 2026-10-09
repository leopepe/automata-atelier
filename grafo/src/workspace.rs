use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Caller-owned reusable scratch storage for cost-only shortest-path searches.
///
/// Reuse with [`Graph::shortest_path_cost_with_workspace`](crate::Graph::shortest_path_cost_with_workspace)
/// or [`Graph::shortest_path_filtered_cost_with_workspace`](crate::Graph::shortest_path_filtered_cost_with_workspace)
/// to avoid initializing
/// every distance on each query. The graph remains immutable. Give each
/// simultaneous worker a separate mutable workspace.
///
/// First use and growth initialize buffers. Distance/stamp elements use
/// 12 bytes per retained vertex slot. Buffers retain their peak size, and vector
/// and heap capacity may exceed their lengths. Allocator overhead is additional.
/// A rare generation rollover clears all retained stamps. Drop the workspace
/// to release its memory. Warm narrow
/// searches can benefit; cold, broad and mixed searches may not.
///
/// A workspace is not tied to a graph: reuse across sizes, node-ID reorderings
/// and different graphs is supported, including after a filter panic unwinds.
/// Existing one-shot and full-path methods are unchanged.
///
/// # Examples
///
/// ```
/// use grafo::{Graph, SearchWorkspace};
/// let graph = Graph::new(&["a", "b"], &[("a", "b", 2.0)]).unwrap();
/// let mut workspace = SearchWorkspace::default();
/// assert_eq!(graph.shortest_path_cost_with_workspace("a", "b", &mut workspace).unwrap(), Some(2.0));
/// assert_eq!(graph.shortest_path_cost_with_workspace("b", "a", &mut workspace).unwrap(), None);
/// ```
#[derive(Debug, Default)]
pub struct SearchWorkspace {
    // Graph's search implementation shares this scratch storage within the crate.
    pub(crate) distances: Vec<f64>,
    pub(crate) stamps: Vec<u32>,
    pub(crate) generation: u32,
    pub(crate) heap: BinaryHeap<(Reverse<u64>, u32)>,
}

impl SearchWorkspace {
    /// Create an empty workspace without allocating search buffers.
    ///
    /// Buffers grow on the first nontrivial query and are reused thereafter.
    ///
    /// # Examples
    ///
    /// ```
    /// use grafo::{Graph, SearchWorkspace};
    /// let graph = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
    /// let mut workspace = SearchWorkspace::new();
    /// assert_eq!(graph.shortest_path_cost_with_workspace("a", "b", &mut workspace).unwrap(), Some(1.0));
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn begin(&mut self, nodes: usize) {
        // An early goal return or unwound predicate can leave pending entries.
        self.heap.clear();
        if self.distances.len() < nodes {
            self.distances.resize(nodes, 0.0);
            self.stamps.resize(nodes, 0);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Zero denotes unseen. Clear every retained slot, including those
            // outside a smaller current graph, before reusing generation one.
            self.stamps.fill(0);
            self.generation = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Graph;

    #[test]
    fn workspace_rollover_invalidates_old_distances_even_after_graph_shrink() {
        let first = Graph::new(&["a", "b", "c", "d"], &[("a", "b", 0.5), ("a", "d", 1.0)]).unwrap();
        let small = Graph::new(&["a", "b"], &[("a", "b", 2.0)]).unwrap();
        let later = Graph::new(&["a", "b", "c", "d"], &[("a", "d", 9.0)]).unwrap();
        let mut workspace = SearchWorkspace::new();
        // Seed retained slots in generation two: after wrap on the smaller
        // graph, expansion reaches two again and must not observe that tail.
        workspace.generation = 1;
        let initial = first
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap();
        workspace.generation = u32::MAX;
        let shrunk = small
            .shortest_path_cost_with_workspace("a", "b", &mut workspace)
            .unwrap();
        let expanded = later
            .shortest_path_cost_with_workspace("a", "d", &mut workspace)
            .unwrap();
        assert_eq!(initial, Some(1.0));
        assert_eq!(shrunk, Some(2.0));
        assert_eq!(expanded, Some(9.0));
        assert_eq!(workspace.stamps[3], workspace.generation);
    }

    #[test]
    fn workspace_warm_queries_retain_buffers_and_do_not_fill_unused_distances() {
        let graph = Graph::new(
            &["a", "b", "c", "d"],
            &[("a", "b", 1.0), ("a", "c", 10.0), ("c", "d", 1.0)],
        )
        .unwrap();
        let mut workspace = SearchWorkspace::new();
        let first = graph
            .shortest_path_cost_with_workspace("a", "b", &mut workspace)
            .unwrap();
        let distances = workspace.distances.as_ptr();
        let stamps = workspace.stamps.as_ptr();
        let heap_capacity = workspace.heap.capacity();
        workspace.distances[3] = 123.0;
        workspace.stamps[3] = workspace.generation;
        let next = graph
            .shortest_path_cost_with_workspace("b", "d", &mut workspace)
            .unwrap();
        assert_eq!(first, Some(1.0));
        assert_eq!(next, None);
        assert_eq!(workspace.distances.as_ptr(), distances);
        assert_eq!(workspace.stamps.as_ptr(), stamps);
        assert_eq!(workspace.heap.capacity(), heap_capacity);
        assert!(workspace.heap.is_empty());
        assert_eq!(workspace.distances[3], 123.0);
        assert_ne!(workspace.stamps[3], workspace.generation);
    }

    #[test]
    fn workspace_self_and_rejected_queries_do_not_allocate_buffers() {
        let graph = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
        let mut workspace = SearchWorkspace::new();
        let same = graph
            .shortest_path_cost_with_workspace("a", "a", &mut workspace)
            .unwrap();
        let rejected = graph
            .shortest_path_filtered_cost_with_workspace("a", "b", |_| false, &mut workspace)
            .unwrap();
        assert_eq!(same, Some(0.0));
        assert_eq!(rejected, None);
        assert_eq!(workspace.distances.capacity(), 0);
        assert_eq!(workspace.stamps.capacity(), 0);
        assert_eq!(workspace.heap.capacity(), 0);
    }
}
