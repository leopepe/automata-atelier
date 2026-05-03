---
status: accepted
date: 2026-05-03
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Value-carrying facts live in the uncharles runtime, not in goap-planner's State

## Context and Problem Statement

`goap-planner`'s `State` is Boolean: a fact is either present or absent. Three
real configs (`release_watch.yaml`, `merge_gate.yaml`, `podcast.yaml`) and the
`tofu_drift_watch.yaml` watcher landed alongside ADR 0002 all reach for the
same workaround — write a string ("commit SHA", "PR number", "episode GUID",
"three-valued plan exit code") to `.uncharles/state/<thing>` and round-trip it
through `cat` / `test` from action commands and downstream sensors. The
pattern is consistent enough to commit to a first-class abstraction. The
question this ADR settles is **where the values live and what surface they
expose**, given two distinct interpretations of the recommendation in #18:
extending `goap-planner`'s `State` to carry values alongside facts (option a
as worded), or keeping the planner Boolean and routing values entirely
through the `uncharles` runtime. Refs #18, refs ADR 0002.

## Decision Drivers

* **Hot-path sensitivity in `goap-planner`.** `State::signature`,
  `State::contains`, `Action::applicable`, and the BFS exploration loop are
  called millions of times during planning. Any extra field on `State` —
  even one the planner explicitly ignores — adds clone cost, hash cost, and
  cache pressure. The crate's own `CLAUDE.md` lists those exact methods as
  bench-trigger conditions.
* **Layering.** The workspace `CLAUDE.md` is explicit: `goap-planner` is
  pure CPU work over `State` / `Action` / `Goal`. I/O, side effects, and
  *anything stdout-shaped* belong in `uncharles`. Capturing a sensor's
  stdout into a value is exactly the kind of thing that should not push
  down into the planner.
* **Minimum new public surface.** Every field added to a public type in
  `goap-planner` is a back-compat commitment. If values can flow through
  `uncharles` end-to-end without `goap-planner` ever seeing them, the
  planner's API stays unchanged and we avoid creating a deprecation surface
  later.
* **Use cases all live in the runtime anyway.** The three sidecar-file
  workarounds in existing configs all flow value → action command (via
  shell). They never need the planner to *reason* about the value (the
  recommendation in #18 is explicit: planner ignores values during BFS).
  If the planner doesn't read the value, there's no reason to put the
  value in the planner.
* **Forward compatibility with the watcher's three-valued plan exit.**
  ADR 0002's drift-watcher uses a marker file plus three Boolean sensors
  (`plan_clean` / `plan_has_changes` / `plan_errored`) to encode one
  three-valued integer. A value-carrying-fact mechanism that lives in
  `uncharles` retires that workaround directly, with no `goap-planner`
  change required.

## Considered Options

* **Option A — Extend `goap-planner::State` to carry values the planner
  ignores during BFS.** The `Hash`/`PartialEq` impls treat `State` as
  Boolean; an additional `BTreeMap<String, String>` field carries values.
  `uncharles` reads/writes that field and injects values into action
  commands as env vars.
* **Option B — Values live entirely in the `uncharles` runtime, alongside
  `State` but never inside it.** The planner stays untouched. `uncharles`
  carries a `Values` map (`BTreeMap<String, String>`) through the
  sense → plan → execute loop, populated by sensor stdout capture
  (`capture: stdout` on `SensorSpec`) and consumed by env-var injection
  (`UNCHARLES_FACT_<NAME>=<value>`) when invoking action commands.
* **Option C — Keep state Boolean and standardise the sidecar-file pattern
  in `uncharles` as a documented convention** (with a small library helper
  for read/write). No runtime data structure; the disk is the source of
  truth.
* **Option D — Promote sensors and actions to a templating / parameter
  system**, where each sensor produces a fact-with-value and actions consume
  by name through a templating layer.

## Decision Outcome

Chosen option: **"Option B — values live in the `uncharles` runtime, not in
`goap-planner::State`"**, because it satisfies every concrete use case
without changing the planner's hot-path footprint by a single byte. The
planner stays Boolean; the runtime grows one `BTreeMap<String, String>` and
two narrow extension points (a `capture` field on `SensorSpec`, env-var
injection in `execute_action`). The mechanism is invisible to anyone who
doesn't opt in: existing configs continue to work bit-for-bit, and
benchmarks should report no movement on `goap-planner`'s headline numbers.

The runtime contract is:

* **Capture.** A sensor opts into value capture by setting `capture: stdout`
  in YAML (the only variant in v1; `stderr` and structured forms are future
  work). When the sensor's command exits successfully, its trimmed stdout is
  stored in the runtime's `Values` map under the sensor's `name`. The
  `Hash`/`PartialEq` of `goap-planner::State` is *not* affected — the
  Boolean fact `<sensor.name>` is added to `State` as before.
* **Lifetime within a cycle.** Values live for the duration of one
  `uncharles` invocation. They are populated by sensors at each iteration's
  top and consumed by action commands during the same iteration. A
  successful sensor that re-runs in a later iteration overwrites its prior
  value. A sensor that fails (and whose default failure effect removes the
  fact) drops the value.
* **Atomic remove.** When an action's `removes` list (or a sensor's
  `on_failure.remove` list) clears a fact, the corresponding value is
  dropped from `Values` in the same operation. Fact and value are one unit;
  there is no "removed-fact-but-kept-value" state.
* **Env-var injection.** When `uncharles` invokes an action command, every
  fact in the action's `requires` list that has a matching value in
  `Values` is exported as `UNCHARLES_FACT_<UPPER_SNAKE_CASE_NAME>=<value>`
  on the child process's environment. Facts without values produce no env
  var (not an empty one). `forbids` facts are by definition absent and so
  never have values to inject.
* **Cardinality.** Single-value per fact. Lists are not modelled — scripts
  that need a list use newline-delimited stdout.
* **Persistence across cycles.** Out of scope for this ADR. v1 lives within
  one invocation. The eventual `--watch` mode (#17) will need a decision on
  cross-cycle persistence; that is a separate ADR keyed off the same
  abstraction.

### Consequences

* Good, because `goap-planner` is unchanged — no clone, hash, or cache cost
  added to the BFS hot path. The bench gate stays meaningful.
* Good, because the YAML schema gains a single optional field on
  `SensorSpec` (`capture: stdout`). Existing configs are bit-for-bit
  compatible; nothing is breaking.
* Good, because the `tofu_drift_watch.yaml` config's wrapper-script trick
  (write three-valued exit to a marker file, three Boolean sensors decode
  it) becomes a one-line `capture: stdout` annotation in a follow-up PR.
  ADR 0002's headline "Bad, because" point on shell-in-config retires.
* Good, because env-var injection is the lowest-friction way to surface
  values to shell commands — every existing action `cmd` in every existing
  config can read it without touching argv parsing or templating.
* Bad, because the planner cannot reason about values. A future feature
  request for "plan based on the *value* of a fact" (e.g. "this action only
  applies when `target_sha` matches `current_sha`") is not expressible in
  this design. Such a feature would need a new ADR and almost certainly an
  Option-A-shaped change to `goap-planner`.
* Bad, because values are runtime-only and don't survive across
  invocations. A watcher that probes "did the value change since last
  cycle?" must either persist its own marker (the existing pattern stays
  available) or wait for cross-cycle persistence (out of scope).
* Bad, because the env-var name mapping (`my-fact.name` →
  `UNCHARLES_FACT_MY_FACT_NAME`) is lossy: two distinct fact names that
  collapse to the same env-var name would clash. We accept this and
  document the mapping; configs that hit the collision are doing
  something exotic enough that a YAML-load-time error is the right
  remediation if it ever bites.

### Confirmation

Compliance is verified by three gates:

1. **`goap-planner` benchmarks** — `cargo bench -p goap-planner --bench
   performance -- --save-baseline pre-issue-18` and the same with
   `--baseline pre-issue-18` after the change. The `pre-` and `post-`
   numbers must agree within criterion's noise band; the CI gate
   (`.github/workflows/bench.yml`, 10 % robust-regression threshold) is
   the long-term enforcement.
2. **Unit and integration tests** — `cargo test -p uncharles` covers
   `SensorSpec.capture` parsing (presence, absence, unknown variant
   rejected), the `Values` lifecycle (sensor success populates, action
   `removes` clears, sensor failure clears), env-var injection
   (`UNCHARLES_FACT_<NAME>` set for every value-bearing `requires` fact;
   absent facts → no var), and end-to-end flow through `run_loop`.
3. **`uncharles inspect` static analysis** — the new `capture` field is
   ignored by inspect's BFS (which is goap-planner-shaped); existing
   configs still emit identical inspection reports. Verified by re-running
   inspect on every checked-in config and diffing the JSON output.

## Pros and Cons of the Options

### Option A — extend `State` with a values field

* Good, because values are co-located with facts in the planner type.
* Good, because if a future feature does want value-aware planning, the
  value is already there.
* Bad, because every state clone in BFS pays the (cold but real) cost of
  cloning a `BTreeMap` even if it's empty. State allocation is in the
  hottest part of the planner.
* Bad, because the planner's public API gains a field that exists only for
  another crate's use case. That's leaky layering, and the workspace
  CLAUDE.md is explicit that runtime concerns belong in `uncharles`.
* Bad, because the bench gate becomes harder to interpret: a no-op-feature
  PR could move headline numbers a few percent purely from cache layout
  changes, and disambiguating "real regression" from "Option A overhead"
  consumes review time forever.

### Option B — values live in `uncharles` (chosen)

* Good, because `goap-planner` is bit-for-bit unchanged. The bench gate
  keeps its full signal.
* Good, because the surface is small and orthogonal — opt-in YAML field,
  one runtime data structure, one env-var convention.
* Good, because every existing action `cmd` in every existing config can
  consume values without modification.
* Neutral, because values cannot influence planning. This is a deliberate
  scope cut: every observed use case is "value flows to action command",
  not "planner branches on value".
* Bad, because if value-aware planning ever does become a requirement, this
  ADR will need to be superseded and the runtime values map migrated into
  `goap-planner::State`. That migration is mechanical (move a field, update
  call sites) but non-trivial.

### Option C — sidecar-file convention

* Good, because zero code change.
* Bad, because every config keeps reinventing the read/write idiom; the
  abstraction tax is paid forever rather than once.
* Bad, because the disk is the synchronisation point, which means every
  read is a `cat` subshell and every write is a `tee`. Performance is
  fine; cognitive overhead is not.
* Bad, because there is no machine-readable surface for tooling
  (`uncharles inspect`, future debugging UIs) to discover what values
  exist.

### Option D — templating

* Good, because it scales to more complex parameterisation than env vars.
* Bad, because it is a much larger surface than this problem needs. Three
  configs that want to pass a SHA to a shell command do not justify a
  templating system.
* Bad, because templating in a sense → plan → act loop tangles with
  replan semantics in ways that are hard to reason about (when does the
  template re-evaluate?). Env vars are a clean blast radius: each action
  invocation gets a fresh environment.

## More Information

* **Driving issue**: #18 (status `accepted`). This ADR closes the design
  half; the implementing PR closes the issue itself.
* **Composes with ADR 0002**: the drift-watcher's `plan_clean` /
  `plan_has_changes` / `plan_errored` workaround retires once a follow-up
  config edit replaces it with a single `plan_result` fact carrying the
  exit code as its value. Tracked as part of the ADR 0002 follow-up gaps.
* **Composes with #17 (`--watch` mode)**: cross-cycle persistence of
  values is deliberately out of scope for v1. The `--watch` ADR will need
  to settle "does a value survive between watch ticks?" as part of its
  loop semantics.
* **Trigger to revisit**: the first concrete request for **value-aware
  planning** ("the planner should treat states differing only in a value
  as distinct" or "this action only applies when fact `X` has value
  matching pattern `Y`"). At that point, this ADR is superseded and
  `State` grows the values field. Until that request appears, the
  layering decision stays where it is.
