# TODO

Open design notes captured from session work. Each item is small and isolated;
prioritise when there's a real consumer driving the requirement.

## Open

- **Watcher mode for long-lived configs.** `uncharles` always exits at `LoopOutcome::GoalSatisfied`. Configs like `release_watch.yaml` and `podcast.yaml` that should keep monitoring need an external `while true` wrapper plus a goal that's "always almost-satisfied" (e.g. `idle`). Consider an opt-in `--watch` flag that re-enters the sense/plan loop after goal satisfaction instead of returning, with a configurable inter-cycle delay.
- **Value-carrying facts.** Sensors and state are Boolean — a fact is either present or absent. Three configs (`release_watch.yaml`, `merge_gate.yaml`, `podcast.yaml`) now use sidecar files on disk to carry "which thing are we working on" (commit SHA, PR number, episode GUIDs). Pattern is consistent enough across configs to commit to a design: extend `State` to carry key/value facts that planner ignores but sensor/action runtimes can read via env-var injection (e.g. `UNCHARLES_FACT_<key>=<value>`). The pressure is now strong enough that picking wrong is cheaper than waiting longer.
- **Sensor ordering as a foot-gun.** Sensors execute in YAML order, and a sensor with side effects (e.g. `new_episodes_available` populating `pending/`) must run before any sensor that reads what it produces. `podcast.yaml` documents this with a header comment but the next config that hits this will rediscover it. Consider either (a) declaring sensor ordering explicitly via dependency arrows, or (b) running side-effect-free sensors after side-effecting ones, or (c) splitting "discover" actions out of sensors entirely (sensors stay pure, an action `refresh_pending` does the side-effecting work).

## CI performance improvements (bench workflow)

Observed on PR #2: goap-planner bench job ~6m48s for a single head-only run; grafo bench job ~15 min for the full base + head comparison. Decomposing one `cargo bench -p <crate>`: ~2 min release-mode compilation (criterion + rayon + serde + grafo deps), ~4-5 min criterion sampling (32 unique bench cases × ~8s/case at default 3s warmup + 5s measurement). Sampling dominates, not compilation. The workflow itself isn't inefficient — the cost is intrinsic to running 32 cases twice with default criterion settings.

Three of the four originally-tracked items have landed: base-baseline caching (A), reduced criterion sampling (B), and the CI bench-case subset (D). See `docs/performance-tests.md` for the implementation details of each. One item remains:

- **(C) Parallelize save-baseline and comparison as separate matrix jobs (~50% reduction on cache miss).** Currently sequential within one job: save → compare → detect. Refactor to two parallel jobs (`bench-base` and `bench-head`) plus a third lightweight `detect-regression` job that downloads both artifacts. Wall time becomes `max(base, head)` instead of `base + head`. *Tradeoff:* more job boilerplate, three runner allocations, artifact-pass adds a few seconds, and `detect-regression` would have to compute the comparison itself (parsing criterion JSON or two raw bench outputs) since criterion's `--baseline` flag needs both data sets on the same machine. Largely redundant now that the base-baseline cache is in place — only matters on cache miss, which happens roughly weekly when `main` moves. Tackle only if CI wall time becomes a problem again.

## Bench gate fragility (follow-up to A/B/D)

PR #5 and PR #6 both tripped the regression gate on what later analysis showed to be runner noise rather than real regressions. Two compounding causes:

- **Cross-runner comparisons.** The base-baseline cache (A) makes the cache-hit path a comparison of "base measured on runner X" against "head measured on runner Y". GitHub-hosted runners differ by ±20-50% in micro-bench throughput from heap allocator state, neighbour load, and CPU model. Same-machine comparisons (cache miss) cancel those biases; cross-machine comparisons (the common case) surface them as "regressions" of 50-100% on small benches.
- **Drop- and warmup-dominated micro-benches.** `goap-planner`'s `ops/state/insert` (`iter_batched` includes the drop of a 100-fact `HashSet` per timed iteration; the actual `State::insert` is ~50 ns, observed time ~1-2 µs of which ~99% is drop) and `concurrent_plans/parallel_*_rayon` (Rayon thread-pool spin-up varies per runner). Reduced sampling (B) made these visibly flaky; PR #5 flagged 3 such benches, PR #6 flagged a different 4.

Possible directions, no commitment:

- Pin same-machine comparisons by moving the head bench back into the same job as the base bench so the cache-hit path always compares apples-to-apples. Reverts (A)'s parallelism on the cache-miss path but kills the bigger noise source.
- Redesign the flaky micro-benches: `iter_custom` for `state/insert` (do many inserts per timed window, amortising drop), pre-warm the Rayon pool before the first parallel bench.
- Excluded-list mechanism in the regression detector: skip specific bench IDs from the gate while keeping their numbers in the artifact for local inspection.
- Raise the threshold to 25-30% (loses sensitivity to real 10-20% regressions; least appealing).

Workaround until then: the `bench:allow-regression` label downgrades the gate to a warning. PR #5 was merged this way (admin merge before the bench check finished); PR #6 was merged this way (label applied explicitly). Using the label without a follow-up plan is how a flaky gate becomes a permanently-ignored gate — this entry exists so the plan stays visible.
