---
status: proposed
date: 2026-06-18
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Actor-based reactive runtime for uncharles (kameo)

## Context and Problem Statement

`uncharles`'s execute path is a single synchronous loop (`run.rs:run_loop`):
it runs every sensor in sequence, plans once, executes one action, applies
optimistic effects, sleeps, and repeats. The loop is structurally serial
(sensors cannot observe concurrently) and goal-singular (one `Goal`, one
planner). The committed roadmap — external plugins over IPC, multiple
concurrent goals with arbitration, hot-reload of YAML at runtime — pushes in
exactly one direction: many independent components with their own lifecycles
exchanging messages. The immediate forcing requirement is **reactive
sensing**: sensors run continuously and in parallel, world-state changes
trigger replanning, and the executor runs the freshest available plan. This
ADR settles **what concurrency foundation uncharles adopts** to get there.
Refs #17 (watcher mode), refs #19 (sensor ordering as a foot-gun).

## Decision Drivers

* **Reactive, parallel sensing.** Sensors must run concurrently (real OS-thread
  parallelism when the host has cores), each shelling out on its own cadence,
  feeding a shared world-state. The serial `for sensor in &config.sensors`
  loop cannot express this, and #19 (sensor ordering foot-gun) is a direct
  symptom — sequential sensors leak ordering into observed state.
* **Replan on change, not on tick.** A world-state change must trigger the
  planner; an idle tick that observes nothing new must not. This needs a
  component that owns state and emits change events, decoupled from whatever
  consumes them.
* **Forward fit to the roadmap.** Plugins, multi-goal arbitration, and
  hot-reload each independently want "a component with a lifecycle that talks
  by messages and can be supervised/restarted." Retrofitting that topology
  later is far costlier than starting on it now.
* **`goap-planner` stays load-bearing and pure.** The sibling crate remains the
  sole planning engine. `goap_planner::{State, Goal, Action, Plan, Planner}`
  are the canonical types that flow *through* messages; no parallel
  `ActorState`/`ActorPlan` types, no planning logic in the runtime layer. The
  workspace layering rule (async/I/O only in `uncharles`) is non-negotiable.
* **Operability.** Signal handling, graceful drain (never abort mid-action),
  and restart-on-failure are production concerns the foundation must address —
  `docs/adrs.md` lists replan triggers and signal handling as ADR-worthy
  operability surface.

## Considered Options

* **Option A — Keep the serial loop, parallelise sensors with raw threads.**
  Spawn a thread per sensor writing into a `Mutex<State>`; keep `run_loop`
  otherwise intact.
* **Option B — Actor model on `kameo` (0.20).** Each subsystem (world-state,
  each sensor, planner, executor, goal-supervisor) is a kameo `Actor` with its
  own mailbox, running on tokio; supervision via actor linking.
* **Option C — Actor model on `ractor`.** Same topology, Erlang/`gen_server`
  flavoured framework with a separate cluster crate.
* **Option D — Hand-rolled tokio tasks + channels, no actor framework.** Model
  each subsystem as a `tokio::spawn` task communicating over `mpsc`/`watch`
  channels, write the supervision/restart logic ourselves.

## Decision Outcome

Chosen option: **"Option B — actor model on `kameo`"**. The actor model is the
right shape for every roadmap item (each is "independent lifecycled components
exchanging messages"), and kameo gives us that on the workspace's existing
`tokio` runtime with the least ceremony: actors are plain structs implementing
`Actor` + `Message<M>`, spawning returns a typed `ActorRef<A>`, and `tell`
(fire-and-forget) / `ask` (request-reply) cover the message patterns we need.
Lifecycle hooks (`on_start`, `on_stop`, `on_panic`, `on_link_died`) and the
default `OneForOne` `supervision_strategy` give us the supervision tree
without a hand-rolled restart loop (Option D).

This **reverses an earlier, informal (non-ADR) decision to use `ractor`**
recorded in project notes on 2026-06-12. That decision favoured ractor on
adoption breadth and release stability; it is captured below under
[Option C](#option-c--actor-model-on-ractor) and weighed honestly. The
maintainer reversed it on 2026-06-18 after re-evaluating ergonomics: kameo's
trait-per-message model maps directly onto our typed `goap-planner` payloads,
and it keeps the dependency surface lean (the `remote`/libp2p stack is an
opt-in feature, not pulled by default). No prior ADR exists to mark
`superseded`; this is the first ADR on the runtime's concurrency model.

The runtime contract this establishes:

* **One owner per piece of state.** A `WorldStateActor` is the sole owner of
  the `goap_planner::State` and the `Values` map (ADR 0003). Nothing else
  mutates them; readers `ask` for a snapshot. This is the actor model's
  state-ownership rule — *not* a revival of the old central serializer. The
  thing being retired is one component doing sense→plan→act in lockstep, not
  the existence of a state owner.
* **Sensors are independent, continuous actors.** One `SensorActor` per
  `SensorSpec`, each scheduling its own poll tick from `on_start`. Each tick
  shells out (variable-latency subprocess → async per the Async-vs-Rayon rule)
  and sends an `ApplyReading` message to the world-state. Sensors never
  observe each other; ordering (#19) stops being load-bearing.
* **Replan is edge-triggered on a state diff.** `WorldStateActor` computes
  whether an applied reading actually changed the fact set or values, and emits
  `StateChanged` only on a real delta. No-op ticks cause no planning.
* **Planning stays in `goap-planner`, off the async workers.** A
  `PlannerActor` wraps a single `Planner` (built once from the config's
  actions) and the `Goal`. On `StateChanged` it snapshots state and runs
  `planner.plan()` inside `spawn_blocking` (CPU-bound sync work must not block
  a tokio worker — coding-guidelines.md). It does **not** hand-roll
  "is the remaining plan still valid"; it just re-plans (cheap, per the
  standing guardrail). Multiple changes arriving during a plan are
  **coalesced**: a dirty flag triggers exactly one follow-up plan, never a
  queue-storm.
* **The executor runs the freshest plan and never aborts mid-action.** An
  `ExecutorActor` holds the latest `Plan`; between actions it adopts the newest
  plan received, executes one action's `cmd`, applies optimistic effects back
  to the world-state (closing the loop), and continues. An in-flight action
  always runs to completion (matches the existing `run.sh` contract).
* **Goal arbitration has a seat from day one.** A `GoalSupervisor` routes
  `StateChanged` to planner(s) and arbitrates which `NewPlan` reaches the
  executor. Today it manages one goal; multi-goal lands here later as **N
  independent `Planner::plan` calls arbitrated at this actor** — never joint
  planning inside `goap-planner`.
* **A root `AgentSupervisor`** links all of the above, applies the supervision
  strategy (restart a crashed sensor; escalate a crashed world-state), and
  translates stop signals into a graceful drain.
* **uncharles is a daemon: the run loop is perpetual by default.** Under
  `--execute` the automaton senses continuously, plans and acts toward the goal
  when the world diverges, and on reaching the goal **returns to sensing** — it
  does not exit. Goal-satisfied and no-plan are non-terminal; only an
  unrecoverable action failure, the action-cap, or a stop signal end the
  process. As a service it drains on **SIGTERM** (systemd/Docker `stop`) as well
  as SIGINT, and a signal-driven shutdown exits 0 (a clean stop, not an error).
  `--once` opts into a one-shot drive-to-goal-and-exit run for CI/scripting,
  where goal-satisfied exits 0 and an unreachable goal exits 1. This replaces
  the previous external `run.sh` `while true; sleep` polling wrapper — the loop
  now lives inside the automaton, which is where the maintainer placed it as the
  core concept of the agent.

Scope: this ADR covers the concurrency foundation, the perpetual reactive-sensing
behaviour, and the service lifecycle (perpetual default, `--once`, SIGINT/SIGTERM
drain). Plugins (IPC wire protocol), concurrent multi-goal arbitration policy,
and config hot-reload are explicitly **out of scope** here — the topology is
designed to absorb them, but each gets its own ADR when built.

### Consequences

* Good, because sensors run with true parallelism on tokio's multi-threaded
  runtime, and replanning is reactive (edge-triggered on change) rather than
  paced by a fixed loop interval.
* Good, because `goap-planner` and `grafo` are untouched — the planner's
  hot-path benchmarks keep their full signal, and the layering boundary holds.
* Good, because the supervision tree, message types, and per-subsystem
  lifecycles are exactly the seams plugins / multi-goal / hot-reload need, so
  those features extend the topology instead of fighting it.
* Good, because kameo runs on the `tokio` runtime the coding guidelines
  already mandate, and its default `remote` feature is off, so the dependency
  surface stays modest.
* Bad, because `uncharles` gains a non-trivial async dependency surface
  (`kameo` + `tokio` with `rt-multi-thread`/`process`/`time`/`signal`) and the
  binary grows. The crate is now async at its core.
* Bad, because debugging shifts from a linear loop to message-passing between
  concurrent actors — harder to trace; mitigated by structured per-actor event
  logging carried through to the existing JSON/NDJSON emitters.
* Bad, because kameo's pre-1.0 release cadence (0.18→0.20 within six months)
  means breaking upgrades are likely; pinned to `0.20` and isolated behind the
  `actors` module so a future framework swap (or the rejected ractor option)
  touches one module, not the whole runtime.
* Bad, because we accept a small risk of replan churn under rapidly flapping
  sensors; the diff-trigger + coalescing bound it, but a pathological config
  can still keep the planner busy. Acceptable for v1; a debounce interval is a
  follow-up if it bites.

### Confirmation

1. **Tests** — `cargo test -p uncharles` covers each actor in isolation
   (spawn + `ask`/`tell` with side-effect-free `true`/`false`/`echo`
   commands), an integration test reproducing the existing `three_step_config`
   reaching goal-satisfied through the actor runtime, and a reactive test
   proving a sensor flip mid-run triggers a replan that changes the executed
   action. The existing `run.rs`/`config.rs` unit tests stay green (the pure
   sensor/action helpers are reused, not rewritten).
2. **`goap-planner` benchmarks** — unchanged crate ⇒ the bench gate
   (`.github/workflows/bench.yml`) must report no movement. Any movement is a
   layering violation (planner logic leaked into the runtime) and blocks merge.
3. **Lint/format** — `cargo fmt --all`, `cargo clippy --all-targets
   --all-features -- -D warnings`, `cargo test --workspace` clean.
4. **CLI invariants** — `inspect` (pure planning) and one-shot non-execute
   `run` are unchanged; their integration tests still pass, and `--execute`'s
   JSON/NDJSON/pretty output keeps its documented shape.

## Pros and Cons of the Options

### Option A — serial loop + raw sensor threads

* Good, because minimal new dependency surface.
* Good, because the planner/executor code barely changes.
* Bad, because it solves only parallel sensing — multi-goal, plugins, and
  hot-reload still have nowhere to live, so the loop gets rewritten again soon.
* Bad, because a shared `Mutex<State>` across N sensor threads reintroduces the
  serialization point and the lock contention the actor model avoids by giving
  state a single owning task.

### Option B — actor model on kameo (chosen)

* Good, because the topology fits every roadmap item, not just sensing.
* Good, because trait-per-message (`Message<M>` with `type Reply`) maps cleanly
  onto typed `goap-planner` payloads, and `tell`/`ask` cover our patterns.
* Good, because supervision/lifecycle is provided, not hand-rolled.
* Neutral, because it commits the runtime to `tokio` — already the workspace
  runtime, so no new axis.
* Bad, because pre-1.0 churn risk; mitigated by pinning and module isolation.

### Option C — actor model on ractor

The 2026-06-12 front-runner, reversed here.

* Good, because broader real-world Cargo.toml adoption and patch-only 0.15.x
  releases (more stable than kameo's 0.18→0.20 churn).
* Good, because the `gen_server`/Erlang supervision model is battle-tested
  (documented production use at Meta) and maps onto a supervision tree.
* Bad, because its message dispatch (a single `Message` enum + `handle` match
  per actor) is a looser fit for our typed-per-message `goap-planner` payloads
  than kameo's trait-per-message; more boilerplate at every actor.
* Bad, because the maintainer judged kameo's ergonomics materially better for
  this codebase on re-evaluation — the deciding factor in the reversal.

### Option D — hand-rolled tokio tasks + channels

* Good, because zero framework dependency and full control.
* Good, because the message types would still be our own `goap-planner` types.
* Bad, because we'd reimplement supervision, restart, linking, and graceful
  shutdown — exactly the wheel kameo/ractor already provide and test.
* Bad, because every roadmap feature (plugins, multi-goal, hot-reload) would
  grow its own ad-hoc lifecycle plumbing, accreting the bespoke framework we
  chose not to depend on.

## More Information

* **Driving issues**: #17 (long-lived watcher mode) and #19 (sensor ordering as
  a foot-gun) are both subsumed by the reactive runtime — watcher behaviour is
  now the *default* (perpetual daemon), not an opt-in flag. The actor-pivot
  tracking issue is #58.
* **Composes with ADR 0003**: the runtime `Values` map keeps living in
  `uncharles`, now owned by `WorldStateActor` and travelling beside `State`
  through `ApplyReading`/snapshot messages. No change to ADR 0003's contract.
* **Reverses (informally)**: the 2026-06-12 project-note decision favouring
  `ractor`. No ADR existed for it, so there is nothing to mark `superseded`;
  this ADR is the record of record for the concurrency model.
* **Follow-up ADRs expected**: plugin IPC wire protocol (JSON-RPC over stdio
  is the current leaning), multi-goal arbitration policy, and config
  hot-reload semantics. Each is out of scope here.
* **Trigger to revisit**: a kameo breaking release that the `actors` module
  cannot absorb cheaply, or a measured replan-churn problem that the
  diff-trigger + coalescing cannot contain (would add a debounce interval).
