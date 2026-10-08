# Sparse CSR search performance investigation

Diagnose why stock petgraph CSR Dijkstra outperformed grafo on the 100,000-node, 499,909-edge sparse fixture. Record source inspection, causal controls, native profiles, tested improvements and their applicability limits. This is diagnosis-only: production source and search behavior were not modified; no architectural decision was ratified.

## Result

1. The dominant bottleneck is **initializing every distance to infinity**, not CSR traversal or string labels. Grafo writes 800,000 bytes per query even though this search discovers only 380 nodes.
2. Native sampling attributes **4,308 of 5,103 main-thread samples (84.42%)** to `_platform_memset_pattern16` below `Graph::shortest_path_cost`.
3. Petgraph uses a sparse distance map and a full-graph visited **bitset**. It still has graph-sized initialization, but the bitset is about 12.5 KB instead of an 800 KB f64 distance array.
4. Warm, caller-owned epoch and touched-node workspaces reduce this particular query to **1.61 µs** and **1.84 µs**. These are prototypes over copied grafo CSR fields, not shipped APIs.
5. A fresh epoch workspace takes **14.27 µs** for one query, versus **11.59 µs** for the current public API in the first-call experiment. Mixed broad-frontier queries also erase the warm-workspace advantage. Do not replace the default algorithm indiscriminately.

## Scope and evidence

- Environment: Apple M5, macOS 26.6.2, Rust/Cargo 1.99.0, optimized profiles with debug information, Criterion 0.5.1, petgraph pinned to 0.8.3. Other processes were active; CPU placement, clocks and background load were not controlled.
- Comparison: grafo's public string-endpoint cost-only query; petgraph `Csr<String, f64, Directed, u32>` with stock Dijkstra and a goal. Both terminate at the goal and include result destruction. Petgraph additionally creates a map of discovered costs.
- Fixture: deterministic forward shortcuts, four shortcuts per node plus a weight-100 chain backbone. Deduplicate parallel edges retaining the cheapest weight. This is not the repository's historical sparse generator or a real GOAP trace.
- Both libraries traverse neighbors in the same sorted source/target order. Their heap tie policies differ; instrumentation below quantifies the resulting work difference.
- Main study: 89 cases, each run twice; the second run reverses search implementation order. Follow-up: 25 cases, each run twice in the same order. Each case uses 50 samples, 0.5-second warmup and a 1.5-second target measurement.
- Reported times average independent Criterion point estimates, using slope when available and mean otherwise. They are not pooled-sample estimates or new confidence intervals. Original per-run intervals are retained in JSON.
- Main-thread stack SVGs were generated from native macOS `sample` output. Idle Rayon worker threads were excluded; identical function names were merged. SVG widths represent inclusive sample counts, not an exact percentage of CPU instructions.
- [Evidence directory](./csr-sparse-search-evidence-2026-10-08/) contains raw logs, estimates, profiles, validation logs, source hashes and a reproducible source archive. It does not depend on retention of `/tmp`.

## Source mechanism

### Grafo

[`Graph::dijkstra_cost`](../src/graph.rs#L434) starts each nontrivial query with:

```rust
let v = self.node_ids.len();
let mut dist = vec![f64::INFINITY; v];
dist[src as usize] = 0.0;
let mut heap: BinaryHeap<(Reverse<u64>, u32)> = BinaryHeap::new();
```

- Initialization is O(V) even when early termination explores a small subset of the graph.
- The buffer is allocated and dropped on every query. In this macOS build, infinity initialization calls a nonzero-pattern fill routine.
- Each relaxed edge uses dense indexed distance lookup; this is advantageous on searches that touch many nodes.
- The heap begins empty and reallocates as it grows. Stale entries are discarded by comparing popped cost with the current distance.
- [`shortest_path_filtered_cost`](../src/graph.rs#L758) performs label resolution and endpoint-filter checks before search. The plain-cost API supplies a pass-all filter; these experiments do not benchmark expensive attribute predicates.
- [`dijkstra_path`](../src/graph.rs#L477) also initializes a graph-sized predecessor array. That is a related possible bottleneck, but full-path improvements were **not** measured here.

### Petgraph

[Petgraph 0.8.3 Dijkstra source](https://docs.rs/petgraph/0.8.3/src/petgraph/algo/dijkstra.rs.html) creates a visited bitset, an initially empty distance map and a binary heap.

- Only discovered nodes are added to the distance map. It does not initialize V floating-point distances.
- The full-graph bitset is zero-initialized. Thus petgraph is **not** free of O(V)-scaled metadata; its per-vertex metadata write volume is much smaller.
- The sparse map adds hashing, growth and lookup overhead. Those costs become disadvantageous on long chains or broad searches.
- Visited-node checks and a different heap tie policy change which equally scored nodes are expanded before the goal. Equal answers do not imply identical internal work.

## Quantified bottlenecks

### Distance initialization

| Isolated operation | 100k entries | 1m entries |
|---|---:|---:|
| Allocate/drop capacity, no element initialization | 0.077 µs | 0.102 µs |
| Allocate/fill/drop infinity f64 vector | 9.922 µs | 98.027 µs |
| Fill an already retained vector with infinity | 8.864 µs | 97.063 µs |
| Allocate/zero/drop u64 vector | 5.195 µs | 48.970 µs |

Interpretation:

1. The allocation bookkeeping is small in this warm allocator experiment. **Writing the graph-sized buffer** is the expensive part.
2. Reusing the allocation but still filling all V entries does not solve the main problem.
3. Zero initialization is substantially faster on this allocator/platform, but remains graph-sized and is not sufficient to match sparse-map search in the losing fixture.
4. Component benchmarks do not provide an exact additive decomposition of a complete query: cache state, compiler layout, allocation reuse and code generation differ.

### Causal padding control

Keep the original reachable edges, source 0 and goal 99,999 unchanged. Append isolated nodes, so the actual search problem is identical while V changes.

| Total vertices | Current grafo | Petgraph | Warm epoch | Warm touched | Stateless sparse Fx map |
|---:|---:|---:|---:|---:|---:|
| 100,000 | 12.873 µs | 6.116 µs | 1.610 µs | 1.839 µs | 4.753 µs |
| 250,000 | 26.290 µs | 6.340 µs | 1.644 µs | 1.874 µs | 4.817 µs |
| 1,000,000 | 99.470 µs | 6.776 µs | 1.657 µs | 1.896 µs | 4.852 µs |

This is the strongest causal evidence: grafo slows with unused vertices, while workspace and sparse-map strategies stay nearly flat. The original graph's CSR traversal is not becoming more expensive.

The 100k production query drifted from 14.249 µs to 11.497 µs between the main runs. Its copied native-kernel control was steadier at 11.514/11.106 µs. Do not interpret the gap between those two compiled instances as label-lookup cost or a precisely identified second defect; environment, compiler layout and allocation/cache placement remain possible influences. The unchanged graph-sized fill and its profile attribution are independently confirmed.

### Frontier and heap work

| Counter | Grafo trace | Stock petgraph |
|---|---:|---:|
| Discovered nodes | 380 | 389 |
| Expanded non-goal nodes | 89 | Not independently counted |
| Edges evaluated | 434 | 420 |
| Heap pushes | 396 | Not independently counted |
| Heap pops | 90 | Not independently counted |
| Stale heap pops | 0 | Not independently counted |
| Peak heap length | 307 | Not independently counted |

- Grafo evaluates only **0.087%** of the graph's 499,909 edges, but initializes distances for 100% of the vertices.
- Removing or optimizing stale-entry checks cannot explain the original loss: there are no stale pops in this grafo query.
- The different tie policies account for a modest difference in frontier work, not the dominant graph-sized initialization. The padding control keeps this work fixed.
- Pre-reserving heap capacity 512 removes seven heap reallocations in this fixture, but leaves the main fill intact.

### Allocation counters

An untimed diagnostic executable wraps `System` allocation calls. Atomic counters are enabled only around one query; they are not present in the benchmark or profile executables.

| Implementation | Allocation calls | Reallocation calls | Cumulative requested bytes |
|---|---:|---:|---:|
| Current grafo | 2 | 7 | 816,320 |
| Stock petgraph | 10 | 7 | 46,228 |
| Grafo kernel, heap reserved to 512 | 2 | 0 | 808,192 |
| Stateless sparse Fx map | 9 | 7 | 33,724 |
| Warm dense/touched/epoch workspace | 0 | 0 | 0 |

Requested bytes include each new reallocation size and are **not peak/live bytes**. Petgraph makes more allocation calls but requests far fewer bytes and runs faster. Allocation count alone is therefore the wrong optimization target.

### API and post-fix costs

- Resolving both string endpoints in the isolated 100k-node control takes about **0.007 µs**. It cannot explain a multi-microsecond disadvantage.
- Original native profile: infinity fill 84.42%; `BinaryHeap::pop` 3.57% of main-thread samples.
- Warm epoch profile: no pattern-fill samples; `BinaryHeap::pop` accounts for **33.47%**. Heap and relaxation-loop work become the next bottlenecks after initialization is removed.
- The isolated retained-fill loop compiles to inlined stores in `profile::main`; absence of a named fill routine in that control does not mean the stores disappeared. Almost all of its main-thread samples are at the fill-loop source line.
- [Native stack profile](./csr-sparse-search-evidence-2026-10-08/flamegraph-sample-native.svg) and [warm epoch stack profile](./csr-sparse-search-evidence-2026-10-08/flamegraph-sample-epoch.svg) provide the corresponding views.

## Tested improvements

All variants use the copied grafo CSR fields and existing non-negative f64 cost/bit-order heap strategy. They are safe-Rust search prototypes. Only the untimed allocation-counter wrapper uses `unsafe` to delegate standard allocator operations with documented safety contracts.

| Variant, original 100k sparse query | Time | Assessment |
|---|---:|---|
| Copied unchanged native kernel | 11.310 µs | Control; no algorithm improvement |
| Reserve heap capacity 512 | 10.351 µs | Small secondary win; fixture-specific capacity, not a universal default |
| Reuse distance allocation, fill all distances | 10.992 µs | Main fill remains; insufficient |
| Zero-sentinel distance encoding | 7.302 µs | Better initialization, but still slower than petgraph's 6.116 µs |
| Stateless sparse Fx distance map | 4.753 µs | Beats petgraph on this fixture without retained workspace; regresses broad/chain searches |
| Warm touched-node workspace | 1.839 µs | Reset only previously discovered entries; retain heap capacity |
| Warm epoch workspace | 1.610 µs | Per-query generation increment; initialize entries only when encountered |

### Touched-node workspace

1. Initialize the distance array once.
2. Record a node in a touched list on its first relaxation in a query.
3. Before the next query, reset only recorded distances to infinity and clear the list.
4. Clear the heap without releasing its allocation.

Costs/risks: one touched-list append per new node and O(K_previous) reset work; worst case remains O(V). A large query can make the following tiny query pay a large reset. The prototype records filtered-state-free cost searches only.

### Epoch workspace

1. Keep a distance vector, a u32 generation vector and a reusable heap.
2. Increment the generation for each query.
3. Treat a distance as uninitialized when its stored generation differs from the current generation.
4. On u32 wraparound, clear all stamps before reuse.

Costs/risks: another approximately 400 KB of stamp memory at 100k nodes, generation checks on relaxation, and one-time setup. Correctness tests cover generation wraparound, unreachable and self queries, stale heap entries, changed graph sizes and changed policies. The public design must define ownership, resizing and interrupted-query reset semantics; those are not ratified by this prototype.

### Zero-sentinel encoding

Use zero for unseen entries and encode known non-negative finite distance bits as `bits + 1`. The allocation can use zero initialization instead of a nonzero infinity fill.

Costs/risks: more representation complexity and branches, domain assumptions about finite non-negative costs, and continued O(V) initialization. Tests include zero/negative-zero edge costs and sums overflowing to infinity. It is not a replacement for complete numeric-contract validation. Chain performance regressed, so this is not the preferred universal fix.

## Applicability checks

### First-call setup and amortization

| Batch, including new scratch setup and destruction | Current grafo | Epoch workspace | Touched workspace |
|---|---:|---:|---:|
| One query | 11.592 µs | 14.274 µs | 11.943 µs |
| Four identical queries | 46.397 µs | 19.157 µs | 17.522 µs |

- Warm epoch is about 7× faster than the copied native kernel on the narrow fixture, but a cold epoch workspace is about 23% slower than the current API for one query.
- Four-query batches including setup verify an amortized gain: approximately 2.4× for epoch and 2.6× for touched reset.
- Epoch initializes an infinity distance vector on first use in this prototype. Zero-initializing that vector would be a reasonable additional cold-start experiment because generation stamps govern validity; it was not tested as a separate epoch variant.
- An optional workspace must be reused by a caller or worker to capture the measured benefit. Allocating and dropping it inside every existing query is not the proposed improvement.

### Alternative seeds

| 100k-node fixture seed | Current grafo | Petgraph | Warm epoch | Warm touched | Stateless sparse Fx map |
|---:|---:|---:|---:|---:|---:|
| 1 | 10.717 µs | 4.631 µs | 0.865 µs | 0.983 µs | 2.663 µs |
| 42 | 11.995 µs | 9.622 µs | 2.120 µs | 2.457 µs | 7.279 µs |
| 20,261,008 | 11.457 µs | 5.587 µs | 1.426 µs | 1.630 µs | 4.368 µs |

The narrow-frontier win is not unique to the original seed. These are still three additional synthetic fixtures, not a representative workload distribution.

### Broad searches and changing endpoints

| Workload | Copied native kernel | Warm epoch | Warm touched | Stateless sparse Fx map |
|---|---:|---:|---:|---:|
| 100k-node chain | 481.65 µs | 480.91 µs | 509.69 µs | 835.32 µs |
| Layered 3k-node graph | 332.45 µs | 298.73 µs | 297.35 µs | 425.79 µs |
| Eight changing queries on the 100k sparse graph, whole batch | Current production API: 2,676.70 µs | 2,679.97 µs | 2,702.74 µs | 3,244.41 µs |

- Touched reset regresses the chain control by about 6%; sparse-map search regresses it by about 73%.
- The changing-query batch includes goals 99,999 and 50,000, shifted sources, adjacent queries, unreachable reverse direction and a self query. Some queries expand broad frontiers; overall there is no meaningful workspace improvement.
- Do not generalize an approximately 7× narrow-query improvement to all query distributions or to overall planner runtime.

## Recommended direction

1. **For repeated narrow-frontier queries, design an optional caller-owned search workspace.** Prefer an epoch design when predictable tiny-query reset cost matters; compare touched reset when lower memory and simpler generation management matter. Keep `Graph` immutable and preserve concurrent readers by giving each concurrent worker its own mutable workspace, rather than adding a global lock or mutable cache inside `Graph`.
2. **Keep the current default until representative workloads justify changing it.** A fresh scratch allocation does not help single queries. [`Planner::plan`](../../goap-planner/src/planner.rs#L119) constructs a graph and calls full-path search once; cost-only warm-workspace results do not prove a planner improvement. A planner-level integration would need its own benchmark and public ownership design.
3. **Investigate an explicit sparse strategy or sparse-to-dense promotion for one-shot narrow searches.** The one-shot Fx map prototype beats petgraph in this case but loses on chains and changing-query batches. Promotion or automatic selection was not implemented; do not invent a universal node-count threshold from this fixture.
4. **Optimize the heap only after initialization is addressed.** Reusing heap storage is already included in workspace wins. A d-ary or monotone-key queue is a future experiment, not a measured recommendation to replace `BinaryHeap`. Arbitrary f64 costs, stale-entry behavior and tie handling must remain correct.
5. **Add benchmark coverage before production changes.** Include padded graphs with a fixed reachable subgraph, warm and cold workspaces, broad/unreachable queries, mixed endpoint batches, full-path and filtered queries, graph resizing and concurrent independent workspaces. Preserve the existing chain, layered and dense controls.

No new issue or ADR was created. Open `type:design` and `type:feature` issues were checked on 2026-10-08; none in the returned lists directly tracked this search-workspace optimization. A significant allocation/API decision would require the project's issue/ADR workflow before implementation.

## Assumptions challenged

1. **“Petgraph has faster CSR.”** The padding control changes no searched edges but changes grafo latency by nearly an order of magnitude. Inverted approach: optimize per-query metadata, retaining the CSR storage.
2. **“Fewer allocations means faster queries.”** Petgraph makes more allocation calls but writes much less graph-sized metadata. Inverted approach: measure initialized bytes and sampled fill time, not allocation count alone.
3. **“Reuse a buffer and the problem is fixed.”** Retained full-array fill still takes roughly 9 µs at 100k entries. Inverted approach: reset only touched entries or use generations.
4. **“A warm workspace or sparse map should replace everything.”** Cold single-query and changing-query tests contradict that. Inverted approach: expose workload-appropriate optional reuse while retaining a competitive one-shot default.

## Inverted approach

Move query state into an explicitly owned reusable context, not into the graph's immutable storage. Validate benefits at the caller's query lifecycle, including setup and graph changes, rather than at a single repeated microbenchmark alone.

## The do-nothing option

Retain the current dense-distance default if callers primarily issue one query per newly built graph or traverse broad frontiers. The losing fixture is not evidence that the CSR architecture is faulty. Documentation and benchmark coverage can improve without immediately changing search behavior.

## Simpler problem

The current anomaly is a short search paying for a full-graph infinity fill. Removing that unconditional repeat initialization is a smaller problem than swapping graph libraries or changing shortest-path algorithms.

## Risks and tests

- Workspace sharing could serialize or corrupt concurrent queries: test separate per-worker contexts and preserve `Graph: Send + Sync` without interior mutability.
- Old distances or predecessor entries could leak across queries: test alternating reachable/unreachable/self queries, interrupted queries, filtered queries, wraparound and graph ID reassignment. Production full-path/filtered workspace behavior remains unimplemented.
- A sparse-to-dense heuristic could regress long searches: benchmark promotion overhead and mixed distributions rather than selecting solely from V.
- Numeric node IDs are not guaranteed topological order: do not prune all IDs above the goal. All-pairs tests here explicitly reverse IDs while retaining acyclic topology.
- Warm results could hide expensive setup: retain first-call and amortized-batch measurements.
- Hidden thread-local caching could cause reentrant borrowing failures or unbounded retained memory: prefer explicit mutable ownership and document memory limits.

## Verification and remaining limits

- Original failing-correctness stub was observed returning `None` instead of known cost 3 before probe implementation; see `test-red.txt`.
- Harness: **3 tests passed**, including 10,752 all-pairs variant comparisons over six reordered DAGs with zero/fractional weights.
- Copied crate: **40 unit tests passed**, including five prototype-focused tests and the existing 35 grafo tests.
- Every main benchmark independently checks eight endpoint pairs for each of seven variants on each fixture against production grafo and petgraph.
- Harness and copied crate formatting and warnings-as-errors Clippy checks passed. Active LSP probes were inconclusive, not confirmed-clean evidence.
- The production workspace test suite, full-path candidate benchmarks and filtered candidate benchmarks were not run; production source was unchanged and candidate public behavior is not shipped.
- Native profiling succeeded without installing tools or changing permissions. The customary `cargo flamegraph` command remains unavailable, recorded in `cargo-flamegraph-unavailable.txt`. These scoped sample profiles are diagnostic evidence, not a claim that the repository's standard per-run profiling pipeline was completed.
- Production `grafo/src/graph.rs` SHA-256 remains `9f90facf365b86dda26febc8fa4801047d1993a2b4e4043205a2e9df19d7bc70`, identical to the previous comparison and investigation start.
- No measured memory footprint or architecture-independent speed guarantee is asserted. Requested allocation bytes and static scratch sizes are different quantities from resident-memory measurements.

## Reproduce

Extract [the source archive](./csr-sparse-search-evidence-2026-10-08/harness-source.tar.gz) into a temporary directory. The tested manifest points at this machine's live workspace grafo path. Verify the recorded source hash before rerunning, or change that dependency path to the included `baseline-source` snapshot for a frozen reproduction.

```sh
cargo test --release
cargo test --release --lib --manifest-path grafo-probe/Cargo.toml
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --bench investigation -- --save-baseline study1
REVERSE_BENCH_ORDER=1 cargo bench --bench investigation -- --save-baseline study2
cargo bench --bench followup -- --save-baseline follow1
cargo bench --bench followup -- --save-baseline follow2
cargo run --release --example work
```

To sample a bounded owned process, build `examples/profile.rs` in release mode, launch `profile native 10`, wait for its `READY` message, then run `sample <its-pid> 6 1 -file profile-native.txt` and wait for that process to exit. Repeat with `epoch` and `fill` modes. Do not profile another user's process or run profiles concurrently with benchmark measurements.