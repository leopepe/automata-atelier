# Coding guidelines

Workspace-wide rules for any change in `grafo`, `goap-planner`, or `uncharles`.
Per-crate `CLAUDE.md` files may impose stricter rules; nothing here weakens them.

## Tests

Every behaviour change ships with tests. Do not mark work done without coverage proportional to the change.

- **New public API**: at least one happy-path test, one edge-case test, and one error-path test.
- **Bug fix**: a regression test that fails on the old code and passes on the new.
- **Refactor**: existing tests must still cover the affected paths; add tests for any newly-introduced branches.

Where tests live, by crate:

- **grafo** — unit tests in `#[cfg(test)] mod tests` inside `src/lib.rs` or `src/<module>.rs`. Doctests (`///` blocks) on every public function or type.
- **goap-planner** — integration tests in `tests/` (one file per scenario). Doctests on public types and constructors.
- **uncharles** — unit tests inline in source modules. Integration tests in `tests/` should exercise the loop with side-effect-free configs (e.g. `cmd: ["true"]`).

Aim for the highest *meaningful* coverage — every public path, every error variant, every documented behaviour. Avoid coverage theatre: tests that execute code without asserting anything useful are noise, not signal.

## Lint and format

Before considering any change finished, run all three:

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

All must pass clean. If clippy flags an existing issue you did not introduce, decide explicitly: fix it as part of the change, file it in `docs/todo.md`, or annotate with `#[allow(...)]` plus a one-line comment explaining why. Never silently `#[allow(...)]` to make a lint go away.

## Documentation

When a change alters behaviour or public interface, update the docs in the same commit. Lagging docs are worse than missing docs — they actively mislead.

After a behaviour or interface change, update each that applies:

- **Docstrings** (`///` and `//!`) on every changed public item — function summary, parameters, return value, panic conditions, error variants. Make them reflect the new behaviour exactly.
- **Module-level docs** at the top of `lib.rs` / `main.rs` if the change affects the module's role or surface.
- **Doctests** must compile and assert the new behaviour. They are part of the test suite — a broken example is a broken test.
- **Example binaries** under `examples/` that use the changed API.
- **YAML configs** under `uncharles/configs/` if the schema changed.
- **`CLAUDE.md`** files at the relevant scope if the change invalidates rules captured there.
- **`docs/todo.md`** if the change resolves a tracked item — remove or strike the entry.

For renames and removals, prefer atomic changes (rename/remove + update every caller + update every doc) over deprecation shims. The workspace is small enough that grep-and-replace is the right tool.

## When in doubt

If a change is non-trivial and the right-sized test, lint posture, or documentation update is unclear, prefer the more thorough option. The cost of an extra test or a fuller docstring is minutes; the cost of a regression or a stale doc compounds.
