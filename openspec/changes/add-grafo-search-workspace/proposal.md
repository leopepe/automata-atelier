## Why

The 2026-10-08 sparse CSR investigation ([investigation published in implementation draft #61](https://github.com/leopepe/automata-atelier/blob/09bc36ba5bced6089cc9402dddb6b2f4bc6f08fb/grafo/docs/csr-sparse-search-investigation-2026-10-08.md)) attributed a narrow query's cost largely to graph-sized distance initialization; warm epoch prototypes helped this fixture, but first-call and mixed/broad measurements did not justify replacing the current default. Offer explicit reuse for callers that perform repeated cost-only queries without changing existing search behavior.

## Publication Scope

This change publishes the Conductor build plan only; it contains no Rust implementation, benchmarks, raw evidence or ADR ratification. Separate implementation draft [#61](https://github.com/leopepe/automata-atelier/pull/61) already contains a proposed ADR 0006; it is not validated or accepted by this planning PR. Its implementation choices and failing quality gates must be reconciled with this plan before marking tasks complete.

## What Changes

- Add an optional caller-owned, reusable epoch workspace and explicit unfiltered/filtered cost-only search entry points on `Graph`.
- Define reuse across graph sizes and same-sized graph instances, early returns, panicking filters, numeric edge behavior, and independent concurrent readers.
- Add public doctests, external integration and unit tests, benchmark cases and release-mode before/after evidence with the approved cycle-local native sample profile exception.
- Draft proposed ADR 0006 (not ratified) for ownership and allocation strategy; track discussion in [design issue #60](https://github.com/leopepe/automata-atelier/issues/60).
- Leave default cost/full-path queries and planner integration untouched; no automatic strategy selection or queue replacement.

## Capabilities

### New Capabilities

- `grafo-search-workspace`: Optional reusable cost-only search with explicit caller ownership and query-isolated state.

### Modified Capabilities

None. Existing API behavior is a compatibility constraint, not a proposed requirement change.

## Impact

`grafo/src/graph.rs`, `grafo/src/lib.rs`, grafo unit/integration tests and doctests, `grafo/benches/performance.rs`, grafo performance/API docs, and proposed `docs/adrs/0006-use-caller-owned-cost-search-workspace.md` plus index. `goap-planner` benchmarks/tests act as regression controls; no planner production edits or dependency additions are proposed. The profiling workaround is evidence-specific and does not modify CI or profiling policy.
