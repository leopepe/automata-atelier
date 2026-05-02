# goap-planner

Goal-Oriented Action Planning over a state-space graph built with [`grafo`](../grafo/). The first planner in the [Automata Atelier](../) workspace.

Given an initial [`State`], a [`Goal`], and a library of [`Action`]s with preconditions and effects, [`Planner::plan`] expands reachable states by forward search, builds a directed graph of state transitions, and runs Dijkstra (via `grafo`) to return the cheapest action sequence that reaches the goal.

This crate has no opinion on how [`State`] is gathered — observation, shell-outs, and cloud adapters are the runtime's concern (see [`uncharles`](../uncharles/)). Callers construct [`State`] however they like (programmatically, from CLI flags, deserialised from JSON, etc.) and pass it to [`Planner::plan`].

## Features

- **Pure planning** — no I/O, no async, no shell-outs. Pure CPU work over `State` / `Action` / `Goal`.
- **Builder-style action definition** — `Action::new("chop_tree", 5.0).requires("has_axe").adds("has_log")`.
- **Bounded BFS** — `max_states` cap (default 10 000) returns a typed `PlannerError::StateSpaceLimitExceeded` rather than running forever on unbounded state spaces.
- **Cheapest-path selection** — when multiple action sequences reach the goal, the planner returns the one with the lowest total cost.
- **Thread-safe** — `Planner` is `Send + Sync`; share across threads with `Arc` for parallel plan calls.

## Installation

This crate is a workspace member, not yet published to crates.io.

```toml
# In your Cargo.toml
[dependencies]
goap-planner = { path = "../goap-planner" }
```

## Usage

```rust
use goap_planner::{Action, Goal, Planner, State};

let actions = vec![
    Action::new("chop_tree", 5.0).requires("has_axe").adds("has_log"),
    Action::new("split_log", 2.0)
        .requires("has_log")
        .adds("has_firewood")
        .removes("has_log"),
];

let initial = State::from_facts(["has_axe"]);
let goal = Goal::new().requires("has_firewood");

let plan = Planner::new(actions).plan(&initial, &goal).unwrap().unwrap();
assert_eq!(plan.steps, vec!["chop_tree", "split_log"]);
assert_eq!(plan.cost, 7.0);
```

### Examples

```bash
cargo run -p goap-planner --example deploy        # service deployment workflow
cargo run -p goap-planner --example release       # release-tagging workflow
cargo run -p goap-planner --example refactor      # multi-step code-refactor plan
cargo run -p goap-planner --example validate      # repo validation pipeline
cargo run -p goap-planner --example watch         # always-almost-satisfied loop
cargo run -p goap-planner --example dependency    # build-graph dependency resolution
```

## Performance highlights

All figures from `cargo bench -p goap-planner` (Criterion 0.5, 100 samples, release profile, macOS / Rust stable, last measured 2026-05-02).

| Scenario | Result |
|---|---|
| 5-step linear plan | **3.3 µs** |
| 50-step linear plan | **32 µs** |
| 128-action library, single correct branch | **337 µs** |
| 16 redundant paths, picks the cheapest | **23 µs** |
| Goal already satisfied (fast-path early return) | **7.2 ns** |
| `Goal::satisfied_by` (1 required fact) | **6.5 ns** |
| `State::contains` (hit / miss) | **5.1 ns / 2.6 ns** |
| `Action::applicable` (met / unmet) | **16 ns / 13 ns** |
| 64 concurrent plans via Rayon over `Arc<Planner>` | **148 µs** (5.2× faster than sequential) |

Canonical summary with full tables and trade-offs: [`docs/performance.md`](docs/performance.md). Raw bench logs accumulate as `docs/bench-<label>.txt`; per-change deltas live in dated `docs/perf-comparison-YYYY-MM-DD.md` snapshots once the suite is touched.

## Contributing

1. Read [`CLAUDE.md`](CLAUDE.md) for the per-crate rules and the workspace [`docs/performance-tests.md`](../docs/performance-tests.md) before touching the planner hot path.
2. Run the test suite before opening a pull request: `cargo test -p goap-planner`.
3. Run benchmarks if your change touches `Planner::plan`, `State`, `Action`, or `Goal`: `cargo bench -p goap-planner`. Pair with a flamegraph (see `CLAUDE.md`).
4. Keep public API changes backward-compatible or discuss them in the PR first.

## License

MIT
