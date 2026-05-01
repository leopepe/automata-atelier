# Grafo

Grafo is a library, it is used to generate fast graphs with fast search using rust async and parallelization capabilities to provide an fast access to graphs, nodes and edges.

## Search Algorithm

Grafo uses Djikstra as search algorithm.

## Tech Stack

### Language & Edition
- **Rust** (edition 2024)

### Graph Model
- **Directed Acyclic Graph (DAG)** — the primary graph type exposed by the library
- **Compressed Sparse Row (CSR)** — internal storage layout; O(V + E) memory, cache-friendly edge traversal
- **String node identifiers** — nodes and edges are referenced by human-readable `&str` labels, interned as `String` internally via a `HashMap<String, usize>` index
- **`f64` edge weights** — all edge costs are 64-bit floating-point values

### Search Algorithm
- **Dijkstra's algorithm** — binary min-heap with lazy deletion, O((V + E) log V)
- Uses `std::collections::BinaryHeap` as the priority queue
- Supports two variants: full path reconstruction (`Vec<String>`) and cost-only (avoids `String::clone` overhead)

### Design Pattern
- **Goal-Oriented Action Planning (GOAP)** — node attributes map to world states, directed weighted edges map to actions, and the `filter` closure passed to `shortest_path_filtered` acts as a per-node precondition; nodes failing the predicate are pruned from the search frontier

### Node Attributes
- Each node may carry an arbitrary `Vec<String>` of string tags (e.g. `"taxi"`, `"bus"`)
- Attribute-based filtering is the primary mechanism for GOAP-style constrained path queries

### Concurrency & Parallelism
- **`rayon`** (v1) — data-parallel construction helpers (parallel graph building)
- **`std::sync::Arc`** — enables shared, read-only `Graph` access across threads
- **`std::thread`** — used for concurrent query workloads; `Graph` is `Send + Sync`

### Error Handling
- Custom **`GraphError`** enum with `Display` + `std::error::Error` impls
  - `DuplicateNode(String)` — raised when a node label appears more than once during construction
  - `UnknownNode(String)` — raised when an edge or query references an unregistered label
- All public API returns `Result<_, GraphError>`

### Benchmarking
- **`criterion`** (v0.5, `html_reports` feature) — micro-benchmark harness with statistical analysis and HTML report generation under `target/criterion/`
- Benchmark suite: `benches/performance.rs`. Header doc comment lists the canonical invocation forms.

## When to run benchmarks

Run the full benchmark suite **before and after** any change that could affect runtime performance, so a numerical comparison exists. The "after" run alone is not evidence — without a baseline, regressions are invisible.

Trigger a benchmark run when changes touch:
- `Graph` construction (`Graph::new`, `Graph::new_with_attrs`, CSR build, label interning)
- Search algorithms or their helpers (`dijkstra_cost`, `dijkstra_path`, `reconstruct_path`)
- Public search APIs (`shortest_path*`, `shortest_path_filtered*`)
- Internal data layout (`offsets`, `targets`, `weights`, `node_ids`, `node_attrs`, `node_index`) or their types
- `NodeAttrs` storage or lookup
- Parallelism boundaries (rayon usage, thresholds, `Arc<Graph>` sharing)
- Dependencies that affect hashing, allocation, or threading (e.g. `rustc-hash`, `rayon`)

Skip benchmarks for changes that are clearly perf-neutral: doc-only edits, test-only edits, error message strings, public API renames without semantic change.

### Benchmark workflow

Every `cargo bench` invocation has a paired `cargo flamegraph` invocation — two artifacts per run (`bench-<label>.txt` and `flamegraph-<label>.svg`). See [Flamegraphs](#flamegraphs) below for the rationale.

1. **Capture baseline on unmodified code** (criterion + flamegraph):
   ```sh
   cargo bench --bench performance -- --save-baseline before 2>&1 | tee docs/bench-before.txt
   cargo flamegraph --bench performance -o docs/flamegraph-before.svg -- --bench --profile-time 10
   ```
2. **Apply the change**, then re-run both:
   ```sh
   cargo bench --bench performance -- --baseline before 2>&1 | tee docs/bench-after.txt
   cargo flamegraph --bench performance -o docs/flamegraph-after.svg -- --bench --profile-time 10
   ```
   Note: `--save-baseline` and `--baseline` are mutually exclusive in criterion.
3. **Save raw logs** to `docs/bench-<label>.txt` and **save flamegraphs** to `docs/flamegraph-<label>.svg` so future runs have something to diff against.
4. **Write a comparison summary** as `docs/perf-comparison-YYYY-MM-DD.md` listing wins, regressions, and trade-offs. Reference the matching `docs/flamegraph-<label>.svg` files so the criterion numbers and CPU profile are linked in one document.
5. **Iterate subsets** during development with `cargo bench --bench performance -- <filter>` (e.g. `search/filter_cost`) — full runs take ~10 minutes. Pair each iteration run with a scoped flamegraph: `cargo flamegraph --bench performance -o docs/flamegraph-<label>.svg -- --bench --profile-time 10 <filter>`.

### Flamegraphs

A flamegraph is required output of every benchmark run, not optional. Without it, regressions surface as "function X got slower" with no insight into the call path responsible. Pair every `cargo bench` run with a `cargo flamegraph` run using the same `<label>`, so the criterion log and the CPU profile sit side by side under `docs/`.

- **Install once:** `cargo install flamegraph` (provides the `cargo flamegraph` subcommand backed by DTrace on macOS, `perf` on Linux).
- **Generate command** (also shown inline in the workflow above):
  ```sh
  cargo flamegraph --bench performance -o docs/flamegraph-<label>.svg -- --bench --profile-time 10
  ```
  - `<label>` matches the paired criterion log (`before`, `after`, or a date / change identifier — exactly the same suffix as `bench-<label>.txt`).
  - `--profile-time 10` caps each bench at 10 seconds in profiling mode — full criterion timings are not needed for stack sampling.
  - To restrict to a subset, append a filter after `--profile-time 10` (e.g. `search/filter_cost`).
- **macOS requires sudo for DTrace.** Either prefix with `sudo` or grant the user DTrace permission. Without it, the run fails with "dtrace: failed to initialize dtrace".
- **Save under `docs/flamegraph-<label>.svg`.** The SVG is the committed artifact; do not commit raw `perf.data` / `out.stacks` files (add them to `.gitignore` if they appear at repo root).
- **Reference the flamegraph from the perf-comparison summary.** A `perf-comparison-YYYY-MM-DD.md` that mentions a regression must link the flamegraph that justifies the diagnosis.
- **Skip flamegraphs only when benchmarks are skipped** — i.e. doc-only / test-only / clearly perf-neutral changes. If you ran a bench, you generate a flamegraph.

### Interpreting results

- A change crossing ±5% with `p < 0.05` is real. "Change within noise threshold" is criterion telling you it is statistically significant but small enough to ignore for a single run.
- Always weigh per-shape trade-offs: a hot-path change can win on dense graphs and lose on chains, or vice versa. Document the trade-off explicitly rather than reporting only the wins.
- HTML reports under `target/criterion/` give per-bench distribution plots when a number is suspicious.

## Testing

When writting unit tests, integration tests and collecting evidence please read docs/testing.md
