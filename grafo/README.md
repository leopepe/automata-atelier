# Grafo

A fast directed acyclic graph (DAG) library for Rust with shortest-path search and attribute-based path filtering inspired by Goal-Oriented Action Planning (GOAP).

## Features

- **CSR storage** — O(V + E) memory, cache-friendly edge traversal
- **Dijkstra search** — settled-node heap keeps memory near O(V) even on dense graphs
- **Node attributes** — attach string tags to nodes; O(1) membership checks via `HashSet`
- **Filtered search** — predicate closure acts as a per-node precondition, pruning the frontier during search
- **Cost-only variants** — skip path reconstruction for ~5× speedup on long paths
- **Thread-safe** — `Graph` is `Send + Sync`; share across threads with `Arc`
- **Parallel construction** — Rayon-backed edge resolution and sort; FxHashMap label interning

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
grafo = "0.1"
```

## Usage

### Basic shortest path

```rust
use grafo::Graph;

let graph = Graph::new(
    &["a", "b", "c", "d"],
    &[
        ("a", "b", 1.0),
        ("a", "c", 4.0),
        ("b", "c", 2.0),
        ("b", "d", 5.0),
        ("c", "d", 1.0),
    ],
)
.unwrap();

let result = graph.shortest_path("a", "d").unwrap().unwrap();
assert_eq!(result.cost, 4.0);
assert_eq!(result.path, vec!["a", "b", "c", "d"]);
```

### Attribute-based filtering

Attach tags to nodes and pass a predicate to constrain which nodes the search may visit.
Nodes failing the predicate — including source and destination — are pruned entirely.

```rust
use grafo::{Graph, NodeAttrs};

let graph = Graph::new_with_attrs(
    &[
        ("London",     &["taxi", "bus", "train"][..]),
        ("Oxford",     &["bus"][..]),             // no taxi stop
        ("Birmingham", &["taxi", "bus", "train"][..]),
        ("Manchester", &["taxi", "bus", "train"][..]),
    ],
    &[
        ("London",     "Oxford",      60.0),
        ("London",     "Birmingham", 150.0),
        ("Oxford",     "Birmingham",  45.0),
        ("Oxford",     "Manchester", 120.0),
        ("Birmingham", "Manchester",  90.0),
    ],
)
.unwrap();

// Oxford has no taxi stop — the search skips it and routes via Birmingham.
let r = graph
    .shortest_path_filtered("London", "Manchester", |attrs: &NodeAttrs| {
        attrs.contains("taxi")
    })
    .unwrap()
    .unwrap();

assert_eq!(r.path, vec!["London", "Birmingham", "Manchester"]);
assert_eq!(r.cost, 240.0);
```

### Cost-only search

When you only need the cost, use the `_cost` variants to skip path reconstruction:

```rust
let cost = graph.shortest_path_cost("a", "d").unwrap();
let cost = graph.shortest_path_filtered_cost("London", "Manchester", |a: &NodeAttrs| {
    a.contains("taxi")
}).unwrap();
```

### Concurrent queries

`Graph` is `Send + Sync`. Wrap in `Arc` to share across threads with no synchronisation overhead:

```rust
use std::sync::Arc;
use grafo::Graph;

let graph = Arc::new(Graph::new(...).unwrap());

let handles: Vec<_> = queries
    .iter()
    .map(|&(from, to)| {
        let g = Arc::clone(&graph);
        std::thread::spawn(move || g.shortest_path_cost(from, to))
    })
    .collect();
```

### Examples

```bash
cargo run --example basic                # minimal graph + path query
cargo run --example city_routes          # road network with transport-mode filtering
cargo run --example build_pipeline       # CI/CD dependency graph with error handling
cargo run --example concurrent_queries   # Arc<Graph> shared across threads with Rayon
cargo run --example goap_quest_planner   # GOAP: RPG dungeon with warrior/rogue/mage classes
cargo run --example goap_robot_delivery  # GOAP: warehouse delivery with robot capability constraints
```

## Performance highlights

All figures from `cargo bench` (Criterion, 100 samples, release profile, macOS/Rust 1.94.1).

| Scenario | Result |
|---|---|
| Sparse DAG search (10k nodes, fan=4) | **5.25 µs** |
| Selective filter (rare attribute) | **13 ns** — Dijkstra exits at first failing node |
| `shortest_path_cost` vs `shortest_path` on a 10k-hop path | **6.4× faster** |
| Attribute count scaling (1 → 20 attrs) | **flat at ~13 ns** — O(1) `HashSet` lookup |
| Construction, 100k-node sparse DAG | **7.4 ms** with Rayon + FxHashMap |
| 512 parallel queries via Rayon | **945 µs** — scales with available cores |

Full benchmark results are in [`docs/benchmarks-2026-04-20.md`](docs/benchmarks-2026-04-20.md).

## Contributing

1. Fork the repository and create a feature branch.
2. Run the test suite before opening a pull request: `cargo test`
3. Run benchmarks if your change touches search or construction: `cargo bench`
4. Keep public API changes backward-compatible or discuss them in the PR first.

## License

MIT
