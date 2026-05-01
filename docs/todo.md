# TODO

Open design notes captured from session work. Each item is small and isolated;
prioritise when there's a real consumer driving the requirement.

## Open

- **Watcher mode for long-lived configs.** `uncharles` always exits at `LoopOutcome::GoalSatisfied`. Configs like `release_watch.yaml` that should keep monitoring need an external `while true` wrapper plus a goal that's "always almost-satisfied" (e.g. `idle`). If more configs converge on this pattern, consider an opt-in `--watch` flag that re-enters the sense/plan loop after goal satisfaction instead of returning, with a configurable inter-cycle delay.
- **Value-carrying facts.** Sensors and state are Boolean — a fact is either present or absent. Configs that need to track *what* (e.g. "the SHA we triggered a deploy for") use sidecar files on disk and read/write them from sensor and action shells. Workable but it leaks orchestration state out of the planner. If multiple configs need it, consider extending `State` to carry key/value facts that planner and sensor/action runtime can both read.
