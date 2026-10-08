---
status: proposed
date: 2026-10-08
decision-makers: [leopepe]
consulted: []
informed: []
---

# Use caller-owned search workspaces

## Context and Problem Statement

Repeated narrow-frontier cost searches initialize distances for every vertex, including vertices never visited. The investigation found this initialization dominating a sparse query, but cold setup and broad searches do not establish a universal improvement. Refs #60.

## Decision Drivers

- Preserve immutable CSR graphs and concurrent readers without shared mutable caches.
- Remove repeated graph-sized distance initialization for callers that reuse query state.
- Preserve existing APIs, error ordering, filtering, numeric behavior and one-shot performance.
- Make retained memory and first-call setup an explicit caller trade-off.

## Considered Options

- Keep only the existing one-shot dense-distance search.
- Add a caller-owned generation-stamped distance array and reusable heap.
- Add a touched-node reset workspace.
- Replace dense distances with a sparse map or a graph-held cache.

## Decision Outcome

Chosen option: **caller-owned generation-stamped distances and reusable heap**. Add `SearchWorkspace` with `new` and `Default`, and opt-in `Graph::shortest_path_cost_with_workspace` and `Graph::shortest_path_filtered_cost_with_workspace`. Leave existing cost and full-path methods and planner integration unchanged.

Each nontrivial search clears the heap, advances its generation and grows buffers only when required. A stamp identifies distances belonging to that search, not to a particular graph. Reuse therefore works across graphs, node-ID reorderings, graph sizes and unwound filter panics. Generation wraparound clears all retained stamps before reusing generation one. Unseen distances compare as infinity; overflowing path costs remain unreachable, as in the existing kernel.

### Consequences

- Good, because warm narrow searches avoid initializing unused vertices.
- Good, because each simultaneous caller supplies its own mutable workspace while borrowing the graph immutably.
- Good, because generation resets do not charge a small query for the preceding broad query's visited set.
- Bad, because distance/stamp elements use 12 bytes per retained vertex slot; peak buffer lengths, spare vector/heap capacity and allocator overhead remain allocated until drop.
- Bad, because first use/growth and rare generation rollover perform graph-sized initialization; broad or one-shot workloads may not benefit.
- Bad, because the optional kernel must remain semantically consistent with the original kernel.

### Confirmation

Verify doctests, external-consumer tests, generation rollover, retained buffers, changing graphs and filters, panic recovery, numeric boundaries and independent concurrent workspaces. Preserve deterministic cold/warm/amortized, padding, broad, filtered and mixed benchmark controls and before/after release evidence. Missing profiling or CI evidence remains an explicit incomplete gate, not ratification.

## More Information

- [Design discussion](https://github.com/leopepe/automata-atelier/issues/60).
- [Workspace performance requirements](../performance-tests.md).
- [Grafo workspace performance evidence](../../grafo/docs/perf-comparison-2026-10-08.md).
- Keep this ADR proposed until PR review ratifies it.
