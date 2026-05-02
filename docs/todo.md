# TODO

Open design notes captured from session work. Each item is small and isolated;
prioritise when there's a real consumer driving the requirement.

## Open

- **Watcher mode for long-lived configs.** `uncharles` always exits at `LoopOutcome::GoalSatisfied`. Configs like `release_watch.yaml` and `podcast.yaml` that should keep monitoring need an external `while true` wrapper plus a goal that's "always almost-satisfied" (e.g. `idle`). Consider an opt-in `--watch` flag that re-enters the sense/plan loop after goal satisfaction instead of returning, with a configurable inter-cycle delay.
- **Value-carrying facts.** Sensors and state are Boolean — a fact is either present or absent. Three configs (`release_watch.yaml`, `merge_gate.yaml`, `podcast.yaml`) now use sidecar files on disk to carry "which thing are we working on" (commit SHA, PR number, episode GUIDs). Pattern is consistent enough across configs to commit to a design: extend `State` to carry key/value facts that planner ignores but sensor/action runtimes can read via env-var injection (e.g. `UNCHARLES_FACT_<key>=<value>`). The pressure is now strong enough that picking wrong is cheaper than waiting longer.
- **Sensor ordering as a foot-gun.** Sensors execute in YAML order, and a sensor with side effects (e.g. `new_episodes_available` populating `pending/`) must run before any sensor that reads what it produces. `podcast.yaml` documents this with a header comment but the next config that hits this will rediscover it. Consider either (a) declaring sensor ordering explicitly via dependency arrows, or (b) running side-effect-free sensors after side-effecting ones, or (c) splitting "discover" actions out of sensors entirely (sensors stay pure, an action `refresh_pending` does the side-effecting work).

## CI performance improvements (bench workflow)

Observed on PR #2: goap-planner bench job ~6m48s for a single head-only run; grafo bench job ~15 min for the full base + head comparison. Decomposing one `cargo bench -p <crate>`: ~2 min release-mode compilation (criterion + rayon + serde + grafo deps), ~4-5 min criterion sampling (32 unique bench cases × ~8s/case at default 3s warmup + 5s measurement). Sampling dominates, not compilation. The workflow itself isn't inefficient — the cost is intrinsic to running 32 cases twice with default criterion settings.

The two easy wins — base-baseline caching and reduced criterion sampling — have already landed (originally tracked here as items A and B; see `docs/performance-tests.md` for what each one does). Remaining items are smaller and partially redundant once the cache is in place; tackle only if CI wall time becomes a problem again.

- **(C) Parallelize save-baseline and comparison as separate matrix jobs (~50% reduction within a crate).** Currently sequential within one job: save → compare → detect. Refactor to two parallel jobs (`bench-base` and `bench-head`) plus a third lightweight `detect-regression` job that downloads both artifacts. Wall time becomes `max(base, head)` instead of `base + head`. *Tradeoff:* more job boilerplate, three runner allocations, artifact-pass adds a few seconds. Largely redundant now that the base-baseline cache is in place — only matters on cache miss.
- **(D) Reduce CI fanout in bench cases (~30% reduction).** Groups like `planning/chain/steps/{5,10,20,50}` and `planning/wide/branches/{8,32,128,512}` iterate over input sizes; for regression detection, the largest size dominates. Gate smaller sizes behind `if std::env::var("CI_BENCH_SUBSET").is_ok()` so CI runs the largest per group, local devs run the full sweep. *Tradeoff:* the per-size scaling curve isn't validated in CI, so a regression hurting only small inputs could slip through. Probably acceptable given the threshold-based gate.
