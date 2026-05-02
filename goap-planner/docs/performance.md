# Performance

Canonical "current state" summary for `goap-planner`. Refreshed after each
benchmark session — link this from the README so the URL never rots.
Per-change deltas live in `perf-comparison-YYYY-MM-DD.md` snapshots,
never here.

**Last measured:** 2026-05-02 (Criterion 0.5, 100 samples, 3 s warm-up,
release profile)
**Library version:** goap-planner 0.1.0
**Platform:** macOS Darwin 25.3.0, Rust stable
**Raw log:** [`bench-baseline.txt`](./bench-baseline.txt)

`goap-planner` runs forward BFS over reachable states, builds a state-
transition graph, then delegates the shortest-path search to
[`grafo`](../../grafo/docs/performance.md). Most of the per-state cost
is on goap-planner's side (state cloning, signature hashing, applicability
scans); the final Dijkstra is grafo's hot path and inherits its numbers.

---

## At a glance

| Workload | Result |
|---|---|
| 5-step linear plan (chain) | **3.4 µs** |
| 50-step linear plan (chain) | **33 µs** |
| 128-action library, single correct branch | **362 µs** |
| 512-action library, single correct branch | **5.2 ms** |
| 16 redundant paths, picks the cheapest | **24 µs** |
| Goal already satisfied (fast-path early return) | **6.9 ns** |
| `Goal::satisfied_by` with one required fact | **6.2 ns** |
| `State::contains` (hit / miss) | **5.4 ns / 2.7 ns** |
| `Action::applicable` (preconditions met / unmet) | **17 ns / 14 ns** |
| 64 concurrent plans via Rayon over `Arc<Planner>` | **145 µs** (5.4× faster than sequential) |

---

## Planning — chain plans (linear plan length)

Single-path scenario: each state has exactly one applicable action; state
space size = plan length.

| Plan length | Time     |
|------------:|----------|
|           5 | 3.43 µs  |
|          10 | 6.19 µs  |
|          20 | 11.9 µs  |
|          50 | 33.3 µs  |

Roughly linear in plan length: per-step cost is dominated by `Action::apply`
(state clone + effect application) and the final Dijkstra over a chain of
that length.

## Planning — wide branching (action library size)

`n` first-step actions all applicable from initial state, only one leads
to the goal. Stresses per-state action-library scan.

| Branches | Total actions | Time     |
|---------:|--------------:|----------|
|        8 |            16 | 10.8 µs  |
|       32 |            64 | 49.7 µs  |
|      128 |           256 | 362 µs   |
|      512 |          1024 | 5.22 ms  |

Cost grows ~quadratically with branch count: state space scales linearly,
and the per-state action scan also scales linearly. Expect this shape on
configs with many "tried fast path" / "tried slow path" alternatives.

## Planning — redundant paths (cheapest-path selection)

`n` parallel two-step routes from start to goal with random costs. The
planner discovers all of them and picks the cheapest via the underlying
Dijkstra.

| Paths | Time     |
|------:|----------|
|     2 | 3.29 µs  |
|     4 | 6.03 µs  |
|     8 | 11.7 µs  |
|    16 | 24.0 µs  |
|    32 | 51.9 µs  |

Linear scaling — `edge_map` insertions and the final Dijkstra both grow
proportionally with path count.

## Planning — boundary cases

| Scenario             | Time     |
|----------------------|----------|
| `already_satisfied`  | 6.87 ns  |
| `unreachable`        | 3.53 µs  |

`already_satisfied` is the fast-path early return at the top of
`Planner::plan` — no BFS, no graph build, no Dijkstra. It is essentially
the cost of one `Goal::satisfied_by` call.

`unreachable` exhausts the bounded discovered state space (`max_states
= 64` on this scenario) and returns `Ok(None)`. Time scales with
`max_states × |actions|`.

## Micro-ops — `State`

| Operation               | Time     |
|-------------------------|----------|
| `contains` (hit)        | 5.35 ns  |
| `contains` (miss)       | 2.71 ns  |
| `insert`                | 905 ns   |
| `from_facts(1)`         | 33.4 ns  |
| `from_facts(10)`        | 228 ns   |
| `from_facts(100)`       | 2.21 µs  |

`contains` is `FxHashSet::contains` — flat O(1). `insert` is dominated by
`String` allocation + hash; the `from_facts` numbers grow linearly with
input size for the same reason.

## Micro-ops — `Action`

| Operation                          | Time     |
|------------------------------------|----------|
| `applicable` (preconditions met)   | 17.3 ns  |
| `applicable` (preconditions unmet) | 13.8 ns  |
| `apply`                            | 126 ns   |

`applicable` is `O(|preconditions|)` membership lookups; `apply` clones
the state then mutates it in place. Most of `apply`'s cost is the
`State::clone`.

## Micro-ops — `Goal`

| Goal shape                   | Time     |
|------------------------------|----------|
| 1 required fact              | 6.21 ns  |
| 10 required + 10 forbidden   | 77.1 ns  |
| 1 required, unmet            | 3.48 ns  |

`satisfied_by` short-circuits as soon as a required fact is missing —
that's why the unmet case is the fastest.

## Concurrent plans

Run independent plan calls sequentially vs. parallel via Rayon over a
shared `Arc<Planner>`. Workload: 20-step chain plan per call.

| Mode                      | Calls | Time     |
|---------------------------|------:|----------|
| Sequential                |    64 | 781 µs   |
| Rayon `par_iter`          |     8 | 53.5 µs  |
| Rayon `par_iter`          |    32 | 95.8 µs  |
| Rayon `par_iter`          |    64 | 145 µs   |
| Rayon `par_iter`          |   128 | 235 µs   |

Rayon delivers **~5.4× wall-clock speedup at 64 parallel calls** — each
plan is independent of the others (no shared mutable state inside
`Planner::plan`), so throughput scales near-linearly with available cores.

---

## Trade-offs we accept

`Planner::plan` does forward BFS bounded by `max_states` (default 10 000).
The state-space encoding (`State::signature` builds a sorted, separator-
joined `String`) trades hashing speed for human-readable graph node labels
during debugging. For configs with hundreds of facts per state, this can
become the bottleneck before the Dijkstra does — investigate
`State::signature` first if a profile shows planning time dominated by
allocation rather than action scanning.

`Action::apply` clones the entire state on every expansion. For state
spaces dominated by tens of facts and short plans this is fine; for very
wide states (hundreds of facts) it allocates linearly per step. A
copy-on-write or arena-backed `State` is on the table if a real workload
ever hits this.

Both items will be revisited the next time a real consumer pushes the
planner past current numbers — neither is a problem until it is.

## History

| Date       | Document                                          | What changed                                          |
|------------|---------------------------------------------------|-------------------------------------------------------|
| 2026-05-02 | [`bench-baseline.txt`](./bench-baseline.txt)      | Initial benchmark sweep: planning, micro-ops, concurrent plans |
