---
status: proposed
date: 2026-10-20
decision-makers: [leopepe]
---

# Return recoverable graph errors

## Context and Problem Statement

The grafo audit found debug-only weight assertions, unchecked path-label indices and CSR limits, and reachable overflowing paths reported as unreachable. The workspace implementation originated in [#60](https://github.com/leopepe/automata-atelier/issues/60) and [PR #61](https://github.com/leopepe/automata-atelier/pull/61); its compatibility constraint in [ADR 0006](./0006-use-caller-owned-search-workspaces.md) preserved the prior numerical behavior. This proposed record captures the explicitly approved error-contract correction without rewriting that accepted record.

## Decision Drivers

- Distinguish expected absence from invalid input and unrepresentable path cost.
- Keep all four one-shot and both workspace queries consistent.
- Avoid extra warmed-workspace allocation, fallback traversals and predicate replay.
- Retain public indices without persistent graph identity tokens.

## Considered Options

- Preserve assertions, unchecked label resolution and overflow-as-absence.
- Return domain errors and propagate infinite discovered distances in Dijkstra.
- Add a separate reachability traversal when a finite search overflows.

## Decision Outcome

Choose domain errors and a single Dijkstra traversal. Add `InvalidWeight { from: String, to: String, weight: f64 }`, `InvalidNodeIndex(u32)`, `GraphTooLarge { nodes: usize, edges: usize }` and `CostOverflow` to the existing `GraphError` enum. Change `PathResult::resolve_labels(&self, graph: &Graph)` to `Result<Vec<String>, GraphError>`, returning the first invalid index; callers remain responsible for supplying the originating graph because bounds checks do not establish graph identity.

Both constructors validate node/edge counts before internal allocation, including the plain-label adapter. Counts must fit u32 and node count + 1 must fit usize; u32::MAX remains the missing-predecessor sentinel. These bounds do not guarantee allocator memory. Preserve duplicate-label precedence, then source and destination lookup before per-edge weight validation; reject negative, NaN and infinite weights in every build profile, accepting -0.0. Parallel failures on different edges need not select the same error.

Keep all six query signatures as Result/Option. A finite route returns success; no accepted route returns None; an accepted reachable goal with no finite route returns CostOverflow. Queue infinity on first discovery and sort finite costs before infinity, so finite alternatives win and unrelated overflow does not become an error. One-shot searches use -1 for unseen distance; workspace searches retain existing stamps and heap reuse. Starting at +0 and adding validated nonnegative weights cannot produce a negative-zero key.

This deliberately corrects ADR 0006's numerical compatibility constraint for both one-shot and workspace methods, but retains its caller-owned storage and concurrency decisions. Deterministic predicates define filtered reachability; no fallback may replay an interior-mutable Fn. Planner label resolution propagates errors through its existing GraphError conversion.

## Consequences

- Good, because recoverable input and numerical failures no longer panic or masquerade as unreachable.
- Good, because workspace searches retain their allocation strategy and recover after errors, early exits and caught predicate panics.
- Bad, because label resolution and exhaustive GraphError matches require source migration.
- Bad, because overflowing components can require more exploration than the previous finite-only traversal.
- Bad, because bounds checking cannot detect a same-sized foreign graph with different label ordering.

## Confirmation

Run regression-first constructor, index and six-query numeric tests in debug/release, external-consumer and workspace-reuse tests, examples and doctests. Compare both benchmark harnesses before/after; report repaired attribute-scaling inputs as a workload reset rather than a speed change. Profile when tooling is available; absent cargo-flamegraph is a recorded validation limitation, not fabricated evidence.

## More Information

- [Approved change design](../../openspec/changes/archive/2026-10-20-fix-grafo-audit-contracts/design.md).
- [Workspace performance requirements](../performance-tests.md).
- This record remains proposed pending independent review and ratification; no public issue or PR operation is part of this implementation.
