# Performance

Canonical measurement summary for grafo. Refer to this file for current observations and use dated comparisons for immutable historical evidence. Do not interpret provisional measurements as an accepted performance floor.

## Measurement status

- Last measured: **2026-10-08**, Apple M5, macOS 26.6.2 / Darwin 25.6.0, Rust/Cargo 1.99.0, Criterion 0.5.1.
- Profile: release, 100 samples, 3-second warm-up, 5-second measurement target.
- Complete runs: 57 legacy cases before, 57 after, 23 separate workspace cases.
- **Incomplete gates:** 19 legacy cases crossed the existing regression detector threshold; attribution remains unresolved. Standard paired flamegraphs are unavailable. Full-workspace Clippy fails on unchanged uncharles code under this toolchain.
- Other sessions and background load were not controlled. No threshold or CI override was applied.
- Full raw logs, confidence intervals, samples, saved baselines and reproduction commands: [dated comparison](./perf-comparison-2026-10-08.md).

## At a glance

| Workload | One-shot | Warm workspace |
|---|---:|---:|
| Fixed sparse frontier, 100k nodes | 15.151 µs | 1.918 µs |
| Same frontier, 250k nodes | 26.023 µs | 1.841 µs |
| Same frontier, 1m nodes | 99.484 µs | 1.847 µs |
| 100k-node chain | 552.579 µs | 498.075 µs |
| Broad equal-cost layers | 98.527 µs | 150.884 µs |
| Eight mixed queries, whole batch | 2.920 ms | 3.268 ms |

Warm narrow searches benefited, but the broad layered and mixed controls were approximately **53% and 12% slower**. Keep the workspace optional. Full-path and planner integration are unchanged; no planner speedup is established.

## Construction

Existing constructor code is unchanged. Observed after-run values are provisional because the legacy regression gate failed.

| Legacy benchmark | After-run estimate |
|---|---:|
| `construction/sparse_dag_fan4/1000` | 84.895 µs |
| `construction/parallel_sort/sparse_dag_fan4/500000` | 44.241 ms |

The [detector output](./workspace-search-evidence-2026-10-08/regression-check.log) lists every flagged construction/search/concurrency case. Do not dismiss it as noise without another attributable measurement.

## Search

The existing `search/no_filter` benchmarks call **full-path** `shortest_path`, not the cost-only API. The legacy `search/path_reconstruction` group separately compares full paths and costs.

| Legacy benchmark | After-run estimate |
|---|---:|
| Full path on 10k-node fan-4 sparse DAG | 3.749 µs |
| Full path on 100k-node chain | 639.367 µs |
| Cost-only on 100k-node chain | 553.003 µs |

Fresh workspaces measured 10.256 µs at 100k, 20.854 µs at 250k and 75.364 µs at 1m on the fixed-frontier fixture. Four identical queries including one fresh workspace's setup/drop measured 15.523 µs versus 47.229 µs with four one-shot queries. These are per-run estimates, not a universal amortization threshold.

The selective waypoint-filter control measured 9.881 µs one-shot versus 34.138 ns warm. Its endpoints are accepted and its cheap intermediate route is rejected. The unreachable reverse query measured 9.869 µs versus 11.780 ns. Both are intentionally tiny frontiers, not application-level speed predictions.

## Concurrent queries

| Workload | Estimate |
|---|---:|
| Legacy 512-query Rayon batch | 1.071 ms |
| Four tasks × 16 narrow one-shot queries | 551.890 µs |
| Four tasks × 16 narrow warm-workspace queries | 108.208 µs |

Rayon scheduling is included in both new variants. Each task owns one mutable workspace and shares an immutable graph. No global cache, lock or thread-local workspace is introduced.

## Memory and applicability

- Distance/stamp elements use 12 bytes per retained vertex slot.
- Buffers retain their maximum logical length, and vector/heap capacity can exceed lengths; allocator overhead is additional. This is not a resident-memory measurement.
- First use and growth initialize buffers. Generation rollover clears retained stamps; ordinary warm searches initialize discovered distances only.
- Drop the workspace to release retained memory.
- Prefer existing one-shot APIs when reuse or representative measurements do not justify retained state.
- Keep historical performance trade-offs and these unresolved gates distinct. The optional workspace does not ratify regressions in unchanged APIs.

## History

| Date | Document | Change |
|---|---|---|
| 2026-10-08 | [Workspace comparison](./perf-comparison-2026-10-08.md) | Optional generation-stamped cost workspace; provisional timings and unresolved gates |
| 2026-10-08 | [Sparse CSR investigation](./csr-sparse-search-investigation-2026-10-08.md) | Diagnosis and prototype comparisons against petgraph CSR, not shipped workspace results |
| 2026-05-01 | [Earlier comparison](./perf-comparison-2026-05-01.md) | Attribute hashing, settled-array removal, construction threshold and CSR cursor |
| 2026-04-20 | [First sweep](./benchmarks-2026-04-20.md) | Initial grafo baseline |
