# Grafo audit repair performance comparison

Record the full before/after measurements for the F1/F2/F3/F4/F5/F9 repair candidate. Treat functional correctness and measured performance as separate results. Initial performance acceptance was blocked; the operator subsequently waived these failures for this change on 2026-10-20. The failed measurements below are preserved, not relabelled as passing.

## Measurement identity

- Baseline source: `70d543fbd9c08c58b65560f8a2801b8ec367f3f1`.
- Candidate: uncommitted `fix/grafo-audit-contracts`; the reviewed source was restored byte-for-byte after two rejected optimization experiments.
- Platform: native aarch64 macOS Darwin 25.6.0; Rust 1.99.1; Criterion 0.5.
- Full sampling: release profile, 100 samples, 3-second warm-up, 5-second requested measurement; Criterion extended some measurements.
- Both harnesses ran on the same machine, without changing inputs except the explicitly repaired attribute-scaling workload.
- The named `grafo-audit-before` baseline was captured before production changes and preserved throughout diagnosis.
- Same-machine sequential measurements do not establish fixed CPU scheduling, thermal state or identical generated code layout.

## Raw evidence

| Harness | Before | After | Process exit |
|---|---|---|---|
| performance, 57 cases | [Before log](./bench-audit-before.txt) | [After log](./bench-audit-after.txt) | 0 / 0 |
| workspace, 23 cases | [Before log](./bench-workspace-audit-before.txt) | [After log](./bench-workspace-audit-after.txt) | 0 / 0 |
| repaired attribute scaling, focused run | Not comparable | [Focused log](./bench-attribute-scaling-audit.txt) | 0 |

Successful benchmark process exits do not imply passing performance gates.

## Reproduction

Run both harnesses before applying the repair, then compare the candidate against that same saved baseline:

```sh
cargo bench --locked -p grafo-dag --bench performance -- --save-baseline grafo-audit-before
cargo bench --locked -p grafo-dag --bench workspace -- --save-baseline grafo-audit-before
# Apply the repair before the following commands; do not replace the baseline.
cargo bench --locked -p grafo-dag --bench performance -- --baseline grafo-audit-before
cargo bench --locked -p grafo-dag --bench workspace -- --baseline grafo-audit-before
```

## Comparable-workload gate result

Use the repository's unchanged `detect-bench-regression.py` robust gate: Criterion must classify a regression and its lower 95% confidence bound must exceed the size-aware threshold, normally 10%, with a 2 ns noise floor.

| Population | Cases | Robust failures |
|---|---:|---:|
| Original performance output, including changed scaling inputs | 57 | 32 |
| Comparable performance cases, excluding only five scaling workload resets | 52 | 27 |
| Unchanged workspace cases | 23 | 10 |

Do not interpret the five changed scaling cases as library slowdowns. Excluding them does not clear either gate. No threshold, benchmark input or regression-allow policy was changed to obtain a pass.

## Representative full-run changes

Times and changes in this table are Criterion's displayed point estimates and comparison intervals from the raw logs, not JSON median estimates. The canonical [performance summary](./performance.md) uses separately labelled JSON medians.

| Case | Before | After | Criterion change / interval |
|---|---:|---:|---|
| construction sparse fan4, 100k | 6.2913 ms | 8.8510 ms | +40.686%; [+39.419%, +42.044%] |
| construction parallel_sort, 500k | 43.352 ms | 54.012 ms | +24.588%; [+22.941%, +26.430%] |
| full-path search chain, 100k | 611.87 µs | 803.01 µs | +32.163%; [+31.594%, +32.771%] |
| cost-only reconstruction control, 100k | 500.57 µs | 562.99 µs | +11.924%; [+11.376%, +12.475%] |
| workspace padding100k, warm | 1.8844 µs | 2.1508 µs | +16.821%; [+15.555%, +17.980%] |
| workspace fresh batch x4 | 15.846 µs | 25.300 µs | +65.142%; [+63.175%, +67.280%] |
| workspace filtered, warm | 36.983 ns | 58.293 ns | +56.584%; [+54.605%, +58.433%] |
| workspace chain, one-shot | See raw log | 508.64 µs | -30.887%; [-31.246%, -30.497%] |
| workspace padding1M, warm | See raw log | 2.0780 µs | -22.021%; [-22.845%, -21.210%] |

Report improvements as observed cases, not a universal speedup. Do not attribute the entire change to error handling or to host noise without controlled evidence.

## Attribute-scaling workload reset

- Previous fixtures sampled with replacement from 20 names. Some cases rejected endpoints; 50 distinct attributes were impossible.
- The repaired fixture contains exactly 1/5/10/20/50 distinct tags on every node, including taxi.
- Topology, weights, endpoints and the nontrivial accepted chain remain fixed across counts.
- The actual generator and preflight are shared with executable integration tests. Preflight runs outside timing.
- The existing non-scaling attributed generator was preserved, so filter-cost controls remain comparable.
- Measure the repaired cases as a new full-search baseline; do not compare their approximately 0.4 ms timings with the old endpoint-rejection timings as a speed regression.

## Bounded diagnostic result

One same-source subset remeasurement showed substantial movement relative to the full after run: parallel construction500k measured 42.605 ms rather than 54.012 ms, and filtered warm workspace measured 35.825 ns rather than 58.293 ns. These observations establish measurement variability, not that every original regression is false.

Two internal-only candidates were measured against a separately saved `repair-current` subset: compact private error carriers, and a NaN unseen sentinel. Neither improved the representative failure set. Both were rejected and the reviewed production source was restored exactly. The original failed full logs and baseline evidence remain retained; no repeated full rerun was used to seek a green result.

Performance attribution remains unresolved. The operator's subsequent change-specific waiver removes the acceptance blocker, but does not establish performance neutrality or a passing gate.

## Profiling limitation

Paired `cargo flamegraph` commands were attempted for both baseline/after harnesses and focused measurements. Each failed with exit101 because `cargo-flamegraph` is unavailable. No installation, sudo escalation or fabricated SVG was performed. CPU attribution is unavailable; full profiling-policy compliance is not claimed.

## Functional verification

The retained candidate passes 236 workspace tests/doctests and 104 grafo tests/doctests in release, strict workspace all-target/all-feature Clippy, formatting, all-target checking and warning-denied rustdoc. Three independent source-review lenses returned PASS. The separate operator waiver below, not these functional results, disposes of the performance failures.

## Acceptance state

- Implemented: approved six functional/coverage/fixture repairs.
- Deferred: low-severity F6/F7/F8 cleanup, except necessary changed-contract documentation.
- Accepted exception: the operator explicitly instructed ignoring performance-test failures for this change on 2026-10-20; task3.3 closes under that scoped waiver and local archival may proceed.
- Preserved limitations: the measurements failed and attribution/profiling remain unresolved. No thresholds, benchmark inputs, CI configuration or future acceptance policy were changed.
- Optional future investigation: controlled base/candidate measurements with stable host conditions and usable profiling. No further performance retries are part of this change.