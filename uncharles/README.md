<p align="center">
  <img src="assets/banner.svg" alt="uncharles — sense, plan, act" width="100%">
</p>

# uncharles

Sense → plan → act runtime that drives [`goap-planner`](../goap-planner/) from a YAML config. The first automaton built in the [Automata Atelier](../) workspace.

`uncharles` reads a config that declares **sensors** (shell probes that read the world) and **actions** (shell commands with preconditions, effects, and a cost). Without `--execute` it does a single sense → plan and prints the cheapest plan to the goal. With `--execute` it runs the **reactive runtime** ([ADR 0005](../docs/adrs/0005-actor-based-reactive-runtime.md)): sensors poll continuously and in parallel, a change to the world state triggers a replan, and an executor runs the freshest plan one action at a time — re-sensing and replanning so divergence reroutes execution rather than blindly continuing.

## Features

- **YAML-first** — describe the world in declarative `sensors` / `actions` / `goal` lists. No Rust required for new automatons.
- **Reactive actor runtime** — under `--execute`, sensors run continuously and in parallel ([`kameo`](https://docs.rs/kameo) actors on `tokio`); world-state changes edge-trigger replanning and the executor always runs the freshest plan. See [ADR 0005](../docs/adrs/0005-actor-based-reactive-runtime.md).
- **Plan-only or execute** — without `--execute`, prints the plan (`--pretty` for human-readable); `--execute` runs it. Same config, two modes.
- **Watcher mode** — `--execute --watch` stays up past goal-satisfied, acting again whenever the world next diverges.
- **Graceful shutdown** — `Ctrl+C` drains the runtime cleanly between actions, never mid-action.

## Installation

### From crates.io

```sh
cargo install uncharles
```

The workspace's three crates publish in dependency order: `grafo-dag`,
`goap-planner`, then `uncharles`. The first published release is `0.1.0`;
publishing is triggered via the `Release` workflow.

### Install via Nix

> **Status:** pending first nixpkgs PR.
>
> Per [ADR 0004](../docs/adrs/0004-publish-uncharles-to-nixpkgs.md),
> `uncharles` is being submitted to nixpkgs as
> `pkgs/by-name/un/uncharles/package.nix`. Until that PR merges and propagates
> to `nixpkgs-unstable`, this section is a placeholder — use [the
> from-source path](#from-source) below in the meantime.
>
> Once landed, the install command will be:
>
> ```sh
> # legacy CLI
> nix-env -iA nixpkgs.uncharles
>
> # flake-style
> nix profile install nixpkgs#uncharles
> ```
>
> Binaries are served by `cache.nixos.org`; no flake input or third-party
> cache is required. The in-repo mirror of the derivation lives at
> [`nix/package.nix`](../nix/package.nix) and is built on every PR by
> [`ci.yml`](../.github/workflows/ci.yml)'s `nix-build` job.

### From source

```sh
cargo build -p uncharles --release
```

## Usage

`uncharles` exposes two subcommands. The legacy flat form
(`uncharles --config X`) still works — it is parsed as `uncharles run`.

### `run` — sense, plan, optionally execute

Plan a sequence and print it:

```sh
cargo run -p uncharles -- run --config uncharles/configs/deploy.yaml --pretty
```

Execute via the reactive runtime, re-sensing and replanning on divergence:

```sh
cargo run -p uncharles -- run --config uncharles/configs/deploy.yaml --execute --pretty
```

Stay up as a watcher (act again whenever the world next diverges), pacing
sensor polls to once a second:

```sh
cargo run -p uncharles -- run --config uncharles/configs/podcast.yaml \
  --execute --watch --interval-ms 1000
```

Under `--execute`, `--interval-ms` is the per-sensor poll cadence and
`--max-iterations` caps the number of actions executed.

### `inspect` — visualise the state-action graph

Load a config and print the static structure plus the bounded reachable
state-action graph **without running any sensor or action commands**.
Useful for debugging configs that produce no plan, surfacing typo'd fact
names, orphan actions, unreachable goal facts, and dead-end states.

```sh
cargo run -p uncharles -- inspect --config uncharles/configs/deploy.yaml
```

The initial state is *simulated* — each sensor's `on_success` effects are
applied in YAML declaration order without running anything. Use `--have
<fact>` to layer additional facts on top, and `--max-states <N>` to override
the planner's default exploration cap (10 000). Exit code is `0` when the
config is clean, `1` when static analysis finds issues, `2` if the config
could not be loaded.

#### Output formats

`--format <text|dot|mermaid|json>` selects how the inspection report is
rendered. Default is `text` (the human-readable six-section report).

```sh
# Visual graph in your terminal via graph-easy (Perl):
brew install graph-easy
cargo run -q -p uncharles -- inspect --config X.yaml --format dot \
  | graph-easy --as=boxart

# Or pipe Graphviz to chafa for an image-based ASCII rendering:
cargo run -q -p uncharles -- inspect --config X.yaml --format dot \
  | dot -Tpng | chafa -

# In iTerm2 or Kitty you can render the real image inline:
cargo run -q -p uncharles -- inspect --config X.yaml --format dot \
  | dot -Tpng | imgcat                # iTerm2
cargo run -q -p uncharles -- inspect --config X.yaml --format dot \
  | dot -Tpng | kitty +kitten icat    # Kitty

# Mermaid for Markdown viewers / mermaid.live:
cargo run -q -p uncharles -- inspect --config X.yaml --format mermaid

# JSON for piping to jq or other tooling:
cargo run -q -p uncharles -- inspect --config X.yaml --format json \
  | jq '.static_analysis'
```

In all formats, initial state is filled blue, goal-satisfying states are
filled green, and any state that is *both* (rare) is filled orange. DOT
and Mermaid include the static-analysis findings as `//` and `%%`
comments respectively; JSON exposes them as a structured `static_analysis`
field. Exit codes are unchanged across formats.

Implements [issue #22](https://github.com/leopepe/automata-atelier/issues/22).

A minimal config:

```yaml
sensors:
  - name: code_committed
    cmd: ["git", "diff", "--quiet", "HEAD"]

actions:
  - name: run_tests
    cost: 2.0
    requires: [code_committed]
    adds: [tests_pass]
    cmd: ["cargo", "test"]

goal:
  requires: [tests_pass]
```

Actions also accept a `forbids` field — facts that must be **absent** for
the action to fire (mirrors `goal.forbids`). Lets you express "do this
only when X is not present" directly, instead of inventing a synthetic
sensor that observes the negative shape:

```yaml
actions:
  - name: install_tools
    cost: 5.0
    forbids: [tools_installed]   # only fire when tools are missing
    adds: [tools_installed]
    cmd: ["brew", "install", "..."]
```

A fact in both `requires` and `forbids` on the same action is rejected
at config-load time as a `ConfigError::ActionContradiction` (the action
would be structurally unsatisfiable). See `pendrive_audit.yaml` for a
real-world migration of two synthetic-sensor proxies onto `forbids`.

Sensors can also opt into **value capture** — `capture: stdout` stores the
sensor command's trimmed stdout under the sensor's name. Action commands
whose `requires` list mentions a fact with a captured value receive it as
`UNCHARLES_FACT_<UPPER_SNAKE_CASE_NAME>=<value>` in their environment.
Replaces sidecar-file workarounds for "carry this string to the next step"
patterns. See [ADR 0003](../docs/adrs/0003-value-carrying-facts-runtime-side.md):

```yaml
sensors:
  - name: target_sha
    cmd: ["git", "rev-parse", "origin/main"]
    capture: stdout

actions:
  - name: deploy
    cost: 5.0
    requires: [target_sha]
    adds: [deployed]
    # cmd reads $UNCHARLES_FACT_TARGET_SHA from its env
    cmd: ["sh", "-c", 'gh workflow run deploy.yml --ref "$UNCHARLES_FACT_TARGET_SHA"']
```

The planner stays Boolean over fact presence — values do not influence
planning. They flow runtime-side from sensors → action env vars; lifetime is
one invocation. Removing a fact (via `Action::removes` or sensor
`on_failure.remove`) drops its value atomically.

## Example configs

Real configs covering different domains — read them as a tour of what the runtime can express:

| Config | What it does |
|---|---|
| [`deploy.yaml`](configs/deploy.yaml) | Service deployment pipeline (test → build → push → deploy → smoke) |
| [`release_watch.yaml`](configs/release_watch.yaml) | Long-lived release-tagging watcher |
| [`tofu_drift_watch.yaml`](configs/tofu_drift_watch.yaml) | Post-apply OpenTofu/IaC convergence watcher (drift detection, readonly creds — see [ADR 0002](../docs/adrs/0002-uncharles-as-post-apply-iac-convergence-watcher.md)) |
| [`merge_gate.yaml`](configs/merge_gate.yaml) | PR-merge gate with status-check sensors (single PR, driven by `$PR_NUMBER`) |
| [`dep_pr_automerge.yaml`](configs/dep_pr_automerge.yaml) | Dependabot/Renovate auto-merge watcher: merges CLEAN bot PRs one at a time, comments `@dependabot rebase` on BEHIND ones, halts on post-merge regression of main CI |
| [`podcast.yaml`](configs/podcast.yaml) | Podcast-episode download pipeline (RSS → fetch → archive), hermetic skeleton |
| [`podcast_spotdl.yaml`](configs/podcast_spotdl.yaml) | Spotify / YouTube playlist download pipeline driven by `uvx spotdl` (per-track, ffprobe-verified) |
| [`repo_validate.yaml`](configs/repo_validate.yaml) | Repo-validation pipeline (fmt + clippy + test) |
| [`smoke_loop.yaml`](configs/smoke_loop.yaml) / [`smoke_recover.yaml`](configs/smoke_recover.yaml) / [`smoke_failure.yaml`](configs/smoke_failure.yaml) / [`smoke_capture.yaml`](configs/smoke_capture.yaml) | Side-effect-free smoke configs used by the integration tests |

## Architecture

```
┌─────────────────────────────────────────────┐
│  uncharles  (this crate)                    │
│   YAML · sensors · actions · actors · I/O   │
└────────────────────┬────────────────────────┘
                     │  uses
┌────────────────────▼────────────────────────┐
│  goap-planner                               │
│   state · action · goal · plan              │
└────────────────────┬────────────────────────┘
                     │  uses
┌────────────────────▼────────────────────────┐
│  grafo                                      │
│   DAG · CSR storage · Dijkstra              │
└─────────────────────────────────────────────┘
```

Side effects (shell exec, YAML parsing, signal handling) live here, not in the layers below. Cloud-specific adapters and any future I/O glue belong in `uncharles` (or a sibling runtime crate), never in `goap-planner`.

Under `--execute`, the runtime is a small tree of [`kameo`](https://docs.rs/kameo) actors on `tokio` ([ADR 0005](../docs/adrs/0005-actor-based-reactive-runtime.md)): a sensor actor per `SensorSpec` (continuous, parallel), a world-state actor that owns the `State`/values and edge-triggers replans, a planner actor wrapping `goap-planner`, an executor actor, and a goal supervisor that arbitrates plans. `goap-planner`'s types are the messages — the actor layer wraps the planner, it never replaces it.

## Contributing

1. Read [`CLAUDE.md`](CLAUDE.md) (when present) and the workspace [`CLAUDE.md`](../CLAUDE.md) before opening a PR.
2. Run the test suite: `cargo test -p uncharles`.
3. Side-effect-free integration tests live in `tests/` against the smoke configs — extend those when changing the loop.

## License

MIT
