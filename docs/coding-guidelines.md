# Coding guidelines

Workspace-wide rules for any change in `grafo`, `goap-planner`, or `uncharles`.
Per-crate `CLAUDE.md` files may impose stricter rules; nothing here weakens them.

Read this file before writing or modifying code anywhere in the workspace. The
sibling `CLAUDE.md` in this directory is just an index — the substantive rules
live here.

## Quick reference

Use this checklist for every change before considering it done:

- [ ] Tests added/updated proportional to the change (see [Tests](#tests)).
- [ ] User-facing interfaces have **doctests** and **integration tests** (see [Doctests](#doctests-on-user-exposed-interfaces) and [Integration tests](#integration-tests-for-user-exposed-interfaces)).
- [ ] Performance-sensitive code follows [Performance and concurrency](#performance-and-concurrency).
- [ ] I/O-bound network code is `async`; CPU-bound parallel-safe iteration uses `rayon` (see [Async vs Rayon](#async-vs-rayon-decision-rule)).
- [ ] Any benchmark, perf measurement, or perf-critical demo runs under `--release` (see [Always build `--release` for perf work](#always-build---release-for-perf-work)).
- [ ] CLI changes follow [CLI conventions](#cli-conventions-posix--kubectl).
- [ ] Docs updated in the same commit (see [Documentation](#documentation)).
- [ ] `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --workspace` all pass clean.
- [ ] Commit message and PR title follow [Conventional Commits](#commits-and-pull-requests).

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

### Doctests on user-exposed interfaces

Every **user-exposed interface** ships a docstring with at least one **doctest** that compiles and asserts behaviour. Doctests are the contract the user reads; if it doesn't run, it lies.

A "user-exposed interface" is anything reachable from outside the crate boundary or from an external operator:

- All `pub` items in a library crate (`grafo`, `goap-planner`).
- CLI subcommands, flags, and exit-code semantics in `uncharles`.
- HTTP/gRPC handlers, request/response types, and any future network surface.
- YAML/config schema types deserialised from user input.

Rules:

- Every `pub fn`, `pub struct`, `pub enum`, `pub trait`, and `pub mod` has a `///` summary line, a description if non-obvious, and a `# Examples` section with a runnable doctest.
- Document `# Errors`, `# Panics`, and `# Safety` whenever they apply. If a function can panic, say when. If it returns `Result`, list the error variants and what triggers each.
- Doctests must `assert!` / `assert_eq!` something meaningful. A doctest that only constructs a value and never asserts is documentation, not a test.
- Hide setup noise behind `#` lines so the rendered example reads naturally:

  ```/dev/null/example.rs#L1-10
  /// Adds two numbers.
  ///
  /// # Examples
  ///
  /// ```
  /// # use mycrate::add;
  /// assert_eq!(add(2, 3), 5);
  /// ```
  pub fn add(a: i32, b: i32) -> i32 { a + b }
  ```

- For CLI binaries (`uncharles`), put usage examples in the binary's `//!` module docs (`src/main.rs`) and in `--help` output (clap derive docs). Doctests in binary crates are limited; back them with [integration tests](#integration-tests-for-user-exposed-interfaces).
- For HTTP/gRPC handlers, doctest the request/response types (serde round-trips, validation) and back the wire behaviour with integration tests.

Doctests run as part of `cargo test --workspace`. A failing doctest is a failing test — fix it, don't ignore it. `#[doc(hidden)]` and `no_run` are escape hatches; justify each one with a comment.

### Integration tests for user-exposed interfaces

Every user-exposed interface ships an **integration test** that exercises the **compiled binary or compiled library entry point**, not just internal functions. Doctests prove the API shape is right; integration tests prove the artifact actually works after a real build.

Rules by surface:

- **CLI (`uncharles` and any future binary)** — integration tests under `tests/` that invoke the compiled binary via `assert_cmd` (or equivalent) and assert on **stdout**, **stderr**, and **exit code**. Cover at minimum:
  - `--help` and `--version` succeed and contain the expected sections.
  - The happy path with a side-effect-free config (e.g. `cmd: ["true"]`).
  - One representative error path (bad config, missing flag, etc.) returning a non-zero exit code with a useful stderr message.
- **Library APIs (`grafo`, `goap-planner`)** — integration tests under `tests/` that link the crate as an external consumer would (no access to private items). This catches `pub(crate)` leaks and surface-area regressions that unit tests inside the crate cannot.
- **HTTP/gRPC handlers** — spin up the real router/server in the test, drive it over the wire (loopback), and assert on status code, headers, and body. Mock external dependencies, never the transport.
- **YAML/config schemas** — round-trip representative configs from `configs/` (or a test fixture) through `serde` and assert the resulting in-memory shape.

Integration tests must run as part of `cargo test --workspace`. They may be slower than unit tests; that is acceptable. Mark genuinely slow tests (`> 1s`) with `#[ignore]` only with a comment explaining why and how to run them in CI.

For CLI specifically, see also [CLI conventions](#cli-conventions-posix--kubectl) — the integration tests are how those conventions are verified.

## Performance and concurrency

Performance is a first-class requirement, not an afterthought. The workspace exists to plan and execute actions; both the planner kernel (`goap-planner` over `grafo`) and the runtime (`uncharles`) sit on hot paths a user will feel.

Default to Rust's memory-safety + zero-cost-abstraction posture: the compiler's guarantees are what make aggressive optimisation safe here, so use them rather than working around them.

### Memory and ownership

- **Avoid unnecessary allocation.** Prefer `&str` over `String`, `&[T]` over `Vec<T>`, and iterator chains over intermediate collections. Allocate when an owned value crosses an ownership boundary or is genuinely needed; not by reflex.
- **Reuse buffers in hot loops.** A `Vec` or `String` cleared with `.clear()` keeps its capacity. A `HashMap` cleared with `.clear()` does too. Reach for this before reaching for `Rc`/`Arc`.
- **Prefer borrowing to cloning.** Every `.clone()` in a hot path needs a one-line comment justifying it (cheap `Arc` clone, required by trait, etc.) or it should be removed.
- **Use `Cow<'_, T>` when a value is *sometimes* owned.** Don't pay the allocation cost on the borrowed path.
- **Choose the right collection.** `Vec` for ordered/dense, `HashMap`/`HashSet` for keyed lookup, `BTreeMap`/`BTreeSet` for ordered keyed lookup, `SmallVec`/`tinyvec` for short-lived small collections that would otherwise heap-allocate. Profile before adding a new collection dep.
- **Reach for `unsafe` only with a written justification.** A `// SAFETY:` comment naming the invariants is mandatory. New `unsafe` in `grafo` or `goap-planner` requires a benchmark showing the safe version is materially slower.
- **Minimise dynamic dispatch on hot paths.** Prefer generics + monomorphisation (`impl Trait`, `T: Trait`) over `Box<dyn Trait>` when the call site is hot. `dyn` is fine for cold setup code, plugin boundaries, and heterogeneous collections.
- **Avoid `.unwrap()` / `.expect()` in library code.** Return `Result`; let the caller decide. In `uncharles` they're acceptable at the binary entry point with a clear message.

### Async vs Rayon decision rule

Pick the concurrency model by **what the work is bound on**, not by which framework you used last:

| Work type                                                | Use      | Why                                                                 |
| -------------------------------------------------------- | -------- | ------------------------------------------------------------------- |
| Network I/O — HTTP, gRPC, DNS, TCP, cloud SDKs           | `async`  | Latency varies wildly; tasks block on the network, not the CPU.     |
| Disk I/O with high latency variance (e.g. remote FS, S3) | `async`  | Same reasoning as network.                                          |
| Subprocess / shell exec waited on a timeout              | `async`  | Wait time is unpredictable; concurrent waits multiplex cleanly.     |
| CPU-bound iteration over an unordered collection         | `rayon`  | Embarrassingly parallel; result order doesn't matter.               |
| CPU-bound graph/planner search (`grafo`, `goap-planner`) | sync     | Already tight, latency-sensitive, single-threaded by design today.  |
| Tiny bookkeeping (< ~10 µs total)                        | sync     | Concurrency overhead exceeds the work.                              |

Rules:

- **`async` for variable-latency I/O.** All new HTTP, gRPC, and network code is `async` and runs on the workspace runtime (`tokio`). Block-on bridges (`block_on`, `spawn_blocking`) are allowed at the binary boundary or to call legacy sync code; document why at the call site.
- **No `async` in `goap-planner`.** It is a pure CPU library; keep it that way (see workspace `CLAUDE.md`'s layering rule). I/O concerns belong in `uncharles`.
- **`rayon` when ordering doesn't matter.** Iterating a list to compute or filter where the order of processing and the order of results are both irrelevant → `par_iter` / `par_iter_mut` / `into_par_iter`. Preserve sequential iteration when ordering or early-exit semantics matter.
- **Don't mix `rayon` inside an `async` task** without `spawn_blocking` or `tokio::task::block_in_place` — `rayon`'s thread pool will starve the async runtime otherwise. Document the bridge.
- **Measure before parallelising.** A `rayon` conversion that doesn't move a benchmark number is dead weight; revert it. Pair the change with a [`docs/perf-comparison-YYYY-MM-DD.md`](./performance-tests.md) entry.
- **Don't parallelise inside `grafo`/`goap-planner` without an issue.** Adding `rayon` to a core library is a design decision (changes Send/Sync requirements on user types); open a `type:design` issue first.

### Always build `--release` for perf work

Debug builds are 10×–100× slower than release builds and have *no* relation to production performance. Any time you measure, profile, or demonstrate performance, build with `--release`:

- **Benchmarks**: `cargo bench` is release by default — don't override it.
- **Flamegraphs**: `cargo flamegraph --release ...` (per [`performance-tests.md`](./performance-tests.md)).
- **Manual timing of an example or binary**: `cargo run --release --bin <name>` or `cargo run --release --example <name>`.
- **Reproducing a perf bug locally**: `--release` first; only drop to debug if you need symbol-level detail the release build doesn't expose, and even then prefer `--release` with `[profile.release] debug = true` set in `Cargo.toml`.
- **Running CI-equivalent perf checks**: match what `.github/workflows/bench.yml` does (release + the documented `$CRITERION_CI_FLAGS`).

If a measurement looks anomalously slow, the first thing to check is whether the binary was built with `--release`. Numbers from a debug build are not evidence and must not be quoted in PR descriptions or perf comparisons.

## Performance tests for core libraries

Any crate used as a dependency by at least one other crate in this workspace ("core library") **must** ship a benchmark suite. Today that is `grafo` (consumed by `goap-planner` and indirectly by `uncharles`) and `goap-planner` (consumed by `uncharles`); the rule applies automatically to any future crate that gains a workspace-internal consumer. The full guideline — required surface (`Cargo.toml`, `benches/performance.rs`, `docs/performance.md`, `docs/bench-<label>.txt`, `docs/perf-comparison-YYYY-MM-DD.md`, `docs/flamegraph-<label>.svg`), workflow, trigger conditions, and CI enforcement — lives in [`performance-tests.md`](./performance-tests.md).

Read that file before touching a core library's hot path or adding a new core crate. Pair every benchmark run with a flamegraph; pair every change that moves the headline numbers with a `perf-comparison-YYYY-MM-DD.md` snapshot and refresh the canonical `docs/performance.md`. CI enforces these rules on PRs touching a core library — see [`performance-tests.md`](./performance-tests.md) and `.github/workflows/bench.yml`.

## CLI conventions (POSIX + kubectl)

`uncharles` is the workspace's CLI today; any future binary follows the same rules. The CLI is a user-exposed interface, so [doctests](#doctests-on-user-exposed-interfaces) and [integration tests](#integration-tests-for-user-exposed-interfaces) both apply.

### POSIX baseline

Follow the [POSIX Utility Conventions](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap12.html):

- **Short flags are single dashes**: `-v`, `-h`. Short flags can stack: `-vh` ≡ `-v -h`.
- **Long flags are double dashes**: `--verbose`, `--help`. Long flags use kebab-case: `--dry-run`, not `--dryRun` or `--dry_run`.
- **`--` ends option parsing.** Anything after `--` is positional, even if it starts with `-`.
- **`-` means stdin/stdout** when used in place of a path argument.
- **Exit codes**: `0` on success, non-zero on failure. Reserve `2` for usage errors (bad flags, missing required args). Document any custom exit codes in `--help` and in the binary's module docs.
- **Errors go to stderr; data goes to stdout.** Never mix them. Piping the stdout of one invocation into another must work without filtering.
- **Respect `NO_COLOR`** (https://no-color.org/) and only emit ANSI colour to a TTY by default. Provide `--color=auto|always|never` matching coreutils.
- **`--help` and `--version` succeed with exit code `0`** and write to stdout. They never require a config file or network.

### kubectl-inspired structure

Model the command tree on `kubectl`'s `verb [resource] [name] [flags]` shape. It is the most widely-internalised CLI grammar in our domain; matching it lowers the cognitive load for operators.

- **Verb-first subcommands**: `uncharles run`, `uncharles validate`, `uncharles describe`, `uncharles plan`. Verbs are imperative and lower-case.
- **Resource as second token where it makes sense**: `uncharles get plan <name>`, `uncharles describe sensor <name>`. Singular resource names; plural is acceptable for list verbs (`uncharles get plans`).
- **Global flags before the verb or anywhere after, both should parse**: `-v`, `--config`, `--context`, `--namespace`-equivalent (if/when relevant). Use [`clap`](https://docs.rs/clap)'s derive API and put global flags on the top-level struct.
- **`--output` / `-o` for format selection**: support at least `-o yaml` and `-o json` for any subcommand that prints structured data. Default to a human-readable table or summary on a TTY.
- **`--dry-run`**: any subcommand that mutates state (executes actions, writes files, hits an API) supports `--dry-run` and prints what it would do without doing it.
- **`--config <path>` / `KUBECONFIG`-style env**: configs are discovered in this order: `--config` flag → env var (`UNCHARLES_CONFIG` or equivalent) → `./<default>.yaml` → XDG config dir. Document the order in `--help`.
- **Sensible auto-completion**: expose `uncharles completion bash|zsh|fish|powershell` (clap supports this with `clap_complete`).
- **Stable subcommand names**. Renaming a verb or resource is a breaking change — flag it with `!` and a `BREAKING CHANGE:` footer in the commit (see [Commits and pull requests](#commits-and-pull-requests)).

### What to test

The CLI's [integration tests](#integration-tests-for-user-exposed-interfaces) verify the conventions above:

- `--help`, `--version`, and each subcommand's `--help` exit `0`, write to stdout, and contain the documented sections.
- A bad flag exits `2` with a useful stderr message.
- `-o json` and `-o yaml` round-trip through `serde_json::Value` / `serde_yaml::Value` cleanly.
- `--dry-run` produces no side effects (no files written, no commands executed) — assert with a config that would be detectable if executed (e.g. `cmd: ["false"]` and assert exit code `0` under `--dry-run`).

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

- **Docstrings** (`///` and `//!`) on every changed public item — function summary, parameters, return value, panic conditions, error variants. Make them reflect the new behaviour exactly. See [Doctests on user-exposed interfaces](#doctests-on-user-exposed-interfaces) for required structure.
- **Module-level docs** at the top of `lib.rs` / `main.rs` if the change affects the module's role or surface.
- **Doctests** must compile and assert the new behaviour. They are part of the test suite — a broken example is a broken test.
- **Example binaries** under `examples/` that use the changed API.
- **YAML configs** under `uncharles/configs/` if the schema changed.
- **CLI `--help` text and `uncharles/docs/`** if a verb, flag, resource, or output format changed (see [CLI conventions](#cli-conventions-posix--kubectl)).
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

Specifically:

- **Unsure if it needs a doctest?** Add one. Public API without a runnable example is half-documented.
- **Unsure if `async` or `rayon` is right?** Re-read [Async vs Rayon](#async-vs-rayon-decision-rule) and pick by what the work is bound on. If still unsure, default to sync and open an issue.
- **Unsure if a perf change is real?** It isn't, until you've measured under `--release` with the benchmark suite and pasted the numbers in the PR.
- **Unsure if a CLI flag follows POSIX?** Check the table in [CLI conventions](#cli-conventions-posix--kubectl) and `kubectl`'s equivalent flag; match the more conservative of the two.
