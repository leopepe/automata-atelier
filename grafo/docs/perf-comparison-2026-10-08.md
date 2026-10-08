# Reusable cost-search workspace performance

Compare the unchanged one-shot APIs with the shipped caller-owned workspace. Use this record to reproduce measurements, restore saved Criterion baselines and evaluate future changes. The historical CSR investigation is preserved separately; its prototype numbers are not production-workspace measurements.

## Scope

- Add optional `SearchWorkspace` storage and unfiltered/filtered cost-query entry points.
- Keep CSR, existing cost/path kernels, planner integration and graph mutability unchanged.
- Use one mutable workspace per simultaneous worker.
- Retain maximum distance/stamp lengths and heap capacity; drop the workspace to release memory.
- Compare fresh and warm workspaces independently. Fresh means a new workspace per timed iteration, not flushed CPU or allocator caches.
- Preserve the investigation's deterministic 100,000-node reachable sparse fixture: seed `0xdead_beef_cafe`, four forward shortcuts and a weight-100 chain backbone, retaining the cheapest duplicate edge.
- Pad that fixture with isolated vertices without changing source `0`, destination `99999`, edges or reachable frontier.
- Include chain, broad layered, unreachable, mixed endpoint, filtered and concurrent workloads. Filtered endpoints pass; an intermediate node is actually rejected.

## Reproduce later

Run from the workspace root. Default Criterion sampling uses 100 samples, a 3-second warm-up and a 5-second measurement target. Cargo benchmarks use release builds. Keep machine load comparable and do not profile concurrently with timed benchmarks.

```sh
# Compare existing kernels against the recorded pre-change run.
cargo bench -p grafo-dag --bench performance -- --baseline reusable-before

# Save or compare the shipped one-shot/fresh/warm workload suite.
cargo bench -p grafo-dag --bench workspace -- --save-baseline workspace-v2
cargo bench -p grafo-dag --bench workspace -- --baseline workspace-v1

# Restrict iteration to the fixed-frontier control.
cargo bench -p grafo-dag --bench workspace -- 'workspace/padding'
```

Restore the committed JSON-only Criterion archive into a separate clean checkout before using recorded baseline names:

```sh
tar -xzf grafo/docs/workspace-criterion-2026-10-08.tar.gz
```

The archive restores `target/criterion/` entries, including `reusable-before` and `workspace-v1`. It contains estimates, raw samples and benchmark metadata, not compiled executables. A comparison with a historical run on another machine or under different load is not a causal regression test: record a fresh same-machine baseline for that purpose. Do not overwrite baselines you still need.

## Petgraph CSR comparison

The [investigation](./csr-sparse-search-investigation-2026-10-08.md) and its [reproducible source archive](./csr-sparse-search-evidence-2026-10-08/harness-source.tar.gz) preserve the petgraph **0.8.3 CSR** comparison and baseline source. No other petgraph graph representation is an accepted comparator. The new repository benchmark has no petgraph dependency and compares shipped grafo strategies only.

When using the archived external harness with this implementation, point its `grafo` path dependency at the current crate. Add a candidate that creates `SearchWorkspace` outside the timed loop and calls `shortest_path_cost_with_workspace` inside it; separately benchmark a fresh workspace inside the loop. Verify candidate results against stock petgraph CSR Dijkstra before timing. Do not relabel the archived prototype `Scratch` results as measurements of the shipped `SearchWorkspace`.

## Verification and incomplete gates

- New API integration tests fail against the pre-change source because the type/methods are absent, then pass with the implementation.
- Removing rollover stamp invalidation makes the rollover test return `None` instead of `Some(2.0)`.
- Removing heap clearing makes the retained-frontier test return `Some(11.0)` for an unreachable query.
- Workspace debug/release tests and doctests pass; workspace-wide tests and formatting pass.
- Grafo warnings-as-errors Clippy passes, including its benchmark and integration targets.
- Full-workspace Clippy fails on unchanged `uncharles` code under Rust 1.99: seven `unused_async_trait_impl` sites and eleven additional `assert_is_empty` sites in the test target. No unrelated crate or lint policy was changed. Preserve this as an incomplete workspace gate.
- Paired `cargo flamegraph` commands fail because the subcommand is not installed. Historical native sample profiles are diagnosis evidence only; they do not satisfy the required paired profiling gate for these production measurements. Keep the PR draft until the missing gates are resolved.
- Implementation and review are direct, per the operator's instruction; no independent-review pass is claimed.

## Measurement results

Measured 2026-10-08 on Apple M5, macOS 26.6.2 / Darwin 25.6.0, Rust/Cargo 1.99.0, Criterion 0.5.1. Both complete 57-case legacy runs and the 23-case workspace run exited zero. Use the regression detector separately: successful benchmark execution does not mean the performance gate passed. Background load, CPU placement and caches were not controlled.

Values below are Criterion slope point estimates (mean when slope is absent), rounded from the archived estimates. Confidence intervals and raw samples remain in the archive; speedup ratios are descriptive, not pooled statistical estimates.

| Workload | One-shot | Warm workspace | Fresh workspace |
|---|---:|---:|---:|
| Fixed frontier, 100k total nodes | 15.151 µs | 1.918 µs | 10.256 µs |
| Same frontier, 250k total nodes | 26.023 µs | 1.841 µs | 20.854 µs |
| Same frontier, 1m total nodes | 99.484 µs | 1.847 µs | 75.364 µs |
| 100k-node chain | 552.579 µs | 498.075 µs | not measured |
| 100 × 30 equal-cost layers | 98.527 µs | 150.884 µs | not measured |
| Unreachable reverse query, 100k nodes | 9.869 µs | 0.0118 µs | not measured |
| Eight mixed queries, complete batch | 2.920 ms | 3.268 ms | not measured |
| Four identical queries, complete batch | 47.229 µs | not measured | 15.523 µs |
| Filtered narrow query, 100k nodes | 9.881 µs | 0.0341 µs | not measured |
| Four concurrent tasks × 16 queries, complete batch | 551.890 µs | 108.208 µs | not measured |

- Warm narrow searches improved approximately 7.9× at 100k nodes and 53.8× at 1m padded nodes in this run.
- Fresh zero-initialized buffers performed better than the old infinity-initialized prototype on this platform. Do not generalize this first-call result to other allocators or workloads.
- The equal-cost broad layered control was approximately **53% slower** with the workspace, and the mixed batch approximately **12% slower**. These are reasons to preserve the optional API and one-shot default, not hide the losing shapes.
- Filtered and unreachable controls remove a large initialization cost from very small searches; their nanosecond query times do not predict planner or end-to-end application gains.
- The separate `workspace` benchmark target preserves legacy benchmark IDs and avoids requiring nonexistent pre-feature workspace baselines. The existing CI target remains `performance`; new workspace measurements must be run explicitly.

## Legacy regression gate

The existing detector (`python3 .github/scripts/detect-bench-regression.py <after-log> 10`) exited **1**: 19 of 57 unchanged-API control benchmarks exceeded the lower-confidence-bound threshold. They include ten construction cases, seven unfiltered searches, the pass-all filtered search and a 32-query concurrent case. See the [complete detector output](./workspace-search-evidence-2026-10-08/regression-check.log).

The existing construction, CSR fields and default search kernels were not changed. The observed regressions are nevertheless an **unresolved gate**, not a proven noise explanation or an accepted new floor. Do not apply `bench:allow-regression`, relax thresholds or mark the PR ready based on this record. A quiet same-machine rerun and paired profiling are needed to attribute them. Broad-workspace losses above are a separate comparison, not an explanation for legacy constructor/search changes.

## Saved evidence

- [Before-change full legacy run](./bench-search-workspace-before.txt).
- [After-change full legacy comparison](./bench-search-workspace-after.txt).
- [All workspace strategies](./bench-search-workspace-strategies.txt).
- [Raw Criterion JSON archive](./workspace-criterion-2026-10-08.tar.gz).
- [Validation, missing profiling commands and source metadata](./workspace-search-evidence-2026-10-08/).
- [Historical petgraph CSR investigation](./csr-sparse-search-investigation-2026-10-08.md).
