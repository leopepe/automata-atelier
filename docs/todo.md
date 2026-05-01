# TODO

Open design notes captured from session work. Each item is small and isolated;
prioritise when there's a real consumer driving the requirement.

## Open

- **Watcher mode for long-lived configs.** `uncharles` always exits at `LoopOutcome::GoalSatisfied`. Configs like `release_watch.yaml` and `podcast.yaml` that should keep monitoring need an external `while true` wrapper plus a goal that's "always almost-satisfied" (e.g. `idle`). Consider an opt-in `--watch` flag that re-enters the sense/plan loop after goal satisfaction instead of returning, with a configurable inter-cycle delay.
- **Value-carrying facts.** Sensors and state are Boolean — a fact is either present or absent. Three configs (`release_watch.yaml`, `merge_gate.yaml`, `podcast.yaml`) now use sidecar files on disk to carry "which thing are we working on" (commit SHA, PR number, episode GUIDs). Pattern is consistent enough across configs to commit to a design: extend `State` to carry key/value facts that planner ignores but sensor/action runtimes can read via env-var injection (e.g. `UNCHARLES_FACT_<key>=<value>`). The pressure is now strong enough that picking wrong is cheaper than waiting longer.
- **Sensor ordering as a foot-gun.** Sensors execute in YAML order, and a sensor with side effects (e.g. `new_episodes_available` populating `pending/`) must run before any sensor that reads what it produces. `podcast.yaml` documents this with a header comment but the next config that hits this will rediscover it. Consider either (a) declaring sensor ordering explicitly via dependency arrows, or (b) running side-effect-free sensors after side-effecting ones, or (c) splitting "discover" actions out of sensors entirely (sensors stay pure, an action `refresh_pending` does the side-effecting work).
