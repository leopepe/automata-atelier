<p align="center">
  <img src="assets/banner.svg" alt="uncharles — sense, plan, act" width="100%">
</p>

# uncharles

Sense → plan → act runtime that drives [`goap-planner`](../goap-planner/) from a YAML config. The first automaton built in the [Automata Atelier](../) workspace.

`uncharles` reads a config that declares **sensors** (shell probes that read the world) and **actions** (shell commands with preconditions, effects, and a cost). On each cycle it runs the sensors to derive the current state, asks `goap-planner` for the cheapest plan to the goal, and either prints the plan or executes it step by step — re-sensing between steps so divergence triggers a replan rather than blind execution.

## Features

- **YAML-first** — describe the world in declarative `sensors` / `actions` / `goal` lists. No Rust required for new automatons.
- **Sense → plan → act loop** — each cycle is independent; replanning on divergence is the default failure mode.
- **Plan-only or execute** — `--pretty` prints the plan; `--execute` runs it. Same config, two modes.
- **Graceful shutdown** — `Ctrl+C` interrupts between steps, never mid-action.

## Installation

This crate is a workspace member, not yet published to crates.io.

```sh
cargo build -p uncharles --release
```

## Usage

Plan a sequence and print it:

```sh
cargo run -p uncharles -- --config uncharles/configs/deploy.yaml --pretty
```

Execute the plan, re-sensing and replanning on divergence:

```sh
cargo run -p uncharles -- --config uncharles/configs/deploy.yaml --execute --pretty
```

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

## Example configs

Real configs covering different domains — read them as a tour of what the runtime can express:

| Config | What it does |
|---|---|
| [`deploy.yaml`](configs/deploy.yaml) | Service deployment pipeline (test → build → push → deploy → smoke) |
| [`release_watch.yaml`](configs/release_watch.yaml) | Long-lived release-tagging watcher |
| [`merge_gate.yaml`](configs/merge_gate.yaml) | PR-merge gate with status-check sensors |
| [`podcast.yaml`](configs/podcast.yaml) | Podcast-episode download pipeline (RSS → fetch → archive) |
| [`repo_validate.yaml`](configs/repo_validate.yaml) | Repo-validation pipeline (fmt + clippy + test) |
| [`smoke_loop.yaml`](configs/smoke_loop.yaml) / [`smoke_recover.yaml`](configs/smoke_recover.yaml) / [`smoke_failure.yaml`](configs/smoke_failure.yaml) | Side-effect-free smoke configs used by the integration tests |

## Architecture

```
┌─────────────────────────────────────────────┐
│  uncharles  (this crate)                    │
│   YAML · sensors · actions · loop · I/O     │
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

## Contributing

1. Read [`CLAUDE.md`](CLAUDE.md) (when present) and the workspace [`CLAUDE.md`](../CLAUDE.md) before opening a PR.
2. Run the test suite: `cargo test -p uncharles`.
3. Side-effect-free integration tests live in `tests/` against the smoke configs — extend those when changing the loop.

## License

MIT
