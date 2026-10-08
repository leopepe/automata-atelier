## Purpose

Provide an opt-in caller-owned search state for repeated cost-only graph queries, retaining current graph search results while avoiding repeated full-distance initialization for suitable workloads.

## ADDED Requirements

### Requirement: Opt-in cost-only workspace queries
`grafo` SHALL expose a caller-owned reusable workspace and unfiltered and filtered cost-only graph query methods that take mutable access to it. A query SHALL report the same cost, absence, or `GraphError::UnknownNode` as its corresponding existing cost-only method for the same graph, endpoints and predicate; existing default and full-path methods SHALL remain available without a workspace.

#### Scenario: Unfiltered reachable and unreachable queries
- **WHEN** a caller runs a workspace query on a reachable pair and then on an unreachable pair
- **THEN** the costs and `None` respectively match the existing unfiltered cost-only query

#### Scenario: Unknown endpoints
- **WHEN** either endpoint label is not in the graph
- **THEN** the workspace query returns the corresponding `UnknownNode` error, including when a previous query used the same workspace

#### Scenario: Existing API remains independent
- **WHEN** a caller uses existing unfiltered, filtered and full-path query methods without a workspace
- **THEN** their result, error and predicate behavior remain unchanged

### Requirement: Query isolation across reuse and graph switches
Each workspace query SHALL ignore per-query state from preceding queries, including preceding queries on graphs of different sizes or on graph instances of equal size with different node IDs, edges or attributes. Reuse SHALL require neither graph mutation nor caller-managed graph identity/reset. A filter panic SHALL propagate normally, and a later query on that workspace SHALL have fresh search state when the caller catches the panic and reuses it.

#### Scenario: Repeated results and early exits
- **WHEN** a caller alternates reachable, unreachable, self and endpoint-rejected queries with the same workspace
- **THEN** each result matches an independent query, including after a previous query stopped with frontier entries pending

#### Scenario: Different graphs and reused numeric IDs
- **WHEN** a workspace is used on larger, smaller and same-sized graph instances with reassigned labels, edges or attributes
- **THEN** each result depends on the selected graph and current predicate, not a previous graph or query

#### Scenario: Filter panic then reuse
- **WHEN** a filter panics during traversal and the caller catches the unwind before running a new query
- **THEN** the panic is not converted into a result and the next query matches an independent query

#### Scenario: Generation rollover
- **WHEN** query generations roll over during repeated reuse
- **THEN** a subsequent query cannot read a distance from an earlier generation

### Requirement: Filter and numeric compatibility
Filtered workspace queries SHALL apply the predicate to source and destination before the same-node result and to candidate neighbors during traversal, matching existing filtered cost-only behavior. Costs SHALL follow the existing finite, non-negative input expectation and f64 relaxation behavior without introducing a new edge-weight validation or overflow policy.

#### Scenario: Changing filters
- **WHEN** a workspace is reused with predicates that alternately permit and reject the source, destination or an intermediate node
- **THEN** results match separate calls to the current filtered cost-only method for each predicate

#### Scenario: Floating-point boundaries
- **WHEN** valid zero, negative-zero, fractional and finite-extreme weights are queried or a finite sum overflows to infinity
- **THEN** workspace results agree with the current cost-only method, including its unreachable result for paths whose only candidate cost overflows

### Requirement: Independent concurrent use
A graph SHALL remain shareable for concurrent immutable searches, with one mutable workspace owned by each simultaneous workspace query. The workspace API SHALL not introduce graph-wide mutable state or implicit synchronization.

#### Scenario: Independent readers
- **WHEN** readers run searches on the same immutable graph with separate workspaces concurrently
- **THEN** each result matches an equivalent independent cost-only query
