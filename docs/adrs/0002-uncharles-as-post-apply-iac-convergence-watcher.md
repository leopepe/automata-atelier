---
status: accepted
date: 2026-05-03
decision-makers: ["@leopepe"]
consulted: []
informed: []
---

# Use uncharles as a post-apply IaC convergence watcher, not a CI replacement

## Context and Problem Statement

`uncharles` is shaped like the loop "did our last push to `main` actually
converge the world, and is the world still converged now?" — sense, plan,
act, replan on divergence. Empirical sampling of two weeks of failed runs in
the `Infrastructure Pipeline` workflow at `hyground-ai/hyground` showed that
this loop has a clear home: **between** CI pushes, scoped to one
state key, asking `tofu plan -refresh-only` whether the deployed world still
matches `main`. That is signal CI does not produce. The decision to make is
which of three plausible roles `uncharles` plays in the IaC lifecycle, and
how its config schema is shaped to support it. Refs #25.

## Decision Drivers

* **Complementarity over replacement.** CI already runs `plan` and `apply`
  reliably. Re-implementing that loop inside `uncharles` adds risk without
  producing a new signal, and forks a working code path.
* **Blast radius.** A watcher reads state. A planner-applier writes state.
  These have very different security postures (readonly vs. apply-capable
  credentials, lock acquisition behaviour, recovery semantics on partial
  failure). The first version must commit to one posture so reviewers can
  reason about it.
* **Coverage of real failure modes.** Of seven empirical failure categories
  observed in the upstream workflow (lock-file vs. version-constraint drift,
  state-schema drift, self-induced rug-pull, stale state lease, concurrent
  runs across state keys, project-specific pre-fetch, cross-run drift), the
  existing `release_watch.yaml` vocabulary covers roughly half. The chosen
  role must make the remaining gaps explicit follow-up work, not silently
  paper over them.
* **Minimal surface to start.** A config the planner can statically analyse
  with `uncharles inspect` and that exercises the runtime as it stands today
  (no new YAML schema, no new exit-code semantics) lets us land the pattern
  before any runtime change is required. Runtime gaps then have a concrete
  example to point at when they're proposed individually.

## Considered Options

* **Option A — Replace the workflow.** Run `uncharles` in place of the CI
  workflow's `plan` / `apply` jobs.
* **Option B — Mirror the workflow as an `uncharles` config.** One config (or
  one per cloud × tier) that runs `plan` → `apply` → `validate` outside CI.
* **Option C — Post-apply convergence watcher.** `uncharles` runs *between*
  pushes, scoped to one state key, asking "does `tofu plan -refresh-only`
  still report no changes?" The watcher reads state with readonly credentials
  and never applies.

## Decision Outcome

Chosen option: **"Option C — post-apply convergence watcher"**. It produces a
signal CI does not produce (cross-run drift, silent state-schema decay,
artifacts referenced by `tofu plan` going stale), it composes with the
existing CI pipeline rather than competing with it, and its readonly-creds
posture is small enough to reason about in a single review.

The example config landed alongside this ADR
([`uncharles/configs/tofu_drift_watch.yaml`](../../uncharles/configs/tofu_drift_watch.yaml))
encodes the minimum viable shape: senses `push_completed`,
`lock_file_in_sync`, `prefetch_done`, and the three-valued plan result
(`plan_clean` / `plan_has_changes` / `plan_errored`); acts via `tofu_init`,
`prefetch_artifacts`, `tofu_plan_refresh`, and three `archive_*` actions
that surface the result and exit. The watcher does **not** apply, does
**not** auto-remediate, and does **not** break state locks — those are
explicit follow-ups gated on apply-capable credentials.

### Consequences

* Good, because the watcher is a strict superset of what CI produces: every
  signal it emits is one CI structurally cannot produce between pushes.
* Good, because the readonly-credentials posture is auditable in one line of
  config (the cloud provider block bound to `*_READONLY_ROLE_ARN`-equivalents)
  and cannot drift to apply-capable without a deliberate config change.
* Good, because the example config is statically analysable today —
  `uncharles inspect` finds orphan actions, unreachable goal facts, and
  dead-end states without running anything.
* Good, because runtime gaps surfaced by the empirical failure modes (per
  state-key in-flight markers, three-valued plan exit codes as first-class
  facts, stale-lock recovery, partial-apply detection, evidence archiving on
  the failure path, lint preflight) become individually reviewable
  follow-ups against a working baseline rather than entangled with the
  "should we do this at all?" decision.
* Bad, because the three-valued plan exit code is encoded today via a
  wrapper script that writes `.uncharles/state/last_plan_exit` plus three
  sensors that decode it. That is shell-in-config, not first-class facts —
  acceptable as the bootstrap, ugly as the long-term shape. Tracked as a
  follow-up gap (overlaps with issue #18 on value-carrying facts).
* Bad, because drift surfaced by the watcher is human-triaged via
  `archive_drift` / `archive_error` rather than auto-remediated. That is
  deliberate (auto-remediation against possibly half-applied state is the
  rug-pull failure mode) but it means the watcher does not by itself close
  the loop on detected drift.
* Bad, because the runtime today treats action exit codes as binary and
  cannot natively model "plan succeeded but reports changes". Every config
  using this pattern duplicates the wrapper-script trick until the runtime
  gap lands.

### Confirmation

Compliance is verified by three lightweight gates:

1. **Static analysis** — `cargo run -p uncharles -- inspect --config
   uncharles/configs/tofu_drift_watch.yaml` exits `0` (no orphan actions,
   no unreachable goal facts, no dead-end states off the goal path). This
   runs as part of the workspace's standard `cargo test` because the config
   ships in the crate's `configs/` directory and the inspect path is
   exercised by the unit tests in `uncharles/src/inspect.rs`.
2. **Schema parse** — the config deserialises under
   `serde_yaml::from_str::<Config>` with `deny_unknown_fields`. The runtime's
   own parse tests in `uncharles/src/config.rs` enforce the schema; adding a
   typo'd field to the example would fail `cargo test -p uncharles`.
3. **Code review checklist** — when adding new watcher configs, reviewers
   confirm: (a) no action invokes `tofu apply`, (b) all `tofu` commands use
   `-lockfile=readonly` or `-refresh-only`, (c) `archive_*` actions are pure
   evidence-collection — they never mutate cloud or state.

## Pros and Cons of the Options

### Option A — replace the workflow

* Good, because it removes a duplicated abstraction (CI + watcher both knowing
  about plan/apply) at the cost of running `apply` from `uncharles`.
* Bad, because it duplicates working code (the workflow already orchestrates
  plan/apply across 12 concurrency groups) and replaces it with something
  less battle-tested.
* Bad, because the blast radius is enormous — a bug in `uncharles` could
  apply against production state. CI's blast radius is bounded by GitHub's
  job isolation; a long-lived watcher's is not.
* Bad, because it produces no new signal: replacing CI with `uncharles`
  doesn't tell us anything between pushes that CI didn't already tell us at
  push time.

### Option B — mirror the workflow as an `uncharles` config

* Good, because it's a useful exercise for shaking out runtime gaps and
  validates that `uncharles` can express the existing pipeline.
* Bad, because the value-over-CI is small until `uncharles` does something
  CI can't. We'd be writing a duplicate config that has to track every CI
  change and adds a second source of truth.
* Bad, because intra-cloud sequencing across 12 concurrency groups requires
  per-state-key in-flight markers (gap 1 in #25) before this is even
  expressible. We'd be paying that runtime cost up front for no new signal.

### Option C — post-apply convergence watcher (chosen)

* Good, because it produces a signal CI does not produce: cross-run drift,
  state-schema decay, artifact freshness.
* Good, because the readonly-creds posture caps blast radius at "wrong
  evidence file written" — no path leads to mutating cloud state.
* Good, because it's expressible *today* with the runtime as it stands:
  binary action exit codes plus a small wrapper-script trick for the
  three-valued plan result. Runtime improvements then have a working
  baseline to compare against.
* Neutral, because drift is human-triaged rather than auto-remediated. For
  IaC, this is the conservative choice — auto-remediation against possibly
  half-applied state is the documented "rug-pull" failure mode.
* Bad, because some real failure modes (stale lease, lock-file drift,
  partial-apply detection) are explicitly out-of-scope for the minimal
  config. They are tracked as follow-ups, not silently glossed over.

## More Information

* **Driving issue**: #25 (this ADR is the captured form of its `accepted`
  decision). The issue stays open for the six runtime-gap follow-ups it
  enumerates; this ADR closes only the "which option do we adopt?"
  sub-question.
* **Example config**: [`uncharles/configs/tofu_drift_watch.yaml`](../../uncharles/configs/tofu_drift_watch.yaml).
  Generic enough to drop into any OpenTofu repo; the project-specific bits
  (prefetch script, archive script, watch-workflow name) are surfaced as
  prereqs in the config's header comment.
* **Follow-up gaps tracked separately** (each becomes its own
  `type:design` or `type:feature` issue once scoped):
  1. Per-state-key in-flight markers (replace the single `in_flight_sha`
     idiom with `.uncharles/state/in_flight/<cloud>-<tier>-<env>`).
  2. Three-valued plan exit code as first-class facts in the runtime
     (overlaps with #18 on value-carrying facts).
  3. `refresh_lock_file` action gated by an approval marker — recovery for
     lock-file vs. version-constraint drift.
  4. `apply_partial_detected` escape hatch — flips after any apply that
     exited non-zero plus a follow-up `plan -refresh-only -detailed-exitcode
     == 2`, gates further apply behind manual approval.
  5. `archive_failure` action mirroring `archive_evidence` on the failure
     path — captures stderr, state snapshot, SHA.
  6. `lint_clean` precondition wrapping `tofu fmt -check && tofu validate` —
     cheapest preflight, mirrors the upstream `lint-infrastructure` job.
* **Trigger to revisit**: when any of the six follow-up gaps lands a runtime
  change (especially gap 2 — three-valued exit codes as first-class facts),
  this ADR's "Bad, because" point on shell-in-config can be retired and the
  example config simplified. If a future role for `uncharles` (e.g., a
  remediation agent that *does* hold apply-capable credentials) is
  proposed, this ADR is the reference for the role this watcher chose
  *not* to play, and the new ADR should be filed under a new number with
  a `supersedes` link if the watcher and remediator cannot coexist.
