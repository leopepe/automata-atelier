# Performance

Use this document for the latest measured working-tree candidate. Keep functional correctness separate from measured performance. The audit repairs pass functional checks; the operator waived the failed before/after performance gates for this change on 2026-10-20. This is an acceptance exception, not a passing benchmark or an independently accepted release baseline.

## Measurement identity

- Last measured: 2026-10-20; Criterion0.5, 100 samples, 3-second warm-up, release profile.
- Candidate: grafo-dag0.1.0, uncommitted `fix/grafo-audit-contracts`.
- Platform: native aarch64 macOS Darwin25.6.0, Rust1.99.1.
- Statistic in tables: Criterion JSON median point estimate from the preserved full after runs. Criterion's displayed time/comparison statistic can differ; do not mix them.
- Raw evidence: [performance before](./bench-audit-before.txt), [performance after](./bench-audit-after.txt), [workspace before](./bench-workspace-audit-before.txt), [workspace after](./bench-workspace-audit-after.txt).
- Comparison and acceptance limitations: [2026-10-20 audit repairs](./perf-comparison-2026-10-20.md).
- Workspace architecture: [ADR0006](../../docs/adrs/0006-use-caller-owned-search-workspaces.md); error-contract correction: [proposed ADR0007](../../docs/adrs/0007-return-recoverable-graph-errors.md).

## At a glance

| Workload | Candidate median |
|---|---:|
| Full-path search, sparse DAG fan4, 10k nodes | 4.502 µs |
| Cost-only chain, 100k nodes | 564.848 µs |
| Full-path reconstruction control, 100k nodes | 721.148 µs |
| Construction, sparse DAG fan4, 1k nodes | 83.826 µs |
| Parallel-construction group, sparse DAG fan4, 500k nodes | 53.031 ms |
| 512 parallel Rayon queries | 866.173 µs |
| Repaired 5k-node attribute-scaling search, exact1–50 distinct tags | 389–411 µs; fixed accepted route |
| Warm workspace, 100k-node graph with narrow frontier | 2.165 µs |

The 100k reconstruction control is approximately1.28× slower than cost-only search at that callsite. Do not substitute a different benchmark group's absolute time for the control.

## Construction

Includes label/attribute ownership, endpoint and weight validation, parallel edge resolution when applicable, and count/prefix-sum CSR placement. The legacy `parallel_sort` benchmark identifier remains unchanged for comparability; it does not imply a current sorting step.

| Shape | Nodes | Candidate median |
|---|---:|---:|
| chain | 1,000 | 40.357 µs |
| sparse DAG fan4 | 1,000 | 83.826 µs |
| chain | 10,000 | 389.869 µs |
| sparse DAG fan4 | 10,000 | 905.549 µs |
| chain | 100,000 | 4.309 ms |
| sparse DAG fan4 | 100,000 | 8.823 ms |
| parallel_sort group, sparse DAG fan4 | 500,000 | 53.031 ms |

## Search — no filter

These are the `search/no_filter` full-path `shortest_path` benchmarks, not cost-only searches.

| Shape | Nodes | Candidate median |
|---|---:|---:|
| sparse DAG fan4 | 1,000 | 630.559 ns |
| sparse DAG fan4 | 10,000 | 4.502 µs |
| sparse DAG fan4 | 50,000 | 8.495 µs |
| sparse DAG fan16 | 10,000 | 5.403 µs |
| sparse DAG fan16 | 50,000 | 20.005 µs |
| layered50×20 | 1,000 | 42.331 µs |
| layered100×30 | 3,000 | 213.151 µs |
| chain | 10,000 | 81.161 µs |
| chain | 100,000 | 806.947 µs |

## Search — legacy filter controls

The unchanged10,000-node attributed generator remains the input to `search/filter_cost`. Selective cases can reject endpoints; their nanosecond results do not represent frontier traversal.

| Variant | Candidate median |
|---|---:|
| shortest_path, no filter | 4.556 µs |
| pass-all closure | 4.875 µs |
| single taxi membership | 9.982 ns |
| compound AND | 10.161 ns |
| rare-attribute predicate | 10.264 ns |

## Search — path reconstruction controls

Compare `shortest_path`, which returns indices, with `shortest_path_cost` at the same benchmark callsite. Label resolution is not inside the timed path.

| Path length | Full path | Cost only |
|---:|---:|---:|
| 10 | 176.253 ns | 73.292 ns |
| 100 | 904.078 ns | 584.367 ns |
| 1,000 | 7.235 µs | 5.389 µs |
| 10,000 | 72.139 µs | 55.898 µs |
| 100,000 | 721.148 µs | 564.848 µs |

## Search — repaired attribute scaling

Every node has the advertised number of distinct tags, including taxi. All counts share deterministic topology, weights, endpoints and the accepted5,000-node chain; preflight assertions run outside timing.

| Distinct tags per node | Candidate median |
|---:|---:|
| 1 | 398.321 µs |
| 5 | 389.240 µs |
| 10 | 410.927 µs |
| 20 | 402.093 µs |
| 50 | 409.387 µs |

Treat these as a workload reset. Previous endpoint-rejection measurements are not a speed baseline for these searches.

## Caller-owned workspace

All values below are median estimates from the separate workspace harness, not the one-shot performance harness.

| Shape | One shot | Warm workspace |
|---|---:|---:|
| padding100k, narrow frontier | 12.037 µs | 2.165 µs |
| padding250k, narrow frontier | 26.471 µs | 2.158 µs |
| padding1M, narrow frontier | 100.656 µs | 2.085 µs |
| chain | 506.157 µs | 673.220 µs |
| layered | 115.190 µs | 126.239 µs |
| unreachable | 10.161 µs | 13.830 ns |
| filtered | 12.123 µs | 58.042 ns |
| mixed batch x8 | 3.520 ms | 3.585 ms |
| four workers, x16 queries each | 741.286 µs | 208.354 µs |

Fresh-workspace padding costs10.370/21.211/76.699 µs at100k/250k/1M nodes. The amortized fresh-workspace x4 batch measured25.710 µs versus65.001 µs for its one-shot x4 control. Warm narrow queries benefit from retained buffers; broad/mixed or cold workloads need not improve. These comparisons are workload-specific, not a universal workspace speedup.

## Concurrent queries

The performance harness shares one10k-node graph through Arc.

| Mode | Query count | Candidate median |
|---|---:|---:|
| sequential | 64 | 224.819 µs |
| Rayon parallel64 control | 64 | 144.679 µs |
| Rayon sweep | 8 | 35.022 µs |
| Rayon sweep | 32 | 76.434 µs |
| Rayon sweep | 128 | 237.379 µs |
| Rayon sweep | 512 | 866.173 µs |

## Trade-offs and operator waiver

- Required input validation and overflow-reachability handling remain enabled; no gate was weakened to recover old timing numbers.
- The full comparison flagged27 unchanged performance cases and10 workspace cases beyond the repository's robust threshold. Five additional scaling flags are changed workloads, not comparable regressions.
- A bounded same-source remeasurement moved substantially. Neither compact internal carriers nor a NaN sentinel improved the representative failure set; both experiments were reverted.
- Do not attribute every observed slowdown to code or every difference to host noise. Stable-host attribution remains unresolved.
- The operator waived performance-test failures for this change on 2026-10-20. This removes the local completion blocker without changing thresholds, CI configuration, future policy or the recorded failed results.
- cargo-flamegraph is unavailable. Paired attempts failed; no CPU attribution or full profiling compliance is claimed.
- Preserve earlier settled-array/shape trade-off reasoning in the immutable historical reports rather than applying old measurements to the current candidate.

## History

| Date | Document | Change |
|---|---|---|
| 2026-10-20 | [Audit repair comparison](./perf-comparison-2026-10-20.md) | Typed errors, overflow distinction, checked resolution, controlled scaling fixtures; performance failures explicitly waived by operator |
| 2026-05-01 | [Previous comparison](./perf-comparison-2026-05-01.md) | FxHashSet, settled-array removal, Rayon threshold and in-place CSR cursor |
| 2026-04-20 | [Initial sweep](./benchmarks-2026-04-20.md) | First full benchmark sweep |