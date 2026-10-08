use super::{Graph, GraphError, NodeAttrs};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Caller-owned reusable scratch storage for cost-only shortest-path searches.
///
/// Reuse with [`Graph::shortest_path_cost_with_workspace`] or
/// [`Graph::shortest_path_filtered_cost_with_workspace`] to avoid initializing
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
    distances: Vec<f64>,
    stamps: Vec<u32>,
    generation: u32,
    heap: BinaryHeap<(Reverse<u64>, u32)>,
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

    fn begin(&mut self, nodes: usize) {
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

impl Graph {
    /// Find a minimum path cost using caller-owned reusable search storage.
    ///
    /// This opt-in alternative to [`Graph::shortest_path_cost`] reuses buffers
    /// and initializes only discovered distances after setup. Returns `Ok(None)`
    /// when unreachable. See [`SearchWorkspace`] for memory and setup trade-offs.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] if either endpoint is absent, checking
    /// the source first. A failed query does not prevent later workspace reuse.
    ///
    /// # Examples
    ///
    /// ```
    /// use grafo::{Graph, SearchWorkspace};
    /// let graph = Graph::new(&["a", "b", "c"], &[("a", "b", 1.0), ("b", "c", 2.0)]).unwrap();
    /// let mut workspace = SearchWorkspace::new();
    /// assert_eq!(graph.shortest_path_cost_with_workspace("a", "c", &mut workspace).unwrap(), Some(3.0));
    /// assert_eq!(graph.shortest_path_cost_with_workspace("a", "a", &mut workspace).unwrap(), Some(0.0));
    /// ```
    pub fn shortest_path_cost_with_workspace(
        &self,
        from: &str,
        to: &str,
        workspace: &mut SearchWorkspace,
    ) -> Result<Option<f64>, GraphError> {
        self.shortest_path_filtered_cost_with_workspace(from, to, |_| true, workspace)
    }

    /// Find a filtered minimum path cost using reusable search storage.
    ///
    /// Semantics match [`Graph::shortest_path_filtered_cost`]: resolve endpoints
    /// first, apply the predicate to both endpoints, then consider waypoints.
    /// Returns `Ok(None)` if an endpoint is rejected or the goal is unreachable.
    /// Accepted self queries return zero without initializing buffers.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] for an absent endpoint, source first.
    ///
    /// # Panics
    ///
    /// Propagates a panic from `filter`. If the panic unwinds and the caller
    /// catches it, the workspace can safely be reused for a later query.
    ///
    /// # Examples
    ///
    /// ```
    /// use grafo::{Graph, SearchWorkspace};
    /// let graph = Graph::new_with_attrs(
    ///     &[("a", &["taxi"][..]), ("b", &["bus"][..]), ("c", &["taxi"][..])],
    ///     &[("a", "b", 1.0), ("b", "c", 1.0), ("a", "c", 5.0)],
    /// ).unwrap();
    /// let mut workspace = SearchWorkspace::new();
    /// assert_eq!(graph.shortest_path_filtered_cost_with_workspace(
    ///     "a", "c", |attrs| attrs.contains("taxi"), &mut workspace,
    /// ).unwrap(), Some(5.0));
    /// ```
    pub fn shortest_path_filtered_cost_with_workspace<F>(
        &self,
        from: &str,
        to: &str,
        filter: F,
        workspace: &mut SearchWorkspace,
    ) -> Result<Option<f64>, GraphError>
    where
        F: Fn(&NodeAttrs) -> bool,
    {
        let src = *self
            .node_index
            .get(from)
            .ok_or_else(|| GraphError::UnknownNode(from.to_string()))?;
        let dst = *self
            .node_index
            .get(to)
            .ok_or_else(|| GraphError::UnknownNode(to.to_string()))?;
        if !filter(&self.node_attrs[src as usize]) || !filter(&self.node_attrs[dst as usize]) {
            return Ok(None);
        }
        if src == dst {
            return Ok(Some(0.0));
        }
        Ok(self.dijkstra_cost_with_workspace(src, dst, &filter, workspace))
    }

    fn dijkstra_cost_with_workspace<F>(
        &self,
        src: u32,
        dst: u32,
        filter: &F,
        workspace: &mut SearchWorkspace,
    ) -> Option<f64>
    where
        F: Fn(&NodeAttrs) -> bool,
    {
        workspace.begin(self.node_ids.len());
        let generation = workspace.generation;
        workspace.distances[src as usize] = 0.0;
        workspace.stamps[src as usize] = generation;
        workspace.heap.push((Reverse(0_u64), src));
        while let Some((Reverse(bits), node)) = workspace.heap.pop() {
            let cost = f64::from_bits(bits);
            // Every heap entry was stamped in this generation before insertion.
            if cost > workspace.distances[node as usize] {
                continue;
            }
            if node == dst {
                return Some(cost);
            }
            let start = self.offsets[node as usize] as usize;
            let end = self.offsets[node as usize + 1] as usize;
            for edge in start..end {
                let next = self.targets[edge];
                let index = next as usize;
                if !filter(&self.node_attrs[index]) {
                    continue;
                }
                let previous = if workspace.stamps[index] == generation {
                    workspace.distances[index]
                } else {
                    f64::INFINITY
                };
                let candidate = cost + self.weights[edge];
                // Comparing against infinity (rather than accepting every unseen
                // node) preserves the existing overflow/NaN rejection behavior.
                if candidate < previous {
                    workspace.distances[index] = candidate;
                    workspace.stamps[index] = generation;
                    workspace.heap.push((Reverse(candidate.to_bits()), next));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_rollover_invalidates_old_distances_even_after_graph_shrink() {
        let first = Graph::new(&["a", "b", "c", "d"], &[("a", "b", 0.5), ("a", "d", 1.0)]).unwrap();
        let small = Graph::new(&["a", "b"], &[("a", "b", 2.0)]).unwrap();
        let later = Graph::new(&["a", "b", "c", "d"], &[("a", "d", 9.0)]).unwrap();
        let mut workspace = SearchWorkspace::new();
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
