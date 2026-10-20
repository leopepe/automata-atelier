## Context

See proposal.md for motivation and approved scope. Source inspection confirms infinity currently doubles as unseen distance, GraphError has two variants, and planner label resolution is infallible. KWB lookup followed `wiki/index.md` → Rust indexes → code guidelines and error-handling: typed recoverable failures, distinction between absence and failure, public error compatibility, representative release measurements. Local authorities: docs/coding-guidelines.md, docs/performance-tests.md, docs/adrs.md, grafo/CLAUDE.md, grafo/docs/testing.md.

## Goals / Non-Goals

**Goals:** explicit numerical outcomes, bounded CSR representation, recoverable label resolution, atomic migration, retained workspace hot-path allocation behavior.

**Non-Goals:** graph identity tokens, private result indices, DAG validation, F6/F7/F8 cleanup, dependencies, parallelism redesign, edits to user investigations, commits or public operations.

## Decisions

### Exact public contracts

Keep `#[derive(Debug)]`, manual Display/Error and existing variants; add domain variants without payload dependencies:

```rust
pub enum GraphError {
    DuplicateNode(String),
    UnknownNode(String),
    InvalidWeight { from: String, to: String, weight: f64 },
    InvalidNodeIndex(u32),
    GraphTooLarge { nodes: usize, edges: usize },
    CostOverflow,
}
```

Display additions: `invalid weight on edge '{from}' -> '{to}': {weight}; expected finite non-negative weight`, `invalid node index: {index}`, `graph too large: {nodes} nodes, {edges} edges`, `path cost overflow`. Preserve existing messages. Tests pattern-match NaN with `is_nan`, not equality. No ErrorKind wrapper, thiserror dependency, or non_exhaustive migration: these widen the authorized API repair.

```rust
impl PathResult {
    pub fn resolve_labels(&self, graph: &Graph) -> Result<Vec<String>, GraphError>;
}
impl Graph {
    pub fn new(nodes: &[&str], edges: &[(&str, &str, f64)]) -> Result<Self, GraphError>;
    pub fn new_with_attrs(nodes: &[(&str, &[&str])], edges: &[(&str, &str, f64)]) -> Result<Self, GraphError>;
    pub fn shortest_path(&self, from: &str, to: &str) -> Result<Option<PathResult>, GraphError>;
    pub fn shortest_path_cost(&self, from: &str, to: &str) -> Result<Option<f64>, GraphError>;
    pub fn shortest_path_filtered<F>(&self, from: &str, to: &str, filter: F) -> Result<Option<PathResult>, GraphError> where F: Fn(&NodeAttrs) -> bool;
    pub fn shortest_path_filtered_cost<F>(&self, from: &str, to: &str, filter: F) -> Result<Option<f64>, GraphError> where F: Fn(&NodeAttrs) -> bool;
    pub fn shortest_path_cost_with_workspace(&self, from: &str, to: &str, workspace: &mut SearchWorkspace) -> Result<Option<f64>, GraphError>;
    pub fn shortest_path_filtered_cost_with_workspace<F>(&self, from: &str, to: &str, filter: F, workspace: &mut SearchWorkspace) -> Result<Option<f64>, GraphError> where F: Fn(&NodeAttrs) -> bool;
}
```

Signatures above are declarations documenting the contract, not replacement source. Query/constructor signatures remain unchanged; label resolution changes. Resolve each index with checked access in sequence; return first invalid index, with empty indices yielding Ok(empty). Bounds checking does not detect a same-size foreign graph: callers must supply the originating graph; keep public indices.

### Construction boundaries

Run a shared private count validator at the start of BOTH constructors, before the `new` adapter collect or internal allocations. Accept node and edge counts <= u32::MAX, provided nodes.checked_add(1) succeeds on the target usize; reject otherwise with GraphTooLarge. Node indices then stop at u32::MAX-1, preserving the predecessor sentinel. This representation check does not promise sufficient allocator memory. Test validator counts directly (including usize::MAX), without allocating huge slices.

After count validation retain duplicate-node-first handling. Per edge resolve source then destination, then reject `!weight.is_finite() || weight < 0.0`; accept zero including negative zero. Apply identical resolver to sequential and >=10,000-edge Rayon paths. Do not promise which malformed edge wins among multiple parallel failures. This preserves endpoint precedence within an edge, not deterministic parallel error ordering.

### Overflow without a second traversal

One-shot searches initialize distances to -1.0 (unseen); workspace uses its existing generation stamps. Relax if unseen OR candidate < previous, including infinity on first discovery. Queue infinity using the existing nonnegative float-bit ordering. First popped goal: finite returns success; infinity returns CostOverflow. Exhaustion returns None. Finite entries sort before infinity; finite alternatives therefore win before an infinite goal can be popped. Infinity propagation reaches overflow-only goals and terminates on cycles because discovered infinity is not re-enqueued on equal infinity. Skip stale entries as today. Normalize candidate zero to positive zero before bit-key insertion to retain numerical ordering for accepted -0.0 weights. Private search helpers become fallible and propagate through the existing public Results.

This avoids a separate allocation in the workspace ordinary hot path and avoids fallback predicate replay; it can traverse more of an overflowing reachable component. Do not short-circuit on the first overflowing addition. Preserve endpoint lookup source-first, then short-circuit source/goal predicates before same-node zero result, and neighbor predicate calls before relaxations. Stateful Fn via interior mutability remains accepted: do not add a fallback traversal or claim mathematical route equivalence for predicates whose answers change. Deterministic predicates define filtered reachability; stateful regression tests force call schedules and detect replay rather than assuming purity. Existing next-search heap clearing and generation invalidation must work after errors, early exits, rejection, graph-size changes, and rollover.

### Companion ownership and migration

Survey command: `rg -n --hidden -g '!.git/**' -g '!target/**' 'resolve_labels|GraphError' .` across repository tracked/untracked text. Hits requiring ownership:

- F3 task: grafo/src/graph.rs and lib.rs doctests/unit assertions; grafo/README.md examples; grafo/examples/{basic,build_pipeline,city_routes,concurrent_queries,goap_robot_delivery,goap_quest_planner}.rs; goap-planner/src/planner.rs:168 uses `?` with existing PlannerError::Graph/From mapping (lines 18,34). Examples may unwrap results produced by the same graph; planner must propagate.
- F1/F2/F4/F5 tasks: GraphError definitions, Display, public errors docs, grafo/tests/search_workspace.rs, grafo/CLAUDE.md; grafo/docs/testing.md update stale PathResult field examples only as needed for consumer-contract tests.
- Documentation task: grafo/CLAUDE.md weights/endpoints/errors and search API list; do not add topology behavior. New MADR ADR uses next free number, refs #60 and PR #61, explains numerical exception to ADR0006 compatibility without rewriting its accepted decision; index new record in docs/adrs/README.md. Existing GraphError unit matches remain valid but need new-variant tests. Other future hits require classification before apply completion.
- Parent-generated baseline log is evidence input, not a child edit. User-owned grafo/docs/csr-sparse-search-evidence-2026-10-08/ and csr-sparse-search-investigation-2026-10-08.md are explicit non-targets, even if future grep finds contract text there.

Final blast-radius sweep expanded the same command pattern to `resolve_labels|GraphError|InvalidWeight|InvalidNodeIndex|GraphTooLarge|CostOverflow|attr_count_scaling|attributed_dag`. Additional hits: grafo/benches/performance.rs is owned by F9; grafo/docs/{bench-baseline.txt,bench-final.txt,bench-after-hotpath.txt,benchmarks-2026-04-20.md,perf-comparison-2026-05-01.md} are historical evidence, intentionally not rewritten. Newly generated planning-file hits refer to this change itself. No configuration keys are introduced.

### Benchmark controls

F9 uses distinct generated tags (including taxi) to achieve exactly 1/5/10/20/50 attrs on each node; keep a deterministic traversable route and fixed topology/weights/endpoints across counts. Check cardinality, source/goal predicate acceptance and successful nontrivial expected route/cost outside timing. Keep black_box and report this as changed workload; historical attribute-scaling timings are not an apples-to-apples speed baseline. Compare unchanged benchmark IDs separately.

## Risks / Trade-offs

- Extra work on overflowing components → termination, finite-alternative, disconnected-goal and cyclic fixtures; full before/after benchmarks.
- Breaking label resolution and enum exhaustiveness → atomic consumer/doc updates and workspace compile/test gates.
- Stateful filters need not define a fixed subgraph → retain evaluation placement, explicitly test no fallback replay.
- Flamegraph tooling unavailable per handoff → attempt paired commands without install/sudo; preserve exact failure and mark CPU attribution unavailable. Do not invent SVGs or claim full profiling compliance.

## Migration Plan

Parent captures untouched baseline first. For each finding retain RED output, implement narrowly, run focused GREEN and workspace tests; coverage-only F5 records previously missing tests rather than fabricating behavioral failure. Roll back an unsuccessful implementation coherently with consumer changes, preserving evidence and user files. After review, apply performs final release/debug tests, documentation/lints and comparative measurements. No implementation is authorized by this planning phase itself.
