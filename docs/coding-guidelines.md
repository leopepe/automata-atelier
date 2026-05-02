# Coding guidelines

Workspace-wide rules for any change in `grafo`, `goap-planner`, or `uncharles`.
Per-crate `CLAUDE.md` files may impose stricter rules; nothing here weakens them.

Read this file before writing or modifying code anywhere in the workspace. The
sibling `CLAUDE.md` in this directory is just an index — the substantive rules
live here.

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

## Performance tests for core libraries

Any crate used as a dependency by at least one other crate in this workspace ("core library") **must** ship a benchmark suite. Today that is `grafo` (consumed by `goap-planner` and indirectly by `uncharles`) and `goap-planner` (consumed by `uncharles`); the rule applies automatically to any future crate that gains a workspace-internal consumer. The full guideline — required surface (`Cargo.toml`, `benches/performance.rs`, `docs/performance.md`, `docs/bench-<label>.txt`, `docs/perf-comparison-YYYY-MM-DD.md`, `docs/flamegraph-<label>.svg`), workflow, trigger conditions, and CI enforcement — lives in [`performance-tests.md`](./performance-tests.md).

Read that file before touching a core library's hot path or adding a new core crate. Pair every benchmark run with a flamegraph; pair every change that moves the headline numbers with a `perf-comparison-YYYY-MM-DD.md` snapshot and refresh the canonical `docs/performance.md`. CI enforces these rules on PRs touching a core library — see [`performance-tests.md`](./performance-tests.md) and `.github/workflows/bench.yml`.

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

## Commits and pull requests

Every commit message and PR title follows the [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/#specification) specification. Pull requests use the template at `.github/PULL_REQUEST_TEMPLATE.md` — fill it in, don't delete sections.

### Format

```
<type>[optional scope][!]: <description>

[optional body]

[optional footer(s)]
```

- **type** (required): one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
- **scope** (optional but encouraged): the crate or area touched — e.g. `grafo`, `goap-planner`, `uncharles`, `ci`, `docs`.
- **description** (required): imperative mood, lowercase, no trailing period, ideally under 72 characters.
- **body** (optional): explain the *why*, not the *what*. Separate from the subject by one blank line. Wrap at ~72 chars.
- **footers** (optional): `BREAKING CHANGE: <impact>` for breaking changes; issue refs such as `Refs #123` or `Closes #123`.

Breaking changes are flagged two ways together: a `!` after the type/scope (`feat(grafo)!: ...`) **and** a `BREAKING CHANGE:` footer describing the impact. One without the other is not enough.

### Examples

- `feat(uncharles): add podcast download pipeline config and e2e test`
- `fix(grafo): handle empty graph in Dijkstra search`
- `docs(goap-planner): document State/Action invariants`
- `refactor(grafo)!: rename Graph::nodes to Graph::node_count`

### Pull requests

- The PR title is the future squash-merge commit subject — it must already conform; don't rely on editing the merge commit afterward.
- The PR body uses `.github/PULL_REQUEST_TEMPLATE.md`. Tick the test-plan items only after they actually pass.
- One logical change per PR. If you're tempted to write `feat: do X and refactor Y`, that's two PRs.

## When in doubt

If a change is non-trivial and the right-sized test, lint posture, or documentation update is unclear, prefer the more thorough option. The cost of an extra test or a fuller docstring is minutes; the cost of a regression or a stale doc compounds.
