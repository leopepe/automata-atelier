# grafo-error-contracts Specification

## Purpose

Define grafo construction, numerical-search and label-resolution outcomes so consumers can distinguish invalid input, unreachable destinations and unrepresentable costs.

## Requirements

### Requirement: Constructor input errors are recoverable

Both constructors SHALL reject negative, NaN and infinite weights with `GraphError::InvalidWeight { from: String, to: String, weight: f64 }` in debug and release builds, including sequential and >=10,000-edge construction. Finite nonnegative weights including -0.0 MUST remain accepted. Duplicate nodes SHALL be checked before edge resolution; source and destination lookup SHALL precede weight validation within an edge. Parallel failures across different edges need not select a deterministic failing edge.

#### Scenario: Invalid edge weights
- **WHEN** either constructor receives a known-endpoint edge with a negative, NaN or infinite weight in either edge-count branch
- **THEN** it returns InvalidWeight carrying that edge and value rather than panicking or building the graph

#### Scenario: Endpoint precedence
- **WHEN** an edge has both an unknown endpoint and an invalid weight
- **THEN** its resolver reports UnknownNode before checking weight; duplicate node inputs retain DuplicateNode precedence

### Requirement: Representation limits precede internal allocation

Both constructors MUST return `GraphError::GraphTooLarge { nodes: usize, edges: usize }` before internal allocation if either count exceeds u32::MAX or node_count + 1 is unrepresentable as usize. The limit SHALL reserve u32::MAX as predecessor sentinel. Representation acceptance MUST NOT imply allocator success.

#### Scenario: Count boundary validation
- **WHEN** the shared count validator is exercised directly with limit, over-limit where representable, and usize::MAX counts
- **THEN** it accepts representable allowed counts and rejects unsupported counts without allocating graph-sized buffers

### Requirement: Checked label resolution

`PathResult::resolve_labels(&self, graph: &Graph)` SHALL return `Result<Vec<String>, GraphError>`. It SHALL preserve index order, accept an empty sequence, and return `GraphError::InvalidNodeIndex(u32)` for the first out-of-bounds index. Documentation MUST require the originating graph; bounds checks MUST NOT be described as graph-identity validation.

#### Scenario: Modified or foreign indices
- **WHEN** caller-modified indices or a result resolved against an empty graph contain an invalid index
- **THEN** resolution returns InvalidNodeIndex without an indexing panic

#### Scenario: Valid resolution
- **WHEN** valid indices from the originating graph are resolved
- **THEN** ordered owned labels are returned in Ok; empty indices yield Ok(empty)

### Requirement: Search absence differs from numerical failure

The four one-shot and two workspace queries SHALL retain their existing public Result/Option signatures and agree for deterministic filters: a finite route returns the minimum finite cost (and ordered indices for path queries); an accepted reachable goal without a finite-cost route returns `GraphError::CostOverflow`; an unreachable or filter-rejected goal returns Ok(None). An overflow on an unrelated branch MUST NOT turn an unreachable goal into an error. Finite alternatives MUST take precedence over overflowing alternatives, independent of discovery order. Finite MAX and zero weights MUST remain valid.

#### Scenario: Overflow-only goal
- **WHEN** the accepted route to a goal requires adding two f64::MAX edges and lacks a finite alternative
- **THEN** each applicable query returns CostOverflow rather than None

#### Scenario: Finite alternative wins
- **WHEN** both an overflowing route and a finite route reach the accepted goal, including a route discovered after the overflow candidate
- **THEN** queries return the finite minimum

#### Scenario: Overflow dead end is not goal reachability
- **WHEN** an accepted overflow branch is reachable but the goal is disconnected or a required intermediate node is rejected
- **THEN** queries return Ok(None)

#### Scenario: Endpoint and filter semantics
- **WHEN** labels are unknown, endpoints are rejected, or source equals goal
- **THEN** unknown labels are checked source-first; source predicate then goal predicate short-circuit before same-node zero success; rejected endpoints yield None

#### Scenario: No fallback filter replay
- **WHEN** an interior-mutable Fn records evaluations during overflow exploration
- **THEN** evaluation follows endpoint checks and neighbor traversal without a second fallback reachability pass

### Requirement: Workspace errors do not poison later queries

Caller-owned workspaces SHALL remain reusable after overflow and unknown-node errors, early success and endpoint rejection, including graph-size changes and generation rollover, without adding a separate per-query allocation to ordinary warmed search.

#### Scenario: Error followed by reuse
- **WHEN** the same workspace handles overflow, then a valid finite query on smaller and larger graphs
- **THEN** subsequent results match one-shot results without stale heap or distance contamination

### Requirement: Consumer and diagnostic coverage

External-consumer tests MUST cover four one-shot entry points, NodeAttrs iter/len/is_empty and membership, constructor sequential/parallel success/errors, and GraphError variant payloads and Display messages. Changed resolve_labels callsites and docs MUST compile with fallible resolution; planner SHALL propagate through its existing Graph error mapping.

#### Scenario: Consumer migration
- **WHEN** workspace tests, examples and doctests compile and execute after the signature change
- **THEN** label callers explicitly handle Result and error tests verify the intended variants and messages

### Requirement: Attribute scaling fixtures represent their advertised workload

Attribute scaling fixtures SHALL contain exactly 1, 5, 10, 20 and 50 distinct attributes per node, accepted endpoints and a controlled nontrivial traversable route, with deterministic topology/weights/endpoints held constant across counts. Checks MUST run outside the timed closure. Changed workloads MUST be reported separately from comparable unchanged benchmarks.

#### Scenario: Fixture preflight
- **WHEN** each attribute-count fixture is built before measurement
- **THEN** cardinality, endpoint acceptance, route and cost assertions pass before timing begins
