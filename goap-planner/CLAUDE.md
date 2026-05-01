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
- Builder methods (`Action::new(...).requires(...).adds(...)`) return `Self`
  by value; preserve that ergonomic when extending.

## Tests

- Integration tests live in `tests/` (one file per scenario, e.g.
  `chopping.rs`).
- Unit tests for module-internal helpers live in `#[cfg(test)] mod tests`
  inside the relevant `src/<module>.rs`.
- Every public path, every error variant, every documented behaviour must
  have at least one assertion covering it.

## Performance work

The crate currently has no benchmark suite. **When the first benchmark is
introduced** (criterion, custom, or otherwise), the following rules apply
from day one:

- Add a `[[bench]]` entry to `Cargo.toml` and a `criterion` dev-dependency
  matching the version pinned in `grafo/Cargo.toml`.
- Follow the same workflow as `grafo`: baseline → change → comparison →
  written summary in `docs/perf-comparison-YYYY-MM-DD.md`.
- **Generate a flamegraph for every bench run** (see [Flamegraphs](#flamegraphs)
  below). Criterion answers *what is slow*; the flamegraph answers *where in
  the call stack the time is spent*. Both are required to reason about a
  regression — neither alone is sufficient evidence.

### Flamegraphs

Pair every benchmark run with a flamegraph SVG saved into `docs/`. This is a
hard rule, not a suggestion: a perf regression report without a flamegraph
is incomplete.

- **Install once:** `cargo install flamegraph` (provides the `cargo flamegraph`
  subcommand backed by DTrace on macOS, `perf` on Linux).
- **Generate alongside each bench run:**
  ```sh
  cargo flamegraph --bench <bench-name> -o docs/flamegraph-<label>.svg -- --bench --profile-time 10
  ```
  - `<bench-name>` is the `[[bench]]` target's `name` field.
  - `<label>` matches the criterion log it pairs with (`before`, `after`, or
    a date / change identifier).
  - `--profile-time 10` caps each bench at 10 seconds in profiling mode —
    sufficient for stack sampling without the full criterion timing budget.
  - Restrict to a subset during iteration by appending a filter after
    `--profile-time 10`.
- **macOS requires sudo for DTrace.** Either prefix with `sudo` or grant the
  user DTrace permission. Without it, the run fails with
  "dtrace: failed to initialize dtrace".
- **Save under `docs/flamegraph-<label>.svg`.** The SVG is the artifact; do
  not commit raw `perf.data` or `out.stacks` files.
- **Reference the flamegraph from the perf-comparison summary** so the
  criterion numbers and CPU profile are linked in the same document.
- **Skip flamegraphs only when benchmarks are skipped** — i.e. doc-only /
  test-only / clearly perf-neutral changes. If you ran a bench, you generate
  a flamegraph.

## Lint, format, test

Workspace-level rules apply (see `docs/CLAUDE.md`). All three must pass clean
before any change is considered finished:

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```
