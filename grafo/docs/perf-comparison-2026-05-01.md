# Performance Comparison — 2026-05-01

Comparison of three benchmark runs on `master`, all measured with Criterion 0.5 (100 samples, 3 s warm-up). Raw logs:

- `bench-baseline.txt` — unmodified `master`
- `bench-after-hotpath.txt` — after #1 (drop `settled` array) and #3 (FxHashSet for `NodeAttrs`)
- `bench-final.txt` — additionally with #5 (single-probe duplicate detection), #6 (rayon threshold), #7 (in-place CSR cursor), #8 (debug-only weight validation)

All "change" percentages are relative to baseline.

## Applied Changes

### Hot path (after-hotpath stage)

1. **#1 — Drop `settled: Vec<bool>` array** in `dijkstra_cost` and `dijkstra_path`. Use the popped cost (`f64::from_bits(d_bits)`) compared against `dist[u]` as the stale-entry check. Removes one O(V) zeroed allocation per query.
2. **#3 — `NodeAttrs` switched from `std::collections::HashSet` to `rustc_hash::FxHashSet`**. Filter predicates that call `attrs.contains(...)` no longer pay SipHash cost.

### Construction (final stage)

3. **#5 — Single hash probe for duplicate detection**. `node_index.insert(...).is_some()` replaces `contains_key + insert`.
4. **#6 — Rayon threshold for edge resolution**. Below 10 000 edges, fall back to sequential `iter()`. Eliminates thread-pool dispatch overhead on small graphs.
5. **#7 — In-place CSR cursor**. `offsets.clone()` removed; `offsets` is mutated as the write cursor and shifted right by one after placement.
6. **#8 — Debug-only weight validation**. `debug_assert!(w.is_finite() && w >= 0.0)` during edge resolution; zero cost in release builds.

## Results

### Search — filtered queries (FxHashSet wins)

| Bench                                      | Baseline      | Final         | Change  |
|--------------------------------------------|---------------|---------------|---------|
| search/filter_cost/simple_one_attr         | (baseline)    | -             | **−29.1%** |
| search/filter_cost/compound_and            | (baseline)    | -             | **−29.5%** |
| search/filter_cost/strict_rare_attr        | (baseline)    | -             | **−31.3%** |
| search/attr_count_scaling/attrs_per_node/1 | (baseline)    | -             | **−30.1%** |
| search/attr_count_scaling/attrs_per_node/5 | (baseline)    | -             | **−33.3%** |
| search/attr_count_scaling/attrs_per_node/50| (baseline)    | -             | **−22.9%** |

### Search — unfiltered (settled-array removal trade-off)

| Bench                                  | Change  |
|----------------------------------------|---------|
| search/no_filter/sparse_dag_fan4/50000 | **−13.3%** |
| search/no_filter/sparse_dag_fan16/50000| **−7.0%**  |
| search/no_filter/sparse_dag_fan4/10000 | −3.4%   |
| search/no_filter/layered/20x10         | −2.7%   |
| search/no_filter/chain/1000            | **+26.5%** |
| search/no_filter/chain/10000           | **+31.7%** |
| search/no_filter/chain/100000          | **+30.2%** |

### Search — path reconstruction (chain shape)

| Bench                                       | Change  |
|---------------------------------------------|---------|
| search/path_reconstruction/full_path/10     | −3.1%   |
| search/path_reconstruction/full_path/100    | +16.7%  |
| search/path_reconstruction/full_path/100000 | +30.9%  |
| search/path_reconstruction/cost_only/10     | −16.7%  |
| search/path_reconstruction/cost_only/100000 | +38.9%  |

### Search — fan-out scaling (heap pressure)

| Bench                            | Change  |
|----------------------------------|---------|
| search/fanout_scaling/fan_out/1  | −3.5%   |
| search/fanout_scaling/fan_out/8  | −4.8%   |
| search/fanout_scaling/fan_out/32 | −2.7%   |

### Construction (rayon threshold + in-place CSR)

| Bench                                | Change      |
|--------------------------------------|-------------|
| construction/sparse_dag_fan4/1000    | **−69.6%**  |
| construction/chain/10000             | **−34.0%**  |
| construction/chain/50000             | ±0.2%       |
| construction/chain/100000            | +3.9%       |
| construction/sparse_dag_fan4/100000  | +4.8%       |
| construction/parallel_sort/sparse_dag_fan4/10000  | ±0.2%   |
| construction/parallel_sort/sparse_dag_fan4/500000 | +3.4%   |

### Concurrent queries

| Bench                                | Change  |
|--------------------------------------|---------|
| concurrent_queries/sequential_64     | **−7.2%** |
| concurrent_queries/parallel_64_rayon | −1.8%   |
| concurrent_queries/parallel_rayon/8  | −2.2%   |
| concurrent_queries/parallel_rayon/32 | −2.1%   |

## Analysis

### Where the gains come from

- **FxHashSet on `NodeAttrs`** is the single largest contribution. Every neighbour expansion calls `filter(&attrs)`; the closures in the bench (`attrs.contains("taxi")`) hash a `&str` against each set. SipHash → FxHash is a flat ≈30% reduction across all filter benchmarks regardless of attribute count.
- **Rayon threshold** turned 1k-edge construction into a near-instant operation (−70% on `sparse_dag_fan4/1000`). Below the 10k threshold, the par_iter dispatch was costing more than the work itself.
- **`settled` array removal** wins on dense / wide graphs (sparse_dag_fan4 at 50k: −13%) where stale heap entries are real and skipping the `Vec<bool>` allocation matters.

### Where the regressions come from

- **Chain-shaped graphs regress 25–32% on search**, and chain reconstruction benches regress up to +39%. Chains have fan-out 1, so there are zero stale heap entries to skip — the new `cost > dist[u]` check pays per-pop work for no benefit, while the old `settled[u]` path was a hot 1-byte branch. This is the trade-off: chains lose, dense graphs win.
- **Construction at very large V (≥100k)** regresses ~3–5%. The in-place cursor + shift-right has the same asymptotic work as the old `pos.clone()` approach but a less cache-friendly access pattern at scale. The savings are an O(V) `Vec<u32>` allocation, which is small at this size.

### Net assessment

For realistic GOAP / graph-search workloads (filtered traversal over wide DAGs), the changes are a clear win. The chain regression is real but applies to a degenerate shape rarely used in practice. Two follow-ups worth considering:

- Re-introduce a cheap "skip already-visited neighbour" check for fan-out-1 cases, possibly by reusing `dist[nb] != INFINITY` as a proxy.
- Revert #7 (CSR cursor) if the construction regression on >100k-node graphs matters more than saving one O(V) allocation.
