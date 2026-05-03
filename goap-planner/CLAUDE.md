# goap-planner

Pure GOAP planner over `grafo`. No I/O, no opinion on how state is observed —
callers construct [`State`] however they like and pass it to [`Planner::plan`].
Side effects, shell exec, and config parsing belong in `uncharles` (or another
runtime crate), never here.

## Layering rules

- This crate must never depend on `uncharles` or any runtime concerns. The
  dependency arrow is one-way: `uncharles → goap-planner → grafo`.
- No `std::process`, `std::fs`, async runtimes, or anything that touches the
  outside world. Pure CPU work over `State` / `Action` / `Goal`.
- If a change feels like it should live in a lower layer (`grafo`), push it
  *up* to the runtime instead — see the workspace `CLAUDE.md` for the
  rationale.

## Public API rules

- `State`, `Action`, `Goal`, `Plan`, `Planner`, `PlannerError` are the public
  surface. Adding a new public type is a deliberate decision; prefer extending
  existing builders over introducing new entry points.
- Every public item carries a doctest that compiles and asserts behaviour.
  Doctests are part of the test suite — a broken example is a broken test.
- Builder methods (`Action::new(...).requires(...).forbids(...).adds(...)`)
  return `Self` by value; preserve that ergonomic when extending.
- `Action::applicable(&state)` is a conjunction over both positive
  preconditions (`requires` — every fact must be present) and negative
  preconditions (`forbids` — every fact must be absent). The library
  is loose about overlap: a fact in both `requires` and `forbidden`
  makes the action unsatisfiable but is not rejected at this layer;
  `uncharles::Config::validate` rejects it at config-load time
  instead.

## Tests

- Integration tests live in `tests/` (one file per scenario, e.g.
  `chopping.rs`).
- Unit tests for module-internal helpers live in `#[cfg(test)] mod tests`
  inside the relevant `src/<module>.rs`.
- Every public path, every error variant, every documented behaviour must
  have at least one assertion covering it.

## Performance work

This crate is a core library (consumed by `uncharles`), so the workspace's
[`docs/performance-tests.md`](../docs/performance-tests.md) rules apply:
benchmarks are required, every bench run pairs with a flamegraph, and the
canonical [`docs/performance.md`](docs/performance.md) is refreshed when
headline numbers move.

- **Bench suite:** [`benches/performance.rs`](benches/performance.rs).
  Mirrors `grafo/benches/performance.rs` in style — deterministic LCG for
  shape generation, factory functions per scenario, one `bench_*` function
  per group. Run with `cargo bench -p goap-planner`.
- **Trigger conditions:** any change to `Planner::plan`, `Action::applicable`
  / `Action::apply`, `State::signature` / `State::contains` / `State::insert`,
  `Goal::satisfied_by`, the BFS bound (`max_states`), or the `grafo`
  dependency.
- **Workflow:** identical to grafo's `Benchmark workflow` and `Flamegraphs`
  sections in [`grafo/CLAUDE.md`](../grafo/CLAUDE.md). Read those before a
  bench session — they are the canonical reference for the whole workspace.
- **CI gate:** `.github/workflows/bench.yml` runs the suite on every PR
  that touches this crate and fails on regressions beyond the configured
  threshold. See `docs/performance-tests.md` for the override procedure
  (`bench:allow-regression` label) when a regression is a deliberate
  trade-off.

### Refreshing `docs/performance.md`

After a bench run that moves any headline number, update
[`docs/performance.md`](docs/performance.md): the `Last measured` line,
the absolute numbers in `## At a glance` / `## Planning` / `## Micro-ops`
/ `## Concurrent plans`, the `## History` table entry, and (if applicable)
the `## Trade-offs we accept` section. The dated
`perf-comparison-YYYY-MM-DD.md` files are immutable history;
`performance.md` is the always-current summary linked from
`goap-planner/README.md` and the workspace `README.md`.

## Lint, format, test

Workspace-level rules apply (see [`docs/coding-guidelines.md`](../docs/coding-guidelines.md)). All three must pass clean
before any change is considered finished:

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```
