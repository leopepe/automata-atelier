# Performance Tests

Workspace-wide rules for benchmark suites. Applies to every crate in this
workspace; per-crate `CLAUDE.md` files may add stricter rules but cannot
weaken these.

## When a crate must have a benchmark suite

A crate **must** ship a benchmark suite if **any** of the following holds:

- It is a library used as a dependency by at least one other crate in this
  workspace (a "core library"). Today: `grafo`, `goap-planner`. As soon as a
  new crate is added that another crate depends on, this rule applies to it.
- It owns a hot path that other crates explicitly delegate to (e.g. a custom
  hash table, a memory pool, a parser). The presence of internal callers
  alone is sufficient — public exposure is not required.

A crate **may** add a benchmark suite if it has a measurable hot path even
without downstream callers (e.g. `uncharles`'s sense → plan → execute loop
once it has a real workload). When in doubt, add the suite.

A crate is **exempt** if it is purely glue (binary entry points, configuration
parsing, integration tests with no algorithmic work). Exemption is a
deliberate decision; if in doubt, do not exempt.

## Required benchmark surface

Every benchmark suite ships at minimum:

- **`Cargo.toml`** — `[dev-dependencies]` includes `criterion = { version = "0.5", features = ["html_reports"] }` (the workspace-pinned version) and a `[[bench]]` entry with `harness = false`. If the suite parallelises with Rayon, add `rayon = "1"` to dev-deps as well.
- **`benches/performance.rs`** — Criterion harness. Match the canonical style established in [`grafo/benches/performance.rs`](../grafo/benches/performance.rs):
  - Top-level doc comment with the canonical invocation forms.
  - Deterministic LCG helper for shape generation (no external RNG dep).
  - Factory functions per scenario, with doc comments explaining what each one stresses.
  - One `bench_*` function per group, separated by section-header comments.
  - `criterion_group!` and `criterion_main!` at the bottom.
- **`docs/performance.md`** — canonical "current state" summary. Always reflects the latest measured numbers. Linked from the crate's `README.md` and the workspace `README.md`.
- **`docs/bench-<label>.txt`** — raw criterion logs, one per saved baseline. Never deleted; they accumulate as historical evidence.
- **`docs/perf-comparison-YYYY-MM-DD.md`** — immutable snapshots written when a change moves the headline numbers. Lists wins, regressions, and trade-offs, with links to flamegraphs.
- **`docs/flamegraph-<label>.svg`** — pair with every saved bench run. See the per-crate `CLAUDE.md` for the exact `cargo flamegraph` invocation.

## Required workflow

Pair every benchmark run with a flamegraph capture. Pair every set of perf
artifacts with a written summary. The full procedure is in
[`grafo/CLAUDE.md`](../grafo/CLAUDE.md) under **Benchmark workflow** and
**Flamegraphs**, and is the canonical reference for every crate in this
workspace.

Two artifacts per run, no exceptions:

1. `docs/bench-<label>.txt` — raw criterion log
2. `docs/flamegraph-<label>.svg` — CPU profile for the same workload

When the absolute numbers in any headline table change, refresh
`docs/performance.md`. When you write up a change, file the comparison as
`docs/perf-comparison-YYYY-MM-DD.md` and link the flamegraphs you captured.

## Trigger conditions

Run the full benchmark suite **before and after** any change that could
affect runtime performance, so a numerical comparison exists. The "after"
run alone is not evidence. Each crate's `CLAUDE.md` lists the specific
triggers for that crate (which modules / APIs / dependencies are sensitive);
the workspace-level rule is simpler:

- Any change to a core library's hot path → benchmarks required.
- Doc-only / test-only / clearly perf-neutral changes → benchmarks skipped.
- Dependency upgrades on a core library (especially hashing, allocation, or
  threading deps) → benchmarks required.

## CI enforcement

Benchmarks run in CI on every pull request that touches a core library
(`grafo`, `goap-planner`, or any future core crate). The workflow lives
at [`.github/workflows/bench.yml`](../.github/workflows/bench.yml) and
fans out one job per core crate via a matrix. Each job:

1. Checks out the PR's base branch and runs the full bench suite, saving the result with `--save-baseline pr-base`.
2. Switches to the PR head and runs the same suite with `--baseline pr-base`.
3. Pipes the comparison output through [`.github/scripts/detect-bench-regression.py`](../.github/scripts/detect-bench-regression.py), which fails the workflow if any benchmark's CI **lower bound** exceeds `REGRESSION_THRESHOLD_PCT` (default 10%) **and** criterion classified the change as "Performance has regressed".

The script uses the *lower* bound of criterion's 95% confidence interval
deliberately — the "robust regression" gate. A benchmark only counts as
a regression when even the optimistic interpretation of the data crosses
the threshold. This trades sensitivity for false-positive rate: shared
GitHub-hosted runners drift by tens of percent under neighbour load, so
a gate keyed on the upper or median CI bound triggers on noise. Real
regressions present as tight intervals well above the threshold and
clear the lower-bound check easily; runner-noise events present as wide
intervals where only the upper bound looks scary, and the lower-bound
gate ignores them.

The criterion+threshold combination is two filters layered: criterion's
own statistical test (`p < 0.05` plus its noise floor) eliminates random
fluctuations, and the percent-bound check eliminates true-but-tiny
changes that happen to clear significance on an unusually quiet run.

Lockfile drift is removed as a noise source by committing `Cargo.lock`
at the workspace root — both the baseline and PR-head bench runs resolve
identical dependency versions instead of regenerating the lockfile when
a PR adds dev-deps to one workspace member.

### Base-branch baseline cache

The base-branch bench result is the same for every PR opened against the
same `main` SHA, but a naive workflow re-runs it every time. The bench
workflow caches that result keyed by `bench-base-v3-<crate>-<base-sha>`,
storing `target/criterion/` (where criterion's `pr-base` baseline data
lives) plus the raw `bench-base.txt` log. On cache hit the base-bench
step is skipped entirely; on miss it runs as before and the result is
saved for the next PR. The cache invalidates naturally whenever `main`
moves — the SHA is part of the key — so a stale baseline can never
silently shadow a base-branch change.

The first PR after a `main` push pays the full base-bench cost; every
subsequent PR against the same `main` SHA pays close to zero for that
half of the workflow. Caches expire after 7 days of no access (GitHub's
default), so a cold start happens roughly weekly on quiet branches.

The `v3` segment in the cache key is a profile version: bump it
whenever the CI sampling settings or bench-case set changes so cached
baselines captured under the old profile cannot mix with head runs
under the new one.

### CI sampling profile

Criterion's defaults — 3s warm-up + 5s measurement × 100 samples per
case — target nanosecond-precision research. CI does not need that
precision: the gate fires on regressions above
`REGRESSION_THRESHOLD_PCT` (10%), well above the noise floor of any
sensible reduced sample.

The bench workflow passes `--measurement-time 2 --warm-up-time 1
--sample-size 50` (exposed as `$CRITERION_CI_FLAGS` in `bench.yml`) to
every `cargo bench` invocation it runs — base, head, and the
introductory bootstrap path. Per-case sampling drops from ~8s to ~3s,
which roughly halves the head-run wall time. Confidence intervals
widen modestly; both the criterion-level "performance has regressed"
classification and the percent-bound gate continue to flag genuine
regressions because real regressions present as tight intervals well
clear of the threshold.

These flags only apply inside the workflow. Local `cargo bench` runs
under each crate use criterion defaults — the higher-precision profile
is the right one for capturing canonical baselines and writing
comparison summaries.

### CI bench-case subset

Multi-size sweeps in each crate's `benches/performance.rs`
(`planning/chain/steps/{5,10,20,50}`,
`construction/chain/{1k…100k}`, etc.) iterate over input sizes so the
per-size scaling curve is visible during local development. For
regression detection the largest input dominates: a code change that
hurts performance shows up at the largest size first, and any
regression that only affects small inputs is below the threshold the
gate cares about.

Both bench files include a small `ci_sample_sizes()` helper. When
`CI_BENCH_SUBSET` is set in the environment it returns only the last
(largest) entry of any size slice; otherwise it returns the slice
unchanged. The bench workflow exports `CI_BENCH_SUBSET=1` at the job
level so every base, head, and bootstrap run takes the subset path.
Local `cargo bench` (env unset) keeps the full sweep.

Cuts roughly 30% of bench cases overall. The trade-off is that the
per-size scaling curve isn't exercised in CI — a regression hurting
only small inputs could land. Acceptable for a threshold-based gate;
local profiling and the canonical bench artifacts under `docs/` are
the right tools for size-curve analysis, not the CI gate.

The cache key carries this profile too: bumping it (`bench-base-v2-…`
→ `bench-base-v3-…` in this change) prevents a full-sweep baseline
captured under the old profile from mixing with subset head runs.

### Overriding the gate for an intentional trade-off

A change can be a real regression and still be the right move (see grafo's
chain-shape regressions in exchange for filtered-search wins). When that
happens, add the `bench:allow-regression` label to the PR. The workflow
downgrades the failure to a warning and the merge can proceed.

Using the label is a deliberate act with required follow-through:

1. The PR description must explain *why* the regression is acceptable.
2. The corresponding `perf-comparison-YYYY-MM-DD.md` must list the regressed bench(es) and the trade-off rationale.
3. `docs/performance.md` must be updated so the new floor is the documented baseline.

Lowering the threshold permanently in `bench.yml` is a separate, more
serious decision: it affects every future PR. Don't do it casually, and
never to mute a regression you intend to investigate later.

## What to do when a regression is real and intentional

A change can be a measurable regression and still be the right move (a
trade-off — see grafo's chain-shape regressions in exchange for filtered
search wins). When that happens:

1. Document the trade-off explicitly in the PR description and in
   `perf-comparison-YYYY-MM-DD.md`.
2. Update the affected crate's `docs/performance.md` "Trade-offs we accept"
   section so future readers see the decision in context.
3. Adjust the CI threshold for the specific bench(es) only if the regression
   is genuinely accepted as the new floor — never to mute a regression you
   intend to investigate later.

Lowering a threshold is a one-way ratchet you have to justify; do not do
it casually.
