use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The set of attributes attached to a node, passed to filter predicates in
/// [`Graph::shortest_path_filtered`] and [`Graph::shortest_path_filtered_cost`].
///
/// Backed by a [`HashSet`] so membership checks via [`NodeAttrs::contains`]
/// are **O(1)** regardless of how many attributes a node carries. Prefer
/// `contains` over iterating manually.
///
/// # Example
///
/// ```
/// use grafo::Graph;
///
/// let g = Graph::new_with_attrs(
///     &[
///         ("a", &["taxi", "bus"][..]),
///         ("b", &["bus"][..]),
///         ("c", &["taxi", "bus"][..]),
///     ],
///     &[("a", "b", 1.0), ("b", "c", 2.0), ("a", "c", 10.0)],
/// )
/// .unwrap();
///
/// // O(1) membership check — no iteration needed.
/// let r = g
///     .shortest_path_filtered("a", "c", |attrs| attrs.contains("taxi"))
///     .unwrap()
///     .unwrap();
/// assert_eq!(r.cost, 10.0); // "b" has no taxi stop; direct a→c is used
/// ```
#[derive(Debug, Default)]
pub struct NodeAttrs(FxHashSet<String>);

impl NodeAttrs {
    /// Returns `true` if this node has the given attribute.
    ///
    /// This is an **O(1)** operation. Prefer it over
    /// `.iter().any(|a| a == attr)`.
    ///
    /// # Example
    ///
    /// ```
    /// use grafo::Graph;
    ///
    /// let g = Graph::new_with_attrs(
    ///     &[("city", &["taxi", "bus"][..])],
    ///     &[],
    /// )
    /// .unwrap();
    ///
    /// // The filter receives a &NodeAttrs; use .contains() for O(1) lookup.
    /// let result = g.shortest_path_filtered("city", "city", |attrs| attrs.contains("taxi"));
    /// assert!(result.unwrap().is_some());
    /// ```
    pub fn contains(&self, attr: &str) -> bool {
        self.0.contains(attr)
    }

    /// Iterates over every attribute of this node.
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }

    /// Returns the number of attributes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the node has no attributes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Errors that can occur when building or querying a [`Graph`].
#[derive(Debug)]
pub enum GraphError {
    /// A node label was provided more than once during construction.
    ///
    /// # Example
    /// ```
    /// use grafo::{Graph, GraphError};
    ///
    /// let err = Graph::new(&["a", "a"], &[]).unwrap_err();
    /// assert!(matches!(err, GraphError::DuplicateNode(_)));
    /// ```
    DuplicateNode(String),

    /// A node label referenced in an edge or a search query does not exist in
    /// the graph.
    ///
    /// # Example
    /// ```
    /// use grafo::{Graph, GraphError};
    ///
    /// let g = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
    /// let err = g.shortest_path("a", "z").unwrap_err();
    /// assert!(matches!(err, GraphError::UnknownNode(_)));
    /// ```
    UnknownNode(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::DuplicateNode(n) => write!(f, "duplicate node: '{n}'"),
            GraphError::UnknownNode(n) => write!(f, "unknown node: '{n}'"),
        }
    }
}

impl std::error::Error for GraphError {}

/// The result of a successful [`Graph::shortest_path`] or
/// [`Graph::shortest_path_filtered`] query.
///
/// Contains the minimum total cost and the ordered sequence of node labels
/// from source to destination. If you only need the cost, prefer
/// [`Graph::shortest_path_cost`] or [`Graph::shortest_path_filtered_cost`]
/// to avoid the overhead of path reconstruction.
#[derive(Debug, PartialEq)]
pub struct PathResult {
    /// Sum of edge weights along the shortest path.
    pub cost: f64,
    /// Ordered node indices from source (inclusive) to destination (inclusive).
    pub indices: Vec<u32>,
}

impl PathResult {
    /// Resolves the internal node indices back into their string labels.
    pub fn resolve_labels(&self, graph: &Graph) -> Vec<String> {
        self.indices
            .iter()
            .map(|&idx| graph.node_ids[idx as usize].clone())
            .collect()
    }
}

/// A directed acyclic graph (DAG) backed by Compressed Sparse Row (CSR) storage.
///
/// # Storage layout
///
/// CSR keeps all edges in a single flat array sorted by source node, with a
/// separate offset array to locate each node's edge slice. This gives
/// cache-friendly sequential access during Dijkstra's neighbour iteration and
/// uses O(V + E) memory — the minimum possible for a sparse graph.
///
/// Nodes may optionally carry a set of string **attributes** (e.g.
/// `"has_taxi_stop"`, `"has_bus_stop"`). Attributes are inert during a plain
/// [`Graph::shortest_path`] query but become active preconditions when using
/// [`Graph::shortest_path_filtered`].
///
/// # Search algorithm
///
/// Shortest-path queries use **Dijkstra's algorithm** with a binary min-heap
/// and lazy deletion. Time complexity is O((V + E) log V).
///
/// # Thread safety
///
/// `Graph` is `Send + Sync`. Once constructed it is fully immutable, so it can
/// be wrapped in an [`std::sync::Arc`] and shared across threads with zero
/// synchronisation overhead. All search methods take `&self` and are safe to
/// call concurrently from any number of threads.
///
/// ```
/// use std::sync::Arc;
/// use grafo::Graph;
///
/// let graph = Arc::new(
///     Graph::new(&["a", "b", "c"], &[("a", "b", 1.0), ("b", "c", 2.0)]).unwrap(),
/// );
///
/// let handles: Vec<_> = [("a", "b"), ("a", "c"), ("b", "c")]
///     .iter()
///     .map(|&(from, to)| {
///         let g = Arc::clone(&graph);
///         std::thread::spawn(move || g.shortest_path(from, to).unwrap())
///     })
///     .collect();
///
/// for handle in handles {
///     assert!(handle.join().unwrap().is_some());
/// }
/// ```
///
/// # Choosing the right search method
///
/// | Need | Method |
/// |---|---|
/// | Full path + cost | [`shortest_path`](Graph::shortest_path) |
/// | Full path + cost + filter | [`shortest_path_filtered`](Graph::shortest_path_filtered) |
/// | Cost only | [`shortest_path_cost`](Graph::shortest_path_cost) |
/// | Cost only + filter | [`shortest_path_filtered_cost`](Graph::shortest_path_filtered_cost) |
///
/// The cost-only variants skip path reconstruction entirely, eliminating the
/// ~21 ns per-hop `String` clone cost that dominates on long paths.
///
/// # Examples
///
/// **Graph without attributes:**
/// ```
/// use grafo::Graph;
///
/// let graph = Graph::new(
///     &["a", "b", "c", "d"],
///     &[
///         ("a", "b", 1.0),
///         ("a", "c", 4.0),
///         ("b", "c", 2.0),
///         ("b", "d", 5.0),
///         ("c", "d", 1.0),
///     ],
/// )
/// .unwrap();
///
/// let result = graph.shortest_path("a", "d").unwrap().unwrap();
/// assert_eq!(result.cost, 4.0);
/// assert_eq!(result.resolve_labels(&graph), vec!["a", "b", "c", "d"]);
/// ```
///
/// **Graph with attributes and a node filter:**
/// ```
/// use grafo::Graph;
///
/// let graph = Graph::new_with_attrs(
///     &[
///         ("a", &["taxi", "bus"][..]),
///         ("b", &["bus"][..]),
///         ("c", &["taxi", "bus"][..]),
///     ],
///     &[("a", "b", 1.0), ("a", "c", 4.0), ("b", "c", 2.0)],
/// )
/// .unwrap();
///
/// // Only traverse nodes that have a taxi stop.
/// let result = graph
///     .shortest_path_filtered("a", "c", |attrs| attrs.contains("taxi"))
///     .unwrap();
///
/// // "b" has no taxi stop, so the direct a→c path (cost 4) is chosen over a→b→c (cost 3).
/// assert_eq!(result.unwrap().cost, 4.0);
/// ```
#[derive(Debug)]
pub struct Graph {
    /// offsets[i]..offsets[i+1] is the CSR slice of edges for node i.
    offsets: Vec<u32>,
    /// Destination node index for each edge (parallel to `weights`).
    targets: Vec<u32>,
    /// Weight for each edge (parallel to `targets`). Must be ≥ 0.
    weights: Vec<f64>,
    /// index → string label (for path reconstruction).
    node_ids: Vec<String>,
    /// index → attributes (for filter evaluation during search).
    node_attrs: Vec<NodeAttrs>,
    /// string label → index (for O(1) lookups at query time).
    node_index: FxHashMap<String, u32>,
}

impl Graph {
    /// Build a graph from a list of node labels and directed weighted edges.
    ///
    /// Nodes created this way carry no attributes. Use [`Graph::new_with_attrs`]
    /// if you need attribute-based filtering.
    ///
    /// # Arguments
    ///
    /// * `nodes` — unique string labels for every node in the graph.
    /// * `edges` — `(source, destination, weight)` tuples. All node labels
    ///   referenced here must appear in `nodes`. Weights should be ≥ 0 (as
    ///   required by Dijkstra's algorithm).
    ///
    /// # Errors
    ///
    /// * [`GraphError::DuplicateNode`] — a label appears more than once in `nodes`.
    /// * [`GraphError::UnknownNode`] — an edge references a label not in `nodes`.
    ///
    /// # Example
    ///
    /// ```
    /// use grafo::Graph;
    ///
    /// let graph = Graph::new(
    ///     &["start", "middle", "end"],
    ///     &[("start", "middle", 2.0), ("middle", "end", 3.0)],
    /// )
    /// .unwrap();
    /// ```
    pub fn new(nodes: &[&str], edges: &[(&str, &str, f64)]) -> Result<Self, GraphError> {
        let nodes_with_attrs: Vec<(&str, &[&str])> =
            nodes.iter().map(|&id| (id, &[][..])).collect();
        Self::new_with_attrs(&nodes_with_attrs, edges)
    }

    /// Build a graph from a list of `(label, attributes)` pairs and directed
    /// weighted edges.
    ///
    /// Attributes are arbitrary string tags attached to each node (e.g.
    /// `"has_taxi_stop"`, `"has_bus_stop"`). They are used as preconditions
    /// by [`Graph::shortest_path_filtered`] to determine whether a node may
    /// be visited during a search.
    ///
    /// # Arguments
    ///
    /// * `nodes` — `(label, attributes)` pairs. Labels must be unique.
    ///   `attributes` may be empty.
    /// * `edges` — `(source, destination, weight)` tuples. Weights should be ≥ 0.
    ///
    /// # Errors
    ///
    /// * [`GraphError::DuplicateNode`] — a label appears more than once.
    /// * [`GraphError::UnknownNode`] — an edge references a label not in `nodes`.
    ///
    /// # Example
    ///
    /// ```
    /// use grafo::Graph;
    ///
    /// let graph = Graph::new_with_attrs(
    ///     &[
    ///         ("London",     &["has_taxi_stop", "has_bus_stop"][..]),
    ///         ("Oxford",     &["has_bus_stop"][..]),
    ///         ("Birmingham", &["has_taxi_stop", "has_bus_stop"][..]),
    ///     ],
    ///     &[
    ///         ("London", "Oxford",     60.0),
    ///         ("London", "Birmingham", 150.0),
    ///         ("Oxford", "Birmingham", 45.0),
    ///     ],
    /// )
    /// .unwrap();
    /// ```
    pub fn new_with_attrs(
        nodes: &[(&str, &[&str])],
        edges: &[(&str, &str, f64)],
    ) -> Result<Self, GraphError> {
        let mut node_ids: Vec<String> = Vec::with_capacity(nodes.len());
        let mut node_attrs: Vec<NodeAttrs> = Vec::with_capacity(nodes.len());
        let mut node_index: FxHashMap<String, u32> =
            FxHashMap::with_capacity_and_hasher(nodes.len(), Default::default());

        for &(label, attrs) in nodes {
            let idx = node_ids.len() as u32;
            // Single hash probe: insert returns the previous value if the key existed.
            if node_index.insert(label.to_string(), idx).is_some() {
                return Err(GraphError::DuplicateNode(label.to_string()));
            }
            node_ids.push(label.to_string());
            node_attrs.push(NodeAttrs(attrs.iter().map(|s| s.to_string()).collect()));
        }

        let v = node_ids.len();

        // Rayon dispatch overhead dominates for small edge lists; only fan
        // out when the resolution work is large enough to amortise it.
        const PAR_THRESHOLD: usize = 10_000;
        let resolve = |&(src, dst, w): &(&str, &str, f64)| -> Result<(u32, u32, f64), GraphError> {
            let s = *node_index
                .get(src)
                .ok_or_else(|| GraphError::UnknownNode(src.to_string()))?;
            let d = *node_index
                .get(dst)
                .ok_or_else(|| GraphError::UnknownNode(dst.to_string()))?;
            debug_assert!(
                w.is_finite() && w >= 0.0,
                "edge weights must be finite and non-negative"
            );
            Ok((s, d, w))
        };
        let raw: Vec<(u32, u32, f64)> = if edges.len() >= PAR_THRESHOLD {
            edges
                .par_iter()
                .map(resolve)
                .collect::<Result<_, GraphError>>()?
        } else {
            edges
                .iter()
                .map(resolve)
                .collect::<Result<_, GraphError>>()?
        };

        // CSR build without an extra cursor allocation: count, exclusive
        // prefix-sum into `offsets`, place edges while advancing the cursor
        // (which mutates `offsets[s]`), then shift right by one to restore.
        let mut offsets = vec![0u32; v + 1];
        for &(s, _, _) in &raw {
            offsets[s as usize] += 1;
        }
        let mut acc = 0u32;
        for slot in offsets.iter_mut() {
            let c = *slot;
            *slot = acc;
            acc += c;
        }

        let e = raw.len();
        let mut targets = vec![0u32; e];
        let mut weights = vec![0.0f64; e];
        for (s, d, w) in raw {
            let idx = offsets[s as usize] as usize;
            targets[idx] = d;
            weights[idx] = w;
            offsets[s as usize] += 1;
        }
        // After placement offsets[i] holds the start of node i+1; shift right
        // by one to restore the canonical CSR offsets[i] == start of node i.
        for i in (1..=v).rev() {
            offsets[i] = offsets[i - 1];
        }
        offsets[0] = 0;

        Ok(Graph {
            offsets,
            targets,
            weights,
            node_ids,
            node_attrs,
            node_index,
        })
    }

    // -----------------------------------------------------------------------
    // Private Dijkstra helpers
    // -----------------------------------------------------------------------

    /// Runs Dijkstra and returns the minimum cost to reach `dst` from `src`,
    /// or `None` if unreachable.
    ///
    /// Does **not** allocate a predecessor array — use this when only the cost
    /// is needed. Returns as soon as `dst` is popped from the heap.
    fn dijkstra_cost<F>(&self, src: u32, dst: u32, filter: &F) -> Option<f64>
    where
        F: Fn(&NodeAttrs) -> bool,
    {
        let v = self.node_ids.len();
        let mut dist = vec![f64::INFINITY; v];
        dist[src as usize] = 0.0;

        let mut heap: BinaryHeap<(Reverse<u64>, u32)> = BinaryHeap::new();
        heap.push((Reverse(0u64), src));

        while let Some((Reverse(d_bits), u)) = heap.pop() {
            let cost = f64::from_bits(d_bits);
            if cost > dist[u as usize] {
                continue; // stale entry — a shorter path was already found
            }
            if u == dst {
                return Some(cost);
            }

            let start = self.offsets[u as usize] as usize;
            let end = self.offsets[u as usize + 1] as usize;

            for i in start..end {
                let nb = self.targets[i];
                if !filter(&self.node_attrs[nb as usize]) {
                    continue;
                }
                let new_cost = cost + self.weights[i];
                if new_cost < dist[nb as usize] {
                    dist[nb as usize] = new_cost;
                    heap.push((Reverse(new_cost.to_bits()), nb));
                }
            }
        }

        None
    }

    /// Runs Dijkstra and returns `(cost, predecessor array)` when `dst` is
    /// reachable, or `None` otherwise.
    ///
    /// The predecessor array is walked by the caller to reconstruct the path.
    fn dijkstra_path<F>(&self, src: u32, dst: u32, filter: &F) -> Option<(f64, Vec<u32>)>
    where
        F: Fn(&NodeAttrs) -> bool,
    {
        let v = self.node_ids.len();
        let mut dist = vec![f64::INFINITY; v];
        let mut prev = vec![u32::MAX; v];
        dist[src as usize] = 0.0;

        let mut heap: BinaryHeap<(Reverse<u64>, u32)> = BinaryHeap::new();
        heap.push((Reverse(0u64), src));

        while let Some((Reverse(d_bits), u)) = heap.pop() {
            let cost = f64::from_bits(d_bits);
            if cost > dist[u as usize] {
                continue; // stale entry — a shorter path was already found
            }
            if u == dst {
                break;
            }

            let start = self.offsets[u as usize] as usize;
            let end = self.offsets[u as usize + 1] as usize;

            for i in start..end {
                let nb = self.targets[i];
                if !filter(&self.node_attrs[nb as usize]) {
                    continue;
                }
                let new_cost = cost + self.weights[i];
                if new_cost < dist[nb as usize] {
                    dist[nb as usize] = new_cost;
                    prev[nb as usize] = u;
                    heap.push((Reverse(new_cost.to_bits()), nb));
                }
            }
        }

        if dist[dst as usize].is_infinite() {
            None
        } else {
            Some((dist[dst as usize], prev))
        }
    }

    /// Reconstructs the node-index path from a predecessor array.
    fn reconstruct_path(&self, dst: u32, prev: &[u32]) -> Vec<u32> {
        let mut path = Vec::new();
        let mut cur = dst;
        while cur != u32::MAX {
            path.push(cur);
            cur = prev[cur as usize];
        }
        path.reverse();
        path
    }

    // -----------------------------------------------------------------------
    // Public search API
    // -----------------------------------------------------------------------

    /// Find the shortest path between two nodes using Dijkstra's algorithm.
    ///
    /// All nodes are considered traversable regardless of their attributes.
    /// For attribute-based filtering see [`Graph::shortest_path_filtered`].
    /// If you only need the cost and not the node sequence, prefer
    /// [`Graph::shortest_path_cost`].
    ///
    /// Returns `Ok(None)` when the destination is not reachable from the source.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] if either label is not part of the graph.
    ///
    /// # Examples
    ///
    /// **Path found:**
    /// ```
    /// use grafo::Graph;
    ///
    /// let g = Graph::new(
    ///     &["a", "b", "c"],
    ///     &[("a", "b", 1.0), ("b", "c", 2.0), ("a", "c", 10.0)],
    /// )
    /// .unwrap();
    ///
    /// let r = g.shortest_path("a", "c").unwrap().unwrap();
    /// assert_eq!(r.cost, 3.0);
    /// assert_eq!(r.resolve_labels(&g), vec!["a", "b", "c"]);
    /// ```
    ///
    /// **No path (unreachable destination):**
    /// ```
    /// use grafo::Graph;
    ///
    /// let g = Graph::new(&["a", "b"], &[("a", "b", 1.0)]).unwrap();
    /// assert!(g.shortest_path("b", "a").unwrap().is_none());
    /// ```
    ///
    /// **Same source and destination:**
    /// ```
    /// use grafo::Graph;
    ///
    /// let g = Graph::new(&["a", "b"], &[("a", "b", 5.0)]).unwrap();
    /// let r = g.shortest_path("a", "a").unwrap().unwrap();
    /// assert_eq!(r.cost, 0.0);
    /// assert_eq!(r.resolve_labels(&g), vec!["a"]);
    /// ```
    pub fn shortest_path(&self, from: &str, to: &str) -> Result<Option<PathResult>, GraphError> {
        self.shortest_path_filtered(from, to, |_| true)
    }

    /// Find the minimum cost to reach `to` from `from`, without building the
    /// node sequence.
    ///
    /// This is significantly faster than [`Graph::shortest_path`] on long paths
    /// because it skips path reconstruction entirely — no `String` allocations
    /// per hop. Use this when you only need to know whether a path exists and
    /// what it costs.
    ///
    /// Returns `Ok(None)` when the destination is not reachable.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] if either label is not part of the graph.
    ///
    /// # Example
    ///
    /// ```
    /// use grafo::Graph;
    ///
    /// let g = Graph::new(
    ///     &["a", "b", "c"],
    ///     &[("a", "b", 1.0), ("b", "c", 2.0), ("a", "c", 10.0)],
    /// )
    /// .unwrap();
    ///
    /// assert_eq!(g.shortest_path_cost("a", "c").unwrap(), Some(3.0));
    /// assert_eq!(g.shortest_path_cost("c", "a").unwrap(), None);
    /// ```
    pub fn shortest_path_cost(&self, from: &str, to: &str) -> Result<Option<f64>, GraphError> {
        self.shortest_path_filtered_cost(from, to, |_| true)
    }

    /// Find the shortest path between two nodes, visiting only nodes whose
    /// attributes satisfy a predicate.
    ///
    /// This is the GOAP-inspired variant of [`Graph::shortest_path`]: the
    /// `filter` closure acts as a **precondition** on each node (world state).
    /// A node is only eligible to be visited — or used as a waypoint — if
    /// `filter` returns `true` for its attribute list. This applies to the
    /// source and destination nodes as well; if either fails the predicate the
    /// method returns `Ok(None)` immediately.
    ///
    /// If you only need the cost, prefer [`Graph::shortest_path_filtered_cost`].
    ///
    /// # Arguments
    ///
    /// * `from`   — label of the source node.
    /// * `to`     — label of the destination node.
    /// * `filter` — closure that receives the attribute list of a candidate
    ///   node and returns `true` if that node may be visited.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] if either label is not part of the graph.
    ///
    /// # Examples
    ///
    /// **Travel by taxi: only nodes with `"taxi"` are valid stops.**
    /// ```
    /// use grafo::{Graph, NodeAttrs};
    ///
    /// let g = Graph::new_with_attrs(
    ///     &[
    ///         ("A", &["taxi", "bus"][..]),
    ///         ("B", &["bus"][..]),          // no taxi stop
    ///         ("C", &["taxi", "bus"][..]),
    ///     ],
    ///     &[("A", "B", 1.0), ("B", "C", 1.0), ("A", "C", 5.0)],
    /// )
    /// .unwrap();
    ///
    /// let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
    ///
    /// // B has no taxi stop, so A→B→C (cost 2) is blocked; A→C (cost 5) is used.
    /// let r = g.shortest_path_filtered("A", "C", taxi).unwrap().unwrap();
    /// assert_eq!(r.cost, 5.0);
    /// assert_eq!(r.resolve_labels(&g), vec!["A", "C"]);
    /// ```
    ///
    /// **Destination blocked by filter returns `None`.**
    /// ```
    /// use grafo::{Graph, NodeAttrs};
    ///
    /// let g = Graph::new_with_attrs(
    ///     &[
    ///         ("A", &["taxi"][..]),
    ///         ("B", &[][..]),   // no taxi stop
    ///     ],
    ///     &[("A", "B", 1.0)],
    /// )
    /// .unwrap();
    ///
    /// let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
    /// assert!(g.shortest_path_filtered("A", "B", taxi).unwrap().is_none());
    /// ```
    pub fn shortest_path_filtered<F>(
        &self,
        from: &str,
        to: &str,
        filter: F,
    ) -> Result<Option<PathResult>, GraphError>
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
            return Ok(Some(PathResult {
                cost: 0.0,
                indices: vec![src],
            }));
        }

        Ok(self
            .dijkstra_path(src, dst, &filter)
            .map(|(cost, prev)| PathResult {
                cost,
                indices: self.reconstruct_path(dst, &prev),
            }))
    }

    /// Find the minimum cost to reach `to` from `from`, visiting only nodes
    /// whose attributes satisfy a predicate, without building the node sequence.
    ///
    /// Combines the pruning power of [`Graph::shortest_path_filtered`] with the
    /// zero-reconstruction overhead of [`Graph::shortest_path_cost`]. Prefer
    /// this when filtering is needed but the actual path does not matter.
    ///
    /// # Arguments
    ///
    /// * `from`   — label of the source node.
    /// * `to`     — label of the destination node.
    /// * `filter` — closure that receives the attribute list of a candidate
    ///   node and returns `true` if that node may be visited.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::UnknownNode`] if either label is not part of the graph.
    ///
    /// # Example
    ///
    /// ```
    /// use grafo::{Graph, NodeAttrs};
    ///
    /// let g = Graph::new_with_attrs(
    ///     &[
    ///         ("A", &["taxi", "bus"][..]),
    ///         ("B", &["bus"][..]),
    ///         ("C", &["taxi", "bus"][..]),
    ///     ],
    ///     &[("A", "B", 1.0), ("B", "C", 1.0), ("A", "C", 5.0)],
    /// )
    /// .unwrap();
    ///
    /// let taxi = |attrs: &NodeAttrs| attrs.contains("taxi");
    ///
    /// // B has no taxi stop; A→C direct (cost 5) is the only viable route.
    /// assert_eq!(g.shortest_path_filtered_cost("A", "C", taxi).unwrap(), Some(5.0));
    /// ```
    pub fn shortest_path_filtered_cost<F>(
        &self,
        from: &str,
        to: &str,
        filter: F,
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

        Ok(self.dijkstra_cost(src, dst, &filter))
    }
}
