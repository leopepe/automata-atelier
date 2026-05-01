# Testing — Grafo

Rules and templates for Claude Code to follow when writing tests, benchmarks, and documentation tests for this library.

---

## Layout

```text
grafo/
├── src/
│   ├── lib.rs          ← #[cfg(test)] unit tests for public re-exports
│   └── graph.rs        ← #[cfg(test)] unit tests co-located with the module
├── tests/
│   ├── common/mod.rs   ← shared fixtures and helpers
│   ├── graph_construction.rs
│   ├── shortest_path.rs
│   ├── shortest_path_filtered.rs
│   └── cost_only.rs
└── benches/
    └── performance.rs  ← Criterion benchmarks
```

Unit tests live in `#[cfg(test)] mod tests` at the bottom of their source file.
Integration tests live in `tests/` (one file per feature area).
Shared fixtures live in `tests/common/mod.rs`.

---

## Naming

Pattern: `<subject>_<condition>_<expected_outcome>` — lowercase snake_case.

```rust
fn empty_graph_builds_successfully() {}
fn duplicate_node_is_rejected() {}
fn shortest_path_picks_cheapest_route() {}
fn shortest_path_same_node_returns_zero_cost() {}
fn no_path_in_dag_returns_none() {}
fn unknown_source_node_returns_error() {}
fn filtered_by_taxi_skips_bus_only_node() {}
fn cost_only_agrees_with_full_path() {}
```

Never use: `test1`, `it_works`, `check_output`.

---

## Unit Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ── fixtures ────────────────────────────────────────────────────────────

    fn diamond() -> Graph { /* … */ }

    // ── construction ────────────────────────────────────────────────────────

    #[test]
    fn duplicate_node_is_rejected() { /* … */ }

    // ── happy paths ─────────────────────────────────────────────────────────

    #[test]
    fn shortest_path_picks_cheapest_route() { /* … */ }

    // ── error paths ─────────────────────────────────────────────────────────

    #[test]
    fn unknown_source_node_returns_error() { /* … */ }
}
```

---

## Coverage Requirements

**For every `pub fn`:**
- Happy path with typical valid inputs
- Boundary values (empty collection, single element, two elements)
- Every `Err(…)` variant the function can return
- Every `None` the function can return
- Every predicate branch (filter closures, attribute lookups)

**For every `pub enum` variant:**
- At least one test that constructs the variant (directly or via an operation)
- At least one test that pattern-matches the variant
- `Display` output verified for error variants

**For `PathResult`:**
- `cost` field is correct
- `path` field contains the exact ordered sequence of node labels
- `PartialEq` is exercised (equal results compare equal; different results compare unequal)

---

## Fixtures

Fixtures are private functions inside `#[cfg(test)] mod tests` or in `tests/common/mod.rs`.

Rules:
- Name descriptively: `diamond()`, `city()`, `linear_chain(n)`
- Document non-trivial topologies with an ASCII diagram
- Keep minimal — only nodes and edges the consuming tests actually need
- `.unwrap()` is allowed; a panic in a fixture means "bad test data", not "broken library"

```rust
/// Diamond-shaped DAG.
///
/// ```text
///        1       2       1
///   a ──────► b ──────► c ──────► d
///   │                             ▲
///   └──────────────── 4 ──────────┘
///              b ──── 5 ──────────► d
/// ```
fn diamond() -> Graph {
    Graph::new(
        &["a", "b", "c", "d"],
        &[
            ("a", "b", 1.0),
            ("a", "c", 4.0),
            ("b", "c", 2.0),
            ("b", "d", 5.0),
            ("c", "d", 1.0),
        ],
    )
    .unwrap()
}
```

---

## Assertions

**Error variants** — use `matches!` with a guard to verify the carried value:

```rust
let err = Graph::new(&["x", "x"], &[]).unwrap_err();
assert!(matches!(err, GraphError::DuplicateNode(ref n) if n == "x"));
```

**Display output:**

```rust
assert_eq!(err.to_string(), "duplicate node: 'x'");
```

**`PathResult`:**

```rust
let r = graph.shortest_path("a", "d").unwrap().unwrap();
assert_eq!(r.cost, 4.0);
assert_eq!(r.path, vec!["a", "b", "c", "d"]);
```

**`None` results:**

```rust
assert!(graph.shortest_path("d", "a").unwrap().is_none());
```

**Floating-point:** integer weights (`1.0`, `2.0`, …) may use `==`. For computed or fractional weights use an epsilon comparison:

```rust
assert!((result.cost - 3.14).abs() < 1e-10, "cost mismatch: {}", result.cost);
```

---

## Integration Tests

Every integration test file must begin with:

```rust
mod common;
use grafo::{Graph, GraphError, PathResult};
```

Follow **Given-When-Then (GWT)**:
- One `When` per test — a test with two `When` blocks is two tests
- No assertions inside `Given`
- Capture the result in `When`, assert everything in `Then`

```rust
#[test]
fn shortest_path_selects_minimum_cost_route_over_competing_alternatives() {
    // Given — diamond graph where a→b→c→d (cost 4) beats a→b→d (cost 6)
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
    .expect("fixture must build successfully");

    // When — querying the shortest path from source to destination
    let result = graph
        .shortest_path("a", "d")
        .expect("search must not error on valid nodes");

    // Then — cheapest path and its exact cost are returned
    let r = result.expect("a path must exist between a and d");
    assert_eq!(r.cost, 4.0);
    assert_eq!(r.path, vec!["a", "b", "c", "d"]);
}
```

---

## Benchmark Tests

File: `benches/performance.rs`. Framework: Criterion.

Rules:
- Never assert on timing — Criterion manages statistical analysis
- Always wrap inputs in `black_box` to prevent compiler optimisation
- Name groups hierarchically: `construction/chain`, `search/no_filter`, `search/filter_cost`
- Seed all pseudo-random generators with a fixed constant — never `rand::thread_rng()`
- Document what each benchmark stresses and the expected complexity class

```rust
/// Measures Dijkstra search time on a linear chain.
/// Expected complexity: O(V) — one neighbour per node, minimal heap churn.
/// Stresses: path reconstruction (path length == V).
fn bench_search_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("search/chain");
    for &n in &[1_000usize, 10_000, 100_000] {
        let g = chain(n);
        let dst = (n - 1).to_string();
        group.bench_with_input(BenchmarkId::new("n", n), &n, |b, _| {
            b.iter(|| g.shortest_path(black_box("0"), black_box(&dst)).unwrap())
        });
    }
    group.finish();
}
```

---

## Coverage

Tool: `cargo-llvm-cov`.

| Scope                    | Target  |
|--------------------------|---------|
| Line coverage            | ≥ 90 %  |
| Branch coverage          | ≥ 80 %  |
| Public API functions     | 100 %   |
| Error variant reachable  | 100 %   |

```sh
cargo llvm-cov                                             # terminal summary
cargo llvm-cov --html && open target/llvm-cov/html/index.html
cargo llvm-cov --fail-under-lines 90                       # CI gate
```

Use `// llvm-cov: exclude_start` / `// llvm-cov: exclude_end` only for genuinely unreachable invariants. Prefer restructuring code so it is testable.

---

## Commands

```sh
# Tests
cargo test                                      # unit + integration + doctests
cargo test --lib                                # unit tests only
cargo test --test shortest_path                 # one integration test file
cargo test shortest_path_picks_cheapest_route   # one test by name (substring match)
cargo test -- --nocapture                       # show stdout
cargo test --doc                                # doctests only
cargo test --benches                            # compile + run benchmarks as correctness tests

# Benchmarks
cargo bench                                     # full suite + HTML report in target/criterion/
cargo bench -- search                           # groups matching "search"
cargo bench -- --save-baseline main             # save named baseline
cargo bench -- --baseline main                  # compare against saved baseline

# Quality gates
cargo fmt --check
cargo clippy -- -D warnings
cargo llvm-cov --fail-under-lines 90
```

---

## Anti-patterns

| Anti-pattern | Preferred alternative |
|---|---|
| `assert!(result.is_ok())` | `.unwrap()` or `assert!(matches!(…))` |
| Testing private functions directly | Test through the public API |
| Multiple `When` blocks in one test | One test per behaviour |
| `#[ignore]` without a tracking issue | Fix, delete, or link to a tracking issue |
| Relying on test execution order | Use per-test fixtures |
| `thread::sleep` to avoid race conditions | Proper synchronisation primitives |
| Asserting on `Debug` output strings | Assert on typed fields and values |