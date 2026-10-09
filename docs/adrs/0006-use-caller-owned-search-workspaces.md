---
status: accepted
date: 2026-10-08
decision-makers: [leopepe]
---

# Use caller-owned search workspaces

## Context and Problem Statement

The improvement started with a comparison of grafo's cost-only searches against petgraph's CSR Dijkstra. That investigation identified full-graph distance initialization as avoidable work when repeated queries visit only a small frontier; it did not establish that another algorithm is universally faster. Refs [#60](https://github.com/leopepe/automata-atelier/issues/60).

## Decision Drivers

- Avoid repeated distance initialization and heap allocation for reusable queries.
- Preserve immutable graphs, independent concurrent readers and existing search behavior.
- Keep memory retention and setup costs an explicit caller choice.

## Considered Options

- Keep only the existing one-shot searches.
- Add caller-owned generation-stamped distances and a reusable heap.
- Replace the default search with sparse state or introduce a graph-held cache.

## Decision Outcome

Add an optional `SearchWorkspace` and filtered/unfiltered cost-only query methods taking `&mut SearchWorkspace`. Reuse distances through generation stamps, clear the heap for each nontrivial search, and invalidate all retained stamps on generation rollover. Each concurrent caller owns a separate workspace; `Graph`, existing one-shot/full-path searches and planner integration remain unchanged.

## Consequences

- Good, because warm narrow searches avoid initializing distances for unvisited nodes.
- Good, because callers can reuse state across graph sizes, graph instances and caught filter panics without a shared cache.
- Bad, because buffers retain peak capacity until drop, and first use, growth and rollover require initialization.
- Bad, because cold, broad or mixed workloads may not benefit; there is no universal speedup guarantee.

## Confirmation

Exercise API parity, graph switches, filters, panic recovery, retained-tail generation rollover and independent workspaces in tests. Keep cold/warm and broad-query benchmark controls; CI checks remain unchanged.

## More Information

- [Implementation and review](https://github.com/leopepe/automata-atelier/pull/61).
- [Performance requirements](../performance-tests.md).
