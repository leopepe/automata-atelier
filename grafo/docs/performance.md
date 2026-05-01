# Performance

Canonical "current state" summary for `grafo`. Refreshed after each benchmark
session — link this from the README so the URL never rots. Per-change
deltas live in [`perf-comparison-YYYY-MM-DD.md`](.) snapshots, never here.

**Last measured:** 2026-05-01 (Criterion 0.5, 100 samples, 3 s warm-up,
release profile)
**Library version:** grafo 0.1.0
**Platform:** macOS Darwin 25.3.0, Rust 1.94.1
**Raw log:** [`bench-final.txt`](./bench-final.txt) ·
**This release's deltas:** [`perf-comparison-2026-05-01.md`](./perf-comparison-2026-05-01.md)

---

## At a glance

| Workload | Result |
|---|---|
| Selective filter on a 5k-node sparse DAG | **~10 ns** — search exits at the first failing node |
| `shortest_path_cost` on a 10k-node fan=4 sparse DAG | **4.8 µs** |
| `shortest_path_cost` on a 100k-node chain (worst-case path length) | **648 µs** |
| `shortest_path_cost` vs full-path reconstruction at 100k hops | **1.25× faster** (519 µs vs 648 µs) |
| Construction of a 1k-node sparse DAG | **89 µs** (~78% faster than the pre-optimization baseline) |
| Construction of a 500k-node sparse DAG with Rayon | **43 ms** |
| 512 parallel queries via Rayon over a shared `Arc<Graph>` | **914 µs** — scales with available cores |
| Attribute count 1 → 20 per node (filter throughput) | **flat at ~10 ns** — O(1) `FxHashSet` lookup |

---

## Construction

Includes label interning, edge resolution (sequential below 10k edges,
Rayon-parallel above), parallel sort, and CSR offset build.

| Graph shape         | Nodes   | Time         |
|---------------------|---------|--------------|
| chain               |   1 000 | 46.6 µs      |
| sparse DAG (fan=4)  |   1 000 | 89.4 µs      |
| chain               |  10 000 | 487 µs       |
| sparse DAG (fan=4)  |  10 000 | 950 µs       |
| chain               | 100 000 | 5.35 ms      |
| sparse DAG (fan=4)  | 100 000 | 8.39 ms      |

`construction/parallel_sort` (Rayon-backed): 953 µs at 10k → 8.14 ms at 100k →
**43.3 ms at 500k** nodes.

## Search — no filter

`shortest_path_cost` on cold caches.

| Shape              | Nodes   | Time     |
|--------------------|---------|----------|
| sparse DAG (fan=4) |   1 000 | 551 ns   |
| sparse DAG (fan=4) |  10 000 | 4.78 µs  |
| sparse DAG (fan=4) |  50 000 | 7.20 µs  |
| sparse DAG (fan=16)|  10 000 | 5.86 µs  |
| sparse DAG (fan=16)|  50 000 | 17.4 µs  |
| layered (50×20)    |   1 000 | 37.5 µs  |
| layered (100×30)   |   3 000 | 200 µs   |
| chain              |  10 000 | 67.7 µs  |
| chain              | 100 000 | 648 µs   |

## Search — filtered (`shortest_path_filtered_cost`)

5 000-node sparse DAG. Filter closure runs as a per-node precondition.

| Variant                                | Time      |
|----------------------------------------|-----------|
| `shortest_path_cost` (no filter)       | 5.28 µs   |
| pass-all closure (`\|_\| true`)         | 5.21 µs   |
| simple one-attr filter                 | 10.4 ns   |
| compound AND filter                    | 10.4 ns   |
| strict rare-attr filter                | 10.3 ns   |

The pass-all closure is indistinguishable from the unfiltered path — the
compiler monomorphizes and inlines it away. Selective filters return in
~10 ns because Dijkstra is short-circuited at the first node failing the
predicate.

## Search — path reconstruction (cost-only speedup)

`shortest_path` (full `Vec<String>` path) vs `shortest_path_cost`
(cost only) over a chain.

| Path length | Full path | Cost-only | Speedup |
|------------:|-----------|-----------|--------:|
|          10 |   153 ns  |   70 ns   | **2.2×** |
|         100 |   800 ns  |  535 ns   | **1.5×** |
|       1 000 |  6.36 µs  | 4.98 µs   | **1.3×** |
|      10 000 |  67.7 µs  | 51.8 µs   | **1.3×** |
|     100 000 |   648 µs  |   520 µs  | **1.25×** |

## Concurrent queries

All queries share a single `Arc<Graph>` over a 10k-node sparse DAG.

| Mode                      | Queries | Time     |
|---------------------------|--------:|----------|
| Sequential                |      64 | 202 µs   |
| Rayon `par_iter`          |       8 | 45.1 µs  |
| Rayon `par_iter`          |      32 | 94.3 µs  |
| Rayon `par_iter`          |      64 | 149 µs   |
| Rayon `par_iter`          |     128 | 262 µs   |
| Rayon `par_iter`          |     512 | 914 µs   |

Rayon delivers ~1.36× wall-clock speedup at 64 queries; throughput scales
near-linearly with available cores up to 512 queries.

---

## Trade-offs we accept

The current `dijkstra_*` implementations skip the `settled: Vec<bool>`
array (one O(V) zeroed allocation per query is gone). Stale heap entries
are filtered by comparing the popped cost against `dist[u]` instead.

This is a clear win on dense / wide graphs where stale entries are real
and the saved allocation matters (sparse DAG fan=4 at 50k: 7.2 µs vs the
old 8.7 µs — −13%). It costs us on **chain shapes** (fan-out 1, no stale
heap entries to skip) where the per-pop cost comparison is wasted work:
search on a 100k chain went from ~497 µs to 648 µs (+30%) at the same
time the filtered search wins arrived. We treat chains as a degenerate
shape rarely used in practice; the trade-off is documented and revisitable.

See [`perf-comparison-2026-05-01.md`](./perf-comparison-2026-05-01.md) for
the full diff and the rationale per change.

## History

| Date       | Document                                                                   | What changed                                                            |
|------------|----------------------------------------------------------------------------|-------------------------------------------------------------------------|
| 2026-05-01 | [`perf-comparison-2026-05-01.md`](./perf-comparison-2026-05-01.md)         | FxHashSet for `NodeAttrs`, dropped `settled` array, Rayon threshold, in-place CSR cursor |
| 2026-04-20 | [`benchmarks-2026-04-20.md`](./benchmarks-2026-04-20.md)                   | First full benchmark sweep on grafo 0.1.0                               |
